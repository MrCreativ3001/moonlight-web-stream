import { Api, apiGetApps } from "../../api.js";
import { App } from "../../api_bindings.js";
import { showErrorPopup } from "../error.js";
import { FetchListComponent } from "../fetch_list.js";
import { ComponentEvent } from "../index.js";
import { Game, GameCache, GameEventListener } from "./index.js";

export class GameList extends FetchListComponent<App, Game> {
    private api: Api

    private eventTarget = new EventTarget()

    private hostId: number
    private activeApp: number | null = null

    constructor(api: Api, hostId: number, cache: App[] | null) {
        super({
            listClasses: ["app-list"],
            elementLiClasses: ["animated-list-element", "app-element"]
        })

        this.api = api

        this.hostId = hostId

        // Update cache
        if (cache != null) {
            this.updateCache(cache)
        } else {
            this.forceFetch()
        }
    }

    setActiveGame(appId: number | null) {
        this.activeApp = appId

        this.forceFetch()
    }

    async forceFetch() {
        const apps = await apiGetApps(this.api, {
            host_id: this.hostId,
        })

        this.updateCache(apps)
    }
    private createCache(data: App): GameCache {
        const cache = data as GameCache
        cache.activeApp = this.activeApp
        return cache
    }

    protected updateComponentData(component: Game, data: App): void {
        const cache = this.createCache(data)

        component.updateCache(cache)
    }
    protected getComponentDataId(component: Game): number {
        return component.getAppId()
    }
    protected getDataId(data: App): number {
        return data.app_id
    }
    protected insertList(dataId: number, data: App): void {
        const cache = this.createCache(data)

        const game = new Game(this.api, this.hostId, dataId, cache)
        game.addForceReloadListener(this.onForceReload.bind(this))

        this.list.append(game)
    }

    private onForceReload(event: ComponentEvent<Game>) {
        this.eventTarget.dispatchEvent(new ComponentEvent("ml-gamereload", event.component))
    }

    addForceReloadListener(listener: GameEventListener) {
        this.eventTarget.addEventListener("ml-gamereload", listener as any)
    }
    removeForceReloadListener(listener: GameEventListener) {
        this.eventTarget.removeEventListener("ml-gamereload", listener as any)
    }

    getHostId(): number {
        return this.hostId
    }

    mount(parent: HTMLElement): void {
        this.list.mount(parent)
    }
    unmount(parent: HTMLElement): void {
        this.list.unmount(parent)
    }
}