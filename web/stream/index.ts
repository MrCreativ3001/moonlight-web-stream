import { Api, apiHostCancel, apiWebRTCConfiguration, apiWebRTCOffer } from "../api"
import { Component } from "../component/index"
import { Settings, TransportType } from "../component/settings_menu"
import { ControlPacket, ControlPacket_Tags, VideoFormats } from "../uniffi/moonlight_common_bindings"
import { globalObject, wait } from "../util"
import { AudioPlayer, AudioPlayerSetup } from "./audio/index"
import { buildAudioPipeline } from "./audio/pipeline"
import { defaultStreamInputConfig, StreamInput } from "./input"
import { Logger, LogMessageInfo } from "./log"
import { gatherPipeInfo, pipeName } from "./pipeline/index"
import { StreamStats } from "./stats"
import { Transport, TransportAudioType, TransportConnectData, TransportOptions, TransportShutdown, TransportVideoType } from "./transport/index"
import { WebSocketTransport } from "./transport/web_socket"
import { WebRTCTransport } from "./transport/webrtc"
import { allVideoCodecs, andVideoCodecs, emptyVideoCodecs, hasAnyCodec } from "./video"
import { VideoRenderer, VideoRendererSetup } from "./video/index"
import { buildVideoPipeline, queryVideoPipelineInfo, VideoPipelineOptions } from "./video/pipeline"

export type ExecutionEnvironment = {
    main: boolean
    worker: boolean
}

export type StreamCapabilities = {
    touch: boolean
}

export type InfoEvent = CustomEvent<
    { type: "app", appName: string } |
    { type: "connectionComplete", capabilities: StreamCapabilities } |
    { type: "videoReady" } |
    { type: "addDebugLine", line: string, additional?: LogMessageInfo }
>
export type InfoEventListener = (event: InfoEvent) => void

export function getStreamerSize(settings: Settings, viewerScreenSize: [number, number]): [number, number] {
    let width, height
    if (settings.videoSize == "720p") {
        width = 1280
        height = 720
    } else if (settings.videoSize == "1080p") {
        width = 1920
        height = 1080
    } else if (settings.videoSize == "1440p") {
        width = 2560
        height = 1440
    } else if (settings.videoSize == "4k") {
        width = 3840
        height = 2160
    } else if (settings.videoSize == "custom") {
        width = settings.videoSizeCustom.width
        height = settings.videoSizeCustom.height
    } else { // native
        width = viewerScreenSize[0]
        height = viewerScreenSize[1]
    }
    return [width, height]
}

function getVideoCodecHint(settings: Settings): VideoFormats {
    let videoCodecHint = emptyVideoCodecs()
    const chroma444 = settings.yuv444 ?? false
    if (settings.videoCodec == "h264") {
        videoCodecHint.h264 = true
        videoCodecHint.h264High8444 = chroma444
    } else if (settings.videoCodec == "h265") {
        videoCodecHint.h265 = true
        videoCodecHint.h265Main10 = true
        videoCodecHint.h265Rext8444 = chroma444
        videoCodecHint.h265Rext10444 = chroma444
    } else if (settings.videoCodec == "av1") {
        videoCodecHint.av1Main8 = true
        videoCodecHint.av1Main10 = true
        videoCodecHint.av1High8444 = chroma444
        videoCodecHint.av1High10444 = chroma444
    } else if (settings.videoCodec == "auto") {
        videoCodecHint = allVideoCodecs()
        if (!chroma444) {
            videoCodecHint.h264High8444 = false
            videoCodecHint.h265Rext8444 = false
            videoCodecHint.h265Rext10444 = false
            videoCodecHint.av1High8444 = false
            videoCodecHint.av1High10444 = false
        }
    }

    if (isFirefox()) {
        videoCodecHint.av1Main8 = false
        videoCodecHint.av1Main10 = false
    }

    return videoCodecHint
}

function isFirefox(): boolean {
    return navigator.userAgent.includes("Firefox/")
}

const WEBRTC_CONNECT_TIMEOUT_MS = 15000
const FALLBACK_RECONNECT_DELAY_MS = 500

