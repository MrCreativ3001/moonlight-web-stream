import { ControllerButtons } from "../uniffi/moonlight_common_bindings.js"
import { deepEqual } from "../util.js"

export type ControllerConfig = {
    invertXY: boolean
    invertAB: boolean
    sendIntervalOverride: number | null
}

// https://w3c.github.io/gamepad/#remapping
const STANDARD_BUTTONS: Array<keyof ControllerButtons | null> = [
    "b",
    "a",
    "y",
    "x",
    "lb",
    "rb",
    // These are triggers
    null,
    null,
    "back",
    "play",
    "lsClk",
    "rsClk",
    "up",
    "down",
    "left",
    "right",
    "special",
]

export const SUPPORTED_BUTTONS: ControllerButtons = {
    a: true,
    b: true,
    x: true,
    y: true,
    up: true,
    down: true,
    left: true,
    right: true,
    lb: true,
    rb: true,
    play: true,
    back: true,
    lsClk: true,
    rsClk: true,
    special: true,
    paddle1: false,
    paddle2: false,
    paddle3: false,
    paddle4: false,
    touchpad: false,
    misc: false
}


function convertStandardButton(buttonIndex: number, config?: ControllerConfig): keyof ControllerButtons | null {
    let button = STANDARD_BUTTONS[buttonIndex] ?? null

    if (config?.invertAB) {
        if (button == "a") {
            button = "b"
        } else if (button == "b") {
            button = "a"
        }
    }
    if (config?.invertXY) {
        if (button == "x") {
            button = "y"
        } else if (button == "y") {
            button = "x"
        }
    }

    return button
}

export type GamepadState = {
    buttonFlags: ControllerButtons
    leftTrigger: number
    rightTrigger: number
    leftStickX: number
    leftStickY: number
    rightStickX: number
    rightStickY: number
}

export function extractGamepadState(gamepad: Gamepad, config: ControllerConfig): GamepadState {
    const state = emptyGamepadState()

    for (let buttonId = 0; buttonId < gamepad.buttons.length; buttonId++) {
        const button = gamepad.buttons[buttonId]

        const buttonName = convertStandardButton(buttonId, config)
        if (button.pressed && buttonName !== null) {
            state.buttonFlags[buttonName] = true
        }
    }

    state.leftTrigger = gamepad.buttons[6].value
    state.rightTrigger = gamepad.buttons[7].value

    state.leftStickX = gamepad.axes[0]
    state.leftStickY = gamepad.axes[1]
    state.rightStickX = gamepad.axes[2]
    state.rightStickY = gamepad.axes[3]

    return state
}

export function emptyGamepadState(): GamepadState {
    return {
        buttonFlags: {
            a: false,
            b: false,
            x: false,
            y: false,
            up: false,
            down: false,
            left: false,
            right: false,
            lb: false,
            rb: false,
            play: false,
            back: false,
            lsClk: false,
            rsClk: false,
            special: false,
            paddle1: false,
            paddle2: false,
            paddle3: false,
            paddle4: false,
            touchpad: false,
            misc: false
        },
        leftTrigger: 0,
        rightTrigger: 0,
        leftStickX: 0,
        leftStickY: 0,
        rightStickX: 0,
        rightStickY: 0,
    }
}

export function areGamepadStatesEqual(a: GamepadState, b: GamepadState): boolean {
    return deepEqual(a.buttonFlags, b.buttonFlags)
        && areFloatsEqual(a.leftTrigger, b.leftTrigger)
        && areFloatsEqual(a.rightTrigger, b.rightTrigger)
        && areFloatsEqual(a.leftStickX, b.leftStickX)
        && areFloatsEqual(a.leftStickY, b.leftStickY)
        && areFloatsEqual(a.rightStickX, b.rightStickX)
        && areFloatsEqual(a.rightStickY, b.rightStickY)
}

const FLOAT_COMPARE_MULTIPLIER = 100
function areFloatsEqual(a: number, b: number): boolean {
    return Math.round(a * FLOAT_COMPARE_MULTIPLIER) == Math.round(b * FLOAT_COMPARE_MULTIPLIER)
}