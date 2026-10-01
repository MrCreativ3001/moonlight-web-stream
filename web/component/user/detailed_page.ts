import { Component, ComponentEvent } from "../index"
import { Api, apiDeleteDefaultUser, apiGetDefaultUser, apiGetRoles, apiPatchUser, apiPutDefaultUser } from "../../api"
import { DetailedUser, PatchUserRequest } from "../../api_bindings"
import { getCurrentLanguage, getTranslations } from "../../i18n"
import { InputComponent, SelectComponent } from "../input"
import { createSelectRoleInput } from "./role_select"
import { tryDeleteUser, UserEventListener } from "./index"
import { showNotification } from "../notification"
import { showModal } from "../modal"
import { FormModal } from "../modal/form"

export class DetailedUserPage implements Component {

    private api: Api

    private formRoot = document.createElement("form")

    private id

    private idElement: InputComponent
    private name: InputComponent
    private password: InputComponent
    private role: SelectComponent
    private clientUniqueId: InputComponent

    private applyButton = document.createElement("button")
    private deleteButton = document.createElement("button")
    private setDefaultButton = document.createElement("button")
    private removeAsDefaultButton = document.createElement("button")

    constructor(api: Api, user: DetailedUser) {
        this.api = api
        this.id = user.id
        const i = getTranslations(getCurrentLanguage()).admin

        this.formRoot.classList.add("user-info")

        this.idElement = new InputComponent("userId", "number", i.userId, {
            defaultValue: `${user.id}`
        })
        this.idElement.setEnabled(false)
        this.idElement.mount(this.formRoot)

        this.name = new InputComponent("userName", "text", i.userName, {
            defaultValue: user.name,
        })
        this.name.setEnabled(false)
        this.name.mount(this.formRoot)

        this.password = new InputComponent("userPassword", "text", i.password, {
            placeholer: i.newPassword,
            formRequired: true,
            hasEnableCheckbox: true
        })
        this.password.setEnabled(false)
        this.password.mount(this.formRoot)

        this.role = createSelectRoleInput([], user.role_id)
        this.role.mount(this.formRoot)
        apiGetRoles(api).then(roles => {
            this.role.unmount(this.formRoot)

            this.role = createSelectRoleInput(roles.roles, user.role_id)
            this.role.mountBefore(this.formRoot, this.clientUniqueId)
        })

        this.clientUniqueId = new InputComponent("userClientUniqueId", "text", i.moonlightClientId, {
            defaultValue: user.client_unique_id,
        })
        this.clientUniqueId.mount(this.formRoot)

        this.applyButton.innerText = i.apply
        this.applyButton.type = "submit"
        this.formRoot.appendChild(this.applyButton)

        this.deleteButton.addEventListener("click", this.delete.bind(this))
        this.deleteButton.classList.add("user-info-delete")
        this.deleteButton.innerText = i.delete
        this.deleteButton.type = "button"
        this.formRoot.appendChild(this.deleteButton)

        this.setDefaultButton.addEventListener("click", this.setDefault.bind(this))
        this.setDefaultButton.innerText = i.setDefault
        this.setDefaultButton.type = "button"
        this.formRoot.appendChild(this.setDefaultButton)

        this.removeAsDefaultButton.addEventListener("click", this.removeAsDefault.bind(this))
        this.removeAsDefaultButton.classList.add("user-info-remove-as-default")
        this.removeAsDefaultButton.innerText = i.removeAsDefault
        this.removeAsDefaultButton.type = "button"

        this.checkIsDefault()

        this.formRoot.addEventListener("submit", this.apply.bind(this))
    }

    private async apply(event: SubmitEvent) {
        event.preventDefault()
        const i = getTranslations(getCurrentLanguage()).admin

        let password = null
        if (this.password.isEnabled()) {
            password = this.password.getValue()
        }

        const role = this.role.getValue()
        if (!role) {
            showNotification(i.pleaseSelectRole)
            return
        }

        const request: PatchUserRequest = {
            id: this.id,
            role_id: parseInt(role),
            password,
            client_unique_id: this.clientUniqueId.getValue()
        };

        await apiPatchUser(this.api, request)
    }

    private async delete() {
        await tryDeleteUser(this.api, this.id)

        this.formRoot.dispatchEvent(new ComponentEvent("ml-userdeleted", this))
    }

    private async setDefault() {
        const accepted = await showModal(new SetDefaultUserDialog())
        if (!accepted) {
            return
        }

        await apiPutDefaultUser(this.api, {
            id: this.id,
        })

        this.checkIsDefault()
    }

    private async removeAsDefault() {
        await apiDeleteDefaultUser(this.api)

        this.checkIsDefault()
    }

    private async checkIsDefault() {
        const defaultUser = await apiGetDefaultUser(this.api)
        if (defaultUser.id == this.id) {
            this.formRoot.appendChild(this.removeAsDefaultButton)
        } else {
            if (this.formRoot.contains(this.removeAsDefaultButton)) {
                this.formRoot.removeChild(this.removeAsDefaultButton)
            }
        }
    }

    addDeletedListener(listener: UserEventListener, options?: EventListenerOptions) {
        this.formRoot.addEventListener("ml-userdeleted", listener as any, options)
    }
    removeDeletedListener(listener: UserEventListener) {
        this.formRoot.removeEventListener("ml-userdeleted", listener as any)
    }

    getUserId(): number {
        return this.id
    }

    mount(parent: HTMLElement): void {
        parent.appendChild(this.formRoot)
    }
    unmount(parent: HTMLElement): void {
        parent.removeChild(this.formRoot)
    }
}

class SetDefaultUserDialog extends FormModal<boolean> {
    private message: HTMLParagraphElement = document.createElement("p")

    constructor() {
        super()
        const i = getTranslations(getCurrentLanguage()).admin

        this.message.innerText = i.setDefaultUserDialog
    }

    mountForm(form: HTMLFormElement): void {
        form.appendChild(this.message)
    }

    reset(): void {
        // do nothing
    }
    submit(): boolean {
        return true
    }
}
