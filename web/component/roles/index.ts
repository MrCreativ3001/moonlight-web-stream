import { Api, apiDeleteRole, apiGetRole } from "../../api.js";
import { DetailedRole, UndetailedRole } from "../../api_bindings.js";
import { setContextMenu } from "../context_menu.js";
import { Component, ComponentEvent } from "../index.js";

export type RoleEventListener = (event: ComponentEvent<Role>) => void

export function formatRoleName(role: UndetailedRole | DetailedRole): string {
    return `${role.name} (${role.id})`
}

export async function tryDeleteRole(api: Api, id: number) {
    await apiDeleteRole(api, { id })
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

    private onDelete() {
        tryDeleteRole(this.api, this.role.id)

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