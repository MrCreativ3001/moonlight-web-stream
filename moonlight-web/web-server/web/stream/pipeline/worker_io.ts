import { Logger } from "../log.js";
import { FrameVideoRenderer, TrackVideoRenderer, VideoRendererSetup } from "../video/index.js";
import { globalObject, Pipe, PipeInfo } from "./index.js";
import { addPipePassthrough, DataPipe } from "./pipes.js";
import { WorkerPipe, WorkerReceiver } from "./worker_pipe.js";
import { WorkerMessage } from "./worker_types.js";

class WorkerReceiverPipe implements WorkerReceiver, DataPipe, FrameVideoRenderer, TrackVideoRenderer {
    static async getInfo(): Promise<PipeInfo> {
        return {
            environmentSupported: true
        }
    }

    static readonly type = "workeroutput"

    readonly implementationName: string

    private logger: Logger | null = null
    private base: Pipe

    constructor(base: Pipe, logger?: Logger) {
        this.implementationName = `worker_recv -> ${base.implementationName}`

        this.logger = logger ?? null
        this.base = base

        addPipePassthrough(this, ["setup", "cleanup", "submitFrame", "submitPacket", "setTrack"])
    }

    onWorkerMessage(message: WorkerMessage): void {
        if ("call" in message && message.call == "cleanup") {
            this.cleanup()
        } else if ("videoSetup" in message) {
            this.setup(message.videoSetup)
        } else if ("videoFrame" in message) {
            this.submitFrame(message.videoFrame)
        } else if ("data" in message) {
            this.submitPacket(message.data)
        } else if ("track" in message) {
            this.setTrack(message.track)
        }
    }

    getBase(): Pipe {
        return this.base
    }

    // -- Only definition look addPipePassthrough
    setup(_setup: VideoRendererSetup): void { }
    cleanup(): void { }
    submitFrame(_frame: VideoFrame): void { }
    submitPacket(_buffer: ArrayBuffer): void { }
    setTrack(_track: MediaStreamTrack): void { }
}
export class WorkerVideoFrameReceivePipe extends WorkerReceiverPipe {
    static readonly baseType = "videoframe"
}
export class WorkerDataReceivePipe extends WorkerReceiverPipe {
    static readonly baseType = "data"
}
export class WorkerVideoTrackReceivePipe extends WorkerReceiverPipe {
    static readonly baseType = "videotrack"
}

class WorkerSenderPipe implements DataPipe, FrameVideoRenderer, TrackVideoRenderer {
    static async getInfo(): Promise<PipeInfo> {
        return {
            environmentSupported: true
        }
    }

    static readonly baseType = "workerinput"

    readonly implementationName: string

    protected logger: Logger | null = null
    private base: WorkerPipe

    constructor(base: WorkerPipe, logger?: Logger) {
        this.implementationName = `worker_send -> ${base.implementationName}`
        this.logger = logger ?? null
        this.base = base

        addPipePassthrough(this)
    }

    getBase(): WorkerPipe {
        return this.base
    }

    setup(setup: VideoRendererSetup) {
        this.getBase().onWorkerMessage({ videoSetup: setup })
    }

    submitFrame(videoFrame: VideoFrame): void {
        this.getBase().onWorkerMessage({ videoFrame }, [videoFrame])
    }
    submitPacket(data: ArrayBuffer): void {
        // we don't know if we own this data, so we cannot transfer
        this.getBase().onWorkerMessage({ data })
    }
    setTrack(track: MediaStreamTrack): void {
        this.getBase().onWorkerMessage({ track }, [track])
    }
}

export class WorkerVideoFrameSendPipe extends WorkerSenderPipe {
    static readonly type = "videoframe"
}
export class WorkerDataSendPipe extends WorkerSenderPipe {
    static readonly type = "data"
}
export class WorkerVideoTrackSendPipe extends WorkerSenderPipe {
    static readonly type = "videotrack"
}


export class WorkerOffscreenCanvasSendPipe extends WorkerSenderPipe implements FrameVideoRenderer {

    static async getInfo(): Promise<PipeInfo> {
        return {
            environmentSupported: "OffscreenCanvasRenderingContext2D" in globalObject()
        }
    }

    static readonly baseType = "workerinput"
    static readonly type = "videoframe"

    implementationName: string = "offscreen_canvas_send"

    private canvas: OffscreenCanvas | null = null
    private context: OffscreenCanvasRenderingContext2D | null = null

    constructor(base: WorkerPipe, logger?: Logger) {
        super(base, logger)

        addPipePassthrough(this)
    }

    setContext(canvas: OffscreenCanvas) {
        // This is called from the WorkerPipe
        this.canvas = canvas
        this.context = canvas.getContext("2d")

        if (!this.context) {
            this.logger?.debug("Failed to get OffscreenCanvasContext2D", { type: "fatal" })
        }
    }

    override submitFrame(frame: VideoFrame): void {
        if (this.canvas && this.context) {
            this.canvas.width = frame.displayWidth
            this.canvas.height = frame.displayHeight

            this.context.clearRect(0, 0, frame.displayWidth, frame.displayHeight)
            this.context.drawImage(frame, 0, 0, frame.displayWidth, frame.displayHeight)

            if ("commit" in this.canvas && typeof this.canvas.commit == "function") {
                // Signal finished, not supported in all browsers
                this.canvas.commit()
            }
        }

        frame.close()
    }
}