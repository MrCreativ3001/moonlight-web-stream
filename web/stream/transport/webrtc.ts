import { Api, fetchApi, WebRTCAnswer } from "../../api"
import { StreamKeys } from "../../api_bindings"
import { ActiveGamepads, ClientInputEvent, ClientInputEvent_Tags, ControlPacket, ControlPacketConfig, controlPacketDeserialize, controlPacketSerialize, KeyAction, KeyModifiers, keyStatesCanStore, keyStatesEmpty, keyStatesSetPressed, MouseButton, MouseButtonAction, PacketDirection, VideoFormats, WebRtcSessionAnswer, webrtcSessionAnswerParse, WebRtcSessionOffer, webrtcSessionOfferApply } from "../../uniffi/moonlight_common_bindings"
import { globalObject, wait } from "../../util"
import { AudioPlayer, TrackAudioPlayer } from "../audio/index"
import { I16_MAX, U16_MAX, U8_MAX } from "../buffer"
import { createControllerPacketBitflags } from "../gamepad"
import { Logger } from "../log"
import { DataPipe } from "../pipeline/pipes"
import { StatValue } from "../stats"
import { TrackVideoRenderer, VideoRenderer } from "../video/index"
import { generateControlPacketConfig, IControlStream, Transport, TransportAudioType, TransportConnectData, TransportOptions, TransportShutdown, TransportVideoType } from "./index"

export class WebRTCTransport implements Transport {

    readonly implementationName: string = "webrtc"

    readonly controlStream
    onconnect: ((connectData: TransportConnectData) => void) | null = null
    onclose: ((shutdown: TransportShutdown) => void) | null = null

    private logger?: Logger

    private api: Api

    private peer: RTCPeerConnection
    private location: string | null = null

    constructor(api: Api, configuration: RTCConfiguration, logger?: Logger) {
        this.logger = logger

        this.api = api

        // Create peer
        this.peer = new RTCPeerConnection(configuration)
        this.controlStream = new WebRtcControlStream(this.peer)

        this.logger?.debug(`Using ice servers ${JSON.stringify(configuration.iceServers?.flatMap(server => server.urls))}`)

        // Set Event Listeners
        this.peer.addEventListener("connectionstatechange", this.onStateChange.bind(this))
        this.peer.addEventListener("datachannel", this.onDataChannel.bind(this))
        this.peer.addEventListener("track", this.onTrack.bind(this))

        // Ice Gathering
        this.peer.addEventListener("icecandidate", this.onIceCandidate.bind(this))

        // Add Media
        this.peer.addTransceiver("video", { direction: "recvonly" })
        this.peer.addTransceiver("audio", { direction: "recvonly" })

        // Dummy data channel required so that the answerer knows we accept data channels
        this.peer.createDataChannel("dummy")
    }

    private sdpOfferOptions: WebRtcSessionOffer | null = null
    private sdpAnswer: WebRtcSessionAnswer | null = null

    async createOffer(options: TransportOptions): Promise<string> {
        this.logger?.debug("Creating webrtc offer")

        let offer = await this.peer.createOffer()
        if (offer.type != "offer") {
            throw `WHEP offer is of type ${offer.type}`
        }

        this.logger?.debug("Setting webrtc local description")
        await this.peer.setLocalDescription(offer)

        // Insert custom options
        this.sdpOfferOptions = {
            ...options
        }
        const sdp = webrtcSessionOfferApply(offer.sdp ?? "", this.sdpOfferOptions)

        this.logger?.debug(`successfully generated webrtc sdp with options ${JSON.stringify(this.sdpOfferOptions)}`)
        console.debug("Client Sdp", sdp)

        this.logger?.debug(`starting ice candidate sender`)
        this.sendIceCandidates()

        return sdp
    }
    async setAnswer(response: WebRTCAnswer): Promise<void> {
        console.debug("server sdp", JSON.stringify(response))

        this.logger?.debug(`received whep response with location "${response.location}"`)
        // Print ice candidates
        for (const line of response.answerSdp.split("\r\n")) {
            if (line.startsWith("a=candidate")) {
                this.logger?.debug(`received remote ice candidate ${line.substring(2)}`)
            }
        }

        this.location = response.location

        this.sdpAnswer = webrtcSessionAnswerParse(response.answerSdp)
        this.logger?.debug(`Server responded with extensions ${JSON.stringify(this.sdpAnswer)}`)

        await this.peer.setRemoteDescription({
            type: "answer",
            sdp: response.answerSdp,
        })
    }

