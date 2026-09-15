import { ControllerConfig } from "../stream/gamepad"
import { MouseMode, MouseScrollMode, TouchMode } from "../stream/input"
import { PageStyle } from "../styles/index"
import { getLanguageOptions, getTranslations, Language, normalizeLanguage } from "../i18n"
import { Component, ComponentEvent } from "./index"
import { InputComponent, SelectComponent } from "./input"
import { SidebarEdge } from "./sidebar/index"
import { showCustomFpsPrompt, showCustomResolutionPrompt } from "./modal/custom_video"

export type Settings = {
    sidebarEdge: SidebarEdge,
    hideSidebarButton: boolean,
    bitrate: number
    videoSize: "720p" | "1080p" | "1440p" | "4k" | "native" | "custom"
    videoSizeCustom: {
        width: number
        height: number
    },
    fps: number
    videoCodec: StreamCodec,
    forceVideoElementRenderer: boolean
    canvasRenderer: boolean
    canvasVsync: boolean
    playAudioLocal: boolean
    mouseScrollMode: MouseScrollMode
    mouseMode: MouseMode
    touchMode: TouchMode
    localCursorSensitivity: number
    controllerConfig: ControllerConfig
    dataTransport: TransportType
    language: Language
    enterFullscreenOnStreamStart: boolean
    toggleFullscreenWithKeybind: boolean
    pageStyle: PageStyle
    hdr: boolean
    useSelectElementPolyfill: boolean
    swapMouseButtons: boolean
    reverseScrollDirection: boolean
    quitAppOnExit: boolean
    keepDisplayAwake: boolean
    showConnectionWarnings: boolean
    yuv444: boolean
}

export type StreamCodec = "h264" | "auto" | "h265" | "av1"
export type TransportType = "auto" | "webrtc" | "websocket"

import DEFAULT_SETTINGS from "../default_settings"

export function globalDefaultSettings(): Settings {
    // We are deep cloning this
    return deepClone(DEFAULT_SETTINGS)
}

function deepClone<T>(value: T): T {
    if (typeof structuredClone == "function") {
        return structuredClone(value)
    } else {
        return JSON.parse(JSON.stringify(value))
    }
}
function deepMerge(target: any, source: any) {
    for (const key in source) {
        const sourceVal = source[key]
        const targetVal = target[key]

        if (
            sourceVal &&
            typeof sourceVal === "object" &&
            !Array.isArray(sourceVal)
        ) {
            target[key] = deepMerge(
                targetVal && typeof targetVal === "object" ? targetVal : {},
                sourceVal
            )
        } else if (sourceVal !== undefined) {
            target[key] = sourceVal
        }
    }
    return target
}

export function getLocalStreamSettings(defaultSettings: Settings) {
    // Start with FULL global defaults
    let settings = globalDefaultSettings()

    // Fill/override with role defaults (even if partial)
    settings = deepMerge(settings, defaultSettings)

    try {
        const json = localStorage.getItem("mlSettings")
        if (json) {
            const loaded = JSON.parse(json)

            // Finally override with user settings
            settings = deepMerge(settings, loaded)
        }
    } catch (e) {
        localStorage.removeItem("mlSettings")
    }

    // Migration
    if (settings?.pageStyle === "old") {
        settings.pageStyle = "moonlight"
    }

    return settings
}
export function setLocalStreamSettings(settings?: Settings) {
    localStorage.setItem("mlSettings", JSON.stringify(settings))
}

export type StreamSettingsChangeListener = (event: ComponentEvent<StreamSettingsComponent>) => void

function makeSettingsValid(settings: Settings) {
    if (!Number.isFinite(settings.localCursorSensitivity) || settings.localCursorSensitivity <= 0) {
        settings.localCursorSensitivity = globalDefaultSettings().localCursorSensitivity
    }
}

export class StreamSettingsComponent implements Component {

    private divElement: HTMLDivElement = document.createElement("div")

    private sidebarEdge: SelectComponent
    private hideSidebarButton: InputComponent

