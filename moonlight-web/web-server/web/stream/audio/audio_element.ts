import { globalObject } from "../../util.js";
import { Pipe, PipeInfo } from "../pipeline/index.js";
import { addPipePassthrough } from "../pipeline/pipes.js";
import { AudioPlayerSetup, TrackAudioPlayer } from "./index.js";

export class AudioElementPlayer implements TrackAudioPlayer {

    static readonly type = "audiotrack"

    static async getInfo(): Promise<PipeInfo> {
        return {
            environmentSupported: "HTMLAudioElement" in globalObject() && "srcObject" in HTMLAudioElement.prototype,
        }
    }

    readonly implementationName: string = "audio_element"

    private audioElement = document.createElement("audio")
    private oldTrack: MediaStreamTrack | null = null
    private stream = new MediaStream()

    constructor() {
        this.implementationName = "audio_element"

        this.audioElement.classList.add("audio-stream")
        this.audioElement.preload = "none"
        this.audioElement.controls = false
        this.audioElement.autoplay = true
        this.audioElement.muted = true
        this.audioElement.srcObject = this.stream

        addPipePassthrough(this)
    }

    setup(_setup: AudioPlayerSetup) {
        return true
    }
    cleanup(): void {
        if (this.oldTrack) {
            this.stream.removeTrack(this.oldTrack)
            this.oldTrack = null
        }
        this.audioElement.srcObject = null
    }

    setTrack(track: MediaStreamTrack): void {
        if (this.oldTrack) {
            this.stream.removeTrack(this.oldTrack)
            this.oldTrack = null
        }

        this.stream.addTrack(track)
        this.oldTrack = track
    }

    onUserInteraction(): void {
        this.audioElement.muted = false
    }

    mount(parent: HTMLElement): void {
        parent.appendChild(this.audioElement)
    }
    unmount(parent: HTMLElement): void {
        parent.removeChild(this.audioElement)
    }

    getBase(): Pipe | null {
        return null
    }
}