export class Stream implements Component {
    private logger: Logger = new Logger()

    private api: Api

    private hostId: number
    private appId: number

    private settings: Settings

    private divElement = document.createElement("div")
    private eventTarget = new EventTarget()

    private transportOverride: TransportType | null = null

    private videoRenderer: VideoRenderer | null = null
    private audioPlayer: AudioPlayer | null = null

    private input: StreamInput
    private stats: StreamStats

    private streamerSize: [number, number]

    constructor(api: Api, hostId: number, appId: number, settings: Settings, viewerScreenSize: [number, number]) {
        this.logger.addInfoListener((info, type) => {
            this.debugLog(info, { type: type ?? undefined })
        })

        this.api = api

        this.hostId = hostId
        this.appId = appId

        this.settings = settings

        this.streamerSize = getStreamerSize(settings, viewerScreenSize)

        // Stream Input
        const streamInputConfig = defaultStreamInputConfig()
        Object.assign(streamInputConfig, {
            mouseMode: this.settings.mouseMode,
            mouseScrollMode: this.settings.mouseScrollMode,
            touchMode: this.settings.touchMode,
            localCursorSensitivity: this.settings.localCursorSensitivity,
            controllerConfig: this.settings.controllerConfig,
            swapMouseButtons: this.settings.swapMouseButtons,
            reverseScrollDirection: this.settings.reverseScrollDirection
        })
        this.input = new StreamInput(streamInputConfig)

        // Stream Stats
        this.stats = new StreamStats(this.logger)

        this.startConnection()
    }

    private debugLog(message: string, additional?: LogMessageInfo) {
        for (const line of message.split("\n")) {
            const event: InfoEvent = new CustomEvent("stream-info", {
                detail: { type: "addDebugLine", line, additional }
            })

            this.eventTarget.dispatchEvent(event)
        }
    }

    private isStopped = false

    async startConnection() {
        while (!this.isStopped) {
            const wasConnected = await this.connectOnce()

            if (this.isStopped) {
                return
            }
            if (!wasConnected) {
                this.debugLog("Tried all configured transport options but no connection was possible", { type: "fatal" })
                return
            }

            this.debugLog("Connection lost, reconnecting...", { type: "ifErrorDescription" })
            await wait(FALLBACK_RECONNECT_DELAY_MS)
        }
    }

    // Returns true when a connection was established and then lost; false when it never connected.
    private async connectOnce(): Promise<boolean> {
        const desiredTransport = this.transportOverride ?? this.settings.dataTransport
        this.debugLog(`Using transport: ${desiredTransport}`)

        let shutdownReason: TransportShutdown | undefined
        if (desiredTransport == "auto") {
            shutdownReason = await this.tryWebRTCTransport()

            if (shutdownReason == "failednoconnect") {
                this.debugLog("Failed to establish WebRTC connection. Falling back to Web Socket transport.", { type: "ifErrorDescription" })
                shutdownReason = await this.tryWebSocketTransport()
            }
        } else if (desiredTransport == "webrtc") {
            shutdownReason = await this.tryWebRTCTransport()
        } else if (desiredTransport == "websocket") {
            shutdownReason = await this.tryWebSocketTransport()
        }

        return shutdownReason == "failed" || shutdownReason == "disconnect"
    }

    private transport: Transport | null = null

    private setTransport(transport: Transport) {
        if (this.isStopped) {
            // A transport that finishes connecting after stop() began would
            // otherwise stay open since stop() already captured its close promise
            this.debugLog("Closing transport that connected after stop")
            transport.close()
            return
        }
        if (this.transport) {
            this.debugLog("Closing old transport")
            this.transport.close()
        }
        this.debugLog("Setting new transport")

        this.transport = transport

        this.input.setControlStream(this.transport.controlStream)
        this.stats.setTransport(this.transport)
    }

