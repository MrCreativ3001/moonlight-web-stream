import { StreamSignalingMessage, TransportChannelId } from "../../api_bindings.js";
import { Logger } from "../log.js";
import { allVideoCodecs, CAPABILITIES_CODECS, emptyVideoCodecs, maybeVideoCodecs, VideoCodecSupport } from "../video.js";
import { DataTransportChannel, Transport, TRANSPORT_CHANNEL_OPTIONS, TransportAudioSetup, TransportChannel, TransportChannelIdKey, TransportChannelIdValue, TransportVideoSetup, AudioTrackTransportChannel, VideoTrackTransportChannel, TrackTransportChannel, TransportShutdown } from "./index.js";

export class WebRTCTransport implements Transport {
    implementationName: string = "webrtc"

    private logger: Logger | null

    private peer: RTCPeerConnection | null = null

    constructor(logger?: Logger) {
        this.logger = logger ?? null
    }

    async initPeer(configuration?: RTCConfiguration) {
        this.logger?.debug(`Creating Client Peer`)

        if (this.peer) {
            this.logger?.debug(`Cannot create Peer because a Peer already exists`)
            return
        }

        // Configure web rtc
        this.peer = new RTCPeerConnection(configuration)
        this.peer.addEventListener("error", this.onError.bind(this))

        this.peer.addEventListener("negotiationneeded", this.onNegotiationNeeded.bind(this))
        this.peer.addEventListener("icecandidate", this.onIceCandidate.bind(this))

        this.peer.addEventListener("connectionstatechange", this.onConnectionStateChange.bind(this))
        this.peer.addEventListener("signalingstatechange", this.onSignalingStateChange.bind(this))
        this.peer.addEventListener("iceconnectionstatechange", this.onIceConnectionStateChange.bind(this))
        this.peer.addEventListener("icegatheringstatechange", this.onIceGatheringStateChange.bind(this))

        this.peer.addEventListener("track", this.onTrack.bind(this))
        this.peer.addEventListener("datachannel", this.onDataChannel.bind(this))

        this.initChannels()

        // Maybe we already received data
        if (this.remoteDescription) {
            await this.handleRemoteDescription(this.remoteDescription)
        } else {
            await this.onNegotiationNeeded()
        }
        await this.tryDequeueIceCandidates()
    }

    private onError(event: Event) {
        this.logger?.debug(`Web Socket or WebRtcPeer Error`)

        console.error(`Web Socket or WebRtcPeer Error`, event)
    }

    onsendmessage: ((message: StreamSignalingMessage) => void) | null = null
    private sendMessage(message: StreamSignalingMessage) {
        if (this.onsendmessage) {
            this.onsendmessage(message)
        } else {
            this.logger?.debug("Failed to call onicecandidate because no handler is set")
        }
    }
    async onReceiveMessage(message: StreamSignalingMessage) {
        if ("Description" in message) {
            const description = message.Description;
            await this.handleRemoteDescription({
                type: description.ty as RTCSdpType,
                sdp: description.sdp
            })
        } else if ("AddIceCandidate" in message) {
            const candidate = message.AddIceCandidate
            await this.addIceCandidate({
                candidate: candidate.candidate,
                sdpMid: candidate.sdp_mid,
                sdpMLineIndex: candidate.sdp_mline_index,
                usernameFragment: candidate.username_fragment
            })
        }
    }

    private async onNegotiationNeeded() {
        // We're polite
        if (!this.peer) {
            this.logger?.debug("OnNegotiationNeeded without a peer")
            return
        }

        await this.peer.setLocalDescription()
        const localDescription = this.peer.localDescription
        if (!localDescription) {
            this.logger?.debug("Failed to set local description in OnNegotiationNeeded")
            return
        }

        this.logger?.debug(`OnNegotiationNeeded: Sending local description: ${localDescription.type}`)
        this.sendMessage({
            Description: {
                ty: localDescription.type,
                sdp: localDescription.sdp ?? ""
            }
        })
    }

