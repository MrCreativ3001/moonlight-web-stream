import { Component } from "../index.js"
import { showErrorPopup } from "../error.js"
import { FormModal } from "./form.js"

export interface Modal<Output> extends Component {
    onFinish(abort: AbortSignal): Promise<Output>
}

let modalAbort: AbortController | null = null
const modalBackground = document.getElementById("modal-overlay")
const modalParent = document.getElementById("modal-parent")
let previousModal: Modal<unknown> | null = null

// Don't allow context menu event through this background
modalBackground?.addEventListener("contextmenu", event => {
    event.stopImmediatePropagation()
})

export function getModalBackground(): HTMLElement | null {
    return modalBackground
}

export async function showModal<Output>(modal: Modal<Output> | null): Promise<Output | null> {
    if (modalParent == null) {
        showErrorPopup("cannot find modal parent")
        return null
    }
    if (modalBackground == null) {
        showErrorPopup("the modal overlay cannot be found")
    }

    if (modalAbort != null) {
        modalBackground?.classList.add("modal-disabled")
        modalAbort.abort()
        modalAbort = null
    }

    if (!modal) {
        return null
    }

    if (previousModal) {
        previousModal.unmount(modalParent)
    }
    previousModal = modal

    const abortController = new AbortController()

    modalAbort = abortController
    modal.mount(modalParent)
    modalBackground?.classList.remove("modal-disabled")

    const output = await modal.onFinish(abortController.signal)

    modalBackground?.classList.add("modal-disabled")
    modalAbort.abort()
    modalAbort = null

    return output
}

/// --- Helper Modals

export async function showPrompt(prompt: string, promptInit?: PromptInit): Promise<string | null> {
    const modal = new PromptModal(prompt, promptInit)

    return await showModal(modal)
}

type PromptInit = {
    defaultValue?: string,
    name?: string,
    type?: "text" | "password",
}

class PromptModal extends FormModal<string> {
    private message: HTMLElement = document.createElement("p")
    private textInput: HTMLInputElement = document.createElement("input")

    constructor(prompt: string, init?: PromptInit) {
        super()

        this.message.innerText = prompt

        if (init?.type) {
            this.textInput.type = init?.type
        }
        if (init?.defaultValue) {
            this.textInput.defaultValue = init?.defaultValue
        }
        if (init?.name) {
            this.textInput.name = init?.name
        }
    }

    reset(): void {
        this.textInput.value = ""
    }
    submit(): string | null {
        return this.textInput.value
    }

    mountForm(form: HTMLFormElement): void {
        form.appendChild(this.message)
        form.appendChild(this.textInput)
    }
}

type MessageInit = {
    signal?: AbortSignal
}

export async function showMessage(message: string, init?: MessageInit) {
    const modal = new MessageModal(message, init)

    await showModal(modal)
}

class MessageModal implements Component, Modal<void> {

    private signal?: AbortSignal
    private textElement: HTMLElement = document.createElement("p")
    private okButton: HTMLButtonElement = document.createElement("button")

    constructor(message: string, init?: MessageInit) {
        this.textElement.innerText = message

        this.okButton.innerText = "Ok"

        this.signal = init?.signal
    }

    mount(parent: Element): void {
        parent.appendChild(this.textElement)
        parent.appendChild(this.okButton)
    }
    unmount(parent: Element): void {
        parent.removeChild(this.textElement)
        parent.removeChild(this.okButton)
    }

    onFinish(abort: AbortSignal): Promise<void> {
        return new Promise((resolve, reject) => {
            let customController: AbortController | null = null

            if (this.signal) {
                customController = new AbortController()
                this.signal.addEventListener("abort", () => {
                    resolve()
                    customController?.abort()
                }, { once: true, signal: customController.signal })
            }

            this.okButton.addEventListener("click", () => {
                resolve()
                customController?.abort()
            }, { signal: customController?.signal })

            if (customController) {
                abort.addEventListener("abort", customController.abort.bind(customController))
            }
        })
    }
}