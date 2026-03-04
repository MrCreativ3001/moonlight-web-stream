import { PostHostRequest } from "../../api_bindings.js"
import { InputComponent } from "../input.js"
import { FormModal } from "../modal/form.js"

export class AddHostModal extends FormModal<PostHostRequest> {

    private header: HTMLElement = document.createElement("h2")

    private address: InputComponent
    private httpPort: InputComponent

    constructor() {
        super()

        this.header.innerText = "Host"

        this.address = new InputComponent("address", "text", "Address", {
            formRequired: true
        })

        this.httpPort = new InputComponent("httpPort", "text", "Port", {
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