import { Component, ComponentEvent } from "../index.js";
import { Api, apiDeleteUser, apiGetRoles, apiPatchUser } from "../../api.js";
import { DetailedUser, PatchUserRequest } from "../../api_bindings.js";
import { getCurrentLanguage, getTranslations } from "../../i18n.js";
import { InputComponent, SelectComponent } from "../input.js";
import { createSelectRoleInput } from "./role_select.js";
import { tryDeleteUser, UserEventListener } from "./index.js";
import { showErrorPopup } from "../error.js";

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

    constructor(api: Api, user: DetailedUser) {
        this.api = api
        this.id = user.id
        const i = getTranslations(getCurrentLanguage()).admin

        this.formRoot.classList.add("user-info", "settings-group")

        const header = document.createElement("div")
        header.classList.add("settings-group-header", "color-green")
        header.innerText = user.name
        this.formRoot.appendChild(header)

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
            showErrorPopup(i.pleaseSelectRole)
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
