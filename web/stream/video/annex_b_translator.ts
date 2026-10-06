import { VideoFormats } from "../../uniffi/moonlight_common_bindings"
import { numToHex } from "../../util"
import { ByteBuffer } from "../buffer"
import { Logger } from "../log"
import { VideoDecodeUnit } from "./index"

// Translates annex b prefixed NALU's into AvCc 

// TODO: this should use the translator to get the codec instead of just statically defining them
export const VIDEO_DECODER_CODECS_OUT_OF_BAND: Record<keyof VideoFormats, string> = {
    "h264": "avc1.42E01E",
    "h264High8444": "avc1.640032",
    "h265": "hvc1.1.6.L93.B0",
    "h265Main10": "hvc1.2.4.L120.90",
    "h265Rext8444": "hvc1.6.6.L93.90",
    "h265Rext10444": "hvc1.6.10.L120.90",
    "av1Main8": "av01.0.04M.08",
    "av1Main10": "av01.0.04M.10",
    "av1High8444": "av01.0.08M.08",
    "av1High10444": "av01.0.08M.10"
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

    submitDecodeUnit(unit: VideoDecodeUnit): { configure: VideoDecoderConfig | null, chunk: Uint8Array | null, type?: EncodedVideoChunkType, error: false } | { error: true } {
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

type Av1Obu = {
    type: number
    header: Uint8Array
    payload: Uint8Array
}

const AV1_OBU_SEQUENCE_HEADER = 1
const AV1_OBU_TEMPORAL_DELIMITER = 2
const AV1_OBU_TILE_LIST = 8
const AV1_OBU_PADDING = 15

function readLeb128(data: Uint8Array, offset: number): { value: number, offset: number } | null {
    let value = 0

    for (let i = 0; i < 8; i++) {
        if (offset >= data.length) {
            return null
        }

        const b = data[offset++]
        value += (b & 0x7f) * 2 ** (i * 7)

        if (!Number.isSafeInteger(value)) {
            return null
        }

        if ((b & 0x80) == 0) {
            return { value, offset }
        }
    }

    return null
}

function writeLeb128(value: number): Uint8Array {
    const bytes = []

    do {
        let b = value % 0x80
        value = Math.floor(value / 0x80)
        if (value != 0) {
            b |= 0x80
        }
        bytes.push(b)
    } while (value != 0)

    return new Uint8Array(bytes)
}

function av1ObuType(header: number): number {
    return (header >> 3) & 0x0f
}

function isValidAv1ObuHeader(header: number): boolean {
    const forbidden = (header & 0x80) != 0
    const reserved = (header & 0x01) != 0
    const type = av1ObuType(header)

    return !forbidden && !reserved && type > 0
}

function parseAv1LowOverheadObus(data: Uint8Array): Av1Obu[] | null {
    const obus: Av1Obu[] = []
    let offset = 0

    while (offset < data.length) {
        const obuStart = offset
        const header = data[offset++]

        if (!isValidAv1ObuHeader(header)) {
            return null
        }

        const hasExtension = (header & 0x04) != 0
        const hasSize = (header & 0x02) != 0
        const headerEnd = offset + (hasExtension ? 1 : 0)

        if (headerEnd > data.length) {
            return null
        }

        offset = headerEnd

        if (!hasSize) {
            obus.push({
                type: av1ObuType(header),
                header: data.slice(obuStart, headerEnd),
                payload: data.slice(offset),
            })
            offset = data.length
            break
        }

        const size = readLeb128(data, offset)
        if (!size || size.offset + size.value > data.length) {
            return null
        }
        offset = size.offset
        obus.push({
            type: av1ObuType(header),
            header: data.slice(obuStart, headerEnd),
            payload: data.slice(offset, offset + size.value),
        })
        offset += size.value
    }

    return obus.length > 0 ? obus : null
}

function av1ShouldDropObu(obu: Av1Obu): boolean {
    return obu.type == AV1_OBU_TEMPORAL_DELIMITER ||
        obu.type == AV1_OBU_TILE_LIST ||
        obu.type == AV1_OBU_PADDING
}

function parseAv1AnnexBObus(data: Uint8Array): Av1Obu[] | null {
    const obus: Av1Obu[] = []
    let temporalOffset = 0

    while (temporalOffset < data.length) {
        const temporalUnit = readLeb128(data, temporalOffset)
        if (!temporalUnit || temporalUnit.value <= 0 || temporalUnit.offset + temporalUnit.value > data.length) {
            return null
        }

        let frameOffset = temporalUnit.offset
        const temporalEnd = temporalUnit.offset + temporalUnit.value

        while (frameOffset < temporalEnd) {
            const frameUnit = readLeb128(data, frameOffset)
            if (!frameUnit || frameUnit.value <= 0 || frameUnit.offset + frameUnit.value > temporalEnd) {
                return null
            }

            let obuOffset = frameUnit.offset
            const frameEnd = frameUnit.offset + frameUnit.value

            while (obuOffset < frameEnd) {
                const obuLength = readLeb128(data, obuOffset)
                if (!obuLength || obuLength.value <= 0 || obuLength.offset + obuLength.value > frameEnd) {
                    return null
                }

                const obuStart = obuLength.offset
                const header = data[obuStart]
                if (!isValidAv1ObuHeader(header)) {
                    return null
                }

                const hasExtension = (header & 0x04) != 0
                const headerEnd = obuStart + 1 + (hasExtension ? 1 : 0)
                if (headerEnd > obuLength.offset + obuLength.value) {
                    return null
                }

                obus.push({
                    type: av1ObuType(header),
                    header: data.slice(obuStart, headerEnd),
                    payload: data.slice(headerEnd, obuLength.offset + obuLength.value),
                })

                obuOffset = obuLength.offset + obuLength.value
            }

            frameOffset = frameEnd
        }

        temporalOffset = temporalEnd
    }

    return obus.length > 0 ? obus : null
}

function parseAv1SingleObu(data: Uint8Array): Av1Obu[] | null {
    if (data.length == 0 || !isValidAv1ObuHeader(data[0])) {
        return null
    }

    const header = data[0]
    const hasExtension = (header & 0x04) != 0
    const headerEnd = 1 + (hasExtension ? 1 : 0)
    if (headerEnd > data.length) {
        return null
    }

    return [{
        type: av1ObuType(header),
        header: data.slice(0, headerEnd),
        payload: data.slice(headerEnd),
    }]
}

function serializeAv1LowOverheadObus(obus: Av1Obu[]): Uint8Array {
    let size = 0

    for (const obu of obus) {
        size += obu.header.length + writeLeb128(obu.payload.length).length + obu.payload.length
    }

    const output = new Uint8Array(size)
    let offset = 0

    for (const obu of obus) {
        const header = new Uint8Array(obu.header)
        header[0] |= 0x02

        const payloadSize = writeLeb128(obu.payload.length)

        output.set(header, offset)
        offset += header.length
        output.set(payloadSize, offset)
        offset += payloadSize.length
        output.set(obu.payload, offset)
        offset += obu.payload.length
    }

    return output
}

export class Av1StreamVideoTranslator extends CodecStreamTranslator {
    private seenSequenceHeader = false
    private configured = false

    submitDecodeUnit(unit: VideoDecodeUnit): { configure: VideoDecoderConfig | null, chunk: Uint8Array | null, type?: EncodedVideoChunkType, error: false } | { error: true } {
        const data = new Uint8Array(unit.data)
        const lowOverheadObus = parseAv1LowOverheadObus(data)
        const annexBObus = lowOverheadObus ? null : parseAv1AnnexBObus(data)
        const singleObu = lowOverheadObus || annexBObus ? null : parseAv1SingleObu(data)
        const obus = lowOverheadObus ?? annexBObus ?? singleObu

        if (!obus) {
            this.logger?.debug(`Failed to parse AV1 decode unit (${data.length} bytes)`, { type: "fatal" })
            return { error: true }
        }

        const hasSequenceHeader = obus.some(obu => obu.type == AV1_OBU_SEQUENCE_HEADER)

        if (hasSequenceHeader) {
            this.seenSequenceHeader = true
        }

        if (!this.seenSequenceHeader && unit.type != "key") {
            return { configure: null, chunk: null, error: false }
        }

        const frameObus = obus.filter(obu => !av1ShouldDropObu(obu))
        if (frameObus.length == 0) {
            return { configure: null, chunk: null, error: false }
        }

        const chunk = serializeAv1LowOverheadObus(frameObus)

        const configure = this.configured ? null : this.decoderConfig
        this.configured = true

        return {
            configure,
            chunk,
            type: unit.type,
            error: false,
        }
    }

    protected startProcessChunk(_unit: VideoDecodeUnit): { shouldProcess: boolean } {
        return { shouldProcess: false }
    }
    protected onChunkUnit(_slice: Uint8Array): { include: boolean } {
        return { include: false }
    }
    protected endChunk(): { reconfigure: boolean } {
        return { reconfigure: false }
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

        if (nalType == 6) {
            // SEI, not needed, discard
            return { include: false }
        } else if (nalType == 12) {
            // Filler Data, discard
            return { include: false }
        } else if (nalType == 7) {
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
