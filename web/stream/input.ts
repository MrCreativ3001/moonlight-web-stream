import { StreamCapabilities } from "../api_bindings.js"
import { ClientInputEvent, ControllerButtons, ControllerCapabilities, ControllerType, KeyAction, KeyModifiers, MouseButton, MouseButtonAction, TouchEventType } from "../uniffi/moonlight_common_bindings.js"
import { ByteBuffer, I16_MAX, U16_MAX, U8_MAX } from "./buffer.js"
import { ControllerConfig, emptyGamepadState, extractGamepadState, GamepadState, SUPPORTED_BUTTONS } from "./gamepad.js"
import { convertToKey, convertToModifiers, emptyKeyModifiers } from "./keyboard.js"
import { convertToButton } from "./mouse.js"
import { IControlStream } from "./transport/index.js"

// Normal scrolling multiplier
const TOUCH_SCROLL_MULTIPLIER = 1
// Distance until a touch cannot be a click anymore
const TOUCH_AS_CLICK_MAX_DISTANCE = 2
// Time till it's registered as a click, else it might be scrolling
const TOUCH_AS_CLICK_MIN_TIME_MS = 100
// Everything greater than this is a right click
const TOUCH_AS_CLICK_MAX_TIME_MS = 350
// How much to move to open up the screen keyboard when having three touches at the same time
const TOUCHES_AS_KEYBOARD_DISTANCE = 100
// Two-finger scroll only starts after one finger clearly commits to scrolling
const TWO_TOUCH_SCROLL_TRIGGER_DISTANCE = 15
// How long is the first tap allowed to be for it to maybe be a double tap
const DOUBLE_TAP_FIRST_TAP_MAX_TIME_MS = 100
// How much time is allowed after a touch release for a new tap to count both taps as a double tap
const DOUBLE_TAP_SECOND_TAP_MAX_TIME_MS = 200

const CONTROLLER_RUMBLE_INTERVAL_MS = 60

export type MouseScrollMode = "highres" | "normal"
export type MouseMode = "relative" | "follow" | "localCursor" | "pointAndDrag"
export type TouchMode = "touch" | "mouseRelative" | "localCursor" | "pointAndDrag"

export type StreamInputConfig = {
    mouseMode: MouseMode
    touchMode: TouchMode
    localCursorSensitivity: number
    controllerConfig: ControllerConfig
}

export function defaultStreamInputConfig(): StreamInputConfig {
    return {
        mouseMode: "follow",
        touchMode: "mouseRelative",
        localCursorSensitivity: 1,
        controllerConfig: {
            invertAB: false,
            invertXY: false,
            sendIntervalOverride: null
        }
    }
}

export type PredictedTouchAction = "default" | "drag" | "scroll" | "screenKeyboard" | "longPress"
export type ScreenKeyboardSetVisibleEvent = CustomEvent<{ visible: boolean }>
export type LocalCursorState = { visible: boolean, x: number, y: number }

export class StreamInput {

    private eventTarget = new EventTarget()

    private connected = false
    private config: StreamInputConfig
    private capabilities: StreamCapabilities = { touch: true }

    private controlStream: IControlStream | null = null

    private touchSupported: boolean | null = null
    private localCursorPosition: [number, number] | null = null
    buffer: any

    private streamSize: [number, number] = [0, 0]

    constructor(config?: StreamInputConfig) {
        this.config = defaultStreamInputConfig()
        if (config) {
            this.setConfig(config)
        }
    }

    setControlStream(controlStream: IControlStream) {
        // Clear state
        this.raiseAllKeys()

        this.controlStream = controlStream
    }

    setConfig(config: StreamInputConfig) {
        Object.assign(this.config, config)

        // Touch
        this.primaryTouch = null
        this.touchTracker.clear()
    }
    getConfig(): StreamInputConfig {
        return this.config
    }

    getCapabilities(): StreamCapabilities {
        return this.capabilities
    }

    // -- External Event Listeners
    addScreenKeyboardVisibleEvent(listener: (event: ScreenKeyboardSetVisibleEvent) => void) {
        this.eventTarget.addEventListener("ml-screenkeyboardvisible", listener as any)
    }

    // -- On Stream Start
    onStreamStart(capabilities: StreamCapabilities, desktopSize: [number, number]) {
        this.connected = true

        this.streamSize = desktopSize

        this.capabilities = capabilities
        this.registerBufferedControllers()
    }

    // -- Keyboard
    private pressedKeys: Set<number> = new Set()

    onKeyDown(event: KeyboardEvent) {
        this.sendKeyEvent(true, event)
    }
    onKeyUp(event: KeyboardEvent) {
        this.sendKeyEvent(false, event)
    }

    onPaste(event: ClipboardEvent) {

        const data = event.clipboardData
        if (!data) {
            return
        }

        console.debug("PASTE", data)

        const text = data.getData("text/plain")
        if (text) {
            console.debug("PASTE TEXT", text)

            // Before sending text raise all keys
            this.raiseAllKeys()

            this.sendText(text)
        }
    }

