import { ScreenKeyboard, ScreenKeyboardListener, KeyboardModeChangeEvent } from "./index";

export class SimulatedKeyboard implements ScreenKeyboard {

    private element: HTMLDivElement = document.createElement("div")

    private listeners: Array<ScreenKeyboardListener> = []
    private visible = false

    private pressedKeyCodes: Set<string> = new Set()

    constructor() {
        this.element.classList.add("stream-keyboard-simulated")

        // construct keyboard
        for (const row of KEY_CODES) {
            const rowElement = document.createElement("div")
            this.element.appendChild(rowElement)

            // add individual keys
            for (const key of row) {
                const keyElement = document.createElement("button")
                rowElement.appendChild(keyElement)

                keyElement.classList.add("stream-keyboard-simulated-key")
                keyElement.style.flexGrow = `${key.width}`
                keyElement.innerText = `${key.displayName}`

                // mouse
                keyElement.addEventListener("mousedown", () => this.onElementDown(key, keyElement))
                keyElement.addEventListener("mouseup", () => this.onElementUp(key, keyElement))
                keyElement.addEventListener("mouseleave", () => this.onElementLeave(key, keyElement))
            }
        }

        this.setVisible(false)
    }

    private onElementLeave(key: Key, element: HTMLElement) {
        if (key.type == "action") {
            this.onElementUp(key, element)
        }
    }
    private onElementDown(key: Key, element: HTMLElement) {
        if (!key.code) {
            return
        }

        if (key.type == "action") {
            this.dispatchKeyEvent("keydown", key.code)
        }
    }
    private onElementUp(key: Key, element: HTMLElement) {
        if (key.type == "close") {
            this.setVisible(false)
        }

        if (!key.code) {
            return
        }

        if (key.type == "action") {
            this.dispatchKeyEvent("keyup", key.code)
        } else if (key.type == "toggle") {
            const isKeyDown = this.pressedKeyCodes.has(key.code)

            if (isKeyDown) {
                element.classList.remove("toggle-on")
            } else {
                element.classList.add("toggle-on")
            }

            const newEventType = isKeyDown ? "keyup" : "keydown"
            this.dispatchKeyEvent(newEventType, key.code)
        }
    }

    private dispatchKeyEvent(type: "keydown" | "keyup", code: string) {
        const event = new KeyboardEvent(type, {
            code,
        })

        if (type == "keyup") {
            this.pressedKeyCodes.delete(code)
            this.listeners.forEach(listener => listener.onKeyUp(event))
        } else if (type == "keydown") {
            this.pressedKeyCodes.add(code)
            this.listeners.forEach(listener => listener.onKeyDown(event))
        } else {
            throw `invalid key event type "${type}"`
        }
    }
    private releaseAllKeys() {
        for (const keyCode of this.pressedKeyCodes) {
            this.dispatchKeyEvent("keyup", keyCode)
        }
        this.pressedKeyCodes.clear()

        // Also toggle off all toggleable keys for the gui
        this.element.querySelectorAll(".toggle-on").forEach(child => child.classList.remove("toggle-on"))
    }

    isVisible(): boolean {
        return this.visible
    }
    setVisible(enabled: boolean): void {
        this.element.hidden = !enabled

        this.visible = enabled

        // Hide Keyboard if needed
        if (this.visible) {
            this.element.classList.remove("hidden")
        } else {
            this.element.classList.add("hidden")
        }

        // Release all currently held keys
        this.releaseAllKeys()

        // Trigger visible listeners
        const keyboardHeight = this.visible ? this.getKeyboardHeight() : 0
        const event: KeyboardModeChangeEvent = new CustomEvent("ml-keyboardmode", {
            detail: {
                enabled: this.visible,
                keyboardHeight,
            },
        })
        this.listeners.forEach(listener => listener.onKeyboardModeChange(event))
    }

