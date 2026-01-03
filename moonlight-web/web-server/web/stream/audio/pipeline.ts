import { AudioPlayer, DataAudioPlayer, TrackAudioPlayer } from "./index.js"
import { AudioDecoderPipe } from "./audio_decoder_pipe.js"
import { AudioElementPlayer } from "./audio_element.js"
import { AudioMediaStreamTrackGeneratorPipe } from "./media_stream_track_generator_pipe.js"
import { Logger } from "../log.js"
import { buildPipeline, gatherPipeInfo, OutputPipeStatic, PipeInfoStatic, PipeStatic } from "../pipeline/index.js"
import { AudioDecoderPcmPipe } from "./audio_decoder_pcm_pipe.js"
import { AudioBufferPipe as AudioPcmBufferPipe } from "./audio_buffer_pipe.js"
import { ContextDestinationNodeAudioPlayer } from "./audio_context_destination.js"
import { AudioContextTrackPipe } from "./audio_context_track_pipe.js"

const AUDIO_PLAYERS: Array<AudioPlayerStatic> = [
    AudioElementPlayer,
    ContextDestinationNodeAudioPlayer
]

type PipelineResult<T> = { audioPlayer: T, error: false } | { audioPlayer: null, error: true }

interface AudioPlayerStatic extends PipeInfoStatic, OutputPipeStatic { }

export type AudioPipelineOptions = {
}

type Pipeline = { input: string, pipes: Array<PipeStatic>, player: AudioPlayerStatic }

const PIPELINES: Array<Pipeline> = [
    // Convert track -> audio_element, All Browsers
    { input: "audiotrack", pipes: [], player: AudioElementPlayer },
    // Convert data -> audio_sample -> track (MediaStreamTrackGenerator) -> audio_element, Chromium
    { input: "data", pipes: [AudioDecoderPipe, AudioMediaStreamTrackGeneratorPipe], player: AudioElementPlayer },
    // Convert data -> audio_sample -> audio_sample_pcm -> audio_context_element -> audio_element, Safari / Firefox
    // TODO: fix this the audio context track pipe on firefox
    // { input: "data", pipes: [AudioDecoderPcmPipe, AudioPcmBufferPipe, AudioContextTrackPipe], player: AudioElementPlayer },
    // Convert data -> audio_sample -> audio_sample_pcm -> audio_context_element -> audio_element, Safari / Firefox
    { input: "data", pipes: [AudioDecoderPcmPipe, AudioPcmBufferPipe], player: ContextDestinationNodeAudioPlayer }
]

export function buildAudioPipeline(type: "audiotrack", settings: AudioPipelineOptions, logger?: Logger): Promise<PipelineResult<TrackAudioPlayer & AudioPlayer>>
export function buildAudioPipeline(type: "data", settings: AudioPipelineOptions, logger?: Logger): Promise<PipelineResult<DataAudioPlayer & AudioPlayer>>

export async function buildAudioPipeline(type: string, settings: AudioPipelineOptions, logger?: Logger): Promise<PipelineResult<AudioPlayer>> {
    const pipesInfo = await gatherPipeInfo()

    if (logger) {
        // Print supported pipes
        const audioPlayerInfoPromises = []
        for (const audioPlayer of AUDIO_PLAYERS) {
            audioPlayerInfoPromises.push(audioPlayer.getInfo().then(info => [audioPlayer.name, info]))
        }
        const audioPlayerInfo = await Promise.all(audioPlayerInfoPromises)

        logger.debug(`Supported Audio Players: {`)
        let isFirst = true
        for (const [name, info] of audioPlayerInfo) {
            logger.debug(`${isFirst ? "" : ","}"${name}": ${JSON.stringify(info)}`)
            isFirst = false
        }
        logger.debug(`}`)
    }

    logger?.debug(`Building audio pipeline with output "${type}"`)

    let pipelines = PIPELINES

    // TODO: use the depacketize pipe

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