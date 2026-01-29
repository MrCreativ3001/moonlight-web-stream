import { Component } from "../../component/index.js"
import { StreamSupportedVideoCodecs } from "../../api_bindings.js"
import { Pipe } from "../pipeline/index.js"

export type VideoRendererSetup = {
    codec: keyof typeof StreamSupportedVideoCodecs,
    width: number
    height: number
    fps: number
}

export interface VideoRenderer extends Component, Pipe {
    readonly implementationName: string

    /// Returns the success
    setup(setup: VideoRendererSetup): Promise<void>
    cleanup(): void

    /// Only works on web socket pipeline currently
    pollRequestIdr(): boolean

    /// Don't work inside a worker
    onUserInteraction(): void
    /// Don't work inside a worker
    getStreamRect(): DOMRect

    /// Don't work inside a worker
    mount(parent: HTMLElement): void
    /// Don't work inside a worker
    unmount(parent: HTMLElement): void
    
    /// Optional: Set HDR mode (enabled/disabled)
    setHdrMode?(enabled: boolean): void
}

export function getStreamRectCorrected(boundingRect: DOMRect, videoSize: [number, number]): DOMRect {
    const videoAspect = videoSize[0] / videoSize[1]

    const boundingRectAspect = boundingRect.width / boundingRect.height

    let x = boundingRect.x
    let y = boundingRect.y
    let videoMultiplier
    if (boundingRectAspect > videoAspect) {
        // How much is the video scaled up
        videoMultiplier = boundingRect.height / videoSize[1]

        // Note: Both in boundingRect / page scale
        const boundingRectHalfWidth = boundingRect.width / 2
        const videoHalfWidth = videoSize[0] * videoMultiplier / 2

        x += boundingRectHalfWidth - videoHalfWidth
    } else {
        // Same as above but inverted
        videoMultiplier = boundingRect.width / videoSize[0]

        const boundingRectHalfHeight = boundingRect.height / 2
        const videoHalfHeight = videoSize[1] * videoMultiplier / 2

        y += boundingRectHalfHeight - videoHalfHeight
    }

    return new DOMRect(
        x,
        y,
        videoSize[0] * videoMultiplier,
        videoSize[1] * videoMultiplier
    )
}

export interface TrackVideoRenderer extends Pipe {
    // static readonly type = "videotrack"

    setTrack(track: MediaStreamTrack): void
}

export interface FrameVideoRenderer extends Pipe {
    // static readonly type = "videoframe"

    /// Submits a frame. This renderer now "owns" the frame and needs to clean it up via close.
    submitFrame(frame: VideoFrame): void
}

export type VideoDecodeUnit = {
    type: "key" | "delta"
    timestampMicroseconds: number
    durationMicroseconds: number
    /*
      Contains the data for one frame:
      - H264:
        - keyframe: Must contain sps,pps,idr(one or multiple)
        - delta: Must contain the whole frame(one or multiple CodecSliceNonIdr's)
      - H265:
        - keyframe: Must contain sps,pps,idr(one or multiple)
        - delta: Must contain the whole frame(one or multiple CodecSliceNonIdr's)
    */
    data: ArrayBuffer
}

export interface DataVideoRenderer extends Pipe {
    // static readonly type = "videodata"

    /// Data like https://github.com/moonlight-stream/moonlight-common-c/blob/b126e481a195fdc7152d211def17190e3434bcce/src/Limelight.h#L298
    submitDecodeUnit(unit: VideoDecodeUnit): void
}