    getVisibleViewport(): DOMRect | null {
        const viewport = window.visualViewport

        const top = viewport?.offsetTop ?? 0
        const left = viewport?.offsetLeft ?? 0
        const width = viewport?.width ?? window.innerWidth
        const height = viewport?.height ?? window.innerHeight

        return new DOMRect(
            left,
            top,
            width,
            height,
        )
    }

    addListener(listener: ScreenKeyboardListener): void {
        this.listeners.push(listener)
    }
    removeListener(listener: ScreenKeyboardListener): void {
        const index = this.listeners.indexOf(listener)
        if (index == -1) {
            return
        }

        this.listeners.splice(index, 1)
    }

    private getKeyboardHeight(): number {
        return this.element.getBoundingClientRect().height
    }

    getElement(): HTMLElement {
        return this.element
    }

}

type Key = { code: string | null, displayName: string, width: number, type: "action" | "toggle" | "close" }

// See also: https://w3c.github.io/uievents-code/#key-alphanumeric-writing-system
const KEY_CODES: Key[][] = [
    [
        // Function keys
        { code: "Escape", displayName: "ESC", width: 1, type: "action" },
        { code: "F1", displayName: "F1", width: 1, type: "action" },
        { code: "F2", displayName: "F2", width: 1, type: "action" },
        { code: "F3", displayName: "F3", width: 1, type: "action" },
        { code: "F4", displayName: "F4", width: 1, type: "action" },
        { code: "F5", displayName: "F5", width: 1, type: "action" },
        { code: "F6", displayName: "F6", width: 1, type: "action" },
        { code: "F7", displayName: "F7", width: 1, type: "action" },
        { code: "F8", displayName: "F8", width: 1, type: "action" },
        { code: "F9", displayName: "F9", width: 1, type: "action" },
        { code: "F10", displayName: "F10", width: 1, type: "action" },
        { code: "F11", displayName: "F11", width: 1, type: "action" },
        { code: "F12", displayName: "F12", width: 1, type: "action" },
        { code: "PrintScreen", displayName: "PRTSC", width: 1, type: "action" },
        { code: "ScrollLock", displayName: "SCRLK", width: 1, type: "action" },
        { code: "Pause", displayName: "PAUSE", width: 1, type: "action" },
    ],
    [
        // Number row (US QWERTY)
        { code: "Backquote", displayName: "`", width: 1, type: "action" },
        { code: "Digit1", displayName: "1", width: 1, type: "action" },
        { code: "Digit2", displayName: "2", width: 1, type: "action" },
        { code: "Digit3", displayName: "3", width: 1, type: "action" },
        { code: "Digit4", displayName: "4", width: 1, type: "action" },
        { code: "Digit5", displayName: "5", width: 1, type: "action" },
        { code: "Digit6", displayName: "6", width: 1, type: "action" },
        { code: "Digit7", displayName: "7", width: 1, type: "action" },
        { code: "Digit8", displayName: "8", width: 1, type: "action" },
        { code: "Digit9", displayName: "9", width: 1, type: "action" },
        { code: "Digit0", displayName: "0", width: 1, type: "action" },
        { code: "Minus", displayName: "-", width: 1, type: "action" },
        { code: "Equal", displayName: "=", width: 1, type: "action" },
        { code: "Backspace", displayName: "⌫", width: 2, type: "action" },
    ],
    [
        // QWERTY row
        { code: "Tab", displayName: "TAB", width: 1.5, type: "action" },
        { code: "KeyQ", displayName: "Q", width: 1, type: "action" },
        { code: "KeyW", displayName: "W", width: 1, type: "action" },
        { code: "KeyE", displayName: "E", width: 1, type: "action" },
        { code: "KeyR", displayName: "R", width: 1, type: "action" },
        { code: "KeyT", displayName: "T", width: 1, type: "action" },
        { code: "KeyY", displayName: "Y", width: 1, type: "action" },
        { code: "KeyU", displayName: "U", width: 1, type: "action" },
        { code: "KeyI", displayName: "I", width: 1, type: "action" },
        { code: "KeyO", displayName: "O", width: 1, type: "action" },
        { code: "KeyP", displayName: "P", width: 1, type: "action" },
        { code: "BracketLeft", displayName: "[", width: 1, type: "action" },
        { code: "BracketRight", displayName: "]", width: 1, type: "action" },
        { code: "Backslash", displayName: "\\", width: 1.5, type: "action" },
    ],
    [
        // Home row
        { code: "CapsLock", displayName: "CAPS", width: 1.75, type: "action" },
        { code: "KeyA", displayName: "A", width: 1, type: "action" },
        { code: "KeyS", displayName: "S", width: 1, type: "action" },
        { code: "KeyD", displayName: "D", width: 1, type: "action" },
        { code: "KeyF", displayName: "F", width: 1, type: "action" },
        { code: "KeyG", displayName: "G", width: 1, type: "action" },
        { code: "KeyH", displayName: "H", width: 1, type: "action" },
        { code: "KeyJ", displayName: "J", width: 1, type: "action" },
        { code: "KeyK", displayName: "K", width: 1, type: "action" },
        { code: "KeyL", displayName: "L", width: 1, type: "action" },
        { code: "Semicolon", displayName: ";", width: 1, type: "action" },
        { code: "Quote", displayName: "'", width: 1, type: "action" },
        { code: "Enter", displayName: "ENTER", width: 2.25, type: "action" },
    ],
    [
        // Shift row
        { code: "ShiftLeft", displayName: "SHIFT", width: 2.25, type: "toggle" },
        { code: "IntlBackslash", displayName: "< >", width: 1, type: "action" },
        { code: "KeyZ", displayName: "Z", width: 1, type: "action" },
        { code: "KeyX", displayName: "X", width: 1, type: "action" },
        { code: "KeyC", displayName: "C", width: 1, type: "action" },
        { code: "KeyV", displayName: "V", width: 1, type: "action" },
        { code: "KeyB", displayName: "B", width: 1, type: "action" },
        { code: "KeyN", displayName: "N", width: 1, type: "action" },
        { code: "KeyM", displayName: "M", width: 1, type: "action" },
        { code: "Comma", displayName: ",", width: 1, type: "action" },
        { code: "Period", displayName: ".", width: 1, type: "action" },
        { code: "Slash", displayName: "/", width: 1, type: "action" },
        { code: "ShiftRight", displayName: "SHIFT", width: 2.75, type: "toggle" },
    ],
    [
        // Navigation keys
        { code: "Insert", displayName: "INS", width: 1, type: "action" },
        { code: "Home", displayName: "HOME", width: 1, type: "action" },
        { code: "PageUp", displayName: "PGUP", width: 1, type: "action" },
        { code: "Delete", displayName: "DEL", width: 1, type: "action" },
        { code: "End", displayName: "END", width: 1, type: "action" },
        { code: "PageDown", displayName: "PGDN", width: 1, type: "action" },
        { code: "ArrowUp", displayName: "↑", width: 1, type: "action" },
        { code: "ArrowLeft", displayName: "←", width: 1, type: "action" },
        { code: "ArrowDown", displayName: "↓", width: 1, type: "action" },
        { code: "ArrowRight", displayName: "→", width: 1, type: "action" },
    ],
    [
        // Modifier keys and spacebar
        { code: "ControlLeft", displayName: "CTRL", width: 1.5, type: "toggle" },
        { code: "MetaLeft", displayName: "WIN", width: 1, type: "toggle" },
        { code: "AltLeft", displayName: "ALT", width: 1, type: "toggle" },
        { code: "Space", displayName: "SPACE", width: 6, type: "action" },
        { code: "AltRight", displayName: "ALT", width: 1, type: "toggle" },
        { code: "MetaRight", displayName: "WIN", width: 1, type: "toggle" },
        { code: "ContextMenu", displayName: "MENU", width: 1, type: "action" },
        { code: "ControlRight", displayName: "CTRL", width: 1.5, type: "toggle" },
        { code: null, displayName: "CLOSE", width: 2, type: "close" },
    ],
];
