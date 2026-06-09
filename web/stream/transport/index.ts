import { TransportChannelId } from "../../api_bindings.js"
import { ClientInputEvent, ControlPacket } from "../../uniffi/moonlight_common_bindings.js"
import { AudioPlayer, TrackAudioPlayer } from "../audio/index.js"
import { DataPipe } from "../pipeline/pipes.js"
import { StatValue } from "../stats.js"
import { VideoCodecSupport } from "../video.js"
import { TrackVideoRenderer, VideoRenderer } from "../video/index.js"

export type TransportChannelIdKey = keyof typeof TransportChannelId
export type TransportChannelIdValue = typeof TransportChannelId[TransportChannelIdKey]

export type TransportVideoType = "videotrack" // TrackTransportChannel
    | "data" // Data like https://github.com/moonlight-stream/moonlight-common-c/blob/b126e481a195fdc7152d211def17190e3434bcce/src/Limelight.h#L298


export type TransportVideoSetup = {
    // List containing all supported types, priority highest=0, lowest=biggest index
    type: Array<TransportVideoType>
}

export type TransportAudioType = "audiotrack" // TrackTransportChannel
    | "data" // Data like https://github.com/moonlight-stream/moonlight-common-c/blob/b126e481a195fdc7152d211def17190e3434bcce/src/Limelight.h#L356


export type TransportAudioSetup = {
    // List containing all supported types, priority highest=0, lowest=biggest index
    type: Array<TransportAudioType>
}

// failednoconnect => a connection failed without firstly being established
// failed => a connection was ungracefully closed
// disconnect => a connection was gracefully closed
export type TransportShutdown = "failednoconnect" | "failed" | "disconnect"

export interface Transport {
    readonly implementationName: string

    readonly controlStream: IControlStream

    onconnect: (() => void) | null
    onclose: ((shutdown: TransportShutdown) => void) | null
    close(): Promise<void>

    // -- Only allowed after onconnect was called
    getRequiredVideoPipelineCodec(): VideoCodecSupport
    getRequiredVideoPipelineType(): TransportVideoType
    setVideoPipeline(type: "videotrack", pipeline: (TrackVideoRenderer & VideoRenderer)): Promise<void>
    setVideoPipeline(type: "data", pipeline: (DataPipe & VideoRenderer)): Promise<void>

    getRequiredAudioPipelineType(): TransportAudioType
    setAudioPipeline(type: "audiotrack", pipeline: (TrackAudioPlayer & AudioPlayer)): Promise<void>
    setAudioPipeline(type: "data", pipeline: (DataPipe & AudioPlayer)): Promise<void>

    getStats(): Promise<Record<string, StatValue>>
}

export interface IControlStream {
    send(input: ClientInputEvent): void
    sendRaw(packet: ControlPacket): void

    onreceive: ((packet: ControlPacket) => void) | null
}