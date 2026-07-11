import "./polyfill/index.js"
import { Api, apiGetRole, getApi } from "./api.js";
import { Component } from "./component/index.js";
import { showNotification } from "./component/notification.js";
import { InfoEvent, Stream } from "./stream/index.js"
import { getModalBackground, Modal, showMessage, showModal } from "./component/modal/index.js";
import { getSidebarRoot, setSidebar, setSidebarExtended, setSidebarStyle, Sidebar } from "./component/sidebar/index.js";
import { defaultStreamInputConfig, MouseMode, ScreenKeyboardSetVisibleEvent, StreamInputConfig } from "./stream/input.js";
import { getLocalStreamSettings, Settings, TransportType } from "./component/settings_menu.js";
import { SelectComponent } from "./component/input.js";
import { DetailedRole, LogMessageType, StreamCapabilities, StreamKeys, StreamPermissions } from "./api_bindings.js";
import { KeyboardModeEvent, KeyboardModeWillChangeEvent, ScreenKeyboard, TextEvent } from "./screen_keyboard.js";
import { FormModal } from "./component/modal/form.js";
import { streamStatsToText } from "./stream/stats.js";
import { adoptRoleDefaultLanguage, getCurrentLanguage, getTranslations, Language, normalizeLanguage } from "./i18n.js";
import { requestKeyboardLock } from "./iframe.js";

declare global {
    interface Window {
        vGamepad?: any;
        toggleVirtualGamepad?: (btn: HTMLButtonElement) => void;
        unplugVirtualGamepad?: (btn: HTMLButtonElement) => void;
    }
}

let I = getTranslations(getCurrentLanguage())

async function startApp() {
    const api = await getApi()

    const queryParams = new URLSearchParams(location.search)
    let lang = parseLanguageFromQuery(queryParams)
    const bootstrapRole = await apiGetRole(api, { id: null })
    if (!lang) {
        adoptRoleDefaultLanguage(bootstrapRole.role.default_settings)
        lang = getCurrentLanguage()
    }
    I = getTranslations(lang)

    const rootElement = document.getElementById("root");
    if (rootElement == null) {
        showNotification(I.stream.rootNotFound, "error")
        return;
    }

    // Get Host and App via Query
    const hostIdStr = queryParams.get("hostId")
    const appIdStr = queryParams.get("appId")
    if (hostIdStr == null || appIdStr == null) {
        await showMessage(I.stream.missingHostOrApp)

        window.close()
        return
    }
    const hostId = Number.parseInt(hostIdStr)
    const appId = Number.parseInt(appIdStr)

    // event propagation on overlays
    const sidebarRoot = getSidebarRoot()
    if (sidebarRoot) {
        stopPropagationOn(sidebarRoot)
    }

    const modalBackground = getModalBackground()
    if (modalBackground) {
        stopPropagationOn(modalBackground)
    }

    // Start and Mount App
    const app = new ViewerApp(api, hostId, appId, bootstrapRole.role, parseSettingsFromQuery(queryParams))
    app.mount(rootElement);

    (window as any)["app"] = app
}

// Prevent starting transition
window.requestAnimationFrame(() => {
    // Note: elements is a live array
    const elements = document.getElementsByClassName("prevent-start-transition")
    while (elements.length > 0) {
        elements.item(0)?.classList.remove("prevent-start-transition")
    }
})

function parseSettingsFromQuery(queryParams: URLSearchParams): Partial<Settings> {
    const settings: Partial<Settings> = {}

    const bitrate = queryParams.get("bitrate")
    if (bitrate) {
        settings.bitrate = Number(bitrate)
    }

    const fps = queryParams.get("fps")
    if (fps) {
        settings.fps = Number(fps)
    }

    const hdr = queryParams.get("hdr")
    if (hdr != null) {
        settings.hdr = hdr === "true"
    }

    const videoSize = queryParams.get("videoSize")
    if (videoSize) {
        settings.videoSize = videoSize as Settings["videoSize"]
    }

    const width = queryParams.get("videoSizeCustom.width")
    const height = queryParams.get("videoSizeCustom.height")
    if (width && height) {
        settings.videoSizeCustom = {
            width: Number(width),
            height: Number(height),
        }
    }

    const dataTransport = queryParams.get("dataTransport")
    if (dataTransport) {
        settings.dataTransport = dataTransport as TransportType
    }

    return settings
}

function parseLanguageFromQuery(queryParams: URLSearchParams): Language | undefined {
    const language = queryParams.get("language")
    return language ? normalizeLanguage(language) : undefined
}

startApp()

// Force disable highlighting globally on the entire document body
document.body.style.setProperty("-webkit-user-select", "none", "important");
document.body.style.setProperty("user-select", "none", "important");
document.body.style.setProperty("-webkit-touch-callout", "none", "important");
document.body.style.setProperty("-webkit-tap-highlight-color", "transparent", "important");

class ViewerApp implements Component {


    private api: Api

    private sidebar: ViewerSidebar

    private div = document.createElement("div")

    private statsDiv = document.createElement("div")
    private localTouchCursorDiv = document.createElement("div")
    private stream: Stream

    private inputConfig: StreamInputConfig = defaultStreamInputConfig()
    private previousMouseMode: MouseMode
    private autoEnterFullscreenOnStart: boolean = false
    private pendingAutoFullscreenPrompt: boolean = false
    private fullscreenPromptShown: boolean = false
    private fullscreenOnNextInteractionArmed: boolean = false
    private pendingAutoFullscreenTouchGesture: boolean = false
    private pendingAutoFullscreenMouseGesture: boolean = false
    private manualFullscreenExitRequested: boolean = false
    private toggleFullscreenWithKeybind: boolean = false
    private hasShownFullscreenEscapeWarning = false
    private keyboardViewportBaselineHeight: number | null = null
    private streamVideoTopOffsetPx: number = 0

