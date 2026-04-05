import { Api, apiGetRoles } from "../../api.js";
import { PostUserRequest, UndetailedRole } from "../../api_bindings.js";
import { getCurrentLanguage, getTranslations } from "../../i18n.js";
import { showErrorPopup } from "../error.js";
import { InputComponent, SelectComponent } from "../input.js";
import { FormModal } from "../modal/form.js";
import { createSelectRoleInput } from "./role_select.js";

export class AddUserModal extends FormModal<PostUserRequest> {

    private header: HTMLElement = document.createElement("h2")

    private modalRoot: HTMLDivElement = document.createElement("div")

    private name: InputComponent
    private defaultPassword: InputComponent
    private role: SelectComponent
    private clientUniqueId: InputComponent

    constructor(api: Api) {
        super()
        const i = getTranslations(getCurrentLanguage()).admin

        this.header.innerText = i.user
        this.modalRoot.appendChild(this.header)

        this.name = new InputComponent("userName", "text", i.name, {
            formRequired: true
        })
        this.name.mount(this.modalRoot)

        this.defaultPassword = new InputComponent("userPassword", "text", i.defaultPassword, {
            formRequired: true
        })
        this.defaultPassword.mount(this.modalRoot)

        this.role = createSelectRoleInput([])
        this.role.mount(this.modalRoot)
        apiGetRoles(api).then(roles => {
            this.role.unmount(this.modalRoot)

            this.role = createSelectRoleInput(roles.roles)
            this.role.mountBefore(this.modalRoot, this.clientUniqueId)
        })

        this.clientUniqueId = new InputComponent("userClientUniqueId", "text", i.moonlightClientId, {
            formRequired: true,
            hasEnableCheckbox: true
        })
        this.clientUniqueId.mount(this.modalRoot)
        this.name.addChangeListener(this.updateClientUniqueId.bind(this))
    }

    private updateClientUniqueId() {
        this.clientUniqueId.setPlaceholder(this.name.getValue())
    }

    mountForm(form: HTMLFormElement): void {
        form.appendChild(this.modalRoot)
    }

    reset(): void {
        this.name.reset()
        this.defaultPassword.reset()
        this.role.reset()
    }
    submit(): PostUserRequest | null {
        const i = getTranslations(getCurrentLanguage()).admin
        const name = this.name.getValue()
        const password = this.defaultPassword.getValue()
        const role = this.role.getValue()

        if (!role) {
            showErrorPopup(i.pleaseSelectRole)
            return null
        }

        let clientUniqueId = name
        if (this.clientUniqueId.isEnabled()) {
            clientUniqueId = this.clientUniqueId.getValue()
        }

        return {
            name,
            password,
            role_id: parseInt(role),
            client_unique_id: clientUniqueId,
        }
    }
}
