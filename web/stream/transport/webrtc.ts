import { Api, fetchApi, WebRTCAnswer } from "../../api.js";
import { ClientInputEvent, ControlPacket, ControlPacketConfig, controlPacketDeserialize, controlPacketSerialize, ControlStream, ControlStreamEvent, ControlStreamEvent_Tags, EstimatedRttInfo, InputBatcher, PacketDirection, UdpTransmit, webrtcSessionAnswerParse, WebRtcSessionOffer, webrtcSessionOfferApply } from "../../uniffi/moonlight_common_bindings.js";
import { globalObject, uniffiMillisUntil, uniffiNow } from "../../util.js";
import { AudioPlayer, TrackAudioPlayer } from "../audio/index.js";
import { Logger } from "../log.js";
import { DataPipe } from "../pipeline/pipes.js";
import { StatValue } from "../stats.js";
import { TrackVideoRenderer, VideoRenderer } from "../video/index.js";
import { generateControlPacketConfig, IControlStream, Transport, TransportAudioType, TransportConnectData, TransportOptions, TransportShutdown, TransportVideoType } from "./index.js";

export class WebRTCTransport implements Transport {

    readonly implementationName: string = "webrtc"

    readonly controlStream = new WebRtcControlStream()
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
            controlEnet: true,
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
                this.logger?.debug(`received remove ice candidate ${line.substring(2)}`)
            }
        }

        this.location = response.location

        const answer = webrtcSessionAnswerParse(response.answerSdp)
        this.logger?.debug(`Server responded with extensions ${JSON.stringify(answer)}`)

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
                // TODO: gather codec using stats
                codec: "h264",
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
            }
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

        if (this.location) {
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

        this.logger?.debug(`received data channel with label: ${channel.label}, protocol: ${channel.protocol}`)

        if (channel.label == "moonlight.control") {
            const config = generateControlPacketConfig()

            let protocol: "simple" | "enet" = "simple"
            if (channel.protocol == "enet") {
                protocol = "enet"
            }

            this.controlStream.setChannel(channel, protocol, config)
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
        // TODO
        this.peer.close()
    }

    private lastTotalDecodeTime = 0
    private lastFramesDecoded = 0
    async getStats(): Promise<Record<string, StatValue>> {
        const out: Record<string, StatValue> = {}

        // Control Stream
        const estimatedRtt = this.controlStream.estimatedRtt()
        if (estimatedRtt) {
            out.estimatedClientToRelayRttMs = estimatedRtt.rtt
            out.estimatedClientToRelayRttVarianceMs = estimatedRtt.rttVariance
        }

        const stats = await this.peer.getStats()

        for (const [_key, value] of stats) {
            console.debug(value)

            // Video Stream
            if ("type" in value && "kind" in value
                && value.type == "inbound-rtp" && value.kind == "video"
            ) {
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

const ENET_IP = "192.168.178.2:47999"

class WebRtcControlStream implements IControlStream {

    private logger?: Logger

    private config: ControlPacketConfig | null = null

    private channel: RTCDataChannel | null = null
    private streamType: "simple" | "enet" = "simple"

    private batcher: InputBatcher = new InputBatcher()

    // Enet control stream
    private controlStream: ControlStream | null = null
    private controlStreamPollTimeout: number | null = null
    private enetConnected = false

    private packetBuffer: Array<ControlPacket> = []

    constructor(logger?: Logger) {
        this.logger = logger
    }

    setChannel(channel: null): void
    setChannel(channel: RTCDataChannel, streamType: "simple" | "enet", config: ControlPacketConfig): void
    setChannel(channel: RTCDataChannel | null, streamType?: "simple" | "enet", config?: ControlPacketConfig): void {
        this.channel = channel

        // Clean up old control stream if present
        if (this.controlStream) {
            this.controlStream.uniffiDestroy()
            this.controlStream = null
        }
        if (this.controlStreamPollTimeout != null) {
            globalObject().clearTimeout(this.controlStreamPollTimeout)
        }
        this.enetConnected = false

        if (this.channel && streamType && config) {
            this.config = config
            this.streamType = streamType

            this.channel.binaryType = "arraybuffer"

            this.channel.addEventListener("open", this.boundChannelStateChange)
            this.channel.addEventListener("message", this.boundMessage)

            if (this.streamType == "enet") {
                this.controlStream = new ControlStream(uniffiNow(), {
                    serverVersion: this.config.serverVersion,
                    addr: ENET_IP,
                })
                this.onDataChannelStateChange()
            }

            this.trySendBufferedPackets()
        } else {
            this.streamType = "simple"
            this.channel?.removeEventListener("open", this.boundChannelStateChange)
            this.channel?.removeEventListener("message", this.boundMessage)
        }
    }

    onreceive: ((packet: ControlPacket) => void) | null = null

    private boundMessage = this.onMessage.bind(this)
    private onMessage(event: MessageEvent) {
        if (!this.config) {
            throw "packet config not configured, but a packet was received"
        }

        if (this.streamType == "simple") {
            const packet = controlPacketDeserialize(this.config, PacketDirection.ClientBound, event.data)

            if (packet && this.onreceive) {
                this.onreceive(packet)
            }
        } else if (this.streamType == "enet") {
            if (!this.controlStream) {
                throw "dropping packet because enet control stream is not initialized"
            }

            this.controlStream.handleReceive(
                uniffiNow(),
                ENET_IP,
                event.data
            )

            this.controlStreamPollOutput(false)
        } else {
            this.logger?.debug("failed to deserialize packet")
            console.debug("failed to deserialize packet", event.data)
        }
    }

    private boundChannelStateChange = this.onDataChannelStateChange.bind(this)
    private onDataChannelStateChange() {
        if (this.channel?.readyState == "open") {
            this.trySendBufferedPackets()

            if (this.streamType == "enet" && this.controlStreamPollTimeout == null) {
                // Start loop
                this.controlStreamPollOutput()
            }
        }
    }
    private trySendBufferedPackets() {
        if (!this.channel || this.channel.readyState != "open") {
            return
        }

        if (this.streamType == "enet" && !this.enetConnected) {
            return
        }

        // Send buffered packets
        for (const packet of this.packetBuffer.splice(0)) {
            this.sendRaw(packet)
        }
    }

    send(input: ClientInputEvent): void {
        for (const packet of this.batcher.batchInput(input)) {
            this.sendRaw(packet)
        }

        this.sendBatchedInputs()
    }

    sendRaw(packet: ControlPacket): void {
        console.debug(packet, "sending control packet")

        if (
            !this.channel || this.channel.readyState != "open" ||
            (this.streamType == "enet" && (!this.controlStream || !this.enetConnected))
        ) {
            this.packetBuffer.push(packet)
            return
        }
        if (!this.config) {
            throw "packet config not configured, but a packet was sent"
        }

        this.trySendBufferedPackets()

        if (this.streamType == "simple") {
            const data = controlPacketSerialize(this.config, packet)
            console.debug(data, "sending control data")
            if (data) {
                this.channel.send(data)
            }
        } else if (this.streamType == "enet") {
            this.controlStream?.sendRaw(packet)
            this.controlStreamPollOutput()
        } else {
            this.logger?.debug(`failed to send control packet ${JSON.stringify(packet)}`)
        }
    }

    estimatedRtt(): EstimatedRttInfo | null {
        return this.controlStream?.estimatedRtt() ?? null
    }

    private sendBatchedInputs() {
        for (const packet of this.batcher.removeBatchedInputs()) {
            this.sendRaw(packet)
        }
    }

    private boundPollOutput = this.controlStreamPollOutput.bind(this)
    private controlStreamPollOutput(handleInput = true) {
        if (this.controlStreamPollTimeout != null) {
            globalObject().clearTimeout(this.controlStreamPollTimeout)
        }
        this.controlStreamPollTimeout = null

        if (!this.controlStream) {
            return
        }
        if (!this.channel) {
            return
        }

        if (handleInput) {
            this.controlStream.handleTimeout(uniffiNow())
        }

        let send: UdpTransmit | undefined
        while (send = this.controlStream.pollPacket()) {
            console.debug(send.contents, "enet send")
            this.channel.send(send.contents)
        }

        let event: ControlStreamEvent | undefined
        while (event = this.controlStream.pollEvent()) {
            if (event.tag === ControlStreamEvent_Tags.Connect) {
                this.enetConnected = true

                this.trySendBufferedPackets()
            } else if (event.tag === ControlStreamEvent_Tags.Packet) {
                if (this.onreceive) {
                    this.onreceive(event.inner[0]);
                }
            } else if (event.tag === ControlStreamEvent_Tags.Disconnect) {
                // TODO: reconstruct control stream?
                this.enetConnected = false
            }
        }

        const timeout = this.controlStream.pollTimeout()
        if (timeout != undefined) {
            this.controlStreamPollTimeout = globalObject().setTimeout(this.boundPollOutput, uniffiMillisUntil(timeout))
        }
    }
}