    private remoteDescription: RTCSessionDescriptionInit | null = null
    private async handleRemoteDescription(sdp: RTCSessionDescriptionInit | null) {
        this.logger?.debug(`Received remote description: ${sdp?.type}`)

        const remoteDescription = sdp
        this.remoteDescription = remoteDescription
        if (!this.peer) {
            return
        }
        this.remoteDescription = null

        if (remoteDescription) {
            await this.peer.setRemoteDescription(remoteDescription)

            if (remoteDescription.type == "offer") {
                await this.peer.setLocalDescription()
                const localDescription = this.peer.localDescription
                if (!localDescription) {
                    this.logger?.debug("Peer didn't have a localDescription whilst receiving an offer and trying to answer")
                    return
                }

                this.logger?.debug(`Responding to offer description: ${localDescription.type}`)
                this.sendMessage({
                    Description: {
                        ty: localDescription.type,
                        sdp: localDescription.sdp ?? ""
                    }
                })
            }
        }
    }

    private onIceCandidate(event: RTCPeerConnectionIceEvent) {
        if (event.candidate) {
            const candidate = event.candidate.toJSON()
            this.logger?.debug(`Sending ice candidate: ${candidate.candidate}`)

            this.sendMessage({
                AddIceCandidate: {
                    candidate: candidate.candidate ?? "",
                    sdp_mid: candidate.sdpMid ?? null,
                    sdp_mline_index: candidate.sdpMLineIndex ?? null,
                    username_fragment: candidate.usernameFragment ?? null
                }
            })
        } else {
            this.logger?.debug("No new ice candidates")
        }
    }

    private iceCandidates: Array<RTCIceCandidateInit> = []
    private async addIceCandidate(candidate: RTCIceCandidateInit) {
        this.logger?.debug(`Received ice candidate: ${candidate.candidate}`)

        if (!this.peer) {
            this.logger?.debug("Buffering ice candidate")

            this.iceCandidates.push(candidate)
            return
        }
        await this.tryDequeueIceCandidates()

        await this.peer.addIceCandidate(candidate)
    }
    private async tryDequeueIceCandidates() {
        if (!this.peer) {
            this.logger?.debug("called tryDequeueIceCandidates without a peer")
            return
        }

        for (const candidate of this.iceCandidates) {
            await this.peer.addIceCandidate(candidate)
        }
        this.iceCandidates.length = 0
    }

    private wasConnected = false
    private onConnectionStateChange() {
        if (!this.peer) {
            this.logger?.debug("OnConnectionStateChange without a peer")
            return
        }

        let type: null | "fatal" | "recover" = null

        if (this.peer.connectionState == "connected") {
            type = "recover"

            if (this.onconnect) {
                this.onconnect()
            }
            this.wasConnected = true
        } else if ((this.peer.connectionState == "failed" || this.peer.connectionState == "closed" || this.peer.connectionState == "disconnected") && this.peer.iceGatheringState == "complete") {
            type = "fatal"
        }

        if (this.peer.connectionState == "failed" || this.peer.connectionState == "closed") {
            if (this.onclose) {
                if (this.wasConnected) {
                    this.onclose("failed")
                } else {
                    this.onclose("failednoconnect")
                }
            }
        }

        this.logger?.debug(`Changing Peer State to ${this.peer.connectionState}`, {
            type: type ?? undefined
        })
    }
    private onSignalingStateChange() {
        if (!this.peer) {
            this.logger?.debug("OnSignalingStateChange without a peer")
            return
        }
        this.logger?.debug(`Changing Peer Signaling State to ${this.peer.signalingState}`)
    }
    private onIceConnectionStateChange() {
        if (!this.peer) {
            this.logger?.debug("OnIceConnectionStateChange without a peer")
            return
        }
        this.logger?.debug(`Changing Peer Ice State to ${this.peer.iceConnectionState}`)
    }
    private onIceGatheringStateChange() {
        if (!this.peer) {
            this.logger?.debug("OnIceGatheringStateChange without a peer")
            return
        }
        this.logger?.debug(`Changing Peer Ice Gathering State to ${this.peer.iceGatheringState}`)

        if (this.peer.iceConnectionState == "new" && this.peer.iceGatheringState == "complete") {
            // we failed without connection
            if (this.onclose) {
                this.onclose("failednoconnect")
            }
        }
    }

