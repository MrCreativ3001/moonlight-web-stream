import { Component, ComponentEvent } from "./index.js"
import { getLocalStreamSettings } from "./settings_menu.js"

export class ElementWithLabel implements Component {
    protected div: HTMLDivElement = document.createElement("div")
    protected label: HTMLLabelElement = document.createElement("label")

    constructor(internalName: string, displayName?: string) {
        if (displayName) {
            this.label.htmlFor = internalName
            this.label.innerText = displayName
            this.div.appendChild(this.label)
        }
    }

    mount(parent: HTMLElement): void {
        parent.appendChild(this.div)
    }
    unmount(parent: HTMLElement): void {
        parent.removeChild(this.div)
    }
}

export type InputInit = {
    defaultValue?: string
    value?: string
    checked?: boolean
    step?: string
    accept?: string
    inputMode?: string
    hasEnableCheckbox?: boolean
    placeholer?: string
    formRequired?: boolean
    // Only allowed with type == "number"
    numberSlider?: {
        range_min: number,
        range_max: number
        // Use step to set the step
    }
}

export type InputChangeListener = (event: ComponentEvent<InputComponent>) => void

export class InputComponent extends ElementWithLabel {

    private fileLabel: HTMLDivElement | null = null
    private numberSlider: HTMLInputElement | null = null

    private inputEnabled: HTMLInputElement | null = null
    private input: HTMLInputElement = document.createElement("input")

    constructor(internalName: string, type: string, displayName?: string, init?: InputInit) {
        super(internalName, displayName)

        this.div.classList.add("input-div")

        this.input.id = internalName
        this.input.type = type
        if (init?.defaultValue != null) {
            this.input.defaultValue = init.defaultValue
        }
        if (init?.value != null) {
            this.input.value = init.value
        }
        if (init && init.checked != null) {
            this.input.checked = init.checked
        }
        if (init && init.step != null) {
            this.input.step = init.step
        }
        if (init && init.accept != null) {
            this.input.accept = init.accept
        }
        if (init && init.inputMode != null) {
            this.input.inputMode = init.inputMode
        }
        if (init && init.formRequired != null) {
            this.input.required = init.formRequired
        }
        if (init && init.placeholer != null) {
            this.input.placeholder = init.placeholer
        }

        if (type == "file") {
            this.fileLabel = document.createElement("div")
            this.fileLabel.innerText = this.label.innerText
            this.fileLabel.classList.add("file-label")

            this.label.innerText = "Open File"
            this.label.classList.add("file-button")

            this.div.insertBefore(this.fileLabel, this.label)
        }

        if (init?.hasEnableCheckbox) {
            this.inputEnabled = document.createElement("input")
            this.inputEnabled.type = "checkbox"
            this.inputEnabled.defaultChecked = false

            this.inputEnabled.addEventListener("change", () => {
                this.setEnabled(
                    this.inputEnabled?.checked ?? (() => { throw "inputEnabled is null" })()
                )

                this.div.dispatchEvent(new ComponentEvent("ml-change", this))
            })

            this.div.appendChild(this.inputEnabled)
        }

        this.div.appendChild(this.input)

        this.input.addEventListener("change", () => {
            if (this.numberSlider) {
                this.numberSlider.value = this.input.value
            }

            this.div.dispatchEvent(new ComponentEvent("ml-change", this))
        })

        if (init?.numberSlider && type != "number") {
            throw "tried to create InputComponent with number slider but type wasn't number"
        }
        if (type == "number" && init?.numberSlider) {
            this.numberSlider = document.createElement("input")
            this.numberSlider.type = "range"
            this.numberSlider.min = `${init.numberSlider.range_min}`
            this.numberSlider.max = `${init.numberSlider.range_max}`
            this.numberSlider.step = init.step?.toString() ?? ""

            this.numberSlider.addEventListener("change", () => {
                if (this.numberSlider) {
                    this.input.value = this.numberSlider.value
                } else {
                    throw "failed to get value of number slider because it wasn't created"
                }

                this.div.dispatchEvent(new ComponentEvent("ml-change", this))
            })

            this.div.appendChild(this.numberSlider)
        }

        if (init?.hasEnableCheckbox) {
            // The main logic is further up
            this.setEnabled(false)
        }
    }

    reset() {
        this.input.value = ""
        if (this.numberSlider) {
            this.numberSlider.value = ""
        }
    }

    setValue(value: string) {
        this.input.value = value
        if (this.numberSlider) {
            this.numberSlider.value = value
        }
    }
    getValue(): string {
        return this.input.value
    }

    isChecked(): boolean {
        return this.input.checked
    }

    getFiles(): FileList | null {
        return this.input.files
    }