    private sendKeyEvent(isDown: boolean, event: KeyboardEvent) {
        const key = convertToKey(event)
        if (key == null) {
            return
        }

        if (isDown) {
            if (this.pressedKeys.has(key)) {
                return
            }

            this.pressedKeys.add(key)
        } else {
            if (!this.pressedKeys.has(key)) {
                return
            }

            this.pressedKeys.delete(key)
        }

        const modifiers = convertToModifiers(event)

        if ("debug" in console) {
            console.debug(
                isDown ? "DOWN" : "UP",
                event.code,
                convertToKey(event),
                convertToModifiers(event)
            )
        }
        this.sendKey(isDown, key, modifiers)
    }

    raiseAllKeys() {
        for (const key of this.pressedKeys) {
            this.sendKey(false, key, emptyKeyModifiers())
        }
        this.pressedKeys.clear()
    }

    // Note: key = StreamKeys.VK_, modifiers = StreamKeyModifiers.
    sendKey(isDown: boolean, key: number, modifiers: KeyModifiers) {
        this.controlStream?.send(new ClientInputEvent.Keyboard({
            action: isDown ? KeyAction.Down : KeyAction.Up,
            flags: {
                // TODO: what is this?
                sunshineNonNormalized: false
            },
            keyCode: key,
            modifiers,
        }))
    }
    sendText(text: string) {
        // TODO
    }

    // -- Mouse
    onMouseDown(event: MouseEvent, rect: DOMRect) {
        const button = convertToButton(event)
        if (button == null) {
            return
        }

        if (this.config.mouseMode == "relative" || this.config.mouseMode == "follow") {
            this.sendMouseButton(true, button)
        } else if (this.config.mouseMode == "localCursor") {
            this.initializeLocalCursor(rect, event.clientX, event.clientY)
            this.sendLocalCursorPosition()
            this.sendMouseButton(true, button)
        } else if (this.config.mouseMode == "pointAndDrag") {
            this.sendMousePositionClientCoordinates(event.clientX, event.clientY, rect, button)
        }
    }
    onMouseUp(event: MouseEvent) {
        const button = convertToButton(event)
        if (button == null) {
            return
        }

        if (this.config.mouseMode == "relative" || this.config.mouseMode == "follow" || this.config.mouseMode == "localCursor") {
            this.sendMouseButton(false, button)
        } else if (this.config.mouseMode == "pointAndDrag") {
            this.sendMouseButton(false, button)
        }
    }
    onMouseMove(event: MouseEvent, rect: DOMRect) {
        if (this.config.mouseMode == "relative") {
            this.sendMouseMoveClientCoordinates(event.movementX, event.movementY, rect)
        } else if (this.config.mouseMode == "follow") {
            this.sendMousePositionClientCoordinates(event.clientX, event.clientY, rect)
        } else if (this.config.mouseMode == "localCursor") {
            this.initializeLocalCursor(rect, event.clientX, event.clientY)
            this.moveLocalCursorClientCoordinates(event.movementX, event.movementY, rect)
        } else if (this.config.mouseMode == "pointAndDrag") {
            if (event.buttons) {
                // some button pressed
                this.sendMouseMoveClientCoordinates(event.movementX, event.movementY, rect)
            }
        }
    }
    onMouseWheel(event: WheelEvent) {
        this.sendMouseWheel(event.deltaX, -event.deltaY)
    }

