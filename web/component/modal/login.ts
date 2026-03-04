import { ComponentEvent } from "../index.js"
import { InputComponent } from "../input.js"
import { FormModal } from "./form.js"

export type UserAuth = {
    name: string,
    password: string
}

export class ApiUserPasswordPrompt extends FormModal<UserAuth> {

    private text: HTMLElement = document.createElement("h3")

    private name: InputComponent
    private password: InputComponent
    private passwordFile: InputComponent

    constructor() {
        super()

        this.text.innerText = "Login"

        this.name = new InputComponent("ml-api-name", "text", "Username", {
            formRequired: true
        })

        this.password = new InputComponent("ml-api-password", "password", "Password", {
            formRequired: true
        })

        this.passwordFile = new InputComponent("ml-api-password-file", "file", "Password as File", { accept: ".txt" })
        this.passwordFile.addChangeListener(this.setFilePassword.bind(this))
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
    }
}
