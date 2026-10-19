import { AudioBufferPipe } from "../audio/audio_buffer_pipe"
import { AudioContextTrackPipe } from "../audio/audio_context_track_pipe"
import { OpusAudioDecoderPipe } from "../audio/opus_decoder_pipe"
import { AudioDecoderPipe } from "../audio/audio_decoder_pipe"
import { DepacketizeAudioPipe } from "../audio/depacketize_pipe"
import { AudioMediaStreamTrackGeneratorPipe } from "../audio/media_stream_track_generator_pipe"
import { Logger } from "../log"
import { OpenH264DecoderPipe } from "../video/openh264_decoder_pipe"
import { CanvasFrameDrawPipe, CanvasRgbaFrameDrawPipe, CanvasYuv420FrameDrawPipe } from "../video/canvas_frame"
import { DepacketizeVideoPipe } from "../video/depackitize_pipe"
import { VideoMediaStreamTrackGeneratorPipe } from "../video/media_stream_track_generator_pipe"
import { VideoMediaStreamTrackProcessorPipe } from "../video/media_stream_track_processor_pipe"
import { WorkerDataToCanvasGlRenderOpenH264Pipe, WorkerDataToVideoTrackPipe, WorkerVideoMediaStreamProcessorCanvasPipe, WorkerVideoMediaStreamProcessorPipe } from "../video/pipeline"
import { VideoDecoderPipe } from "../video/video_decoder_pipe"
import { VideoTrackGeneratorPipe } from "../video/video_track_generator"
import { WorkerDataReceivePipe, WorkerDataSendPipe, WorkerOffscreenCanvasSendPipe, WorkerVideoDataReceivePipe, WorkerVideoDataSendPipe, WorkerVideoFrameReceivePipe, WorkerVideoFrameSendPipe, WorkerVideoTrackReceivePipe, WorkerVideoTrackSendPipe } from "./worker_io"
import { StatValue } from "../stats"
import { Yuv420ToRgbaFramePipe } from "../video/video_frame"
import { MediaSourceDecoder } from "../video/media_source_decoder"
import { VideoFormats } from "../../uniffi/moonlight_common_bindings"

export interface Pipe {
    readonly implementationName: string

    reportStats?(statsObject: Record<string, StatValue>): Promise<void>

    getBase(): Pipe | null
}

export type PipeInfo = {
    environmentSupported: boolean
    supportedVideoCodecs?: VideoFormats
}

export interface PipeInfoStatic {
    readonly pipeName: string

    getInfo(): Promise<PipeInfo>
}
export interface PipeStatic extends PipeInfoStatic, InputPipeStatic {
    readonly type: string

    new(base: any, logger?: Logger): Pipe
}

export interface InputPipeStatic extends PipeInfoStatic {
    readonly baseType: string
}
export interface OutputPipeStatic extends PipeInfoStatic {
    readonly type: string

    new(logger?: Logger, options?: unknown): Pipe
}

export type Pipeline = {
    pipes: Array<string | PipeStatic>
    output?: PipeInfoStatic & OutputPipeStatic
}

export function pipelineToString(pipeline: Pipeline): string {
    let string = pipeline.pipes.map(pipe => pipeName(pipe)).join(" -> ")

    if (pipeline.output) {
        string += ` -> ${pipeName(pipeline.output)}`
    }

    return string
}

export function pipeName(pipe: string | { pipeName: string }): string {
    if (typeof pipe == "string") {
        return pipe
    } else {
        return pipe.pipeName
    }
}
export function getPipe(pipe: string | PipeStatic): PipeStatic | null {
    if (typeof pipe == "string") {
        const foundPipe = pipes().find(check => check.pipeName == pipe)

        return foundPipe ?? null
    } else {
        return pipe
    }
}

export function pipelineInputType(pipeline: Pipeline, logger?: Logger): string | null {
    if (pipeline.pipes[0]) {
        const pipe = getPipe(pipeline.pipes[0])
        if (!pipe) {
            logger?.debug(`Failed to find input type for pipeline ${pipelineToString(pipeline)}`)
            return null
        }
        return pipeline.pipes[0] && pipe.type
    }
    if (pipeline.output) {
        return pipeline.output.type
    }

    return null
}
export function pipelineOutputType(pipeline: Pipeline, logger?: Logger): string | null {
    if (pipeline.output) {
        return null
    }

    if (pipeline.pipes.length > 0) {
        const lastIndex = pipeline.pipes.length - 1

        const pipe = getPipe(pipeline.pipes[lastIndex])
        if (!pipe) {
            logger?.debug(`Failed to find output type for pipeline ${pipelineToString(pipeline)}`)
            return null
        }
        return pipeline.pipes[lastIndex] && pipe.baseType
    }

    return null
}

