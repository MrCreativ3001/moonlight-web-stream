import type { PipeInfo, PipeInfoStatic } from "./index"

// Optional capability checks must not hold up every other pipeline.
export function boundedProbe<T>(run: () => T | PromiseLike<T>, timeout: number, fallback: T): Promise<T> {
    return new Promise(resolve => {
        let done = false
        const finish = (value: T) => {
            if (done) return
            done = true
            clearTimeout(timer)
            resolve(value)
        }
        const timer = setTimeout(() => finish(fallback), timeout)
        Promise.resolve().then(run).then(finish, () => finish(fallback))
    })
}

export async function queryPipeInfo(pipe: PipeInfoStatic): Promise<PipeInfo> {
    const unsupported: PipeInfo = { environmentSupported: false }
    const info = await boundedProbe(() => pipe.getInfo(), 3000, unsupported)
    return info && typeof info.environmentSupported == "boolean" ? info : unsupported
}