    private bitrate: HTMLInputElement = document.createElement("input")
    private bitrateLabel: HTMLParagraphElement = document.createElement("p")
    private bitrateLabelText: string
    private fps: SelectComponent
    private lastFps: string
    private customFps: number
    private displayMode: SelectComponent
    private videoCodec: SelectComponent
    private forceVideoElementRenderer: InputComponent
    private canvasRenderer: InputComponent
    private canvasVsync: InputComponent
    private hdr: InputComponent

    private videoSize: SelectComponent
    private lastVideoSize: string
    private customVideoSize: { width: number, height: number }

    private playAudioLocal: InputComponent

    private mouseScrollMode: SelectComponent
    private mouseMode: SelectComponent
    private touchMode: SelectComponent
    private localCursorSensitivity: InputComponent

    private controllerInvertAB: InputComponent
    private controllerInvertXY: InputComponent
    private controllerSendIntervalOverride: InputComponent

    private dataTransport: SelectComponent
    private language: SelectComponent
    private toggleFullscreenWithKeybind: InputComponent

    private pageStyle: SelectComponent

    private useSelectElementPolyfill: InputComponent

    private swapMouseButtons: InputComponent
    private reverseScrollDirection: InputComponent
    private quitAppOnExit: InputComponent
    private keepDisplayAwake: InputComponent
    private showConnectionWarnings: InputComponent
    private yuv444: InputComponent

    private customLabel: string