    setEnabled(enabled: boolean) {
        if (this.inputEnabled) {
            this.inputEnabled.checked = enabled
        }

        this.input.disabled = !enabled
        if (this.numberSlider) {
            this.numberSlider.disabled = !enabled
        }
    }
    isEnabled(): boolean {
        return !this.input.disabled
    }

    addChangeListener(listener: InputChangeListener, options?: AddEventListenerOptions) {
        this.div.addEventListener("ml-change", listener as any, options)
    }
    removeChangeListener(listener: InputChangeListener) {
        this.div.removeEventListener("ml-change", listener as any)
    }

    setPlaceholder(newPlaceholder: string) {
        this.input.placeholder = newPlaceholder
    }

    mount(parent: HTMLElement): void {
        super.mount(parent)

        if (this.numberSlider) {
            this.numberSlider.value = this.input.value
        }
    }
}

export type SelectInit = {
    // Only uses datalist if supported
    hasSearch?: boolean
    preSelectedOption?: string
    displayName?: string,
}

type SelectStrategy =
    { name: "select", optionRoot: HTMLSelectElement } |
    { name: "datalist", optionRoot: HTMLDataListElement, inputElement: HTMLInputElement } |
    { name: "polyfill", opened: boolean, wrapper: HTMLDivElement, display: HTMLParagraphElement, list: HTMLDivElement, value: string | null, disabled: Set<string> }

function useSelectElementPolyfill(): boolean {
    return getLocalStreamSettings()?.useSelectElementPolyfill ?? false
}

export class SelectComponent extends ElementWithLabel {

    private preSelectedOption: string = ""
    private options: Array<{ value: string, name: string }>

    private strategy: SelectStrategy

    constructor(internalName: string, options: Array<{ value: string, name: string }>, init?: SelectInit) {
        super(internalName, init?.displayName)

        if (init && init.preSelectedOption) {
            this.preSelectedOption = init.preSelectedOption
        }
        this.options = options

        // Create base
        if (useSelectElementPolyfill() || !isElementSupported("select")) {
            const wrapper = document.createElement("div")

            wrapper.classList.add("select-polyfill-wrapper")

            this.div.appendChild(wrapper)
            this.div.classList.add("input-div")

            const display = document.createElement("p")
            display.classList.add("select-polyfill-display")

            display.addEventListener("click", () => {
                if (this.strategy.name != "polyfill") {
                    throw "SelectComponent strategy is not polyfill"
                }

                this.setStrategyPolyfillOpened(!this.strategy.opened)
            })

            const list = document.createElement("div")

            list.classList.add("select-polyfill-list")

            wrapper.appendChild(display)

            this.strategy = {
                name: "polyfill",
                opened: false,
                wrapper,
                display,
                list,
                value: init?.preSelectedOption ?? "",
                disabled: new Set()
            }
        } else if (init && init.hasSearch && isElementSupported("datalist")) {
            const dataListElement = document.createElement("datalist")

            dataListElement.id = `${internalName}-list`

            const inputElement = document.createElement("input")
            inputElement.type = "text"
            inputElement.id = internalName
            inputElement.setAttribute("list", dataListElement.id)

            if (init && init.preSelectedOption) {
                inputElement.defaultValue = init.preSelectedOption
            }

            this.div.appendChild(inputElement)
            this.div.appendChild(dataListElement)

            this.strategy = {
                name: "datalist",
                optionRoot: dataListElement,
                inputElement,
            }
        } else {
            const selectElement = document.createElement("select")
            selectElement.id = internalName

            this.div.appendChild(selectElement)

            this.strategy = {
                name: "select",
                optionRoot: selectElement,
            }
        }

        // Append values
        if (this.strategy.name == "datalist" || this.strategy.name == "select") {
            const optionRoot = this.strategy.optionRoot

            for (const option of options) {
                const optionElement = document.createElement("option")

                if (this.strategy.name == "datalist") {
                    optionElement.value = option.name
                } else if (this.strategy.name == "select") {
                    optionElement.innerText = option.name
                    optionElement.value = option.value
                }

                if (init && init.preSelectedOption == option.value) {
                    optionElement.selected = true
                }

                optionRoot.appendChild(optionElement)
            }

            optionRoot.addEventListener("change", () => {
                this.dispatchChange()
            })
        } else if (this.strategy.name == "polyfill") {
            const optionRoot = this.strategy.list

            for (const option of options) {
                const optionElement = document.createElement("p")

                optionElement.innerText = option.name

                // @ts-ignore
                optionElement.value = option.value

                optionElement.addEventListener("click", () => {
                    if (this.strategy.name != "polyfill") {
                        throw "SelectComponent strategy is not polyfill even though it was initialized using polyfill strategy"
                    }

                    if (this.strategy.disabled.has(option.value)) {
                        return
                    }

                    this.strategy.value = option.value
                    this.setStrategyPolyfillOpened(false)

                    this.updateStrategyPolyfill()

                    this.dispatchChange()
                })

                optionRoot.appendChild(optionElement)
            }

            this.updateStrategyPolyfill()
        }
    }

