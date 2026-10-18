import { UrlVideoElementRenderer, VideoElementRenderer } from "./video_element"
import { VideoMediaStreamTrackProcessorPipe } from "./media_stream_track_processor_pipe"
import { VideoDecoderPipe } from "./video_decoder_pipe"
import { DepacketizeVideoPipe } from "./depackitize_pipe"
import { Logger } from "../log"
import { allVideoCodecs, andVideoCodecs, emptyVideoCodecs, hasAnyCodec, orVideoCodecs } from "../video"
import { gatherPipeInfo, getPipe, OutputPipeStatic, PipeInfo, PipeInfoStatic, Pipeline, pipelineInputType, pipelineToString, pipeName } from "../pipeline/index"
import { workerPipe } from "../pipeline/worker_pipe"
import { WorkerVideoDataSendPipe, WorkerVideoFrameReceivePipe, WorkerVideoTrackReceivePipe, WorkerVideoTrackSendPipe } from "../pipeline/worker_io"
import { OffscreenCanvasRenderer } from "./offscreen_canvas"
import { MainCanvasRenderer } from "./canvas"
import { CanvasFrameDrawPipe, CanvasRgbaFrameDrawPipe, CanvasYuv420FrameDrawPipe as CanvasYuv420FrameDrawPipe } from "./canvas_frame"
import { OpenH264DecoderPipe } from "./openh264_decoder_pipe"
import { VideoMediaStreamTrackGeneratorPipe } from "./media_stream_track_generator_pipe"
import { Yuv420ToRgbaFramePipe } from "./video_frame"
import { MediaSourceDecoder } from "./media_source_decoder"
import { VideoFormats } from "../../uniffi/moonlight_common_bindings"

// -- Build the pipeline
export type VideoPipelineOptions = {
    supportedVideoCodecs: VideoFormats
    canvasRenderer: boolean
    forceVideoElementRenderer: boolean
    /// When true:
    /// - enable desynchronized in the context creation options (lower latency)
    /// - draw in submitFrame (low latency)
    /// When false:
    /// - draw only on rAF (VSync-like, may reduce tearing).
    canvasVsync: boolean
}

export const WorkerVideoMediaStreamProcessorPipe = workerPipe("WorkerVideoMediaStreamProcessorPipe", { pipes: ["WorkerVideoTrackReceivePipe", "VideoMediaStreamTrackProcessorPipe", "WorkerVideoFrameSendPipe"] })
export const WorkerVideoMediaStreamProcessorCanvasPipe = workerPipe("WorkerVideoMediaStreamProcessorCanvasPipe", { pipes: ["WorkerVideoTrackReceivePipe", "VideoMediaStreamTrackProcessorPipe", "CanvasFrameDrawPipe", "WorkerOffscreenCanvasSendPipe"] })
export const WorkerDataToVideoTrackPipe = workerPipe("WorkerVideoFrameToTrackPipe", { pipes: ["WorkerVideoDataReceivePipe", "VideoDecoderPipe", "VideoTrackGeneratorPipe", "WorkerVideoTrackSendPipe"] })
export const WorkerDataToCanvasGlRenderOpenH264Pipe = workerPipe("WorkerDataToCanvasGlRenderOpenH264Pipe", { pipes: ["WorkerVideoDataReceivePipe", "OpenH264DecoderPipe", "CanvasYuv420FrameDrawPipe", "WorkerOffscreenCanvasSendPipe"] })

const PIPELINES: Array<Pipeline> = [
    // -- track
    // Convert track -> video element, Default (should be supported everywhere)
    { pipes: [], output: VideoElementRenderer },
    // Convert track -> video frame -> canvas, Chromium
    { pipes: [VideoMediaStreamTrackProcessorPipe, CanvasFrameDrawPipe], output: MainCanvasRenderer },
    // Convert track -> video frame (in worker) -> canvas (in worker), Safari
    { pipes: [WorkerVideoTrackSendPipe, WorkerVideoMediaStreamProcessorCanvasPipe], output: OffscreenCanvasRenderer },
    // Convert track -> video frame (in worker) -> canvas, Safari
    { pipes: [WorkerVideoTrackSendPipe, WorkerVideoMediaStreamProcessorPipe, WorkerVideoFrameReceivePipe], output: MainCanvasRenderer },
    // -- data
    // - VideoDecoder
    // Convert data -> video frame (in worker) -> track (in worker, VideoTrackGenerator) -> video element, Safari
    { pipes: [DepacketizeVideoPipe, WorkerVideoDataSendPipe, WorkerDataToVideoTrackPipe, WorkerVideoTrackReceivePipe], output: VideoElementRenderer },
    // Convert data -> video frame -> track (MediaStreamTrackGenerator) -> video element, Chromium
    { pipes: [DepacketizeVideoPipe, VideoDecoderPipe, VideoMediaStreamTrackGeneratorPipe], output: VideoElementRenderer },
    // Convert data -> video frame -> canvas, Default (Secure Context), Firefox
    { pipes: [DepacketizeVideoPipe, VideoDecoderPipe, CanvasFrameDrawPipe], output: MainCanvasRenderer },
    // - OpenH264 Decoder
    // Convert data -> decode -> draw using webgl (in worker) -> canvas
    { pipes: [DepacketizeVideoPipe, WorkerVideoDataSendPipe, WorkerDataToCanvasGlRenderOpenH264Pipe], output: OffscreenCanvasRenderer },
    // Convert data -> decode -> draw using webgl -> canvas
    { pipes: [DepacketizeVideoPipe, OpenH264DecoderPipe, CanvasYuv420FrameDrawPipe], output: MainCanvasRenderer },
    // Convert data -> decode -> draw using image -> canvas
    { pipes: [DepacketizeVideoPipe, OpenH264DecoderPipe, Yuv420ToRgbaFramePipe, CanvasRgbaFrameDrawPipe], output: MainCanvasRenderer },
    // - MediaSourceDecoder
    // Convert data -> MediaSourceDecoder -> video element, Default (should be supported everywhere)
    { pipes: [DepacketizeVideoPipe, MediaSourceDecoder], output: UrlVideoElementRenderer },
]