    sendMouseMove(movementX: number, movementY: number) {
        this.controlStream?.send(new ClientInputEvent.MouseMoveRelative({
            deltaX: movementX,
            deltaY: movementY,
        }))
    }
    sendMouseMoveClientCoordinates(movementX: number, movementY: number, rect: DOMRect) {
        const scaledMovementX = movementX / rect.width * this.streamSize[0];
        const scaledMovementY = movementY / rect.height * this.streamSize[1];

        this.sendMouseMove(scaledMovementX, scaledMovementY)
    }
    sendMousePosition(x: number, y: number, referenceWidth: number, referenceHeight: number) {
        this.controlStream?.send(new ClientInputEvent.MouseMoveAbsolute({
            x,
            y,
            referenceWidth,
            referenceHeight,
        }))
    }
    sendMousePositionClientCoordinates(clientX: number, clientY: number, rect: DOMRect, mouseButton?: number) {
        const position = this.calcNormalizedPosition(clientX, clientY, rect)
        if (position) {
            const [x, y] = position
            this.sendMousePosition(x * 4096.0, y * 4096.0, 4096.0, 4096.0)

            if (mouseButton != undefined) {
                this.sendMouseButton(true, mouseButton)
            }
        }
    }
    private initializeLocalCursor(rect: DOMRect, clientX?: number, clientY?: number) {
        if (this.localCursorPosition) {
            return
        }

        if (clientX != null && clientY != null) {
            const position = this.calcNormalizedPosition(clientX, clientY, rect)
            if (position) {
                this.localCursorPosition = [
                    position[0] * this.streamSize[0],
                    position[1] * this.streamSize[1],
                ]
                return
            }
        }

        this.localCursorPosition = [
            this.streamSize[0] / 2,
            this.streamSize[1] / 2,
        ]
    }
    private clampLocalCursorPosition() {
        if (!this.localCursorPosition) {
            return
        }

        this.localCursorPosition[0] = Math.min(Math.max(this.localCursorPosition[0], 0), this.streamSize[0])
        this.localCursorPosition[1] = Math.min(Math.max(this.localCursorPosition[1], 0), this.streamSize[1])
    }
    private sendLocalCursorPosition() {
        if (!this.localCursorPosition) {
            return
        }

        this.sendMousePosition(
            this.localCursorPosition[0],
            this.localCursorPosition[1],
            this.streamSize[0],
            this.streamSize[1],
        )
    }
    private moveLocalCursorClientCoordinates(movementX: number, movementY: number, rect: DOMRect) {
        if (this.streamSize[0] <= 0 || this.streamSize[1] <= 0 || rect.width <= 0 || rect.height <= 0) {
            return
        }

        this.initializeLocalCursor(rect)
        if (!this.localCursorPosition) {
            return
        }

        this.localCursorPosition[0] += movementX / rect.width * this.streamSize[0] * this.config.localCursorSensitivity
        this.localCursorPosition[1] += movementY / rect.height * this.streamSize[1] * this.config.localCursorSensitivity
        this.clampLocalCursorPosition()
        this.sendLocalCursorPosition()
    }
    sendMouseButton(isDown: boolean, button: MouseButton) {
        this.controlStream?.send(new ClientInputEvent.MouseButton({
            action: isDown ? MouseButtonAction.Press : MouseButtonAction.Release,
            button
        }))
    }
    sendMouseWheel(deltaX: number, deltaY: number) {
        this.controlStream?.send(new ClientInputEvent.MouseScrollHorizontal({
            scrollX: deltaX
        }))
        this.controlStream?.send(new ClientInputEvent.MouseScrollVertical({
            scrollY: deltaY
        }))
    }

    // -- Touch
    private touchTracker: Map<number, {
        startTime: number
        originX: number
        originY: number
        x: number
        y: number
        mouseClicked: null | MouseButton,
        // point and drag: if we've moved the mouse to the touch
        // mouse relative: we've moved the mouse enough that it shouldn't be a click anymore
        mouseMoved: boolean
    }> = new Map()
    // The current action of all touches on screen
    // - default -> the default action for this touch mode / we're still trying to figure out what the user is trying to do
    // - drag -> movement continues without click handling on release
    // - scroll -> we're currently scrolling using primary touch
    // - screenKeyboard -> this current action is trying to pull up the on screen keyboard
    // - longPress -> single-finger long press is armed and will become a right click on release
    private touchMouseAction: PredictedTouchAction = "default"
    // The touch that is selected as the primary / controller of the action
    // Used in touch mode "relative" and "pointAndDrag"
    // E.g. scrolling movement
    private primaryTouch: number | null = null
    // If the next touch is a double tap?
    private nextTouchDoubleTap: boolean = false

    private onTouchData(data: ArrayBuffer) {
        const buffer = new ByteBuffer(new Uint8Array(data))
        this.touchSupported = buffer.getBool()
    }
    getLocalCursorState(): LocalCursorState {
        if (
            (this.config.touchMode != "localCursor" && this.config.mouseMode != "localCursor") ||
            !this.localCursorPosition ||
            this.streamSize[0] <= 0 ||
            this.streamSize[1] <= 0
        ) {
            return { visible: false, x: 0, y: 0 }
        }

        return {
            visible: true,
            x: this.localCursorPosition[0] / this.streamSize[0],
            y: this.localCursorPosition[1] / this.streamSize[1],
        }
    }

    private updateTouchTracker(touch: Touch) {
        const oldTouch = this.touchTracker.get(touch.identifier)
        if (!oldTouch) {
            this.touchTracker.set(touch.identifier, {
                startTime: Date.now(),
                originX: touch.clientX,
                originY: touch.clientY,
                x: touch.clientX,
                y: touch.clientY,
                mouseClicked: null,
                mouseMoved: false,
            })
        } else {
            oldTouch.x = touch.clientX
            oldTouch.y = touch.clientY
        }
    }

    private calcTouchTime(touch: { startTime: number }): number {
        return Date.now() - touch.startTime
    }
    private calcTouchOriginDistance(
        touch: { x: number, y: number } | { clientX: number, clientY: number },
        oldTouch: { originX: number, originY: number }
    ): number {
        if ("clientX" in touch) {
            return Math.hypot(touch.clientX - oldTouch.originX, touch.clientY - oldTouch.originY)
        } else {
            return Math.hypot(touch.x - oldTouch.originX, touch.y - oldTouch.originY)
        }
    }
    private shouldStartTwoTouchScroll(activeTouch?: Touch): boolean {
        if (this.touchTracker.size != 2) {
            return false
        }

        for (const [id, trackedTouch] of this.touchTracker.entries()) {
            const touchForDistance = activeTouch && activeTouch.identifier == id
                ? activeTouch
                : trackedTouch

            if (this.calcTouchOriginDistance(touchForDistance, trackedTouch) > TWO_TOUCH_SCROLL_TRIGGER_DISTANCE) {
                return true
            }
        }

        return false
    }

