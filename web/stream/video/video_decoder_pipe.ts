import { globalObject } from "../../util.js";
import { ByteBuffer } from "../buffer.js";
import { Logger } from "../log.js";
import { Pipe, PipeInfo } from "../pipeline/index.js";
import { addPipePassthrough } from "../pipeline/pipes.js";
import { emptyVideoCodecs, maybeVideoCodecs, VideoCodecSupport } from "../video.js";
import { CodecStreamTranslator, H264StreamVideoTranslator, H265StreamVideoTranslator, VIDEO_DECODER_CODECS_OUT_OF_BAND } from "./annex_b_translator.js";
import { DataVideoRenderer, FrameVideoRenderer, VideoDecodeUnit, VideoRendererSetup } from "./index.js";

export const VIDEO_DECODER_CODECS_IN_BAND: Record<keyof VideoCodecSupport, string> = {
    // avc1 = out of band config, avc3 = in band with sps, pps, idr
    "H264": "avc3.42E01E",
    "H264_HIGH8_444": "avc3.640032",
    // hvc1 = out of band config, hev1 = in band with sps, pps, idr
    "H265": "hev1.1.6.L93.B0",
    "H265_MAIN10": "hev1.2.4.L120.90",
    "H265_REXT8_444": "hev1.6.6.L93.90",
    "H265_REXT10_444": "hev1.6.10.L120.90",
    // av1 doesn't have in band and out of band distinction
    "AV1_MAIN8": "av01.0.04M.08",
    "AV1_MAIN10": "av01.0.04M.10",
    "AV1_HIGH8_444": "av01.0.08M.08",
    "AV1_HIGH10_444": "av01.0.08M.10"
}

async function detectCodecs(): Promise<VideoCodecSupport> {
    if (!("isConfigSupported" in VideoDecoder)) {
        return maybeVideoCodecs()
    }

    const codecs = emptyVideoCodecs()
    const promises = []

    for (const codec in codecs) {
        promises.push((async () => {
            const supportedInBand = await VideoDecoder.isConfigSupported({
                codec: VIDEO_DECODER_CODECS_IN_BAND[codec]
            })

            const supportedOutOfBand = await VideoDecoder.isConfigSupported({
                codec: VIDEO_DECODER_CODECS_OUT_OF_BAND[codec]
            })

            codecs[codec] = supportedInBand.supported || supportedOutOfBand.supported ? true : false
        })())
    }
    await Promise.all(promises)

    // TODO: Firefox, Safari say they can play this codec, but they can't
    codecs.H264_HIGH8_444 = false

    return codecs
}
async function getIfConfigSupported(config: VideoDecoderConfig): Promise<VideoDecoderConfig | null> {
    const supported = await VideoDecoder.isConfigSupported(config)
    if (supported.supported) {
        return config
    }
    return null
}

export class VideoDecoderPipe implements DataVideoRenderer {
    static readonly baseType = "videoframe"
    static readonly type = "videodata"

    static async getInfo(): Promise<PipeInfo> {
        const supported = "VideoDecoder" in globalObject()

        return {
            environmentSupported: supported,
            supportedVideoCodecs: supported ? await detectCodecs() : emptyVideoCodecs()
        }
    }

    readonly implementationName: string

    private logger: Logger | null

    private base: FrameVideoRenderer

    private fps = 0

    private errored = false
    private config: VideoDecoderConfig | null = null
    private translator: CodecStreamTranslator | null = null
    private decoder: VideoDecoder

    constructor(base: FrameVideoRenderer, logger?: Logger) {
        this.implementationName = `video_decoder -> ${base.implementationName}`
        this.logger = logger ?? null

        this.base = base

        this.decoder = new VideoDecoder({
            error: this.onError.bind(this),
            output: this.onOutput.bind(this)
        })

        addPipePassthrough(this)
    }

    private onError(error: any) {
        this.errored = true

        this.logger?.debug(`VideoDecoder has an error ${"toString" in error ? error.toString() : `${error}`}`, { type: "fatal" })
        console.error(error)
    }

    private onOutput(frame: VideoFrame) {
        this.base.submitFrame(frame)
    }

