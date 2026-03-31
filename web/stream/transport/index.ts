import { TransportChannelId } from "../../api_bindings.js"
import { StatValue } from "../stats.js"
import { VideoCodecSupport } from "../video.js"

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

// TOOD: common transport channel types: e.g. reliable / unreliable, ordered usw
export type TransportChannelOption = {
    ordered: boolean
    reliable: boolean
    // default = false
    serverCreated?: boolean
}
// failednoconnect => a connection failed without firstly being established
// failed => a connection was ungracefully closed
// disconnect => a connection was gracefully closed
export type TransportShutdown = "failednoconnect" | "failed" | "disconnect"

export interface Transport {
    readonly implementationName: string

    getChannel(id: TransportChannelIdValue): TransportChannel

    setupHostVideo(setup: TransportVideoSetup): Promise<VideoCodecSupport>
    setupHostAudio(setup: TransportAudioSetup): Promise<void>

    onclose: ((shutdown: TransportShutdown) => void) | null
    close(): Promise<void>

    getStats(): Promise<Record<string, StatValue>>
}

export type TransportChannel = VideoTrackTransportChannel | AudioTrackTransportChannel | DataTransportChannel
interface TransportChannelBase {
    readonly type: string

    readonly canReceive: boolean
    readonly canSend: boolean
}

export interface TrackTransportChannel extends TransportChannelBase {
    setTrack(track: MediaStreamTrack | null): void

    addTrackListener(listener: (track: MediaStreamTrack) => void): void
    removeTrackListener(listener: (track: MediaStreamTrack) => void): void
}
export interface VideoTrackTransportChannel extends TrackTransportChannel {
    readonly type: "videotrack"
}
export interface AudioTrackTransportChannel extends TrackTransportChannel {
    readonly type: "audiotrack"
}

export interface DataTransportChannel extends TransportChannelBase {
    readonly type: "data"

    addReceiveListener(listener: (data: ArrayBuffer) => void): void
    removeReceiveListener(listener: (data: ArrayBuffer) => void): void

    send(message: ArrayBuffer): void
    estimatedBufferedBytes(): number | null
}