// -- Polyfill

class PolyfillAbortController {
    constructor() {
        this.signal = new PolyfillAbortSignal()
    }

    signal: PolyfillAbortSignal

    abort(reason?: any) {
        if (this.signal.aborted) {
            return
        }

        this.signal.aborted = true
        this.signal.reason = reason

        this.signal.dispatchEvent(new Event("abort"))
    }
}
class PolyfillAbortSignal extends EventTarget {
    static timeout(timeout: number): AbortSignal {
        if (timeout < 0 || timeout > Number.MAX_SAFE_INTEGER) {
            throw `Illegal argument for AbortSignal.timeout: timeout must be in range 0 to Number.MAX_SAFE_INTEGER`
        }

        const controller = new AbortController()

        setTimeout(controller.abort.bind(controller), timeout)

        return controller.signal
    }

    constructor() {
        super()
    }

    aborted: boolean = false
    reason: any

    throwIfAborted() {
        if (this.aborted) {
            throw this.reason
        }
    }
}

let realFetch: typeof fetch
function polyfillFetch(input: RequestInfo | URL, init?: RequestInit): Promise<Response> {
    return new Promise((resolve, reject) => {
        if (init && "signal" in init && init.signal) {
            const signal = init.signal
            init.signal = undefined

            signal.addEventListener("abort", () => {
                reject(signal.reason)
            })
        }

        realFetch(input, init)
            .then(resolve)
            .catch(reject)
    })
}

let realAddEventListener: typeof addEventListener
let realRemoveEventListener: typeof removeEventListener
function polyfillAddEventListener(this: EventTarget, type: string, listener: EventListenerOrEventListenerObject, options?: boolean | AddEventListenerOptions) {
    if (options && typeof options == "object" && "signal" in options && options.signal) {
        const signal = options.signal
        options.signal = undefined

        const removeListener = () => {
            realRemoveEventListener.call(this, type, listener)
            signal.removeEventListener("abort", removeListener)
        }
        signal.addEventListener("abort", removeListener)
    }

    realAddEventListener.call(this, type, listener, options)
}

// -- Apply Polyfill if required

function fillAbortController(global: any) {
    if ("AbortController" in global) {
        return
    }
    console.info("using AbortController polyfill")

    global["AbortController"] = PolyfillAbortController;
    global["AbortSignal"] = PolyfillAbortSignal;

    // Overwrite fetch
    realFetch = global["fetch"]

    global["fetch"] = polyfillFetch

    // Overwrite EventTarget.addEventListener
    const EventTargetPrototype = global["EventTarget"].prototype

    realAddEventListener = EventTargetPrototype["addEventListener"]
    realRemoveEventListener = EventTargetPrototype["removeEventListener"]

    EventTargetPrototype["addEventListener"] = polyfillAddEventListener
}
function fillTimeout(global: any) {
    const AbortSignal = global["AbortSignal"]
    if ("timeout" in AbortSignal) {
        return
    }
    console.info("using AbortSignal.timeout polyfill")

    AbortSignal["timeout"] = PolyfillAbortSignal.timeout
}

const globalObj = globalThis ?? (typeof window !== 'undefined' ? window : undefined)
fillAbortController(globalObj)
fillTimeout(globalObj)