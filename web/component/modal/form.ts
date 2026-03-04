import { Component } from "../index.js"
import { Modal, showModal } from "./index.js"

export abstract class FormModal<Output> implements Component, Modal<Output | null> {

    private formElement: HTMLFormElement = document.createElement("form")
    private mounted: boolean = false
    private submitButton: HTMLButtonElement = document.createElement("button")
    private cancelButton: HTMLButtonElement = document.createElement("button")

    constructor() {
        this.submitButton.type = "submit"
        this.submitButton.innerText = "Ok"

        this.cancelButton.innerText = "Cancel"

        this.formElement.addEventListener("submit", (event) => event.preventDefault())
    }

    abstract reset(): void
    abstract submit(): Output | null

    abstract mountForm(form: HTMLFormElement): void

    mount(parent: Element): void {
        if (!this.mounted) {
            this.mountForm(this.formElement)
            this.formElement.appendChild(this.submitButton)
            this.formElement.appendChild(this.cancelButton)
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
