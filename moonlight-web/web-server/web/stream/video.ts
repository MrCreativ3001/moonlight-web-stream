import { StreamSupportedVideoCodecs } from "../api_bindings.js"

/// "maybe" means that we might support this codec but we can't know until we play the video
export type VideoCodecSupportFlag = boolean | "maybe"

export type VideoCodecSupport = {
    H264: VideoCodecSupportFlag
    H264_HIGH8_444: VideoCodecSupportFlag
    H265: VideoCodecSupportFlag
    H265_MAIN10: VideoCodecSupportFlag
    H265_REXT8_444: VideoCodecSupportFlag
    H265_REXT10_444: VideoCodecSupportFlag
    AV1_MAIN8: VideoCodecSupportFlag
    AV1_MAIN10: VideoCodecSupportFlag
    AV1_HIGH8_444: VideoCodecSupportFlag
    AV1_HIGH10_444: VideoCodecSupportFlag
} & Record<string, VideoCodecSupportFlag>

export const CAPABILITIES_CODECS: Record<keyof VideoCodecSupport, { mimeType: string, fmtpLine: Array<string> }> = {
    // H264
    "H264": { mimeType: "video/H264", fmtpLine: ["profile-level-id=42e01f"] },
    "H264_HIGH8_444": { mimeType: "video/H264", fmtpLine: ["profile-level-id=640032"] },
    // H265
    "H265": { mimeType: "video/H265", fmtpLine: [] }, // <-- Safari H265 fmtpLine is empty (for some dumb reason)
    "H265_MAIN10": { mimeType: "video/H265", fmtpLine: ["profile-id=2", "tier-flag=0", "tx-mode=SRST"] },
    "H265_REXT8_444": { mimeType: "video/H265", fmtpLine: ["profile-id=4", "tier-flag=0", "tx-mode=SRST"] },
    "H265_REXT10_444": { mimeType: "video/H265", fmtpLine: ["profile-id=5", "tier-flag=0", "tx-mode=SRST"] },
    // Av1
    "AV1_MAIN8": { mimeType: "video/AV1", fmtpLine: [] }, // <-- Safari AV1 fmtpLine is empty
    "AV1_MAIN10": { mimeType: "video/AV1", fmtpLine: [] }, // <-- Safari AV1 fmtpLine is empty
    "AV1_HIGH8": { mimeType: "video/AV1", fmtpLine: ["profile=1"] },
    "AV1_HIGH10": { mimeType: "video/AV1", fmtpLine: ["profile=1"] },
}

export function emptyVideoCodecs(): VideoCodecSupport {
    return {
        H264: false,
        H264_HIGH8_444: false,
        H265: false,
        H265_MAIN10: false,
        H265_REXT8_444: false,
        H265_REXT10_444: false,
        AV1_MAIN8: false,
        AV1_MAIN10: false,
        AV1_HIGH8_444: false,
        AV1_HIGH10_444: false
    }
}

export function maybeVideoCodecs(): VideoCodecSupport {
    return {
        H264: "maybe",
        H264_HIGH8_444: "maybe",
        H265: "maybe",
        H265_MAIN10: "maybe",
        H265_REXT8_444: "maybe",
        H265_REXT10_444: "maybe",
        AV1_MAIN8: "maybe",
        AV1_MAIN10: "maybe",
        AV1_HIGH8_444: "maybe",
        AV1_HIGH10_444: "maybe"
    }
}

export function allVideoCodecs(): VideoCodecSupport {
    return {
        H264: true,
        H264_HIGH8_444: true,
        H265: true,
        H265_MAIN10: true,
        H265_REXT8_444: true,
        H265_REXT10_444: true,
        AV1_MAIN8: true,
        AV1_MAIN10: true,
        AV1_HIGH8_444: true,
        AV1_HIGH10_444: true
    }
}

export function andVideoCodecs(a: VideoCodecSupport, b: VideoCodecSupport): VideoCodecSupport {
    const output: any = {}

    for (const codec in a) {
        const supportedA = a[codec]
        const supportedB = b[codec]

        let supported: VideoCodecSupportFlag = false
        if (supportedA === true && supportedB === true) {
            supported = true
        } else if (
            (supportedA === "maybe" && supportedB === "maybe") ||
            (supportedA === "maybe" && supportedB === true) ||
            (supportedB === "maybe" && supportedA === true)
        ) {
            supported = "maybe"
        }

        output[codec] = supported
    }

    return output
}

export function hasAnyCodec(codecs: VideoCodecSupport): boolean {
    for (const key in codecs) {
        if (codecs[key]) {
            return true
        }
    }

    return false
}

export function createSupportedVideoFormatsBits(support: VideoCodecSupport): number {
    let mask = 0

    if (support.H264) {
        mask |= StreamSupportedVideoCodecs.H264
    }
    if (support.H264_HIGH8_444) {
        mask |= StreamSupportedVideoCodecs.H264_HIGH8_444
    }
    if (support.H265) {
        mask |= StreamSupportedVideoCodecs.H265
    }
    if (support.H265_MAIN10) {
        mask |= StreamSupportedVideoCodecs.H265_MAIN10
    }
    if (support.H265_REXT8_444) {
        mask |= StreamSupportedVideoCodecs.H265_REXT8_444
    }
    if (support.H265_REXT10_444) {
        mask |= StreamSupportedVideoCodecs.H265_REXT10_444
    }
    if (support.AV1_MAIN8) {
        mask |= StreamSupportedVideoCodecs.AV1_MAIN8
    }
    if (support.AV1_MAIN10) {
        mask |= StreamSupportedVideoCodecs.AV1_MAIN10
    }
    if (support.AV1_HIGH8_444) {
        mask |= StreamSupportedVideoCodecs.AV1_HIGH8_444
    }
    if (support.AV1_HIGH10_444) {
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