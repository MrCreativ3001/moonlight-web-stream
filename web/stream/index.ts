import { Api, apiWHEPOffer } from "../api.js"
import { App, ConnectionStatus, StreamCapabilities, StreamClientMessage, StreamPermissions, StreamSettings, TransportChannelId } from "../api_bindings.js"
import { showErrorPopup } from "../component/error.js"
import { Component } from "../component/index.js"
import { Settings, TransportType } from "../component/settings_menu.js"
import { ControlPacket, ControlPacket_Tags, VideoFormats } from "../uniffi/moonlight_common_bindings.js"
import { wait } from "../util.js"
import { AudioPlayer } from "./audio/index.js"
import { buildAudioPipeline } from "./audio/pipeline.js"
import { BIG_BUFFER, ByteBuffer } from "./buffer.js"
import { defaultStreamInputConfig, StreamInput } from "./input.js"
import { Logger, LogMessageInfo } from "./log.js"
import { gatherPipeInfo } from "./pipeline/index.js"
import { StreamStats } from "./stats.js"
import { Transport, TransportAudioType, TransportConnectData, TransportShutdown, TransportVideoType } from "./transport/index.js"
import { WebSocketTransport } from "./transport/web_socket.js"
import { WebRTCTransport } from "./transport/whep.js"
import { allVideoCodecs, andVideoCodecs, createSupportedVideoFormatsBits, emptyVideoCodecs, hasAnyCodec, VideoCodecSupport } from "./video.js"
import { VideoRenderer } from "./video/index.js"
import { buildVideoPipeline, VideoPipelineOptions } from "./video/pipeline.js"

export type ExecutionEnvironment = {
    main: boolean
    worker: boolean
}

export type InfoEvent = CustomEvent<
    { type: "app", app: App } |
    { type: "serverMessage", message: string } |
    { type: "connectionComplete", capabilities: StreamCapabilities } |
    { type: "connectionStatus", status: ConnectionStatus } |
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

function getVideoCodecHint(settings: Settings): VideoCodecSupport {
    let videoCodecHint = emptyVideoCodecs()
    if (settings.videoCodec == "h264") {
        videoCodecHint.H264 = true
        videoCodecHint.H264_HIGH8_444 = true
    } else if (settings.videoCodec == "h265") {
        videoCodecHint.H265 = true
        videoCodecHint.H265_MAIN10 = true
        videoCodecHint.H265_REXT8_444 = true
        videoCodecHint.H265_REXT10_444 = true
    } else if (settings.videoCodec == "av1") {
        videoCodecHint.AV1 = true
        videoCodecHint.AV1_MAIN8 = true
        videoCodecHint.AV1_MAIN10 = true
        videoCodecHint.AV1_REXT8_444 = true
        videoCodecHint.AV1_REXT10_444 = true
    } else if (settings.videoCodec == "auto") {
        videoCodecHint = allVideoCodecs()
    }
    return videoCodecHint
}

const WEBRTC_CONNECT_TIMEOUT_MS = 15000
const FALLBACK_RECONNECT_DELAY_MS = 500

export class Stream implements Component {
    private logger: Logger = new Logger()

    private api: Api

    private hostId: number
    private appId: number

    private permissions: StreamPermissions
    private settings: Settings

    private divElement = document.createElement("div")
    private eventTarget = new EventTarget()

    private transportOverride: TransportType | null = null

    private videoRenderer: VideoRenderer | null = null
    private audioPlayer: AudioPlayer | null = null

    private input: StreamInput
    private stats: StreamStats

    private streamerSize: [number, number]

