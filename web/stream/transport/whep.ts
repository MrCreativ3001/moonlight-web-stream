import { WHEPResponse } from "../../api.js";
import { ClientInputEvent, ClientInputEvent_Tags, ControlPacket, ControlPacketConfig, controlPacketConfigNew, controlPacketDeserialize, controlPacketSerialize, MoonlightWebRtcSession, PacketDirection, ServerType, VideoFormats, webrtcSessionApply } from "../../uniffi/moonlight_common_bindings.js";
import { wait } from "../../util.js";
import { TrackAudioPlayer, AudioPlayer } from "../audio/index.js";
import { Logger } from "../log.js";
import { DataPipe } from "../pipeline/pipes.js";
import { StatValue } from "../stats.js";
import { emptyVideoCodecs, VideoCodecSupport } from "../video.js";
import { TrackVideoRenderer, VideoRenderer } from "../video/index.js";
import { IControlStream, Transport, TransportAudioType, TransportConnectData, TransportShutdown, TransportVideoType } from "./index.js";

export type WebRTCWHEPOptions = {
    appId: number,
    width: number,
    height: number,
    fps: number,
    bitrate: number,
    hdr: boolean,
    localAudioPlayMode: boolean,
    preferredCodec?: VideoFormats,
    preferredAudio?: number,
    hostId?: number,
}

export class WebRTCTransport implements Transport {

    readonly implementationName: string = "webrtc-whep"

    readonly controlStream = new WebRtcControlStream()
    onconnect: ((connectData: TransportConnectData) => void) | null = null
    onclose: ((shutdown: TransportShutdown) => void) | null = null

    private logger?: Logger

    private peer = new RTCPeerConnection()

    constructor(logger?: Logger) {
        this.logger = logger

        // Set Event Listeners
        this.peer.addEventListener("connectionstatechange", this.onStateChange.bind(this))
        this.peer.addEventListener("datachannel", this.onDataChannel.bind(this))
        this.peer.addEventListener("track", this.onTrack.bind(this))

        // Add Media
        this.peer.addTransceiver("video", { direction: "recvonly" })
        this.peer.addTransceiver("audio", { direction: "recvonly" })

        // Dummy data channel required so that the answerer knows we accept data channels
        this.peer.createDataChannel("dummy")
    }

    async createOffer(options: WebRTCWHEPOptions): Promise<string> {
        this.logger?.debug("creating webrtc offer")

        const offer = await this.peer.createOffer()
        if (offer.type != "offer") {
            throw `WHEP offer is of type ${offer.type}`
        }

        this.logger?.debug("setting webrtc local description")
        await this.peer.setLocalDescription(offer)

        // Insert custom options
        const sdpOptions: MoonlightWebRtcSession = {
            controlSimple: true,
            // TODO: control enet
            controlEnet: false,
            ...options
        }
        const sdp = webrtcSessionApply(offer.sdp ?? "", sdpOptions)

        this.logger?.debug(`successfully generated webrtc sdp with options ${JSON.stringify(sdpOptions)}`)
        console.debug("Client Sdp", sdp)

        return sdp
    }
    async setAnswer(response: WHEPResponse): Promise<void> {
        console.debug("Server Sdp", JSON.stringify(response))

        this.logger?.debug(`received whep response with location "${response.location}" ice servers ${response.iceServers.flatMap(server => server.urls).concat(",")}`)

        this.peer.setConfiguration({
            iceServers: response.iceServers,
        })

        await this.peer.setRemoteDescription({
            type: "answer",
            sdp: response.answerSdp,
        })
    }

