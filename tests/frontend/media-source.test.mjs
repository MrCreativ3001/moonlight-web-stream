import test from 'node:test'
import assert from 'node:assert/strict'
import { sourceLoader, timers } from './source-loader.mjs'

class Events {
    listeners = new Map()
    addEventListener(type, callback) { this.listeners.set(type, callback) }
    removeEventListener(type, callback) { if (this.listeners.get(type) === callback) this.listeners.delete(type) }
    emit(type) { this.listeners.get(type)?.({ type }) }
}

function rig({ open = true } = {}) {
    const clock = timers()
    const calls = [], logs = []
    let now = 0, source, quota = false
    const ranges = [[0, 0.5]]
    const buffered = { get length() { return ranges.length }, start: i => ranges[i][0], end: i => ranges[i][1] }
    const video = { currentTime: 0, paused: false, seeking: false, readyState: 2 }
    class Buffer extends Events {
        updating = false
        buffered = buffered
        appendBuffer(data) {
            calls.push(['append', data])
            if (quota) throw new DOMException('full', 'QuotaExceededError')
            this.updating = true
        }
        remove(start, end) { calls.push(['remove', start, end]); this.updating = true }
        abort() { calls.push(['abort']); this.updating = false }
        complete() { this.updating = false; this.emit('updateend') }
    }
    const buffer = new Buffer()
    class MediaSource extends Events {
        readyState = open ? 'open' : 'closed'
        constructor() { super(); source = this }
        addSourceBuffer() { return buffer }
        removeSourceBuffer(value) { assert.equal(value, buffer); calls.push(['detach']) }
    }
    class Translator {
        initial = true
        submitDecodeUnit(unit) {
            const configure = this.initial ? { codec: 'avc1.42E01E', description: new Uint8Array([1, 2]) } : undefined
            this.initial = false
            return { configure, chunk: new Uint8Array([0, 0, 0, 1, unit.type === 'key' ? 0x65 : 0x41]) }
        }
    }
    const base = {
        implementationName: 'test', getBase: () => null, getMediaElement: () => video,
        setUrl() {}, cleanup() { calls.push(['cleanup']) },
    }
    const load = sourceLoader({ ...clock, MediaSource, ErrorEvent: class {},
        performance: { now: () => now },
        URL: { createObjectURL: () => 'blob:test', revokeObjectURL: url => calls.push(['revoke', url]) },
    }, {
        'web/api_bindings.ts': {},
        'web/stream/video/annex_b_translator.ts': { H264StreamVideoTranslator: Translator, h264NalType: byte => byte & 31 },
    })
    const { MediaSourceDecoder } = load('web/stream/video/media_source_decoder.ts')
    const decoder = new MediaSourceDecoder(base, { debug: (message, info) => logs.push([message, info]) })
    // Most tests focus on append/removal scheduling, without synthesizing SPS/PPS.
    decoder.sourceBuffer = buffer
    buffer.addEventListener('updateend', decoder.onSourceBufferUpdateEnd)
    buffer.addEventListener('error', decoder.onSourceBufferError)
    return { clock, decoder, buffer, source, video, ranges, calls, logs,
        setNow: value => { now = value }, setQuota: value => { quota = value } }
}

const data = () => new Uint8Array([1, 2, 3])

test('history eviction keeps the known keyframe and serializes removal before append', () => {
    const { decoder, video, buffer, calls } = rig()
    decoder.keyframes = [0, 5, 10, 15]
    video.currentTime = 14
    const pending = data()
    decoder.enqueue(pending)
    decoder.tryAppendDecodeUnit()
    assert.deepEqual(calls[0], ['remove', 0, 4.999999])
    assert.equal(decoder.buffers[0].data, pending)
    decoder.tryAppendDecodeUnit()
    assert.equal(calls.length, 1)
    buffer.complete()
    assert.equal(calls[1][0], 'append')
    assert.equal(calls[1][1], pending)
    assert.equal(decoder.buffers.length, 0)
    assert.equal(decoder.pendingBytes, 0)
    assert.deepEqual([...decoder.keyframes], [5, 10, 15])
    decoder.cleanup()
})

