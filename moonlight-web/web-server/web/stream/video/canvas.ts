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

    pollRequestIdr(): boolean {
        return false
    }

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

export type CanvasVideoRendererOptions = {
    /** When true, draw in submitFrame (low latency). When false, draw only on rAF (VSync-like, may reduce tearing). */
    drawOnSubmit?: boolean
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
    /** Set when we drew in submitFrame so onAnimationFrame can skip redundant draw. */
    private drewInSubmitFrame = false
    private drawOnSubmit: boolean
    private hdrEnabled: boolean = false

    constructor(_logger?: unknown, options?: unknown) {
        super("canvas")
        const opts = options as CanvasVideoRendererOptions | undefined
        this.drawOnSubmit = opts?.drawOnSubmit ?? true
    }
    
    setHdrMode(enabled: boolean): void {
        this.hdrEnabled = enabled
        if (this.context) {
            // Set HDR color space and transfer function
            if ("colorSpace" in this.context) {
                try {
                    (this.context as any).colorSpace = enabled ? "rec2020-pq" : "srgb"
                } catch (err) {
                    console.warn("Failed to set canvas colorSpace:", err)
                }
            }
        }
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
            const context = this.canvas.getContext("2d", {
                colorSpace: this.hdrEnabled ? "rec2020-pq" : "srgb"
            })
            if (context && context instanceof CanvasRenderingContext2D) {
                this.context = context
                // Apply HDR settings if already enabled
                if (this.hdrEnabled && "colorSpace" in context) {
                    try {
                        (context as any).colorSpace = "rec2020-pq"
                    } catch (err) {
                        console.warn("Failed to set canvas colorSpace:", err)
                    }
                }
            } else {
                throw "Failed to get 2d context from canvas"
            }
            if (this.currentFrame) {
                this.drawCurrentFrameIfReady()
            }
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
        if (!frame || !this.context) {
            return
        }
        const w = frame.displayWidth
        const h = frame.displayHeight
        if (this.canvas.width !== w || this.canvas.height !== h) {
            this.canvas.width = w
            this.canvas.height = h
        }
        this.context.clearRect(0, 0, w, h)
        this.context.drawImage(frame, 0, 0, w, h)
        this.drewInSubmitFrame = true
    }

    private onAnimationFrame() {
        if (!this.drawOnSubmit) {
            this.drawCurrentFrameIfReady()
        } else {
            this.drewInSubmitFrame = false
        }
        this.animationFrameRequest = requestAnimationFrame(this.onAnimationFrame.bind(this))
    }
}