    private connectData: TransportConnectData | null = null
    private async generateConnectData(): Promise<TransportConnectData> {
        if (this.connectData) {
            return this.connectData
        }

        if (!this.videoStream || !this.audioStream) {
            throw `WebRTC WHEP response didn't contain a video and audio stream! Video: ${this.videoStream != null}, Audio: ${this.audioStream != null}`
        }
        const codec = await this.findOutCodec()

        const audioSettings = this.audioStream.getSettings()

        this.connectData = {
            capabilities: {
                touch: false
            },
            videoType: "videotrack",
            videoSetup: {
                // Assume the requested parameters are correct
                width: this.sdpOfferOptions?.width ?? -1,
                height: this.sdpOfferOptions?.height ?? -1,
                fps: this.sdpOfferOptions?.fps ?? -1,
                codec,
            },
            audioType: "audiotrack",
            audioSetup: {
                channels: audioSettings.channelCount ?? 2,
                sampleRate: audioSettings.sampleRate ?? 48000,
                // TODO
                streams: 0,
                coupledStreams: 0,
                samplesPerFrame: 0,
                mapping: []
            },
            appName: this.sdpAnswer?.appName ?? "Unknown"
        }
        return this.connectData
    }

    private wasConnected = false
    private onStateChange() {
        if (this.peer.connectionState == "connected") {
            this.wasConnected = true

            this.generateConnectData().then(connectData => {
                if (this.onconnect) {
                    this.onconnect(connectData)
                }
            })
        } else if (this.peer.connectionState == "failed" || this.peer.connectionState == "closed") {
            const shutdown = this.wasConnected ? "failed" : "failednoconnect"

            if (this.onclose) {
                this.onclose(shutdown)
            }
        }
    }

    // -- Trickle Ice
    private iceCandidateSendTimer: number | null = null
    private pendingIceCandidates: Array<string> = []
    private onIceCandidate(event: RTCPeerConnectionIceEvent) {
        if (!event.candidate) {
            // Ice Gathering finished
            this.logger?.debug("ice gathering finished")
            return
        }

        const candidate = event.candidate.toJSON().candidate
        if (candidate) {
            this.pendingIceCandidates.push(candidate)
        }
    }

    private boundSendIceCandidates = this.sendIceCandidates.bind(this)
    private async sendIceCandidates() {
        this.iceCandidateSendTimer = null
        if (this.iceCandidateSendTimer != null) {
            globalObject().clearTimeout(this.iceCandidateSendTimer)
        }

        for (const candidate of this.pendingIceCandidates) {
            this.logger?.debug(`sending ice candidate: ${candidate}`)
        }

        if (this.location && this.pendingIceCandidates.length > 0) {
            const trickleIceSdpFrag = this.pendingIceCandidates.map(x => `a=${x}`).join("\r\n")

            await fetchApi(this.api, this.location, "PATCH", {
                noUrlModify: true,
                trickleIceSdpFrag,
                response: "ignore",
            })

            this.pendingIceCandidates = []
        }

        if (this.peer.iceGatheringState != "complete") {
            this.iceCandidateSendTimer = globalObject().setTimeout(this.boundSendIceCandidates, 2000)
        }
    }

    // -- Control Stream / Media
    private onDataChannel(event: RTCDataChannelEvent) {
        const channel = event.channel

        this.logger?.debug(`received data channel with label: ${channel.label}`)

        if (channel.label == "moonlight.control") {
            const config = generateControlPacketConfig()

            this.controlStream.setChannel(channel, config)
        }
    }

    private onTrack(event: RTCTrackEvent) {
        event.receiver.jitterBufferTarget = 0
        if ("playoutDelayHint" in event.receiver) {
            event.receiver.playoutDelayHint = 0
        }
        const track = event.track

        this.logger?.debug(`received track with label: ${track.label}, kind: ${track.kind}`)

        if (track.kind == "video") {
            track.contentHint = "motion"

            this.videoStream = track
        } else if (track.kind == "audio") {
            this.audioStream = track
        }
    }