    onTouchStart(event: TouchEvent, rect: DOMRect) {
        for (const touch of event.changedTouches) {
            this.updateTouchTracker(touch)
        }

        if (this.config.touchMode == "touch") {
            for (const touch of event.changedTouches) {
                this.sendTouch(0, touch, rect)
            }
        } else if (this.config.touchMode == "mouseRelative" || this.config.touchMode == "localCursor" || this.config.touchMode == "pointAndDrag") {
            // Set primary touch if it doesn't exists currently
            for (const touch of event.changedTouches) {
                if (this.primaryTouch == null) {
                    this.primaryTouch = touch.identifier
                    this.touchMouseAction = "default"

                    if (this.config.touchMode == "localCursor") {
                        this.initializeLocalCursor(rect, touch.clientX, touch.clientY)
                    }
                }
            }

            const primaryTouch = this.primaryTouch != null && this.touchTracker.get(this.primaryTouch)

            // Detect dragging in mouse relative
            if ((this.config.touchMode == "mouseRelative" || this.config.touchMode == "localCursor") && primaryTouch && this.nextTouchDoubleTap) {
                if (primaryTouch.mouseClicked == null) {
                    this.sendMouseButton(true, MouseButton.Left)
                    primaryTouch.mouseClicked = MouseButton.Left
                }

                this.touchMouseAction = "drag"

                this.nextTouchDoubleTap = false
            }

            // Detect scrolling
            if (this.touchTracker.size == 3) {
                this.touchMouseAction = "screenKeyboard"
            }
        }
    }

    onTouchUpdate(rect: DOMRect) {
        if (this.primaryTouch == null) {
            return
        }
        const touch = this.touchTracker.get(this.primaryTouch)
        if (!touch) {
            return
        }

        const time = this.calcTouchTime(touch)
        if (this.config.touchMode == "pointAndDrag") {
            if (this.touchMouseAction == "default" && !touch.mouseMoved && time >= TOUCH_AS_CLICK_MIN_TIME_MS) {
                this.sendMousePositionClientCoordinates(touch.originX, touch.originY, rect)

                touch.mouseMoved = true
            }
        } else if ((this.config.touchMode == "mouseRelative" || this.config.touchMode == "localCursor") &&
            this.touchTracker.size == 1 &&
            this.touchMouseAction == "default" &&
            !touch.mouseMoved &&
            touch.mouseClicked == null &&
            time >= TOUCH_AS_CLICK_MAX_TIME_MS) {
            this.touchMouseAction = "longPress"
            this.nextTouchDoubleTap = false
        }
    }

