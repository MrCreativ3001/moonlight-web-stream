import { globalObject } from "../../util.js"
import { Pipe, PipeInfo } from "../pipeline/index.js"
import { addPipePassthrough } from "../pipeline/pipes.js"
import { allVideoCodecs } from "../video.js"
import { CanvasVideoRendererOptions } from "./canvas.js"
import { CanvasRenderer, FrameVideoRenderer, VideoRendererSetup, RgbaFrameVideoRenderer, RgbaVideoFrame } from "./index.js"

abstract class BaseCanvasFrameDrawPipe implements Pipe {

    static async getInfo(): Promise<PipeInfo> {
        // no link
        return {
            environmentSupported: "CanvasRenderingContext2D" in globalObject() || "OffscreenCanvasRenderingContext2D" in globalObject(),
            supportedVideoCodecs: allVideoCodecs()
        }
    }

    static readonly baseType = "canvas"

    protected base: CanvasRenderer

    private animationFrameRequest: number | null = null

    private drawOnSubmit: boolean

    readonly implementationName

    constructor(implementationName: string, base: CanvasRenderer, _logger?: unknown, options?: unknown) {
        this.implementationName = implementationName
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

    protected onFrameSubmitted() {
        if (this.drawOnSubmit) {
            this.drawCurrentFrameIfReady()
        }
    }

    /** Draw currentFrame to canvas if context and frame are ready. Only updates size when dimensions change. */
    protected abstract drawCurrentFrameIfReady(): void

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

export class CanvasFrameDrawPipe extends BaseCanvasFrameDrawPipe implements FrameVideoRenderer {

    static async getInfo(): Promise<PipeInfo> {
        // no link
        return {
            environmentSupported: "CanvasRenderingContext2D" in globalObject() || "OffscreenCanvasRenderingContext2D" in globalObject(),
            supportedVideoCodecs: allVideoCodecs()
        }
    }

    static readonly type = "videoframe"

    private currentFrame: VideoFrame | null = null

    constructor(base: CanvasRenderer, _logger?: unknown, options?: unknown) {
        super(`canvas_frame -> ${base.implementationName}`, base, _logger, options)

        addPipePassthrough(this)
    }

    submitFrame(frame: VideoFrame): void {
        this.currentFrame?.close()

        this.currentFrame = frame
        this.onFrameSubmitted()
    }

    /** Draw currentFrame to canvas if context and frame are ready. Only updates size when dimensions change. */
    protected drawCurrentFrameIfReady(): void {
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
}

// TODO: implement yuv420 webgl renderer

export class CanvasRgbaFrameDrawPipe extends BaseCanvasFrameDrawPipe implements RgbaFrameVideoRenderer {

    static async getInfo(): Promise<PipeInfo> {
        // no link
        return {
            environmentSupported: "CanvasRenderingContext2D" in globalObject() || "OffscreenCanvasRenderingContext2D" in globalObject(),
            supportedVideoCodecs: allVideoCodecs()
        }
    }

    static readonly type = "rgbavideoframe"

    private currentFrame: ImageData | null = null

    constructor(base: CanvasRenderer, _logger?: unknown, options?: unknown) {
        super(`rgba_canvas_frame -> ${base.implementationName}`, base, _logger, options)

        addPipePassthrough(this)
    }

    submitRawFrame(frame: RgbaVideoFrame): void {
        this.currentFrame = new ImageData(frame.buffer, frame.width, frame.height)

        this.onFrameSubmitted()
    }

    /** Draw currentFrame to canvas if context and frame are ready. Only updates size when dimensions change. */
    protected drawCurrentFrameIfReady(): void {
        const frame = this.currentFrame
        const { context, error } = this.base.useCanvasContext("2d")
        if (!frame || error) {
            return
        }

        const w = frame.width
        const h = frame.height
        this.base.setCanvasSize(w, h)

        context.clearRect(0, 0, w, h)
        context.putImageData(frame, 0, 0)

        this.base.commitFrame()
    }
}