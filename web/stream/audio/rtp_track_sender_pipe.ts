import { TrackAudioPlayer } from ".";
import { globalObject } from "../../util";
import { Pipe, PipeInfo } from "../pipeline";
import { addPipePassthrough } from "../pipeline/pipes";

export class RtpTrackSenderPipe implements TrackAudioPlayer {
    static readonly pipeName = "RtpTrackSender"

    static readonly type = "audiotrack"

    static async getInfo(): Promise<PipeInfo> {
        return {
            environmentSupported: "RTCRtpSender" in globalObject(),
        }
    }

    readonly implementationName: string = "rtp_audio_track_sender"

    private track: MediaStreamTrack | null = null

    private sender: RTCRtpSender | null = null

    constructor() {
        addPipePassthrough(this)
    }

    setRtpSender(sender: RTCRtpSender) {
        this.sender = sender

        this.updateState()
    }

    setTrack(track: MediaStreamTrack) {
        this.track = track

        this.updateState()
    }

    private updateState() {
        if (this.sender && this.track) {
            this.sender.replaceTrack(this.track)
        }
    }

    cleanup(): void {
        this.sender?.replaceTrack(null)
        this.sender = null
    }

    getBase(): Pipe | null {
        return null
    }
}
