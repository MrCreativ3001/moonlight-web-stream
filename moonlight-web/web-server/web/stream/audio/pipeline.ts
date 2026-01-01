import { AudioPlayer, DataAudioPlayer, TrackAudioPlayer } from "./index.js"
import { AudioDecoderPipe } from "./audio_decoder_pipe.js"
import { AudioElementPlayer } from "./audio_element.js"
import { AudioMediaStreamTrackGeneratorPipe } from "./media_stream_track_generator_pipe.js"
import { Logger } from "../log.js"
import { buildPipeline, gatherPipeInfo, OutputPipeStatic, PipeInfoStatic, PipeStatic } from "../pipeline/index.js"

// TODO: print info
const AUDIO_PLAYERS: Array<AudioPlayerStatic> = [
    AudioElementPlayer
]

type PipelineResult<T> = { audioPlayer: T, error: false } | { audioPlayer: null, error: true }

interface AudioPlayerStatic extends PipeInfoStatic, OutputPipeStatic { }

export type AudioPipelineOptions = {
}

type Pipeline = { input: string, pipes: Array<PipeStatic>, player: AudioPlayerStatic }

const PIPELINES: Array<Pipeline> = [
    { input: "audiotrack", pipes: [], player: AudioElementPlayer },
    { input: "data", pipes: [AudioDecoderPipe, AudioMediaStreamTrackGeneratorPipe], player: AudioElementPlayer }
]

export function buildAudioPipeline(type: "audiotrack", settings: AudioPipelineOptions, logger?: Logger): Promise<PipelineResult<TrackAudioPlayer & AudioPlayer>>
export function buildAudioPipeline(type: "data", settings: AudioPipelineOptions, logger?: Logger): Promise<PipelineResult<DataAudioPlayer & AudioPlayer>>

export async function buildAudioPipeline(type: string, settings: AudioPipelineOptions, logger?: Logger): Promise<PipelineResult<AudioPlayer>> {
    logger?.debug(`Building audio pipeline with output "${type}"`)

    const pipesInfo = await gatherPipeInfo()

    const pipelines = PIPELINES

    // TODO: use the depacketize pipe
    // TODO: create a opus decoder using other js sound apis

    pipelineLoop: for (const pipeline of pipelines) {
        if (pipeline.input != type) {
            continue
        }

        // Check if supported
        for (const pipe of pipeline.pipes) {
            const pipeInfo = pipesInfo.get(pipe)
            if (!pipeInfo) {
                logger?.debug(`Failed to query info for audio pipe ${pipe.name}`)
                continue pipelineLoop
            }

            if (!pipeInfo.environmentSupported) {
                continue pipelineLoop
            }
        }

        const playerInfo = await pipeline.player.getInfo()
        if (!playerInfo) {
            logger?.debug(`Failed to query info for audio player ${pipeline.player.name}`)
            continue pipelineLoop
        }

        if (!playerInfo.environmentSupported) {
            continue pipelineLoop
        }

        // Build that pipeline
        const audioPlayer = buildPipeline(pipeline.player, { pipes: pipeline.pipes }, logger)
        if (!audioPlayer) {
            logger?.debug("Failed to build audio pipeline")
            return { audioPlayer: null, error: true }
        }

        return { audioPlayer: audioPlayer as AudioPlayer, error: false }
    }

    logger?.debug("No supported audio player found!")
    return { audioPlayer: null, error: true }
}