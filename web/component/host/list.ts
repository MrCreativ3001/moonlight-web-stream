import { DetailedHost, UndetailedHost } from "../../api_bindings.js"
import { Api, apiGetHosts } from "../../api.js"
import { ComponentEvent } from "../index.js"
import { Host, HostEventListener } from "./index.js"
import { FetchListComponent } from "../fetch_list.js"

export class HostList extends FetchListComponent<DetailedHost | UndetailedHost, Host> {
    private api: Api

    private eventTarget = new EventTarget()

    constructor(api: Api) {
        super({
            listClasses: ["host-list"],
            elementLiClasses: ["animated-list-element", "host-element"]
        })

        this.api = api
    }

    async forceFetch() {
        const hosts = await apiGetHosts(this.api)

        this.updateCache(hosts.response.hosts)

        let update
        while (update = await hosts.next()) {
            const host = this.getHost(update.host_id)
            if (host) {
                this.updateComponentData(host, update)
            }
        }
    }

    protected updateComponentData(component: Host, data: DetailedHost | UndetailedHost): void {
        component.updateCache(data, null)
    }
    protected getComponentDataId(component: Host): number {
        return component.getHostId()
    }
    protected getDataId(data: DetailedHost | UndetailedHost): number {
        return data.host_id
    }

    public insertList(dataId: number, data: DetailedHost | UndetailedHost | null): void {
        const newHost = new Host(this.api, dataId, data)

        this.list.append(newHost)

        newHost.addHostRemoveListener(this.removeHostListener.bind(this))
        newHost.addHostOpenListener(this.onHostOpenEvent.bind(this))
    }
    public removeList(listIndex: number): void {
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