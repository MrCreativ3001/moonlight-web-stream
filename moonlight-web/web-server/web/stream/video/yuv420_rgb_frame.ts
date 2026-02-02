import { Pipe, PipeInfo } from "../pipeline/index.js"
import { addPipePassthrough } from "../pipeline/pipes.js"
import { RgbaFrameVideoRenderer, Yuv420FrameVideoRenderer, Yuv420VideoFrame } from "./index.js"

export class Yuv420ToRgbaFramePipe implements Yuv420FrameVideoRenderer {
    static async getInfo(): Promise<PipeInfo> {
        // no link
        return {
            environmentSupported: true,
        }
    }

    static readonly baseType: string = "rgbavideoframe"
    static readonly type: string = "yuv420videoframe"

    readonly implementationName: string

    private base: RgbaFrameVideoRenderer
    private buffer = new Uint8ClampedArray(0)

    constructor(base: RgbaFrameVideoRenderer) {
        this.base = base
        this.implementationName = `yuv420_to_rgb_frame -> ${this.base.implementationName}`

        addPipePassthrough(this)
    }

    submitRawFrame(frame: Yuv420VideoFrame): void {
        const bufferSize = frame.width * frame.height * 4

        if (this.buffer.length < bufferSize) {
            this.buffer = new Uint8ClampedArray(bufferSize)
        }

        let rgbaIndex = 0

        for (let y = 0; y < frame.height; y++) {
            const yRow = y * frame.yStride
            const uvRow = (y >> 1) * frame.uvStride

            for (let x = 0; x < frame.width; x++) {
                const yValue = frame.yPlane[yRow + x]

                const uvIndex = uvRow + (x >> 1)
                const uValue = frame.uPlane[uvIndex] - 128
                const vValue = frame.vPlane[uvIndex] - 128

                // BT.601 conversion
                let r = yValue + 1.402 * vValue
                let g = yValue - 0.344136 * uValue - 0.714136 * vValue
                let b = yValue + 1.772 * uValue

                this.buffer[rgbaIndex++] = Math.max(0, Math.min(255, r))
                this.buffer[rgbaIndex++] = Math.max(0, Math.min(255, g))
                this.buffer[rgbaIndex++] = Math.max(0, Math.min(255, b))
                this.buffer[rgbaIndex++] = 255
            }
        }

        this.base.submitRawFrame({
            buffer: this.buffer.subarray(0, bufferSize),
            width: frame.width,
            height: frame.height,
        })
    }

    getBase(): Pipe | null {
        return this.base
    }
}