test('quota retry retains the identical segment and safely trims played GOPs', () => {
    const { decoder, video, buffer, calls, setQuota } = rig()
    video.currentTime = 14
    decoder.keyframes = [0, 5, 10]
    const pending = data()
    decoder.enqueue(pending)
    decoder.tryAppendDecodeUnit() // Normal five-second retention trims to 5.
    setQuota(true)
    buffer.complete() // Quota pressure permits trimming to 10, still behind playback.
    assert.deepEqual(calls.map(x => x[0]), ['remove', 'append', 'remove'])
    assert.equal(decoder.buffers[0].data, pending)
    assert.equal(decoder.pendingBytes, pending.byteLength)
    assert.equal(decoder.errored, false)
    setQuota(false)
    buffer.complete()
    assert.equal(calls[3][1], pending)
    assert.equal(decoder.pendingBytes, 0)
    assert.equal(decoder.quotaRetries, 0)
    decoder.cleanup()
})

test('unrecoverable quota and asynchronous buffer error stop appends and release the pending queue', () => {
    for (const asynchronous of [false, true]) {
        const { decoder, buffer, calls, logs, setQuota } = rig()
        decoder.enqueue(data())
        if (asynchronous) buffer.emit('error')
        else { setQuota(true); decoder.tryAppendDecodeUnit() }
        assert.equal(decoder.errored, true)
        assert.equal(decoder.buffers.length, 0)
        assert.equal(decoder.pendingBytes, 0)
        assert.ok(logs.some(([, info]) => info.type === 'fatal'))
        const count = calls.length
        decoder.tryAppendDecodeUnit()
        decoder.submitDecodeUnit({ type: 'delta', data: data() })
        assert.equal(calls.length, count)
        decoder.cleanup()
    }
})

test('pending byte and segment bounds apply even while SourceBuffer is updating', () => {
    for (const limit of ['bytes', 'segments']) {
        const { decoder, buffer, calls } = rig()
        buffer.updating = true
        if (limit === 'bytes') decoder.enqueue(new Uint8Array(16 * 1024 * 1024))
        else for (let i = 0; i < 256; i++) decoder.enqueue(data())
        decoder.tryAppendDecodeUnit()
        assert.equal(decoder.errored, false)
        assert.equal(decoder.enqueue(data()), false)
        assert.equal(decoder.errored, true)
        assert.equal(decoder.pendingBytes, 0)
        assert.equal(decoder.buffers.length, 0)
        assert.equal(calls.length, 0)
        decoder.cleanup()
    }
})

test('only appended keyframes enter retention metadata, using the generated MP4 timeline', async () => {
    const { decoder, buffer } = rig()
    decoder.sourceBuffer = null
    await decoder.setup({ width: 1280, height: 720, fps: 60, codec: 'h264' })
    decoder.submitDecodeUnit({ type: 'key', data: data(), timestampMicroseconds: 999999999 })
    assert.equal(decoder.keyframes.length, 0) // Init segment append is still in flight.
    buffer.complete()
    assert.deepEqual([...decoder.keyframes], [0])
    buffer.complete()
    decoder.sequenceNumber = 300
    decoder.submitDecodeUnit({ type: 'key', data: data(), timestampMicroseconds: 999999999 })
    assert.equal(decoder.keyframes[1], 5)
    decoder.cleanup()
})

test('periodic IDR requests provide retention boundaries without changing stream FPS or bitrate', () => {
    const { decoder, setNow } = rig()
    decoder.sequenceNumber = 1
    setNow(4999); assert.equal(decoder.pollRequestIdr(), false)
    setNow(5000); assert.equal(decoder.pollRequestIdr(), true)
    setNow(5001); assert.equal(decoder.pollRequestIdr(), false)
    setNow(10000); assert.equal(decoder.pollRequestIdr(), true)
    decoder.cleanup()
})

test('live-edge recovery seeks inside the last buffered range and throttles repeated corrections', () => {
    const { decoder, video, ranges, setNow } = rig()
    ranges.splice(0, ranges.length, [0, 1], [10, 11])
    decoder.tryAppendDecodeUnit()
    assert.equal(video.currentTime, 10.85)
    video.currentTime = 0
    setNow(1999); decoder.tryAppendDecodeUnit(); assert.equal(video.currentTime, 0)
    setNow(2000); decoder.tryAppendDecodeUnit(); assert.equal(video.currentTime, 10.85)
    decoder.cleanup()
})

