import { Pipe, PipeInfo } from "../pipeline/index.js";
import { addPipePassthrough, DataPipe } from "../pipeline/pipes.js";
import { DataAudioPlayer } from "./index.js";

export class DepacketizeAudioPipe implements DataPipe {

    static async getInfo(): Promise<PipeInfo> {
        return {
            environmentSupported: true
        }
    }

    static readonly baseType = "data"
    static readonly type = "data"


    readonly implementationName: string

    private base: DataAudioPlayer

    constructor(base: DataAudioPlayer) {
        this.implementationName = `depacketize_audio -> ${base.implementationName}`
        this.base = base

        addPipePassthrough(this)
    }

    submitPacket(buffer: ArrayBuffer) {
        this.base.decodeAndPlay({
            data: buffer,
            // TODO: use actual timestamps / durations
            timestampMicroseconds: 0,
            durationMicroseconds: 0,
        })
    }

    getBase(): Pipe | null {
        return this.base
    }
}