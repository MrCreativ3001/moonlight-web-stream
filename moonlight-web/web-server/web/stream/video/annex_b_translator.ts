import { numToHex } from "../../util.js";
import { ByteBuffer } from "../buffer.js";
import { Logger } from "../log.js";
import { VideoCodecSupport } from "../video.js";
import { VideoDecodeUnit } from "./index.js";

// Translates annex b prefixed NALU's into AvCc 

// TODO: this should use the translator to get the codec instead of just statically defining them
export const VIDEO_DECODER_CODECS_OUT_OF_BAND: Record<keyof VideoCodecSupport, string> = {
    "H264": "avc1.42E01E",
    "H264_HIGH8_444": "avc1.640032",
    "H265": "hvc1.1.6.L93.B0",
    "H265_MAIN10": "hvc1.2.4.L120.90",
    "H265_REXT8_444": "hvc1.6.6.L93.90",
    "H265_REXT10_444": "hvc1.6.10.L120.90",
    "AV1_MAIN8": "av01.0.04M.08",
    "AV1_MAIN10": "av01.0.04M.10",
    "AV1_HIGH8_444": "av01.0.08M.08",
    "AV1_HIGH10_444": "av01.0.08M.10"
}


const START_CODE_SHORT = new Uint8Array([0x00, 0x00, 0x01]); // 3-byte start code
const START_CODE_LONG = new Uint8Array([0x00, 0x00, 0x00, 0x01]); // 4-byte start code
function startsWith(buffer: Uint8Array, position: number, check: Uint8Array): boolean {
    for (let i = 0; i < check.length; i++) {
        if (buffer[position + i] != check[i]) {
            return false
        }
    }
    return true
}

export abstract class CodecStreamTranslator {

    protected logger: Logger | null

    constructor(logger?: Logger) {
        this.logger = logger ?? null
    }

    protected decoderConfig: VideoDecoderConfig = {
        codec: "undefined"
    }

    setBaseConfig(decoderConfig: VideoDecoderConfig) {
        this.decoderConfig = decoderConfig
    }
    getCurrentConfig(): VideoDecoderConfig | null {
        return this.decoderConfig
    }

    protected currentFrame = new Uint8Array(1000)

    submitDecodeUnit(unit: VideoDecodeUnit): { configure: VideoDecoderConfig | null, chunk: Uint8Array | null, error: false } | { error: true } {
        if (!this.decoderConfig) {
            this.logger?.debug("Failed to retrieve decoderConfig which should already exist for VideoDecoder", { type: "fatal" })
            return { error: true }
        }

        // We're getting annex b prefixed nalus but we need length prefixed nalus -> convert them based on codec

        const { shouldProcess } = this.startProcessChunk(unit)

        if (!shouldProcess) {
            return { configure: null, chunk: null, error: false }
        }

        const data = new Uint8Array(unit.data)

        let unitBegin = 0
        let currentPosition = 0
        let currentFrameSize = 0

        let handleStartCode = () => {
            const slice = data.slice(unitBegin, currentPosition)

            const { include } = this.onChunkUnit(slice)

            if (include) {
                // Append size + data
                this.checkFrameBufferSize(currentFrameSize, slice.length + 4)

                // Append size
                const sizeBuffer = new ByteBuffer(4)
                sizeBuffer.putU32(slice.length)
                sizeBuffer.flip()

                this.currentFrame.set(sizeBuffer.getRemainingBuffer(), currentFrameSize)

                // Append data
                this.currentFrame.set(slice, currentFrameSize + 4)

                currentFrameSize += slice.length + 4
            }
        }

        while (currentPosition < data.length) {
            let startCodeLength = 0
            let foundStartCode = false

            if (startsWith(data, currentPosition, START_CODE_LONG)) {
                foundStartCode = true
                startCodeLength = START_CODE_LONG.length
            } else if (startsWith(data, currentPosition, START_CODE_SHORT)) {
                foundStartCode = true
                startCodeLength = START_CODE_SHORT.length
            }

            if (foundStartCode) {
                if (currentPosition != 0) {
                    handleStartCode()
                }

                currentPosition += startCodeLength
                unitBegin = currentPosition
            } else {
                currentPosition += 1;
            }
        }

        // The last nal also needs to get processed
        handleStartCode()

        const { reconfigure } = this.endChunk()

        const chunk = this.currentFrame.slice(0, currentFrameSize)

        return {
            configure: reconfigure ? this.decoderConfig : null,
            chunk,
            error: false
        }
    }

