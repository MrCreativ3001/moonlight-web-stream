import { WHEPResponse } from "../../api.js";
import { ClientInputEvent, ControlPacket, ControlPacketConfig, controlPacketConfigNew, controlPacketDeserialize, controlPacketSerialize, MoonlightWebRtcSession, PacketDirection, ServerType, VideoFormats, webrtcSessionApply } from "../../uniffi/moonlight_common_bindings.js";
import { TrackAudioPlayer, AudioPlayer } from "../audio/index.js";
import { Logger } from "../log.js";
import { DataPipe } from "../pipeline/pipes.js";
import { StatValue } from "../stats.js";
import { emptyVideoCodecs, VideoCodecSupport } from "../video.js";
import { TrackVideoRenderer, VideoRenderer } from "../video/index.js";
import { IControlStream, Transport, TransportAudioType, TransportShutdown, TransportVideoType } from "./index.js";

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
    onconnect: (() => void) | null = null
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
            controlEnet: true,
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

    private wasConnected = false
    private onStateChange() {
        if (this.peer.connectionState == "connected") {
            this.wasConnected = true

            if (this.onconnect) {
                this.onconnect()
            }
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
        if (!this.videoStream) {
            throw "the stream must be connected!"
        }

        if (type == "videotrack") {
            const trackPipeline = pipeline as (TrackVideoRenderer & VideoRenderer)

            const settings = this.videoStream.getSettings()

            await trackPipeline.setup({
                width: settings.width ?? 0,
                height: settings.height ?? 0,
                fps: settings.frameRate ?? 0,
                // TODO: gather codec using stats
                codec: "H264",
            })
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
        if (!this.audioStream) {
            throw "the stream must be connected!"
        }

        if (type == "audiotrack") {
            const trackPipeline = pipeline as (TrackAudioPlayer & AudioPlayer)

            const settings = this.audioStream.getSettings()

            await trackPipeline.setup({
                channels: settings.channelCount ?? 2,
                sampleRate: settings.sampleRate ?? 48000,
                // TODO
                streams: 0,
                coupledStreams: 0,
                samplesPerFrame: 0,
                mapping: []
            })
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
            this.channel.addEventListener("message", this.boundMessage)

            // Send buffered packets
            for (const packet of this.packetBuffer.splice(0)) {
                this.sendRaw(packet)
            }
        } else {
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
    }

    send(input: ClientInputEvent): void {
        // TODO
    }
    sendRaw(packet: ControlPacket): void {
        if (!this.channel || this.channel.readyState != "open") {
            this.packetBuffer.push(packet)
            return
        }
        if (!this.config) {
            throw "packet config not configured, but a packet was sent"
        }

        const data = controlPacketSerialize(this.config, packet)
        if (data) {
            this.channel.send(data)
        } else {
            this.logger?.debug(`failed to send control packet ${JSON.stringify(packet)}`)
        }
    }
}
