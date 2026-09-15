import { MouseButton } from "../uniffi/moonlight_common_bindings"

const BUTTON_MAPPINGS = new Array(5)
BUTTON_MAPPINGS[0] = MouseButton.Left
BUTTON_MAPPINGS[1] = MouseButton.Middle
BUTTON_MAPPINGS[2] = MouseButton.Right
BUTTON_MAPPINGS[3] = MouseButton.X1
BUTTON_MAPPINGS[4] = MouseButton.X2

const SWAPPED_BUTTON_MAPPINGS = new Array(5)
SWAPPED_BUTTON_MAPPINGS[0] = MouseButton.Right
SWAPPED_BUTTON_MAPPINGS[1] = MouseButton.Middle
SWAPPED_BUTTON_MAPPINGS[2] = MouseButton.Left
SWAPPED_BUTTON_MAPPINGS[3] = MouseButton.X1
SWAPPED_BUTTON_MAPPINGS[4] = MouseButton.X2

export function convertToButton(event: MouseEvent, swapButtons: boolean = false): MouseButton | null {
    const mappings = swapButtons ? SWAPPED_BUTTON_MAPPINGS : BUTTON_MAPPINGS
    return mappings[event.button] ?? null
}
