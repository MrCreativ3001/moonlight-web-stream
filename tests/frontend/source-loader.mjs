import { readFileSync, existsSync } from 'node:fs'
import path from 'node:path'
import vm from 'node:vm'
import ts from 'typescript'

// Run tracked TypeScript directly. No generated bindings or release bundle is
// needed; imports supplied in mocks are browser/native or pipeline boundaries.
export function sourceLoader(globals = {}, mocks = {}) {
    const root = path.resolve('.')
    const context = vm.createContext({ console, Uint8Array, ArrayBuffer, DataView, TextEncoder,
        TextDecoder, DOMException, URL, setTimeout, clearTimeout, performance, ...globals })
    const cache = new Map()
    function load(file) {
        const absolute = path.resolve(root, file)
        const relative = path.relative(root, absolute).replaceAll('\\', '/')
        if (relative in mocks) return mocks[relative]
        if (cache.has(absolute)) return cache.get(absolute).exports
        const module = { exports: {} }
        cache.set(absolute, module)
        const source = readFileSync(absolute, 'utf8').replaceAll('import.meta.url', JSON.stringify(new URL(`file://${absolute}`).href))
        const code = ts.transpileModule(source, { compilerOptions: {
            target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS,
        }, fileName: absolute }).outputText
        const require = specifier => {
            let target = path.resolve(path.dirname(absolute), specifier)
            if (!path.extname(target)) target += existsSync(`${target}.ts`) || `${path.relative(root, target)}.ts` in mocks ? '.ts' : '/index.ts'
            return load(target)
        }
        vm.runInContext(`(function(require, module, exports) {${code}\n})`, context, { filename: absolute })(require, module, module.exports)
        return module.exports
    }
    return load
}

export function timers() {
    let serial = 0
    const pending = new Map()
    return {
        pending,
        setTimeout(fn, ms) { const id = ++serial; pending.set(id, { fn, ms }); return id },
        clearTimeout(id) { pending.delete(id) },
        fire(ms) {
            for (const [id, timer] of [...pending]) {
                if (timer.ms === ms && pending.has(id)) { pending.delete(id); timer.fn() }
            }
        },
        async flush() { for (let i = 0; i < 20; i++) await Promise.resolve() },
    }
}
