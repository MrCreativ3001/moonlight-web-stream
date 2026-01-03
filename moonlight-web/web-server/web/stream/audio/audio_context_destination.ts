import { Logger } from "../log.js";
import { globalObject, PipeInfo } from "../pipeline/index.js";
import { AudioContextBasePipe } from "./audio_context_base.js";
import { AudioPlayer, AudioPlayerSetup } from "./index.js";

export class ContextDestinationNodeAudioPlayer extends AudioContextBasePipe implements AudioPlayer {

    static async getInfo(): Promise<PipeInfo> {
        return {
            environmentSupported: "AudioContext" in globalObject()
        }
    }

    static readonly type = "audionode"

    private destination: AudioNode | null = null
    private currentSource: AudioNode | null = null

    constructor(logger?: Logger) {
        super("node_audio_element (player)", null, logger)

        this.addPipePassthrough()
    }

    setup(setup: AudioPlayerSetup) {
        const result = super.setup(setup)

        this.destination = this.getAudioContext().destination;

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

    mount(_parent: HTMLElement): void { }
    unmount(_parent: HTMLElement): void { }

}