    private async createTransportOptions(): Promise<TransportOptions | null> {
        const codecHint = getVideoCodecHint(this.settings)

        const dataCodecs = await this.queryVideoCodecs("data")

        if (!hasAnyCodec(codecHint)) {
            this.debugLog("Couldn't find any supported video format. Change the codec option to H264 in the settings if you're unsure which codecs are supported.", { type: "fatalDescription" })
            return null
        }

        return {
            hostId: this.hostId,
            appId: this.appId,
            width: this.streamerSize[0],
            height: this.streamerSize[1],
            fps: this.settings.fps,
            bitrate: this.settings.bitrate,
            hdr: this.settings.hdr,
            localAudioPlayMode: this.settings.playAudioLocal,
            supportedCodecs: dataCodecs,
            preferredCodecs: codecHint,
        }
    }

    private async tryWebRTCTransport(): Promise<TransportShutdown> {
        this.debugLog("Trying WebRTC transport")

        let config: Awaited<ReturnType<typeof apiWebRTCConfiguration>>
        let options: Awaited<ReturnType<typeof this.createTransportOptions>>
        try {
            [config, options] = await Promise.all([
                apiWebRTCConfiguration(this.api),
                this.createTransportOptions(),
            ])
        } catch (error) {
            this.debugLog(`failed to prepare WebRTC connection because ${error}`)
            return "failednoconnect"
        }

        this.debugLog("Received WebRTC config and transport options")

        if (!options) {
            return "failednoconnect"
        }

        // Create transport
        this.debugLog("Creating WebRTC transport")
        const transport = new WebRTCTransport(
            this.api,
            {
                iceServers: config.iceServers,
            },
            this.logger
        )
        transport.controlStream.onreceive = this.boundReceivePacket

        const onConnect = new Promise<TransportConnectData>(resolve => {
            transport.onconnect = resolve
        })
        const onClose = new Promise<TransportShutdown>(resolve => {
            transport.onclose = resolve
        })

        try {
            // Create offer
            const offer = await transport.createOffer(options)

            // Send Request
            this.debugLog("Sending Offer and waiting for Answer")
            const answer = await apiWebRTCOffer(this.api, offer)
            this.debugLog("Got Response")

            // Apply answer
            await transport.setAnswer(answer)
        } catch (error) {
            this.debugLog(`failed to connect using webrtc because ${error}`)

            await transport.close()
            return "failednoconnect"
        }

        // Set Transport
        this.setTransport(transport)

        // Wait for negotiation, but don't let a stuck ICE check block fallback forever.
        const onTimeout: Promise<TransportShutdown> =
            wait(WEBRTC_CONNECT_TIMEOUT_MS)
                .then(() => "failednoconnect")

        const connectData: TransportShutdown | TransportConnectData = await Promise.race([
            onConnect,
            onClose,
            onTimeout,
        ])
        if (typeof connectData == "string") {
            this.debugLog(`webrtc connection failed: ${connectData}`)
            await transport.close()
            // connection failed
            return connectData
        }

        // -- Connection successful
        await this.onConnect(connectData)

        return await onClose
    }
    private async tryWebSocketTransport() {
        this.debugLog("Trying Web Socket transport")

        const options = await this.createTransportOptions()
        if (!options) {
            return
        }

        const transport = new WebSocketTransport(this.api, this.logger)

        // Add listeners
        transport.controlStream.onreceive = this.boundReceivePacket

        const onConnect = new Promise<TransportConnectData>(resolve => {
            transport.onconnect = resolve
        })
        const onClose = new Promise<TransportShutdown>(resolve => {
            transport.onclose = resolve
        })

        // Start stream
        await transport.startStream(options)

        this.setTransport(transport)

        const connectData = await Promise.race([
            onConnect,
            onClose,
        ])

        if (typeof connectData == "string") {
            this.debugLog(`web socket connection failed: ${connectData}`)
            await transport.close()
            // connection failed
            return connectData
        }

        // -- Connection successful
        this.onConnect(connectData)

        return await onClose
    }

