import { globalDefaultSettings, getLocalStreamSettings } from "../component/settings_menu.js"

// old doesn't exist anymore and is always replaced with moonlight when loading the settings
import standardUrl from "./standard.css";
import moonlightUrl from "./moonlight.css";

export type PageStyle = "standard" | "old" | "moonlight";

let currentStyle: PageStyle | null = null

const styleMap: Record<PageStyle, LazyStyleModule> = {
    standard: standardUrl,
    old: standardUrl,
    moonlight: moonlightUrl
};

export function setStyle(style: PageStyle) {
    if (currentStyle && currentStyle != style) {
        styleMap[currentStyle].unuse()
    }

    styleMap[style].use()
    currentStyle = style
}

export function getStyle(): PageStyle {
    return currentStyle as PageStyle
}

const settings = getLocalStreamSettings(globalDefaultSettings())

setStyle(settings.pageStyle)
