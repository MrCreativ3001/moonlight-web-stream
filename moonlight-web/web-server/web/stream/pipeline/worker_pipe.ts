import { showErrorPopup } from "../../component/error.js";
import { Logger } from "../log.js";
import { VideoRendererSetup } from "../video/index.js";
import { OffscreenCanvasVideoRenderer } from "../video/offscreen_canvas.js";
import { globalObject, Pipe, PipeInfo, Pipeline, pipelineToString, PipeStatic } from "./index.js";
import { addPipePassthrough } from "./pipes.js";
import { ToMainMessage, ToWorkerMessage, WorkerMessage } from "./worker_types.js";

export function createPipelineWorker(): Worker | null {
    if (!("Worker" in globalObject())) {
        return null
    }

    return new Worker(new URL("worker.js", import.meta.url), { type: "module" })
}

export interface WorkerReceiver extends Pipe {
    onWorkerMessage(message: WorkerMessage, transferable?: Transferable[]): void
}

export class WorkerPipe implements WorkerReceiver {
    protected static async getInfoInternal(pipeline: Pipeline): Promise<PipeInfo> {
        const worker = createPipelineWorker()
        if (!worker) {
            return {
                environmentSupported: false
            }
        }

        const sendMessage: ToWorkerMessage = { checkSupport: pipeline }
        worker.postMessage(sendMessage)

        const info = await new Promise<PipeInfo>((resolve, reject) => {
            worker.onmessage = (event) => {
                const message = event.data as ToMainMessage

                if ("checkSupport" in message) {
                    resolve(message.checkSupport)
                } else if ("log" in message) {
                    // TODO: log this using a logger
                    throw message.log
                } else {
                    // TODO: log this
                    throw "Failed to get info about worker pipeline because it returned a wrong message"
                }
            }
            worker.onerror = reject
        })

        return info
    }

    readonly implementationName: string

    private logger: Logger | null

    private worker: Worker
    private base: WorkerReceiver
    private pipeline: Pipeline

    constructor(base: WorkerReceiver, pipeline: Pipeline, logger?: Logger) {
        this.implementationName = `worker_pipe [${pipelineToString(pipeline)}] -> ${base.implementationName}`
        this.logger = logger ?? null

        // TODO: check that the pipeline starts with output and ends with input
        this.base = base
        this.pipeline = pipeline

        const worker = createPipelineWorker()
        if (!worker) {
            throw "Failed to create worker pipeline: Workers not supported!"
        }
        this.worker = worker

        this.worker.onmessage = this.onReceiveWorkerMessage.bind(this)

        const message: ToWorkerMessage = {
            createPipeline: this.pipeline
        }
        this.worker.postMessage(message)

        addPipePassthrough(this)
    }

    onWorkerMessage(input: WorkerMessage, transfer?: Array<Transferable>): void {
        const message: ToWorkerMessage = { input }

        this.worker.postMessage(message, transfer ?? [])
    }

    private onReceiveWorkerMessage(event: MessageEvent) {
        const data: ToMainMessage = event.data

        if ("output" in data) {
            this.base.onWorkerMessage(data.output)
        } else if ("log" in data) {
            this.logger?.debug(data.log, data.info)
        }
    }

    mount() {
        let result
        if ("mount" in this.base && typeof this.base.mount == "function") {
            result = this.base.mount(...arguments)
        }

        // The OffscreenCanvas needs to transfer it's canvas into the worker, do that here
        if (this.base instanceof OffscreenCanvasVideoRenderer && this.base.offscreen) {
            this.logger?.debug("TESTING: TRANSFERRED")

            const canvas = this.base.offscreen
            this.onWorkerMessage({ canvas }, [canvas])

            this.base.transferred = true
            this.base.offscreen = null
        }

        return result
    }

    cleanup() {
        this.worker.terminate()

        if ("cleanup" in this.base && typeof this.base.cleanup == "function") {
            return this.base.cleanup(...arguments)
        }
    }

    getBase(): Pipe | null {
        return this.base
    }
}

export function workerPipe(name: string, pipeline: Pipeline): PipeStatic {
    class CustomWorkerPipe extends WorkerPipe {
        static async getInfo(): Promise<PipeInfo> {
            return await this.getInfoInternal(pipeline)
        }

        static readonly baseType = "workeroutput"
        static readonly type = "workerinput"

        constructor(base: any, logger?: Logger) {
            super(base, pipeline, logger)
        }
    }

    Object.defineProperty(CustomWorkerPipe, "name", { value: name })

    return CustomWorkerPipe
}