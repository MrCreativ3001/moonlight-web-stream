import "./config.js"

export function buildUrl(path: string): string {
    return `${window.location.origin}${window.__CONFIG_JS__.path_prefix ?? ""}${path}`
}
