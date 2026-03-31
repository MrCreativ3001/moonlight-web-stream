import { Api, apiDeleteRole, apiGetRole, apiGetUsers } from "../../api.js";
import { DetailedRole, UndetailedRole } from "../../api_bindings.js";
import { setContextMenu } from "../context_menu.js";
import { Component, ComponentEvent } from "../index.js";
import { showMessage } from "../modal/index.js";

export type RoleEventListener = (event: ComponentEvent<Role>) => void

export function formatRoleName(role: UndetailedRole | DetailedRole): string {
    return `${role.name} (${role.id})`
}

export async function tryDeleteRole(api: Api, id: number): Promise<boolean> {
    // Check if any user still has this role and show error if they do
    const usersResponse = await apiGetUsers(api)
    const usersWithRole = usersResponse.users.filter(user => user.role_id == id)
    if (usersWithRole.length > 0) {
        await showMessage(`To remove this role all users that are currently assigned this role either need to be deleted or assigned another role.\nCurrently these users still have the role:\n${JSON.stringify(usersWithRole.map(user => user.name))}`)
        return false
    }

    // Actually delete the role
    await apiDeleteRole(api, { id })

    return true
}

export class Role implements Component {

    private api: Api

    private role: DetailedRole | { id: number }

    private div = document.createElement("div")
    private nameElement = document.createElement("p")

    constructor(api: Api, role: DetailedRole | { id: number }) {
        this.api = api

        this.div.appendChild(this.nameElement)
        this.div.addEventListener("click", this.onClick.bind(this))
        this.div.addEventListener("contextmenu", this.onContextMenu.bind(this))

        this.role = role
        if ("name" in role) {
            this.updateCache(role)
        } else {
            this.forceFetch()
        }
    }

    async forceFetch() {
        const response = await apiGetRole(this.api, {
            id: this.role.id,
        })

        this.updateCache(response.role)
    }
    updateCache(role: DetailedRole) {
        this.role = role

        this.nameElement.innerText = formatRoleName(role)
    }

    private onClick() {
        this.div.dispatchEvent(new ComponentEvent("ml-roleclicked", this))
    }

    private onContextMenu(event: MouseEvent) {
        setContextMenu(event, {
            elements: [
                {
                    name: "Delete",
                    callback: this.onDelete.bind(this)
                }
            ]
        })
    }

    addClickedListener(listener: RoleEventListener, options?: EventListenerOptions) {
        this.div.addEventListener("ml-roleclicked", listener as any, options)
    }
    removeClickedListener(listener: RoleEventListener) {
        this.div.removeEventListener("ml-roleclicked", listener as any)
    }

    private async onDelete() {
        if (!await tryDeleteRole(this.api, this.role.id)) {
            return
        }

        this.div.dispatchEvent(new ComponentEvent("ml-roledeleted", this))
    }

    addDeletedListener(listener: RoleEventListener, options?: EventListenerOptions) {
        this.div.addEventListener("ml-roledeleted", listener as any, options)
    }
    removeDeletedListener(listener: RoleEventListener) {
        this.div.removeEventListener("ml-roledeleted", listener as any)
    }

    getCache(): DetailedRole | null {
        if ("name" in this.role) {
            return this.role
        } else {
            return null
        }
    }

    getRoleId(): number {
        return this.role.id
    }

    mount(parent: HTMLElement): void {
        parent.appendChild(this.div)
    }
    unmount(parent: HTMLElement): void {
        parent.removeChild(this.div)
    }
}