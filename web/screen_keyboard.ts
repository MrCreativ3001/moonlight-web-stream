
export type TextEvent = CustomEvent<{ text: string }>

const KEYBOARD_SENTINEL = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"

export class ScreenKeyboard {

    private eventTarget = new EventTarget()
    private fakeElement = document.createElement("textarea")

    private visible: boolean = false

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

        document.addEventListener("click", this.hide.bind(this))
        this.fakeElement.addEventListener("blur", this.hide.bind(this))
    }

    getHiddenElement() {
        return this.fakeElement
    }

    show() {
        if (!this.visible) {
            this.visible = true

            this.resetInputValue()
            this.fakeElement.focus()
        }
    }
    hide() {
        if (this.visible) {
            this.visible = false

            this.fakeElement.focus()
            this.fakeElement.blur()
        }
    }

    isVisible(): boolean {
        return this.visible
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