    private async onConnect(connectData: TransportConnectData) {
        this.logger.debug("connected successfully, creating video and audio pipelines")

        // Dispatch app event
        let event: InfoEvent = new CustomEvent("stream-info", {
            detail: {
                type: "app", appName: connectData.appName
            }
        })
        this.eventTarget.dispatchEvent(event)

        // Set input
        this.input.onStreamStart(connectData.capabilities, [connectData.videoSetup.width, connectData.videoSetup.height])

        // Create pipelines
        await this.createPipelines(connectData)

        event = new CustomEvent("stream-info", {
            detail: {
                type: "connectionComplete", capabilities: {
                    // TODO
                    touch: true
                }
            }
        })
        this.eventTarget.dispatchEvent(event)

        this.startConnectionWarnings()
        this.updateWakeLock()
    }

    // -- Connection quality warnings

    private connectionWarningIntervalId: number | null = null
    private lastWarningStats: { packetsLost: number | null, packetsReceived: number | null, framesDropped: number | null } | null = null
    private lastWarningTime = 0

    private startConnectionWarnings() {
        if (this.connectionWarningIntervalId != null) {
            clearInterval(this.connectionWarningIntervalId)
            this.connectionWarningIntervalId = null
        }
        this.lastWarningStats = null
        this.lastWarningTime = 0

        if (!this.settings.showConnectionWarnings) {
            return
        }

        this.connectionWarningIntervalId = globalObject().setInterval(this.checkConnectionQuality.bind(this), 2000)
    }
    private async checkConnectionQuality() {
        if (this.isStopped || !this.transport) {
            return
        }

        let stats: Record<string, string | number>
        try {
            stats = await this.transport.getStats()
        } catch (e) {
            return
        }

        const packetsLost = typeof stats.packetsLost == "number" ? stats.packetsLost : null
        const packetsReceived = typeof stats.packetsReceived == "number" ? stats.packetsReceived : null
        const framesDropped = typeof stats.framesDropped == "number" ? stats.framesDropped : null

        const last = this.lastWarningStats
        this.lastWarningStats = { packetsLost, packetsReceived, framesDropped }
        if (last == null) {
            return
        }

        const now = Date.now()
        if (now - this.lastWarningTime < 15000) {
            return
        }

        const lostDelta = packetsLost != null && last.packetsLost != null ? packetsLost - last.packetsLost : 0
        const receivedDelta = packetsReceived != null && last.packetsReceived != null ? packetsReceived - last.packetsReceived : 0
        const droppedDelta = framesDropped != null && last.framesDropped != null ? framesDropped - last.framesDropped : 0

        if (lostDelta > 0 && (receivedDelta == 0 || lostDelta / Math.max(receivedDelta, 1) > 0.02)) {
            this.debugLog(`Connection quality warning: ${lostDelta} packets lost in the last 2 seconds`, { type: "ifErrorDescription" })
            this.lastWarningTime = now
        } else if (droppedDelta > 30) {
            this.debugLog(`Connection quality warning: ${droppedDelta} frames dropped in the last 2 seconds`, { type: "ifErrorDescription" })
            this.lastWarningTime = now
        }
    }

    // -- Keep display awake (Wake Lock)

    private wakeLock: { release: () => Promise<void> } | null = null
    private wakeLockRequest: Promise<void> | null = null

    private async updateWakeLock() {
        if (this.isStopped || !this.settings.keepDisplayAwake) {
            await this.releaseWakeLock()
            return
        }
        if (this.wakeLock != null || this.wakeLockRequest != null || document.visibilityState != "visible") {
            return
        }

        this.wakeLockRequest = (async () => {
            try {
                const wakeLock = await (navigator as any).wakeLock?.request("screen")
                if (!wakeLock) {
                    return
                }
                // Revalidate: the stream may have stopped or the tab hidden while the request was pending
                if (this.isStopped || !this.settings.keepDisplayAwake || document.visibilityState != "visible") {
                    await wakeLock.release()
                    return
                }
                this.wakeLock = wakeLock
                wakeLock.addEventListener("release", () => {
                    if (this.wakeLock === wakeLock) {
                        this.wakeLock = null
                    }
                })
            } catch (e) {
                console.debug("failed to acquire wake lock", e)
            }
        })()
        try {
            await this.wakeLockRequest
        } finally {
            this.wakeLockRequest = null
        }
    }
    private async releaseWakeLock() {
        const wakeLock = this.wakeLock
        this.wakeLock = null
        try {
            await wakeLock?.release()
        } catch (e) {
            // ignore
        }
    }