    onTouchMove(event: TouchEvent, rect: DOMRect) {
        if (this.config.touchMode == "touch") {
            for (const touch of event.changedTouches) {
                this.sendTouch(1, touch, rect)
            }
        } else if (this.config.touchMode == "mouseRelative" || this.config.touchMode == "localCursor" || this.config.touchMode == "pointAndDrag") {
            for (const touch of event.changedTouches) {
                if (this.primaryTouch != touch.identifier) {
                    continue
                }
                const oldTouch = this.touchTracker.get(this.primaryTouch)
                if (!oldTouch) {
                    continue
                }

                const movementX = touch.clientX - oldTouch.x;
                const movementY = touch.clientY - oldTouch.y;

                if (this.touchMouseAction == "default") {
                    if (this.shouldStartTwoTouchScroll(touch)) {
                        this.touchMouseAction = "scroll"

                        if (oldTouch.mouseClicked != null) {
                            this.sendMouseButton(false, oldTouch.mouseClicked)
                            oldTouch.mouseClicked = null
                        }

                        if (this.config.touchMode == "pointAndDrag" && this.primaryTouch != null) {
                            const primaryTouch = this.touchTracker.get(this.primaryTouch)
                            if (primaryTouch) {
                                let middleX = 0;
                                let middleY = 0;
                                for (const trackedTouch of this.touchTracker.values()) {
                                    middleX += trackedTouch.x
                                    middleY += trackedTouch.y
                                }
                                middleX += touch.clientX - oldTouch.x
                                middleY += touch.clientY - oldTouch.y
                                middleX /= 2
                                middleY /= 2

                                primaryTouch.mouseMoved = true
                                this.sendMousePositionClientCoordinates(middleX, middleY, rect)
                            }
                        }
                    }
                }

                if (this.touchMouseAction == "default") {
                    const touchOriginDistance = this.calcTouchOriginDistance(touch, oldTouch)

                    // Normal mouse relative movement
                    if (this.config.touchMode == "mouseRelative") {
                        this.sendMouseMoveClientCoordinates(movementX, movementY, rect)

                        if (touchOriginDistance > TOUCH_AS_CLICK_MAX_DISTANCE) {
                            oldTouch.mouseMoved = true
                        }
                    } else if (this.config.touchMode == "localCursor") {
                        this.moveLocalCursorClientCoordinates(movementX, movementY, rect)

                        if (touchOriginDistance > TOUCH_AS_CLICK_MAX_DISTANCE) {
                            oldTouch.mouseMoved = true
                        }
                    }
                    // Point and Drag
                    // If we are over the touch as click distance go to the origin and drag
                    else if (this.config.touchMode == "pointAndDrag" && touchOriginDistance > TOUCH_AS_CLICK_MAX_DISTANCE) {
                        if (!oldTouch.mouseMoved) {
                            this.sendMousePositionClientCoordinates(oldTouch.originX, oldTouch.originY, rect)
                            oldTouch.mouseMoved = true
                        }

                        if (oldTouch.mouseClicked == null) {
                            this.sendMouseButton(true, MouseButton.Left)
                            oldTouch.mouseClicked = MouseButton.Left
                        }

                        this.touchMouseAction = "drag"
                    }
                } else if (this.touchMouseAction == "longPress") {
                    if (movementX != 0 || movementY != 0) {
                        this.touchMouseAction = "drag"
                        oldTouch.mouseMoved = true
                        oldTouch.mouseClicked = MouseButton.Left
                        this.sendMouseButton(true, MouseButton.Left)

                        if (this.config.touchMode == "localCursor") {
                            this.moveLocalCursorClientCoordinates(movementX, movementY, rect)
                        } else {
                            this.sendMouseMoveClientCoordinates(movementX, movementY, rect)
                        }
                    }
                } else if (this.touchMouseAction == "drag") {
                    // Do the dragging
                    if (this.config.touchMode == "localCursor") {
                        this.moveLocalCursorClientCoordinates(movementX, movementY, rect)
                    } else {
                        this.sendMouseMoveClientCoordinates(movementX, movementY, rect)
                    }
                } else if (this.touchMouseAction == "scroll") {
                    // inverting horizontal scroll
                    this.sendMouseWheel(-movementX * TOUCH_SCROLL_MULTIPLIER, movementY * TOUCH_SCROLL_MULTIPLIER)
                } else if (this.touchMouseAction == "screenKeyboard") {
                    // calculate if we should open the screen keyboard
                    const distanceY = touch.clientY - oldTouch.originY

                    if (distanceY < -TOUCHES_AS_KEYBOARD_DISTANCE) {
                        const customEvent: ScreenKeyboardSetVisibleEvent = new CustomEvent("ml-screenkeyboardvisible", {
                            detail: { visible: true }
                        })
                        this.eventTarget.dispatchEvent(customEvent)
                    } else if (distanceY > TOUCHES_AS_KEYBOARD_DISTANCE) {
                        const customEvent: ScreenKeyboardSetVisibleEvent = new CustomEvent("ml-screenkeyboardvisible", {
                            detail: { visible: false }
                        })
                        this.eventTarget.dispatchEvent(customEvent)
                    }
                }
            }
        }

        for (const touch of event.changedTouches) {
            this.updateTouchTracker(touch)
        }
    }

