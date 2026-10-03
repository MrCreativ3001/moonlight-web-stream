import { globalObject } from "../../util"
import { Logger } from "../log"
import { OffscreenCanvasRenderer } from "../video/offscreen_canvas"
import { getPipe, Pipe, PipeInfo, Pipeline, pipelineToString, PipeStatic } from "./index"
import { addPipePassthrough } from "./pipes"
import { ToMainMessage, ToWorkerMessage, WorkerMessage } from "./worker_types"

export function createPipelineWorker(): Worker | null {
    if (!("Worker" in globalObject())) {
        return null
    }

    return new Worker(new URL("./worker", import.meta.url), { type: "module" })
}

export interface WorkerReceiver extends Pipe {
    onWorkerMessage(message: WorkerMessage, transferable?: Transferable[]): void
}

export class WorkerPipe implements WorkerReceiver {
    protected static async getInfoInternal(pipeline: Pipeline): Promise<PipeInfo> {
        return new Promise(resolve => {
            let worker: Worker | null = null
            let done = false
            const finish = (info: PipeInfo = { environmentSupported: false }) => {
                if (done) return
                done = true
                clearTimeout(timer)
                if (worker) {
                    worker.onmessage = null
                    worker.onerror = null
                    worker.onmessageerror = null
                    worker.terminate()
                }
                resolve(info && typeof info.environmentSupported == "boolean" ? info : { environmentSupported: false })
            }
            const timer = setTimeout(() => finish(), 2500)

            try {
                worker = createPipelineWorker()
                if (!worker) {
                    finish()
                    return
                }
                // Install handlers before sending, including for synchronous replies.
                worker.onmessage = event => {
                    const message = event.data as ToMainMessage
                    if (message && typeof message == "object" && "checkSupport" in message) {
                        finish(message.checkSupport)
                    }
                    // Log messages during startup are not support responses.
                }
                const onError = (event: Event) => {
                    event.preventDefault()
                    finish()
                }
                worker.onerror = onError
                worker.onmessageerror = onError
                const sendMessage: ToWorkerMessage = { checkSupport: pipeline }
                worker.postMessage(sendMessage)
            } catch {
                finish()
            }
        })
    }

    readonly implementationName: string

    private logger: Logger | null

    private worker: Worker
    private base: WorkerReceiver
    private pipeline: Pipeline

    constructor(base: WorkerReceiver, pipeline: Pipeline, logger?: Logger) {
        this.implementationName = `worker_pipe [${pipelineToString(pipeline)}] -> ${base.implementationName}`
        this.logger = logger ?? null

        if (getPipe(pipeline.pipes[0])?.type != "workeroutput") {
            logger?.debug("Worker Pipeline doesn't start with a workeroutput!", { type: "fatal" })
            throw "Worker Pipeline doesn't start with a workeroutput!"
        }
        if (getPipe(pipeline.pipes[pipeline.pipes.length - 1])?.baseType != "workerinput") {
            logger?.debug("Worker Pipeline doesn't end with a workerinput!", { type: "fatal" })
            throw "Worker Pipeline doesn't start with a workerinput!"
        }

        this.base = base
        this.pipeline = pipeline

        const worker = createPipelineWorker()
        if (!worker) {
            logger?.debug("Failed to create worker pipeline: Workers not supported!", { type: "fatal" })
            throw "Failed to create worker pipeline: Workers not supported!"
        }
        this.worker = worker

        this.worker.onmessage = this.onReceiveWorkerMessage.bind(this)

        const message: ToWorkerMessage = {
            createPipeline: this.pipeline
        }
        this.worker.postMessage(message)

        this.worker.onerror = (event) => {
            this.logger?.debug(`Worker errored because of: ${event.error}`)
        }

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
        if (this.base instanceof OffscreenCanvasRenderer && this.base.offscreen) {
            this.logger?.debug("Transferred OffscreenCanvas into worker")

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
        static readonly pipeName = name

        static async getInfo(): Promise<PipeInfo> {
            return await this.getInfoInternal(pipeline)
        }

        static readonly baseType = "workeroutput"
        static readonly type = "workerinput"

        constructor(base: any, logger?: Logger) {
            super(base, pipeline, logger)
        }
    }

    return CustomWorkerPipe
}