    protected abstract startProcessChunk(unit: VideoDecodeUnit): { shouldProcess: boolean };
    protected abstract onChunkUnit(slice: Uint8Array): { include: boolean };
    protected abstract endChunk(): { reconfigure: boolean };

    protected checkFrameBufferSize(currentSize: number, requiredExtra: number) {
        if (currentSize + requiredExtra > this.currentFrame.length) {
            const newFrame = new Uint8Array((currentSize + requiredExtra) * 2);

            newFrame.set(this.currentFrame);
            this.currentFrame = newFrame;
        }
    }
}

// TODO: search for the spec of Avcc and adjust these to better comply / have more info

export function h264NalType(header: number): number {
    return header & 0x1f;
}

export type H264Sps = {
    profileIdc: number
    constraintFlags: number
    levelIdc: number
    avc1: string
}

export function h264ParseSps(sps: ByteBuffer): H264Sps {
    // First byte is NAL header, skip it
    const nalHeader = sps.getU8()
    const nalType = nalHeader & 0x1f
    if (nalType !== 7) { // 7 = SPS
        throw new Error("Buffer does not start with an SPS NAL unit")
    }

    const profileIdc = sps.getU8()
    const constraintFlags = sps.getU8()
    const levelIdc = sps.getU8()

    const profileHex = numToHex(profileIdc)
    const constraintHex = numToHex(constraintFlags)
    const levelHex = numToHex(levelIdc)

    return {
        profileIdc,
        constraintFlags,
        levelIdc,
        avc1: `avc1.${profileHex}${constraintHex}${levelHex}`
    }
}

function h264MakeAvcC(sps: Uint8Array, pps: Uint8Array): Uint8Array {
    const size =
        7 +                 // header
        2 + sps.length +    // SPS
        1 +                 // PPS count
        2 + pps.length;     // PPS

    const data = new Uint8Array(size);
    let i = 0;

    data[i++] = 0x01;      // configurationVersion
    data[i++] = sps[1];   // AVCProfileIndication
    data[i++] = sps[2];   // profile_compatibility
    data[i++] = sps[3];   // AVCLevelIndication
    data[i++] = 0xFF;     // lengthSizeMinusOne = 3 (4 bytes)

    data[i++] = 0xE1;     // numOfSPS = 1
    data[i++] = sps.length >> 8;
    data[i++] = sps.length & 0xff;
    data.set(sps, i);
    i += sps.length;

    data[i++] = 0x01;     // numOfPPS = 1
    data[i++] = pps.length >> 8;
    data[i++] = pps.length & 0xff;
    data.set(pps, i);

    return data;
}

export class H264StreamVideoTranslator extends CodecStreamTranslator {
    constructor(logger?: Logger) {
        super(logger)
    }

    private hasDescription = false
    private pps: Uint8Array | null = null
    private sps: Uint8Array | null = null

    protected startProcessChunk(unit: VideoDecodeUnit): { shouldProcess: boolean } {
        return {
            shouldProcess: unit.type == "key" || this.hasDescription
        }
    }
    protected onChunkUnit(slice: Uint8Array): { include: boolean } {
        const nalType = h264NalType(slice[0])

        if (nalType == 7) {
            // Sps
            this.sps = new Uint8Array(slice)

            // Parse the sps and set the config.codec based on it
            const sps = h264ParseSps(new ByteBuffer(this.sps, false))

            const decodeConfig = this.decoderConfig ?? { codec: "" }
            decodeConfig.codec = sps.avc1

            return { include: false }
        } else if (nalType == 8) {
            // Pps
            this.pps = new Uint8Array(slice)

            return { include: false }
        }

        return { include: true }
    }
    protected endChunk(): { reconfigure: boolean } {
        if (!this.decoderConfig) {
            throw "UNREACHABLE"
        }

        if (this.pps && this.sps) {
            const description = h264MakeAvcC(this.sps, this.pps)
            this.sps = null
            this.pps = null

            this.decoderConfig.description = description

            console.debug("Reset decoder config using Sps and Pps")

            this.hasDescription = true

            return { reconfigure: true }
        } else if (!this.hasDescription) {
            this.logger?.debug("Received key frame without Sps and Pps", { type: "fatal" })
        }

        return { reconfigure: false }
    }
}