    onVisibilityChanged(visible: boolean) {
        if (visible) {
            this.updateWakeLock()
        }
    }

    private async createPipelines(connectData: TransportConnectData): Promise<void> {
        // Print supported pipes
        const pipesInfo = await gatherPipeInfo()

        this.logger.debug(`Supported Pipes: {`)
        let isFirst = true
        for (const [pipe, info] of pipesInfo) {
            this.logger.debug(`${isFirst ? "" : ","}"${pipeName(pipe)}": ${JSON.stringify(info)}`)
            isFirst = false
        }
        this.logger.debug(`}`)

        const codecSupport = emptyVideoCodecs()
        codecSupport[connectData.videoSetup.codec] = true

        // Create pipelines
        await Promise.all([
            this.createVideoRenderer(connectData.videoType, connectData.videoSetup),
            this.createAudioPlayer(connectData.audioType, connectData.audioSetup)
        ])

        const videoPipelineName = `${connectData.videoType} (transport) -> ${this.videoRenderer?.implementationName} (renderer)`
        this.debugLog(`Using video pipeline: ${videoPipelineName}`)

        const audioPipelineName = `${connectData.audioType} (transport) -> ${this.audioPlayer?.implementationName} (player)`
        this.debugLog(`Using audio pipeline: ${audioPipelineName}`)

        this.stats.setVideoPipeline(videoPipelineName, this.videoRenderer)
        this.stats.setAudioPipeline(audioPipelineName, this.audioPlayer)
    }

    private async queryVideoCodecs(type: "videotrack" | "data"): Promise<VideoFormats> {
        const codecHint = getVideoCodecHint(this.settings)

        const videoSettings: VideoPipelineOptions = {
            supportedVideoCodecs: codecHint,
            canvasRenderer: this.settings.canvasRenderer,
            forceVideoElementRenderer: this.settings.forceVideoElementRenderer,
            canvasVsync: this.settings.canvasVsync
        }

        const info = await queryVideoPipelineInfo(type, videoSettings, this.logger)
        if (!info) {
            this.logger.debug("failed to query video pipelines for information! Disabling high codecs. This could lead to no video being visible!")
            const baseCodecs = {
                h264: true,
                h264High8444: true,
                h265: true,
                h265Main10: true,
                h265Rext8444: true,
                h265Rext10444: true,
                av1Main8: true,
                av1Main10: true,
                av1High8444: true,
                av1High10444: true
            }

            videoSettings.supportedVideoCodecs = andVideoCodecs(codecHint, baseCodecs)
        }

        return info?.supportedVideoCodecs ?? emptyVideoCodecs()
    }
    private async createVideoRenderer(videoType: TransportVideoType, videoSetup: VideoRendererSetup): Promise<boolean> {
        if (this.videoRenderer) {
            this.debugLog("Found an old video renderer -> cleaning it up")

            this.videoRenderer.unmount(this.divElement)
            this.videoRenderer.cleanup()
            this.videoRenderer = null
        }
        if (!this.transport) {
            this.debugLog("Failed to setup video without transport")
            return false
        }

        const supportedVideoCodecs = emptyVideoCodecs()
        supportedVideoCodecs[videoSetup.codec] = true

        const videoSettings: VideoPipelineOptions = {
            supportedVideoCodecs,
            canvasRenderer: this.settings.canvasRenderer,
            forceVideoElementRenderer: this.settings.forceVideoElementRenderer,
            canvasVsync: this.settings.canvasVsync
        }

        let pipelineCodecSupport
        if (videoType == "videotrack") {
            const { videoRenderer, supportedCodecs, error } = await buildVideoPipeline("videotrack", videoSettings, this.logger)

            if (error) {
                return false
            }
            pipelineCodecSupport = supportedCodecs

            videoRenderer.mount(this.divElement)

            await videoRenderer.setup(videoSetup)
            await this.transport.setVideoPipeline("videotrack", videoRenderer)

            this.videoRenderer = videoRenderer
        } else if (videoType == "data") {
            const { videoRenderer, supportedCodecs, error } = await buildVideoPipeline("data", videoSettings, this.logger)

            if (error) {
                return false
            }
            pipelineCodecSupport = supportedCodecs

            videoRenderer.mount(this.divElement)

            await videoRenderer.setup(videoSetup)
            await this.transport.setVideoPipeline("data", videoRenderer)

            this.videoRenderer = videoRenderer
        } else {
            this.debugLog(`Failed to create video pipeline with transport channel of type ${videoType} (${this.transport.implementationName})`)
            return false
        }

        return true
    }
    private async createAudioPlayer(audioType: TransportAudioType, audioSetup: AudioPlayerSetup): Promise<boolean> {
        if (this.audioPlayer) {
            this.debugLog("Found an old audio player -> cleaning it up")

            this.audioPlayer.unmount(this.divElement)
            this.audioPlayer.cleanup()
            this.audioPlayer = null
        }
        if (!this.transport) {
            this.debugLog("Failed to setup audio without transport")
            return false
        }

        if (audioType == "audiotrack") {
            const { audioPlayer, error } = await buildAudioPipeline("audiotrack", {}, this.logger)

            if (error) {
                return false
            }

            audioPlayer.mount(this.divElement)
            await audioPlayer.setup(audioSetup)

            await this.transport.setAudioPipeline("audiotrack", audioPlayer)

            this.audioPlayer = audioPlayer
        } else if (audioType == "data") {
            const { audioPlayer, error } = await buildAudioPipeline("data", {}, this.logger)

            if (error) {
                return false
            }

            audioPlayer.mount(this.divElement)
            await audioPlayer.setup(audioSetup)

            await this.transport.setAudioPipeline("data", audioPlayer)

            this.audioPlayer = audioPlayer
        } else {
            this.debugLog(`Cannot find audio pipeline for transport type "${audioType}"`)
            return false
        }

        return true
    }

