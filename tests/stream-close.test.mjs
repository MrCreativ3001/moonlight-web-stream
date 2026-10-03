import assert from "node:assert/strict"
import { readFileSync } from "node:fs"
import test from "node:test"
import vm from "node:vm"
import ts from "typescript"

// Run the actual stream lifecycle methods with transports isolated from media,
// browser permissions and a real game server.
function load(file, globals = {}) {
    const exports = {}
    const source = ts.transpileModule(readFileSync(new URL(`../${file}`, import.meta.url), "utf8"), {
        compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 }
    }).outputText
    vm.runInNewContext(source, {
        exports,
        require: () => ({
            ControlPacket_Tags: { ServerTermination: "ServerTermination", HdrMode: "HdrMode" },
            TerminationReason_Tags: { Long: "Long", Short: "Short" },
            buildUrl: path => `/moonlight${path}`,
        }),
        CustomEvent, EventTarget, console, ...globals,
    }, { filename: file })
    return exports
}

const { Stream } = load("web/stream/index.ts")
function makeStream(transportType = "auto") {
    const stream = Object.create(Stream.prototype)
    const events = [], logs = []
    let closes = 0, fallbacks = 0
    Object.assign(stream, {
        settings: { dataTransport: transportType }, permissions: {},
        stopped: false, stopPromise: null,
        eventTarget: new EventTarget(),
        transport: { close: async () => { closes++ } },
        input: { onReceivePacket: () => {} },
        debugLog: (message, additional) => logs.push({ message, additional }),
        tryWebSocketTransport: async () => { fallbacks++; return "failed" },
    })
    stream.addInfoListener(event => events.push(event.detail.type))
    return { stream, events, logs, closes: () => closes, fallbacks: () => fallbacks }
}
const termination = (code, tag = "Long") => ({
    tag: "ServerTermination", inner: { reason: { tag, inner: [code] } }
})

for (const transport of ["auto", "webrtc", "websocket"]) {
    test(`${transport}: graceful host close exits once without failure or fallback`, async () => {
        const state = makeStream(transport)
        const end = async () => {
            state.stream.onReceivePacket(termination(0x80030023))
            state.stream.onReceivePacket(termination(0x80030023))
            return "failednoconnect"
        }
        state.stream.tryWebRTCTransport = end
        if (transport == "websocket") state.stream.tryWebSocketTransport = end
        await state.stream.startConnection()
        await state.stream.stop() // beforeunload must not close the session twice
        assert.deepEqual(state.events, ["streamEnded"])
        assert.equal(state.closes(), 1)
        assert.equal(state.fallbacks(), 0)
        assert.equal(state.logs.filter(x => x.additional?.type?.startsWith("fatal")).length, 0)
    })
}

test("an encoder termination remains visible and does not close the tab", async () => {
    const state = makeStream()
    state.stream.tryWebRTCTransport = async () => {
        state.stream.onReceivePacket(termination(0x800e9403))
        return "failednoconnect"
    }
    await state.stream.startConnection()
    assert.deepEqual(state.events, [])
    assert.equal(state.closes(), 1)
    assert.equal(state.fallbacks(), 0)
    assert.match(state.logs.find(x => x.additional?.type == "fatalDescription").message, /800e9403/)
})

test("a WAN failure keeps the viewer open and reports a lost connection", async () => {
    const state = makeStream()
    state.stream.tryWebRTCTransport = async () => "failed"
    await state.stream.startConnection()
    assert.deepEqual(state.events, [])
    assert.equal(state.fallbacks(), 0)
    assert.match(state.logs.find(x => x.additional?.type == "fatal").message, /connection was lost/)
})

test("initial WebRTC failure still falls back to WebSocket", async () => {
    const state = makeStream()
    state.stream.tryWebRTCTransport = async () => "failednoconnect"
    await state.stream.startConnection()
    assert.equal(state.fallbacks(), 1)
    assert.deepEqual(state.events, [])
})

test("manual exit during initial connection suppresses fallback and errors", async () => {
    const state = makeStream()
    state.stream.tryWebRTCTransport = async () => {
        await state.stream.stop()
        return "failednoconnect"
    }
    await state.stream.startConnection()
    assert.equal(state.fallbacks(), 0)
    assert.equal(state.logs.filter(x => x.additional?.type?.startsWith("fatal")).length, 0)
})

for (const mode of ["popup", "direct-tab", "standalone"]) {
    test(`${mode}: leaves the viewer through the correct browser action`, () => {
        const actions = [], tasks = []
        const window = {
            closed: false,
            matchMedia: () => ({ matches: mode == "standalone" }),
            close() { actions.push("close"); this.closed = mode == "popup" },
            setTimeout(callback) { tasks.push(callback) },
            location: { replace: url => actions.push(url) },
        }
        const history = { length: 2, back: () => actions.push("back") }
        load("web/stream/exit.ts", { window, history }).exitStreamPage()
        tasks.forEach(callback => callback())
        assert.deepEqual(actions, mode == "standalone" ? ["back"]
            : mode == "popup" ? ["close"] : ["close", "/moonlight/"])
    })
}
