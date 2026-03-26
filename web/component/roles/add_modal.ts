import { Api } from "../../api.js";
import { PostRoleRequest } from "../../api_bindings.js";
import { InputComponent } from "../input.js";
import { FormModal } from "../modal/form.js";

export class AddRoleModal extends FormModal<PostRoleRequest> {

    private header: HTMLElement = document.createElement("h2")

    private modalRoot: HTMLDivElement = document.createElement("div")

    private name: InputComponent

    constructor() {
        super()

        this.header.innerText = "Role"
        this.modalRoot.appendChild(this.header)

        this.name = new InputComponent("roleName", "text", "Name", {
            formRequired: true
        })
        this.name.mount(this.modalRoot)
    }

    mountForm(form: HTMLFormElement): void {
        form.appendChild(this.modalRoot)
    }

    reset(): void {
        this.name.reset()
    }
    submit(): PostRoleRequest | null {
        const name = this.name.getValue()

        return {
            name,
            default_settings: {},
            permissions: {}
        }
    }
}