    onTouchEnd(event: TouchEvent, rect: DOMRect) {
        if (this.config.touchMode == "touch") {
            for (const touch of event.changedTouches) {
                this.sendTouch(2, touch, rect)
            }
        } else if (this.config.touchMode == "mouseRelative" || this.config.touchMode == "localCursor" || this.config.touchMode == "pointAndDrag") {
            const endingScroll = this.touchMouseAction == "scroll" && this.touchTracker.size == 2
            const endingTwoTouchTap = this.touchMouseAction == "default" && this.touchTracker.size == 2
            let endingTwoTouchTapShouldRightClick = false
            let handledTwoTouchTap = false

            if (endingTwoTouchTap) {
                endingTwoTouchTapShouldRightClick = true
                for (const trackedTouch of this.touchTracker.values()) {
                    if (this.calcTouchTime(trackedTouch) > TOUCH_AS_CLICK_MAX_TIME_MS) {
                        endingTwoTouchTapShouldRightClick = false
                        break
                    }
                }
            }

            for (const touch of event.changedTouches) {
                if (endingTwoTouchTap) {
                    if (!handledTwoTouchTap && endingTwoTouchTapShouldRightClick) {
                        this.sendMouseButton(true, MouseButton.Right)
                        this.sendMouseButton(false, MouseButton.Right)
                    }
                    handledTwoTouchTap = true

                    this.primaryTouch = null
                    this.nextTouchDoubleTap = false
                    continue
                }

                if (this.primaryTouch != touch.identifier) {
                    continue
                }
                const oldTouch = this.touchTracker.get(this.primaryTouch)
                this.primaryTouch = null

                if (oldTouch) {
                    if (endingScroll) {
                        continue
                    }

                    if (this.touchMouseAction == "longPress") {
                        this.sendMouseButton(true, MouseButton.Right)
                        this.sendMouseButton(false, MouseButton.Right)
                        this.nextTouchDoubleTap = false
                        continue
                    }

                    if (this.touchMouseAction == "drag") {
                        if (oldTouch.mouseClicked != null) {
                            this.sendMouseButton(false, oldTouch.mouseClicked)
                            oldTouch.mouseClicked = null
                        }
                        this.nextTouchDoubleTap = false
                        continue
                    }

                    const touchTime = this.calcTouchTime(oldTouch)
                    const touchOriginDistance = this.calcTouchOriginDistance(touch, oldTouch)

                    const maybeDoubleTap = touchTime < DOUBLE_TAP_FIRST_TAP_MAX_TIME_MS

                    // point and drag: Before making a click we should move the mouse to the position
                    if (this.config.touchMode == "pointAndDrag" && !oldTouch.mouseMoved) {
                        this.sendMousePositionClientCoordinates(touch.clientX, touch.clientY, rect)
                    }

                    const doClick = (maybeDoubleTap: boolean) => {
                        // See if we should make a click
                        if (
                            touchOriginDistance < TOUCH_AS_CLICK_MAX_DISTANCE &&
                            // mouse relative:
                            // - when having moved the mouse we shouldn't allow a click
                            // - when it's maybe a double click we shouldn't do a click
                            !(this.config.mouseMode == "relative" && !oldTouch.mouseMoved) &&
                            !maybeDoubleTap
                        ) {
                            // Should we right or left click?
                            let mouseButton
                            if (touchTime > TOUCH_AS_CLICK_MAX_TIME_MS) {
                                mouseButton = MouseButton.Right
                            } else {
                                mouseButton = MouseButton.Left
                            }

                            this.sendMouseButton(true, mouseButton)
                            oldTouch.mouseClicked = mouseButton
                        }

                        // Reset mouse click to neutral
                        if (oldTouch.mouseClicked != null) {
                            this.sendMouseButton(false, oldTouch.mouseClicked)
                        }
                    }

                    doClick(maybeDoubleTap)

                    if (maybeDoubleTap) {
                        this.nextTouchDoubleTap = true

                        // Schedule the click if it's not a double tap
                        setTimeout(() => {
                            if (this.primaryTouch == null) {
                                // no click present -> no double click -> We need to do the actual click
                                doClick(false)

                                // it cannot be a double tap
                                this.nextTouchDoubleTap = false
                            }
                        }, DOUBLE_TAP_SECOND_TAP_MAX_TIME_MS)
                    } else {
                        this.nextTouchDoubleTap = false
                    }
                }
            }
        }

        for (const touch of event.changedTouches) {
            this.touchTracker.delete(touch.identifier)
        }

        if (this.touchMouseAction == "scroll" && this.touchTracker.size < 2) {
            this.touchMouseAction = "default"
        }
    }

    onTouchCancel(event: TouchEvent, rect: DOMRect) {
        this.onTouchEnd(event, rect)
    }

    private calcNormalizedPosition(clientX: number, clientY: number, rect: DOMRect): [number, number] | null {
        const x = (clientX - rect.left) / rect.width
        const y = (clientY - rect.top) / rect.height

        if (x < 0 || x > 1.0 || y < 0 || y > 1.0) {
            // invalid touch
            return null
        }
        return [x, y]
    }
    private sendTouch(eventType: TouchEventType, touch: Touch, rect: DOMRect) {
        const position = this.calcNormalizedPosition(touch.clientX, touch.clientY, rect)
        if (!position) {
            return
        }
        const [x, y] = position

        this.controlStream?.send(new ClientInputEvent.Touch({
            eventType,
            rotation: touch.rotationAngle,
            pointerId: touch.identifier,
            x,
            y,
            pressureOrDistance: touch.force,
            contactAreaMajor: touch.radiusX,
            contactAreaMinor: touch.radiusY,
        }))
    }

    isTouchSupported(): boolean | null {
        return this.touchSupported
    }

    getCurrentPredictedTouchAction(): PredictedTouchAction {
        return this.touchMouseAction
    }

    // -- Controller
    // Wait for stream to connect and then send controllers
    private bufferedControllers: Array<number> = []
    private registerBufferedControllers() {
        const gamepads = navigator.getGamepads()

        for (const index of this.bufferedControllers.splice(0)) {
            const gamepad = gamepads[index]
            if (gamepad) {
                this.onGamepadConnect(gamepad)
            }
        }
    }

    private collectActuators(gamepad: Gamepad): Array<GamepadHapticActuator> {
        const actuators = []
        if ("vibrationActuator" in gamepad && gamepad.vibrationActuator) {
            actuators.push(gamepad.vibrationActuator)
        }
        if ("hapticActuators" in gamepad && gamepad.hapticActuators) {
            const hapticActuators = gamepad.hapticActuators as Array<GamepadHapticActuator>
            actuators.push(...hapticActuators)
        }
        return actuators
    }

    private gamepads: Array<{ gamepadIndex: number, oldState: GamepadState } | null> = []
    private gamepadRumbleInterval: number | null = null

