
export type TextEvent = CustomEvent<{ text: string }>
export type KeyboardModeEvent = CustomEvent<{ enabled: boolean }>
export type KeyboardDebugEvent = CustomEvent<{ label: string, data?: unknown }>

const KEYBOARD_SENTINEL = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"

export class ScreenKeyboard {

    private eventTarget = new EventTarget()
    private fakeElement = document.createElement("textarea")

    private enabled: boolean = false

    constructor() {
        this.fakeElement.classList.add("hiddeninput")
        this.fakeElement.name = "keyboard"
        this.fakeElement.autocomplete = "off"
        this.fakeElement.autocapitalize = "off"
        this.fakeElement.spellcheck = false
        if ("autocorrect" in this.fakeElement) {
            this.fakeElement.autocorrect = false
        }
        this.resetInputValue()

        this.fakeElement.addEventListener("input", this.onKeyInput.bind(this))
        this.fakeElement.addEventListener("compositionend", this.onCompositionEnd.bind(this))

        document.addEventListener("click", this.refocusIfEnabled.bind(this))
    }

    getHiddenElement() {
        return this.fakeElement
    }

    show() {
        this.debug("show")
        this.setEnabled(true)
    }
    hide() {
        this.debug("hide")
        this.setEnabled(false)
    }

    isVisible(): boolean {
        return this.enabled
    }
    private setEnabled(enabled: boolean) {
        const changed = this.enabled != enabled
        this.enabled = enabled

        if (enabled) {
            this.refocusIfEnabled()
        } else if (document.activeElement === this.fakeElement) {
            this.debug("blur-before-hide")
            this.fakeElement.blur()
        }

        if (changed) {
            const event: KeyboardModeEvent = new CustomEvent("ml-keyboardmode", {
                detail: { enabled }
            })
            this.eventTarget.dispatchEvent(event)
        }
    }
    private refocusIfEnabled() {
        if (!this.enabled || document.activeElement === this.fakeElement) {
            this.debug("refocus-skipped", {
                enabled: this.enabled,
                activeElement: describeElement(document.activeElement)
            })
            return
        }

        this.resetInputValue()
        this.debug("focus-before-refocus", {
            activeElement: describeElement(document.activeElement)
        })
        this.fakeElement.focus()
        this.debug("focus-after-refocus", {
            activeElement: describeElement(document.activeElement)
        })
    }

    addKeyDownListener(listener: (event: KeyboardEvent) => void) {
        this.eventTarget.addEventListener("keydown", listener as any)
    }
    addKeyUpListener(listener: (event: KeyboardEvent) => void) {
        this.eventTarget.addEventListener("keyup", listener as any)
    }
    addTextListener(listener: (event: TextEvent) => void) {
        this.eventTarget.addEventListener("ml-text", listener as any)
    }
    addKeyboardModeListener(listener: (event: KeyboardModeEvent) => void) {
        this.eventTarget.addEventListener("ml-keyboardmode", listener as any)
    }
    addDebugListener(listener: (event: KeyboardDebugEvent) => void) {
        this.eventTarget.addEventListener("ml-keyboarddebug", listener as any)
    }

    // -- Events
    private resetInputValue() {
        this.fakeElement.value = KEYBOARD_SENTINEL
        this.fakeElement.setSelectionRange(KEYBOARD_SENTINEL.length, KEYBOARD_SENTINEL.length)
    }
    private dispatchText(text: string) {
        const customEvent: TextEvent = new CustomEvent("ml-text", {
            detail: { text }
        })

        this.eventTarget.dispatchEvent(customEvent)
    }
    private dispatchKey(code: string) {
        const keyDown = new KeyboardEvent("keydown", { code })
        const keyUp = new KeyboardEvent("keyup", { code })

        this.eventTarget.dispatchEvent(keyDown)
        this.eventTarget.dispatchEvent(keyUp)
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
        const value = this.fakeElement.value
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
        this.debug("compositionend", {
            valueLength: this.fakeElement.value.length,
            activeElement: describeElement(document.activeElement)
        })
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
        this.debug("input", {
            inputType: event.inputType,
            data: event.data,
            isComposing: event.isComposing,
            valueLength: this.fakeElement.value.length,
            activeElement: describeElement(document.activeElement)
        })
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

    private debug(label: string, data?: unknown) {
        const event: KeyboardDebugEvent = new CustomEvent("ml-keyboarddebug", {
            detail: { label, data }
        })
        this.eventTarget.dispatchEvent(event)
    }
}

function describeElement(element: Element | null): string {
    if (!element) {
        return "null"
    }

    const id = element.id ? `#${element.id}` : ""
    const className = element instanceof HTMLElement && element.className ? `.${element.className}` : ""
    return `${element.tagName}${id}${className}`
}