export async function buildPipeline(pipeline: Pipeline, logger?: Logger, rendererOptions?: unknown): Promise<Pipe | null> {
    const pipesInfo = await gatherPipeInfo()
    const pipes = pipeline.pipes.map(pipe => getPipe(pipe))

    for (const pipe of pipes) {
        if (!pipe) {
            logger?.debug(`Failed to get a pipe inside pipeline ${pipelineToString(pipeline)}`)
            return null
        }

        let pipeInfo = pipesInfo.get(pipe)
        if (!pipeInfo) {
            pipeInfo = await pipe.getInfo()
            logger?.debug(`Querying pipe info for ${pipeName(pipe)}: ${JSON.stringify(pipeInfo)}`)
        }

        if (!pipeInfo.environmentSupported) {
            logger?.debug(`Failed to create pipeline ${pipelineToString(pipeline)} because pipe ${pipeName(pipe)} is not supported!`)
            return null
        }
    }

    return buildPipelineNoCheck(pipeline, logger, rendererOptions)
}

export function buildPipelineNoCheck(pipeline: Pipeline, logger?: Logger, rendererOptions?: unknown): Pipe | null {
    let previousPipeStatic: OutputPipeStatic | PipeStatic | null = null
    let pipe = null
    if (pipeline.output) {
        const base = pipeline.output

        previousPipeStatic = base
        pipe = new base(logger, rendererOptions)
    }

    for (let index = pipeline.pipes.length - 1; index >= 0; index--) {
        const currentPipeValue = pipeline.pipes[index]
        const currentPipe = getPipe(currentPipeValue)

        if (!currentPipe) {
            logger?.debug(`Failed to construct pipe because it isn't registered: ${pipeName(currentPipeValue)}`)
            return null
        }

        if (previousPipeStatic && currentPipe.baseType != previousPipeStatic.type) {
            logger?.debug(`Failed to create pipeline "${pipelineToString(pipeline)}" because baseType of "${pipeName(currentPipe)}" is "${currentPipe.baseType}", but it's trying to connect with "${previousPipeStatic.type}"`)
            return null
        }

        previousPipeStatic = currentPipe
        pipe = new currentPipe(pipe, logger)
    }

    return pipe
}

export function getRootPipe(pipe: Pipe): Pipe {
    let base = pipe
    let newBase = null

    while ((newBase = base.getBase())) {
        base = newBase
    }

    return base
}

let PIPE_INFO: Promise<Map<PipeStatic, PipeInfo>> | null

export function gatherPipeInfo(): Promise<Map<PipeStatic, PipeInfo>> {
    if (PIPE_INFO) {
        return PIPE_INFO
    } else {
        PIPE_INFO = gatherPipeInfoInternal()
        return PIPE_INFO
    }
}
async function gatherPipeInfoInternal(): Promise<Map<PipeStatic, PipeInfo>> {
    const map = new Map()

    const promises = []

    const all: Array<PipeStatic> = pipes()
    for (const pipe of all) {
        promises.push(pipe.getInfo().then(info => {
            map.set(pipe, info)
        }))
    }

    await Promise.all(promises)

    return map
}

export function pipes(): Array<PipeStatic> {
    return [
        // Worker
        WorkerVideoFrameSendPipe,
        WorkerVideoFrameReceivePipe,
        WorkerDataSendPipe,
        WorkerDataReceivePipe,
        WorkerVideoTrackSendPipe,
        WorkerVideoTrackReceivePipe,
        WorkerVideoDataSendPipe,
        WorkerVideoDataReceivePipe,
        // Video
        DepacketizeVideoPipe,
        VideoMediaStreamTrackGeneratorPipe,
        VideoMediaStreamTrackProcessorPipe,
        VideoDecoderPipe,
        OpenH264DecoderPipe,
        Yuv420ToRgbaFramePipe,
        MediaSourceDecoder,
        VideoTrackGeneratorPipe,
        CanvasFrameDrawPipe,
        CanvasYuv420FrameDrawPipe,
        CanvasRgbaFrameDrawPipe,
        // Video Worker pipes
        WorkerVideoMediaStreamProcessorPipe,
        WorkerOffscreenCanvasSendPipe,
        WorkerVideoMediaStreamProcessorCanvasPipe,
        WorkerDataToVideoTrackPipe,
        WorkerDataToCanvasGlRenderOpenH264Pipe,
        // Audio
        DepacketizeAudioPipe,
        AudioMediaStreamTrackGeneratorPipe,
        AudioDecoderPipe,
        OpusAudioDecoderPipe,
        AudioBufferPipe,
        AudioContextTrackPipe,
    ]
}
