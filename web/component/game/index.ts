import { Component, ComponentEvent } from "../index.js";
import { Api, apiGetAppImage, apiHostCancel } from "../../api.js";
import { App } from "../../api_bindings.js";
import { setContextMenu } from "../context_menu.js";
import { showMessage } from "../modal/index.js";
import { APP_NO_IMAGE } from "../../resources/index.js";
import { buildUrl } from "../../config_.js";

export type GameCache = App & { activeApp: number | null }

export type GameEventListener = (event: ComponentEvent<Game>) => void

export class Game implements Component {
    private api: Api

    private hostId: number
    private appId: number

    private mounted: number = 0
    private divElement: HTMLDivElement = document.createElement("div")

    private imageBlob: Blob | null = null
    private imageBlobUrl: string | null = null
    private imageElement: HTMLImageElement = document.createElement("img")

    private cache: GameCache

    constructor(api: Api, hostId: number, appId: number, cache: GameCache) {
        this.api = api

        this.hostId = hostId
        this.appId = appId

        this.cache = cache

        // Configure image
        this.imageElement.classList.add("app-image")
        this.imageElement.src = APP_NO_IMAGE

        this.forceLoadImage(false)

        // Configure div
        this.divElement.classList.add("app")

        this.divElement.appendChild(this.imageElement)

        this.divElement.addEventListener("click", this.onClick.bind(this))
        this.divElement.addEventListener("contextmenu", this.onContextMenu.bind(this))

        this.updateCache(cache)
    }

    async forceLoadImage(forceServerRefresh: boolean) {
        this.imageBlob = await apiGetAppImage(this.api, {
            host_id: this.hostId,
            app_id: this.appId,
            force_refresh: forceServerRefresh
        })

        this.updateImage()
    }
    private updateImage() {
        // generate and set url
        if (this.imageBlob && !this.imageBlobUrl && this.mounted > 0) {
            this.imageBlobUrl = URL.createObjectURL(this.imageBlob)

            this.imageElement.classList.add("app-image-loaded")
            this.imageElement.src = this.imageBlobUrl
        }

        // revoke url
        if (this.imageBlobUrl && this.mounted <= 0) {
            URL.revokeObjectURL(this.imageBlobUrl)
            this.imageBlobUrl = null

            this.imageElement.classList.remove("app-image-loaded")
            this.imageElement.src = ""
        }
    }

    updateCache(cache: GameCache) {
        this.cache = cache

        this.divElement.classList.remove("app-inactive")
        this.divElement.classList.remove("app-active")

        if (this.isActive()) {
            this.divElement.classList.add("app-active")
        } else if (this.cache.activeApp != null) {
            this.divElement.classList.add("app-inactive")
        }
    }

    private async onClick(event: MouseEvent) {
        if (this.cache.activeApp != null) {
            const elements = []

            if (this.isActive()) {
                elements.push({
                    name: "Resume Session",
                    callback: async () => {
                        this.startStream()

                        const event = new ComponentEvent("ml-gamereload", this)
                        this.divElement.dispatchEvent(event)
                    }
                })
            }

            elements.push({
                name: "Stop Current Session",
                callback: async () => {
                    const response = await apiHostCancel(this.api, { host_id: this.hostId })
                    if (!response.success) {
                        await showMessage("Failed to close app!")
                    }

                    const event = new ComponentEvent("ml-gamereload", this)
                    this.divElement.dispatchEvent(event)
                }
            })

            setContextMenu(event, {
                elements
            })
        } else {
            this.startStream()

            await new Promise(r => window.setTimeout(r, 6000))

            const event = new ComponentEvent("ml-gamereload", this)
            this.divElement.dispatchEvent(event)
        }
    }
    private startStream() {
        let query = new URLSearchParams({
            hostId: this.getHostId(),
            appId: this.getAppId(),
        } as any)

        if (window.matchMedia('(display-mode: standalone)').matches) {
            // If we're in a pwa: open in the current tab
            // If we don't do this we might get a url bar at the top
            window.location.href = buildUrl(`/stream.html?${query}`)
        } else {
            window.open(buildUrl(`/stream.html?${query}`), "_blank")
        }
    }

    private onContextMenu(event: MouseEvent) {
        const elements = []

        elements.push({
            name: "Show Details",
            callback: this.showDetails.bind(this),
        })

        elements.push({
            name: "Open",
            callback: async () => {
                this.startStream()

                const event = new ComponentEvent("ml-gamereload", this)
                this.divElement.dispatchEvent(event)
            }
        })

        setContextMenu(event, {
            elements
        })
    }

    private async showDetails() {
        const app = this.cache

        await showMessage(
            `Title: ${app.title}\n` +
            `Id: ${app.app_id}\n` +
            `HDR Supported: ${app.is_hdr_supported}\n`
        )
    }

    private isActive(): boolean {
        return this.cache.activeApp == this.appId
    }

    addForceReloadListener(listener: GameEventListener) {
        this.divElement.addEventListener("ml-gamereload", listener as any)
    }
    removeForceReloadListener(listener: GameEventListener) {
        this.divElement.removeEventListener("ml-gamereload", listener as any)
    }

    getHostId(): number {
        return this.hostId
    }
    getAppId(): number {
        return this.appId
    }

    mount(parent: HTMLElement): void {
        this.mounted++
        this.updateImage()

        parent.appendChild(this.divElement)
    }
    unmount(parent: HTMLElement): void {

        parent.removeChild(this.divElement)

        this.mounted--
        this.updateImage()
    }
}