    constructor(settings: Settings) {
        // Sometimes the normal settings object doesn't have some values, because they change between versions.
        // Use those as fallback
        const defaultSettings_ = globalDefaultSettings()

        makeSettingsValid(defaultSettings_)
        makeSettingsValid(settings)
        const language = normalizeLanguage(settings?.language ?? defaultSettings_.language)
        const translations = getTranslations(language)
        const i = translations.settings
        const streamI = translations.stream

        // Root div: Moonlight-style two column layout of section cards
        this.divElement.classList.add("settings")

        const columnLeft = document.createElement("div")
        columnLeft.classList.add("settings-column")
        this.divElement.appendChild(columnLeft)
        const columnRight = document.createElement("div")
        columnRight.classList.add("settings-column")
        this.divElement.appendChild(columnRight)

        const createSection = (column: HTMLElement, title: string) => {
            const section = document.createElement("div")
            section.classList.add("settings-section")
            const header = document.createElement("h3")
            header.classList.add("settings-section-title")
            header.innerText = title
            section.appendChild(header)
            column.appendChild(section)
            return section
        }

        const basicSection = createSection(columnLeft, i.basicSettings)
        const audioSection = createSection(columnLeft, i.audioSettings)
        const uiSection = createSection(columnLeft, i.uiSettings)
        const inputSection = createSection(columnRight, i.inputSettings)
        const gamepadSection = createSection(columnRight, i.gamepadSettings)
        const advancedSection = createSection(columnRight, i.advancedSettings)

        const createGroup = (section: HTMLElement, title: string, description?: string) => {
            const group = document.createElement("div")
            group.classList.add("settings-group")
            const heading = document.createElement("p")
            heading.classList.add("settings-group-title")
            heading.innerText = title
            group.appendChild(heading)
            if (description) {
                const desc = document.createElement("p")
                desc.classList.add("settings-group-description")
                desc.innerText = description
                group.appendChild(desc)
            }
            section.appendChild(group)
            return { group, heading }
        }

        // Basic Settings: resolution + fps on one row, then bitrate
        const resFpsGroup = createGroup(basicSection, i.resolutionAndFps, i.resolutionAndFpsDescription).group
        const resFpsRow = document.createElement("div")
        resFpsRow.classList.add("settings-row")
        resFpsGroup.appendChild(resFpsRow)

        this.customLabel = i.custom

        // Video Size (Moonlight-style: "Custom" opens a prompt and then shows the chosen size inline)
        this.customVideoSize = {
            width: settings?.videoSizeCustom?.width ?? defaultSettings_.videoSizeCustom.width,
            height: settings?.videoSizeCustom?.height ?? defaultSettings_.videoSizeCustom.height,
        }
        const initialVideoSize = settings?.videoSize || defaultSettings_.videoSize
        this.lastVideoSize = initialVideoSize

        this.videoSize = new SelectComponent("videoSize",
            [
                { value: "720p", name: "720p" },
                { value: "1080p", name: "1080p" },
                { value: "1440p", name: "1440p" },
                { value: "4k", name: "4k" },
                { value: "native", name: i.native },
                { value: "custom", name: initialVideoSize == "custom" ? this.customResolutionLabel() : i.custom }
            ],
            {
                preSelectedOption: initialVideoSize
            }
        )
        this.videoSize.addChangeListener(this.onVideoSizeChange.bind(this))
        this.videoSize.mount(resFpsRow)

        // Fps (Moonlight-style dropdown with presets + custom prompt)
        const fpsPresets = ["30", "60", "90", "120", "144", "240"]
        const fpsValue = settings?.fps ?? defaultSettings_.fps
        const isPresetFps = fpsPresets.includes(fpsValue.toString())
        this.customFps = fpsValue
        this.lastFps = isPresetFps ? fpsValue.toString() : "custom"

        this.fps = new SelectComponent("fps",
            [
                ...fpsPresets.map(value => ({ value, name: `${value} FPS` })),
                { value: "custom", name: isPresetFps ? i.custom : this.customFpsLabel() }
            ],
            {
                preSelectedOption: this.lastFps
            }
        )
        this.fps.addChangeListener(this.onFpsChange.bind(this))
        this.fps.mount(resFpsRow)

        // Bitrate (Mbps in the UI, stored as Kbps internally)
        this.bitrateLabelText = i.bitrate
        const bitrateGroup = createGroup(basicSection, "", i.bitrateDescription)
        this.bitrateLabel = bitrateGroup.heading
        this.bitrate.id = "bitrate"
        this.bitrate.type = "range"
        this.bitrate.classList.add("bitrate-slider")
        this.bitrate.min = "0.5"
        this.bitrate.max = "150"
        this.bitrate.step = "0.5"
        this.bitrate.value = ((settings?.bitrate ?? defaultSettings_.bitrate) / 1000).toString()
        this.bitrate.addEventListener("input", () => this.updateBitrateLabel())
        this.bitrate.addEventListener("change", this.onSettingsChange.bind(this))
        bitrateGroup.group.appendChild(this.bitrate)
        this.updateBitrateLabel()

        // Display Mode (Moonlight-style select instead of a bare checkbox)
        const displayModeGroup = createGroup(basicSection, i.displayMode).group
        this.displayMode = new SelectComponent("displayMode",
            [
                { value: "windowed", name: i.displayModeWindowed },
                { value: "fullscreen", name: i.displayModeFullscreen }
            ],
            {
                preSelectedOption: (settings?.enterFullscreenOnStreamStart ?? defaultSettings_.enterFullscreenOnStreamStart) ? "fullscreen" : "windowed"
            }
        )
        this.displayMode.addChangeListener(this.onSettingsChange.bind(this))
        this.displayMode.mount(displayModeGroup)

        this.quitAppOnExit = new InputComponent("quitAppOnExit", "checkbox", i.quitAppOnExit, {
            checked: settings?.quitAppOnExit ?? defaultSettings_.quitAppOnExit
        })
        this.quitAppOnExit.addChangeListener(this.onSettingsChange.bind(this))
        this.quitAppOnExit.mount(basicSection)

        // Codec
        const allowedVideoCodecs = [
            { value: "auto", name: i.autoExperimental },
        ]
        allowedVideoCodecs.push(
            { value: "h264", name: "H264" },
            { value: "h265", name: "H265" },
            { value: "av1", name: i.av1Experimental },
        )

        this.videoCodec = new SelectComponent("videoCodec", allowedVideoCodecs, {
            displayName: i.videoCodec,
            preSelectedOption: settings?.videoCodec ?? defaultSettings_.videoCodec
        })
        this.videoCodec.addChangeListener(this.onSettingsChange.bind(this))
        this.videoCodec.mount(advancedSection)

        // Force Video Element renderer
        this.forceVideoElementRenderer = new InputComponent("forceVideoElementRenderer", "checkbox", i.forceVideoElementRenderer, {
            checked: settings?.forceVideoElementRenderer ?? defaultSettings_.forceVideoElementRenderer
        })
        this.forceVideoElementRenderer.addChangeListener(this.onSettingsChange.bind(this))
        this.forceVideoElementRenderer.mount(advancedSection)

        // Use Canvas Renderer
        this.canvasRenderer = new InputComponent("canvasRenderer", "checkbox", i.useCanvasRenderer, {
            defaultValue: defaultSettings_.canvasRenderer.toString(),
            checked: settings === null || settings === void 0 ? void 0 : settings.canvasRenderer
        })
        this.canvasRenderer.addChangeListener(this.onSettingsChange.bind(this))
        this.canvasRenderer.mount(advancedSection)

        // Canvas VSync (Canvas only: sync draw to display refresh to reduce tearing; off = lower latency)
        this.canvasVsync = new InputComponent("canvasVsync", "checkbox", i.canvasVsync, {
            checked: settings?.canvasVsync ?? defaultSettings_.canvasVsync
        })
        this.canvasVsync.addChangeListener(this.onSettingsChange.bind(this))
        this.canvasVsync.mount(advancedSection)

        // HDR
        this.hdr = new InputComponent("hdr", "checkbox", i.enableHdr, {
            checked: settings?.hdr ?? defaultSettings_.hdr
        })
        this.hdr.addChangeListener(this.onSettingsChange.bind(this))
        this.hdr.mount(advancedSection)

        // YUV 4:4:4
        this.yuv444 = new InputComponent("yuv444", "checkbox", i.yuv444, {
            checked: settings?.yuv444 ?? defaultSettings_.yuv444
        })
        this.yuv444.addChangeListener(this.onSettingsChange.bind(this))
        this.yuv444.mount(advancedSection)

        // Audio local
        this.playAudioLocal = new InputComponent("playAudioLocal", "checkbox", i.playAudioLocal, {
            checked: settings?.playAudioLocal
        })
        this.playAudioLocal.addChangeListener(this.onSettingsChange.bind(this))
        this.playAudioLocal.mount(audioSection)

        this.mouseScrollMode = new SelectComponent("mouseScrollMode",
            [
                { value: "highres", name: i.highRes },
                { value: "normal", name: i.normal }
            ],
            {
                displayName: i.scrollMode,
                preSelectedOption: settings?.mouseScrollMode || defaultSettings_.mouseScrollMode
            }
        )
        this.mouseScrollMode.addChangeListener(this.onSettingsChange.bind(this))
        this.mouseScrollMode.mount(inputSection)

        this.mouseMode = new SelectComponent("mouseMode",
            [
                { value: "relative", name: streamI.relative },
                { value: "follow", name: streamI.follow },
                { value: "localCursor", name: streamI.localCursor },
                { value: "pointAndDrag", name: streamI.pointAndDrag }
            ],
            {
                displayName: i.startupMouseMode,
                preSelectedOption: settings?.mouseMode ?? defaultSettings_.mouseMode
            }
        )
        this.mouseMode.addChangeListener(this.onSettingsChange.bind(this))
        this.mouseMode.mount(inputSection)

        this.touchMode = new SelectComponent("touchMode",
            [
                { value: "touch", name: streamI.touch },
                { value: "mouseRelative", name: streamI.relative },
                { value: "localCursor", name: streamI.localCursor },
                { value: "pointAndDrag", name: streamI.pointAndDrag }
            ],
            {
                displayName: i.startupTouchMode,
                preSelectedOption: settings?.touchMode ?? defaultSettings_.touchMode
            }
        )
        this.touchMode.addChangeListener(this.onSettingsChange.bind(this))
        this.touchMode.mount(inputSection)

        this.localCursorSensitivity = new InputComponent("localCursorSensitivity", "number", i.localCursorSensitivity, {
            defaultValue: defaultSettings_.localCursorSensitivity.toString(),
            value: settings?.localCursorSensitivity?.toString(),
            step: "0.1",
            numberSlider: {
                range_min: 0.1,
                range_max: 3
            }
        })
        this.localCursorSensitivity.addChangeListener(this.onSettingsChange.bind(this))
        this.localCursorSensitivity.mount(inputSection)

        this.swapMouseButtons = new InputComponent("swapMouseButtons", "checkbox", i.swapMouseButtons, {
            checked: settings?.swapMouseButtons ?? defaultSettings_.swapMouseButtons
        })
        this.swapMouseButtons.addChangeListener(this.onSettingsChange.bind(this))
        this.swapMouseButtons.mount(inputSection)

        this.reverseScrollDirection = new InputComponent("reverseScrollDirection", "checkbox", i.reverseScrollDirection, {
            checked: settings?.reverseScrollDirection ?? defaultSettings_.reverseScrollDirection
        })
        this.reverseScrollDirection.addChangeListener(this.onSettingsChange.bind(this))
        this.reverseScrollDirection.mount(inputSection)

        this.controllerInvertAB = new InputComponent("controllerInvertAB", "checkbox", i.invertAB, {
            checked: settings?.controllerConfig?.invertAB
        })
        this.controllerInvertAB.addChangeListener(this.onSettingsChange.bind(this))
        this.controllerInvertAB.mount(gamepadSection)

        this.controllerInvertXY = new InputComponent("controllerInvertXY", "checkbox", i.invertXY, {
            checked: settings?.controllerConfig?.invertXY
        })
        this.controllerInvertXY.addChangeListener(this.onSettingsChange.bind(this))
        this.controllerInvertXY.mount(gamepadSection)

        // Controller Send Interval
        this.controllerSendIntervalOverride = new InputComponent("controllerSendIntervalOverride", "number", i.overrideControllerInterval, {
            hasEnableCheckbox: true,
            defaultValue: "20",
            value: settings?.controllerConfig?.sendIntervalOverride?.toString(),
            numberSlider: {
                range_min: 10,
                range_max: 120
            }
        })
        this.controllerSendIntervalOverride.setEnabled(settings?.controllerConfig?.sendIntervalOverride != null)
        this.controllerSendIntervalOverride.addChangeListener(this.onSettingsChange.bind(this))
        this.controllerSendIntervalOverride.mount(gamepadSection)

        if (!window.isSecureContext) {
            this.controllerInvertAB.setEnabled(false)
            this.controllerInvertXY.setEnabled(false)
        }

        // Data Transport
        const allowedDataTransport = [
            { value: "auto", name: i.auto },
        ]
        allowedDataTransport.push(
            { value: "webrtc", name: "WebRTC" },
            { value: "websocket", name: i.webSocket },
        )

        this.language = new SelectComponent("language", getLanguageOptions(), {
            displayName: i.language,
            preSelectedOption: language
        })
        this.language.addChangeListener(this.onSettingsChange.bind(this))
        this.language.mount(uiSection)

        this.dataTransport = new SelectComponent("transport", allowedDataTransport, {
            displayName: i.dataTransport,
            preSelectedOption: settings?.dataTransport ?? defaultSettings_.dataTransport
        })
        this.dataTransport.addChangeListener(this.onSettingsChange.bind(this))
        this.dataTransport.mount(advancedSection)

        this.sidebarEdge = new SelectComponent("sidebarEdge", [
            { value: "left", name: i.left },
            { value: "right", name: i.right },
            { value: "up", name: i.up },
            { value: "down", name: i.down },
        ], {
            displayName: i.sidebarEdge,
            preSelectedOption: settings?.sidebarEdge ?? defaultSettings_.sidebarEdge,
        })
        this.sidebarEdge.addChangeListener(this.onSettingsChange.bind(this))
        this.sidebarEdge.mount(uiSection)

        this.hideSidebarButton = new InputComponent("hideSidebarButton", "checkbox", i.hideSidebarButton, {
            checked: settings?.hideSidebarButton ?? defaultSettings_.hideSidebarButton
        })
        this.hideSidebarButton.addChangeListener(this.onSettingsChange.bind(this))
        this.hideSidebarButton.mount(uiSection)

        // Fullscreen Keybind
        this.toggleFullscreenWithKeybind = new InputComponent("toggleFullscreenWithKeybind", "checkbox", i.toggleFullscreenWithKeybind, {
            checked: settings?.toggleFullscreenWithKeybind
        })
        this.toggleFullscreenWithKeybind.addChangeListener(this.onSettingsChange.bind(this))
        this.toggleFullscreenWithKeybind.mount(uiSection)

        // Page Style
        this.pageStyle = new SelectComponent("pageStyle", [
            { value: "standard", name: "Standard" },
            { value: "moonlight", name: "Moonlight" },
        ], {
            displayName: i.style,
            preSelectedOption: settings?.pageStyle ?? defaultSettings_.pageStyle
        })
        this.pageStyle.addChangeListener(this.onSettingsChange.bind(this))
        this.pageStyle.mount(uiSection)

        // Custom Select Element
        this.useSelectElementPolyfill = new InputComponent("useSelectElementPolyfill", "checkbox", i.useCustomDropdown, {
            checked: settings?.useSelectElementPolyfill ?? defaultSettings_.useSelectElementPolyfill
        })
        this.useSelectElementPolyfill.addChangeListener(this.onSettingsChange.bind(this))
        this.useSelectElementPolyfill.mount(uiSection)

        this.keepDisplayAwake = new InputComponent("keepDisplayAwake", "checkbox", i.keepDisplayAwake, {
            checked: settings?.keepDisplayAwake ?? defaultSettings_.keepDisplayAwake
        })
        this.keepDisplayAwake.addChangeListener(this.onSettingsChange.bind(this))
        this.keepDisplayAwake.mount(uiSection)

        this.showConnectionWarnings = new InputComponent("showConnectionWarnings", "checkbox", i.showConnectionWarnings, {
            checked: settings?.showConnectionWarnings ?? defaultSettings_.showConnectionWarnings
        })
        this.showConnectionWarnings.addChangeListener(this.onSettingsChange.bind(this))
        this.showConnectionWarnings.mount(uiSection)

        this.onSettingsChange()
    }

