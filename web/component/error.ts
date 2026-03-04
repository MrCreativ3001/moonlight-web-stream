import { FetchError } from "../api.js"
import { Component } from "../component/index.js"
import { ERROR_IMAGE, WARN_IMAGE } from "../resources/index.js"
import { ListComponent } from "./list.js"

const ERROR_REMOVAL_TIME_MS = 10000

const errorListElement = document.getElementById("error-list")
const errorListComponent = new ListComponent<ErrorComponent>([], { listClasses: ["error-list"], elementLiClasses: ["error-element"] })
if (errorListElement) {
    errorListComponent.mount(errorListElement)
}

let alertedErrorListNotFound = false

export function showErrorPopup(message: string, fatal: boolean = false, errorObject?: any) {
    console.error(message, errorObject)

    if (!errorListElement) {
        if (!alertedErrorListNotFound) {
            alert("couldn't find the error element")
            alertedErrorListNotFound = true
        }
        alert(message)
        return;
    }

    let error
    if (fatal) {
        error = new ErrorComponent(message, ERROR_IMAGE)
    } else {
        error = new ErrorComponent(message, WARN_IMAGE)
    }

    errorListComponent.append(error)

    setTimeout(() => {
        errorListComponent.removeValue(error)
    }, ERROR_REMOVAL_TIME_MS)
}

// TODO: wtf am i doing here? why fatal = that?
function handleError(event: ErrorEvent) {
    const fatal = event instanceof FetchError

    showErrorPopup(`${event.error}`, fatal, event)
}
function handleRejection(event: PromiseRejectionEvent) {
    const fatal = event instanceof FetchError

    showErrorPopup(`${event.reason}`, fatal, event)
}

window.addEventListener("error", handleError)
window.addEventListener("unhandledrejection", handleRejection)

class ErrorComponent implements Component {
    private messageElement: HTMLElement = document.createElement("p")
    private imageElement: HTMLImageElement = document.createElement("img")

    constructor(message: string, image: string) {
        this.messageElement.innerText = message
        this.messageElement.classList.add("error-message")

        this.imageElement.src = image
        this.imageElement.classList.add("error-image")
    }

    mount(parent: Element): void {
        parent.appendChild(this.imageElement)
        parent.appendChild(this.messageElement)
    }
    unmount(parent: Element): void {
        parent.removeChild(this.imageElement)
        parent.removeChild(this.messageElement)
    }
}