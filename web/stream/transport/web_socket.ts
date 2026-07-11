import { Api } from "../../api.js";
import { WebSocketChannel, WebSocketClientboundMessage, WebSocketServerboundMessage } from "../../api_bindings.js";
import { ClientInputEvent, ControlPacket, ControlPacketConfig, controlPacketDeserialize, controlPacketSerialize, InputBatcher, PacketDirection } from "../../uniffi/moonlight_common_bindings.js";
import { globalObject } from "../../util.js";
import { AudioPlayer, TrackAudioPlayer } from "../audio/index.js";
import { Logger } from "../log.js";
import { DataPipe } from "../pipeline/pipes.js";
import { StatValue } from "../stats.js";
import { createSupportedVideoFormatsBits } from "../video.js";
import { TrackVideoRenderer, VideoRenderer } from "../video/index.js";
import { generateControlPacketConfig, IControlStream, Transport, TransportAudioType, TransportConnectData, TransportOptions, TransportShutdown, TransportVideoType } from "./index.js";

export class WebSocketTransport implements Transport {
    readonly implementationName: string = "web_socket"

    private logger?: Logger

    private ws: WebSocket

    controlStream: WebSocketControlStream
    onconnect: ((connectData: TransportConnectData) => void) | null = null
    onclose: ((shutdown: TransportShutdown) => void) | null = null

    private onOpen: Promise<void>
    private wasConnected: boolean = false

    private internalOnConnect: () => void = () => { }
    private onConnected: Promise<void> = new Promise(resolve => this.internalOnConnect = resolve)

    private connectData: TransportConnectData | null = null

    constructor(api: Api, logger?: Logger) {
        this.logger = logger

        const wsApiHost = api.host_url.replace(/^http(s)?:/, "ws$1:")
        this.ws = new WebSocket(`${wsApiHost}/host/stream/web_socket`)

        // Configure Web Socket
        this.ws.binaryType = "arraybuffer"

        // Add Event Listeners
        this.ws.addEventListener("error", this.onError.bind(this))
        this.onOpen = new Promise((resolve, reject) => {
            this.ws.addEventListener("error", reject)
            this.ws.addEventListener("open", () => resolve())
        })
        this.ws.addEventListener("close", this.onClose.bind(this))
        this.ws.addEventListener("message", this.onMessage.bind(this))

        // Create control stream
        const config = generateControlPacketConfig()
        this.controlStream = new WebSocketControlStream(this.ws, config)
    }

    // -- Web Socket Events
    private onError() {
        // TODO: log the error
        this.close()
    }

    private sendMessage(message: WebSocketServerboundMessage) {
        this.ws.send(JSON.stringify(message))
    }
    private onMessage(event: MessageEvent) {
        const data = event.data

        if (typeof data == "string") {
            const message = JSON.parse(data) as WebSocketClientboundMessage

            if ("Response" in message) {
                const response = message.Response
                this.logger?.debug(`received stream response: ${JSON.stringify(response)}`)

                this.wasConnected = true

                this.connectData = {
                    videoType: "data",
                    videoSetup: {
                        codec: "h264",
                        width: this.options?.width ?? 0,
                        height: this.options?.height ?? 0,
                        fps: this.options?.fps ?? 0
                    },
                    audioType: "data",
                    audioSetup: {
                        channels: response.audio_channel_count,
                        sampleRate: response.audio_sample_rate,
                        streams: response.audio_coupled_streams,
                        coupledStreams: response.audio_coupled_streams,
                        samplesPerFrame: response.audio_samples_per_frame,
                        mapping: response.audio_mapping
                    },
                    capabilities: { touch: true }
                }

                this.internalOnConnect()

                if (this.onconnect) {
                    this.onconnect(this.connectData)
                }
            } else if ("Stats" in message) {
                if ("Pong" in message.Stats) {
                    const pongId = message.Stats.Pong
                    this.onPongReceive(pongId)
                } else if ("RelayRtt" in message.Stats) {
                    const relayRtt = message.Stats.RelayRtt
                    this.relayStats = {
                        rttMs: relayRtt.rtt_ms,
                        rttVarianceMs: relayRtt.rtt_variance_ms
                    }
                }
            }
        } else if (data instanceof ArrayBuffer) {
            const view = new Uint8Array(data)
            const channel = view[0]

            if (channel == WebSocketChannel.CONTROL) {
                this.controlStream.onRawPacket(view.subarray(1))
            } else if (channel == WebSocketChannel.VIDEO) {
                this.videoPipeline?.submitPacket(view.subarray(1))
                if (this.videoPipeline && "pollRequestIdr" in this.videoPipeline && typeof this.videoPipeline.pollRequestIdr == "function" && this.videoPipeline.pollRequestIdr()) {
                    this.logger?.debug("requesting idr")

                    this.controlStream.sendRaw(new ControlPacket.RequestIdr())
                }
            } else if (channel == WebSocketChannel.AUDIO) {
                this.audioPipeline?.submitPacket(view.subarray(1))
            }
        }
    }

    // -- Connect
    private options: TransportOptions | null = null
    async startStream(options: TransportOptions): Promise<void> {
        this.logger?.debug("Waiting for Web Socket to open")

        await this.onOpen
        this.options = options

        this.logger?.debug(`Web Socket opened, sending stream request: ${JSON.stringify(options)}`)

        this.sendMessage({
            Request: {
                host_id: options.hostId,
                app_id: options.appId,
                width: options.width,
                height: options.height,
                fps: options.fps,
                bitrate: options.bitrate,
                hdr: options.hdr,
                local_audio_play_mode: options.localAudioPlayMode,
                supported_codecs: createSupportedVideoFormatsBits(options.supportedCodecs),
                preferred_codecs: options.preferredCodecs ? createSupportedVideoFormatsBits(options.preferredCodecs) : 0,
            }
        })
    }