    onGamepadConnect(gamepad: Gamepad) {
        if (!this.connected) {
            this.bufferedControllers.push(gamepad.index)
            return
        }

        if (this.gamepads.find(value => value?.gamepadIndex == gamepad.index)) {
            return
        }

        let id = -1
        for (let i = 0; i < this.gamepads.length; i++) {
            if (this.gamepads[i] == null) {
                this.gamepads[i] = { gamepadIndex: gamepad.index, oldState: emptyGamepadState() }
                id = i
                break
            }
        }
        if (id == -1) {
            id = this.gamepads.length
            this.gamepads.push({ gamepadIndex: gamepad.index, oldState: emptyGamepadState() })
        }

        // Start Rumble interval
        if (this.gamepadRumbleInterval == null) {
            this.gamepadRumbleInterval = window.setInterval(this.onGamepadRumbleInterval.bind(this), CONTROLLER_RUMBLE_INTERVAL_MS - 10)
        }

        // Reset rumble
        this.gamepadRumbleCurrent[0] = { lowFrequencyMotor: 0, highFrequencyMotor: 0, leftTrigger: 0, rightTrigger: 0 }

        let capabilities: ControllerCapabilities = {
            analogTriggers: false,
            rumble: false,
            triggerRumble: false,
            touchpad: false,
            accel: false,
            gyro: false,
            batteryState: false,
            rgbLed: false
        }

        // Rumble capabilities
        for (const actuator of this.collectActuators(gamepad)) {
            if ("effects" in actuator) {
                const supportedEffects = actuator.effects as Array<string>

                for (const effect of supportedEffects) {
                    if (effect == "dual-rumble") {
                        capabilities.rumble = true
                    } else if (effect == "trigger-rumble") {
                        capabilities.triggerRumble = true
                    }
                }
            } else if ("type" in actuator && (actuator.type == "vibration" || actuator.type == "dual-rumble")) {
                capabilities.rumble = true
            } else if ("playEffect" in actuator && typeof actuator.playEffect == "function") {
                // we're just hoping at this point
                capabilities.rumble = true
                capabilities.triggerRumble = true
            } else if ("pulse" in actuator && typeof actuator.pulse == "function") {
                capabilities.rumble = true
            }
        }

        this.sendControllerAdd(this.gamepads.length - 1, SUPPORTED_BUTTONS, capabilities)

        if (gamepad.mapping != "standard") {
            console.warn(`[Gamepad]: Unable to read values of gamepad with mapping ${gamepad.mapping}`)
        }
    }
    onGamepadDisconnect(event: GamepadEvent) {
        const index = this.gamepads.findIndex(value => value?.gamepadIndex == event.gamepad.index)
        if (index != -1) {
            const id = this.gamepads[index]?.gamepadIndex
            if (id != null) {
                this.sendControllerRemove(id)
            }

            this.gamepads[index] = null
        }
    }

    private lastGamepadUpdate: number = performance.now()
    onGamepadUpdate() {
        if (this.config.controllerConfig.sendIntervalOverride != null) {
            const now = performance.now()
            if (now - this.lastGamepadUpdate < (1000 / this.config.controllerConfig.sendIntervalOverride)) {
                return
            }
            this.lastGamepadUpdate = performance.now()
        }

        for (let gamepadId = 0; gamepadId < this.gamepads.length; gamepadId++) {
            const oldGamepadState = this.gamepads[gamepadId]
            if (oldGamepadState == null) {
                return
            }
            const gamepad = navigator.getGamepads()[oldGamepadState.gamepadIndex]
            if (!gamepad) {
                continue
            }

            if (gamepad.mapping != "standard") {
                continue
            }

            const state = extractGamepadState(gamepad, this.config.controllerConfig)
            if (state == oldGamepadState.oldState) {
                continue
            }
            oldGamepadState.oldState = state

            this.sendController(gamepadId, state)
        }
    }

    private onControllerData(data: ArrayBuffer) {
        this.buffer.reset()

        this.buffer.putU8Array(new Uint8Array(data))
        this.buffer.flip()

        // TODO: maybe move this into their respective controller channels?

        const ty = this.buffer.getU8()
        if (ty == 0) {
            // Rumble
            const id = this.buffer.getU8()
            const lowFrequencyMotor = this.buffer.getU16() / U16_MAX
            const highFrequencyMotor = this.buffer.getU16() / U16_MAX

            const gamepadIndex = this.gamepads[id]?.gamepadIndex
            if (gamepadIndex == null) {
                return
            }

            this.setGamepadEffect(gamepadIndex, "dual-rumble", { lowFrequencyMotor, highFrequencyMotor })
        } else if (ty == 1) {
            // Trigger Rumble
            const id = this.buffer.getU8()
            const leftTrigger = this.buffer.getU16() / U16_MAX
            const rightTrigger = this.buffer.getU16() / U16_MAX

            const gamepadIndex = this.gamepads[id]?.gamepadIndex
            if (gamepadIndex == null) {
                return
            }

            this.setGamepadEffect(gamepadIndex, "trigger-rumble", { leftTrigger, rightTrigger })
        }
    }

    // -- Controller rumble
    private gamepadRumbleCurrent: Array<{
        lowFrequencyMotor: number, highFrequencyMotor: number,
        leftTrigger: number, rightTrigger: number
    }> = []

