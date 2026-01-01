import { defaultSettings, getLocalStreamSettings } from "../component/settings_menu.js"

export type PageStyle = "standard" | "old"

let currentStyle: null | PageStyle = null

let styleLink = document.getElementById("style") as HTMLLinkElement

export function setStyle(style: PageStyle) {
    if (!currentStyle) {
        document.head.appendChild(styleLink)
    }

    currentStyle = style

    const file = `styles/${style}.css`
    if (styleLink.href != file) {
        styleLink.href = file
    }
}
export function getStyle(): PageStyle {
    // Style is set at the bottom of this page so it cannot be null
    return currentStyle as PageStyle
}

const settings = getLocalStreamSettings()
const defaultSettings_ = defaultSettings()

setStyle(settings?.pageStyle ?? defaultSettings_.pageStyle)