    constructor(api: Api, hostId: number, appId: number, bootstrapRole: DetailedRole, options?: Partial<Settings>) {
        this.api = api

        const defaultSettings = getLocalStreamSettings(bootstrapRole.default_settings)
        const settings = {
            ...defaultSettings,
            ...options,
            videoSizeCustom: {
                ...defaultSettings.videoSizeCustom,
                ...options?.videoSizeCustom,
            },
        }
        Object.assign(this.inputConfig, {
            mouseMode: settings.mouseMode,
            mouseScrollMode: settings.mouseScrollMode,
            touchMode: settings.touchMode,
            localCursorSensitivity: settings.localCursorSensitivity,
            controllerConfig: settings.controllerConfig
        })

        // Configure sidebar
        this.sidebar = new ViewerSidebar(this)
        setSidebar(this.sidebar)

        // Configure stats element
        this.statsDiv.hidden = true
        this.statsDiv.classList.add("video-stats")
        this.localTouchCursorDiv.hidden = true
        this.localTouchCursorDiv.classList.add("local-touch-cursor")

        setInterval(() => {
            // Update stats display every 100ms
            const stats = this.getStream()?.getStats()
            if (stats && stats.isEnabled()) {
                this.statsDiv.hidden = false

                const text = streamStatsToText(stats.getCurrentStats())
                this.statsDiv.innerText = text
            } else {
                this.statsDiv.hidden = true
            }
        }, 100)
        this.div.appendChild(this.statsDiv)
        this.div.appendChild(this.localTouchCursorDiv)

        // Configure stream
        this.previousMouseMode = this.inputConfig.mouseMode

        const browserWidth = Math.max(document.documentElement.clientWidth || 0, window.innerWidth || 0)
        const browserHeight = Math.max(document.documentElement.clientHeight || 0, window.innerHeight || 0)

        this.autoEnterFullscreenOnStart = settings.enterFullscreenOnStreamStart
        this.toggleFullscreenWithKeybind = settings.toggleFullscreenWithKeybind

        this.stream = new Stream(this.api, hostId, appId, settings, [browserWidth, browserHeight], bootstrapRole.permissions)
        this.startStream(hostId, appId, bootstrapRole.permissions, settings, [browserWidth, browserHeight])

        // Configure input
        this.addListeners(document)
        this.addListeners(document.getElementById("input") as HTMLDivElement)

        window.addEventListener("blur", () => {
            this.stream.getInput().raiseAllKeys()
        })
        document.addEventListener("visibilitychange", () => {
            if (document.visibilityState !== "visible") {
                this.stream.getInput().raiseAllKeys()
            }
        })

        document.addEventListener("pointerlockchange", this.onPointerLockChange.bind(this))
        document.addEventListener("fullscreenchange", this.onFullscreenChange.bind(this))

        window.addEventListener("gamepadconnected", this.onGamepadConnect.bind(this))
        window.addEventListener("gamepaddisconnected", this.onGamepadDisconnect.bind(this))
        // Connect all gamepads
        for (const gamepad of navigator.getGamepads()) {
            if (gamepad != null) {
                this.onGamepadAdd(gamepad)
            }
        }
    }
    private addListeners(element: GlobalEventHandlers) {
        element.addEventListener("keydown", this.onKeyDown.bind(this), { passive: false })
        element.addEventListener("keyup", this.onKeyUp.bind(this), { passive: false })
        element.addEventListener("paste", this.onPaste.bind(this))

        element.addEventListener("mousedown", this.onMouseButtonDown.bind(this), { passive: false })
        element.addEventListener("mouseup", this.onMouseButtonUp.bind(this), { passive: false })
        element.addEventListener("mousemove", this.onMouseMove.bind(this), { passive: false })
        element.addEventListener("wheel", this.onMouseWheel.bind(this), { passive: false })
        element.addEventListener("contextmenu", this.onContextMenu.bind(this), { passive: false })

        element.addEventListener("touchstart", this.onTouchStart.bind(this), { passive: false })
        element.addEventListener("touchend", this.onTouchEnd.bind(this), { passive: false })
        element.addEventListener("touchcancel", this.onTouchCancel.bind(this), { passive: false })
        element.addEventListener("touchmove", this.onTouchMove.bind(this), { passive: false })
    }

    private async startStream(hostId: number, appId: number, permissions: StreamPermissions, settings: Settings, browserSize: [number, number]) {
        setSidebarStyle({
            edge: settings.sidebarEdge,
        })

        // Add app info listener
        this.stream.addInfoListener(this.onInfo.bind(this))

        // Create connection info modal
        const connectionInfo = new ConnectionInfoModal()
        const connectionInfoListener = connectionInfo.onInfo.bind(connectionInfo)
        this.stream.addInfoListener(connectionInfoListener)
        void showModal(connectionInfo).then(async () => {
            this.stream.removeInfoListener(connectionInfoListener)
            if (this.autoEnterFullscreenOnStart && this.pendingAutoFullscreenPrompt && !this.fullscreenPromptShown && !this.isFullscreen()) {
                this.fullscreenPromptShown = true
                this.pendingAutoFullscreenPrompt = false
                this.armFullscreenOnNextInteraction()
            }
        })

        // Start animation frame loop
        this.onTouchUpdate()
        this.onGamepadUpdate()

        this.stream.getInput().addScreenKeyboardVisibleEvent(this.onScreenKeyboardSetVisible.bind(this))

        this.stream.mount(this.div)

        if (this.autoEnterFullscreenOnStart) {
            this.pendingAutoFullscreenPrompt = true
        }
    }

    private async onInfo(event: InfoEvent) {
        const data = event.detail

        if (data.type == "app") {
            const app = data.app

            document.title = `Stream: ${app.title}`
        } else if (data.type == "connectionComplete") {
            this.sidebar.onCapabilitiesChange(data.capabilities)
        }
    }

    private focusInput() {
        if (this.stream.getInput().getCurrentPredictedTouchAction() != "screenKeyboard" && !this.sidebar.getScreenKeyboard().isVisible()) {
            const inputElement = document.getElementById("input") as HTMLDivElement
            inputElement.focus()
        }
    }

    onUserInteraction() {
        this.focusInput()

        this.stream.getVideoRenderer()?.onUserInteraction()
        this.stream.getAudioPlayer()?.onUserInteraction()
    }
    private armFullscreenOnNextInteraction() {
        if (this.autoEnterFullscreenOnStart) {
            this.fullscreenOnNextInteractionArmed = true
        }
    }
    private consumeAutoFullscreenInteraction(): boolean {
        if (!this.fullscreenOnNextInteractionArmed || this.isFullscreen()) {
            return false
        }

        this.fullscreenOnNextInteractionArmed = false
        void this.requestFullscreen().then(() => {
            if (!this.isFullscreen()) {
                this.armFullscreenOnNextInteraction()
            }
        })
        return true
    }
    private beginAutoFullscreenTouchGesture(): boolean {
        if (!this.fullscreenOnNextInteractionArmed || this.isFullscreen()) {
            return false
        }

        this.pendingAutoFullscreenTouchGesture = true
        return true
    }
    private consumeAutoFullscreenTouchGesture(): boolean {
        if (!this.pendingAutoFullscreenTouchGesture) {
            return false
        }

        this.pendingAutoFullscreenTouchGesture = false
        return this.consumeAutoFullscreenInteraction()
    }
    private onScreenKeyboardSetVisible(event: ScreenKeyboardSetVisibleEvent) {
        console.info(event.detail)
        const screenKeyboard = this.sidebar.getScreenKeyboard()

        const newShown = event.detail.visible
        if (newShown != screenKeyboard.isVisible()) {
            if (newShown) {
                screenKeyboard.show()
            } else {
                screenKeyboard.hide()
            }
        }
    }

    // Input
    getInputConfig(): StreamInputConfig {
        return this.inputConfig
    }
    setInputConfig(config: StreamInputConfig) {
        Object.assign(this.inputConfig, config)

        this.stream.getInput().setConfig(this.inputConfig)
        this.renderLocalTouchCursor()
    }

    // Keyboard
    onKeyDown(event: KeyboardEvent) {
        this.onUserInteraction()

        console.debug(event)
        if (event.shiftKey && event.ctrlKey && event.code == "KeyV") {
            // We are likely pasting -> don't send keys
        } else if (event.code == "F11") {
            // Allow manual fullscreen
        } else {
            event.preventDefault()
            this.stream.getInput().onKeyDown(event)
        }

        event.stopPropagation()
    }

