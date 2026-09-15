import { getCurrentLanguage, getTranslations } from "../../i18n"
import { FormModal } from "./form"
import { showModal } from "./index"

export type CustomResolution = { width: number, height: number }

export async function showCustomResolutionPrompt(current?: CustomResolution): Promise<CustomResolution | null> {
    return await showModal(new CustomResolutionModal(current))
}

export async function showCustomFpsPrompt(current?: number): Promise<number | null> {
    return await showModal(new CustomFpsModal(current))
}

function createNumberInput(placeholder: string, value?: number): HTMLInputElement {
    const input = document.createElement("input")
    input.type = "number"
    input.inputMode = "numeric"
    input.min = "1"
    input.step = "1"
    input.required = true
    input.placeholder = placeholder
    if (value != null && Number.isFinite(value)) {
        input.defaultValue = value.toString()
    }
    return input
}

function parsePositiveInt(value: string): number | null {
    const parsed = Number(value)
    return Number.isInteger(parsed) && parsed > 0 ? parsed : null
}

class CustomResolutionModal extends FormModal<CustomResolution> {
    private message: HTMLParagraphElement = document.createElement("p")
    private prompt: HTMLParagraphElement = document.createElement("p")
    private row: HTMLDivElement = document.createElement("div")
    private widthInput: HTMLInputElement
    private heightInput: HTMLInputElement

    constructor(current?: CustomResolution) {
        super()
        const i = getTranslations(getCurrentLanguage()).settings

        this.message.classList.add("custom-value-message")
        this.message.innerText = i.customResolutionWarning

        this.prompt.classList.add("custom-value-prompt")
        this.prompt.innerText = i.customResolutionPrompt

        this.row.classList.add("custom-resolution-row")
        this.widthInput = createNumberInput(i.videoWidth, current?.width)
        this.heightInput = createNumberInput(i.videoHeight, current?.height)

        const separator = document.createElement("span")
        separator.classList.add("custom-resolution-separator")
        separator.innerText = "x"

        this.row.appendChild(this.widthInput)
        this.row.appendChild(separator)
        this.row.appendChild(this.heightInput)
    }

    reset(): void {
        this.widthInput.value = this.widthInput.defaultValue
        this.heightInput.value = this.heightInput.defaultValue
    }
    submit(): CustomResolution | null {
        const width = parsePositiveInt(this.widthInput.value)
        const height = parsePositiveInt(this.heightInput.value)
        if (width == null || height == null) {
            return null
        }
        return { width, height }
    }

    mountForm(form: HTMLFormElement): void {
        form.classList.add("custom-value-form")
        form.appendChild(this.message)
        form.appendChild(this.prompt)
        form.appendChild(this.row)
    }
}

class CustomFpsModal extends FormModal<number> {
    private message: HTMLParagraphElement = document.createElement("p")
    private prompt: HTMLParagraphElement = document.createElement("p")
    private row: HTMLDivElement = document.createElement("div")
    private fpsInput: HTMLInputElement

    constructor(current?: number) {
        super()
        const i = getTranslations(getCurrentLanguage()).settings

        this.message.classList.add("custom-value-message")
        this.message.innerText = i.customFpsWarning

        this.prompt.classList.add("custom-value-prompt")
        this.prompt.innerText = i.customFpsPrompt

        this.row.classList.add("custom-resolution-row")
        this.fpsInput = createNumberInput(i.fps, current)
        this.row.appendChild(this.fpsInput)
    }

    reset(): void {
        this.fpsInput.value = this.fpsInput.defaultValue
    }
    submit(): number | null {
        return parsePositiveInt(this.fpsInput.value)
    }

    mountForm(form: HTMLFormElement): void {
        form.classList.add("custom-value-form")
        form.appendChild(this.message)
        form.appendChild(this.prompt)
        form.appendChild(this.row)
    }
}
