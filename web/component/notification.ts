import { Component } from "./index.js"
import { ERROR_IMAGE, INFO_IMAGE, WARN_IMAGE } from "../resources/index.js"
import { ListComponent } from "./list.js"

type NotificationLevel = "error" | "warn" | "info"

const ERROR_REMOVAL_TIME_MS = 10000

const notificationListElement = document.getElementById("notification-list")
const notificationListComponent = new ListComponent<NotificationComponent>([], { listClasses: ["notification-list"], elementLiClasses: ["notification-element"] })
if (notificationListElement) {
    notificationListComponent.mount(notificationListElement)
}

let alertedNotificationListNotFound = false

export function showNotification(message: string, level: NotificationLevel = "error", errorObject?: any) {
    console.error(message, errorObject)

    if (!notificationListElement) {
        if (!alertedNotificationListNotFound) {
            alert("couldn't find the notification element")
            alertedNotificationListNotFound = true
        }
        alert(message)
        return;
    }

    let error: NotificationComponent
    if (level == "error") {
        error = new NotificationComponent(message, ERROR_IMAGE)
    } else if (level == "warn") {
        error = new NotificationComponent(message, WARN_IMAGE)
    } else if (level = "info") {
        error = new NotificationComponent(message, INFO_IMAGE)
    } else {
        error = new NotificationComponent(`Unknown notification level (\"${level}\") for message: ${message}`, ERROR_IMAGE)
    }

    notificationListComponent.append(error)

    setTimeout(() => {
        notificationListComponent.removeValue(error)
    }, ERROR_REMOVAL_TIME_MS)
}

function handleError(event: ErrorEvent) {
    showNotification(`${event.error}`, "error", event)
}
function handleRejection(event: PromiseRejectionEvent) {
    showNotification(`${event.reason}`, "error", event)
}

window.addEventListener("error", handleError)
window.addEventListener("unhandledrejection", handleRejection)

class NotificationComponent implements Component {
    private messageElement: HTMLElement = document.createElement("p")
    private imageElement: HTMLImageElement = document.createElement("img")

    constructor(message: string, image: string) {
        this.messageElement.innerText = message
        this.messageElement.classList.add("notification-message")

        this.imageElement.src = image
        this.imageElement.classList.add("notification-image")
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