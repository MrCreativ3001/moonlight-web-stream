
import { Logger } from "../log.js";
import { Pipe } from "../pipeline/index.js";
import { AudioPlayerSetup, NodeAudioPlayer } from "./index.js";

export abstract class AudioContextBasePipe implements NodeAudioPlayer {

    readonly implementationName: string

    private logger: Logger | null = null

    private base: Pipe | null
    // TODO: include baseLatency and outputLatency in stats
    private audioContext: AudioContext | null = null

    constructor(implementationName: string, base: Pipe | null, logger?: Logger) {
        this.logger = logger ?? null

        this.implementationName = implementationName
        this.base = base
    }

    setup(setup: AudioPlayerSetup) {
        try {
            this.audioContext = new AudioContext({
                latencyHint: "interactive",
                sampleRate: setup.sampleRate
            })
        } catch (e: any) {
            this.logger?.debug(`Failed to setup audio node with latency hint "interactive". Trying to setup without latency hint. ${"toString" in e && typeof e.toString == "function" ? e.toString() : e}`)
        }

        if (!this.audioContext) {
            this.audioContext = new AudioContext({
                sampleRate: setup.sampleRate
            })
        }

        if (this.base && "setup" in this.base && typeof this.base.setup == "function") {
            return this.base.setup(...arguments)
        }
    }
    cleanup(): void {
        this.audioContext?.close()
    }

    abstract setSource(source: AudioNode): void

    getAudioContext(): AudioContext {
        if (!this.audioContext) {
            this.logger?.debug("Failed to get audio context", { type: "fatal" })
            throw "Failed to get audio context."
        }
        return this.audioContext
    }

    getBase(): Pipe | null {
        return this.base
    }

    // -- Only definition look addPipePassthrough
    onUserInteraction(): void { }
    mount(_parent: HTMLElement): void { }
    unmount(_parent: HTMLElement): void { }
}