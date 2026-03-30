import { Api } from "../../api.js";
import { PostRoleRequest } from "../../api_bindings.js";
import { InputComponent } from "../input.js";
import { FormModal } from "../modal/form.js";
import { globalDefaultSettings, StreamSettingsComponent } from "../settings_menu.js";
import { RolePermissionsMenu } from "./permissions.js";

export class AddRoleModal extends FormModal<PostRoleRequest> {

    private header: HTMLElement = document.createElement("h2")

    private modalRoot: HTMLDivElement = document.createElement("div")

    private name: InputComponent

    private permissionsHeader = document.createElement("h3")
    private permissions: RolePermissionsMenu

    private defaultSettingsHeader = document.createElement("h3")
    private defaultSettings: StreamSettingsComponent

    constructor() {
        super()

        this.header.innerText = "Role"
        this.modalRoot.appendChild(this.header)

        // Name
        this.name = new InputComponent("roleName", "text", "Name", {
            formRequired: true
        })
        this.name.mount(this.modalRoot)

        // Permissions
        this.permissionsHeader.innerText = "Permissions"
        this.modalRoot.appendChild(this.permissionsHeader)

        this.permissions = new RolePermissionsMenu()
        this.permissions.mount(this.modalRoot)

        // Default Settings
        this.defaultSettingsHeader.innerText = "Default Settings"
        this.modalRoot.appendChild(this.defaultSettingsHeader)

        this.defaultSettings = new StreamSettingsComponent(this.permissions.getPermissions(), globalDefaultSettings())
        this.defaultSettings.mount(this.modalRoot)
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
            default_settings: this.defaultSettings.getStreamSettings(),
            permissions: this.permissions.getPermissions()
        }
    }
}
