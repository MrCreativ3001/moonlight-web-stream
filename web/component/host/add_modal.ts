import { PostHostRequest } from "../../api_bindings.js"
import { getCurrentLanguage, getTranslations } from "../../i18n.js"
import { InputComponent } from "../input.js"
import { FormModal } from "../modal/form.js"

export class AddHostModal extends FormModal<PostHostRequest> {

    private header: HTMLElement = document.createElement("h2")

    private address: InputComponent
    private httpPort: InputComponent

    constructor() {
        super()
        const i = getTranslations(getCurrentLanguage()).addHost

        this.header.innerText = i.header

        this.address = new InputComponent("address", "text", i.address, {
            formRequired: true
        })

        this.httpPort = new InputComponent("httpPort", "text", i.port, {
            inputMode: "numeric"
        })
    }

    reset(): void {
        this.address.reset()
        this.httpPort.reset()
    }
    submit(): PostHostRequest | null {
        const address = this.address.getValue()
        const httpPort = parseInt(this.httpPort.getValue())

        return {
            address,
            http_port: httpPort
        }
    }

    mountForm(form: HTMLFormElement): void {
        form.appendChild(this.header)
        this.address.mount(form)
        this.httpPort.mount(form)
    }
}