    private onClose() {
        this.close()
    }

    private videoPipeline: DataPipe | null = null

    setVideoPipeline(type: "videotrack", pipeline: (TrackVideoRenderer & VideoRenderer)): Promise<void>
    setVideoPipeline(type: "data", pipeline: (DataPipe & VideoRenderer)): Promise<void>
    async setVideoPipeline(type: TransportVideoType, pipeline: unknown): Promise<void> {
        if (type != "data") {
            throw `invalid web socket video pipeline type ${type}`
        }

        this.videoPipeline = pipeline as (DataPipe & AudioPlayer)
    }

    private audioPipeline: (DataPipe & AudioPlayer) | null = null

    setAudioPipeline(type: "audiotrack", pipeline: (TrackAudioPlayer & AudioPlayer)): Promise<void>
    setAudioPipeline(type: "data", pipeline: (DataPipe & AudioPlayer)): Promise<void>
    async setAudioPipeline(type: TransportAudioType, pipeline: unknown): Promise<void> {
        if (!this.connectData) {
            throw "web socket stream not yet connected!"
        }
        if (type != "data") {
            throw `invalid web socket audio pipeline type ${type}`
        }

        this.audioPipeline = pipeline as (DataPipe & AudioPlayer)
    }

    private relayStats: { rttMs: number, rttVarianceMs: number } | null = null
    async getStats(): Promise<Record<string, StatValue>> {
        const out: Record<string, StatValue> = {}

        if (this.relayStats) {
            out.hostToRelayRttMs = this.relayStats.rttMs
            out.hostToRelayRttVarianceMs = this.relayStats.rttVarianceMs
        }
        out.relayToClientRttMs = await this.doPing()

        return out
    }

    // Do ping pong to get rtt
    private onPongPromise: Promise<number> | null = null
    private pongReceiveResolve: (() => void) | null = null

    private onPongReceive(_id: number) {
        if (this.pongReceiveResolve) {
            this.pongReceiveResolve()
        }
    }
    private async doPing(): Promise<number> {
        // Make sure that the socket is open and we're connected
        await this.onConnected

        // Check if we've already send a ping
        if (this.onPongPromise) {
            return await this.onPongPromise
        }

        // Setup pong receive
        this.onPongPromise = new Promise<number>(resolve => {
            const start = performance.now()

            this.pongReceiveResolve = () => {
                this.pongReceiveResolve = null
                this.onPongPromise = null

                const diff = performance.now() - start
                resolve(diff)
            }
        })

        // Send Ping
        this.sendMessage({
            Stats: {
                Ping: Math.floor(Math.random() * 1000)
            }
        })

        return await this.onPongPromise
    }

    private dispatchedClosed: boolean = false
    async close(): Promise<void> {
        this.ws.close()
        if (!this.dispatchedClosed && this.onclose) {
            if (this.wasConnected) {
                this.onclose("failed")
            } else {
                this.onclose("failednoconnect")
            }
        }
    }

}
class WebSocketControlStream implements IControlStream {
    onreceive: ((packet: ControlPacket) => void) | null = null

    private ws: WebSocket

    private config: ControlPacketConfig

    private batcher: InputBatcher = new InputBatcher()
    private batchSendTimeout: number | null = null

    constructor(ws: WebSocket, config: ControlPacketConfig) {
        this.ws = ws
        this.config = config
    }

    send(input: ClientInputEvent): void {
        const sendNow = this.batcher.batchInput(input)
        for (const packet of sendNow) {
            this.sendRaw(packet)
        }

        if (this.batchSendTimeout == null) {
            globalObject().setTimeout(this.boundSendBatchedInputs, 1)
        }
    }

    private trySendBufferedPackets() {
        if (this.ws.readyState != WebSocket.OPEN) {
            return
        }

        for (const packet of this.packetBuffer.splice(0)) {
            this.sendRaw(packet)
        }
    }

    private packetBuffer: Array<ControlPacket> = []
    sendRaw(packet: ControlPacket): void {
        if (this.ws.readyState != WebSocket.OPEN) {
            this.packetBuffer.push(packet)
            return
        }
        this.trySendBufferedPackets()

        const packetBuffer = controlPacketSerialize(this.config, packet)
        if (!packetBuffer) {
            console.debug("failed to serialize control packet", packet)
            return
        }
        const packetView = new Uint8Array(packetBuffer)

        const message = new Uint8Array(1 + packetView.length)
        message.set(packetView, 1)

        this.ws.send(message)
    }

    onRawPacket(packetBuffer: Uint8Array) {
        const packet = controlPacketDeserialize(this.config, PacketDirection.ClientBound, packetBuffer.slice().buffer)
        if (this.onreceive && packet) {
            this.onreceive(packet)
        } else if (!packet) {
            console.debug("failed to deserialize packet", packetBuffer)
        }
    }

    private boundSendBatchedInputs = this.sendBatchedInputs.bind(this)
    private sendBatchedInputs() {
        if (this.batchSendTimeout != null) {
            globalObject().clearTimeout(this.batchSendTimeout)
        }
        this.batchSendTimeout = null

        const packets = this.batcher.removeBatchedInputs()

        for (const packet of packets) {
            this.sendRaw(packet)
        }
    }
}