    mount(parent: HTMLElement): void {
        parent.appendChild(this.divElement)
    }
    unmount(parent: HTMLElement): void {
        parent.removeChild(this.divElement)
    }

    getVideoRenderer(): VideoRenderer | null {
        return this.videoRenderer
    }
    getAudioPlayer(): AudioPlayer | null {
        return this.audioPlayer
    }

    async stop(): Promise<boolean> {
        this.isStopped = true

        if (this.connectionWarningIntervalId != null) {
            clearInterval(this.connectionWarningIntervalId)
            this.connectionWarningIntervalId = null
        }

        // keepalive so the request still completes when the page is unloading;
        // start it before the first await so it isn't skipped during teardown
        const quitAppPromise = this.settings.quitAppOnExit
            ? apiHostCancel(this.api, { host_id: this.hostId }, true).catch(e => {
                this.debugLog(`Failed to quit app on host: ${e}`)
            })
            : null

        const transportClosePromise = this.transport?.close()

        await this.releaseWakeLock()
        await quitAppPromise

        // Stop transport
        await transportClosePromise

        return true
    }

    private boundReceivePacket = this.onReceivePacket.bind(this)
    private onReceivePacket(packet: ControlPacket) {
        switch (packet.tag) {
            case ControlPacket_Tags.HdrMode:
                if (this.videoRenderer && this.videoRenderer.setHdrMode) {
                    this.videoRenderer?.setHdrMode(packet.inner.enabled, packet.inner.sunshine)
                }
                break
        }

        this.input.onReceivePacket(packet)
    }

    // -- Class Api
    addInfoListener(listener: InfoEventListener) {
        this.eventTarget.addEventListener("stream-info", listener as EventListenerOrEventListenerObject)
    }
    removeInfoListener(listener: InfoEventListener) {
        this.eventTarget.removeEventListener("stream-info", listener as EventListenerOrEventListenerObject)
    }

    getInput(): StreamInput {
        return this.input
    }
    getStats(): StreamStats {
        return this.stats
    }

    getStreamerSize(): [number, number] {
        return this.streamerSize
    }
}
