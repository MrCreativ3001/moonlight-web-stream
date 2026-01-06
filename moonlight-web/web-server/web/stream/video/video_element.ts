import { globalObject, Pipe, PipeInfo } from "../pipeline/index.js";
import { emptyVideoCodecs, maybeVideoCodecs, VideoCodecSupport } from "../video.js";
import { getStreamRectCorrected, TrackVideoRenderer, VideoRenderer, VideoRendererSetup } from "./index.js";

const VIDEO_DECODER_CODECS: Record<keyof VideoCodecSupport, string> = {
    "H264": "avc1.42E01E",
    "H264_HIGH8_444": "avc1.640032",
    "H265": "hvc1.1.6.L93.B0",
    "H265_MAIN10": "hvc1.2.4.L120.90",
    "H265_REXT8_444": "hvc1.6.6.L93.90",
    "H265_REXT10_444": "hvc1.6.10.L120.90",
    "AV1_MAIN8": "av01.0.04M.08",
    "AV1_MAIN10": "av01.0.04M.10",
    "AV1_HIGH8_444": "av01.0.08M.08",
    "AV1_HIGH10_444": "av01.0.08M.10"
}

function detectCodecs(): VideoCodecSupport {
    if (!("canPlayType" in HTMLVideoElement.prototype)) {
        return maybeVideoCodecs()
    }

    const codecs = emptyVideoCodecs()

    const testElement = document.createElement("video")

    for (const codec in codecs) {
        const supported = testElement.canPlayType(`video/mp4; codecs=${VIDEO_DECODER_CODECS[codec]}`)

        if (supported == "probably") {
            codecs[codec] = true
        } else if (supported == "maybe") {
            codecs[codec] = "maybe"
        } else {
            // unsupported
            codecs[codec] = false
        }
    }

    return codecs
}

export class VideoElementRenderer implements TrackVideoRenderer, VideoRenderer {
    static readonly type = "videotrack"

    static async getInfo(): Promise<PipeInfo> {
        const supported = "HTMLVideoElement" in globalObject() && "srcObject" in HTMLVideoElement.prototype

        return {
            environmentSupported: supported,
            supportedVideoCodecs: supported ? detectCodecs() : emptyVideoCodecs()
        }
    }

    readonly implementationName: string = "video_element"

    private videoElement = document.createElement("video")
    private oldTrack: MediaStreamTrack | null = null
    private stream = new MediaStream()

    private size: [number, number] | null = null
    private hdrEnabled: boolean = false

    constructor() {
        this.videoElement.classList.add("video-stream")
        this.videoElement.preload = "none"
        this.videoElement.controls = false
        this.videoElement.autoplay = true
        this.videoElement.disablePictureInPicture = true
        this.videoElement.playsInline = true
        this.videoElement.muted = true

        if ("srcObject" in this.videoElement) {
            try {
                this.videoElement.srcObject = this.stream
            } catch (err: any) {
                if (err.name !== "TypeError") {
                    throw err;
                }

                console.error(err)
                throw `video_element renderer not supported: ${err}`
            }
        }
    }

    async setup(setup: VideoRendererSetup) {
        this.size = [setup.width, setup.height]
    }
    cleanup(): void {
        if (this.oldTrack) {
            this.stream.removeTrack(this.oldTrack)
        }
        this.videoElement.srcObject = null
    }

    setTrack(track: MediaStreamTrack): void {
        if (this.oldTrack) {
            this.stream.removeTrack(this.oldTrack)
        }

        this.stream.addTrack(track)
        this.oldTrack = track
    }

    pollRequestIdr(): boolean {
        return false
    }

    mount(parent: HTMLElement): void {
        parent.appendChild(this.videoElement)
    }
    unmount(parent: HTMLElement): void {
        parent.removeChild(this.videoElement)
    }

    onUserInteraction(): void {
        if (this.videoElement.paused) {
            this.videoElement.play().then(() => {
                // Playing
            }).catch(error => {
                console.error(`Failed to play videoElement: ${error.message || error}`);
            })
        }
    }
    getStreamRect(): DOMRect {
        if (!this.size) {
            return new DOMRect()
        }

        return getStreamRectCorrected(this.videoElement.getBoundingClientRect(), this.size)
    }

    getBase(): Pipe | null {
        return null
    }

    setHdrMode(enabled: boolean): void {
        this.hdrEnabled = enabled
        // Request HDR display mode if supported
        if (enabled && "requestHDR" in this.videoElement) {
            try {
                (this.videoElement as any).requestHDR()
            } catch (err) {
                console.warn("Failed to request HDR mode:", err)
            }
        }
        // Set color space attributes for HDR
        if (enabled) {
            this.videoElement.setAttribute("color-gamut", "rec2020")
            this.videoElement.setAttribute("transfer-function", "pq")
        } else {
            this.videoElement.removeAttribute("color-gamut")
            this.videoElement.removeAttribute("transfer-function")
        }
    }
}