    private async trySetConfig(codec: string) {
        if (!this.config) {
            this.config = await getIfConfigSupported({
                codec,
                hardwareAcceleration: "prefer-hardware",
                optimizeForLatency: true
            })
        }

        if (!this.config) {
            this.config = await getIfConfigSupported({
                codec,
                optimizeForLatency: true
            })
        }

        if (!this.config) {
            this.config = await getIfConfigSupported({
                codec,
            })
        }
    }
    async setup(setup: VideoRendererSetup): Promise<void> {
        this.fps = setup.fps

        const codec = VIDEO_DECODER_CODECS_IN_BAND[setup.codec]
        await this.trySetConfig(codec)

        if (!this.config) {
            if (setup.codec == "H264" || setup.codec == "H264_HIGH8_444") {
                this.translator = new H264StreamVideoTranslator(this.logger ?? undefined)

                const codec = VIDEO_DECODER_CODECS_OUT_OF_BAND[setup.codec]
                await this.trySetConfig(codec)
            } else if (setup.codec == "H265" || setup.codec == "H265_MAIN10" || setup.codec == "H265_REXT8_444" || setup.codec == "H265_REXT10_444") {
                this.translator = new H265StreamVideoTranslator(this.logger ?? undefined)

                const codec = VIDEO_DECODER_CODECS_OUT_OF_BAND[setup.codec]
                await this.trySetConfig(codec)
            } else if (setup.codec == "AV1_MAIN8" || setup.codec == "AV1_MAIN10" || setup.codec == "AV1_HIGH8_444" || setup.codec == "AV1_HIGH10_444") {
                this.errored = true
                this.logger?.debug("Av1 stream translator is not implemented currently!", { type: "fatalDescription" })
                return
            } else {
                this.errored = true
                this.logger?.debug(`Failed to find stream translator for codec ${setup.codec}`)
                return
            }
        }

        if (!this.config) {
            this.errored = true
            this.logger?.debug(`Failed to setup VideoDecoder for codec ${setup.codec} because of missing config`)
            return
        }
        this.translator?.setBaseConfig(this.config)

        this.logger?.debug(`VideoDecoder config: ${JSON.stringify(this.config)}`)

        this.reset()

        this.decoderSetupFinished = true

        if ("setup" in this.base && typeof this.base.setup == "function") {
            return await this.base.setup(...arguments)
        }
    }

    private decoderSetupFinished = false
    private requestedIdr = false
    private needsKeyFrame = true

    private bufferedUnits: Array<VideoDecodeUnit> = []
    submitDecodeUnit(unit: VideoDecodeUnit): void {
        if (this.errored) {
            console.debug("Cannot submit video decode unit because the stream errored")
            return
        }
        if (!this.decoderSetupFinished) {
            this.bufferedUnits.push(unit)
            return
        }

        if (this.bufferedUnits.length > 0) {
            const bufferedUnits = this.bufferedUnits.splice(0)

            for (const bufferedUnit of bufferedUnits) {
                this.submitDecodeUnit(bufferedUnit)
            }
        }


        if (this.translator) {
            const value = this.translator.submitDecodeUnit(unit)
            if (value.error) {
                this.errored = true
                this.logger?.debug("VideoDecoder has errored!")
                return
            }

            const { configure, chunk } = value

            if (!chunk) {
                console.debug("No chunk received!")
                return
            }

            if (configure) {
                console.debug("Resetting video decoder config with", configure)

                this.decoder.reset()
                this.decoder.configure(configure)

                // This likely is an idr
                this.requestedIdr = false
            }

            const encodedChunk = new EncodedVideoChunk({
                type: unit.type,
                timestamp: unit.timestampMicroseconds,
                duration: unit.durationMicroseconds,
                data: chunk,
            })
            this.decoder.decode(encodedChunk)
        } else {
            if (unit.type != "key" && this.needsKeyFrame) {
                return
            }
            this.needsKeyFrame = false
            this.requestedIdr = false

            const chunk = new EncodedVideoChunk({
                type: unit.type,
                data: unit.data,
                timestamp: unit.timestampMicroseconds,
                duration: unit.durationMicroseconds
            })

            this.decoder.decode(chunk)
        }
    }

    private reset() {
        if (!this.translator) {
            this.decoder.reset()
            this.needsKeyFrame = true

            if (this.config) {
                this.decoder.configure(this.config)
            } else {
                this.logger?.debug("Failed to configure VideoDecoder because of missing config", { type: "fatal" })
            }
        } else if (this.config) {
            this.translator.setBaseConfig(this.config)
        }
    }

    pollRequestIdr(): boolean {
        let requestIdr = false

        const estimatedQueueDelayMs = this.decoder.decodeQueueSize * 1000 / this.fps
        if (estimatedQueueDelayMs > 200 && this.decoder.decodeQueueSize > 2) {
            // We have more than 200ms second backlog in the decoder
            // -> This decoder is ass, request idr, flush that decoder

            if (!this.requestedIdr) {
                requestIdr = true
                this.reset()
            }
            console.debug(`Requesting idr because of decode queue size(${this.decoder.decodeQueueSize}) and estimated delay of the queue: ${estimatedQueueDelayMs}`)
        }

        if ("pollRequestIdr" in this.base && typeof this.base.pollRequestIdr == "function") {
            if (this.base.pollRequestIdr(...arguments)) {
                requestIdr = true
            }
        }

        if (requestIdr) {
            this.requestedIdr = true
        }

        return requestIdr
    }

    cleanup() {
        this.decoder.close()

        if ("cleanup" in this.base && typeof this.base.cleanup == "function") {
            return this.base.cleanup(arguments)
        }
    }

    getBase(): Pipe | null {
        return this.base
    }
}