    private isTogglingFullscreenWithKeybind: "waitForCtrl" | "makingFullscreen" | "none" = "none"
    onKeyUp(event: KeyboardEvent) {
        this.onUserInteraction()

        event.preventDefault()
        this.stream.getInput().onKeyUp(event)
        event.stopPropagation()

        if (this.toggleFullscreenWithKeybind && this.isTogglingFullscreenWithKeybind == "none" && event.ctrlKey && event.shiftKey && event.code == "KeyI") {
            this.isTogglingFullscreenWithKeybind = "waitForCtrl"
        }
        if (this.isTogglingFullscreenWithKeybind == "waitForCtrl" && (event.code == "ControlRight" || event.code == "ControlLeft")) {
            this.isTogglingFullscreenWithKeybind = "makingFullscreen";

            (async () => {
                if (this.isFullscreen()) {
                    await this.exitPointerLock()
                    await this.exitFullscreen()
                } else {
                    await this.requestFullscreen()
                    await this.requestPointerLock()
                }

                this.isTogglingFullscreenWithKeybind = "none"
            })()
        }
    }

    onPaste(event: ClipboardEvent) {
        this.onUserInteraction()

        this.stream.getInput().onPaste(event)

        event.stopPropagation()
    }

    // Mouse
    onMouseButtonDown(event: MouseEvent) {
        if (this.consumeAutoFullscreenInteraction()) {
            this.pendingAutoFullscreenMouseGesture = true
            event.preventDefault()
            event.stopPropagation()
            return
        }

        this.onUserInteraction()

        event.preventDefault()
        this.stream.getInput().onMouseDown(event, this.getStreamRect());

        event.stopPropagation()
    }
    onMouseButtonUp(event: MouseEvent) {
        if (this.pendingAutoFullscreenMouseGesture) {
            this.pendingAutoFullscreenMouseGesture = false
            event.preventDefault()
            event.stopPropagation()
            return
        }

        this.onUserInteraction()

        event.preventDefault()
        this.stream.getInput().onMouseUp(event)

        event.stopPropagation()
    }
    onMouseMove(event: MouseEvent) {
        if (this.pendingAutoFullscreenMouseGesture) {
            event.preventDefault()
            event.stopPropagation()
            return
        }

        event.preventDefault()
        this.stream.getInput().onMouseMove(event, this.getStreamRect())

        event.stopPropagation()
    }
    onMouseWheel(event: WheelEvent) {
        event.preventDefault()
        this.stream.getInput().onMouseWheel(event)

        event.stopPropagation()
    }
    onContextMenu(event: MouseEvent) {
        event.preventDefault()

        event.stopPropagation()
    }

    // Touch
    onTouchStart(event: TouchEvent) {
        if (this.beginAutoFullscreenTouchGesture()) {
            event.preventDefault()
            event.stopPropagation()
            return
        }

        this.onUserInteraction()

        event.preventDefault()
        this.stream.getInput().onTouchStart(event, this.getStreamRect())

        event.stopPropagation()
    }
    onTouchEnd(event: TouchEvent) {
        if (this.consumeAutoFullscreenTouchGesture()) {
            event.preventDefault()
            event.stopPropagation()
            return
        }

        this.onUserInteraction()

        event.preventDefault()
        this.stream.getInput().onTouchEnd(event, this.getStreamRect())

        event.stopPropagation()
    }
    onTouchCancel(event: TouchEvent) {
        if (this.pendingAutoFullscreenTouchGesture) {
            this.pendingAutoFullscreenTouchGesture = false
            event.preventDefault()
            event.stopPropagation()
            return
        }

        this.pendingAutoFullscreenTouchGesture = false

        this.onUserInteraction()

        event?.preventDefault()
        this.stream.getInput().onTouchCancel(event, this.getStreamRect())

        event.stopPropagation()
    }
    onTouchUpdate() {
        this.stream.getInput().onTouchUpdate(this.getStreamRect())
        this.updateKeyboardViewportVideoOffset()
        this.renderLocalTouchCursor()

        window.requestAnimationFrame(this.onTouchUpdate.bind(this))
    }
    onTouchMove(event: TouchEvent) {
        if (this.pendingAutoFullscreenTouchGesture) {
            event.preventDefault()
            event.stopPropagation()
            return
        }

        event.preventDefault()
        this.stream.getInput().onTouchMove(event, this.getStreamRect())

        event.stopPropagation()
    }

    // Gamepad
    onGamepadConnect(event: GamepadEvent) {
        this.onGamepadAdd(event.gamepad)
    }
    onGamepadAdd(gamepad: Gamepad) {
        this.stream.getInput().onGamepadConnect(gamepad)
    }
    onGamepadDisconnect(event: GamepadEvent) {
        this.stream.getInput().onGamepadDisconnect(event)
    }
    onGamepadUpdate() {
        this.stream.getInput().onGamepadUpdate()

        window.requestAnimationFrame(this.onGamepadUpdate.bind(this))
    }

    // Fullscreen
    private async promptAutoFullscreen() {
        await showModal(new AutoFullscreenModal(this.requestFullscreen.bind(this)))
    }
    async requestFullscreen(showEscapeWarning: boolean = true) {
        const body = document.body
        if (body) {
            if (!("requestFullscreen" in body && typeof body.requestFullscreen == "function")) {
                await showMessage(I.stream.fullscreenUnsupported)

                return
            }

            this.focusInput()

            if (!this.isFullscreen()) {
                try {
                    await body.requestFullscreen({
                        navigationUI: "hide"
                    })
                } catch (e) {
                    console.warn("failed to request fullscreen", e)
                }
            }

            try {
                await requestKeyboardLock();
                if (showEscapeWarning && !this.hasShownFullscreenEscapeWarning) {
                    showNotification(I.stream.fullscreenEscapeHint, "info")
                    this.hasShownFullscreenEscapeWarning = true
                }
            } catch (e) {
                console.warn("Keyboard lock failed, skipping notification.", e);
            }

            if (this.getStream()?.getInput().getConfig().mouseMode == "relative") {
                await this.requestPointerLock()
            }

            try {
                if (screen && "orientation" in screen) {
                    const orientation = screen.orientation

                    if ("lock" in orientation && typeof orientation.lock == "function") {
                        await orientation.lock("landscape")
                    }
                }
            } catch (e) {
                console.warn("failed to set orientation to landscape", e)
            }
        } else {
            console.warn("root element not found")
        }
    }
    async exitFullscreen() {
        if ("keyboard" in navigator && navigator.keyboard && "unlock" in navigator.keyboard) {
            await navigator.keyboard.unlock()
        }

        if ("exitFullscreen" in document && typeof document.exitFullscreen == "function") {
            await document.exitFullscreen()
        }
    }
    isFullscreen(): boolean {
        return "fullscreenElement" in document && !!document.fullscreenElement
    }
    private async onFullscreenChange() {
        if (this.isFullscreen()) {
            this.fullscreenOnNextInteractionArmed = false
            this.pendingAutoFullscreenTouchGesture = false
            this.pendingAutoFullscreenMouseGesture = false
            this.manualFullscreenExitRequested = false
        } else {
            const manualExit = this.manualFullscreenExitRequested
            this.manualFullscreenExitRequested = false

            if (this.autoEnterFullscreenOnStart && !manualExit) {
                this.armFullscreenOnNextInteraction()
            }
        }

        this.checkFullyImmersed()
    }
    markManualFullscreenExitRequested() {
        this.manualFullscreenExitRequested = true
    }

