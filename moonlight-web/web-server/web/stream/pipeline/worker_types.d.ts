import { LogMessageInfo } from "../log.js"
import { VideoRendererSetup } from "../video/index.js"
import { PipeInfo, Pipeline } from "./index.js"

export type ToWorkerMessage =
    { checkSupport: Pipeline } |
    { createPipeline: Pipeline } |
    { input: WorkerMessage }

export type WorkerMessage =
    { call: "cleanup" } |
    { videoSetup: VideoRendererSetup } |
    // VideoFrame is a transferable object
    { videoFrame: VideoFrame } |
    // MediaStreamTrack is a transferable object when using the transfer parameter
    { track: MediaStreamTrack } |
    { data: ArrayBuffer } |
    // Canvas stuff
    { canvas: OffscreenCanvas }

export type ToMainMessage =
    { checkSupport: PipeInfo } |
    { log: string, info: LogMessageInfo } |
    { output: WorkerMessage }
