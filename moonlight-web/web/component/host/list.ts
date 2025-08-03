import { DetailedHost, UndetailedHost } from "../../api_bindings.js"
import { Api, apiGetHosts } from "../../api.js"
import { ComponentEvent } from "../index.js"
import { Host, HostEventListener } from "./index.js"
import { ListComponent } from "../list.js"
import { FetchListComponent } from "../fetch_list.js"

export class HostList extends FetchListComponent<DetailedHost | UndetailedHost, Host> {
    private api: Api

    private eventTarget = new EventTarget()

    constructor(api: Api) {
        super()

        this.api = api

        this.list = new ListComponent([], {
            listElementClasses: ["host-list"],
            componentDivClasses: ["host-element"]
        })
    }

    async forceFetch() {
        const hosts = await apiGetHosts(this.api)

        this.updateCache(hosts)
    }

    protected updateComponentData(component: Host, data: DetailedHost | UndetailedHost): void {
        component.updateCache(data)
    }
    protected getComponentDataId(component: Host): number {
        return component.getHostId()
    }
    protected getDataId(data: DetailedHost | UndetailedHost): number {
        return data.host_id
    }

    protected insertList(dataId: number, data: DetailedHost | UndetailedHost | null): void {
        const newHost = new Host(this.api, dataId, data)

        this.list.append(newHost)

        newHost.addHostRemoveListener(this.removeHostListener.bind(this))
        newHost.addHostOpenListener(this.onHostOpenEvent.bind(this))
    }
    protected removeList(listIndex: number): void {
        const hostComponent = this.list.remove(listIndex)

        hostComponent?.addHostOpenListener(this.onHostOpenEvent.bind(this))
        hostComponent?.removeHostRemoveListener(this.removeHostListener.bind(this))
    }

    private removeHostListener(event: ComponentEvent<Host>) {
        const listIndex = this.list.get().findIndex(component => component.getHostId() == event.component.getHostId())

        this.removeList(listIndex)
    }

    getHost(hostId: number): Host | undefined {
        return this.list.get().find(host => host.getHostId() == hostId)
    }

    private onHostOpenEvent(event: ComponentEvent<Host>) {
        this.eventTarget.dispatchEvent(new ComponentEvent("ml-hostopen", event.component))
    }

    addHostOpenListener(listener: HostEventListener, options?: EventListenerOptions) {
        this.eventTarget.addEventListener("ml-hostopen", listener as EventListenerOrEventListenerObject, options)
    }
    removeHostOpenListener(listener: HostEventListener, options?: EventListenerOptions) {
        this.eventTarget.removeEventListener("ml-hostopen", listener as EventListenerOrEventListenerObject, options)
    }

    mount(parent: Element): void {
        this.list.mount(parent)
    }
    unmount(parent: Element): void {
        this.list.unmount(parent)
    }
}