// See https://github.com/MrCreativ3001/libopus-js

import type { MainModule } from "./libopus.js"

export const OPUS_OK = 0
export const OPUS_BAD_ARG = -1
export const OPUS_BUFFER_TOO_SMALL = -2
export const OPUS_INTERNAL_ERROR = -3
export const OPUS_INVALID_PACKET = -4
export const OPUS_UNIMPLEMENTED = -5
export const OPUS_INVALID_STATE = -6
export const OPUS_ALLOC_FAIL = -7

export class OpusError extends Error {
    static getMessageFromCode(errorCode: number): string {
        switch (errorCode) {
            case OPUS_OK:
                return "Ok"
            case OPUS_BAD_ARG:
                return "Bad Argument"
            case OPUS_BUFFER_TOO_SMALL:
                return "Buffer Too Small"
            case OPUS_INTERNAL_ERROR:
                return "Internal Error"
            case OPUS_INVALID_PACKET:
                return "Invalid Packet"
            case OPUS_UNIMPLEMENTED:
                return "Unimplemented Feature"
            case OPUS_INVALID_STATE:
                return "Invalid State"
            case OPUS_ALLOC_FAIL:
                return "Memory Allocation Failed"
            default:
                return `Unknown Opus error code: ${errorCode}`
        }
    }

    readonly errorCode: number

    constructor(errorCode: number) {
        super(OpusError.getMessageFromCode(errorCode))
        this.name = "OpusError"

        this.errorCode = errorCode
    }
}

// https://www.opus-codec.org/docs/opus_api-1.1.2/group__opus__multistream.html
export class OpusMultistreamDecoder {
    private module: MainModule
    private ptr: number = 0

    private channels: number = 0

    private inputBuffer: Buffer
    private outputBuffer: Buffer

    constructor(module: MainModule, sampleRate: number, channels: number, streams: number, coupled_channels: number, mappings: Array<number>) {
        if (mappings.length < channels) {
            throw new OpusError(OPUS_BAD_ARG)
        }

        this.module = module
        this.channels = channels

        const stackTop = module.stackSave()

        const mappingPtr = module.stackAlloc(mappings.length)
        for (let index = 0; index < channels; index++) {
            const mapping = mappings[index]

            if (mapping < 0 || mapping > 255) {
                throw new OpusError(OPUS_BAD_ARG)
            }
            module.HEAPU8[mappingPtr + index] = mapping
        }

        const errorPtr = module.stackAlloc(4)

        this.ptr = module._opus_multistream_decoder_create(sampleRate, channels, coupled_channels, streams, mappingPtr, errorPtr)

        module.stackRestore(stackTop)

        const error = this.module.getValue(errorPtr, "i32")
        if (error != OPUS_OK) {
            throw new OpusError(error)
        }

        this.inputBuffer = new Buffer(this.module)
        this.outputBuffer = new Buffer(this.module)
    }

    private checkPtr() {
        if (this.ptr == 0) {
            throw new OpusError(OPUS_INVALID_STATE)
        }
    }

    /**
     * Decode a multistream Opus packet with floating point output.
     * @param input Input payload. Use a NULL pointer to indicate packet loss.
     * @param output Output signal, with interleaved samples. This must contain room for frame_size*channels samples.
     * @param frameSize The number of samples per channel of available space in pcm. If this is less than the maximum packet duration (120 ms 5760 for 48kHz), this function will not be capable of decoding some packets. In the case of PLC (data==NULL) or FEC (decode_fec=1), then frame_size needs to be exactly the duration of audio that is missing, otherwise the decoder will not be in the optimal state to decode the next incoming packet. For the PLC and FEC cases, frame_size must be a multiple of 2.5 ms.
     * @param decodeFec Request that any in-band forward error correction data be decoded. If no such data is available, the frame is decoded as if it were lost.
     * @returns Number of samples decoded
     */
    decodeFloat(input: ArrayBuffer | null, output: Float32Array, frameSize: number, decodeFec: boolean): number {
        const outputSize = this.channels * frameSize * 4
        // 4 bytes per float
        if (output.byteLength < outputSize) {
            throw new OpusError(OPUS_BUFFER_TOO_SMALL)
        }

        this.checkPtr()

        if (input) {
            this.inputBuffer.ensureSize(input.byteLength)
            this.inputBuffer.writeBuffer(new Uint8Array(input))
        }

        this.outputBuffer.ensureSize(outputSize)

        const result = this.module._opus_multistream_decode_float(this.ptr, this.inputBuffer.getPtr(), input?.byteLength ?? 0, this.outputBuffer.getPtr(), frameSize, decodeFec ? 1 : 0)

        if (result < 0) {
            throw new OpusError(result)
        }

        const outputBuffer = this.outputBuffer.asBufferF32(0, this.channels * frameSize)
        output.set(outputBuffer)

        return result
    }

    destroy() {
        this.checkPtr()

        this.inputBuffer.free()
        this.outputBuffer.free()

        this.module._opus_multistream_decoder_destroy(this.ptr)
        this.ptr = 0
    }
}

class Buffer {
    private module: MainModule

    private length: number = 0
    private ptr: number = 0

    constructor(module: MainModule, byteSize?: number) {
        this.module = module

        this.ensureSize(byteSize ?? 0)
    }

    ensureSize(length: number) {
        if (length == 0) {
            return
        }

        if (!this.ptr) {
            this.ptr = this.module._malloc(length)
            this.length = length
        } else if (this.length < length) {
            this.free()

            this.ptr = this.module._malloc(length)
            this.length = length
        }
    }

    free() {
        if (this.ptr) {
            this.module._free(this.ptr)
            this.ptr = 0
            this.length = 0
        }
    }

    writeBuffer(input: Uint8Array) {
        if (input.byteLength == 0) {
            return
        }

        this.checkPtr()
        if (this.length < input.byteLength) {
            throw "BufferOutOfBounds"
        }

        this.module.writeArrayToMemory(input, this.ptr)
    }
    asBufferF32(offsetF32: number, lengthF32: number): Float32Array {
        if (lengthF32 <= 0) {
            return new Float32Array([])
        }

        this.checkPtr()
        if ((offsetF32 + lengthF32) * 4 < this.length || offsetF32 < 0 || lengthF32 < 0) {
            throw "BufferOutOfBounds"
        }

        const buffer = new Float32Array(this.module.HEAPF32.buffer, this.ptr + (offsetF32 * 4), lengthF32)

        return buffer
    }

    private checkPtr() {
        if (!this.ptr) {
            throw "NullPointer"
        }
    }

    getPtr(): number {
        return this.ptr
    }
    getLength(): number {
        return this.length
    }
}