    private customResolutionLabel(): string {
        return `${this.customLabel} (${this.customVideoSize.width}x${this.customVideoSize.height})`
    }
    private updateBitrateLabel() {
        const mbps = parseFloat(this.bitrate.value)
        this.bitrateLabel.innerText = `${this.bitrateLabelText}: ${Number.isFinite(mbps) ? mbps : ""} Mbps`
    }

    private customFpsLabel(): string {
        return `${this.customLabel} (${this.customFps} FPS)`
    }

    private async onVideoSizeChange() {
        const value = this.videoSize.getValue() ?? this.lastVideoSize
        if (value == "custom") {
            const result = await showCustomResolutionPrompt(this.customVideoSize)
            if (result == null) {
                this.videoSize.setValue(this.lastVideoSize)
                return
            }
            this.customVideoSize = result
            this.videoSize.setOptionName("custom", this.customResolutionLabel())
        } else {
            this.videoSize.setOptionName("custom", this.customLabel)
        }
        this.lastVideoSize = value
        this.onSettingsChange()
    }

    private async onFpsChange() {
        const value = this.fps.getValue() ?? this.lastFps
        if (value == "custom") {
            const result = await showCustomFpsPrompt(this.customFps)
            if (result == null) {
                this.fps.setValue(this.lastFps)
                return
            }
            this.customFps = result
            this.fps.setOptionName("custom", this.customFpsLabel())
        } else {
            this.fps.setOptionName("custom", this.customLabel)
        }
        this.lastFps = value
        this.onSettingsChange()
    }