    private setGamepadEffect(id: number, ty: "dual-rumble", params: { lowFrequencyMotor: number, highFrequencyMotor: number }): void
    private setGamepadEffect(id: number, ty: "trigger-rumble", params: { leftTrigger: number, rightTrigger: number }): void

    private setGamepadEffect(id: number, _ty: "dual-rumble" | "trigger-rumble", params: { lowFrequencyMotor: number, highFrequencyMotor: number } | { leftTrigger: number, rightTrigger: number }) {
        const rumble = this.gamepadRumbleCurrent[id]

        Object.assign(rumble, params)
    }

    private onGamepadRumbleInterval() {
        for (let id = 0; id < this.gamepads.length; id++) {
            const gamepadIndex = this.gamepads[id]?.gamepadIndex
            if (gamepadIndex == null) {
                continue
            }

            const rumble = this.gamepadRumbleCurrent[gamepadIndex]
            const gamepad = navigator.getGamepads()[gamepadIndex]
            if (gamepad && rumble) {
                this.refreshGamepadRumble(rumble, gamepad)
            }
        }
    }
    private refreshGamepadRumble(
        rumble: {
            lowFrequencyMotor: number, highFrequencyMotor: number,
            leftTrigger: number, rightTrigger: number
        },
        gamepad: Gamepad
    ) {
        // Browsers are making this more complicated than it is

        const actuators = this.collectActuators(gamepad)

        for (const actuator of actuators) {
            if ("effects" in actuator) {
                const supportedEffects = actuator.effects as Array<string>

                for (const effect of supportedEffects) {
                    if (effect == "dual-rumble") {
                        actuator.playEffect("dual-rumble", {
                            duration: CONTROLLER_RUMBLE_INTERVAL_MS,
                            weakMagnitude: rumble.lowFrequencyMotor,
                            strongMagnitude: rumble.highFrequencyMotor
                        })
                    } else if (effect == "trigger-rumble") {
                        actuator.playEffect("trigger-rumble", {
                            duration: CONTROLLER_RUMBLE_INTERVAL_MS,
                            leftTrigger: rumble.leftTrigger,
                            rightTrigger: rumble.rightTrigger
                        })
                    }
                }
            } else if ("type" in actuator && (actuator.type == "vibration" || actuator.type == "dual-rumble")) {
                actuator.playEffect(actuator.type as any, {
                    duration: CONTROLLER_RUMBLE_INTERVAL_MS,
                    weakMagnitude: rumble.lowFrequencyMotor,
                    strongMagnitude: rumble.highFrequencyMotor
                })
            } else if ("playEffect" in actuator && typeof actuator.playEffect == "function") {
                actuator.playEffect("dual-rumble", {
                    duration: CONTROLLER_RUMBLE_INTERVAL_MS,
                    weakMagnitude: rumble.lowFrequencyMotor,
                    strongMagnitude: rumble.highFrequencyMotor
                })
                actuator.playEffect("trigger-rumble", {
                    duration: CONTROLLER_RUMBLE_INTERVAL_MS,
                    leftTrigger: rumble.leftTrigger,
                    rightTrigger: rumble.rightTrigger
                })
            } else if ("pulse" in actuator && typeof actuator.pulse == "function") {
                const weak = Math.min(Math.max(rumble.lowFrequencyMotor, 0), 1);
                const strong = Math.min(Math.max(rumble.highFrequencyMotor, 0), 1);

                const average = (weak + strong) / 2.0

                actuator.pulse(average, CONTROLLER_RUMBLE_INTERVAL_MS)
            }
        }
    }

    // -- Controller Sending
    sendControllerAdd(id: number, supportedButtons: ControllerButtons, capabilities: ControllerCapabilities) {
        // TODO: add all controller when a new control stream is set
        this.controlStream?.send(new ClientInputEvent.ControllerConnect({
            controllerNumber: id,
            ty: ControllerType.Unknown,
            capabilities,
            supportedButtons,
        }))
    }
    sendControllerRemove(id: number) {
        this.controlStream?.send(new ClientInputEvent.ControllerDisconnect({
            controllerNumber: id,
        }))
    }
    // Values
    // - Trigger: range 0..1
    // - Stick: range -1..1
    sendController(id: number, state: GamepadState) {
        this.controlStream?.send(new ClientInputEvent.ControllerState({
            controllerNumber: id,
            pressedButtons: state.buttonFlags,
            leftTrigger: Math.max(0.0, Math.min(1.0, state.leftTrigger)) * U8_MAX,
            rightTrigger: Math.max(0.0, Math.min(1.0, state.rightTrigger)) * U8_MAX,
            leftStickX: Math.max(-1.0, Math.min(1.0, state.leftStickX)) * I16_MAX,
            leftStickY: Math.max(-1.0, Math.min(1.0, -state.leftStickY)) * I16_MAX,
            rightStickX: Math.max(-1.0, Math.min(1.0, state.rightStickX)) * I16_MAX,
            rightStickY: Math.max(-1.0, Math.min(1.0, -state.rightStickY)) * I16_MAX,
        }))
    }
}
