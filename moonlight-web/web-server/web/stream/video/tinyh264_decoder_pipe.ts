import createTinyH264Module from "../../libtinyh264/TinyH264.js";
import TinyH264Decoder from "../../libtinyh264/TinyH264Decoder.js";
import { Pipe, PipeInfo } from "../pipeline/index.js";
import { addPipePassthrough } from "../pipeline/pipes.js";
import { emptyVideoCodecs } from "../video.js";
import { DataVideoRenderer, RawFrameVideoRenderer, VideoDecodeUnit } from "./index.js";

// This implementation doesn't work:
// - sunshine only supports outputting H264 High profile but TinyH264 only supports Constained-Baseline and Baseline profiles which leads to errors :(
// - This is unused but maybe in the future it'll find it's use

/// A fallback for the normal VideoDecoder that only works in a secure context
export class TinyH264DecoderPipe implements DataVideoRenderer {
    static async getInfo(): Promise<PipeInfo> {
        const videoCodecs = emptyVideoCodecs()
        videoCodecs.H264 = true

        // no link
        return {
            environmentSupported: false,
            supportedVideoCodecs: videoCodecs
        }
    }

    static readonly baseType: string = "rawvideoframe"
    static readonly type: string = "videodata"

    readonly implementationName: string

    private isReady = false
    private onReady: Promise<void>

    private base: RawFrameVideoRenderer
    private decoder: TinyH264Decoder | null = null

    constructor(base: RawFrameVideoRenderer) {
        this.implementationName = `tinyh264_decoder -> ${base.implementationName}`
        this.base = base

        this.onReady = createTinyH264Module().then(module => {
            this.decoder = new TinyH264Decoder(module, this.onFrame.bind(this))
            this.isReady = true
        })

        addPipePassthrough(this)
    }

    async setup(): Promise<void> {
        if (this.isReady) {
            return
        }
        await this.onReady

        if ("setup" in this.base && typeof this.base.setup == "function") {
            return await this.base.setup(...arguments)
        }
    }

    submitDecodeUnit(unit: VideoDecodeUnit): void {
        this.decoder?.decode(unit.data)
    }

    private onFrame(
        buffer: Uint8Array,
        width: number,
        height: number,
    ) {
        this.base.submitRawFrame({
            buffer: buffer.buffer,
            width,
            height
        })
    }

    getBase(): Pipe | null {
        return this.base
    }
}