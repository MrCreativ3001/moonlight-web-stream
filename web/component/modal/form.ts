import { Component } from "../index"
import { getCurrentLanguage, getTranslations } from "../../i18n"
import { Modal } from "./index"

export abstract class FormModal<Output> implements Component, Modal<Output | null> {

    private formElement: HTMLFormElement = document.createElement("form")
    private mounted: boolean = false
    private submitButton: HTMLButtonElement = document.createElement("button")
    private cancelButton: HTMLButtonElement = document.createElement("button")
    private actions: HTMLDivElement = document.createElement("div")

    constructor() {
        const i = getTranslations(getCurrentLanguage()).modal
        this.submitButton.type = "submit"
        this.submitButton.innerText = i.ok
        this.submitButton.classList.add("modal-ok")

        this.cancelButton.type = "button"
        this.cancelButton.innerText = i.cancel
        this.cancelButton.classList.add("modal-cancel")

        this.actions.classList.add("modal-actions")
        this.actions.appendChild(this.cancelButton)
        this.actions.appendChild(this.submitButton)

        this.formElement.addEventListener("submit", (event) => event.preventDefault())
    }

    abstract reset(): void
    abstract submit(): Output | null

    abstract mountForm(form: HTMLFormElement): void

    mount(parent: Element): void {
        if (!this.mounted) {
            this.mountForm(this.formElement)
            this.formElement.appendChild(this.actions)
            this.mounted = true
        }

        this.reset()

        parent.appendChild(this.formElement)
    }
    unmount(parent: Element): void {
        parent.removeChild(this.formElement)
    }

    onFinish(signal: AbortSignal): Promise<Output | null> {
        const abortController = new AbortController()
        signal.addEventListener("abort", abortController.abort.bind(abortController))

        return new Promise((resolve, reject) => {
            this.formElement.addEventListener("submit", event => {
                const output = this.submit()

                if (output == null) {
                    return
                }

                abortController.abort()
                resolve(output)
            }, { signal: abortController.signal })

            this.cancelButton.addEventListener("click", event => {
                event.preventDefault()

                abortController.abort()
                resolve(null)
            }, { signal: abortController.signal })
        })
    }
}