    private channels: Array<TransportChannel | null> = []
    private initChannels() {
        if (!this.peer) {
            this.logger?.debug("Failed to initialize channel without peer")
            return
        }
        if (this.channels.length > 0) {
            this.logger?.debug("Already initialized channels")
            return
        }

        for (const channelRaw in TRANSPORT_CHANNEL_OPTIONS) {
            const channel = channelRaw as TransportChannelIdKey
            const options = TRANSPORT_CHANNEL_OPTIONS[channel]

            if (channel == "HOST_VIDEO") {
                const channel: VideoTrackTransportChannel = new WebRTCInboundTrackTransportChannel<"videotrack">(this.logger, "videotrack", "video", this.videoTrackHolder)
                this.channels[TransportChannelId.HOST_VIDEO] = channel
                continue
            }
            if (channel == "HOST_AUDIO") {
                const channel: AudioTrackTransportChannel = new WebRTCInboundTrackTransportChannel<"audiotrack">(this.logger, "audiotrack", "audio", this.audioTrackHolder)
                this.channels[TransportChannelId.HOST_AUDIO] = channel
                continue
            }

            const id = TransportChannelId[channel]
            const dataChannel = this.peer.createDataChannel(channel.toLowerCase(), {
                ordered: options.ordered,
                maxRetransmits: options.reliable ? undefined : 0
            })

            this.channels[id] = new WebRTCDataTransportChannel(channel, dataChannel)
        }
    }

    private videoTrackHolder: TrackHolder = { ontrack: null, track: null }
    private videoReceiver: RTCRtpReceiver | null = null

    private audioTrackHolder: TrackHolder = { ontrack: null, track: null }

    private onTrack(event: RTCTrackEvent) {
        const track = event.track

        const receiver = event.receiver
        if (track.kind == "video") {
            this.videoReceiver = receiver
        }

        receiver.jitterBufferTarget = 0
        if ("playoutDelayHint" in receiver) {
            receiver.playoutDelayHint = 0
        }

        this.logger?.debug(`Adding receiver: ${track.kind}, ${track.id}, ${track.label}`)

        if (track.kind == "video") {
            if ("contentHint" in track) {
                track.contentHint = "motion"
            }

            this.videoTrackHolder.track = track
            if (!this.videoTrackHolder.ontrack) {
                throw "No video track listener registered!"
            }
            this.videoTrackHolder.ontrack()
        } else if (track.kind == "audio") {
            this.audioTrackHolder.track = track
            if (!this.audioTrackHolder.ontrack) {
                throw "No audio track listener registered!"
            }
            this.audioTrackHolder.ontrack()
        }
    }

    // Handle data channels created by the remote peer (server)
    private onDataChannel(event: RTCDataChannelEvent) {
        const remoteChannel = event.channel
        const label = remoteChannel.label

        this.logger?.debug(`Received remote data channel: ${label}`)

        // Map the channel label to the corresponding TransportChannelId
        const channelKey = label.toUpperCase() as TransportChannelIdKey
        if (channelKey in TransportChannelId) {
            const id = TransportChannelId[channelKey]
            const existingChannel = this.channels[id]

            // If we already have a channel for this ID, replace its underlying RTCDataChannel
            // with the remote one so we can receive messages from the server
            if (existingChannel && existingChannel.type === "data") {
                this.logger?.debug(`Replacing underlying channel for ${label} with remote channel`);
                (existingChannel as WebRTCDataTransportChannel).replaceChannel(remoteChannel)
            } else {
                this.logger?.debug(`Creating new channel for ${label}`)
                this.channels[id] = new WebRTCDataTransportChannel(label, remoteChannel)
            }
        } else {
            this.logger?.debug(`Unknown remote data channel: ${label}`)
        }
    }

