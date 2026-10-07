import { Api, apiGetRole, getApi } from "./api"
import { DetailedRole, StreamKeys } from "./api_bindings"
import { Component } from "./component/index"
import { SelectComponent } from "./component/input"
import { FormModal } from "./component/modal/form"
import { getModalBackground, Modal, showMessage, showModal } from "./component/modal/index"
import { showNotification } from "./component/notification"
import { getLocalStreamSettings, Settings, TransportType } from "./component/settings_menu"
import { getSidebarRoot, setSidebar, setSidebarExtended, setSidebarStyle, Sidebar } from "./component/sidebar/index"
import { adoptRoleDefaultLanguage, getCurrentLanguage, getTranslations, Language, normalizeLanguage } from "./i18n"
import { requestKeyboardLock } from "./iframe"
import "./polyfill/index"
import { KeyboardModeChangeEvent, ScreenKeyboard, ScreenKeyboardListener, TextEvent } from "./keyboard"
import { InfoEvent, Stream, StreamCapabilities } from "./stream/index"
import { defaultStreamInputConfig, MouseMode, ScreenKeyboardSetVisibleEvent, StreamInputConfig } from "./stream/input"
import { emptyKeyModifiers } from "./stream/keyboard"
import { LogMessageType } from "./stream/log"
import { streamStatsToText } from "./stream/stats"
import "./styles/index"
import { LogLevel, uniffiInitAsync, Logger as UniffiLogger, setLogger as uniffiSetLogger } from "./uniffi/entry"
import { NativeScreenKeyboard } from "./keyboard/native_keyboard"

let I = getTranslations(getCurrentLanguage())

