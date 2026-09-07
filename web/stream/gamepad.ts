import { ControllerButtons } from "../uniffi/moonlight_common_bindings"
import { deepEqual } from "../util"

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

// -- Bitflag mapping
const A_FLAG = 4096
const B_FLAG = 8192
const X_FLAG = 16384
const Y_FLAG = 32768
const UP_FLAG = 1
const DOWN_FLAG = 2
const LEFT_FLAG = 4
const RIGHT_FLAG = 8
const LB_FLAG = 256
const RB_FLAG = 512
const PLAY_FLAG = 16
const BACK_FLAG = 32
const LS_CLK_FLAG = 64
const RS_CLK_FLAG = 128
const SPECIAL_FLAG = 1024
const PADDLE1_FLAG = 65536
const PADDLE2_FLAG = 131072
const PADDLE3_FLAG = 262144
const PADDLE4_FLAG = 524288
const TOUCHPAD_FLAG = 1048576
const MISC_FLAG = 2097152

const BITFLAG_MAP: Record<keyof ControllerButtons, number> = {
    a: A_FLAG,
    b: B_FLAG,
    x: X_FLAG,
    y: Y_FLAG,
    up: UP_FLAG,
    down: DOWN_FLAG,
    left: LEFT_FLAG,
    right: RIGHT_FLAG,
    lb: LB_FLAG,
    rb: RB_FLAG,
    play: PLAY_FLAG,
    back: BACK_FLAG,
    lsClk: LS_CLK_FLAG,
    rsClk: RS_CLK_FLAG,
    special: SPECIAL_FLAG,
    paddle1: PADDLE1_FLAG,
    paddle2: PADDLE2_FLAG,
    paddle3: PADDLE3_FLAG,
    paddle4: PADDLE4_FLAG,
    touchpad: TOUCHPAD_FLAG,
    misc: MISC_FLAG
}

export function createControllerPacketBitflags(buttons: ControllerButtons): number {
    let bitflag = 0

    for (const entry in Object.entries(buttons)) {
        const [key, value] = entry
        const button = key as keyof ControllerButtons

        if (value) {
            bitflag |= BITFLAG_MAP[button]
        }
    }

    return bitflag
}
