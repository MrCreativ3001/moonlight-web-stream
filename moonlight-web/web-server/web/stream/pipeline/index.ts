import { AudioDecoderPipe } from "../audio/audio_decoder_pipe.js";
import { DepacketizeAudioPipe } from "../audio/depacketize_pipe.js";
import { AudioMediaStreamTrackGeneratorPipe } from "../audio/media_stream_track_generator_pipe.js";
import { Logger } from "../log.js";
import { VideoCodecSupport } from "../video.js";
import { DepacketizeVideoPipe } from "../video/depackitize_video_pipe.js";
import { VideoMediaStreamTrackGeneratorPipe } from "../video/media_stream_track_generator_pipe.js";
import { VideoMediaStreamTrackProcessorPipe } from "../video/media_stream_track_processor_pipe.js";
import { WorkerDataToVideoTrackPipe, WorkerVideoMediaStreamProcessorCanvasPipe, WorkerVideoMediaStreamProcessorPipe } from "../video/pipeline.js";
import { VideoDecoderPipe } from "../video/video_decoder_pipe.js";
import { VideoTrackGeneratorPipe } from "../video/video_track_generator.js";
import { WorkerDataReceivePipe, WorkerDataSendPipe, WorkerOffscreenCanvasSendPipe, WorkerVideoFrameReceivePipe, WorkerVideoFrameSendPipe, WorkerVideoTrackReceivePipe, WorkerVideoTrackSendPipe } from "./worker_io.js";

export function globalObject(): any {
    if (typeof self !== 'undefined') {
        return self
    }

    if (typeof window !== 'undefined') {
        return window
    }

    return globalThis;
}

export interface Pipe {
    readonly implementationName: string

    getBase(): Pipe | null
}

export type PipeInfo = {
    environmentSupported: boolean
    supportedVideoCodecs?: VideoCodecSupport
}

export interface PipeInfoStatic {
    getInfo(): Promise<PipeInfo>
}
export interface PipeStatic extends PipeInfoStatic, InputPipeStatic {
    readonly type: string

    new(base: any, logger?: Logger): Pipe
}

export interface InputPipeStatic {
    readonly baseType: string
}
export interface OutputPipeStatic {
    readonly type: string

    new(logger?: Logger): Pipe
}

export type Pipeline = {
    pipes: Array<string | PipeStatic>
}

export function pipelineToString(pipeline: Pipeline): string {
    return pipeline.pipes.map(pipe => pipeName(pipe)).join(" -> ")
}

export function pipeName(pipe: string | PipeStatic): string {
    if (typeof pipe == "string") {
        return pipe
    } else {
        return pipe.name
    }
}
export function getPipe(pipe: string | PipeStatic): PipeStatic | null {
    if (typeof pipe == "string") {
        const foundPipe = pipes().find(check => check.name == pipe)

        return foundPipe ?? null
    } else {
        return pipe
    }
}

export function buildPipeline(base: OutputPipeStatic, pipeline: Pipeline, logger?: Logger): Pipe | null {
    let previousPipeStatic = base
    let pipe = new base(logger)

    for (let index = pipeline.pipes.length - 1; index >= 0; index--) {
        const currentPipeValue = pipeline.pipes[index]
        const currentPipe = getPipe(currentPipeValue)

        if (!currentPipe) {
            logger?.debug(`Failed to construct pipe because it isn't registered: ${pipeName(currentPipeValue)}`)
            return null
        }

        if (previousPipeStatic && currentPipe.baseType != previousPipeStatic.type) {
            logger?.debug(`Failed to create pipeline "${pipelineToString(pipeline)}" because baseType of "${currentPipe.name}" is "${currentPipe.baseType}", but it's trying to connect with "${previousPipeStatic.type}"`)
            return null
        }

        previousPipeStatic = currentPipe
        pipe = new currentPipe(pipe, logger)
    }

    return pipe
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
        // Video
        DepacketizeVideoPipe,
        VideoMediaStreamTrackGeneratorPipe,
        VideoMediaStreamTrackProcessorPipe,
        VideoDecoderPipe,
        VideoTrackGeneratorPipe,
        // Video Worker pipes
        WorkerVideoMediaStreamProcessorPipe,
        WorkerOffscreenCanvasSendPipe,
        WorkerVideoMediaStreamProcessorCanvasPipe,
        WorkerDataToVideoTrackPipe,
        // Audio
        DepacketizeAudioPipe,
        AudioMediaStreamTrackGeneratorPipe,
        AudioDecoderPipe,
    ]
}
