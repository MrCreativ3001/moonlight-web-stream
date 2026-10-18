import { AudioDecoderPipe } from "./audio_decoder_pipe"
import { AudioElementPlayer } from "./audio_element"
import { AudioMediaStreamTrackGeneratorPipe } from "./media_stream_track_generator_pipe"
import { Logger } from "../log"
import { gatherPipeInfo, getPipe, OutputPipeStatic, PipeInfoStatic, Pipeline, pipelineInputType, pipelineToString, pipeName } from "../pipeline/index"
import { OpusAudioDecoderPipe } from "./opus_decoder_pipe"
import { AudioBufferPipe as AudioPcmBufferPipe } from "./audio_buffer_pipe"
import { ContextDestinationNodeAudioPlayer } from "./audio_context_destination"
import { AudioContextTrackPipe } from "./audio_context_track_pipe"
import { DepacketizeAudioPipe } from "./depacketize_pipe"

const AUDIO_PLAYERS: Array<AudioPlayerStatic> = [
    AudioElementPlayer,
    ContextDestinationNodeAudioPlayer
]

interface AudioPlayerStatic extends PipeInfoStatic, OutputPipeStatic {
    readonly pipeName: string
}

export type AudioPipelineOptions = {
}

const PIPELINES: Array<Pipeline> = [
    // Convert track -> audio_element, All Browsers
    { pipes: [], output: AudioElementPlayer },
    // Convert data -> audio_sample -> track (MediaStreamTrackGenerator) -> audio_element, Chromium
    { pipes: [DepacketizeAudioPipe, AudioDecoderPipe, AudioMediaStreamTrackGeneratorPipe], output: AudioElementPlayer },
    // Convert data -> audio_sample -> audio_sample_pcm -> audio_context_element -> audio_element, Safari / Firefox
    { pipes: [DepacketizeAudioPipe, OpusAudioDecoderPipe, AudioPcmBufferPipe, AudioContextTrackPipe], output: AudioElementPlayer },
    // Convert data -> audio_sample -> audio_sample_pcm -> audio_context_element -> audio_element, Safari / Firefox
    { pipes: [DepacketizeAudioPipe, OpusAudioDecoderPipe, AudioPcmBufferPipe], output: ContextDestinationNodeAudioPlayer },
]

export async function selectAudioPipeline(type: string, settings: AudioPipelineOptions, logger?: Logger): Promise<Pipeline | null> {
    const pipesInfo = await gatherPipeInfo()

    if (logger) {
        // Print supported pipes
        const audioPlayerInfoPromises = []
        for (const audioPlayer of AUDIO_PLAYERS) {
            audioPlayerInfoPromises.push(audioPlayer.getInfo().then(info => [pipeName(audioPlayer), info]))
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

    logger?.debug(`Building audio pipeline with input "${type}" and settings ${JSON.stringify(settings)}`)

    let pipelines = PIPELINES

    pipelineLoop: for (const pipeline of pipelines) {
        const inputType = pipelineInputType(pipeline)
        if (inputType != type) {
            continue
        }

        // Check if supported
        for (const pipe of pipeline.pipes) {
            const pipeStatic = getPipe(pipe)
            if (!pipeStatic) {
                logger?.debug(`Failed to find pipe ${pipeName(pipe)} whilst querying video pipeline info`)
                continue
            }

            const pipeInfo = pipesInfo.get(pipeStatic)
            if (!pipeInfo) {
                logger?.debug(`Failed to query info for audio pipe ${pipeName(pipe)}`)
                continue pipelineLoop
            }

            if (!pipeInfo.environmentSupported) {
                continue pipelineLoop
            }
        }

        const playerInfo = await pipeline.output?.getInfo()
        if (!playerInfo) {
            logger?.debug(`Failed to query info for audio player ${pipelineToString(pipeline)}`)
            continue pipelineLoop
        }

        if (!playerInfo.environmentSupported) {
            continue pipelineLoop
        }

        return pipeline
    }

    logger?.debug("No supported audio player found!")
    return null
}
