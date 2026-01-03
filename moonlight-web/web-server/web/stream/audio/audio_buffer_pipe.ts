import { globalObject, Pipe, PipeInfo } from "../pipeline/index.js";
import { addPipePassthrough } from "../pipeline/pipes.js";
import { AudioPcmUnit, AudioPlayerSetup, NodeAudioPlayer, PcmAudioPlayer } from "./index.js";

export class AudioBufferPipe implements PcmAudioPlayer {

    static async getInfo(): Promise<PipeInfo> {
        return {
            environmentSupported: "AudioBufferSourceNode" in globalObject()
        }
    }

    static readonly baseType = "audionode"
    static readonly type = "audiopcm"

    readonly implementationName: string

    private base: NodeAudioPlayer

    constructor(base: NodeAudioPlayer) {
        this.implementationName = `audio_pcm_buffer -> ${base.implementationName}`
        this.base = base

        addPipePassthrough(this)
    }

    private channels: number = -1
    private sampleRate: number = -1

    private node: AudioNode | null = null

    setup(setup: AudioPlayerSetup) {
        this.channels = setup.channels
        this.sampleRate = setup.sampleRate

        let result
        if ("setup" in this.base && typeof this.base.setup == "function") {
            this.base.setup(...arguments)
        }

        this.node = this.base.getAudioContext().createGain()

        this.base.setSource(this.node)

        return result
    }

    private hadUserInteraction: boolean = false

    onUserInteraction() {
        this.hadUserInteraction = true

        if ("onUserInteraction" in this.base && typeof this.base.onUserInteraction == "function") {
            return this.base.onUserInteraction(...arguments)
        }
    }

    private nextAudioPlayTime: number | null = null

    playPcm(unit: AudioPcmUnit): void {
        if (!this.node) {
            return
        }
        if (!this.hadUserInteraction) {
            return
        }

        const TARGET_LATENCY_SECS = 0.12
        const MAX_LATENCY_SECS = 0.25

        const now = this.base.getAudioContext().currentTime

        if (this.nextAudioPlayTime == null) {
            this.nextAudioPlayTime = now + TARGET_LATENCY_SECS
        }

        let ahead = this.nextAudioPlayTime - this.base.getAudioContext().currentTime

        // Too far ahead -> gently pull back audio
        if (ahead > MAX_LATENCY_SECS) {
            console.debug("Audio too far ahead, trimming latency");
            this.nextAudioPlayTime = now + TARGET_LATENCY_SECS;
            ahead = TARGET_LATENCY_SECS;
        }

        // Underrun -> jump forward
        if (ahead < 0) {
            console.debug("Audio underrun")

            // We are behind (underrun), jump to current time + small buffer
            this.nextAudioPlayTime = now + TARGET_LATENCY_SECS;
        }

        const context = this.base.getAudioContext()

        const buffer = context.createBuffer(this.channels, unit.channelData[0].length, this.sampleRate)

        for (let channel = 0; channel < this.channels; channel++) {
            const channelPcm = unit.channelData[channel]

            buffer.copyToChannel(channelPcm, channel)
        }

        const source = context.createBufferSource()
        source.buffer = buffer
        source.connect(this.node)

        source.start(this.nextAudioPlayTime)
        this.nextAudioPlayTime += buffer.duration

        source.onended = () => source.disconnect()
    }

    getBase(): Pipe | null {
        return this.base
    }
}