    // Pointer Lock
    async requestPointerLock(errorIfNotFound: boolean = false) {
        this.previousMouseMode = this.inputConfig.mouseMode

        const inputElement = document.getElementById("input") as HTMLDivElement

        if (inputElement && "requestPointerLock" in inputElement && typeof inputElement.requestPointerLock == "function") {
            this.focusInput()

            this.inputConfig.mouseMode = "relative"
            this.setInputConfig(this.inputConfig)

            setSidebarExtended(false)

            const onLockError = () => {
                document.removeEventListener("pointerlockerror", onLockError)

                // Fallback: try to request pointer lock without options
                inputElement.requestPointerLock()
            }

            document.addEventListener("pointerlockerror", onLockError, { once: true })

            try {
                let promise = inputElement.requestPointerLock({
                    unadjustedMovement: true
                })

                if (promise) {
                    await promise
                } else {
                    inputElement.requestPointerLock()
                }
            } catch (error) {
                // Some platforms do not support unadjusted movement. If you
                // would like PointerLock anyway, request again.
                if (error instanceof Error && error.name == "NotSupportedError") {
                    inputElement.requestPointerLock()
                } else {
                    throw error
                }
            } finally {
                document.removeEventListener("pointerlockerror", onLockError)
            }

        } else if (errorIfNotFound) {
            await showMessage(I.stream.pointerLockUnsupported)
        }
    }
    async exitPointerLock() {
        if ("exitPointerLock" in document && typeof document.exitPointerLock == "function") {
            document.exitPointerLock()
        }
    }
    private onPointerLockChange() {
        this.checkFullyImmersed()

        if (!document.pointerLockElement) {
            this.inputConfig.mouseMode = this.previousMouseMode
            this.setInputConfig(this.inputConfig)
        }
    }

    // -- Fully immersed Fullscreen -> Fullscreen API + Pointer Lock
    private checkFullyImmersed() {
        if ("pointerLockElement" in document && document.pointerLockElement &&
            "fullscreenElement" in document && document.fullscreenElement) {
            // We're fully immersed -> remove sidebar
            setSidebar(null)
        } else {
            setSidebar(this.sidebar)
        }
    }
    private renderLocalTouchCursor() {
        const localCursorState = this.stream.getInput().getLocalCursorState()
        if (!localCursorState?.visible) {
            this.localTouchCursorDiv.hidden = true
            return
        }

        const rect = this.getStreamRect()
        if (rect.width <= 0 || rect.height <= 0) {
            this.localTouchCursorDiv.hidden = true
            return
        }

        this.localTouchCursorDiv.hidden = false
        this.localTouchCursorDiv.style.left = `${rect.left + localCursorState.x * rect.width}px`
        this.localTouchCursorDiv.style.top = `${rect.top + localCursorState.y * rect.height}px`
    }

    onScreenKeyboardModeWillChange(event: KeyboardModeWillChangeEvent) {
        if (event.detail.enabled) {
            this.captureKeyboardViewportBaseline()
        }
    }

    private captureKeyboardViewportBaseline() {
        this.keyboardViewportBaselineHeight = window.visualViewport?.height ?? null
        this.streamVideoTopOffsetPx = 0
        this.applyStreamVideoTopOffset()
        this.updateKeyboardFloatingButtonPosition()
    }
    resetKeyboardViewportVideoOffset() {
        this.keyboardViewportBaselineHeight = null
        this.streamVideoTopOffsetPx = 0
        this.applyStreamVideoTopOffset()
        this.resetKeyboardFloatingButtonPosition()
    }
    private updateKeyboardViewportVideoOffset() {
        this.updateKeyboardFloatingButtonPosition()

        const screenKeyboard = this.sidebar.getScreenKeyboard()
        const visualViewport = window.visualViewport
        const baselineHeight = this.keyboardViewportBaselineHeight
        const localCursorState = this.stream.getInput().getLocalCursorState()

        if (!screenKeyboard.isVisible() || !visualViewport || baselineHeight == null) {
            if (this.streamVideoTopOffsetPx != 0 && !screenKeyboard.isVisible()) {
                this.resetKeyboardViewportVideoOffset()
            }
            return
        }

        const viewportShrink = baselineHeight - visualViewport.height
        if (viewportShrink < 80) {
            if (this.streamVideoTopOffsetPx != 0) {
                this.streamVideoTopOffsetPx = 0
                this.applyStreamVideoTopOffset()
            }
            return
        }

        const streamRect = this.getStreamRect()
        if (streamRect.width <= 0 || streamRect.height <= 0) {
            return
        }

        const visibleTop = visualViewport.offsetTop
        const visibleBottom = visualViewport.offsetTop + visualViewport.height

        let newTopOffsetPx = this.streamVideoTopOffsetPx
        if (localCursorState.visible) {
            let delta = 0

            const safeMargin = Math.min(100, visualViewport.height * 0.25)
            const cursorY = streamRect.top + localCursorState.y * streamRect.height

            if (cursorY < visibleTop + safeMargin) {
                delta = visibleTop + safeMargin - cursorY
            } else if (cursorY > visibleBottom - safeMargin) {
                delta = visibleBottom - safeMargin - cursorY
            }

            newTopOffsetPx += delta
        } else {
            const screenTopToVideoTop = visualViewport.height - streamRect.height
            if (screenTopToVideoTop > 0) {
                newTopOffsetPx = visibleTop - screenTopToVideoTop
            }
        }

        if (Math.abs(newTopOffsetPx - this.streamVideoTopOffsetPx) >= 1) {
            this.streamVideoTopOffsetPx = newTopOffsetPx
            this.applyStreamVideoTopOffset()
        }
    }
    private applyStreamVideoTopOffset() {
        if (Math.abs(this.streamVideoTopOffsetPx) < 0.5) {
            document.documentElement.style.removeProperty("--stream-video-top")
            return
        }

        document.documentElement.style.setProperty("--stream-video-top", `calc(50% + ${this.streamVideoTopOffsetPx}px)`)
    }
    private updateKeyboardFloatingButtonPosition() {
        const screenKeyboard = this.sidebar.getScreenKeyboard()
        const visualViewport = window.visualViewport
        if (!screenKeyboard.isVisible() || !visualViewport) {
            this.resetKeyboardFloatingButtonPosition()
            return
        }

        const bottomInset = Math.min(16, visualViewport.height * 0.08)
        const buttonTop = visualViewport.offsetTop + visualViewport.height - bottomInset
        document.documentElement.style.setProperty("--stream-keyboard-button-top", `${buttonTop}px`)
    }
    private resetKeyboardFloatingButtonPosition() {
        document.documentElement.style.removeProperty("--stream-keyboard-button-top")
    }

    mount(parent: HTMLElement): void {
        parent.appendChild(this.div)
    }
    unmount(parent: HTMLElement): void {
        parent.removeChild(this.div)
    }

    getStreamRect(): DOMRect {
        // The bounding rect of the videoElement or canvasElement can be bigger than the actual video
        // -> We need to correct for this when sending positions, else positions are wrong
        return this.stream.getVideoRenderer()?.getStreamRect() ?? new DOMRect()
    }
    getStream(): Stream | null {
        return this.stream
    }
}

class ConnectionInfoModal implements Modal<void> {

    private eventTarget = new EventTarget()

    private root = document.createElement("div")

    private textTy: LogMessageType | null = null
    private text = document.createElement("p")

    private options = document.createElement("div")
    private debugDetailButton = document.createElement("button")
    private closeButton = document.createElement("button")

