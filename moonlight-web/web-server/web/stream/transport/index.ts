import { TransportChannelId } from "../../api_bindings.js"
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
export const TRANSPORT_CHANNEL_OPTIONS: Record<keyof typeof TransportChannelId, TransportChannelOption> = {
    GENERAL: { reliable: true, ordered: true, serverCreated: true },
    STATS: { reliable: true, ordered: true },
    HOST_VIDEO: { reliable: false, ordered: true },
    HOST_AUDIO: { reliable: false, ordered: true },
    MOUSE_RELIABLE: { reliable: true, ordered: true },
    MOUSE_ABSOLUTE: { reliable: false, ordered: false },
    MOUSE_RELATIVE: { reliable: true, ordered: false },
    KEYBOARD: { reliable: true, ordered: true },
    TOUCH: { reliable: true, ordered: true },
    CONTROLLERS: { reliable: true, ordered: true },
    CONTROLLER0: { reliable: false, ordered: false },
    CONTROLLER1: { reliable: false, ordered: false },
    CONTROLLER2: { reliable: false, ordered: false },
    CONTROLLER3: { reliable: false, ordered: false },
    CONTROLLER4: { reliable: false, ordered: false },
    CONTROLLER5: { reliable: false, ordered: false },
    CONTROLLER6: { reliable: false, ordered: false },
    CONTROLLER7: { reliable: false, ordered: false },
    CONTROLLER8: { reliable: false, ordered: false },
    CONTROLLER9: { reliable: false, ordered: false },
    CONTROLLER10: { reliable: false, ordered: false },
    CONTROLLER11: { reliable: false, ordered: false },
    CONTROLLER12: { reliable: false, ordered: false },
    CONTROLLER13: { reliable: false, ordered: false },
    CONTROLLER14: { reliable: false, ordered: false },
    CONTROLLER15: { reliable: false, ordered: false },
    RTT: { reliable: true, ordered: true }
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

    getStats(): Promise<Record<string, string>>
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