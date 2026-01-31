import { globalObject } from "../../util.js"
import { Logger } from "../log.js"
import { Pipe, PipeInfo } from "../pipeline/index.js"
import { addPipePassthrough } from "../pipeline/pipes.js"
import { allVideoCodecs } from "../video.js"
import { CanvasRenderer, getStreamRectCorrected, UseCanvasResult, VideoRendererSetup } from "./index.js"

function getColorSpace(hdrEnabled: boolean): string {
    return hdrEnabled ? "rec2020-pq" : "srgb"
}

export class BaseCanvasVideoRenderer implements CanvasRenderer {

    static createMainCanvas(): HTMLCanvasElement {
        const canvas = document.createElement("canvas")

        canvas.classList.add("video-stream")

        return canvas
    }

    private div: HTMLDivElement | null = ("document" in globalObject()) ? globalObject().document.createElement("div") : null
    protected canvas: HTMLCanvasElement | OffscreenCanvas | null = null
    private isTransferred = false
    protected context: WebGLRenderingContext | WebGL2RenderingContext | (OffscreenCanvasRenderingContext2D | CanvasRenderingContext2D) | null = null

    private hdrEnabled: boolean = false
    private videoSize: [number, number] | null = null
    private options: CanvasVideoRendererOptions | null = null

    readonly implementationName: string

    constructor(implementationName: string, options?: CanvasVideoRendererOptions) {
        this.implementationName = implementationName
        this.options = options ?? null
    }

    setCanvas(canvas: HTMLCanvasElement | OffscreenCanvas, isTransferred?: boolean) {
        this.isTransferred = isTransferred ?? false
        this.canvas = canvas

        if (this.div && canvas instanceof HTMLCanvasElement) {
            this.div.appendChild(canvas)
        }
    }

    setHdrMode(enabled: boolean): void {
        this.hdrEnabled = enabled

        // Update existing context
        if (this.context) {
            // Set HDR color space and transfer function
            if ("colorSpace" in this.context) {
                try {
                    (this.context as any).colorSpace = getColorSpace(enabled)
                } catch (err) {
                    console.warn("Failed to set canvas colorSpace:", err)
                }
            }
        }
    }

    useCanvasContext(type: "webgl"): UseCanvasResult<WebGLRenderingContext>
    useCanvasContext(type: "webgl2"): UseCanvasResult<WebGL2RenderingContext>
    useCanvasContext(type: "2d"): UseCanvasResult<(OffscreenCanvasRenderingContext2D | CanvasRenderingContext2D)>
    useCanvasContext(type: "webgl" | "webgl2" | "2d"): UseCanvasResult<WebGLRenderingContext> | UseCanvasResult<WebGL2RenderingContext> | UseCanvasResult<(OffscreenCanvasRenderingContext2D | CanvasRenderingContext2D)> {
        if (!this.canvas) {
            return {
                context: null,
                error: "noCanvas",
            }
        }

        if (!this.context) {
            const options = {
                colorSpace: getColorSpace(this.hdrEnabled),
                // https://developer.mozilla.org/en-US/docs/Web/API/OffscreenCanvas/getContext#desynchronized
                desynchronized: this.options?.drawOnSubmit
            }

            if (type == "webgl") {
                this.context = this.canvas.getContext("webgl", options) as WebGLRenderingContext | null
            } else if (type == "webgl2") {
                this.context = this.canvas.getContext("webgl2", options) as WebGL2RenderingContext | null
            } else if (type == "2d") {
                this.context = this.canvas.getContext("2d", options) as (CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D) | null
            }

            if (!this.context) {
                return {
                    context: null,
                    error: "creationFailed",
                }
            }
        }

        if (type == "webgl" && (this.context instanceof WebGLRenderingContext || this.context instanceof WebGL2RenderingContext)) {
            return {
                error: null,
                context: this.context
            }
        } else if (type == "webgl2" && this.context instanceof WebGL2RenderingContext) {
            return {
                error: null,
                context: this.context
            }
        } else if (type == "2d" && (this.context instanceof OffscreenCanvasRenderingContext2D || this.context instanceof CanvasRenderingContext2D)) {
            return {
                error: null,
                context: this.context
            }
        }

        return {
            context: null,
            error: "otherContextInUse"
        }
    }
    setCanvasSize(width: number, height: number): void {
        if (this.canvas && !this.isTransferred) {
            this.canvas.width = width
            this.canvas.height = height
        }
    }

    commitFrame(): void {
        if (this.canvas && "commit" in this.canvas && typeof this.canvas.commit == "function") {
            // Signal finished, not supported in all browsers
            this.canvas.commit()
        }
    }

    async setup(setup: VideoRendererSetup): Promise<void> {
        this.videoSize = [setup.width, setup.height]

        this.setCanvasSize(setup.width, setup.height)
    }

    cleanup(): void { }

    pollRequestIdr(): boolean {
        return false
    }

    onUserInteraction(): void {
        // Nothing
    }

    mount(parent: HTMLElement): void {
        if (!this.div) {
            throw "Cannot mount div inside a worker!"
        }

        parent.appendChild(this.div)
    }
    unmount(parent: HTMLElement): void {
        if (!this.div) {
            throw "Cannot unmount div inside a worker!"
        }

        parent.removeChild(this.div)
    }

    getStreamRect(): DOMRect {
        if (!this.videoSize || !this.canvas) {
            return new DOMRect()
        }
        if (!(this.canvas instanceof HTMLCanvasElement)) {
            throw "Cannot get client bounding rect of OffscreenCanvas!"
        }

        return getStreamRectCorrected(this.canvas.getBoundingClientRect(), this.videoSize)
    }

    getBase(): Pipe | null {
        return null
    }
}

export type CanvasVideoRendererOptions = {
    /// When true:
    /// - enable desynchronized in the context creation options (lower latency)
    /// - draw in submitFrame (low latency)
    /// When false:
    /// - draw only on rAF (VSync-like, may reduce tearing).
    drawOnSubmit?: boolean
}

export class MainCanvasRenderer extends BaseCanvasVideoRenderer {

    static async getInfo(): Promise<PipeInfo> {
        // no link
        return {
            environmentSupported: "HTMLCanvasElement" in globalObject() && "CanvasRenderingContext2D" in globalObject(),
            supportedVideoCodecs: allVideoCodecs()
        }
    }

    static readonly type = "canvas"

    constructor(logger?: Logger, options?: unknown) {
        super("canvas", options as CanvasVideoRendererOptions | undefined)

        logger?.debug(`Applying canvas options: ${JSON.stringify(options)}`)

        this.setCanvas(BaseCanvasVideoRenderer.createMainCanvas())

        addPipePassthrough(this)
    }

    async setup(setup: VideoRendererSetup): Promise<void> {
        await super.setup(setup)
    }

    cleanup(): void {
        super.cleanup()
    }

    mount(parent: HTMLElement): void {
        super.mount(parent)
    }
}