    // Video
    private videoStream: MediaStreamTrack | null = null

    setVideoPipeline(type: "videotrack", pipeline: (TrackVideoRenderer & VideoRenderer)): Promise<void>;
    setVideoPipeline(type: "data", pipeline: (DataPipe & VideoRenderer)): Promise<void>;
    async setVideoPipeline(type: TransportVideoType, pipeline: unknown): Promise<void> {
        if (!this.videoStream || !this.connectData) {
            throw "the stream must be connected!"
        }

        if (type == "videotrack") {
            const trackPipeline = pipeline as (TrackVideoRenderer & VideoRenderer)

            trackPipeline.setTrack(this.videoStream)
        } else if (type == "data") {
            throw "unimplemented"
        }
    }

    // Audio
    private audioStream: MediaStreamTrack | null = null

    setAudioPipeline(type: "audiotrack", pipeline: (TrackAudioPlayer & AudioPlayer)): Promise<void>
    setAudioPipeline(type: "data", pipeline: (DataPipe & AudioPlayer)): Promise<void>
    async setAudioPipeline(type: TransportAudioType, pipeline: AudioPlayer): Promise<void> {
        if (!this.audioStream || !this.connectData) {
            throw "the stream must be connected!"
        }

        if (type == "audiotrack") {
            const trackPipeline = pipeline as (TrackAudioPlayer & AudioPlayer)

            trackPipeline.setTrack(this.audioStream)
        } else if (type == "data") {
            throw "unimplemented"
        }
    }

    async close(): Promise<void> {
        // Close the peer
        this.peer.close()

        // Delete the ice candidate send loop
        globalObject().clearTimeout(this.iceCandidateSendTimer)
        this.iceCandidateSendTimer = null

        // Delete our current session on the server
        if (this.location) {
            try {
                await fetchApi(this.api, this.location, "DELETE", {
                    keepalive: true,
                    noUrlModify: true,
                    response: "ignore",
                })
            } catch (e) {
                console.debug("failed to DELETE webrtc session", e)
            }
        }
    }

    private async findOutCodec(): Promise<keyof VideoFormats> {
        let tries = 0

        while (true) {
            const stats = await this.peer.getStats()
            for (const [_key, value] of stats) {
                // Video Stream
                if ("type" in value && "kind" in value
                    && value.type == "inbound-rtp" && value.kind == "video"
                ) {

                }
            }
            tries += 1
            if (tries > 10) {
                this.logger?.debug(`failed to determine codec using stats after ${tries} tries, assuming h264`)
                return "h264"
            }

            await wait(100)
        }
    }

    private lastTotalDecodeTime = 0
    private lastFramesDecoded = 0
    async getStats(): Promise<Record<string, StatValue>> {
        const out: Record<string, StatValue> = {}

        // Control Stream
        // TODO

        const stats = await this.peer.getStats()

        for (const [_key, value] of stats) {
            console.debug(value)

            // Video Stream
            if ("type" in value && "kind" in value
                && value.type == "inbound-rtp" && value.kind == "video"
            ) {
                out.resolution = `Width: ${value?.frameWidth}, Height: ${value?.frameHeight}`

                out.framesDecoded = value?.framesDecoded
                out.framesDropped = value?.framesDropped
                out.keyFramesDecoded = value?.keyFramesDecoded

                out.packetsLost = value?.packetsLost
                out.packetsReceived = value?.packetsReceived

                out.nackCount = value?.nackCount
                out.pliCount = value?.pliCount
                out.firCount = value?.firCount

                if ("totalDecodeTime" in value && "framesDecoded" in value) {
                    out.decodeTimePerFrameMs = (value.totalDecodeTime - this.lastTotalDecodeTime) / (value.framesDecoded - this.lastFramesDecoded) * 1000.0

                    this.lastFramesDecoded = value.framesDecoded
                    this.lastTotalDecodeTime = value.totalDecodeTime
                }

                out.currentFps = value?.framesPerSecond
            }
            if ("type" in value && "mimeType" in value && typeof value.mimeType == "string"
                && value.type == "codec" && value.mimeType.startsWith("video/")
            ) {
                out.codec = value.mimeType.substring(6)
                out.codecSdpFmtpLine = value?.sdpFmtpLine
            }

            // Audio Stream
        }

        return out
    }
}

