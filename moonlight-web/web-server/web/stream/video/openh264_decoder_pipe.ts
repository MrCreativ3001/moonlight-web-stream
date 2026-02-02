import { OpenH264Decoder } from "../../libopenh264/index.js";
import { Pipe, PipeInfo } from "../pipeline/index.js";
import { addPipePassthrough } from "../pipeline/pipes.js";
import { emptyVideoCodecs } from "../video.js";
import { DataVideoRenderer, Yuv420FrameVideoRenderer, VideoDecodeUnit } from "./index.js";

/// A fallback for the normal VideoDecoder that only works in a secure context
export class OpenH264DecoderPipe implements DataVideoRenderer {
    static async getInfo(): Promise<PipeInfo> {
        const videoCodecs = emptyVideoCodecs()
        videoCodecs.H264 = true

        let environmentSupported = false
        try {
            await import("../../libopenh264/decoder.js")
            environmentSupported = true
        } catch (e) { }

        // no link
        return {
            environmentSupported,
            supportedVideoCodecs: videoCodecs
        }
    }

    // TODO: this needs to be yuv420 frame
    static readonly baseType: string = "yuv420videoframe"
    static readonly type: string = "videodata"

    readonly implementationName: string

    private isReady = false
    private onReady: Promise<void>

    private base: Yuv420FrameVideoRenderer
    private decoder: OpenH264Decoder | null = null

    private errored = false

    constructor(base: Yuv420FrameVideoRenderer) {
        this.implementationName = `openh264_decoder -> ${base.implementationName}`
        this.base = base

        const createOpenH264Module = async () => {
            const module = await import("../../libopenh264/decoder.js")
            return await module.default()
        }

        this.onReady = createOpenH264Module().then(module => {
            this.decoder = new OpenH264Decoder(module, {
                onFrame: this.onFrame.bind(this)
            })
            this.isReady = true
        })

        addPipePassthrough(this)
    }

    async setup(): Promise<void> {
        if (!this.isReady) {
            await this.onReady
        }

        if ("setup" in this.base && typeof this.base.setup == "function") {
            return await this.base.setup(...arguments)
        }
    }

    submitDecodeUnit(unit: VideoDecodeUnit): void {
        if (this.errored) {
            return
        }

        try {
            this.decoder?.decode(new Uint8Array(unit.data))
        } catch (e) {
            console.error(e)
            this.errored = true
        }
    }

    private onFrame(
        buffers: Uint8Array[],
        stride: [number, number],
        width: number,
        height: number,
    ) {
        this.base.submitRawFrame({
            yPlane: buffers[0],
            uPlane: buffers[1],
            vPlane: buffers[2],
            yStride: stride[0],
            uvStride: stride[1],
            width,
            height,
        })
    }

    cleanup() {
        this.decoder?.destroy()

        if ("cleanup" in this.base && typeof this.base.cleanup == "function") {
            return this.base.cleanup(...arguments)
        }
    }

    getBase(): Pipe | null {
        return this.base
    }
}