const FORCE_CANVAS_PIPELINES: Array<Pipeline> = PIPELINES.filter(pipeline => pipeline?.output && pipeName(pipeline.output).includes("Canvas"))

async function queryVideoPipelineInfo(pipeline: Pipeline, logger?: Logger): Promise<PipeInfo> {
    let supportedCodecs = allVideoCodecs()
    const pipesInfo = await gatherPipeInfo()

    // Check if supported and contains codecs
    for (const pipe of pipeline.pipes) {
        const pipeStatic = getPipe(pipe)
        if (!pipeStatic) {
            logger?.debug(`Failed to find pipe ${pipeName(pipe)} whilst querying video pipeline info`)
            continue
        }

        const pipeInfo = pipesInfo.get(pipeStatic)
        if (!pipeInfo) {
            logger?.debug(`Failed to query info for video pipe ${pipeName(pipe)}`)
            return {
                environmentSupported: false
            }
        }

        if (!pipeInfo?.environmentSupported) {
            return {
                environmentSupported: false,
            }
        }

        if (pipeInfo?.supportedVideoCodecs) {
            supportedCodecs = andVideoCodecs(supportedCodecs, pipeInfo.supportedVideoCodecs)
        }
    }

    const rendererInfo = await pipeline.output?.getInfo()
    if (!rendererInfo) {
        logger?.debug(`Failed to query info for video renderer ${pipelineToString(pipeline)}`)
        return {
            environmentSupported: false
        }
    }

    // See if the pipeline is supported and has the required codecs
    if (!rendererInfo.environmentSupported) {
        return {
            environmentSupported: false
        }
    }
    if (rendererInfo.supportedVideoCodecs) {
        supportedCodecs = andVideoCodecs(supportedCodecs, rendererInfo.supportedVideoCodecs)
    }

    return {
        environmentSupported: true,
        supportedVideoCodecs: supportedCodecs,
    }
}

async function allSelectedVideoPipelines(settings: VideoPipelineOptions, logger?: Logger): Promise<Array<Pipeline>> {
    let pipelines: Array<Pipeline> = []

    // Forced renderer
    if (settings.forceVideoElementRenderer) {
        logger?.debug("Forcing Video Element Renderer")

        // H264 is assumed universal, if we don't currently support something force it!
        if (!hasAnyCodec(settings.supportedVideoCodecs)) {
            logger?.debug("No codec currently found. Setting H264 as supported even though the browser says it is not supported")

            settings.supportedVideoCodecs.h264 = true
        }

        return [{ pipes: [], output: VideoElementRenderer }]
    }

    if (settings.canvasRenderer) {
        logger?.debug("Forcing canvas renderer")

        pipelines = FORCE_CANVAS_PIPELINES
    } else {
        logger?.debug("Selecting pipeline automatically")

        pipelines = PIPELINES
    }

    // Forced renderer
    if (settings.forceVideoElementRenderer) {
        logger?.debug("Forcing Video Element Renderer")

        // H264 is assumed universal, if we don't currently support something force it!
        if (!hasAnyCodec(settings.supportedVideoCodecs)) {
            logger?.debug("No codec currently found. Setting H264 as supported even though the browser says it is not supported")

            settings.supportedVideoCodecs.h264 = true
        }

        return [{ pipes: [], output: VideoElementRenderer }]
    }

    if (settings.canvasRenderer) {
        logger?.debug("Forcing canvas renderer")

        pipelines = FORCE_CANVAS_PIPELINES
    } else {
        logger?.debug("Selecting pipeline automatically")

        pipelines = PIPELINES
    }

    const outputPipelines = []

    pipelineLoop: for (const pipeline of pipelines) {
        const pipelineInfo = await queryVideoPipelineInfo(pipeline)
        const supportedCodecs = andVideoCodecs(pipelineInfo.supportedVideoCodecs ?? allVideoCodecs(), settings.supportedVideoCodecs)

        if (!hasAnyCodec(supportedCodecs)) {
            logger?.debug(`Not using pipe ${pipelineToString(pipeline)} because it doesn't support any codec the user wants`)
            continue pipelineLoop
        }

        outputPipelines.push(pipeline)
    }

    return pipelines
}

export async function querySupportedDataVideoCodecs(settings: VideoPipelineOptions, logger?: Logger): Promise<VideoFormats> {
    let supportedCodecs = emptyVideoCodecs()
    const pipelines = await allSelectedVideoPipelines(settings, logger)

    for (const pipeline of pipelines) {
        const inputType = pipelineInputType(pipeline)
        if (inputType != "data") {
            continue
        }

        const pipelineInfo = await queryVideoPipelineInfo(pipeline)
        if (pipelineInfo.supportedVideoCodecs) {
            supportedCodecs = orVideoCodecs(supportedCodecs, pipelineInfo.supportedVideoCodecs)
        }
    }

    return supportedCodecs
}

export async function selectVideoPipeline(type: string, settings: VideoPipelineOptions, logger?: Logger): Promise<Pipeline | null> {
    const pipelines = await allSelectedVideoPipelines(settings, logger)

    for (const pipeline of pipelines) {
        const inputType = pipelineInputType(pipeline)
        if (inputType != type) {
            continue
        }

        return pipeline
    }

    return null
}
