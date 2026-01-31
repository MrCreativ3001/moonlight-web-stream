import { globalObject } from "../../util.js"
import { Pipe, PipeInfo } from "../pipeline/index.js"
import { addPipePassthrough } from "../pipeline/pipes.js"
import { allVideoCodecs } from "../video.js"
import { CanvasVideoRendererOptions } from "./canvas.js"
import { CanvasRenderer, FrameVideoRenderer, VideoRendererSetup } from "./index.js"

export class CanvasFrameDrawPipe implements FrameVideoRenderer {

    static async getInfo(): Promise<PipeInfo> {
        // no link
        return {
            environmentSupported: "CanvasRenderingContext2D" in globalObject() || "OffscreenCanvasRenderingContext2D" in globalObject(),
            supportedVideoCodecs: allVideoCodecs()
        }
    }

    static readonly baseType = "canvas"
    static readonly type = "videoframe"

    readonly implementationName: string

    private base: CanvasRenderer

    private animationFrameRequest: number | null = null

    private currentFrame: VideoFrame | null = null
    private drawOnSubmit: boolean

    constructor(base: CanvasRenderer, _logger?: unknown, options?: unknown) {
        this.implementationName = `canvas_frame -> ${base.implementationName}`

        this.base = base

        const opts = options as CanvasVideoRendererOptions | undefined
        this.drawOnSubmit = opts?.drawOnSubmit ?? true

        addPipePassthrough(this)
    }

    async setup(setup: VideoRendererSetup): Promise<void> {
        if (this.animationFrameRequest == null) {
            this.animationFrameRequest = requestAnimationFrame(this.onAnimationFrame.bind(this))
        }

        if ("setup" in this.base && typeof this.base.setup == "function") {
            return this.base.setup(...arguments)
        }
    }

    cleanup(): void {
        if ("cleanup" in this.base && typeof this.base.cleanup == "function") {
            return this.base.cleanup(...arguments)
        }
    }

    submitFrame(frame: VideoFrame): void {
        this.currentFrame?.close()

        this.currentFrame = frame
        if (this.drawOnSubmit) {
            this.drawCurrentFrameIfReady()
        }
    }

    /** Draw currentFrame to canvas if context and frame are ready. Only updates size when dimensions change. */
    private drawCurrentFrameIfReady(): void {
        const frame = this.currentFrame
        const { context, error } = this.base.useCanvasContext("2d")
        if (!frame || error) {
            return
        }

        const w = frame.displayWidth
        const h = frame.displayHeight
        this.base.setCanvasSize(w, h)

        context.clearRect(0, 0, w, h)
        context.drawImage(frame, 0, 0, w, h)
        this.base.commitFrame()
    }

    private onAnimationFrame() {
        if (!this.drawOnSubmit) {
            this.drawCurrentFrameIfReady()
        }
        this.animationFrameRequest = requestAnimationFrame(this.onAnimationFrame.bind(this))
    }

    getBase(): Pipe | null {
        return this.base
    }
}