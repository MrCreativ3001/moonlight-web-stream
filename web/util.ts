
export function globalObject(): any {
    if (typeof self !== 'undefined') {
        return self
    }

    if (typeof window !== 'undefined') {
        return window
    }

    return globalThis;
}

export function download(data: Uint8Array, filename: string, mime: string = "application/octet-stream") {
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
