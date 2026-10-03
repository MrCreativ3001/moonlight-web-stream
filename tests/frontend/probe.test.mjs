import test from 'node:test'
import assert from 'node:assert/strict'
import { sourceLoader, timers } from './source-loader.mjs'

const probeFile = 'web/stream/pipeline/probe.ts'

test('optional probes isolate synchronous throws, rejections, hangs and late results', async () => {
    const clock = timers()
    const { queryPipeInfo } = sourceLoader(clock)(probeFile)
    let finish
    const results = Promise.all([
        { getInfo() { throw Error('failed') } },
        { getInfo: () => Promise.reject(Error('failed')) },
        { getInfo: () => new Promise(resolve => { finish = resolve }) },
        { getInfo: () => ({ environmentSupported: true, supportedVideoCodecs: { h264: true } }) },
        { getInfo: () => null },
    ].map(queryPipeInfo))
    await clock.flush()
    clock.fire(3000)
    const info = await results
    assert.deepEqual(info.map(x => x.environmentSupported), [false, false, false, true, false])
    finish({ environmentSupported: true })
    await clock.flush()
    assert.equal(info[2].environmentSupported, false)
    assert.equal(info[3].supportedVideoCodecs.h264, true)
    assert.equal(clock.pending.size, 0)
})

function workerRig(mode) {
    const clock = timers()
    const worker = {
        stopped: 0,
        terminate() { this.stopped++ },
        postMessage(value) {
            this.sent = value
            if (mode === 'post') throw Error('post failed')
            if (mode === 'success') this.onmessage({ data: { checkSupport: { environmentSupported: true } } })
            if (mode === 'error' || mode === 'messageerror') this[`on${mode}`]({ preventDefault() {} })
            if (mode === 'invalid') this.onmessage({ data: { checkSupport: {} } })
        },
    }
    function Worker() { if (mode === 'constructor') throw Error('constructor failed'); return worker }
    const load = sourceLoader({ ...clock, Worker }, {
        'web/stream/pipeline/index.ts': {},
        'web/stream/video/offscreen_canvas.ts': { OffscreenCanvasRenderer: class {} },
    })
    const { workerPipe } = load('web/stream/pipeline/worker_pipe.ts')
    return { clock, worker, Pipe: workerPipe('TestWorker', { pipes: [] }) }
}

for (const mode of ['success', 'constructor', 'post', 'error', 'messageerror', 'invalid', 'timeout']) {
    test(`worker probe ${mode} settles and releases handlers, worker and timer`, async () => {
        const { clock, worker, Pipe } = workerRig(mode)
        const result = Pipe.getInfo()
        if (mode === 'timeout') clock.fire(2500)
        assert.equal((await result).environmentSupported, mode === 'success')
        assert.equal(worker.stopped, mode === 'constructor' ? 0 : 1)
        if (mode !== 'constructor') {
            assert.equal(worker.onmessage, null)
            assert.equal(worker.onerror, null)
            assert.equal(worker.onmessageerror, null)
        }
        assert.equal(clock.pending.size, 0)
    })
}

test('worker startup logs and malformed unrelated messages do not throw or settle the probe', async () => {
    const { clock, worker, Pipe } = workerRig('manual')
    const result = Pipe.getInfo()
    const callback = worker.onmessage
    for (const data of [null, 'unrelated', { log: 'startup' }]) callback({ data })
    callback({ data: { checkSupport: { environmentSupported: true } } })
    assert.equal((await result).environmentSupported, true)
    callback({ data: { checkSupport: { environmentSupported: false } } })
    assert.equal((await result).environmentSupported, true)
    assert.equal(worker.stopped, 1)
    assert.equal(clock.pending.size, 0)
})

test('individual rejected and hung codecs preserve supported H264 in either config format', async () => {
    const clock = timers()
    const VideoDecoder = {
        isConfigSupported(config) {
            if (config.codec === 'avc1.42E01E') return Promise.resolve({ supported: true, config })
            if (config.codec.startsWith('av01.')) return new Promise(() => {})
            return Promise.reject(Error('codec failed'))
        },
    }
    const load = sourceLoader({ ...clock, VideoDecoder }, {
        'web/api_bindings.ts': {},
    })
    const { VideoDecoderPipe } = load('web/stream/video/video_decoder_pipe.ts')
    const result = VideoDecoderPipe.getInfo()
    await clock.flush()
    clock.fire(1000)
    const info = await result
    assert.equal(info.environmentSupported, true)
    assert.equal(info.supportedVideoCodecs.h264, true)
    assert.equal(info.supportedVideoCodecs.av1Main8, false)
    assert.equal(info.supportedVideoCodecs.h265, false)
    assert.equal(clock.pending.size, 0)
})

test('actual gather loop caches results and tolerates failed pipes without suppressing OpenH264', async () => {
    const { readFileSync, existsSync } = await import('node:fs')
    const path = await import('node:path')
    const clock = timers()
    const file = 'web/stream/pipeline/index.ts'
    const source = readFileSync(file, 'utf8')
    const mocks = {}
    let calls = 0
    // Replace leaf pipeline classes, but exercise the real gather/cache logic.
    for (const match of source.matchAll(/import \{([^}]+)\} from "([^"]+)"/g)) {
        if (match[2] === './probe') continue
        let target = path.resolve(path.dirname(file), match[2])
        target += existsSync(`${target}.ts`) ? '.ts' : '/index.ts'
        mocks[path.relative('.', target)] = Object.fromEntries(match[1].split(',').map(value => {
            const name = value.trim()
            return [name, class {
                static pipeName = name
                static getInfo() {
                    calls++
                    if (name === 'VideoDecoderPipe') return new Promise(() => {})
                    if (name === 'CanvasFrameDrawPipe') throw Error('failed')
                    return Promise.resolve({ environmentSupported: true, supportedVideoCodecs: { h264: true } })
                }
            }]
        }))
    }
    const { gatherPipeInfo, pipes } = sourceLoader(clock, mocks)(file)
    const first = gatherPipeInfo()
    assert.equal(gatherPipeInfo(), first)
    await clock.flush()
    clock.fire(3000)
    const result = await first
    const list = pipes()
    assert.equal(result.size, list.length)
    assert.equal(result.get(list.find(pipe => pipe.pipeName === 'VideoDecoderPipe')).environmentSupported, false)
    assert.equal(result.get(list.find(pipe => pipe.pipeName === 'CanvasFrameDrawPipe')).environmentSupported, false)
    assert.equal(result.get(list.find(pipe => pipe.pipeName === 'OpenH264DecoderPipe')).environmentSupported, true)
    assert.equal(result.get(list.find(pipe => pipe.pipeName === 'MediaSourceDecoder')).supportedVideoCodecs.h264, true)
    assert.equal(calls, list.length)
    assert.equal(clock.pending.size, 0)
})
