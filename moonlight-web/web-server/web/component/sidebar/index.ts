import { Component } from "../index.js"
import { showErrorPopup } from "../error.js"

export interface Sidebar extends Component {
    extended(): void
    unextend(): void
}

let sidebarMounted = false
let sidebarExtended = false
const sidebarRoot = document.getElementById("sidebar-root")
const sidebarParent = document.getElementById("sidebar-parent")
const sidebarButton = document.getElementById("sidebar-button")

sidebarButton?.addEventListener("click", toggleSidebar)
setSidebarStyle({
    edge: "left"
})

let sidebarComponent: Sidebar | null = null

export type SidebarEdge = "up" | "down" | "left" | "right"
export type SidebarStyle = {
    edge?: SidebarEdge
}

export function setSidebarStyle(style: SidebarStyle) {
    // Default values
    const edge = style.edge ?? "left"

    // Set edge
    sidebarRoot?.classList.remove("sidebar-edge-left", "sidebar-edge-right", "sidebar-edge-up", "sidebar-edge-down")
    sidebarRoot?.classList.add(`sidebar-edge-${edge}`)
}

export function toggleSidebar() {
    setSidebarExtended(!isSidebarExtended())
}
export function setSidebarExtended(extended: boolean) {
    if (extended == sidebarExtended) {
        return
    }

    if (extended) {
        sidebarRoot?.classList.add("sidebar-show")
    } else {
        sidebarRoot?.classList.remove("sidebar-show")
    }
    sidebarExtended = extended
}
export function isSidebarExtended(): boolean {
    return sidebarExtended
}

export function setSidebar(sidebar: Sidebar | null) {
    if (sidebarParent == null) {
        showErrorPopup("failed to get sidebar")
        return
    }

    if (sidebarMounted) {
        // unmount
        sidebarComponent?.unmount(sidebarParent)
        sidebarMounted = false
    }
    if (sidebar) {
        // mount
        sidebarComponent = sidebar
        sidebar?.mount(sidebarParent)
    }
}

export function getSidebarRoot(): HTMLElement | null {
    return sidebarRoot
}