
export const U8_MAX = 255

export const U16_MAX = 65535
export const I16_MAX = 32767

export class ByteBuffer {
    private position: number = 0
    private limit: number = 0
    private littleEndian: boolean
    private buffer: Uint8Array

    constructor(length?: number, littleEndian?: boolean)
    constructor(buffer: Uint8Array, littleEndian?: boolean)
    constructor(value?: number | Uint8Array, littleEndian?: boolean) {
        this.littleEndian = littleEndian ?? false

        if (value instanceof Uint8Array) {
            this.buffer = value
            this.limit = value.length
        } else {
            this.buffer = new Uint8Array(value ?? 0)
            this.limit = value ?? 0
        }
    }

    private bytesUsed(amount: number, reading: boolean) {
        this.position += amount
        if (reading && this.position > this.limit) {
            throw "failed to read over the limit"
        }
    }
    private useDataView(length: number, reading: boolean, index?: number): DataView {
        let position = this.position
        if (index != null) {
            position = index
        }

        const maxPosition = position + length
        if (maxPosition > this.buffer.length) {
            throw "failed to write over the capacity"
        }

        if (maxPosition > this.limit) {
            this.limit = maxPosition
        }

        if (index == null) {
            this.bytesUsed(length, reading)
        }
        return new DataView(this.buffer.buffer, position, length)
    }

    putU8Array(data: Uint8Array, index?: number) {
        let position = this.position
        if (index) {
            position = index
        }

        const maxPosition = position + data.length
        if (maxPosition > this.buffer.length) {
            throw "failed to write over the capacity"
        }

        if (maxPosition > this.limit) {
            this.limit = maxPosition
        }

        this.buffer.set(data, position)

        if (index == null) {
            this.bytesUsed(data.length, false)
        }
    }
    putU16Array(data: Uint16Array, index?: number) {
        let position = index ?? this.position

        const maxPosition = position + data.length * 2
        if (maxPosition > this.buffer.length) {
            throw "failed to write over the capacity"
        }

        for (let i = 0; i < data.length; i++) {
            const view = new DataView(this.buffer.buffer, position + i * 2, 2)
            view.setUint16(0, data[i], this.littleEndian)
        }

        if (maxPosition > this.limit) {
            this.limit = maxPosition
        }

        if (index == null) {
            this.bytesUsed(data.length * 2, false)
        }
    }

    putU32Array(data: Uint32Array, index?: number) {
        let position = index ?? this.position

        const maxPosition = position + data.length * 4
        if (maxPosition > this.buffer.length) {
            throw "failed to write over the capacity"
        }

        for (let i = 0; i < data.length; i++) {
            const view = new DataView(this.buffer.buffer, position + i * 4, 4)
            view.setUint32(0, data[i], this.littleEndian)
        }

        if (maxPosition > this.limit) {
            this.limit = maxPosition
        }

        if (index == null) {
            this.bytesUsed(data.length * 4, false)
        }
    }


    putU8(data: number, index?: number) {
        const view = this.useDataView(1, false, index)
        view.setUint8(0, data)
    }
    putBool(data: boolean) {
        this.putU8(data ? 1 : 0)
    }

    putI8(data: number, index?: number) {
        const view = this.useDataView(1, false, index)
        view.setInt8(0, data)
    }

    putU16(data: number, index?: number) {
        const view = this.useDataView(2, false, index)
        view.setUint16(0, data, this.littleEndian)
    }
    putI16(data: number, index?: number) {
        const view = this.useDataView(2, false, index)
        view.setInt16(0, data, this.littleEndian)
    }

    putU32(data: number, index?: number) {
        const view = this.useDataView(4, false, index)
        view.setUint32(0, data, this.littleEndian)
    }
    putI32(data: number, index?: number) {
        const view = this.useDataView(4, false, index)
        view.setInt32(0, data)
    }

    putU64(data: number, index?: number) {
        const hi = Math.floor(data / 0x100000000)   // upper 32 bits
        const lo = data >>> 0                       // lower 32 bits

        const view = this.useDataView(8, false, index)

        if (this.littleEndian) {
            view.setUint32(0, lo, true)
            view.setUint32(4, hi, true)
        } else {
            view.setUint32(0, hi, false)
            view.setUint32(4, lo, false)
        }

        this.bytesUsed(8, false)
    }

    putUtf8Raw(text: string) {
        const encoder = new TextEncoder()
        const result = encoder.encodeInto(text, this.buffer.subarray(this.position))

        this.bytesUsed(result.written, false)
        if (result.read != text.length) {
            throw "failed to put utf8 text"
        }
    }

    putF32(data: number) {
        const view = new DataView(this.buffer.buffer)
        view.setFloat32(this.position, data, this.littleEndian)
        this.bytesUsed(4, false)
    }

    get(buffer: Uint8Array, offset: number, length: number) {
        buffer.set(this.buffer.subarray(this.position, this.position + length), offset)
        this.bytesUsed(length, true)
    }

    getU8(): number {
        const view = new DataView(this.buffer.buffer)
        const byte = view.getUint8(this.position)
        this.bytesUsed(1, true)
        return byte
    }
    getU16(): number {
        const view = new DataView(this.buffer.buffer)
        const byte = view.getUint16(this.position)
        this.bytesUsed(2, true)
        return byte
    }
    getU32(): number {
        const view = new DataView(this.buffer.buffer)
        const byte = view.getUint32(this.position)
        this.bytesUsed(4, true)
        return byte
    }
    getBool(): boolean {
        return this.getU8() != 0
    }

    getUtf8Raw(length: number): string {
        const buffer = new Uint8Array(length)
        this.get(buffer, 0, length)

        const decoder = new TextDecoder()
        return decoder.decode(buffer)
    }

    reset() {
        this.position = 0
        this.limit = 0
    }
    flip() {
        this.limit = this.position
        this.position = 0
    }
    isLittleEndian() {
        return this.littleEndian
    }
    getPosition() {
        return this.position
    }

    getRemainingBuffer(): Uint8Array {
        return this.buffer.slice(this.position, this.limit)
    }
}

export const BIG_BUFFER = new ByteBuffer(1000000)