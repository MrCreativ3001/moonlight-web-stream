import { globalObject, Pipe, PipeInfo } from "../pipeline/index.js";
import { addPipePassthrough } from "../pipeline/pipes.js";
import { allVideoCodecs } from "../video.js";
import { FrameVideoRenderer, TrackVideoRenderer } from "./index.js";

export class VideoTrackGeneratorPipe implements FrameVideoRenderer {
    static readonly baseType = "videotrack"
    static readonly type = "videoframe"

    static async getInfo(): Promise<PipeInfo> {
        // https://developer.mozilla.org/en-US/docs/Web/API/VideoTrackGenerator
        return {
            environmentSupported: "VideoTrackGenerator" in globalObject(),
            supportedVideoCodecs: allVideoCodecs()
        }
    }

    readonly implementationName: string

    private base: TrackVideoRenderer

    private trackGenerator: VideoTrackGenerator
    private writer: WritableStreamDefaultWriter<VideoFrame>

    constructor(base: TrackVideoRenderer) {
        this.implementationName = `video_track_generator -> ${base.implementationName}`
        this.base = base

        this.trackGenerator = new VideoTrackGenerator()
        this.writer = this.trackGenerator.writable.getWriter()

        addPipePassthrough(this)
    }

    private isFirstSample = true
    submitFrame(frame: VideoFrame): void {
        if (this.isFirstSample) {
            this.isFirstSample = false

            this.base.setTrack(this.trackGenerator.track)
        }
        this.writer.write(frame)
    }

    getBase(): Pipe | null {
        return this.base
    }
}