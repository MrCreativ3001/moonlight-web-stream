import { Pipe } from "./index.js";

export interface DataPipe extends Pipe {
    submitPacket(buffer: ArrayBuffer): void
}

export function addPipePassthrough(pipe: Pipe, overwrite?: Array<string>) {
    const pipeAny = pipe as any
    const passthrough = (name: string, force: boolean) => {
        if (name in pipeAny && !force) {
            return
        }
        pipeAny[name] = function () {
            const base = pipe.getBase() as any
            if (base) {
                return base[name].apply(base, arguments)
            }
        }
    }

    passthrough("setup", false)
    passthrough("cleanup", false)
    passthrough("pollRequestIdr", false)
    passthrough("getStreamRect", false)
    passthrough("onUserInteraction", false)
    passthrough("mount", false)
    passthrough("unmount", false)

    if (overwrite) {
        for (const overwriteFn of overwrite) {
            passthrough(overwriteFn, true)
        }
    }
}