class WebRtcControlStream implements IControlStream {

    private logger?: Logger

    private config: ControlPacketConfig | null = null

    private channel: RTCDataChannel | null = null
    private mouseAbsolute: RTCDataChannel
    private mouse: RTCDataChannel
    private keysCompact: RTCDataChannel
    private keys: RTCDataChannel
    private touch: RTCDataChannel
    private controller: RTCDataChannel

    // Input Batching
    private mouseState:
        { x: number, y: number, referenceWidth: number, referenceHeight: number } |
        { moveX: number, moveY: number }
        = { moveX: 0, moveY: 0 }
    private mouseScrollX = 0
    private mouseScrollY = 0

    private remoteKeyStates: Set<number> = new Set()
    private currentPressedKeys: Set<number> = new Set()
    private keyStatesSequenceNumber = 0

    private controllerStates: Array<boolean> = new Array(16)

    // Buffering
    private packetBuffer: Array<ControlPacket> = []

    constructor(peer: RTCPeerConnection, logger?: Logger) {
        this.logger = logger

        this.controllerStates.fill(false)

        this.mouseAbsolute = peer.createDataChannel("moonlight.control.mouseAbsolute", {
            ordered: false,
            maxRetransmits: 0,
        })
        this.mouseAbsolute.bufferedAmountLowThreshold = this.maxBufferedAmount(this.mouseAbsolute)

        this.mouse = peer.createDataChannel("moonlight.control.mouse", {
            ordered: false,
            maxPacketLifeTime: 30,
        })
        this.mouse.bufferedAmountLowThreshold = this.maxBufferedAmount(this.mouse)

        this.keysCompact = peer.createDataChannel("moonlight.control.keysCompact", {
            ordered: false,
            maxRetransmits: 0,
        })
        this.keysCompact.bufferedAmountLowThreshold = this.maxBufferedAmount(this.keysCompact)

        this.keys = peer.createDataChannel("moonlight.control.keys")
        this.keys.bufferedAmountLowThreshold = this.maxBufferedAmount(this.keys)

        this.touch = peer.createDataChannel("moonlight.control.touch")
        this.touch.bufferedAmountLowThreshold = this.maxBufferedAmount(this.keys)

        this.controller = peer.createDataChannel("moonlight.control.controller", {
            ordered: false,
            maxRetransmits: 0,
        })
        this.controller.bufferedAmountLowThreshold = this.maxBufferedAmount(this.controller)

        for (const channel of [this.mouseAbsolute, this.mouse, this.keysCompact, this.keys, this.controller]) {
            channel.onbufferedamountlow = this.boundTrySendBufferedPackets
        }

        // Hook into frame loop for sending packets
        globalObject().requestAnimationFrame(this.boundSendBatchedInputs)
    }

    private maxBufferedAmount(channel: RTCDataChannel): number {
        switch (channel) {
            case this.mouseAbsolute:
            case this.mouse:
            case this.keys:
                return 512
            case this.controller:
                return 4 * 512
            case this.touch:
                return 16 * 1024
            case this.channel:
                return 16 * 1024
            default:
                return 1024
        }
    }

    setChannel(channel: RTCDataChannel | null, config?: ControlPacketConfig): void {
        if (channel && config) {
            this.channel = channel

            this.config = config

            this.channel.binaryType = "arraybuffer"

            this.channel.addEventListener("open", this.boundTrySendBufferedPackets)
            this.channel.addEventListener("bufferedamountlow", this.boundTrySendBufferedPackets)
            this.channel.addEventListener("message", this.boundMessage)

            this.channel.bufferedAmountLowThreshold = this.maxBufferedAmount(this.channel)

            this.trySendBufferedPackets()
        } else {
            this.channel?.removeEventListener("open", this.boundTrySendBufferedPackets)
            this.channel?.removeEventListener("bufferedamountlow", this.boundTrySendBufferedPackets)
            this.channel?.removeEventListener("message", this.boundMessage)

            this.channel = null
        }
    }

