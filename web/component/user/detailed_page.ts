import { Component, ComponentEvent } from "../index.js";
import { Api, apiDeleteUser, apiPatchUser } from "../../api.js";
import { DetailedUser, PatchUserRequest, UserRole } from "../../api_bindings.js";
import { InputComponent, SelectComponent } from "../input.js";
import { createSelectRoleInput } from "./role_select.js";
import { tryDeleteUser, UserEventListener } from "./index.js";

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

        this.formRoot.classList.add("user-info")

        this.idElement = new InputComponent("userId", "number", "User Id", {
            defaultValue: `${user.id}`
        })
        this.idElement.setEnabled(false)
        this.idElement.mount(this.formRoot)

        this.name = new InputComponent("userName", "text", "User Name", {
            defaultValue: user.name,
        })
        this.name.setEnabled(false)
        this.name.mount(this.formRoot)

        this.password = new InputComponent("userPassword", "text", "Password", {
            placeholer: "New Password",
            formRequired: true,
            hasEnableCheckbox: true
        })
        this.password.setEnabled(false)
        this.password.mount(this.formRoot)

        this.role = createSelectRoleInput(user.role)
        this.role.mount(this.formRoot)

        this.clientUniqueId = new InputComponent("userClientUniqueId", "text", "Moonlight Client Id", {
            defaultValue: user.client_unique_id,
        })
        this.clientUniqueId.mount(this.formRoot)

        this.applyButton.innerText = "Apply"
        this.applyButton.type = "submit"
        this.formRoot.appendChild(this.applyButton)

        this.deleteButton.addEventListener("click", this.delete.bind(this))
        this.deleteButton.classList.add("user-info-delete")
        this.deleteButton.innerText = "Delete"
        this.deleteButton.type = "button"
        this.formRoot.appendChild(this.deleteButton)

        this.formRoot.addEventListener("submit", this.apply.bind(this))
    }

    private async apply(event: SubmitEvent) {
        event.preventDefault()

        let password = null
        if (this.password.isEnabled()) {
            password = this.password.getValue()
        }

        const request: PatchUserRequest = {
            id: this.id,
            role: this.role.getValue() as UserRole,
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