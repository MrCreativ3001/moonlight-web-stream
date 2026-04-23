import { en, type Translations } from "./locales/en.js"
import { zhCN } from "./locales/zh-CN.js"
import { ptBR } from "./locales/pt-BR.js"
import { frFr } from "./locales/fr-FR.js"
import { koKR } from "./locales/ko-KR.js"

export type Language = "en" | "zh-CN" | "pt-BR" | "fr-FR" | "ko-KR"

// Translations is defined in locales/en.ts (the canonical locale).
// Adding a new locale requires: (1) create web/locales/<code>.ts implementing
// Translations, (2) add it to the Language union and the locales map below.
export type { Translations }

const locales: Record<Language, Translations> = {
    "en": en,
    "zh-CN": zhCN,
    "pt-BR": ptBR,
    "fr-FR": frFr,
    "ko-KR": koKR,
}

export function getTranslations(language: Language): Translations {
    return locales[language]
}

export function normalizeLanguage(language: unknown): Language {
    if (language === "zh" || language === "zh-CN" || language === "zh_CN") {
        return "zh-CN"
    }
    if (language === "pt" || language === "pt-BR" || language === "pt_BR") {
        return "pt-BR"
    }
    if (language === "ko" || language === "ko-KR" || language === "ko_KR") {
        return "ko-KR"
    }
    return "en"
}

function getStoredSettings(): Record<string, unknown> | null {
    try {
        const raw = localStorage.getItem("mlSettings")
        return raw ? JSON.parse(raw) : null
    } catch {
        return null
    }
}

export function getCurrentLanguage(): Language {
    return normalizeLanguage(getStoredSettings()?.language)
}

export function hasStoredLanguage(): boolean {
    return getStoredSettings()?.language != null
}

export function adoptRoleDefaultLanguage(roleDefaultSettings: { language?: unknown } | null | undefined): boolean {
    if (hasStoredLanguage()) {
        return false
    }

    const roleLanguage = normalizeLanguage(roleDefaultSettings?.language)
    if (roleLanguage === getCurrentLanguage()) {
        return false
    }

    try {
        const settings = getStoredSettings() ?? {}
        settings.language = roleLanguage
        localStorage.setItem("mlSettings", JSON.stringify(settings))
        return true
    } catch {
        localStorage.setItem("mlSettings", JSON.stringify({ language: roleLanguage }))
        return true
    }
}

export function getLanguageOptions(): Array<{ value: Language, name: string }> {
    return [
        { value: "en", name: "English" },
        { value: "zh-CN", name: "中文" },
        { value: "pt-BR", name: "Português (Brasil)" },
        { value: "fr-FR", name: "Français" },
        { value: "ko-KR", name: "한국어" },
    ]
}