async function startApp() {
    const uniffiInit = uniffiInitAsync()

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

    // Wait for uniffi to finish it's initialization
    await uniffiInit
    // Set Uniffy Logger
    class CustomUniffiLogger implements UniffiLogger {
        log(level: LogLevel, message: string): void {
            switch (level) {
                case LogLevel.Trace:
                    console.trace(message)
                    break;
                case LogLevel.Debug:
                    console.debug(message)
                    break;
                case LogLevel.Warn:
                    console.warn(message)
                    break;
                case LogLevel.Info:
                    console.info(message)
                    break;
                case LogLevel.Error:
                    console.error(message)
                    break;
            }
        }
    }
    uniffiSetLogger(new CustomUniffiLogger(), LogLevel.Debug)

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

class ViewerApp implements Component {
    private api: Api

    private sidebar: ViewerSidebar

    private div = document.createElement("div")

    private screenKeyboard: ScreenKeyboard

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

        // Configure Screen Keyboard
        this.screenKeyboard = new NativeScreenKeyboard(I)

        this.screenKeyboard.addListener(this.sidebar)
        stopPropagationOn(this.screenKeyboard.getHiddenElement())

        // Configure stream
        this.previousMouseMode = this.inputConfig.mouseMode

        const browserWidth = Math.max(document.documentElement.clientWidth || 0, window.innerWidth || 0)
        const browserHeight = Math.max(document.documentElement.clientHeight || 0, window.innerHeight || 0)

        this.autoEnterFullscreenOnStart = settings.enterFullscreenOnStreamStart
        this.toggleFullscreenWithKeybind = settings.toggleFullscreenWithKeybind

        this.stream = new Stream(this.api, hostId, appId, settings, [browserWidth, browserHeight], bootstrapRole.permissions)
        this.startStream(settings)

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

        // When the page gets destroyed
        window.addEventListener("beforeunload", async () => {
            // Stop the current stream
            await this.stream.stop()
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

    private async startStream(settings: Settings) {
        setSidebarStyle({
            edge: settings.sidebarEdge,
        })

        // Optionally keep the sidebar toggle invisible until hovered/focused
        document.getElementById("sidebar-button")
            ?.classList.toggle("sidebar-button-unobtrusive", settings.hideSidebarButton)

        // Add app info listener
        this.stream.addInfoListener(this.onInfo.bind(this))

        // Create connection info modal
        const connectionInfo = new ConnectionInfoModal()
        const connectionInfoListener = connectionInfo.onInfo.bind(connectionInfo)
        this.stream.addInfoListener(connectionInfoListener)
        showModal(connectionInfo)

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
            const appName = data.appName

            document.title = `Stream: ${appName}`
        } else if (data.type == "connectionComplete") {
            this.sidebar.onCapabilitiesChange(data.capabilities)

            this.armFullscreenOnNextInteraction()
        }
    }

    private focusInput() {
        if (this.stream.getInput().getCurrentPredictedTouchAction() != "screenKeyboard" && !this.screenKeyboard.isVisible()) {
            const inputElement = document.getElementById("input") as HTMLDivElement
            inputElement.focus()
        }
    }

    onUserInteraction() {
        this.focusInput()

        this.stream.getVideoRenderer()?.onUserInteraction()
        this.stream.getAudioPlayer()?.onUserInteraction()
    }

    // -- Auto Fullscreen
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
        const screenKeyboard = this.screenKeyboard

        const newShown = event.detail.visible
        screenKeyboard.setVisible(newShown)
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
        window.requestAnimationFrame(this.onTouchUpdate.bind(this))

        this.stream.getInput().onTouchUpdate(this.getStreamRect())
        this.updateVideoOffset()
        this.renderLocalTouchCursor()
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
        window.requestAnimationFrame(this.onGamepadUpdate.bind(this))

        this.stream.getInput().onGamepadUpdate()
    }

    // Fullscreen
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

    // -- On Screen Keyboard + Local Touch Viewport
    private streamVideoTopOffsetPx = -100000
    private keyboardHeight = 0

    onScreenKeyboardModeChange(event: KeyboardModeChangeEvent) {
        this.keyboardHeight = event.detail.keyboardHeight

        this.updateVideoOffset()
    }

    private updateVideoOffset() {
        const viewport = this.screenKeyboard.getVisibleViewport()

        if (!viewport) {
            return
        }

        // get video offset
        const visibleTop = viewport.top
        const visibleBottom = visibleTop + viewport.height - this.keyboardHeight
        const streamRect = this.getStreamRect()
        const cursor = this.stream.getInput().getLocalCursorState()

        let offset = this.getVideoCenterOffset(visibleTop, visibleBottom)
        if (cursor.visible) {
            offset = this.getVideoOffsetWithCursor(cursor.y, streamRect.height, offset, visibleTop, visibleBottom)
        }

        this.setVideoPositionAndSize(offset, viewport.height)
    }

    private getVideoCenterOffset(visibleTop: number, visibleBottom: number): number {
        const visibleHeight = visibleBottom - visibleTop
        const visibleMiddle = visibleTop + visibleHeight / 2

        return visibleMiddle
    }
    private getVideoOffsetWithCursor(
        cursorY: number,
        streamHeight: number,
        streamMiddle: number,
        visibleTop: number,
        visibleBottom: number,
    ): number {
        const safeMargin = Math.min(100, streamHeight * 0.2)
        const scaledCursorY = cursorY * streamHeight

        const streamHeightHalf = streamHeight / 2
        const streamTop = streamMiddle - streamHeightHalf
        const absoluteCursorY = streamTop + scaledCursorY

        if (absoluteCursorY < visibleTop + safeMargin) {
            const targetY = visibleTop + safeMargin

            return streamMiddle + (targetY - absoluteCursorY)
        }

        if (absoluteCursorY > visibleBottom - safeMargin) {
            const targetY = visibleBottom - safeMargin

            return streamMiddle + (targetY - absoluteCursorY)
        }

        return streamMiddle
    }

    private setVideoPositionAndSize(offset: number, height: number) {
        if (Math.abs(offset - this.streamVideoTopOffsetPx) < 1) {
            return
        }

        this.streamVideoTopOffsetPx = offset

        document.documentElement.style.setProperty(
            "--stream-video-middle",
            `${offset}px`,
        )
        document.documentElement.style.setProperty(
            "--stream-video-height",
            `${height}px`,
        )
    }

    getScreenKeyboard(): ScreenKeyboard {
        return this.screenKeyboard
    }

    mount(parent: HTMLElement): void {
        parent.appendChild(this.div)
        parent.appendChild(this.screenKeyboard.getHiddenElement())
    }
    unmount(parent: HTMLElement): void {
        parent.removeChild(this.div)
        parent.removeChild(this.screenKeyboard.getHiddenElement())
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

            showModal(null)
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

class ViewerSidebar implements Component, Sidebar, ScreenKeyboardListener {
    private app: ViewerApp

    private div = document.createElement("div")

    private buttonDiv = document.createElement("div")

    private sendKeycodeButton = document.createElement("button")

    private keyboardButton = document.createElement("button")

    private lockMouseButton = document.createElement("button")
    private fullscreenButton = document.createElement("button")

    private statsButton = document.createElement("button")
    private exitStreamButton = document.createElement("button")

    private mouseMode: SelectComponent
    private touchMode: SelectComponent

    constructor(app: ViewerApp) {
        this.app = app

        // -- Configure divs
        this.div.classList.add("sidebar-stream")

        this.buttonDiv.classList.add("sidebar-stream-buttons")
        this.div.appendChild(this.buttonDiv)

        // Send keycode
        this.sendKeycodeButton.innerText = I.stream.sendKeycode
        this.sendKeycodeButton.addEventListener("click", async () => {
            const key = await showModal(new SendKeycodeModal())

            if (key == null) {
                return
            }

            this.app.getStream()?.getInput().sendKey(true, key, emptyKeyModifiers())
            this.app.getStream()?.getInput().sendKey(false, key, emptyKeyModifiers())
        })
        this.buttonDiv.appendChild(this.sendKeycodeButton)

        // Pointer Lock
        this.lockMouseButton.innerText = I.stream.lockMouse
        this.lockMouseButton.addEventListener("click", async () => {
            await this.app.requestPointerLock(true)
        })
        this.buttonDiv.appendChild(this.lockMouseButton)

        // Screen Keyboard Buttons
        this.keyboardButton.innerText = I.stream.keyboard
        this.keyboardButton.addEventListener("click", async () => {
            setSidebarExtended(false)
            this.app.getScreenKeyboard().setVisible(true)
        })
        this.buttonDiv.appendChild(this.keyboardButton)

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

    // -- Keyboard
    onText(event: TextEvent) {
        this.app.getStream()?.getInput().sendText(event.detail.text)
    }
    onKeyDown(event: KeyboardEvent) {
        this.app.getStream()?.getInput().onKeyDown(event)
    }
    onKeyUp(event: KeyboardEvent) {
        this.app.getStream()?.getInput().onKeyUp(event)
    }
    onKeyboardModeChange(event: KeyboardModeChangeEvent): void {
        this.app.onScreenKeyboardModeChange(event)
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

    extended(): void {

    }
    unextend(): void {

    }

    mount(parent: HTMLElement): void {
        parent.appendChild(this.div)
    }
    unmount(parent: HTMLElement): void {
        parent.removeChild(this.div)
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

// Stop propagation so the stream doesn't get it
export function stopPropagationOn(element: HTMLElement) {
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
