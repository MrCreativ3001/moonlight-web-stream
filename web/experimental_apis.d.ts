declare global {
    interface Navigator {
        // Keyboard Lock: https://developer.mozilla.org/en-US/docs/Web/API/Keyboard/lock
        keyboard: {
            lock(): Promise<void>;
            unlock(): void;
        };
    }

    // MediaStreamTrackProcessor: https://developer.mozilla.org/en-US/docs/Web/API/MediaStreamTrackProcessor
    interface MediaStreamTrackProcessor {
        readonly readable: ReadableStream<VideoFrame>
    }

    var MediaStreamTrackProcessor: {
        prototype: MediaStreamTrackProcessor
        new(options: { track: MediaStreamTrack, maxBufferSize?: number }): MediaStreamTrackProcessor
        new(): MediaStreamTrackProcessor
    }

    // MediaStreamTrackGenerator: https://developer.mozilla.org/en-US/docs/Web/API/MediaStreamTrackGenerator
    interface MediaStreamTrackGenerator extends MediaStreamTrack {
        readonly writable: WritableStream<VideoFrame | AudioData>
    }

    var MediaStreamTrackGenerator: {
        prototype: MediaStreamTrackGenerator
        new(options: { kind: "audio" | "video" }): MediaStreamTrackGenerator
    }

    // VideoTrackGenerator: https://developer.mozilla.org/en-US/docs/Web/API/VideoTrackGenerator
    interface VideoTrackGenerator {
        readonly muted: boolean
        readonly track: MediaStreamTrack
        readonly writable: WritableStream<VideoFrame>
    }

    var VideoTrackGenerator: {
        prototype: VideoTrackGenerator
        new(): VideoTrackGenerator
    }
}


export { };