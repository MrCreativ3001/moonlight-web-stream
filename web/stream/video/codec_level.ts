/// Computes codec strings whose level matches the actual stream dimensions
/// and frame rate.
///
/// The level in the codec string is a promise to the decoder about the
/// heaviest bitstream it will see. If the hint promises less than the SPS
/// inside the stream asks for (e.g. a level 3.0 hint for a 1440p120 stream),
/// Chrome can silently fall back to software decoding even though the
/// hardware decoder would have been fine. See
/// https://github.com/MrCreativ3001/moonlight-web-stream/issues/136

import { VideoFormats } from "../../uniffi/moonlight_common_bindings"

// H.264 (ITU-T H.264 Table A-1): level -> [level_idc, MaxFS in MBs, MaxMBPS in MB/s]
const H264_LEVELS: Array<[number, number, number]> = [
    [0x1E, 1620, 40500],      // 3.0
    [0x1F, 3600, 108000],     // 3.1
    [0x20, 5120, 216000],     // 3.2
    [0x28, 8192, 245760],     // 4.0
    [0x2A, 8704, 522240],     // 4.2
    [0x32, 22080, 589824],    // 5.0
    [0x33, 36864, 983040],    // 5.1
    [0x34, 36864, 2073600],   // 5.2
    [0x3C, 139264, 4177920],  // 6.0
    [0x3D, 139264, 8355840],  // 6.1
    [0x3E, 139264, 16711680], // 6.2
]

// H.265 (ITU-T H.265 Table A.8, main tier): level -> [level as used in the
// codec string (30 * level number), MaxLumaPs, MaxLumaSr]
const H265_LEVELS: Array<[number, number, number]> = [
    [93, 983040, 33177600],       // 3.1
    [120, 2228224, 66846720],     // 4.0
    [123, 2228224, 133693440],    // 4.1
    [150, 8912896, 267386880],    // 5.0
    [153, 8912896, 534773760],    // 5.1
    [156, 8912896, 1069547520],   // 5.2
    [180, 35651584, 1069547520],  // 6.0
    [183, 35651584, 2139095040],  // 6.1
    [186, 35651584, 4278190080],  // 6.2
]

// AV1 (spec Annex A.3): seq_level_idx -> [idx, MaxPicSize in samples, MaxDisplayRate in samples/s]
const AV1_LEVELS: Array<[number, number, number]> = [
    [4, 1704960, 39938048],      // 3.0
    [5, 2359296, 116444160],     // 3.1
    [8, 2359296, 141557760],     // 4.0 (MaxDisplayRate of 4.1 tier used conservatively below via ordering)
    [9, 2359296, 283115520],     // 4.1
    [12, 8912896, 534773760],    // 5.0
    [13, 8912896, 1069547520],   // 5.1
    [16, 35651584, 2139095040],  // 6.0
    [17, 35651584, 4278190080],  // 6.1
]

function pickLevel(
    levels: Array<[number, number, number]>,
    frameSize: number,
    sampleRate: number,
): number {
    for (const [id, maxSize, maxRate] of levels) {
        if (frameSize <= maxSize && sampleRate <= maxRate) {
            return id
        }
    }
    // Heavier than the largest level we know: promise the largest. The
    // decoder will still reject the stream if it truly can't handle it.
    return levels[levels.length - 1][0]
}

export function h264LevelIdc(width: number, height: number, fps: number): number {
    const macroblocks = Math.ceil(width / 16) * Math.ceil(height / 16)
    return pickLevel(H264_LEVELS, macroblocks, macroblocks * fps)
}

export function h265Level(width: number, height: number, fps: number): number {
    const lumaSamples = width * height
    return pickLevel(H265_LEVELS, lumaSamples, lumaSamples * fps)
}

export function av1LevelIdx(width: number, height: number, fps: number): number {
    const samples = width * height
    return pickLevel(AV1_LEVELS, samples, samples * fps)
}

function toHex(value: number): string {
    return ("0" + value.toString(16).toUpperCase()).slice(-2)
}

function pad2(value: number): string {
    return ("0" + value).slice(-2)
}

/// In-band codec strings (avc3/hev1: parameter sets travel in the
/// bitstream) with the level part computed from the actual stream setup.
/// Profile bytes stay identical to the previous hardcoded strings; only the
/// level is dynamic.
export function videoDecoderCodecInBand(
    codec: keyof VideoFormats,
    width: number,
    height: number,
    fps: number,
): string | null {
    switch (codec) {
        case "h264":
            return `avc3.42E0${toHex(h264LevelIdc(width, height, fps))}`
        case "h264High8444":
            return `avc3.6400${toHex(h264LevelIdc(width, height, fps))}`
        case "h265":
            return `hev1.1.6.L${h265Level(width, height, fps)}.B0`
        case "h265Main10":
            return `hev1.2.4.L${h265Level(width, height, fps)}.90`
        case "h265Rext8444":
            return `hev1.6.6.L${h265Level(width, height, fps)}.90`
        case "h265Rext10444":
            return `hev1.6.10.L${h265Level(width, height, fps)}.90`
        case "av1Main8":
            return `av01.0.${pad2(av1LevelIdx(width, height, fps))}M.08`
        case "av1Main10":
            return `av01.0.${pad2(av1LevelIdx(width, height, fps))}M.10`
        case "av1High8444":
            return `av01.0.${pad2(av1LevelIdx(width, height, fps))}M.08`
        case "av1High10444":
            return `av01.0.${pad2(av1LevelIdx(width, height, fps))}M.10`
        default:
            // Unknown codec: let the caller fall back to its static table
            return null
    }
}
