import { Logger } from "../log.js";
import { globalObject, Pipe, PipeInfo } from "../pipeline/index.js";
import { addPipePassthrough } from "../pipeline/pipes.js";
import { AudioDecodeUnit, AudioPlayerSetup, DataAudioPlayer, SampleAudioPlayer } from "./index.js";

async function detectCodec(): Promise<boolean> {
    if (!("isConfigSupported" in AudioDecoder)) {
        // Opus is most likely supported
        return true
    }

    const supported = await AudioDecoder.isConfigSupported({
        codec: "opus",
        // normal Stereo configuration
        numberOfChannels: 2,
        sampleRate: 48000
    })

    return supported?.supported ?? false
}

export class AudioDecoderPipe implements DataAudioPlayer {

    static readonly baseType = "audiosample"
    static readonly type = "audiodata_opus"

    static async getInfo(): Promise<PipeInfo> {
        return {
            environmentSupported: "AudioDecoder" in globalObject() && await detectCodec(),
        }
    }

    readonly implementationName: string

    private logger: Logger | null = null

    private base: SampleAudioPlayer

    private errored = false
    private decoder: AudioDecoder

    constructor(base: SampleAudioPlayer, logger?: Logger) {
        this.implementationName = `audio_decoder -> ${base.implementationName}`
        this.logger = logger ?? null

        this.base = base

        this.decoder = new AudioDecoder({
            error: this.onError.bind(this),
            output: this.onOutput.bind(this)
        })

        addPipePassthrough(this)
    }

    private onError(error: any) {
        this.errored = true

        this.logger?.debug(`AudioDecoder has an error ${"toString" in error ? error.toString() : `${error}`}`, { type: "fatal" })
        console.error(error)
    }

    private onOutput(sample: AudioData) {
        this.base.submitSample(sample)
    }

    setup(setup: AudioPlayerSetup): void {
        if ("setup" in this.base && typeof this.base.setup == "function") {
            this.base.setup(setup)
        }

        this.decoder.configure({
            codec: "opus",
            numberOfChannels: setup.channels,
            sampleRate: setup.sampleRate
        })

    }

    private isFirstPacket = true

    decodeAndPlay(unit: AudioDecodeUnit): void {
        if (this.errored) {
            console.debug("Cannot submit audio decode unit because the stream errored")
            return
        }

        const chunk = new EncodedAudioChunk({
            type: this.isFirstPacket ? "key" : "delta",
            data: unit.data,
            timestamp: unit.timestampMicroseconds,
            duration: unit.durationMicroseconds,
        })
        this.isFirstPacket = false

        this.decoder.decode(chunk)
    }

    cleanup(): void {
        if ("cleanup" in this.base && typeof this.base.cleanup == "function") {
            this.base.cleanup()
        }

        this.decoder.close()
    }

    getBase(): Pipe | null {
        return this.base
    }

}