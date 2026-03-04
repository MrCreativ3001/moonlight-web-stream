import { User, UserEventListener } from "./index.js";
import { DetailedUser } from "../../api_bindings.js";
import { FetchListComponent } from "../fetch_list.js";
import { Api, apiGetUsers } from "../../api.js";
import { ComponentEvent } from "../index.js";

export class UserList extends FetchListComponent<DetailedUser, User> {
    private api: Api

    private eventTarget = new EventTarget()

    constructor(api: Api) {
        super({
            listClasses: ["user-list"],
            elementLiClasses: ["user-element"]
        })

        this.api = api
    }

    async forceFetch(): Promise<void> {
        const response = await apiGetUsers(this.api)

        this.updateCache(response.users)
    }

    public insertList(dataId: number, data: DetailedUser): void {
        const newUser = new User(this.api, data)

        this.list.append(newUser)

        newUser.addClickedListener(this.onUserClicked.bind(this))
        newUser.addDeletedListener(this.onUserDeleted.bind(this))
    }
    protected removeList(listIndex: number): void {

        const userComponent = this.list.remove(listIndex)

        userComponent?.removeClickedListener(this.onUserClicked.bind(this))
        userComponent?.removeDeletedListener(this.onUserDeleted.bind(this))
    }

    setFilter(filter: string) {
        this.list.setFilter((user) =>
            user.getCache()?.name.includes(filter) ?? false
        )
    }

    removeUser(id: number) {
        const componentIndex = this.list.get().findIndex(user => user.getUserId() == id)
        if (componentIndex != -1) {
            this.list.remove(componentIndex)
        }
    }

    private onUserClicked(event: ComponentEvent<User>) {
        this.eventTarget.dispatchEvent(new ComponentEvent("ml-userclicked", event.component))
    }

    addUserClickedListener(listener: UserEventListener, options?: EventListenerOptions) {
        this.eventTarget.addEventListener("ml-userclicked", listener as EventListenerOrEventListenerObject, options)
    }
    removeUserClickedListener(listener: UserEventListener, options?: EventListenerOptions) {
        this.eventTarget.removeEventListener("ml-userclicked", listener as EventListenerOrEventListenerObject, options)
    }

    private onUserDeleted(event: ComponentEvent<User>) {
        // Remove from our list
        this.list.removeValue(event.component)

        // Call other listeners
        this.eventTarget.dispatchEvent(new ComponentEvent("ml-userdeleted", event.component))
    }

    addUserDeletedListener(listener: UserEventListener, options?: EventListenerOptions) {
        this.eventTarget.addEventListener("ml-userdeleted", listener as EventListenerOrEventListenerObject, options)
    }
    removeUserDeletedListener(listener: UserEventListener, options?: EventListenerOptions) {
        this.eventTarget.removeEventListener("ml-userdeleted", listener as EventListenerOrEventListenerObject, options)
    }

    protected updateComponentData(component: User, data: DetailedUser): void {
        component.updateCache(data)
    }

    protected getDataId(data: DetailedUser): number {
        return data.id
    }
    protected getComponentDataId(component: User): number {
        return component.getUserId()
    }
}