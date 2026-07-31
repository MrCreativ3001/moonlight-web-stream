import { ClientInputEvent, ControlPacket, ControlPacketConfig, controlPacketConfigNew, ServerType, VideoFormats } from "../../uniffi/moonlight_common_bindings.js"
import { AudioPlayer, AudioPlayerSetup, TrackAudioPlayer } from "../audio/index.js"
import { StreamCapabilities } from "../index.js"
import { DataPipe } from "../pipeline/pipes.js"
import { StatValue } from "../stats.js"
import { TrackVideoRenderer, VideoRenderer, VideoRendererSetup } from "../video/index.js"

export type TransportVideoType = "videotrack" // TrackTransportChannel
    | "data" // Data like https://github.com/moonlight-stream/moonlight-common-c/blob/b126e481a195fdc7152d211def17190e3434bcce/src/Limelight.h#L298

export type TransportAudioType = "audiotrack" // TrackTransportChannel
    | "data" // Data like https://github.com/moonlight-stream/moonlight-common-c/blob/b126e481a195fdc7152d211def17190e3434bcce/src/Limelight.h#L356

// failednoconnect => a connection failed without firstly being established
// failed => a connection was ungracefully closed
// disconnect => a connection was gracefully closed
export type TransportShutdown = "failednoconnect" | "failed" | "disconnect"

export type TransportOptions = {
    appId: number,
    width: number,
    height: number,
    fps: number,
    bitrate: number,
    hdr: boolean,
    localAudioPlayMode: boolean,
    /// These are the available video codecs when using data transport
    supportedCodecs: VideoFormats,
    preferredCodecs?: VideoFormats,
    preferredAudio?: number,
    hostId: number,
}

export type TransportConnectData = {
    capabilities: StreamCapabilities,
    videoType: TransportVideoType,
    videoSetup: VideoRendererSetup,
    audioType: TransportAudioType,
    audioSetup: AudioPlayerSetup,
    appName: string,
}

export interface Transport {
    readonly implementationName: string

    readonly controlStream: IControlStream

    onconnect: ((connectData: TransportConnectData) => void) | null
    onclose: ((shutdown: TransportShutdown) => void) | null
    close(): Promise<void>

    // -- Only allowed after onconnect was called
    setVideoPipeline(type: "videotrack", pipeline: (TrackVideoRenderer & VideoRenderer)): Promise<void>
    setVideoPipeline(type: "data", pipeline: (DataPipe & VideoRenderer)): Promise<void>

    setAudioPipeline(type: "audiotrack", pipeline: (TrackAudioPlayer & AudioPlayer)): Promise<void>
    setAudioPipeline(type: "data", pipeline: (DataPipe & AudioPlayer)): Promise<void>

    getStats(): Promise<Record<string, StatValue>>
}

export function generateControlPacketConfig(): ControlPacketConfig {
    const config = controlPacketConfigNew(
        { major: 7, minor: 0, patch: 0, sunshineIdentifier: -1, serverType: ServerType.Sunshine },
        true
    )
    if (!config) {
        throw "generated invalid packet config"
    }

    return config
}

export interface IControlStream {
    send(input: ClientInputEvent): void
    sendRaw(packet: ControlPacket): void

    onreceive: ((packet: ControlPacket) => void) | null
}
