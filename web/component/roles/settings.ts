import { StreamPermissions, StreamSettings } from "../../api_bindings.js";
import { createSupportedVideoFormatsBits, emptyVideoCodecs } from "../../stream/video.js";
import { Component } from "../index.js";
import { InputComponent, SelectComponent } from "../input.js";

export function defaultStreamSettings(): StreamSettings {
    const videoCodecs = emptyVideoCodecs()
    videoCodecs.H264 = true
    videoCodecs.H264_HIGH8_444 = true

    return {
        bitrate_kpbs: 10000,
        width: 1920,
        height: 1080,
        fps: 60,
        play_audio_local: false,
        supported_codecs: createSupportedVideoFormatsBits(videoCodecs),
        hdr: false,
    }
}

export class RoleSettingsMenu implements Component {

    private rootDiv = document.createElement("div")

    private bitrateKpbs: InputComponent
    private videoSize: SelectComponent
    private videoWidth: InputComponent
    private videoHeight: InputComponent
    private fps: InputComponent
    private playAudioLocal: InputComponent
    private hdr: InputComponent

    constructor(settings: StreamSettings = defaultStreamSettings(), permissions?: StreamPermissions) {

        // Bitrate
        this.bitrateKpbs = new InputComponent("bitrateKbps", "number", "Bitrate (Kpbs)", {
            numberSlider: {
                range_min: Math.min(1000, permissions?.maximum_bitrate_kbps ?? 1000),
                range_max: permissions?.maximum_bitrate_kbps ? Math.min(10000, permissions.maximum_bitrate_kbps) : 10000
            },
            step: "100",
            defaultValue: `${settings.bitrate_kpbs}`
        })
        this.bitrateKpbs.mount(this.rootDiv)
        this.bitrateKpbs.addChangeListener(this.onChange.bind(this))

        // Video Size
        // Figure out the selected video size or custom
        let preSelectedVideoSize = "custom"
        if (settings.width === 1280 && settings.height === 720) {
            preSelectedVideoSize = "720p"
        } else if (settings.width === 1920 && settings.height === 1080) {
            preSelectedVideoSize = "1080p"
        } else if (settings.width === 2560 && settings.height === 1440) {
            preSelectedVideoSize = "1440p"
        } else if (settings.width === 3840 && settings.height === 2160) {
            preSelectedVideoSize = "4k"
        }

        this.videoSize = new SelectComponent("videoSize",
            [
                { value: "720p", name: "720p" },
                { value: "1080p", name: "1080p" },
                { value: "1440p", name: "1440p" },
                { value: "4k", name: "4k" },
                { value: "custom", name: "custom" }
            ],
            {
                displayName: "Video Size",
                preSelectedOption: preSelectedVideoSize
            }
        )
        this.videoSize.mount(this.rootDiv)
        this.videoSize.addChangeListener(this.onChange.bind(this))

        this.videoWidth = new InputComponent("videoWidth", "number", "Video Width", {
            defaultValue: `${settings.width}`
        })
        this.videoWidth.mount(this.rootDiv)
        this.videoWidth.addChangeListener(this.onChange.bind(this))

        this.videoHeight = new InputComponent("videoHeight", "number", "Video Height", {
            defaultValue: `${settings.height}`
        })
        this.videoHeight.mount(this.rootDiv)
        this.videoHeight.addChangeListener(this.onChange.bind(this))

        // Fps
        this.fps = new InputComponent("fps", "number", "Fps", {
            defaultValue: `${settings.fps}`
        })
        this.fps.mount(this.rootDiv)
        this.fps.addChangeListener(this.onChange.bind(this))

        // Play Audio Local
        this.playAudioLocal = new InputComponent("playAudioLocal", "checkbox", "Play Audio Local", {
            checked: settings.play_audio_local
        })
        this.playAudioLocal.mount(this.rootDiv)
        this.playAudioLocal.addChangeListener(this.onChange.bind(this))

        // Hdr
        this.hdr = new InputComponent("hdr", "checkbox", "Hdr", {
            checked: settings.hdr
        })
        this.hdr.mount(this.rootDiv)
        this.hdr.addChangeListener(this.onChange.bind(this))

        // This will update some fields
        this.onChange()
    }

    private onChange() {
        const useCustomVideoSize = this.videoSize.getValue() == "custom"
        this.videoWidth.setEnabled(useCustomVideoSize)
        this.videoHeight.setEnabled(useCustomVideoSize)
    }

    mount(parent: HTMLElement): void {
        parent.appendChild(this.rootDiv)
    }
    unmount(parent: HTMLElement): void {
        parent.removeChild(this.rootDiv)
    }

    getSettings(): StreamSettings {
        let width
        let height
        const videoSize = this.videoSize.getValue()
        if (videoSize == "720p") {
            width = 1280
            height = 720
        } else if (videoSize == "1080p") {
            width = 1920
            height = 1080
        } else if (videoSize == "1440p") {
            width = 2560
            height = 1440
        } else if (videoSize == "4k") {
            width = 3840
            height = 2160
        } else {
            width = parseInt(this.videoWidth.getValue())
            height = parseInt(this.videoHeight.getValue())
        }

        return {
            bitrate_kpbs: parseInt(this.bitrateKpbs.getValue()),
            width,
            height,
            fps: parseInt(this.fps.getValue()),
            play_audio_local: false,
            supported_codecs: 0,
            hdr: false,
        }
    }
}