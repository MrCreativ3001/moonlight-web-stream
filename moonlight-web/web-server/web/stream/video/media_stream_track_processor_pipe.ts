import { globalObject, Pipe, PipeInfo } from "../pipeline/index.js";
import { addPipePassthrough } from "../pipeline/pipes.js";
import { allVideoCodecs } from "../video.js";
import { FrameVideoRenderer, TrackVideoRenderer, VideoRendererSetup } from "./index.js";

function wait(time: number): Promise<void> {
    return new Promise((resolve, _reject) => {
        setTimeout(resolve, time)
    })
}

export class VideoMediaStreamTrackProcessorPipe implements TrackVideoRenderer {

    static readonly baseType = "videoframe"
    static readonly type = "videotrack"

    static async getInfo(): Promise<PipeInfo> {
        // https://developer.mozilla.org/en-US/docs/Web/API/MediaStreamTrackProcessor
        return {
            environmentSupported: "MediaStreamTrackProcessor" in globalObject(),
            supportedVideoCodecs: allVideoCodecs()
        }
    }

    readonly implementationName: string

    private running: boolean = false
    private newProcessor: boolean = false
    private trackProcessor: MediaStreamTrackProcessor | null = null

    private base: FrameVideoRenderer

    constructor(base: FrameVideoRenderer) {
        this.implementationName = `media_stream_track_processor -> ${base.implementationName}`
        this.base = base

        addPipePassthrough(this)
    }

    setTrack(track: MediaStreamTrack): void {
        this.trackProcessor = new MediaStreamTrackProcessor({ track })
        this.newProcessor = true
    }

    private async readTrack() {
        let reader: ReadableStreamDefaultReader<VideoFrame> | null = null

        while (this.running) {
            if (!reader || this.newProcessor) {
                this.newProcessor = false

                if (this.trackProcessor?.readable.locked) {
                    // Shouldn't happen
                    throw "Canvas video track processor is locked"
                }

                const newReader = this.trackProcessor?.readable.getReader()
                if (newReader) {
                    reader = newReader
                }
                await wait(100)
                continue
            }

            // TODO: byob?
            const { done, value } = await reader.read()
            if (done) {
                console.error("Track Processor is done!")
                return
            }

            this.base.submitFrame(value)
        }
    }

    setup(setup: VideoRendererSetup) {
        this.running = true
        this.readTrack()

        if ("setup" in this.base && typeof this.base.setup == "function") {
            return this.base.setup(setup)
        }
    }
    cleanup() {
        this.running = false
        try {
            if (this.trackProcessor) {
                this.trackProcessor.readable.cancel()
            }
        } catch (e) {
            console.error(e)
        }
        this.trackProcessor = null

        if ("cleanup" in this.base && typeof this.base.cleanup == "function") {
            return this.base.cleanup(...arguments)
        }
    }

    getBase(): Pipe | null {
        return this.base
    }
}