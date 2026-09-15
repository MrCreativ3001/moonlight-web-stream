import { DetailedHost, UndetailedHost } from "../../api_bindings"
import { Api, apiDeleteHost, apiGetHost, isDetailedHost, apiPostPair, apiWakeUp, apiPatchHost } from "../../api"
import { Component, ComponentEvent } from "../index"
import { getCurrentLanguage, getTranslations } from "../../i18n"
import { setContextMenu } from "../context_menu"
import { showNotification } from "../notification"
import { showMessage } from "../modal/index"
import { HOST_IMAGE, HOST_OVERLAY_LOCK, HOST_OVERLAY_NONE, HOST_OVERLAY_OFFLINE } from "../../resources/index"

export type HostEventListener = (event: ComponentEvent<Host>) => void

export class Host implements Component {
    private api: Api

    private hostId: number
    private cache: UndetailedHost | DetailedHost | null = null
    // True once a fresh server_info response (or confirmed offline result) has been received.
    // While false, server_state == null means "still checking" rather than "offline".
    private stateKnown = false

    private divElement: HTMLDivElement = document.createElement("div")

    private imageElement: HTMLImageElement = document.createElement("img")
    private imageOverlayElement: HTMLImageElement = document.createElement("img")
    private nameElement: HTMLElement = document.createElement("p")

    constructor(api: Api, hostId: number, host: UndetailedHost | DetailedHost | null, stateKnown: boolean = false) {
        this.api = api

        this.hostId = hostId
        this.cache = host
        this.stateKnown = stateKnown

        // Configure image
        this.imageElement.classList.add("host-image")
        this.imageElement.src = HOST_IMAGE

        // Configure image overlay
        this.imageOverlayElement.classList.add("host-image-overlay")

        // Configure name
        this.nameElement.classList.add("host-name")

        // Append elements
        this.divElement.appendChild(this.imageElement)
        this.divElement.appendChild(this.imageOverlayElement)
        this.divElement.appendChild(this.nameElement)

        this.divElement.addEventListener("click", this.onClick.bind(this))
        this.divElement.addEventListener("contextmenu", this.onContextMenu.bind(this))

        // Update cache
        if (host != null) {
            this.updateCache(host)

        } else {
            this.forceFetch()
        }

        // Render the initial overlay (unknown state shows no offline badge)
        this.updateOverlay()
    }

    async forceFetch() {
        const newCache = await apiGetHost(this.api, {
            host_id: this.hostId,
        })

        this.updateCache(newCache, true)
    }
    async getCurrentGame(): Promise<number | null> {
        await this.forceFetch()

        if (this.cache && isDetailedHost(this.cache) && this.cache.current_game != 0) {
            return this.cache.current_game
        } else {
            return null
        }
    }

    private async onClick(event: MouseEvent) {
        if (this.cache?.server_state == null) {
            this.onContextMenu(event)
        } else if (this.cache?.paired == "Paired") {
            this.divElement.dispatchEvent(new ComponentEvent("ml-hostopen", this))
        } else {
            await this.pair()
        }
    }

    private onContextMenu(event: MouseEvent) {
        const i = getTranslations(getCurrentLanguage()).host
        const elements = []

        if (this.cache?.server_state != null) {
            elements.push({
                name: i.showDetails,
                callback: this.showDetails.bind(this),
            })

            elements.push({
                name: i.open,
                callback: this.onClick.bind(this)
            })
        } else if (this.cache?.paired == "Paired") {
            elements.push({
                name: i.sendWakeUpPacket,
                callback: this.wakeUp.bind(this)
            })
        }

        elements.push({
            name: i.reload,
            callback: async () => this.forceFetch()
        })

        if (this.cache?.server_state != null && this.cache?.paired == "NotPaired") {
            elements.push({
                name: i.pair,
                callback: this.pair.bind(this)
            })
        }

        elements.push({
            name: i.removeHost,
            callback: this.remove.bind(this)
        })

        setContextMenu(event, {
            elements
        })
    }

