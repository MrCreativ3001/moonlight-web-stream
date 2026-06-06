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
    if (!message.trim()) {
        console.debug("Suppressed empty notification", errorObject)
        return
    }

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
    const message = errorMessage(event.error, event.message)
    if (!message) {
        console.debug("Suppressed empty error event", event)
        return
    }

    showNotification(message, "error", event)
}
function handleRejection(event: PromiseRejectionEvent) {
    const message = errorMessage(event.reason)
    if (!message) {
        console.debug("Suppressed empty promise rejection", event)
        return
    }

    showNotification(message, "error", event)
}

function errorMessage(value: unknown, fallback?: string): string | null {
    if (value instanceof Error && value.message.trim()) {
        return value.message
    }
    if (typeof value == "string" && value.trim()) {
        return value
    }
    if (value != null) {
        const message = `${value}`
        if (message.trim() && message != "[object Object]") {
            return message
        }
    }
    if (fallback?.trim()) {
        return fallback
    }
    return null
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