    async setupHostVideo(_setup: TransportVideoSetup): Promise<VideoCodecSupport> {
        // TODO: check transport type

        let capabilities
        if ("getCapabilities" in RTCRtpReceiver && (capabilities = RTCRtpReceiver.getCapabilities("video"))) {
            const codecs = emptyVideoCodecs()

            for (const codec in codecs) {
                const supportRequirements = CAPABILITIES_CODECS[codec]

                if (!supportRequirements) {
                    continue
                }

                let supported = false
                capabilityCodecLoop: for (const codecCapability of capabilities.codecs) {
                    if (codecCapability.mimeType != supportRequirements.mimeType) {
                        continue
                    }

                    for (const fmtpLine of supportRequirements.fmtpLine) {
                        if (!codecCapability.sdpFmtpLine?.includes(fmtpLine)) {
                            continue capabilityCodecLoop
                        }
                    }

                    supported = true
                    break
                }

                codecs[codec] = supported
            }

            return codecs
        } else {
            return maybeVideoCodecs()
        }
    }

    async setupHostAudio(_setup: TransportAudioSetup): Promise<void> {
        // TODO: check transport type
    }

    getChannel(id: TransportChannelIdValue): TransportChannel {
        const channel = this.channels[id]
        if (!channel) {
            this.logger?.debug("Failed to setup video without peer")
            throw `Failed to get channel because it is not yet initialized, Id: ${id}`
        }

        return channel
    }

    onconnect: (() => void) | null = null

    onclose: ((shutdown: TransportShutdown) => void) | null = null
    async close(): Promise<void> {
        this.logger?.debug("Closing WebRTC Peer")

        this.peer?.close()
    }

    async getStats(): Promise<Record<string, string>> {
        const statsData: Record<string, string> = {}

        if (!this.videoReceiver) {
            return {}
        }
        const stats = await this.videoReceiver.getStats()

        console.debug("----------------- raw video stats -----------------")
        for (const [key, value] of stats.entries()) {
            console.debug("raw video stats", key, value)

            if ("decoderImplementation" in value && value.decoderImplementation != null) {
                statsData.decoderImplementation = value.decoderImplementation
            }
            if ("frameWidth" in value && value.frameWidth != null) {
                statsData.videoWidth = value.frameWidth
            }
            if ("frameHeight" in value && value.frameHeight != null) {
                statsData.videoHeight = value.frameHeight
            }
            if ("framesPerSecond" in value && value.framesPerSecond != null) {
                statsData.webrtcFps = value.framesPerSecond
            }

            if ("jitterBufferDelay" in value && value.jitterBufferDelay != null) {
                statsData.webrtcJitterBufferDelayMs = value.jitterBufferDelay
            }
            if ("jitterBufferTargetDelay" in value && value.jitterBufferTargetDelay != null) {
                statsData.webrtcJitterBufferTargetDelayMs = value.jitterBufferTargetDelay
            }
            if ("jitterBufferMinimumDelay" in value && value.jitterBufferMinimumDelay != null) {
                statsData.webrtcJitterBufferMinimumDelayMs = value.jitterBufferMinimumDelay
            }
            if ("jitter" in value && value.jitter != null) {
                statsData.webrtcJitterMs = value.jitter
            }
            if ("totalDecodeTime" in value && value.totalDecodeTime != null) {
                statsData.webrtcTotalDecodeTimeMs = value.totalDecodeTime
            }
            if ("totalAssemblyTime" in value && value.totalAssemblyTime != null) {
                statsData.webrtcTotalAssemblyTimeMs = value.totalAssemblyTime
            }
            if ("totalProcessingDelay" in value && value.totalProcessingDelay != null) {
                statsData.webrtcTotalProcessingDelayMs = value.totalProcessingDelay
            }
            if ("packetsReceived" in value && value.packetsReceived != null) {
                statsData.webrtcPacketsReceived = value.packetsReceived
            }
            if ("packetsLost" in value && value.packetsLost != null) {
                statsData.webrtcPacketsLost = value.packetsLost
            }
            if ("framesDropped" in value && value.framesDropped != null) {
                statsData.webrtcFramesDropped = value.framesDropped
            }
            if ("keyFramesDecoded" in value && value.keyFramesDecoded != null) {
                statsData.webrtcKeyFramesDecoded = value.keyFramesDecoded
            }
            if ("nackCount" in value && value.nackCount != null) {
                statsData.webrtcNackCount = value.nackCount
            }
        }

        return statsData
    }
}