    private async showDetails() {
        const i = getTranslations(getCurrentLanguage()).host
        let host = this.cache;
        if (!host || !isDetailedHost(host)) {
            host = await apiGetHost(this.api, {
                host_id: this.hostId,
            })
        }
        if (!host || !isDetailedHost(host)) {
            showNotification(i.failedToGetDetails(this.hostId))
            return;
        }
        this.updateCache(host, true)

        await showMessage(i.details(host))
    }

    addHostRemoveListener(listener: HostEventListener, options?: EventListenerOptions) {
        this.divElement.addEventListener("ml-hostremove", listener as any, options)
    }
    removeHostRemoveListener(listener: HostEventListener, options?: EventListenerOptions) {
        this.divElement.removeEventListener("ml-hostremove", listener as any, options)
    }

    addHostOpenListener(listener: HostEventListener, options?: EventListenerOptions) {
        this.divElement.addEventListener("ml-hostopen", listener as any, options)
    }
    removeHostOpenListener(listener: HostEventListener, options?: EventListenerOptions) {
        this.divElement.removeEventListener("ml-hostopen", listener as any, options)
    }

    private async remove() {
        await apiDeleteHost(this.api, {
            host_id: this.getHostId()
        })

        this.divElement.dispatchEvent(new ComponentEvent("ml-hostremove", this))
    }
    private async wakeUp() {
        const i = getTranslations(getCurrentLanguage()).host
        await apiWakeUp(this.api, {
            host_id: this.getHostId()
        })

        await showMessage(i.wakeUpSent)
    }
    private async pair() {
        const i = getTranslations(getCurrentLanguage()).host
        if (this.cache?.paired == "Paired") {
            await this.forceFetch()

            if (this.cache?.paired == "Paired") {
                showMessage(i.alreadyPaired)
                return;
            }
        }

        const responseStream = await apiPostPair(this.api, {
            host_id: this.getHostId()
        })

        if (typeof responseStream.response == "string") {
            throw `failed to pair (stage 1): ${responseStream.response}`
        }

        const messageAbort = new AbortController()
        showMessage(i.pairPrompt(this.getCache()?.name ?? "", responseStream.response.Pin), { signal: messageAbort.signal })

        const resultResponse = await responseStream.next()
        messageAbort.abort()

        if (!resultResponse) {
            throw "missing stage 2 of pairing"
        } else if (typeof resultResponse == "string") {
            throw `failed to pair (stage 2): ${resultResponse}`
        }

        this.updateCache(resultResponse.Paired, true)
    }

    getHostId(): number {
        return this.hostId
    }

    getCache(): DetailedHost | UndetailedHost | null {
        return this.cache
    }

    updateCache(host: UndetailedHost | DetailedHost, stateKnown?: boolean) {
        const i = getTranslations(getCurrentLanguage()).host
        if (this.getHostId() != host.host_id) {
            showNotification(i.overwriteMismatch(this.getHostId(), host.host_id))
            return
        }

        if (stateKnown !== undefined) {
            this.stateKnown = stateKnown
        }

        if (this.cache == null) {
            this.cache = host
        } else {
            // if server_state == null it means this host is offline
            // -> updating cache means setting it to offline
            if (this.cache.server_state != null) {
                Object.assign(this.cache, host)
            } else {
                this.cache = host
            }
        }

        // Update Elements
        this.nameElement.innerText = this.cache.name
        this.updateOverlay()
    }

    private updateOverlay() {
        if (this.cache == null || (this.cache.server_state == null && this.stateKnown)) {
            this.imageOverlayElement.src = HOST_OVERLAY_OFFLINE
        } else if (this.cache.server_state == null) {
            // State is still being checked - show no overlay instead of a stale offline badge
            this.imageOverlayElement.src = HOST_OVERLAY_NONE
        } else if (this.cache.paired != "Paired") {
            this.imageOverlayElement.src = HOST_OVERLAY_LOCK
        } else {
            this.imageOverlayElement.src = HOST_OVERLAY_NONE
        }
    }

    mount(parent: HTMLElement): void {
        parent.appendChild(this.divElement)
    }
    unmount(parent: HTMLElement): void {
        parent.removeChild(this.divElement)
    }
}
