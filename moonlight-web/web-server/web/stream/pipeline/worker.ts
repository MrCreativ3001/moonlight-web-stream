import { LogMessageType } from "../../api_bindings.js"
import { Logger } from "../log.js"
import { andVideoCodecs } from "../video.js"
import { buildPipeline, getPipe, Pipe, PipeInfo, pipeName } from "./index.js"
import { WorkerOffscreenCanvasSendPipe } from "./worker_io.js"
import { WorkerReceiver } from "./worker_pipe.js"
import { ToMainMessage, ToWorkerMessage, WorkerMessage } from "./worker_types.js"

// Configure logger
const logger = new Logger()

function onLog(text: string, type: LogMessageType | null) {
    const message: ToMainMessage = {
        log: text,
        info: { type: type ?? undefined }
    }

    postMessage(message)
}

logger?.addInfoListener(onLog)

let pipelineErrored = false
let currentPipeline: WorkerReceiver | null = null
let canvasPipe: WorkerOffscreenCanvasSendPipe | null = null

class WorkerMessageSender implements WorkerReceiver {
    static readonly type: string = "workerinput"

    readonly implementationName: string = "worker_output"

    constructor(logger?: Logger) {
    }

    onWorkerMessage(output: WorkerMessage): void {
        const message: ToMainMessage = { output }

        postMessage(message)
    }

    getBase(): Pipe | null {
        return null
    }
}

async function onMessage(message: ToWorkerMessage) {
    if ("checkSupport" in message) {
        const pipeline = message.checkSupport

        const pipelineInfo: PipeInfo = {
            environmentSupported: true
        }

        for (const pipeRaw of pipeline.pipes) {
            const pipe = getPipe(pipeRaw)
            if (!pipe) {
                logger.debug(`Failed to find pipe "${pipeName(pipeRaw)}"`)
                pipelineInfo.environmentSupported = false
                break
            }
            const pipeInfo = await pipe.getInfo()

            if (!pipeInfo.environmentSupported) {
                pipelineInfo.environmentSupported = false
                break
            }

            if ("supportedVideoCodecs" in pipeInfo && pipeInfo.supportedVideoCodecs) {
                if (pipelineInfo.supportedVideoCodecs) {
                    pipelineInfo.supportedVideoCodecs = andVideoCodecs(pipelineInfo.supportedVideoCodecs, pipeInfo.supportedVideoCodecs)
                } else {
                    pipelineInfo.supportedVideoCodecs = pipeInfo.supportedVideoCodecs
                }
            }
        }

        const response: ToMainMessage = {
            checkSupport: pipelineInfo
        }
        postMessage(response)
    } else if ("createPipeline" in message) {
        const pipeline = message.createPipeline

        const newPipeline = buildPipeline(WorkerMessageSender, pipeline, logger)
        if (newPipeline && "onWorkerMessage" in newPipeline && typeof newPipeline.onWorkerMessage == "function") {
            currentPipeline = newPipeline as WorkerReceiver
        } else {
            logger.debug("Failed to build worker pipeline!", { type: "fatal" })
        }

        let base = newPipeline
        let newBase = newPipeline?.getBase()
        while ((newBase = base?.getBase()) && !(newBase instanceof WorkerMessageSender)) {
            base = newBase
        }

        if (base && base instanceof WorkerOffscreenCanvasSendPipe) {
            canvasPipe = base
            logger.debug("Found WorkerOffscreenCanvasSendPipe in worker pipeline")
        }
    } else if ("input" in message) {
        if (pipelineErrored) {
            return
        }

        if ("canvas" in message.input) {
            // Filter out the canvas, the last pipe needs that
            if (canvasPipe) {
                canvasPipe.setContext(message.input.canvas)
            }
        } else {
            if (currentPipeline) {
                currentPipeline.onWorkerMessage(message.input)
            } else {
                pipelineErrored = true
                logger.debug("Failed to submit worker pipe input because the worker wasn't assigned a pipeline!")
            }
        }

    }
}

onmessage = (event) => {
    const message = event.data as ToWorkerMessage
    onMessage(message)
}