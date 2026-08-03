import { Pipe, PipeInfo } from "../pipeline/index"
import { addPipePassthrough, DataPipe } from "../pipeline/pipes"
import { AudioPlayerSetup, DataAudioPlayer } from "./index"

export class DepacketizeAudioPipe implements DataPipe {
    static readonly pipeName = "DepacketizeAudioPipe"

    static async getInfo(): Promise<PipeInfo> {
        return {
            environmentSupported: true
        }
    }

    static readonly baseType = "audiodata"
    static readonly type = "wsdata"

    readonly implementationName: string

    private base: DataAudioPlayer
    private timestampMicroseconds: number = 0
    private packetDurationMicroseconds: number = 0

    constructor(base: DataAudioPlayer) {
        this.implementationName = `depacketize_audio -> ${base.implementationName}`
        this.base = base

        addPipePassthrough(this)
    }

    setup(setup: AudioPlayerSetup) {
        this.packetDurationMicroseconds = setup.samplesPerFrame * 1_000_000 / setup.sampleRate

        if ("setup" in this.base && typeof this.base.setup == "function") {
            return this.base.setup(...arguments)
        }
    }

    submitPacket(buffer: Uint8Array) {
        this.base.decodeAndPlay({
            data: buffer,
            timestampMicroseconds: this.timestampMicroseconds,
            durationMicroseconds: this.packetDurationMicroseconds,
        })

        this.timestampMicroseconds += this.packetDurationMicroseconds
    }

    getBase(): Pipe | null {
        return this.base
    }
}
