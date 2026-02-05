import { defaultSettings, getLocalStreamSettings } from "../component/settings_menu.js"

export type PageStyle = "standard" | "old" | "moonlight"

let currentStyle: PageStyle | null = null
let styleLink = document.getElementById("style") as HTMLLinkElement

export function setStyle(style: PageStyle) {
    if (!currentStyle) {
        document.head.appendChild(styleLink)
    }

    currentStyle = style

    const file = `${style}.css`
    if (!styleLink.href.endsWith(file)) {
        styleLink.href = `styles/${file}`
    }
}

export function getStyle(): PageStyle {
    return currentStyle as PageStyle
}

const settings = getLocalStreamSettings()
const defaultSettings_ = defaultSettings()

setStyle(settings?.pageStyle ?? defaultSettings_.pageStyle)
