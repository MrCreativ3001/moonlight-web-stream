import { globalObject } from "../../util.js";
import { PipeInfo } from "../pipeline/index.js";
import { WorkerReceiver } from "../pipeline/worker_pipe.js";
import { WorkerMessage } from "../pipeline/worker_types.js";
import { BaseCanvasVideoRenderer } from "./canvas.js";
import { VideoRendererSetup } from "./index.js";

export class OffscreenCanvasRenderer extends BaseCanvasVideoRenderer implements WorkerReceiver {

    static async getInfo(): Promise<PipeInfo> {
        return {
            environmentSupported: "HTMLCanvasElement" in globalObject() && "transferControlToOffscreen" in HTMLCanvasElement.prototype
        }
    }

    static readonly type = "workeroutput"

    private mainCanvas = BaseCanvasVideoRenderer.createMainCanvas()

    transferred: boolean = false
    offscreen: OffscreenCanvas | null = null

    constructor() {
        super("offscreen_canvas", {
            // This won't make any difference because the rendering is done offscreen
            // drawOnSubmit: true
        })

        this.setCanvas(this.mainCanvas, true)
    }

    async setup(setup: VideoRendererSetup): Promise<void> {
        await super.setup(setup)
    }

    mount(parent: HTMLElement): void {
        super.mount(parent)

        if (!this.offscreen && !this.transferred) {
            this.offscreen = this.mainCanvas.transferControlToOffscreen()

            // The transfer happens in the WorkerPipe
        }
    }

    onWorkerMessage(message: WorkerMessage): void {
        if ("videoSetup" in message) {
            this.setup(message.videoSetup)
        }
    }

}
