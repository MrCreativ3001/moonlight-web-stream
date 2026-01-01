import { globalObject, Pipe, PipeInfo } from "../pipeline/index.js"
import { allVideoCodecs } from "../video.js"
import { FrameVideoRenderer, getStreamRectCorrected, VideoRenderer, VideoRendererSetup } from "./index.js"

export abstract class BaseCanvasVideoRenderer implements VideoRenderer {

    protected canvas: HTMLCanvasElement = document.createElement("canvas")

    private videoSize: [number, number] | null = null

    readonly implementationName: string

    constructor(implementationName: string) {
        this.implementationName = implementationName

        this.canvas.classList.add("video-stream")
    }

    async setup(setup: VideoRendererSetup): Promise<void> {
        this.videoSize = [setup.width, setup.height]
    }

    cleanup(): void { }

    onUserInteraction(): void {
        // Nothing
    }

    mount(parent: HTMLElement): void {
        parent.appendChild(this.canvas)
    }
    unmount(parent: HTMLElement): void {
        parent.removeChild(this.canvas)
    }

    getStreamRect(): DOMRect {
        if (!this.videoSize) {
            return new DOMRect()
        }

        return getStreamRectCorrected(this.canvas.getBoundingClientRect(), this.videoSize)
    }

    getBase(): Pipe | null {
        return null
    }
}

export class CanvasVideoRenderer extends BaseCanvasVideoRenderer implements FrameVideoRenderer {

    static async getInfo(): Promise<PipeInfo> {
        // no link
        return {
            environmentSupported: "HTMLCanvasElement" in globalObject() && "CanvasRenderingContext2D" in globalObject(),
            supportedVideoCodecs: allVideoCodecs()
        }
    }

    static readonly type = "videoframe"

    private context: CanvasRenderingContext2D | null = null
    private animationFrameRequest: number | null = null

    private currentFrame: VideoFrame | null = null

    constructor() {
        super("canvas")
    }

    async setup(setup: VideoRendererSetup): Promise<void> {
        await super.setup(setup)

        if (this.animationFrameRequest == null) {
            this.animationFrameRequest = requestAnimationFrame(this.onAnimationFrame.bind(this))
        }
    }

    cleanup(): void {
        super.cleanup()

        this.context = null

        if (this.animationFrameRequest != null) {
            cancelAnimationFrame(this.animationFrameRequest)
            this.animationFrameRequest = null
        }
    }

    mount(parent: HTMLElement): void {
        super.mount(parent)

        if (!this.context) {
            const context = this.canvas.getContext("2d")
            if (context) {
                this.context = context
            } else {
                throw "Failed to get 2d context from canvas"
            }
        }
    }

    submitFrame(frame: VideoFrame): void {
        this.currentFrame?.close()

        this.currentFrame = frame
    }

    private onAnimationFrame() {
        const frame = this.currentFrame

        if (frame && this.context) {
            this.canvas.width = frame.displayWidth
            this.canvas.height = frame.displayHeight

            // Clear the canvas before drawing the new frame to prevent artifacts
            this.context.clearRect(0, 0, this.canvas.width, this.canvas.height)
            this.context.drawImage(frame, 0, 0, this.canvas.width, this.canvas.height)
        }

        this.animationFrameRequest = requestAnimationFrame(this.onAnimationFrame.bind(this))
    }
}