function h265NalType(header: number): number {
    return (header >> 1) & 0x3f;
}

function h265MakeHvcC(
    vps: Uint8Array,
    sps: Uint8Array,
    pps: Uint8Array
): Uint8Array {

    // Minimal hvcC with 3 arrays (VPS/SPS/PPS)
    const size =
        23 + // fixed header (minimal compliant)
        (3 * 3) + // array headers
        (2 + vps.length) +
        (2 + sps.length) +
        (2 + pps.length);

    const data = new Uint8Array(size);
    let i = 0;

    data[i++] = 1;        // configurationVersion

    // profile_tier_level
    data[i++] = (sps[1] >> 1) & 0x3f; // general_profile_space/tier/profile_idc
    data[i++] = 0;        // general_profile_compatibility_flags (part 1)
    data[i++] = 0;
    data[i++] = 0;
    data[i++] = 0;

    data[i++] = 0;        // general_constraint_indicator_flags (6 bytes)
    data[i++] = 0;
    data[i++] = 0;
    data[i++] = 0;
    data[i++] = 0;
    data[i++] = 0;

    data[i++] = sps[12];  // general_level_idc (heuristic, works in practice)

    data[i++] = 0xF0;     // min_spatial_segmentation_idc
    data[i++] = 0x00;

    data[i++] = 0xFC;     // parallelismType
    data[i++] = 0xFD;     // chromaFormat
    data[i++] = 0xF8;     // bitDepthLumaMinus8
    data[i++] = 0xF8;     // bitDepthChromaMinus8

    data[i++] = 0x00;     // avgFrameRate (2 bytes)
    data[i++] = 0x00;

    data[i++] = 0x0F;     // constantFrameRate + numTemporalLayers + lengthSizeMinusOne
    data[i++] = 3;        // numOfArrays

    // VPS
    data[i++] = 0x20;     // array_completeness=0, nal_unit_type=32
    data[i++] = 0;
    data[i++] = 1;
    data[i++] = vps.length >> 8;
    data[i++] = vps.length & 0xff;
    data.set(vps, i); i += vps.length;

    // SPS
    data[i++] = 0x21;     // nal_unit_type=33
    data[i++] = 0;
    data[i++] = 1;
    data[i++] = sps.length >> 8;
    data[i++] = sps.length & 0xff;
    data.set(sps, i); i += sps.length;

    // PPS
    data[i++] = 0x22;     // nal_unit_type=34
    data[i++] = 0;
    data[i++] = 1;
    data[i++] = pps.length >> 8;
    data[i++] = pps.length & 0xff;
    data.set(pps, i);

    return data;
}

export class H265StreamVideoTranslator extends CodecStreamTranslator {
    constructor(logger?: Logger) {
        super(logger)
    }

    private hasDescription = false
    private vps: Uint8Array | null = null
    private sps: Uint8Array | null = null
    private pps: Uint8Array | null = null

    protected startProcessChunk(unit: VideoDecodeUnit): { shouldProcess: boolean } {
        return {
            shouldProcess: unit.type === "key" || this.hasDescription
        }
    }

    protected onChunkUnit(slice: Uint8Array): { include: boolean } {
        const nalType = h265NalType(slice[0])

        if (nalType === 32) {
            this.vps = new Uint8Array(slice)
            return { include: false }
        }
        if (nalType === 33) {
            this.sps = new Uint8Array(slice)
            return { include: false }
        }
        if (nalType === 34) {
            this.pps = new Uint8Array(slice)
            return { include: false }
        }

        return { include: true }
    }

    protected endChunk(): { reconfigure: boolean } {
        if (!this.decoderConfig) {
            throw "UNREACHABLE"
        }

        if (this.vps && this.sps && this.pps) {
            this.decoderConfig.description =
                h265MakeHvcC(this.vps, this.sps, this.pps)

            this.vps = this.sps = this.pps = null
            this.hasDescription = true

            console.debug("Reset decoder config using VPS/SPS/PPS")
            return { reconfigure: true }
        }

        if (!this.hasDescription) {
            this.logger?.debug("Received key frame without VPS/SPS/PPS")
        }

        return { reconfigure: false }
    }
}
