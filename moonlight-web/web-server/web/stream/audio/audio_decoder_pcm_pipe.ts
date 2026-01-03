import { OpusMultistreamDecoder } from "../../libopus/index.js";
import loadOpus from "../../libopus/libopus.js";
import { MainModule as OpusModule } from "../../libopus/libopus.js";
import { Logger } from "../log.js";
import { Pipe, PipeInfo } from "../pipeline/index.js";
import { addPipePassthrough } from "../pipeline/pipes.js";
import { AudioDecodeUnit, AudioPlayerSetup, DataAudioPlayer, PcmAudioPlayer } from "./index.js";

export class AudioDecoderPcmPipe implements DataAudioPlayer {

    static async getInfo(): Promise<PipeInfo> {
        return {
            // TODO: what does this require?
            environmentSupported: true
        }
    }

    static readonly baseType = "audiopcm"
    static readonly type = "audiodata"

    readonly implementationName: string

    private logger: Logger | null = null

    private base: PcmAudioPlayer

    private errored: boolean = false

    private decoder: OpusMultistreamDecoder | null = null

    private opusModule: OpusModule | null = null
    private setupData: AudioPlayerSetup | null = null

    private buffer: Float32Array = new Float32Array([])

    constructor(base: PcmAudioPlayer, logger?: Logger) {
        loadOpus().then(module => this.opusModule = module)

        this.logger = logger ?? null

        this.implementationName = `audio_decode_pcm -> ${base.implementationName}`
        this.base = base

        addPipePassthrough(this)
    }

    setup(setup: AudioPlayerSetup) {
        this.setupData = setup

        if ("setup" in this.base && typeof this.base.setup == "function") {
            return this.base.setup(...arguments)
        }
    }

    decodeAndPlay(unit: AudioDecodeUnit): void {
        if (this.errored) {
            return
        }

        if (!this.setupData) {
            this.errored = true
            this.logger?.debug("Failed to play audio sample because audio player is not initialized")
            return
        }

        if (!this.decoder) {
            if (!this.opusModule) {
                return
            }

            try {
                this.decoder = new OpusMultistreamDecoder(this.opusModule, this.setupData.sampleRate, this.setupData.channels, this.setupData.streams, this.setupData.coupledStreams, this.setupData.mapping)
            } catch (e: any) {
                this.errored = true

                const message = `Failed to initialize opus decoder: ${"toString" in e && typeof e.toString == "function" ? e.toString() : e}`
                this.logger?.debug(message, { type: "informError" })

                return
            }
            this.buffer = new Float32Array(this.setupData.samplesPerFrame * this.setupData.channels)
        }

        // -- Decode samples
        let samplesDecoded
        try {
            samplesDecoded = this.decoder.decodeFloat(unit.data, this.buffer, this.setupData.samplesPerFrame, false)
        } catch (e: any) {
            this.errored = true

            const message = `Failed to decode audio sample: ${"toString" in e && typeof e.toString == "function" ? e.toString() : e}`
            this.logger?.debug(message, { type: "informError" })

            return
        }

        const channels = this.setupData.channels
        // TODO: have multiple buffers / caches for that
        const channelData: Float32Array[] = new Array(channels)

        // -- De-interleave interleaved PCM

        // Initialize channel arrays
        for (let channelIndex = 0; channelIndex < channels; channelIndex++) {
            channelData[channelIndex] = new Float32Array(samplesDecoded)
            for (let sample = 0; sample < samplesDecoded; sample++) {
                channelData[channelIndex][sample] = this.buffer[(sample * channels) + channelIndex]
            }
        }

        // -- Pass data to next decoder
        this.base.playPcm({
            durationMicroseconds: unit.durationMicroseconds,
            timestampMicroseconds: unit.timestampMicroseconds,
            channelData
        })
    }

    cleanup() {
        this.decoder?.destroy()

        if ("cleanup" in this.base && typeof this.base.cleanup == "function") {
            return this.base.cleanup(...arguments)
        }
    }

    getBase(): Pipe | null {
        return this.base
    }
}