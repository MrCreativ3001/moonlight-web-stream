import { TransportChannelId } from "../../api_bindings.js"
import { ClientInputEvent, ControlPacket, ControlStreamInput } from "../../uniffi/moonlight_common_bindings.js"
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

    onclose: ((shutdown: TransportShutdown) => void) | null
    close(): Promise<void>

    getStats(): Promise<Record<string, StatValue>>
}

export interface IControlStream {
    send(input: ClientInputEvent): void
    sendRaw(packet: ControlPacket): void

    onreceive: (packet: ControlPacket) => void | null
}