    private connectData: TransportConnectData | null = null
    private async collectConnectData(): Promise<TransportConnectData> {
        if (this.connectData) {
            return this.connectData
        }

        if (!this.videoStream || !this.audioStream) {
            throw `WebRTC WHEP response didn't contain a video and audio stream! Video: ${this.videoStream != null}, Audio: ${this.audioStream != null}`
        }

        // Wait for the stream to receive any video data
        const videoSettings = this.videoStream.getSettings()
        const audioSettings = this.audioStream.getSettings()

        this.connectData = {
            capabilities: {
                touch: false
            },
            videoType: "videotrack",
            videoSetup: {
                width: videoSettings.width ?? -1,
                height: videoSettings.height ?? -1,
                fps: videoSettings.frameRate ?? -1,
                // TODO: gather codec using stats
                codec: "H264",
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

            this.collectConnectData().then(connectData => {
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

    // -- Control Stream / Media
    private onDataChannel(event: RTCDataChannelEvent) {
        const channel = event.channel

        this.logger?.debug(`received data channel with label: ${channel.label}, protocol: ${channel.protocol}`)

        if (channel.label == "control") {
            const config = controlPacketConfigNew(
                { major: 7, minor: 0, patch: 0, sunshineIdentifier: -1, serverType: ServerType.Sunshine },
                true
            )
            if (!config) {
                throw "generated invalid packet config"
            }

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

    getRequiredVideoPipelineCodec(): VideoCodecSupport {
        if (!this.videoStream) {
            throw "the stream must be connected!"
        }

        // TODO: figure out the exact codec
        const codecs = emptyVideoCodecs()

        codecs.H264 = true

        return codecs
    }
    getRequiredVideoPipelineType(): TransportVideoType {
        return "videotrack"
    }

    setVideoPipeline(type: "videotrack", pipeline: (TrackVideoRenderer & VideoRenderer)): Promise<void>;
    setVideoPipeline(type: "data", pipeline: (DataPipe & VideoRenderer)): Promise<void>;
    async setVideoPipeline(type: TransportVideoType, pipeline: unknown): Promise<void> {
        if (!this.videoStream || !this.connectData) {
            throw "the stream must be connected!"
        }

        if (type == "videotrack") {
            const trackPipeline = pipeline as (TrackVideoRenderer & VideoRenderer)

            await trackPipeline.setup(this.connectData.videoSetup)
            trackPipeline.setTrack(this.videoStream)
        } else if (type == "data") {
            throw "unimplemented"
        }
    }

    // Audio
    private audioStream: MediaStreamTrack | null = null

    getRequiredAudioPipelineType(): TransportAudioType {
        return "audiotrack"
    }
    setAudioPipeline(type: "audiotrack", pipeline: (TrackAudioPlayer & AudioPlayer)): Promise<void>
    setAudioPipeline(type: "data", pipeline: (DataPipe & AudioPlayer)): Promise<void>
    async setAudioPipeline(type: TransportAudioType, pipeline: AudioPlayer): Promise<void> {
        if (!this.audioStream || !this.connectData) {
            throw "the stream must be connected!"
        }

        if (type == "audiotrack") {
            const trackPipeline = pipeline as (TrackAudioPlayer & AudioPlayer)

            await trackPipeline.setup(this.connectData.audioSetup)
            trackPipeline.setTrack(this.audioStream)
        } else if (type == "data") {
            throw "unimplemented"
        }
    }

    async close(): Promise<void> {
        // TODO
    }

    async getStats(): Promise<Record<string, StatValue>> {
        // TODO
        return {}
    }
}

class WebRtcControlStream implements IControlStream {

    private logger?: Logger

    private config: ControlPacketConfig | null = null

    private channel: RTCDataChannel | null = null

    private packetBuffer: Array<ControlPacket> = []

    constructor(logger?: Logger) {
        this.logger = logger
    }

    setChannel(channel: null): void
    setChannel(channel: RTCDataChannel, config: ControlPacketConfig): void
    setChannel(channel: RTCDataChannel | null, config?: ControlPacketConfig): void {
        this.channel = channel

        if (this.channel && config) {
            this.config = config

            this.channel.binaryType = "arraybuffer"

            this.channel.addEventListener("open", this.boundChannelOpen)
            this.channel.addEventListener("message", this.boundMessage)

            this.trySendBufferedPackets()
        } else {
            this.channel?.removeEventListener("open", this.boundChannelOpen)
            this.channel?.removeEventListener("message", this.boundMessage)
        }
    }

    onreceive: ((packet: ControlPacket) => void) | null = null

    private boundMessage = this.onMessage.bind(this)
    private onMessage(event: MessageEvent) {
        if (!this.config) {
            throw "packet config not configured, but a packet was received"
        }

        const packet = controlPacketDeserialize(this.config, PacketDirection.ClientBound, event.data)
        if (packet) {
            if (this.onreceive) {
                this.onreceive(packet)
            }
        } else {
            this.logger?.debug("failed to deserialize packet")
            console.debug("failed to deserialize packet", event.data)
        }
    }

    private boundChannelOpen = this.onDataChannelOpen.bind(this)
    private onDataChannelOpen() {
        this.trySendBufferedPackets()
    }
    private trySendBufferedPackets() {
        if (!this.channel || this.channel.readyState != "open") {
            return
        }

        // Send buffered packets
        for (const packet of this.packetBuffer.splice(0)) {
            this.sendRaw(packet)
        }
    }

    send(input: ClientInputEvent): void {
        if (input.tag == ClientInputEvent_Tags.MouseMoveRelative) {
            this.sendRaw(new ControlPacket.MouseMoveRelative({
                deltaX: input.inner.deltaX,
                deltaY: input.inner.deltaY,
            }))
        }
    }
    sendRaw(packet: ControlPacket): void {
        console.debug(packet, "sending control packet")

        if (!this.channel || this.channel.readyState != "open") {
            this.packetBuffer.push(packet)
            return
        }
        if (!this.config) {
            throw "packet config not configured, but a packet was sent"
        }

        this.trySendBufferedPackets()

        const data = controlPacketSerialize(this.config, packet)
        if (data) {
            console.debug(data, "sending control data")
            this.channel.send(data)
        } else {
            this.logger?.debug(`failed to send control packet ${JSON.stringify(packet)}`)
        }
    }
}
