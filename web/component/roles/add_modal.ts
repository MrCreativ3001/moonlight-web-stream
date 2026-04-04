import { PostRoleRequest, RoleType } from "../../api_bindings.js";
import { getCurrentLanguage, getTranslations } from "../../i18n.js";
import { InputComponent, SelectComponent } from "../input.js";
import { FormModal } from "../modal/form.js";
import { globalDefaultSettings, StreamSettingsComponent } from "../settings_menu.js";
import { RolePermissionsMenu } from "./permissions.js";

export class AddRoleModal extends FormModal<PostRoleRequest> {

    private header: HTMLElement = document.createElement("h2")

    private modalRoot: HTMLDivElement = document.createElement("div")

    private name: InputComponent
    private ty: SelectComponent

    private permissionsHeader = document.createElement("h3")
    private permissions: RolePermissionsMenu

    private defaultSettingsHeader = document.createElement("h3")
    private defaultSettings: StreamSettingsComponent

    constructor() {
        super()
        const i = getTranslations(getCurrentLanguage()).admin

        this.header.innerText = i.role
        this.modalRoot.appendChild(this.header)

        // Name
        this.name = new InputComponent("roleName", "text", i.name, {
            formRequired: true
        })
        this.name.mount(this.modalRoot)

        // Ty
        this.ty = new SelectComponent("roleTy", [
            { value: "User", name: "User" },
            { value: "Admin", name: "Admin" },
        ], {
            displayName: i.roleType,
            preSelectedOption: "User",
        })
        this.ty.mount(this.modalRoot)

        // Permissions
        this.permissionsHeader.innerText = i.permissions
        this.modalRoot.appendChild(this.permissionsHeader)

        this.permissions = new RolePermissionsMenu()
        this.permissions.mount(this.modalRoot)
        this.permissions.addChangeListener(this.onPermissionsChange.bind(this))

        // Default Settings
        this.defaultSettingsHeader.innerText = i.defaultSettings
        this.modalRoot.appendChild(this.defaultSettingsHeader)

        this.defaultSettings = new StreamSettingsComponent(this.permissions.getPermissions(), globalDefaultSettings())
        this.defaultSettings.mount(this.modalRoot)
    }

    private onPermissionsChange() {
        const settings = this.defaultSettings.getStreamSettings()
        this.defaultSettings.unmount(this.modalRoot)

        this.defaultSettings = new StreamSettingsComponent(this.permissions.getPermissions(), settings)
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
        const ty = this.ty.getValue() as RoleType

        return {
            name,
            ty,
            default_settings: this.defaultSettings.getStreamSettings(),
            permissions: this.permissions.getPermissions()
        }
    }
}
