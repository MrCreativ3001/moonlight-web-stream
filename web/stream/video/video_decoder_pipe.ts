import { VideoFormats } from "../../uniffi/moonlight_common_bindings"
import { globalObject } from "../../util"
import { Logger } from "../log"
import { Pipe, PipeInfo } from "../pipeline/index"
import { addPipePassthrough } from "../pipeline/pipes"
import { emptyVideoCodecs, } from "../video"
import { videoDecoderCodecInBand } from "./codec_level"
import { Av1StreamVideoTranslator, CodecStreamTranslator, H264StreamVideoTranslator, H265StreamVideoTranslator, VIDEO_DECODER_CODECS_OUT_OF_BAND } from "./annex_b_translator"
import { DataVideoRenderer, FrameVideoRenderer, VideoDecodeUnit, VideoRendererSetup } from "./index"

const DECODER_QUEUE_STALL_MS = 200
const DECODER_OUTPUT_STALL_MS = 1500
const VIDEO_INPUT_STALL_MS = 1500
const ACTIVE_VIDEO_INPUT_MS = 500
const IDR_RETRY_INTERVAL_MS = 1000

export const VIDEO_DECODER_CODECS_IN_BAND: Record<keyof VideoFormats, string> = {
    // avc1 = out of band config, avc3 = in band with sps, pps, idr
    "h264": "avc3.42E01E",
    "h264High8444": "avc3.640032",
    // hvc1 = out of band config, hev1 = in band with sps, pps, idr
    "h265": "hev1.1.6.L93.B0",
    "h265Main10": "hev1.2.4.L120.90",
    "h265Rext8444": "hev1.6.6.L93.90",
    "h265Rext10444": "hev1.6.10.L120.90",
    // av1 doesn't have in band and out of band distinction
    "av1Main8": "av01.0.04M.08",
    "av1Main10": "av01.0.04M.10",
    "av1High8444": "av01.0.08M.08",
    "av1High10444": "av01.0.08M.10"
}