    constructor(api: Api, hostId: number, appId: number, settings: Settings, viewerScreenSize: [number, number], permissions: StreamPermissions) {
        this.logger.addInfoListener((info, type) => {
            this.debugLog(info, { type: type ?? undefined })
        })

        this.api = api

        this.hostId = hostId
        this.appId = appId

        this.permissions = permissions
        this.settings = settings

        this.streamerSize = getStreamerSize(settings, viewerScreenSize)

        // Stream Input
        const streamInputConfig = defaultStreamInputConfig()
        Object.assign(streamInputConfig, {
            mouseMode: this.settings.mouseMode,
            mouseScrollMode: this.settings.mouseScrollMode,
            touchMode: this.settings.touchMode,
            localCursorSensitivity: this.settings.localCursorSensitivity,
            controllerConfig: this.settings.controllerConfig
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

    async startConnection() {
        this.debugLog(`Permissions: ${JSON.stringify(this.permissions)}`)

        const desiredTransport = this.transportOverride ?? this.settings.dataTransport
        this.debugLog(`Using transport: ${desiredTransport}`)

        // TODO: how should those events be handled?
        // const event: InfoEvent = new CustomEvent("stream-info", {
        //     detail: { type: "connectionComplete", capabilities }
        // })
        // const event: InfoEvent = new CustomEvent("stream-info", {
        //     detail: { type: "app", app: message.UpdateApp.app }
        // })

        if (desiredTransport == "auto") {
            let shutdownReason = await this.tryWebRTCTransport()

            if (shutdownReason == "failednoconnect") {
                this.debugLog("Failed to establish WebRTC connection. Falling back to Web Socket transport.", { type: "ifErrorDescription" })
                await this.tryWebSocketTransport()
            }
        } else if (desiredTransport == "webrtc") {
            await this.tryWebRTCTransport()
        } else if (desiredTransport == "websocket") {
            await this.tryWebSocketTransport()
        }

        this.debugLog("Tried all configured transport options but no connection was possible", { type: "fatal" })
    }

    private transport: Transport | null = null

    private setTransport(transport: Transport) {
        if (this.transport) {
            this.debugLog("Closing old transport")
            this.transport.close()
        }
        this.debugLog("Setting new transport")

        this.transport = transport

        this.input.setControlStream(this.transport.controlStream)
        this.stats.setTransport(this.transport)
    }

    private async tryWebRTCTransport(): Promise<TransportShutdown> {
        if (!this.permissions.allow_transport_webrtc) {
            this.debugLog("Not trying WebRTC transport because permissions disallow it")
            return "failednoconnect"
        }

        this.debugLog("Trying WebRTC transport")

        // Create and configure transport
        const transport = new WebRTCTransport(this.logger)
        transport.controlStream.onreceive = this.boundReceivePacket

        const onConnect = new Promise<TransportConnectData>(resolve => {
            transport.onconnect = resolve
        })
        const onClose = new Promise<TransportShutdown>(resolve => {
            transport.onclose = resolve
        })

        const codecHint = getVideoCodecHint(this.settings)
        this.debugLog(`Codec Hint by the user: ${JSON.stringify(codecHint)}`)

        if (!hasAnyCodec(codecHint)) {
            this.debugLog("Couldn't find any supported video format. Change the codec option to H264 in the settings if you're unsure which codecs are supported.", { type: "fatalDescription" })
            return "failednoconnect"
        }

        try {
            // Create WHEP offer
            const offer = await transport.createOffer({
                hostId: this.hostId,
                appId: this.appId,
                width: this.streamerSize[0],
                height: this.streamerSize[1],
                fps: this.settings.fps,
                bitrate: this.settings.bitrate,
                hdr: this.settings.hdr,
                localAudioPlayMode: this.settings.playAudioLocal,
                // TODO: use codecHint for preferredCodec
            })

            // Send Request
            this.debugLog("Sending WHEP Offer and waiting for Answer")
            const answer = await apiWHEPOffer(this.api, offer)
            this.debugLog("Got WHEP Response")

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
        await this.onConnect(transport, connectData)

        return await onClose
    }
    private async tryWebSocketTransport() {
        if (!this.permissions.allow_transport_websockets) {
            this.debugLog("Not trying WebSocket transport becaues permissions disallow it")
            return
        }

        this.debugLog("Trying Web Socket transport")

        const codecHint = getVideoCodecHint(this.settings)
        this.debugLog(`Codec Hint by the user: ${JSON.stringify(codecHint)}`)

        if (!hasAnyCodec(codecHint)) {
            this.debugLog("Couldn't find any supported video format. Change the codec option to H264 in the settings if you're unsure which codecs are supported.", { type: "fatalDescription" })
            return null
        }


        // TODO
        // const transport = new WebSocketTransport(this.ws, BIG_BUFFER, this.logger)

        // this.setTransport(transport)

        // const videoCodecSupport = await this.createPipelines()
        // if (!videoCodecSupport) {
        //     this.debugLog("Failed to start stream because no video pipeline with support for the specified codec was found!", { type: "fatalDescription" })
        //     return
        // }

        // await this.startStream(videoCodecSupport)

        // return new Promise((resolve) => {
        //     transport.onclose = (shutdown) => {
        //         resolve(shutdown)
        //     }
        // })
        return new Promise((resolve, reject) => { })
    }

    private async onConnect(transport: Transport, connectData: TransportConnectData) {
        // Set input
        this.input.onStreamStart(connectData.capabilities, [connectData.videoSetup.width, connectData.videoSetup.height])

        // Create pipelines
        const videoCodecSupport = await this.createPipelines(connectData)
        if (!videoCodecSupport) {
            this.debugLog("No video pipeline was found for the codec that was specified. If you're unsure which codecs are supported use H264.", { type: "fatalDescription" })

            await transport.close()
            return "failednoconnect"
        }

        const event: InfoEvent = new CustomEvent("stream-info", {
            detail: {
                type: "connectionComplete", capabilities: {
                    // TODO
                    touch: true
                }
            }
        })
        this.eventTarget.dispatchEvent(event)
    }

    private async createPipelines(connectData: TransportConnectData): Promise<VideoCodecSupport | null> {
        // Print supported pipes
        const pipesInfo = await gatherPipeInfo()

        this.logger.debug(`Supported Pipes: {`)
        let isFirst = true
        for (const [pipe, info] of pipesInfo) {
            this.logger.debug(`${isFirst ? "" : ","}"${pipe.name}": ${JSON.stringify(info)}`)
            isFirst = false
        }
        this.logger.debug(`}`)

        const codecSupport = emptyVideoCodecs()
        codecSupport[connectData.videoSetup.codec] = true

        // Create pipelines
        const [supportedVideoCodecs] = await Promise.all([
            this.createVideoRenderer(connectData.videoType, codecSupport),
            this.createAudioPlayer(connectData.audioType)
        ])

        const videoPipelineName = `${connectData.videoType} (transport) -> ${this.videoRenderer?.implementationName} (renderer)`
        this.debugLog(`Using video pipeline: ${videoPipelineName}`)

        const audioPipelineName = `${connectData.audioType} (transport) -> ${this.audioPlayer?.implementationName} (player)`
        this.debugLog(`Using audio pipeline: ${audioPipelineName}`)

        this.stats.setVideoPipeline(videoPipelineName, this.videoRenderer)
        this.stats.setAudioPipeline(audioPipelineName, this.audioPlayer)

        return supportedVideoCodecs
    }
    private async createVideoRenderer(videoType: TransportVideoType, codec: VideoCodecSupport): Promise<VideoCodecSupport | null> {
        if (this.videoRenderer) {
            this.debugLog("Found an old video renderer -> cleaning it up")

            this.videoRenderer.unmount(this.divElement)
            this.videoRenderer.cleanup()
            this.videoRenderer = null
        }
        if (!this.transport) {
            this.debugLog("Failed to setup video without transport")
            return null
        }

        const videoSettings: VideoPipelineOptions = {
            supportedVideoCodecs: codec,
            canvasRenderer: this.settings.canvasRenderer,
            forceVideoElementRenderer: this.settings.forceVideoElementRenderer,
            canvasVsync: this.settings.canvasVsync
        }

        let pipelineCodecSupport
        if (videoType == "videotrack") {
            const { videoRenderer, supportedCodecs, error } = await buildVideoPipeline("videotrack", videoSettings, this.logger)

            if (error) {
                return null
            }
            pipelineCodecSupport = supportedCodecs

            videoRenderer.mount(this.divElement)

            await this.transport.setVideoPipeline("videotrack", videoRenderer)

            this.videoRenderer = videoRenderer
        } else if (videoType == "data") {
            const { videoRenderer, supportedCodecs, error } = await buildVideoPipeline("data", videoSettings, this.logger)

            if (error) {
                return null
            }
            pipelineCodecSupport = supportedCodecs

            videoRenderer.mount(this.divElement)

            await this.transport.setVideoPipeline("data", videoRenderer)

            this.videoRenderer = videoRenderer
        } else {
            this.debugLog(`Failed to create video pipeline with transport channel of type ${videoType} (${this.transport.implementationName})`)
            return null
        }

        return pipelineCodecSupport
    }
    private async createAudioPlayer(audioType: TransportAudioType): Promise<boolean> {
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
            const { audioPlayer, error } = await buildAudioPipeline("audiotrack", this.settings, this.logger)

            if (error) {
                return false
            }

            audioPlayer.mount(this.divElement)

            await this.transport.setAudioPipeline("audiotrack", audioPlayer)

            this.audioPlayer = audioPlayer
        } else if (audioType == "data") {
            const { audioPlayer, error } = await buildAudioPipeline("data", this.settings, this.logger)

            if (error) {
                return false
            }

            audioPlayer.mount(this.divElement)

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
        await this.transport?.close()

        // Wait for the message to get sent
        await new Promise((resolve, _reject) => {
            setTimeout(() => resolve(true), 100)
        })

        return true
    }

    private boundReceivePacket = this.onReceivePacket.bind(this)
    private onReceivePacket(packet: ControlPacket) {
        if (packet.tag == ControlPacket_Tags.HdrMode) {
            if (this.videoRenderer && this.videoRenderer.setHdrMode) {
                this.videoRenderer?.setHdrMode(packet.inner.enabled)
            }
        }
        // TODO
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

function createPrettyList(list: Array<string>): string {
    return `[${list.join(", ")}]`
}
