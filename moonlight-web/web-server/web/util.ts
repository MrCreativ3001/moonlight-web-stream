
export function globalObject(): any {
    if (typeof self !== 'undefined') {
        return self
    }

    if (typeof window !== 'undefined') {
        return window
    }

    return globalThis;
}