    private debugDetail = "" // We store this seperate because line breaks don't work when the element is not mounted on the dom
    private debugDetailDisplay = document.createElement("div")

    constructor() {
        this.root.classList.add("modal-video-connect")

        this.text.innerText = I.stream.connecting
        this.root.appendChild(this.text)

        this.root.appendChild(this.options)
        this.options.classList.add("modal-video-connect-options")

        this.debugDetailButton.innerText = I.stream.showLogs
        this.debugDetailButton.addEventListener("click", this.onDebugDetailClick.bind(this))
        this.options.appendChild(this.debugDetailButton)

        this.closeButton.innerText = I.stream.close
        this.closeButton.addEventListener("click", this.onClose.bind(this))
        this.options.appendChild(this.closeButton)

        this.debugDetailDisplay.classList.add("textlike")
        this.debugDetailDisplay.classList.add("modal-video-connect-debug")
    }

    private onDebugDetailClick() {
        let debugDetailCurrentlyShown = this.root.contains(this.debugDetailDisplay)

        if (debugDetailCurrentlyShown) {
            this.debugDetailButton.innerText = I.stream.showLogs
            this.root.removeChild(this.debugDetailDisplay)
        } else {
            this.debugDetailButton.innerText = I.stream.hideLogs
            this.root.appendChild(this.debugDetailDisplay)
            this.debugDetailDisplay.innerText = this.debugDetail
        }
    }

    private debugLog(line: string) {
        this.debugDetail += `${line}\n`
        this.debugDetailDisplay.innerText = this.debugDetail
        console.info(`[Stream]: ${line}`)
    }

    onInfo(event: InfoEvent) {
        const data = event.detail

        if (data.type == "connectionComplete") {
            const text = I.stream.connectionComplete
            this.text.innerText = text
            this.debugLog(text)
        } else if (data.type == "videoReady") {

            this.eventTarget.dispatchEvent(new Event("ml-connected"))
        } else if (data.type == "addDebugLine") {
            const message = data.line.trim()
            if (message) {
                this.debugLog(message)

                if (!this.textTy) {
                    this.text.innerText = message
                    this.textTy = data.additional?.type ?? null
                } else if (data.additional?.type == "fatalDescription" || data.additional?.type == "ifErrorDescription") {
                    if (this.text.innerText) {
                        this.text.innerText += "\n" + message
                    } else {
                        this.text.innerText = message
                    }
                    this.textTy = data.additional.type
                }
            }

            if (data.additional?.type == "fatal" || data.additional?.type == "fatalDescription") {
                showModal(this)
            } else if (data.additional?.type == "informError") {
                showNotification(data.line)
            }
        } else if (data.type == "serverMessage") {
            const text = I.stream.serverMessage(data.message)
            this.text.innerText = text
            this.debugLog(text)
        }
    }

    onClose() {
        showModal(null)
    }

    onFinish(abort: AbortSignal): Promise<void> {
        return new Promise((resolve, reject) => {
            this.eventTarget.addEventListener("ml-connected", () => resolve(), { once: true, signal: abort })
        })
    }

    mount(parent: HTMLElement): void {
        parent.appendChild(this.root)
    }
    unmount(parent: HTMLElement): void {
        parent.removeChild(this.root)
    }
}

class AutoFullscreenModal implements Component, Modal<void> {
    private message = document.createElement("p")
    private root = document.createElement("div")
    private okButton = document.createElement("button")
    private cancelButton = document.createElement("button")
    private onConfirm: () => Promise<void>

    constructor(onConfirm: () => Promise<void>) {
        this.onConfirm = onConfirm
        this.message.innerText = I.stream.autoFullscreenPrompt
        this.okButton.innerText = I.modal.ok
        this.cancelButton.innerText = I.modal.cancel
    }

    mount(parent: HTMLElement): void {
        this.root.appendChild(this.message)
        this.root.appendChild(this.okButton)
        this.root.appendChild(this.cancelButton)
        parent.appendChild(this.root)
    }
    unmount(parent: HTMLElement): void {
        parent.removeChild(this.root)
    }

    onFinish(abort: AbortSignal): Promise<void> {
        return new Promise((resolve) => {
            this.okButton.addEventListener("click", async () => {
                await this.onConfirm()
                resolve()
            }, { once: true, signal: abort })

            this.cancelButton.addEventListener("click", () => {
                resolve()
            }, { once: true, signal: abort })
        })
    }
}

class ViewerSidebar implements Component, Sidebar {
    private app: ViewerApp

    private gamepadButton = document.createElement("button");

    private div = document.createElement("div")

    private buttonDiv = document.createElement("div")

    private sendKeycodeButton = document.createElement("button")

    private keyboardButton = document.createElement("button")
    private floatingKeyboardButton = document.createElement("button")
    private screenKeyboard = new ScreenKeyboard()

    private lockMouseButton = document.createElement("button")
    private fullscreenButton = document.createElement("button")

    private statsButton = document.createElement("button")
    private exitStreamButton = document.createElement("button")

    private mouseMode: SelectComponent
    private touchMode: SelectComponent

    // Class-level state for sticky modifiers to ensure reliable resets
    private stickyModifiers: Record<number, boolean> = {
        0x11: false, // Ctrl
        0x12: false, // Alt
        0x10: false, // Shift
        0x5B: false  // Win
    };
    private modifierButtons: Record<number, HTMLElement> = {};

    private resetStickyModifiers() {
        const input = this.app.getStream()?.getInput();
        if (!input) return;

        [0x11, 0x12, 0x10, 0x5B].forEach(code => {
            if (this.stickyModifiers[code]) {
                input.sendKey(false, code, 0); // Release via class-level input access
                this.stickyModifiers[code] = false;
                
                // Visual reset
                if (this.modifierButtons[code]) {
                    this.modifierButtons[code].style.background = "rgba(55, 55, 55, 0.4)";
                }
            }
        });
    }