test('live-edge recovery leaves normal short buffers, paused, seeking and unready playback alone', () => {
    for (const state of ['short', 'paused', 'seeking', 'unready', 'narrow']) {
        const { decoder, video, ranges } = rig()
        ranges[0] = state === 'short' ? [0, 0.7] : state === 'narrow' ? [10, 10.1] : [0, 10]
        if (state === 'paused') video.paused = true
        if (state === 'seeking') video.seeking = true
        if (state === 'unready') video.readyState = 1
        decoder.tryAppendDecodeUnit()
        assert.equal(video.currentTime, 0)
        decoder.cleanup()
    }
})

test('source-open timeout and cleanup unblock setup and detach event listeners', async () => {
    for (const cancel of ['timeout', 'cleanup', 'close']) {
        const { decoder, source, clock } = rig({ open: false })
        const setup = assert.rejects(decoder.setup({ width: 1280, height: 720, fps: 60, codec: 'h264' }), /did not open/)
        if (cancel === 'timeout') clock.fire(5000)
        if (cancel === 'cleanup') decoder.cleanup()
        if (cancel === 'close') source.emit('sourceclose')
        await setup
        assert.equal(source.listeners.size, 0)
        assert.equal(clock.pending.size, 0)
        decoder.cleanup()
    }
})

test('cleanup aborts in-flight updates, clears listeners and queues, and revokes URL once', () => {
    const { decoder, buffer, calls } = rig()
    decoder.enqueue(data())
    decoder.tryAppendDecodeUnit()
    decoder.enqueue(data())
    decoder.cleanup()
    decoder.cleanup()
    assert.equal(decoder.sourceBuffer, null)
    assert.equal(decoder.pendingBytes, 0)
    assert.equal(buffer.listeners.size, 0)
    assert.deepEqual(calls.map(x => x[0]), ['append', 'abort', 'detach', 'cleanup', 'revoke'])
})

test('unused pipeline does not expire before setup starts the source-open deadline', async () => {
    const { decoder, source, clock } = rig({ open: false })
    assert.equal(clock.pending.size, 0)
    clock.fire(5000)
    const setup = decoder.setup({ width: 1280, height: 720, fps: 60, codec: 'h264' })
    assert.equal(clock.pending.size, 1)
    source.readyState = 'open'
    source.emit('sourceopen')
    await setup
    assert.equal(clock.pending.size, 0)
    decoder.cleanup()
})

test('paused playback keeps random-access metadata bounded', () => {
    const { decoder, video, buffer } = rig()
    video.paused = true
    for (let i = 0; i < 140; i++) {
        decoder.enqueue(data(), i)
        decoder.tryAppendDecodeUnit()
        buffer.complete()
    }
    assert.equal(decoder.keyframes.length, 128)
    assert.equal(decoder.errored, false)
    assert.equal(decoder.pendingBytes, 0)
    decoder.cleanup()
})

test('URL renderer exposes playback for retention and releases its source during cleanup', () => {
    const calls = []
    const video = {
        classList: { add() {} },
        pause() { calls.push('pause') },
        removeAttribute(name) { calls.push(`remove:${name}`) },
        load() { calls.push('load') },
    }
    const load = sourceLoader({ document: { createElement: () => video } }, {
        'web/api_bindings.ts': {},
    })
    const { UrlVideoElementRenderer } = load('web/stream/video/video_element.ts')
    const renderer = new UrlVideoElementRenderer()
    renderer.setUrl('blob:test')
    assert.equal(renderer.getMediaElement(), video)
    renderer.cleanup()
    assert.deepEqual(calls, ['pause', 'remove:src', 'load'])
})

test('dense all-intra streams retain old enough boundaries to keep trimming history', () => {
    const { decoder, video, buffer, calls } = rig()
    video.paused = true // Disable live-edge correction while advancing playback manually.
    for (let i = 0; i < 750; i++) {
        video.currentTime = i / 60
        decoder.enqueue(data(), i / 60)
        decoder.tryAppendDecodeUnit()
        while (buffer.updating) buffer.complete()
    }
    const removals = calls.filter(([operation]) => operation === 'remove')
    assert.ok(removals.length > 5)
    assert.ok(removals.at(-1)[2] >= 6.9)
    assert.ok(decoder.keyframes.length < 10)
    assert.equal(decoder.errored, false)
    decoder.cleanup()
})