    onreceive: ((packet: ControlPacket) => void) | null = null

    private boundMessage = this.onMessage.bind(this)
    private onMessage(event: MessageEvent) {
        if (!this.config) {
            throw "packet config not configured, but a packet was received"
        }

        const packet = controlPacketDeserialize(this.config, PacketDirection.ClientBound, event.data)

        if (packet && this.onreceive) {
            this.onreceive(packet)
        }
    }

    send(input: ClientInputEvent): void {
        const LI_ROT_UNKNOWN = 65535
        const LI_TILT_UNKNOWN = 255
        const MC_HEADER_B = 0x001A
        const MC_MID_B = 0x0014
        const MC_TAIL_A = 0x009C
        const MC_TAIL_B = 0x0055

        switch (input.tag) {
            case ClientInputEvent_Tags.MouseMoveAbsolute:
                this.mouseState = {
                    x: input.inner.x,
                    y: input.inner.y,
                    referenceWidth: input.inner.referenceWidth,
                    referenceHeight: input.inner.referenceHeight,
                }
                break
            case ClientInputEvent_Tags.MouseMoveRelative:
                if ("moveX" in this.mouseState) {
                    this.mouseState.moveX += input.inner.deltaX
                    this.mouseState.moveY += input.inner.deltaY
                } else {
                    this.mouseState = {
                        moveX: input.inner.deltaX,
                        moveY: input.inner.deltaY
                    }
                }
                break
            case ClientInputEvent_Tags.MouseScrollVertical:
                this.mouseScrollY += input.inner.scrollY
                break
            case ClientInputEvent_Tags.MouseScrollHorizontal:
                this.mouseScrollX += input.inner.scrollX
                break
            case ClientInputEvent_Tags.MouseButton:
                let keyCode = null
                switch (input.inner.button) {
                    case MouseButton.Left:
                        keyCode = StreamKeys.VK_LBUTTON
                        break
                    case MouseButton.Middle:
                        keyCode = StreamKeys.VK_MBUTTON
                        break
                    case MouseButton.Right:
                        keyCode = StreamKeys.VK_RBUTTON
                        break
                    case MouseButton.X1:
                        keyCode = StreamKeys.VK_XBUTTON1
                        break
                    case MouseButton.X2:
                        keyCode = StreamKeys.VK_XBUTTON2
                        break
                }

                if (keyCode) {
                    if (input.inner.action == MouseButtonAction.Press) {
                        this.currentPressedKeys.add(keyCode)
                    } else {
                        this.currentPressedKeys.delete(keyCode)
                    }
                }

                this.sendKeysCompact()
                break
            case ClientInputEvent_Tags.Keyboard:
                if (input.inner.action == KeyAction.Down) {
                    this.currentPressedKeys.add(input.inner.keyCode)
                } else {
                    this.currentPressedKeys.delete(input.inner.keyCode)
                }

                this.sendKeysCompact()
                break
            case ClientInputEvent_Tags.ControllerConnect:
                let controllerNumber = input.inner.controllerNumber % 16

                this.controllerStates[controllerNumber] = true

                this.sendRaw(new ControlPacket.ControllerArrival({
                    controllerNumber,
                    ty: input.inner.ty,
                    supportedButtons: input.inner.supportedButtons,
                    capabilities: input.inner.capabilities,
                }))
                break
            case ClientInputEvent_Tags.ControllerState:
                controllerNumber = input.inner.controllerNumber % 16

                const controllerBitflags = createControllerPacketBitflags(input.inner.pressedButtons)

                if (this.controllerStates[controllerNumber]) {
                    this.trySendOn(this.controller, new ControlPacket.ControllerState({
                        headerB: MC_HEADER_B,
                        controllerNumber,
                        activeGamepadMask: this.getControllerMask(),
                        midB: MC_MID_B,
                        buttonFlags: controllerBitflags & 0xFFFF,
                        leftTrigger: Math.min(Math.max(input.inner.leftTrigger, 0), 1) * U8_MAX,
                        rightTrigger: Math.min(Math.max(input.inner.rightTrigger, 0), 1) * U8_MAX,
                        leftStickX: Math.min(Math.max(input.inner.leftStickX, -1), 1) * I16_MAX,
                        leftStickY: Math.min(Math.max(input.inner.leftStickY, -1), 1) * I16_MAX,
                        rightStickX: Math.min(Math.max(input.inner.rightStickX, -1), 1) * I16_MAX,
                        rightStickY: Math.min(Math.max(input.inner.rightStickY, -1), 1) * I16_MAX,
                        tailA: MC_TAIL_A,
                        buttonFlags2: (controllerBitflags >> 16) & 0xFFFF,
                        tailB: MC_TAIL_B,
                    }))
                } else {
                    console.debug("cannot send state for controller that wasn't added")
                }
                break
            case ClientInputEvent_Tags.ControllerDisconnect:
                controllerNumber = input.inner.controllerNumber % 16

                this.controllerStates[controllerNumber] = false

                this.sendRaw(new ControlPacket.ControllerState({
                    controllerNumber,
                    activeGamepadMask: this.getControllerMask(),
                    buttonFlags: 0,
                    buttonFlags2: 0,
                    headerB: 0,
                    leftStickX: 0,
                    leftStickY: 0,
                    leftTrigger: 0,
                    midB: 0,
                    rightStickX: 0,
                    rightStickY: 0,
                    rightTrigger: 0,
                    tailA: 0,
                    tailB: 0,
                }))
                break
            case ClientInputEvent_Tags.Touch:
                this.trySendOn(this.touch, new ControlPacket.Touch({
                    eventType: input.inner.eventType,
                    reserved: 0,
                    pointerId: input.inner.pointerId,
                    x: input.inner.x,
                    y: input.inner.y,
                    rotation: input.inner.rotation ?? LI_ROT_UNKNOWN,
                    contactAreaMajor: input.inner.contactAreaMajor,
                    contactAreaMinor: input.inner.contactAreaMinor,
                    pressureOrDistance: input.inner.pressureOrDistance,
                }))
                break
            case ClientInputEvent_Tags.Pen:
                this.sendRaw(new ControlPacket.Pen({
                    eventType: input.inner.eventType,
                    toolType: input.inner.toolType,
                    buttons: input.inner.buttons,
                    zero: 0,
                    x: input.inner.x,
                    y: input.inner.y,
                    pressureOrDistance: input.inner.pressureOrDistance,
                    rotation: input.inner.rotation ?? LI_ROT_UNKNOWN,
                    tilt: input.inner.tilt ?? LI_TILT_UNKNOWN,
                    zero2: 0,
                    contactAreaMajor: input.inner.contactAreaMajor,
                    contactAreaMinor: input.inner.contactAreaMinor,
                }))
                break
        }
    }

