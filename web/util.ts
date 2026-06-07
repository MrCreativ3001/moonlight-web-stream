
export function globalObject(): any {
    if (typeof self !== 'undefined') {
        return self
    }

    if (typeof window !== 'undefined') {
        return window
    }

    return globalThis;
}

export function deepEqual(a: unknown, b: unknown): boolean {
    if (Object.is(a, b)) {
        return true;
    }

    if (
        a === null ||
        b === null ||
        typeof a !== "object" ||
        typeof b !== "object"
    ) {
        return false;
    }

    if (Array.isArray(a) !== Array.isArray(b)) {
        return false;
    }

    const keysA = Object.keys(a);
    const keysB = Object.keys(b);

    if (keysA.length !== keysB.length) {
        return false;
    }

    for (const key of keysA) {
        if (!Object.prototype.hasOwnProperty.call(b, key)) {
            return false;
        }

        if (
            !deepEqual(
                (a as Record<string, unknown>)[key],
                (b as Record<string, unknown>)[key]
            )
        ) {
            return false;
        }
    }

    return true;
}

export function download(data: Uint8Array<ArrayBuffer>, filename: string, mime: string = "application/octet-stream") {
    const blob = data instanceof Blob ? data : new Blob([data], { type: mime })
    const url = URL.createObjectURL(blob)

    const a = document.createElement("a")
    a.href = url
    a.download = filename
    document.body.appendChild(a)
    a.click()
    a.remove()

    URL.revokeObjectURL(url)
}

export function numToHex(n: number): string {
    const hex = n.toString(16)
    return hex.length === 1 ? "0" + hex : hex
}
