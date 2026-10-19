import { TrackAudioPlayer } from ".";
import { showNotification } from "../../component/notification";
import { globalObject } from "../../util";
import { Logger } from "../log";
import { Pipe, PipeInfo } from "../pipeline";
import { addPipePassthrough } from "../pipeline/pipes";

export class MicrophonePipe implements Pipe {
    static readonly pipeName = "MicrophonePipe"

    static readonly baseType = "audiotrack"
    static readonly type = "none"

    static async getInfo(): Promise<PipeInfo> {
        return {
            environmentSupported: "navigator" in globalObject() &&
                "mediaDevices" in navigator &&
                "getUserMedia" in navigator.mediaDevices &&
                typeof navigator.mediaDevices.getDisplayMedia == "function"
        }
    }

    readonly implementationName: string

    private logger: Logger | null = null

    private base: TrackAudioPlayer

    private microphoneTrack: MediaStreamTrack | null = null

    constructor(base: TrackAudioPlayer, logger?: Logger) {
        this.implementationName = `microphone -> ${base.implementationName}`

        this.logger = logger ?? null

        this.base = base

        addPipePassthrough(this)
    }

    private triedToAquire = false

    private async tryRequestMicrophone() {
        if (this.microphoneTrack || this.triedToAquire) {
            return
        }
        this.triedToAquire = true

        this.logger?.debug("Trying to aquire microphone")

        let microphone
        try {
            microphone = await navigator.mediaDevices.getUserMedia({ audio: true })
        } catch (e) {
            this.logger?.debug(`Failed to aquire microphone: ${e}`)
            showNotification(`Failed to aquire microphone!`)
            return
        }

        this.microphoneTrack = microphone.getAudioTracks()[0]
        this.base.setTrack(this.microphoneTrack)
    }

    onUserInteraction() {
        this.tryRequestMicrophone()

        if ("onUserInteraction" in this.base && typeof this.base.onUserInteraction == "function") {
            return this.base.onUserInteraction(...arguments)
        }
    }

    getBase(): Pipe | null {
        return this.base
    }
}