    private dispatchChange() {
        this.div.dispatchEvent(new ComponentEvent("ml-change", this))
    }

    reset() {
        if (this.strategy.name == "datalist") {
            const inputElement = this.strategy.inputElement

            inputElement.value = ""
        } else if (this.strategy.name == "select") {
            const selectElement = this.strategy.optionRoot

            selectElement.value = this.preSelectedOption
        } else if (this.strategy.name == "polyfill") {
            this.strategy.value = this.preSelectedOption

            this.updateStrategyPolyfill()
        }
    }

    getValue(): string | null {
        if (this.strategy.name == "datalist") {
            const name = this.strategy.inputElement.value

            return this.options.find(option => option.name == name)?.value ?? ""
        } else if (this.strategy.name == "select") {
            const selectElement = this.strategy.optionRoot

            return selectElement.value
        } else if (this.strategy.name == "polyfill") {
            return this.strategy.value
        }

        throw "Invalid strategy for select input field"
    }

    setOptionEnabled(value: string, enabled: boolean) {
        if (this.strategy.name == "datalist" || this.strategy.name == "select") {
            const optionRoot = this.strategy.optionRoot

            for (const optionElement of optionRoot.options) {
                if (optionElement.value == value) {
                    optionElement.disabled = !enabled
                }
            }
        } else if (this.strategy.name == "polyfill") {
            const element = this.strategy.list

            for (const optionElement of element.children) {
                // @ts-ignore
                const elementValue = optionElement.value

                if (elementValue != value) {
                    continue
                }

                if (enabled) {
                    this.strategy.disabled.delete(value)

                    optionElement.classList.remove("select-polyfill-option-disabled")
                } else {
                    this.strategy.disabled.add(value)

                    optionElement.classList.add("select-polyfill-option-disabled")
                }
            }
        }
    }

    private updateStrategyPolyfill() {
        if (this.strategy.name != "polyfill") {
            throw "SelectComponent strategy is not polyfill"
        }

        for (const optionElement of this.strategy.list.children) {
            // @ts-ignore
            const value = optionElement.value

            if (value == this.strategy.value) {
                optionElement.classList.add("select-polyfill-selected")
            } else {
                optionElement.classList.remove("select-polyfill-selected")
            }
        }

        const value = this.strategy.value
        const selectedOption = this.options.find(option => option.value == value)

        this.strategy.display.innerText = selectedOption?.name ?? "(Not Selected)"
    }
    private setStrategyPolyfillOpened(opened: boolean) {
        if (this.strategy.name != "polyfill") {
            throw "SelectComponent strategy is not polyfill"
        }

        if (opened != this.strategy.opened) {
            if (opened) {
                const list = this.strategy.list

                this.strategy.wrapper.appendChild(this.strategy.list)

                if ("screenTop" in window && "innerHeight" in window) {
                    const displayRect = list.getBoundingClientRect()
                    const viewportBottom = window.screenTop + window.innerHeight

                    const spaceBelow = viewportBottom - displayRect.bottom

                    if (spaceBelow < 20) {
                        list.classList.add("top")
                    } else {
                        list.classList.add("bottom")
                    }
                } else {
                    list.classList.add("bottom")
                }
            } else {
                this.strategy.wrapper.removeChild(this.strategy.list)
                this.strategy.list.classList.remove("top")
                this.strategy.list.classList.remove("bottom")
            }
        }
        this.strategy.opened = opened
    }

    addChangeListener(listener: InputChangeListener, options?: AddEventListenerOptions) {
        this.div.addEventListener("ml-change", listener as any, options)
    }
    removeChangeListener(listener: InputChangeListener) {
        this.div.removeEventListener("ml-change", listener as any)
    }
}

export function isElementSupported(tag: string) {
    // Create a test element for the tag
    const element = document.createElement(tag);

    // Check for support of custom elements registered via
    // `document.registerElement`
    if (tag.indexOf('-') > -1) {
        // Registered elements have their own constructor, while unregistered
        // ones use the `HTMLElement` or `HTMLUnknownElement` (if invalid name)
        // constructor (http://stackoverflow.com/a/28210364/1070244)
        return (
            element.constructor !== window.HTMLUnknownElement &&
            element.constructor !== window.HTMLElement
        );
    }

    // Obtain the element's internal [[Class]] property, if it doesn't 
    // match the `HTMLUnknownElement` interface than it must be supported
    return toString.call(element) !== '[object HTMLUnknownElement]';
};