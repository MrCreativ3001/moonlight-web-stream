import { Logger } from "../log.js";
import { globalObject, PipeInfo } from "../pipeline/index.js";
import { AudioContextBasePipe } from "./audio_context_base.js";
import { AudioPlayerSetup, TrackAudioPlayer } from "./index.js";

export class AudioContextTrackPipe extends AudioContextBasePipe {

    static async getInfo(): Promise<PipeInfo> {
        return {
            environmentSupported: "AudioContext" in globalObject() && "createMediaStreamSource" in AudioContext.prototype
        }
    }

    static readonly baseType = "audiotrack"
    static readonly type = "audionode"

    private destination: MediaStreamAudioDestinationNode | null = null
    private currentSource: AudioNode | null = null

    constructor(base: TrackAudioPlayer, logger?: Logger) {
        super(`audio_context_track -> ${base.implementationName}`, base, logger)

        this.addPipePassthrough()
    }

    setup(setup: AudioPlayerSetup) {
        const result = super.setup(setup)

        // TODO: implement the channels using constructor:
        // https://developer.mozilla.org/en-US/docs/Web/API/MediaStreamAudioDestinationNode/MediaStreamAudioDestinationNode#parameters
        // https://developer.mozilla.org/en-US/docs/Web/API/Web_Audio_API/Basic_concepts_behind_Web_Audio_API#audio_channels
        this.destination = this.getAudioContext().createMediaStreamDestination();

        (this.getBase() as TrackAudioPlayer).setTrack(this.destination.stream.getTracks()[0])

        if (this.currentSource) {
            this.currentSource.connect(this.destination)
        }

        return result
    }

    setSource(source: AudioNode): void {
        if (this.currentSource && this.destination) {
            this.currentSource.disconnect(this.destination)
        }

        this.currentSource = source

        if (this.destination) {
            source.connect(this.destination)
        }
    }

}