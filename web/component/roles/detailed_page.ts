import { Component, ComponentEvent } from "../index.js";
import { Api, apiGetRoles, apiPatchRole } from "../../api.js";
import { DetailedRole, PatchRoleRequest } from "../../api_bindings.js";
import { InputComponent } from "../input.js";
import { tryDeleteRole, RoleEventListener } from "./index.js";
import { showErrorPopup } from "../error.js";

export class DetailedRolePage implements Component {

    private api: Api

    private formRoot = document.createElement("form")

    // -- General role info
    private id

    private idElement: InputComponent
    private name: InputComponent

    // -- Default Settings

    // -- Permissions

    // -- Apply buttons
    private applyButton = document.createElement("button")
    private deleteButton = document.createElement("button")

    constructor(api: Api, role: DetailedRole) {
        this.api = api
        this.id = role.id

        this.formRoot.classList.add("role-info")

        this.idElement = new InputComponent("roleId", "number", "Role Id", {
            defaultValue: `${role.id}`
        })
        this.idElement.setEnabled(false)
        this.idElement.mount(this.formRoot)

        this.name = new InputComponent("roleName", "text", "Role Name", {
            defaultValue: role.name,
        })
        this.name.mount(this.formRoot)

        this.applyButton.innerText = "Apply"
        this.applyButton.type = "submit"
        this.formRoot.appendChild(this.applyButton)

        this.deleteButton.addEventListener("click", this.delete.bind(this))
        this.deleteButton.classList.add("role-info-delete")
        this.deleteButton.innerText = "Delete"
        this.deleteButton.type = "button"
        this.formRoot.appendChild(this.deleteButton)

        this.formRoot.addEventListener("submit", this.apply.bind(this))
    }

    private async apply(event: SubmitEvent) {
        event.preventDefault()

        const request: PatchRoleRequest = {
            id: this.id,
            name: this.name.getValue(),
            default_settings: {},
            permissions: {}
        };

        await apiPatchRole(this.api, request)
    }

    private async delete() {
        if (!await tryDeleteRole(this.api, this.id)) {
            return
        }

        this.formRoot.dispatchEvent(new ComponentEvent("ml-roledeleted", this))
    }

    addDeletedListener(listener: RoleEventListener, options?: EventListenerOptions) {
        this.formRoot.addEventListener("ml-roledeleted", listener as any, options)
    }
    removeDeletedListener(listener: RoleEventListener) {
        this.formRoot.removeEventListener("ml-roledeleted", listener as any)
    }

    getRoleId(): number {
        return this.id
    }

    mount(parent: HTMLElement): void {
        parent.appendChild(this.formRoot)
    }
    unmount(parent: HTMLElement): void {
        parent.removeChild(this.formRoot)
    }
}