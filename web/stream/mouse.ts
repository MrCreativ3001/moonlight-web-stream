import { StreamMouseButton } from "../api_bindings.js"

const BUTTON_MAPPINGS = new Array(5)
BUTTON_MAPPINGS[0] = StreamMouseButton.LEFT
BUTTON_MAPPINGS[1] = StreamMouseButton.MIDDLE
BUTTON_MAPPINGS[2] = StreamMouseButton.RIGHT
BUTTON_MAPPINGS[3] = StreamMouseButton.X1
BUTTON_MAPPINGS[4] = StreamMouseButton.X2

export function convertToButton(event: MouseEvent): number | null {
    return BUTTON_MAPPINGS[event.button] ?? null
}