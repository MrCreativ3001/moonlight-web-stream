import { Component } from "../index.js"
import { Modal } from "../modal/index.js"
import { getCurrentLanguage, getTranslations } from "../../i18n.js"

type PairModalInit = {
    signal?: AbortSignal
}

export type PairModalOutput = "cancelled"

/// Modal shown while a pairing attempt is in flight: displays the pin, a
/// countdown for the pairing window, and a cancel button. Resolves with
/// "cancelled" when the user cancels, or null when closed via the signal
/// (pairing finished on its own).
export class PairModal implements Component, Modal<PairModalOutput | null> {

    private signal?: AbortSignal
    private textElement: HTMLElement = document.createElement("p")
    private countdownElement: HTMLElement = document.createElement("p")
    private cancelButton: HTMLButtonElement = document.createElement("button")

    private secondsLeft: number
    private countdownInterval: ReturnType<typeof setInterval> | null = null

    constructor(hostName: string, pin: string, expiresInSecs: number, init?: PairModalInit) {
        const i = getTranslations(getCurrentLanguage()).host
        this.textElement.innerText = i.pairPrompt(hostName, pin)

        this.secondsLeft = expiresInSecs
        this.countdownElement.innerText = i.pairCountdown(this.secondsLeft)

        this.cancelButton.innerText = i.pairCancel

        this.signal = init?.signal
    }

    mount(parent: HTMLElement): void {
        parent.appendChild(this.textElement)
        parent.appendChild(this.countdownElement)
        parent.appendChild(this.cancelButton)
    }
    unmount(parent: HTMLElement): void {
        parent.removeChild(this.textElement)
        parent.removeChild(this.countdownElement)
        parent.removeChild(this.cancelButton)
    }

    onFinish(abort: AbortSignal): Promise<PairModalOutput | null> {
        this.countdownInterval = setInterval(() => {
            const i = getTranslations(getCurrentLanguage()).host
            this.secondsLeft = Math.max(0, this.secondsLeft - 1)
            this.countdownElement.innerText = i.pairCountdown(this.secondsLeft)
        }, 1000)

        return new Promise(resolve => {
            let customController: AbortController | null = null

            const finish = (output: PairModalOutput | null) => {
                if (this.countdownInterval != null) {
                    clearInterval(this.countdownInterval)
                    this.countdownInterval = null
                }
                resolve(output)
                customController?.abort()
            }

            if (this.signal) {
                customController = new AbortController()
                this.signal.addEventListener("abort", () => {
                    finish(null)
                }, { once: true, signal: customController.signal })
            }

            this.cancelButton.addEventListener("click", () => {
                finish("cancelled")
            }, { signal: customController?.signal })

            if (customController) {
                abort.addEventListener("abort", customController.abort.bind(customController))
            }
        })
    }
}
