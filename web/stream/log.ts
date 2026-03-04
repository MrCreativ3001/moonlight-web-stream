import { LogMessageType } from "../api_bindings"

export type LogMessageInfo = {
    type?: LogMessageType
}

export type LogListener = (fullRawText: string, type: LogMessageType | null) => void

export class Logger {

    constructor() { }

    debug(message: string, info?: LogMessageInfo) {
        this.callListeners(message, info?.type)
    }

    private callListeners(message: string, type?: LogMessageType) {
        for (const listener of this.infoListeners) {
            listener(message, type ?? null)
        }
    }

    private infoListeners: Array<LogListener> = []
    addInfoListener(listener: LogListener) {
        this.infoListeners.push(listener)
    }
    removeInfoListener(listener: LogListener) {
        const index = this.infoListeners.indexOf(listener)
        if (index != -1) {
            this.infoListeners.splice(index, 1)
        }
    }
}