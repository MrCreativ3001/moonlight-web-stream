import { globalObject } from "../../util"
import { Logger } from "../log"
import { PipeInfo } from "../pipeline/index"
import { AudioContextBasePipe } from "./audio_context_base"
import { AudioPlayer, AudioPlayerSetup } from "./index"

export class ContextDestinationNodeAudioPlayer extends AudioContextBasePipe implements AudioPlayer {
    static readonly pipeName = "ContextDestinationNodeAudioPlayer"

    static async getInfo(): Promise<PipeInfo> {
        return {
            environmentSupported: "AudioContext" in globalObject()
        }
    }

    static readonly type = "audionode"

    private destination: AudioNode | null = null
    private currentSource: AudioNode | null = null

    constructor(logger?: Logger) {
        super("node_audio_element", null, logger)

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
