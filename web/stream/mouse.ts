import { MouseButton } from "../uniffi/moonlight_common_bindings.js"

const BUTTON_MAPPINGS = new Array(5)
BUTTON_MAPPINGS[0] = MouseButton.Left
BUTTON_MAPPINGS[1] = MouseButton.Middle
BUTTON_MAPPINGS[2] = MouseButton.Right
BUTTON_MAPPINGS[3] = MouseButton.X1
BUTTON_MAPPINGS[4] = MouseButton.X2

export function convertToButton(event: MouseEvent): MouseButton | null {
    return BUTTON_MAPPINGS[event.button] ?? null
}