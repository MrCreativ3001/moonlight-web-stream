import { Component, ComponentEvent } from "../index.js";
import { Api, apiPatchRole } from "../../api.js";
import { DetailedRole, PatchRoleRequest, RoleType } from "../../api_bindings.js";
import { getCurrentLanguage, getTranslations } from "../../i18n.js";
import { InputComponent, SelectComponent } from "../input.js";
import { tryDeleteRole, RoleEventListener } from "./index.js";
import { RolePermissionsMenu } from "./permissions.js";
import { StreamSettingsComponent } from "../settings_menu.js";

export class DetailedRolePage implements Component {

    private api: Api

    private formRoot = document.createElement("form")

    // -- General role info
    private id

    private idElement: InputComponent
    private name: InputComponent
    private ty: SelectComponent

    // -- Permissions
    private permissionsHeader = document.createElement("h3")
    private permissions: RolePermissionsMenu

    // -- Default Settings
    private defaultSettingsHeader = document.createElement("h3")
    private defaultSettings: StreamSettingsComponent

    // -- Apply buttons
    private applyButton = document.createElement("button")
    private deleteButton = document.createElement("button")

    constructor(api: Api, role: DetailedRole) {
        this.api = api
        this.id = role.id
        const i = getTranslations(getCurrentLanguage()).admin

        this.formRoot.classList.add("role-info", "settings-group")

        const header = document.createElement("div")
        header.classList.add("settings-group-header", "color-yellow")
        header.innerText = role.name
        this.formRoot.appendChild(header)

        // Role stuff
        this.idElement = new InputComponent("roleId", "number", i.roleId, {
            defaultValue: `${role.id}`
        })
        this.idElement.setEnabled(false)
        this.idElement.mount(this.formRoot)

        this.name = new InputComponent("roleName", "text", i.roleName, {
            defaultValue: role.name,
        })
        this.name.mount(this.formRoot)

        this.ty = new SelectComponent("roleTy", [
            { value: "User", name: "User" },
            { value: "Admin", name: "Admin" },
        ], {
            displayName: i.roleType,
            preSelectedOption: role.ty,
        })
        this.ty.mount(this.formRoot)

        // Permissions
        this.permissionsHeader.innerText = i.permissions
        this.formRoot.appendChild(this.permissionsHeader)

        this.permissions = new RolePermissionsMenu(role.permissions)
        this.permissions.mount(this.formRoot)
        this.permissions.addChangeListener(this.onPermissionsChange.bind(this))

        // Default Settings
        this.defaultSettingsHeader.innerText = i.defaultSettings
        this.formRoot.appendChild(this.defaultSettingsHeader)

        this.defaultSettings = new StreamSettingsComponent(role.permissions, role.default_settings)
        this.defaultSettings.mount(this.formRoot)

        // Apply / Delete
        this.applyButton.innerText = i.apply
        this.applyButton.type = "submit"
        this.formRoot.appendChild(this.applyButton)

        this.deleteButton.addEventListener("click", this.delete.bind(this))
        this.deleteButton.classList.add("role-info-delete")
        this.deleteButton.innerText = i.delete
        this.deleteButton.type = "button"
        this.formRoot.appendChild(this.deleteButton)

        this.formRoot.addEventListener("submit", this.apply.bind(this))
    }

    private onPermissionsChange() {
        const currentSettings = this.defaultSettings.getStreamSettings()

        this.defaultSettings.unmount(this.formRoot)

        this.defaultSettings = new StreamSettingsComponent(this.permissions.getPermissions(), currentSettings)
        this.defaultSettings.mountBefore(this.formRoot, this.applyButton)
    }

    private async apply(event: SubmitEvent) {
        event.preventDefault()

        const request: PatchRoleRequest = {
            id: this.id,
            name: this.name.getValue(),
            ty: this.ty.getValue() as RoleType,
            default_settings: this.defaultSettings.getStreamSettings(),
            permissions: this.permissions.getPermissions()
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