    private onSettingsChange() {
        this.divElement.dispatchEvent(new ComponentEvent("ml-settingschange", this))
    }

    addChangeListener(listener: StreamSettingsChangeListener) {
        this.divElement.addEventListener("ml-settingschange", listener as any)
    }
    removeChangeListener(listener: StreamSettingsChangeListener) {
        this.divElement.removeEventListener("ml-settingschange", listener as any)
    }

    getStreamSettings(): Settings {
        const settings = globalDefaultSettings()

        settings.sidebarEdge = this.sidebarEdge.getValue() as any
        settings.hideSidebarButton = this.hideSidebarButton.isChecked()
        const bitrateMbps = parseFloat(this.bitrate.value)
        settings.bitrate = Number.isFinite(bitrateMbps)
            ? Math.min(Math.max(Math.round(bitrateMbps * 1000), 500), 150000)
            : globalDefaultSettings().bitrate
        const fpsValue = this.fps.getValue() ?? "60"
        const fps = fpsValue == "custom" ? this.customFps : parseInt(fpsValue)
        settings.fps = Number.isFinite(fps) && fps > 0 ? fps : globalDefaultSettings().fps
        settings.videoSize = this.videoSize.getValue() as any
        settings.videoSizeCustom = { ...this.customVideoSize }
        settings.videoCodec = this.videoCodec.getValue() as any
        settings.forceVideoElementRenderer = this.forceVideoElementRenderer.isChecked()
        settings.canvasRenderer = this.canvasRenderer.isChecked()
        settings.canvasVsync = this.canvasVsync.isChecked()

        settings.playAudioLocal = this.playAudioLocal.isChecked()

        settings.mouseScrollMode = this.mouseScrollMode.getValue() as any
        settings.mouseMode = this.mouseMode.getValue() as MouseMode
        settings.touchMode = this.touchMode.getValue() as TouchMode
        settings.localCursorSensitivity = parseFloat(this.localCursorSensitivity.getValue())

        settings.controllerConfig.invertAB = this.controllerInvertAB.isChecked()
        settings.controllerConfig.invertXY = this.controllerInvertXY.isChecked()
        if (this.controllerSendIntervalOverride.isEnabled()) {
            settings.controllerConfig.sendIntervalOverride = parseInt(this.controllerSendIntervalOverride.getValue())
        } else {
            settings.controllerConfig.sendIntervalOverride = null
        }

        settings.dataTransport = this.dataTransport.getValue() as any
        settings.language = this.language.getValue() as Language

        settings.enterFullscreenOnStreamStart = this.displayMode.getValue() == "fullscreen"
        settings.toggleFullscreenWithKeybind = this.toggleFullscreenWithKeybind.isChecked()

        settings.pageStyle = this.pageStyle.getValue() as any

        settings.hdr = this.hdr.isChecked()

        settings.useSelectElementPolyfill = this.useSelectElementPolyfill.isChecked()

        settings.swapMouseButtons = this.swapMouseButtons.isChecked()
        settings.reverseScrollDirection = this.reverseScrollDirection.isChecked()
        settings.quitAppOnExit = this.quitAppOnExit.isChecked()
        settings.keepDisplayAwake = this.keepDisplayAwake.isChecked()
        settings.showConnectionWarnings = this.showConnectionWarnings.isChecked()
        settings.yuv444 = this.yuv444.isChecked()

        makeSettingsValid(settings)

        return settings
    }

    mountBefore(parent: HTMLElement, before: HTMLElement): void {
        parent.insertBefore(this.divElement, before)
    }
    mount(parent: HTMLElement): void {
        parent.appendChild(this.divElement)
    }
    unmount(parent: HTMLElement): void {
        parent.removeChild(this.divElement)
    }
}