    constructor(app: ViewerApp) {
        this.app = app

        // Configure divs
        this.div.classList.add("sidebar-stream")

        this.buttonDiv.classList.add("sidebar-stream-buttons")
        this.div.appendChild(this.buttonDiv)

        // Add these two buttons to your buttonDiv
        const switchLeftButton = document.createElement("button");
        switchLeftButton.innerText = "Prev Monitor";
        switchLeftButton.addEventListener("click", () => this.sendMonitorShortcut(0x70)); // VK_F1
        this.buttonDiv.appendChild(switchLeftButton);

        const switchRightButton = document.createElement("button");
        switchRightButton.innerText = "Next Monitor";
        switchRightButton.addEventListener("click", () => this.sendMonitorShortcut(0x71)); // VK_F2
        this.buttonDiv.appendChild(switchRightButton);


        // --- VIRTUAL GAMEPAD BUTTON (First button) ---
        this.gamepadButton = document.createElement("button");
        this.gamepadButton.innerText = "Gamepad";
        this.gamepadButton.id = "vpad-btn";
        this.gamepadButton.title = "Virtual Gamepad (Long press to unplug)";

        let gamepadPressStartTime = 0;
        const GAMEPAD_LONG_PRESS_DURATION = 550;

        const handleGamepadPointerDown = (e: PointerEvent) => {
            if (e.button !== undefined && e.button !== 0) return;
            gamepadPressStartTime = Date.now();
            this.gamepadButton.style.transition = "background-color 0.15s";
            this.gamepadButton.style.backgroundColor = "rgba(255, 255, 255, 0.25)";
        };

        const handleGamepadPointerUp = (e: PointerEvent) => {
            e.preventDefault();
            e.stopImmediatePropagation();

            const duration = Date.now() - gamepadPressStartTime;
            this.gamepadButton.style.backgroundColor = "";

            if (duration >= GAMEPAD_LONG_PRESS_DURATION) {
                window.unplugVirtualGamepad?.(this.gamepadButton);
            } else {
                window.toggleVirtualGamepad?.(this.gamepadButton);
            }
        };

        this.gamepadButton.addEventListener("pointerdown", handleGamepadPointerDown, { passive: false });
        this.gamepadButton.addEventListener("pointerup", handleGamepadPointerUp, { passive: false });
        this.gamepadButton.addEventListener("pointerleave", () => {
            this.gamepadButton.style.backgroundColor = "";
        });
        this.gamepadButton.addEventListener("pointercancel", () => {
            this.gamepadButton.style.backgroundColor = "";
        });

        this.gamepadButton.addEventListener("contextmenu", e => e.preventDefault());
        this.gamepadButton.addEventListener("click", e => e.preventDefault());

        // Insert as the very first button
        this.buttonDiv.appendChild(this.gamepadButton);

        // Send keycode
        this.sendKeycodeButton.innerText = I.stream.sendKeycode
        this.sendKeycodeButton.addEventListener("click", async () => {
            const key = await showModal(new SendKeycodeModal())

            if (key == null) {
                return
            }

            this.app.getStream()?.getInput().sendKey(true, key, 0)
            this.app.getStream()?.getInput().sendKey(false, key, 0)
        })
        this.buttonDiv.appendChild(this.sendKeycodeButton)

        const clipboardButton = document.createElement("button")
        clipboardButton.innerText = "Clipboard"
        clipboardButton.addEventListener("click", async () => {
            setSidebarExtended(false);

            const textToSend = await showModal(new ClipboardModal());

            if (textToSend) {
                const stream = this.app.getStream()?.getInput();
                if (!stream) return;

                // Use a conservative chunk size (e.g., 128) to avoid UTF-8 buffer overflow
                const CHUNK_SIZE = 128;

                for (let i = 0; i < textToSend.length; i += CHUNK_SIZE) {
                    const chunk = textToSend.slice(i, i + CHUNK_SIZE);
                    stream.sendText(chunk);
                    await new Promise(resolve => setTimeout(resolve, 300));
                }

                console.log("📤 Transmitted via native sendText.");
            }
        });
        this.buttonDiv.appendChild(clipboardButton)

        // Pointer Lock
        this.lockMouseButton.innerText = I.stream.lockMouse
        this.lockMouseButton.addEventListener("click", async () => {
            await this.app.requestPointerLock(true)
        })
        this.buttonDiv.appendChild(this.lockMouseButton)

        // --- VISUAL ON-SCREEN KEYBOARD ENGINE ---
        const visualKbdOverlay = document.createElement("div");
        visualKbdOverlay.id = "visual-kbd-overlay";
        visualKbdOverlay.style.pointerEvents = "none";
        visualKbdOverlay.style.display = "none"; // Hidden by default

        // Handle raw key transmissions
        const sendRawKey = (isDown: boolean, vkCode: number) => {
            const input = this.app.getStream()?.getInput();
            if (input) {
                input.sendKey(isDown, vkCode, 0);
            }
        };

        // Key Layout Mapping Matrix [Label, Virtual Key Code, Flex-Grow Weight (optional)]
        const kbdLayout = [
            [ ["ESC", 0x1B], ["F1", 0x70], ["F2", 0x71], ["F3", 0x72], ["F4", 0x73], ["F5", 0x74], ["F6", 0x75], ["F7", 0x76], ["F8", 0x77], ["F9", 0x78], ["F10", 0x79], ["F11", 0x7A], ["F12", 0x7B], ["Ins", 0x2D], ["Del", 0x2E] ],
            [ ["~ `", 0xC0], ["1", 0x31], ["2", 0x32], ["3", 0x33], ["4", 0x34], ["5", 0x35], ["6", 0x36], ["7", 0x37], ["8", 0x38], ["9", 0x39], ["0", 0x30], ["-_", 0xBD], ["+=", 0xBB], ["Back", 0x08, 1.5] ],
            [ ["Tab", 0x09, 1.5], ["Q", 0x51], ["W", 0x57], ["E", 0x45], ["R", 0x52], ["T", 0x54], ["Y", 0x59], ["U", 0x55], ["I", 0x49], ["O", 0x4F], ["P", 0x50], ["{[", 0xDB], ["}]", 0xDD], ["| \\", 0xDC] ],
            [ ["Caps", 0x14, 1.75], ["A", 0x41], ["S", 0x53], ["D", 0x44], ["F", 0x46], ["G", 0x47], ["H", 0x48], ["J", 0x4A], ["K", 0x4B], ["L", 0x4C], [";:", 0xBA], ["'\"", 0xDE], ["Enter", 0x0D, 2.25] ],
            [ ["Shift", 0x10, 2.25], ["Z", 0x5A], ["X", 0x58], ["C", 0x43], ["V", 0x56], ["B", 0x42], ["N", 0x4E], ["M", 0x4D], ["<,", 0xBC], [">.", 0xBE], ["?/", 0xBF], ["↑", 0x26], ["Hide\nKBD", -1, 1.25] ],
            [ ["Ctrl", 0x11, 1.5], ["Win", 0x5B, 1.25], ["Alt", 0x12, 1.25], ["___", 0x20, 5], ["Home", 0x24], ["End", 0x23], ["PgUp", 0x21], ["PgDn", 0x22], ["←", 0x25], ["↓", 0x28], ["→", 0x27] ]
        ];

        // Generate the visual markup dynamically
        // Generate the visual markup dynamically
        kbdLayout.forEach(row => {
            const rowEl = document.createElement("div");
            rowEl.className = "vkbd-row";
            row.forEach(([label, code, weight]) => {
                const btn = document.createElement("div");
                btn.className = "vkbd-key";
                btn.innerText = label.toString();
                if (weight) {
                    btn.style.flexGrow = weight.toString();
                }

                const isModifier = [0x11, 0x12, 0x10, 0x5B].indexOf(code as number) !== -1;
                
                if (isModifier) {
                    this.modifierButtons[code as number] = btn;
                }

                let pressTimer: number | undefined;
                let isLongPressed = false;
                let wasAlreadySticky = false;

                const handleKeyStart = (e: PointerEvent) => {
                    e.preventDefault();
                    e.stopPropagation();
                    btn.classList.add("active");
                    btn.setPointerCapture(e.pointerId);
                    
                    isLongPressed = false;

                    if (isModifier) {
                        wasAlreadySticky = this.stickyModifiers[code as number];
                        
                        if (wasAlreadySticky) {
                            // If it's already lit up, ANY tap instantly disables it.
                            this.stickyModifiers[code as number] = false;
                            btn.style.background = "rgba(55, 55, 55, 0.4)";
                            sendRawKey(false, code as number);
                        } else {
                            // It is not lit up. Start timer to see if they hold it.
                            pressTimer = window.setTimeout(() => {
                                isLongPressed = true;
                                this.stickyModifiers[code as number] = true;
                                btn.style.background = "rgba(255, 255, 255, 0.6)";
                                sendRawKey(true, code as number);
                            }, 500);
                        }
                    } else if (code === -1) {
                        this.resetStickyModifiers();
                        visualKbdOverlay.style.display = "none";
                        visualKbdOverlay.style.pointerEvents = "none";
                    } else {
                        // Standard keys fire immediately
                        sendRawKey(true, code as number);
                    }
                };

                const handleKeyEnd = (e: PointerEvent) => {
                    e.preventDefault();
                    e.stopPropagation();
                    clearTimeout(pressTimer);
                    
                    try {
                        if (btn.hasPointerCapture(e.pointerId)) {
                            btn.releasePointerCapture(e.pointerId);
                        }
                    } catch(err) {}

                    if (isModifier) {
                        if (!wasAlreadySticky && !isLongPressed) {
                            // It was a quick tap on an unlit modifier. Pulse it.
                            sendRawKey(true, code as number);
                            setTimeout(() => sendRawKey(false, code as number), 100);
                        }
                        btn.classList.remove("active");
                    } else {
                        btn.classList.remove("active");
                        if (code !== -1) {
                            sendRawKey(false, code as number);
                        }
                    }
                };

                btn.addEventListener("pointerdown", handleKeyStart);
                btn.addEventListener("pointerup", handleKeyEnd);
                btn.addEventListener("pointercancel", handleKeyEnd);

                rowEl.appendChild(btn);
            });
            visualKbdOverlay.appendChild(rowEl);
        });

        // --- THE LEAK BLOCKER ---
        const blockEvent = (e: Event) => e.stopPropagation();
        const eventsToBlock = [
            "pointerdown", "pointerup", "pointermove", "pointercancel",
            "touchstart", "touchend", "touchmove", "touchcancel",
            "mousedown", "mouseup", "mousemove", "contextmenu"
        ];
        eventsToBlock.forEach(ev => {
            visualKbdOverlay.addEventListener(ev, blockEvent, { passive: false });
        });

        (document.getElementById("root") || document.body).appendChild(visualKbdOverlay);

        // --- Pop up keyboard with Long Press support ---
        this.keyboardButton.innerText = I.stream.keyboard;

        let kbdPressTimer: number | undefined;
        let kbdIsLongPress = false;

        this.keyboardButton.addEventListener("pointerdown", () => {
            kbdIsLongPress = false;
            clearTimeout(kbdPressTimer);

            kbdPressTimer = window.setTimeout(() => {
                kbdIsLongPress = true;
                setSidebarExtended(false);
                if (visualKbdOverlay) {
                    visualKbdOverlay.style.display = "flex";
                    visualKbdOverlay.style.pointerEvents = "auto";
                }
            }, 500);
        });

        const cancelKbdTimer = () => {
            clearTimeout(kbdPressTimer);
        };
        this.keyboardButton.addEventListener("pointerup", cancelKbdTimer);
        this.keyboardButton.addEventListener("pointercancel", cancelKbdTimer);
        this.keyboardButton.addEventListener("pointerleave", cancelKbdTimer);

        this.keyboardButton.addEventListener("contextmenu", (e) => {
            e.preventDefault();
        });

        this.keyboardButton.addEventListener("click", async (e) => {
            if (kbdIsLongPress) {
                e.preventDefault();
                e.stopPropagation();
                return;
            }

            setSidebarExtended(false);
            this.screenKeyboard.show();
        });

        this.buttonDiv.appendChild(this.keyboardButton);

        // --- Floating Keyboard Button ---
        this.floatingKeyboardButton.innerText = "⌨×";
        this.floatingKeyboardButton.title = I.stream.hideKeyboard;
        this.floatingKeyboardButton.ariaLabel = I.stream.hideKeyboard;
        this.floatingKeyboardButton.classList.add("stream-keyboard-floating-button");
        this.floatingKeyboardButton.addEventListener("click", event => {
            event.preventDefault();
            event.stopPropagation();
            this.screenKeyboard.hide();

            if (visualKbdOverlay) {
                this.resetStickyModifiers();
                visualKbdOverlay.style.display = "none";
                visualKbdOverlay.style.pointerEvents = "none";
            }
        });
        stopPropagationOn(this.floatingKeyboardButton);

        // --- Standard Keyboard Listeners ---
        this.screenKeyboard.addKeyDownListener(this.onKeyDown.bind(this));
        this.screenKeyboard.addKeyUpListener(this.onKeyUp.bind(this));
        this.screenKeyboard.addTextListener(this.onText.bind(this));
        this.screenKeyboard.addKeyboardModeWillChangeListener(this.app.onScreenKeyboardModeWillChange.bind(this.app));
        this.screenKeyboard.addKeyboardModeListener(this.onKeyboardModeChange.bind(this));
        this.div.appendChild(this.screenKeyboard.getHiddenElement());


        // Fullscreen
        this.fullscreenButton.innerText = I.stream.fullscreen
        this.fullscreenButton.addEventListener("click", async () => {
            if (this.app.isFullscreen()) {
                this.app.markManualFullscreenExitRequested()
                await this.app.exitFullscreen()
            } else {
                await this.app.requestFullscreen()
            }
        })
        this.buttonDiv.appendChild(this.fullscreenButton)

        // Stats
        this.statsButton.innerText = I.stream.stats
        this.statsButton.addEventListener("click", () => {
            const stats = this.app.getStream()?.getStats()
            if (stats) {
                stats.toggle()
            }
        })
        this.buttonDiv.appendChild(this.statsButton)

        // Close stream
        this.exitStreamButton.innerText = I.stream.exit
        this.exitStreamButton.addEventListener("click", async () => {
            const stream = this.app.getStream()
            if (stream) {
                const success = await stream.stop()
                if (!success) {
                    console.debug("Failed to close stream correctly")
                }
            }

            if (window.matchMedia('(display-mode: standalone)').matches) {
                history.back()
            } else {
                window.close()
            }
        })
        this.buttonDiv.appendChild(this.exitStreamButton)

        // Select Mouse Mode
        this.mouseMode = new SelectComponent("mouseMode", [
            { value: "relative", name: I.stream.relative },
            { value: "follow", name: I.stream.follow },
            { value: "localCursor", name: I.stream.localCursor },
            { value: "pointAndDrag", name: I.stream.pointAndDrag }
        ], {
            displayName: I.stream.mouseMode,
            preSelectedOption: this.app.getInputConfig().mouseMode
        })
        this.mouseMode.addChangeListener(this.onMouseModeChange.bind(this))
        this.mouseMode.mount(this.div)

        // Select Touch Mode
        this.touchMode = new SelectComponent("touchMode", [
            { value: "touch", name: I.stream.touch },
            { value: "mouseRelative", name: I.stream.relative },
            { value: "localCursor", name: I.stream.localCursor },
            { value: "pointAndDrag", name: I.stream.pointAndDrag }
        ], {
            displayName: I.stream.touchMode,
            preSelectedOption: this.app.getInputConfig().touchMode
        })
        this.touchMode.addChangeListener(this.onTouchModeChange.bind(this))
        this.touchMode.mount(this.div)
    }