    sendRaw(packet: ControlPacket): void {
        this.packetBuffer.push(packet)

        this.trySendBufferedPackets()
    }

    private boundTrySendBufferedPackets = this.trySendBufferedPackets.bind(this)
    private trySendBufferedPackets() {
        if (!this.channel) {
            return
        }

        if (this.channel.readyState != "open") {
            return
        }

        // Try to send packets
        for (const packet of this.packetBuffer.splice(0)) {
            this.trySendOn(this.channel, packet)
        }
    }

    private boundSendBatchedInputs = this.sendBatchedInputs.bind(this)
    private sendBatchedInputs() {
        if (this.channel?.readyState == "closed") {
            return
        }
        globalObject().requestAnimationFrame(this.boundSendBatchedInputs)

        // -- Send mouse
        if ("x" in this.mouseState) {
            this.trySendOn(this.mouseAbsolute, new ControlPacket.MouseMoveAbsolute({
                x: this.mouseState.x,
                y: this.mouseState.y,
                referenceWidth: this.mouseState.referenceWidth,
                referenceHeight: this.mouseState.referenceHeight,
                unused: 0,
            }))
        } else {
            const notChanged = this.mouseState.moveX == 0 && this.mouseState.moveY == 0
            const changed = !notChanged

            if (changed) {
                this.trySendOn(this.mouse, new ControlPacket.MouseMoveRelative({
                    deltaX: this.mouseState.moveX,
                    deltaY: this.mouseState.moveY
                }))
            }

            this.mouseState = {
                moveX: 0,
                moveY: 0,
            }
        }

        // -- Send Mouse Scroll
        if (this.mouseScrollX != 0) {
            this.trySendOn(this.mouseAbsolute, new ControlPacket.MouseHorizontalScroll({
                scrollAmount: this.mouseScrollX
            }))
            this.mouseScrollX = 0
        }
        if (this.mouseScrollY != 0) {
            this.trySendOn(this.mouseAbsolute, new ControlPacket.MouseScroll({
                scrollAmount1: this.mouseScrollY,
                scrollAmount2: this.mouseScrollY,
                zero: 0,
            }))
            this.mouseScrollY = 0
        }

        this.sendKeysCompact()
    }

