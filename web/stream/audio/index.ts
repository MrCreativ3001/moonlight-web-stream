import { Component } from "../../component/index.js"
import { Pipe } from "../pipeline/index.js"

export type AudioPlayerSetup = {
    channels: number
    sampleRate: number
    streams: number
    coupledStreams: number
    samplesPerFrame: number
    mapping: Array<number>
}

export interface AudioPlayer extends Component, Pipe {
    readonly implementationName: string

    setup(setup: AudioPlayerSetup): void
    cleanup(): void

    onUserInteraction(): void

    mount(parent: HTMLElement): void
    unmount(parent: HTMLElement): void
}

export interface TrackAudioPlayer extends Pipe {
    // static readonly type = "audiotrack"

    setTrack(track: MediaStreamTrack): void
}

export type AudioDecodeUnit = {
    timestampMicroseconds: number
    durationMicroseconds: number
    data: ArrayBuffer
}

export interface DataAudioPlayer extends Pipe {
    // static readonly type = "audiodata"

    // Data like https://github.com/moonlight-stream/moonlight-common-c/blob/b126e481a195fdc7152d211def17190e3434bcce/src/Limelight.h#L356
    // The unit is only borrowed
    decodeAndPlay(unit: AudioDecodeUnit): void
}

export interface SampleAudioPlayer extends Pipe {
    // static readonly type = "audiosample"

    submitSample(sample: AudioData): void
}

export type AudioPcmUnit = {
    timestampMicroseconds: number
    durationMicroseconds: number
    channelData: Array<Float32Array>
}

export interface PcmAudioPlayer extends Pipe {
    // static readonly type = "audiopcm"

    // The unit is only borrowed
    playPcm(unit: AudioPcmUnit): void
}

export interface NodeAudioPlayer extends Pipe {
    // static readonly type = "audionode"

    setSource(source: AudioNode): void

    getAudioContext(): AudioContext
}