    onCapabilitiesChange(capabilities: StreamCapabilities) {
        this.touchMode.setOptionEnabled("touch", capabilities.touch)
    }

    getScreenKeyboard(): ScreenKeyboard {
        return this.screenKeyboard
    }

    // -- Keyboard
    private onText(event: TextEvent) {
        this.app.getStream()?.getInput().sendText(event.detail.text)
    }
    private onKeyDown(event: KeyboardEvent) {
        this.app.getStream()?.getInput().onKeyDown(event)
    }
    private onKeyUp(event: KeyboardEvent) {
        this.app.getStream()?.getInput().onKeyUp(event)
    }
    private onKeyboardModeChange(event: KeyboardModeEvent) {
        if (event.detail.enabled) {
            this.floatingKeyboardButton.classList.add("visible")
        } else {
            this.floatingKeyboardButton.classList.remove("visible")
            this.app.resetKeyboardViewportVideoOffset()
        }
    }

    // -- Mouse Mode
    private onMouseModeChange() {
        const config = this.app.getInputConfig()
        config.mouseMode = this.mouseMode.getValue() as any
        this.app.setInputConfig(config)
    }

    // -- Touch Mode
    private onTouchModeChange() {
        const config = this.app.getInputConfig()
        config.touchMode = this.touchMode.getValue() as any
        this.app.setInputConfig(config)
    }

