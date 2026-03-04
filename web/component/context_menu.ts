import { Component } from "../component/index.js"
import { showErrorPopup } from "./error.js"
import { ListComponent } from "./list.js"

document.addEventListener("click", () => removeContextMenu())

export type ContextMenuElement = {
    name: string,
    callback(event: MouseEvent): void
    classes?: string[]
}

export type ContextMenuInit = {
    elements?: ContextMenuElement[]
}

const contextMenuElement = document.getElementById("context-menu")
const contextMenuList = new ListComponent<ContextMenuElementComponent>([], {
    listClasses: ["context-menu-list"]
})

export function setContextMenu(event: MouseEvent, init?: ContextMenuInit) {
    event.preventDefault()
    event.stopPropagation()

    if (contextMenuElement == null) {
        showErrorPopup("cannot find the context menu element")
        return;
    }

    contextMenuElement.style.setProperty("left", `${event.clientX}px`)
    contextMenuElement.style.setProperty("top", `${event.clientY}px`)

    contextMenuList.clear()

    for (const element of init?.elements ?? []) {
        contextMenuList.append(new ContextMenuElementComponent(element))
    }

    contextMenuList.mount(contextMenuElement)
    contextMenuElement.classList.remove("context-menu-disabled")
}

export function removeContextMenu() {
    if (contextMenuElement == null) {
        showErrorPopup("cannot find the context menu element")
        return;
    }

    contextMenuElement.classList.add("context-menu-disabled")
}

class ContextMenuElementComponent implements Component {
    private nameElement: HTMLElement = document.createElement("p")

    constructor(element: ContextMenuElement) {
        this.nameElement.innerText = element.name

        this.nameElement.classList.add("context-menu-element")

        this.nameElement.addEventListener("click", event => {
            element.callback(event)
        })

        // Also register right click for certain devices which make left click hard: https://github.com/MrCreativ3001/moonlight-web-stream/issues/55
        this.nameElement.addEventListener("contextmenu", event => {
            event.preventDefault()
            removeContextMenu()

            element.callback(event)
        }, { passive: false })

        if (element.classes) {
            this.nameElement.classList.add(...element.classes)
        }
    }

    mount(parent: Element): void {
        parent.appendChild(this.nameElement)
    }
    unmount(parent: Element): void {
        parent.removeChild(this.nameElement)
    }
}