async function detectCodecs(): Promise<VideoFormats> {
    if (!("isConfigSupported" in VideoDecoder)) {
        const codecs = emptyVideoCodecs()

        // We're just guessing that this browser supports h264
        codecs.h264 = true
        return codecs
    }

    const codecs = emptyVideoCodecs()
    const promises = []

    for (const codec2 in codecs) {
        const codec = codec2 as keyof VideoFormats

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

    // TODO: Firefox, Safari, Chrome say they can play this codec, but they can't
    codecs.h264High8444 = false

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
    static readonly pipeName = "VideoDecoderPipe"

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

    private width = 0
    private height = 0
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
        this.lastDecoderOutputAt = Date.now()
        this.base.submitFrame(frame)
    }

    private async trySetConfig(codec: string) {
        const baseConfig = {
            codedWidth: this.width,
            codedHeight: this.height,
        }

        if (!this.config) {
            this.config = await getIfConfigSupported({
                codec,
                hardwareAcceleration: "prefer-hardware",
                optimizeForLatency: true,
                ...baseConfig
            })
        }

        if (!this.config) {
            this.config = await getIfConfigSupported({
                codec,
                optimizeForLatency: true,
                ...baseConfig
            })
        }

        if (!this.config) {
            this.config = await getIfConfigSupported({
                codec,
                ...baseConfig
            })
        }
    }
    async setup(setup: VideoRendererSetup): Promise<void> {
        this.width = setup.width
        this.height = setup.height
        this.fps = setup.fps
        const isAv1 = setup.codec == "av1Main8" || setup.codec == "av1Main10" || setup.codec == "av1High8444" || setup.codec == "av1High10444"

        if (isAv1) {
            this.translator = new Av1StreamVideoTranslator(this.logger ?? undefined)
        }

        const codec = videoDecoderCodecInBand(setup.codec, setup.width, setup.height, setup.fps)
            ?? VIDEO_DECODER_CODECS_IN_BAND[setup.codec]
        await this.trySetConfig(codec)

        if (!this.config) {
            if (setup.codec == "h264" || setup.codec == "h264High8444") {
                this.translator = new H264StreamVideoTranslator(this.logger ?? undefined)

                const codec = VIDEO_DECODER_CODECS_OUT_OF_BAND[setup.codec]
                await this.trySetConfig(codec)
            } else if (setup.codec == "h265" || setup.codec == "h265Main10" || setup.codec == "h265Rext8444" || setup.codec == "h265Rext10444") {
                this.translator = new H265StreamVideoTranslator(this.logger ?? undefined)

                const codec = VIDEO_DECODER_CODECS_OUT_OF_BAND[setup.codec]
                await this.trySetConfig(codec)
            } else if (setup.codec == "av1Main8" || setup.codec == "av1Main10" || setup.codec == "av1High8444" || setup.codec == "av1High10444") {
                const codec = VIDEO_DECODER_CODECS_OUT_OF_BAND[setup.codec]
                await this.trySetConfig(codec)
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
    private lastIdrRequestAt = 0
    private lastVideoInputAt = 0
    private lastDecodeSubmitAt = 0
    private lastDecoderOutputAt = Date.now()
    private needsKeyFrame = true

    private bufferedUnits: Array<VideoDecodeUnit> = []
    submitDecodeUnit(unit: VideoDecodeUnit): void {
        if (this.errored) {
            console.debug("Cannot submit video decode unit because the stream errored")
            return
        }
        this.lastVideoInputAt = Date.now()
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

            const { configure, chunk, type } = value

            if (!chunk) {
                console.debug("No chunk received!")
                return
            }

            if (configure) {
                console.debug("Resetting video decoder config with", configure)

                this.decoder.reset()
                this.decoder.configure(configure)

            }

            const chunkType = type ?? unit.type
            if (chunkType != "key" && this.needsKeyFrame) {
                return
            }
            this.needsKeyFrame = false
            if (chunkType == "key") {
                // This likely is an idr
                this.requestedIdr = false
            }

            const encodedChunk = new EncodedVideoChunk({
                type: chunkType,
                timestamp: unit.timestampMicroseconds,
                duration: unit.durationMicroseconds,
                data: chunk,
            })
            this.lastDecodeSubmitAt = Date.now()
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

            this.lastDecodeSubmitAt = Date.now()
            this.decoder.decode(chunk)
        }
    }

    private reset() {
        this.decoder.reset()
        this.needsKeyFrame = true
        this.lastDecodeSubmitAt = 0
        this.lastDecoderOutputAt = Date.now()

        if (this.config) {
            this.decoder.configure(this.config)
        } else {
            this.logger?.debug("Failed to configure VideoDecoder because of missing config", { type: "fatal" })
        }

        if (this.translator && this.config) {
            this.translator.setBaseConfig(this.config)
        }
    }

    pollRequestIdr(): boolean {
        let requestIdr = false

        const now = Date.now()
        const estimatedQueueDelayMs = this.decoder.decodeQueueSize * 1000 / this.fps
        // We have more than 200ms second backlog in the decoder
        // -> This decoder is ass, request idr, flush that decoder
        const queueStalled = estimatedQueueDelayMs > DECODER_QUEUE_STALL_MS && this.decoder.decodeQueueSize > 2
        const outputStalled = this.lastDecodeSubmitAt > 0 &&
            now - this.lastDecodeSubmitAt < ACTIVE_VIDEO_INPUT_MS &&
            now - this.lastDecoderOutputAt > DECODER_OUTPUT_STALL_MS
        const inputStalled = this.lastVideoInputAt > 0 && now - this.lastVideoInputAt > VIDEO_INPUT_STALL_MS
        const idrResponseStalled = this.requestedIdr && now - this.lastIdrRequestAt >= IDR_RETRY_INTERVAL_MS

        if (queueStalled || outputStalled || inputStalled || idrResponseStalled) {
            if (!this.requestedIdr || now - this.lastIdrRequestAt >= IDR_RETRY_INTERVAL_MS) {
                requestIdr = true
                this.lastIdrRequestAt = now
                if (!this.requestedIdr) {
                    this.reset()
                }
            }
            if (requestIdr) {
                const reason = outputStalled
                    ? `no decoder output for ${now - this.lastDecoderOutputAt}ms while video input is active`
                    : inputStalled
                        ? `no video input for ${now - this.lastVideoInputAt}ms`
                        : idrResponseStalled
                            ? "the previous idr request did not produce a key frame"
                            : `decode queue size(${this.decoder.decodeQueueSize}) and estimated delay ${estimatedQueueDelayMs}ms`
                console.debug(`Requesting idr because of ${reason}`)
            }
        }

        if ("pollRequestIdr" in this.base && typeof this.base.pollRequestIdr == "function") {
            if (this.base.pollRequestIdr(...arguments)) {
                requestIdr = true
            }
        }

        if (requestIdr) {
            this.requestedIdr = true
            this.lastIdrRequestAt = now
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
