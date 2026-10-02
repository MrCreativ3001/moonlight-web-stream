import { KeyboardModeChangeEvent, ScreenKeyboard, ScreenKeyboardListener, TextEvent } from "."
import { Translations } from "../i18n"
import { stopPropagationOn } from "../stream"

const KEYBOARD_SENTINEL = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"

export class NativeScreenKeyboard implements ScreenKeyboard {
    private root = document.createElement("div")
    private fakeTextbox = document.createElement("textarea")
    private floatingKeyboardButton = document.createElement("button")

    private enabled: boolean = false

    constructor(I: Translations) {
        // Create div
        this.root.classList.add("stream-keyboard-native")

        // Create floating cancel button
        this.floatingKeyboardButton.innerText = "⌨×"
        this.floatingKeyboardButton.title = I.stream.hideKeyboard
        this.floatingKeyboardButton.ariaLabel = I.stream.hideKeyboard
        this.floatingKeyboardButton.addEventListener("click", event => {
            event.preventDefault()
            event.stopPropagation()

            this.setVisible(false)
        })
        stopPropagationOn(this.floatingKeyboardButton)
        this.root.appendChild(this.floatingKeyboardButton)

        if (window.visualViewport) {
            window.visualViewport.addEventListener("resize", this.repositionButton.bind(this))
        }
        this.repositionButton()

        // Create fake element
        this.fakeTextbox.classList.add("hiddeninput")
        this.fakeTextbox.name = "keyboard"
        this.fakeTextbox.autocomplete = "off"
        this.fakeTextbox.autocapitalize = "off"
        this.fakeTextbox.spellcheck = false
        if ("autocorrect" in this.fakeTextbox) {
            this.fakeTextbox.autocorrect = false
        }
        this.resetInputValue()

        this.fakeTextbox.addEventListener("input", this.onKeyInput.bind(this))
        this.fakeTextbox.addEventListener("compositionend", this.onCompositionEnd.bind(this))
        this.root.appendChild(this.fakeTextbox)

        document.addEventListener("click", this.refocusIfEnabled.bind(this))
    }

    getHiddenElement() {
        return this.root
    }

    isVisible(): boolean {
        return this.enabled
    }
    setVisible(enabled: boolean) {
        const changed = this.enabled != enabled

        this.enabled = enabled

        const beforeBaseline = window.visualViewport?.height

        if (enabled) {
            this.refocusIfEnabled()
        } else if (document.activeElement === this.fakeTextbox) {
            this.fakeTextbox.blur()
        }

        if (changed) {
            const afterBaseline = window.visualViewport?.height

            let keyboardHeight = 0
            if (beforeBaseline != null && afterBaseline != null) {
                if (enabled) {
                    keyboardHeight = beforeBaseline - afterBaseline
                } else {
                    keyboardHeight = 0
                }
            }

            const event: KeyboardModeChangeEvent = new CustomEvent("ml-keyboardmode", {
                detail: {
                    enabled,
                    keyboardHeight,
                },
            })
            this.listeners.forEach(listener => listener.onKeyboardModeChange(event))

            if (enabled) {
                this.floatingKeyboardButton.classList.add("visible")
            } else {
                this.floatingKeyboardButton.classList.remove("visible")
            }
        }
    }
    private refocusIfEnabled() {
        if (!this.enabled || document.activeElement === this.fakeTextbox) {
            return
        }

        this.resetInputValue()
        this.fakeTextbox.focus()
    }

    getVisibleViewport(): DOMRect {
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

    private listeners: Array<ScreenKeyboardListener> = []

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

    // -- Events
    private repositionButton() {
        const viewport = window.visualViewport
        if (!viewport) {
            return
        }

        this.floatingKeyboardButton.style.top = `${viewport.offsetTop + viewport.height}`
    }

    private resetInputValue() {
        this.fakeTextbox.value = KEYBOARD_SENTINEL
        this.fakeTextbox.setSelectionRange(KEYBOARD_SENTINEL.length, KEYBOARD_SENTINEL.length)
    }
    private dispatchText(text: string) {
        const customEvent: TextEvent = new CustomEvent("ml-text", {
            detail: { text }
        })

        this.listeners.forEach(listener => listener.onText(customEvent))
    }
    private dispatchKey(code: string) {
        const keyDown = new KeyboardEvent("keydown", { code })
        const keyUp = new KeyboardEvent("keyup", { code })

        this.listeners.forEach(listener => listener.onKeyDown(keyDown))
        this.listeners.forEach(listener => listener.onKeyUp(keyUp))
    }
    private dispatchTextWithLineBreaks(text: string) {
        const parts = text.split(/\r\n|\r|\n/)
        parts.forEach((part, index) => {
            if (part) {
                this.dispatchText(part)
            }
            if (index < parts.length - 1) {
                this.dispatchKey("Enter")
            }
        })
    }
    private extractInsertedText(): string {
        const value = this.fakeTextbox.value
        if (value == KEYBOARD_SENTINEL) {
            return ""
        }
        if (value.startsWith(KEYBOARD_SENTINEL)) {
            return value.slice(KEYBOARD_SENTINEL.length)
        }
        if (value.endsWith(KEYBOARD_SENTINEL)) {
            return value.slice(0, -KEYBOARD_SENTINEL.length)
        }
        if (value.includes(KEYBOARD_SENTINEL)) {
            return value.replace(KEYBOARD_SENTINEL, "")
        }

        return value
    }
    private onCompositionEnd() {
        const text = this.extractInsertedText()
        if (text) {
            this.dispatchTextWithLineBreaks(text)
        }

        this.resetInputValue()
    }
    private onKeyInput(event: Event) {
        if (!(event instanceof InputEvent)) {
            return
        }
        if (event.isComposing) {
            return
        }

        if (event.inputType == "insertLineBreak" || event.inputType == "insertParagraph") {
            this.dispatchKey("Enter")
        } else if ((event.inputType == "insertText" || event.inputType == "insertFromPaste" || event.inputType == "insertReplacementText") && event.data != null) {
            this.dispatchTextWithLineBreaks(event.data)
        } else if (event.inputType == "deleteContentBackward" || event.inputType == "deleteByCut") {
            this.dispatchKey("Backspace")
        } else if (event.inputType == "deleteContentForward") {
            this.dispatchKey("Delete")
        } else {
            const text = this.extractInsertedText()
            if (text) {
                this.dispatchTextWithLineBreaks(text)
            }
        }

        // Repopulate the input so that the deleteContent commands will work
        this.resetInputValue()
    }
}
