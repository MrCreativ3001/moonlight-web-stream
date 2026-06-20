import { StreamSupportedVideoCodecs } from "../api_bindings.js"
import { VideoFormats } from "../uniffi/moonlight_common_bindings.js"

export function emptyVideoCodecs(): VideoFormats {
    return {
        h264: false,
        h264High8444: false,
        h265: false,
        h265Main10: false,
        h265Rext8444: false,
        h265Rext10444: false,
        av1Main8: false,
        av1Main10: false,
        av1High8444: false,
        av1High10444: false
    }
}

export function allVideoCodecs(): VideoFormats {
    return {
        h264: true,
        h264High8444: true,
        h265: true,
        h265Main10: true,
        h265Rext8444: true,
        h265Rext10444: true,
        av1Main8: true,
        av1Main10: true,
        av1High8444: true,
        av1High10444: true
    }
}

export function andVideoCodecs(a: VideoFormats, b: VideoFormats): VideoFormats {
    const output: any = {}

    for (const codec2 in a) {
        const codec = codec2 as keyof VideoFormats
        const supportedA = a[codec]
        const supportedB = b[codec]

        let supported = supportedA && supportedB

        output[codec] = supported
    }

    return output
}

export function hasAnyCodec(codecs: VideoFormats): boolean {
    for (const key2 in codecs) {
        const key = key2 as keyof VideoFormats

        if (codecs[key]) {
            return true
        }
    }

    return false
}

export function createSupportedVideoFormatsBits(formats: VideoFormats): number {
    let mask = 0

    if (formats.h264) {
        mask |= StreamSupportedVideoCodecs.H264
    }
    if (formats.h264High8444) {
        mask |= StreamSupportedVideoCodecs.H264_HIGH8_444
    }
    if (formats.h265) {
        mask |= StreamSupportedVideoCodecs.H265
    }
    if (formats.h265Main10) {
        mask |= StreamSupportedVideoCodecs.H265_MAIN10
    }
    if (formats.h265Rext8444) {
        mask |= StreamSupportedVideoCodecs.H265_REXT8_444
    }
    if (formats.h265Rext10444) {
        mask |= StreamSupportedVideoCodecs.H265_REXT10_444
    }
    if (formats.av1Main8) {
        mask |= StreamSupportedVideoCodecs.AV1_MAIN8
    }
    if (formats.av1Main10) {
        mask |= StreamSupportedVideoCodecs.AV1_MAIN10
    }
    if (formats.av1High8444) {
        mask |= StreamSupportedVideoCodecs.AV1_HIGH8_444
    }
    if (formats.av1High10444) {
        mask |= StreamSupportedVideoCodecs.AV1_HIGH10_444
    }

    return mask
}

export function getSelectedVideoCodec(videoCodec: number): keyof typeof StreamSupportedVideoCodecs | null {
    if (videoCodec == StreamSupportedVideoCodecs.H264) {
        return "H264"
    } else if (videoCodec == StreamSupportedVideoCodecs.H264_HIGH8_444) {
        return "H264_HIGH8_444"
    } else if (videoCodec == StreamSupportedVideoCodecs.H265) {
        return "H265"
    } else if (videoCodec == StreamSupportedVideoCodecs.H265_MAIN10) {
        return "H265_MAIN10"
    } else if (videoCodec == StreamSupportedVideoCodecs.H265_REXT8_444) {
        return "H265_REXT8_444"
    } else if (videoCodec == StreamSupportedVideoCodecs.H265_REXT10_444) {
        return "H265_REXT10_444"
    } else if (videoCodec == StreamSupportedVideoCodecs.AV1_MAIN8) {
        return "AV1_MAIN8"
    } else if (videoCodec == StreamSupportedVideoCodecs.AV1_MAIN10) {
        return "AV1_MAIN10"
    } else if (videoCodec == StreamSupportedVideoCodecs.AV1_HIGH8_444) {
        return "AV1_HIGH8_444"
    } else if (videoCodec == StreamSupportedVideoCodecs.AV1_HIGH10_444) {
        return "AV1_HIGH10_444"
    } else {
        return null
    }
}