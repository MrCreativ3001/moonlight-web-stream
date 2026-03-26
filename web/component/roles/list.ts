import { Role, RoleEventListener } from "./index.js"
import { ComponentEvent } from "../index.js"
import { Api, apiGetRoles } from "../../api.js"
import { DetailedRole, UndetailedRole } from "../../api_bindings.js"
import { FetchListComponent } from "../fetch_list.js"

export class RoleList extends FetchListComponent<UndetailedRole, Role> {
    private api: Api

    private eventTarget = new EventTarget()

    constructor(api: Api) {
        super({
            listClasses: ["role-list"],
            elementLiClasses: ["role-element"]
        })

        this.api = api
    }

    async forceFetch(): Promise<void> {
        const response = await apiGetRoles(this.api)

        this.updateCache(response.roles)
    }

    public insertList(dataId: number, data: UndetailedRole): void {
        const newRole = new Role(this.api, data)

        this.list.append(newRole)

        newRole.addClickedListener(this.onRoleClicked.bind(this))
        newRole.addDeletedListener(this.onRoleDeleted.bind(this))
    }
    protected removeList(listIndex: number): void {

        const userComponent = this.list.remove(listIndex)

        userComponent?.removeClickedListener(this.onRoleClicked.bind(this))
        userComponent?.removeDeletedListener(this.onRoleDeleted.bind(this))
    }

    setFilter(filter: string) {
        this.list.setFilter((user) =>
            user.getCache()?.name.includes(filter) ?? false
        )
    }

    removeRole(id: number) {
        const componentIndex = this.list.get().findIndex(user => user.getRoleId() == id)
        if (componentIndex != -1) {
            this.list.remove(componentIndex)
        }
    }

    private onRoleClicked(event: ComponentEvent<Role>) {
        this.eventTarget.dispatchEvent(new ComponentEvent("ml-userclicked", event.component))
    }

    addRoleClickedListener(listener: RoleEventListener, options?: EventListenerOptions) {
        this.eventTarget.addEventListener("ml-userclicked", listener as EventListenerOrEventListenerObject, options)
    }
    removeRoleClickedListener(listener: RoleEventListener, options?: EventListenerOptions) {
        this.eventTarget.removeEventListener("ml-userclicked", listener as EventListenerOrEventListenerObject, options)
    }

    private onRoleDeleted(event: ComponentEvent<Role>) {
        // Remove from our list
        this.list.removeValue(event.component)

        // Call other listeners
        this.eventTarget.dispatchEvent(new ComponentEvent("ml-userdeleted", event.component))
    }

    addRoleDeletedListener(listener: RoleEventListener, options?: EventListenerOptions) {
        this.eventTarget.addEventListener("ml-userdeleted", listener as EventListenerOrEventListenerObject, options)
    }
    removeRoleDeletedListener(listener: RoleEventListener, options?: EventListenerOptions) {
        this.eventTarget.removeEventListener("ml-userdeleted", listener as EventListenerOrEventListenerObject, options)
    }

    protected updateComponentData(component: Role, data: DetailedRole): void {
        component.updateCache(data)
    }

    protected getDataId(data: DetailedRole): number {
        return data.id
    }
    protected getComponentDataId(component: Role): number {
        return component.getRoleId()
    }
}