    private sendKeysCompact() {
        // Get key modifiers for sending reliable keys as fallback
        let modifiers = { alt: false, ctrl: false, meta: false, shift: false }
        if (this.currentPressedKeys.has(StreamKeys.VK_SHIFT) || this.currentPressedKeys.has(StreamKeys.VK_LSHIFT) || this.currentPressedKeys.has(StreamKeys.VK_RSHIFT)) {
            modifiers.shift = true
        }
        if (this.currentPressedKeys.has(StreamKeys.VK_LWIN) || this.currentPressedKeys.has(StreamKeys.VK_RWIN)) {
            modifiers.meta = true
        }
        if (this.currentPressedKeys.has(StreamKeys.VK_CONTROL) || this.currentPressedKeys.has(StreamKeys.VK_LCONTROL) || this.currentPressedKeys.has(StreamKeys.VK_RCONTROL)) {
            modifiers.ctrl = true
        }
        if (this.currentPressedKeys.has(StreamKeys.VK_MENU) || this.currentPressedKeys.has(StreamKeys.VK_LMENU) || this.currentPressedKeys.has(StreamKeys.VK_RMENU)) {
            modifiers.alt = true
        }

        let keyStates = keyStatesEmpty()

        // Go through pressed keys
        for (const key of this.currentPressedKeys) {
            if (keyStatesCanStore(keyStates, key)) {
                keyStates = keyStatesSetPressed(keyStates, key, KeyAction.Down)
            } else {
                // only send reliable key press if the host doesn't know about it
                if (this.remoteKeyStates.has(key)) {
                    continue
                }

                this.trySendOn(this.keys, new ControlPacket.Keyboard({
                    action: KeyAction.Down,
                    flags: { sunshineNonNormalized: false },
                    keyCode: key,
                    modifiers,
                    zero: 0,
                }))

                this.remoteKeyStates.add(key)
            }
        }

        // Make a copy to not delete while iterating
        const remoteKeyStates = [...this.remoteKeyStates]

        for (const key of remoteKeyStates) {
            if (!this.currentPressedKeys.has(key) && !keyStatesCanStore(keyStates, key)) {
                this.trySendOn(this.keys, new ControlPacket.Keyboard({
                    action: KeyAction.Up,
                    flags: { sunshineNonNormalized: false },
                    keyCode: key,
                    modifiers,
                    zero: 0,
                }))

                this.remoteKeyStates.delete(key)
            }
        }

        // Send key states
        this.trySendOn(this.keysCompact, new ControlPacket.WebState({
            sequenceNumber: this.keyStatesSequenceNumber,
            keys: keyStates
        }))

        if (this.keyStatesSequenceNumber >= U16_MAX - 1) {
            this.keyStatesSequenceNumber = 0
        }
        this.keyStatesSequenceNumber += 1
    }

    private getControllerMask(): ActiveGamepads {
        const gamepads: Record<string, boolean> = {}

        for (let i = 0; i < 16; i++) {
            gamepads[`gamepad${i + 1}`] = this.controllerStates[i]
        }

        return gamepads as ActiveGamepads
    }

    private trySendOn(channel: RTCDataChannel, packet: ControlPacket) {
        if (!this.config) {
            return
        }

        if (channel.bufferedAmount > this.maxBufferedAmount(channel)) {
            // Cannot send more packets because of buffered amount
            // -> Drop the packet
            return
        }

        const buffer = controlPacketSerialize(this.config, packet)
        if (buffer) {
            channel.send(buffer)
        }
    }
}