    // --- Move Monitors ---
    private sendMonitorShortcut(vkCode: number) {
        const input = this.app.getStream()?.getInput();
        if (!input) return;

        const modifiers = [0x11, 0x12, 0x10];

        modifiers.forEach(key => input.sendKey(true, key, 0));
        input.sendKey(true, vkCode, 0);
        input.sendKey(false, vkCode, 0);
        modifiers.forEach(key => input.sendKey(false, key, 0));
    }

    extended(): void {

    }
    unextend(): void {

    }

    mount(parent: HTMLElement): void {
        parent.appendChild(this.div)
        const appRoot = document.getElementById("root")
        ;(appRoot ?? document.body).appendChild(this.floatingKeyboardButton)
    }
    unmount(parent: HTMLElement): void {
        parent.removeChild(this.div)
        if (this.floatingKeyboardButton.parentElement) {
            this.floatingKeyboardButton.parentElement.removeChild(this.floatingKeyboardButton)
        }
    }
}

class SendKeycodeModal extends FormModal<number> {

    private dropdownSearch: SelectComponent

    constructor() {
        super()

        const keyList = []
        for (const keyNameRaw in StreamKeys) {
            const keyName = keyNameRaw as keyof typeof StreamKeys
            const keyValue = StreamKeys[keyName]

            const PREFIX = "VK_"

            let name: string = keyName
            if (name.startsWith(PREFIX)) {
                name = name.slice(PREFIX.length)
            }

            keyList.push({
                value: keyValue.toString(),
                name
            })
        }

        this.dropdownSearch = new SelectComponent("winKeycode", keyList, {
            hasSearch: true,
            displayName: I.stream.selectKeycode
        })
    }

    mountForm(form: HTMLFormElement): void {
        this.dropdownSearch.mount(form)
    }


    reset(): void {
        this.dropdownSearch.reset()
    }

    submit(): number | null {
        const keyString = this.dropdownSearch.getValue()
        if (keyString == null) {
            return null
        }

        return parseInt(keyString)
    }
}

class ClipboardModal implements Modal<string | null> {
    private root = document.createElement("div")
    private title = document.createElement("h3")
    private description = document.createElement("p")
    private textarea = document.createElement("textarea")
    private options = document.createElement("div")
    private sendButton = document.createElement("button")
    private cancelButton = document.createElement("button")

    constructor(incomingText: string = "") {
        this.root.classList.add("modal-content")

        // 1. Optimized width for both Portrait and Landscape
        this.root.style.width = "80vw"      // Slightly narrower to look better
        this.root.style.maxWidth = "400px"  // Stays compact on desktop
        this.root.style.boxSizing = "border-box"
        this.root.style.padding = "20px"
        this.root.style.margin = "0 auto"   // Centers it perfectly

        this.title.innerText = "Clipboard Sync"
        this.title.style.marginTop = "0"
        this.title.style.marginBottom = "10px"

        this.description.innerText = incomingText
            ? "Text received from Host PC:"
            : "Paste text below to send it to the Host PC:"
        this.description.style.fontSize = "14px"
        this.description.style.opacity = "0.8"
        this.description.style.marginBottom = "10px"

        // 2. Textarea: Tight and flush with the padding
        this.textarea.style.width = "100%"
        this.textarea.style.margin = "0"
        this.textarea.style.boxSizing = "border-box" // Crucial for no-clip
        this.textarea.style.display = "block"
        this.textarea.style.height = "120px"
        this.textarea.style.marginBottom = "15px"
        this.textarea.style.resize = "none"
        this.textarea.style.background = "rgba(0,0,0,0.2)"
        this.textarea.style.color = "white"
        this.textarea.style.border = "1px solid rgba(255,255,255,0.2)"
        this.textarea.style.padding = "10px"

        this.textarea.value = incomingText
        this.textarea.placeholder = "Paste here..."

        stopPropagationOn(this.textarea)

        // 3. Button Layout
        this.options.style.display = "flex"
        this.options.style.justifyContent = "flex-end"
        this.options.style.gap = "10px"

        this.sendButton.innerText = incomingText ? "Copy & Close" : "Send to Host"
        this.cancelButton.innerText = "Cancel"

        this.options.appendChild(this.sendButton)
        this.options.appendChild(this.cancelButton)

        this.root.appendChild(this.title)
        this.root.appendChild(this.description)
        this.root.appendChild(this.textarea)
        this.root.appendChild(this.options)
    }

    mount(parent: HTMLElement): void {
        parent.appendChild(this.root)
        // Auto-focus the text area slightly after mounting so pasting is instant
        setTimeout(() => this.textarea.focus(), 50)
    }

    unmount(parent: HTMLElement): void {
        parent.removeChild(this.root)
    }

    // Inside your ClipboardModal class
    onFinish(abort: AbortSignal): Promise<string | null> {
        return new Promise((resolve) => {
            this.sendButton.addEventListener("click", () => {
                // Resolve the text directly from the textarea
                // This avoids any "writeText" clipboard operations that trigger UTF8 errors
                resolve(this.textarea.value);
            }, { once: true, signal: abort });

            this.cancelButton.addEventListener("click", () => {
                resolve(null);
            }, { once: true, signal: abort });
        });
    }
}

// Stop propagation so the stream doesn't get it
function stopPropagationOn(element: HTMLElement) {
    element.addEventListener("keydown", onStopPropagation)
    element.addEventListener("keyup", onStopPropagation)
    element.addEventListener("keypress", onStopPropagation)
    element.addEventListener("click", onStopPropagation)
    element.addEventListener("mousedown", onStopPropagation)
    element.addEventListener("mouseup", onStopPropagation)
    element.addEventListener("mousemove", onStopPropagation)
    element.addEventListener("wheel", onStopPropagation)
    element.addEventListener("contextmenu", onStopPropagation)
    element.addEventListener("touchstart", onStopPropagation)
    element.addEventListener("touchmove", onStopPropagation)
    element.addEventListener("touchend", onStopPropagation)
    element.addEventListener("touchcancel", onStopPropagation)
}
function onStopPropagation(event: Event) {
    event.stopPropagation()
}