type TrackHolder = {
    ontrack: (() => void) | null
    track: MediaStreamTrack | null
}

// This receives track data
class WebRTCInboundTrackTransportChannel<T extends string> implements TrackTransportChannel {
    type: T

    canReceive: boolean = true
    canSend: boolean = false

    private logger: Logger | null

    private label: string
    private trackHolder: TrackHolder

    constructor(logger: Logger | null, type: T, label: string, trackHolder: TrackHolder) {
        this.logger = logger

        this.type = type
        this.label = label
        this.trackHolder = trackHolder

        this.trackHolder.ontrack = this.onTrack.bind(this)
    }
    setTrack(_track: MediaStreamTrack | null): void {
        throw "WebRTCInboundTrackTransportChannel cannot addTrack"
    }

    private onTrack() {
        const track = this.trackHolder.track
        if (!track) {
            this.logger?.debug("WebRTC TrackHolder.track is null!")
            return
        }

        for (const listener of this.trackListeners) {
            listener(track)
        }
    }


    private trackListeners: Array<(track: MediaStreamTrack) => void> = []
    addTrackListener(listener: (track: MediaStreamTrack) => void): void {
        if (this.trackHolder.track) {
            listener(this.trackHolder.track)
        }
        this.trackListeners.push(listener)
    }
    removeTrackListener(listener: (track: MediaStreamTrack) => void): void {
        const index = this.trackListeners.indexOf(listener)
        if (index != -1) {
            this.trackListeners.splice(index, 1)
        }
    }
}

class WebRTCDataTransportChannel implements DataTransportChannel {
    type: "data" = "data"

    canReceive: boolean = true
    canSend: boolean = true

    private label: string
    private channel: RTCDataChannel
    private boundOnMessage: (event: MessageEvent) => void

    constructor(label: string, channel: RTCDataChannel) {
        this.label = label
        this.channel = channel
        this.boundOnMessage = this.onMessage.bind(this)

        this.channel.addEventListener("message", this.boundOnMessage)
    }

    // Replace the underlying channel with a new one (e.g., from remote peer)
    // This is used when we receive a data channel from the server that should
    // replace our locally created one for receiving messages
    replaceChannel(newChannel: RTCDataChannel): void {
        // Remove listener from old channel
        this.channel.removeEventListener("message", this.boundOnMessage)
        // Add listener to new channel
        this.channel = newChannel
        this.channel.addEventListener("message", this.boundOnMessage)
    }

    private sendQueue: Array<ArrayBuffer> = []
    send(message: ArrayBuffer): void {
        console.debug(this.label, message)

        if (this.channel.readyState != "open") {
            console.debug(`Tried sending packet to ${this.label} with readyState ${this.channel.readyState}. Buffering it for the future.`)
            this.sendQueue.push(message)
        } else {
            this.tryDequeueSendQueue()
            this.channel.send(message)
        }
    }
    private tryDequeueSendQueue() {
        for (const message of this.sendQueue) {
            this.channel.send(message)
        }
        this.sendQueue.length = 0
    }

    private onMessage(event: MessageEvent) {
        const data = event.data
        if (!(data instanceof ArrayBuffer)) {
            console.warn(`received text data on webrtc channel ${this.label}`)
            return
        }

        for (const listener of this.receiveListeners) {
            listener(event.data)
        }
    }
    private receiveListeners: Array<(data: ArrayBuffer) => void> = []
    addReceiveListener(listener: (data: ArrayBuffer) => void): void {
        this.receiveListeners.push(listener)
    }
    removeReceiveListener(listener: (data: ArrayBuffer) => void): void {
        const index = this.receiveListeners.indexOf(listener)
        if (index != -1) {
            this.receiveListeners.splice(index, 1)
        }
    }
    estimatedBufferedBytes(): number {
        return this.channel.bufferedAmount
    }
}