import { ComponentEvent } from "../index.js"
import { getCurrentLanguage, getTranslations } from "../../i18n.js"
import { InputComponent } from "../input.js"
import { FormModal } from "./form.js"

export type UserAuth = {
    name: string,
    password: string
}

export type OidcLogin = {
    displayLabel: string,
    loginUrl: string,
}

export class ApiUserPasswordPrompt extends FormModal<UserAuth> {

    private text: HTMLElement = document.createElement("h3")

    private name: InputComponent
    private password: InputComponent
    private passwordFile: InputComponent
    private oidcLogin?: OidcLogin
    private oidcButton: HTMLButtonElement = document.createElement("button")

    constructor(oidcLogin?: OidcLogin) {
        super()
        const i = getTranslations(getCurrentLanguage()).modal

        this.text.innerText = i.login
        this.oidcLogin = oidcLogin

        this.name = new InputComponent("ml-api-name", "text", i.username, {
            formRequired: true
        })

        this.password = new InputComponent("ml-api-password", "password", i.password, {
            formRequired: true
        })

        this.passwordFile = new InputComponent("ml-api-password-file", "file", i.passwordAsFile, { accept: ".txt" })
        this.passwordFile.addChangeListener(this.setFilePassword.bind(this))

        this.oidcButton.type = "button"
        this.oidcButton.innerText = oidcLogin ? `Sign in with ${oidcLogin.displayLabel}` : ""
        this.oidcButton.addEventListener("click", () => {
            if (!this.oidcLogin) {
                return
            }

            const returnTo = `${window.location.pathname}${window.location.search}`
            const loginUrl = new URL(this.oidcLogin.loginUrl, window.location.origin)
            loginUrl.searchParams.set("return_to", returnTo)
            window.location.assign(loginUrl.toString())
        })
    }

    private async setFilePassword(event: ComponentEvent<InputComponent>) {
        const files = event.component.getFiles()
        if (!files) {
            return
        }

        const file = files[0]
        if (!file) {
            return
        }
        const text = await file.text()

        // Remove carriage return and new line
        const password = text
            .replace(/\r/g, "")
            .replace(/\n/g, "")

        this.password.setValue(password)
    }

    reset(): void {
        this.name.reset()
        this.password.reset()
        this.passwordFile.reset()
    }
    submit(): UserAuth | null {
        const name = this.name.getValue()
        const password = this.password.getValue()

        if (name && password) {
            return { name, password }
        } else {
            return null
        }
    }

    onFinish(abort: AbortSignal): Promise<UserAuth | null> {
        const abortController = new AbortController()
        abort.addEventListener("abort", abortController.abort.bind(abortController))

        return new Promise((resolve, reject) => {
            super.onFinish(abortController.signal).then((data) => {
                abortController.abort()
                resolve(data)
            }, (data) => {
                abortController.abort()
                reject(data)
            })
        })
    }

    mountForm(form: HTMLFormElement): void {
        form.appendChild(this.text)

        this.name.mount(form)

        this.password.mount(form)
        this.passwordFile.mount(form)

        if (this.oidcLogin) {
            form.appendChild(this.oidcButton)
        }
    }
}
