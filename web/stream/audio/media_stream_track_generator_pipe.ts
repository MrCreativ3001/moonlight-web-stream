import { globalObject } from "../../util"
import { Pipe, PipeInfo } from "../pipeline/index"
import { addPipePassthrough } from "../pipeline/pipes"
import { SampleAudioPlayer, TrackAudioPlayer } from "./index"

export class AudioMediaStreamTrackGeneratorPipe implements SampleAudioPlayer {

    static readonly baseType = "audiotrack"
    static readonly type = "audiosample"

    static async getInfo(): Promise<PipeInfo> {
        return {
            environmentSupported: "MediaStreamTrackGenerator" in globalObject()
        }
    }

    implementationName: string

    private base: TrackAudioPlayer

    private trackGenerator: MediaStreamTrackGenerator
    private writer: WritableStreamDefaultWriter<AudioData>

    constructor(base: TrackAudioPlayer) {
        this.implementationName = `audio_media_stream_track_generator -> ${base.implementationName}`
        this.base = base

        this.trackGenerator = new MediaStreamTrackGenerator({ kind: "audio" })
        this.writer = this.trackGenerator.writable.getWriter()

        addPipePassthrough(this)
    }

    private isFirstSample = true
    submitSample(sample: AudioData): void {
        if (this.isFirstSample) {
            this.isFirstSample = false

            this.base.setTrack(this.trackGenerator)
        }
        this.writer.write(sample)
    }

    getBase(): Pipe | null {
        return this.base
    }

}
