import { Component, ComponentEvent } from "./index.js"

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
}

export type InputChangeListener = (event: ComponentEvent<InputComponent>) => void

export class InputComponent extends ElementWithLabel {

    private fileLabel: HTMLDivElement | null = null
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

        if (type == "file") {
            this.fileLabel = document.createElement("div")
            this.fileLabel.innerText = this.label.innerText
            this.fileLabel.classList.add("file-label")

            this.label.innerText = "Open File"
            this.label.classList.add("file-button")

            this.div.insertBefore(this.fileLabel, this.label)
        }

        this.div.appendChild(this.input)

        this.input.addEventListener("change", () => {
            this.div.dispatchEvent(new ComponentEvent("ml-change", this))
        })
    }

    reset() {
        this.input.value = ""
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
        this.input.disabled = !enabled
    }

    addChangeListener(listener: InputChangeListener, options?: AddEventListenerOptions) {
        this.div.addEventListener("ml-change", listener as any, options)
    }
    removeChangeListener(listener: InputChangeListener) {
        this.div.removeEventListener("ml-change", listener as any)
    }
}

export type SelectInit = {
    // Only uses datalist if supported
    hasSearch?: boolean
    preSelectedOption?: string
    displayName?: string,
}

export class SelectComponent extends ElementWithLabel {

    private strategy: "select" | "datalist"

    private preSelectedOption: string = ""
    private options: Array<{ value: string, name: string }>

    private inputElement: null | HTMLInputElement
    private optionRoot: HTMLSelectElement | HTMLDataListElement

    constructor(internalName: string, options: Array<{ value: string, name: string }>, init?: SelectInit) {
        super(internalName, init?.displayName)

        if (init && init.preSelectedOption) {
            this.preSelectedOption = init.preSelectedOption
        }
        this.options = options

        if (init && init.hasSearch && isElementSupported("datalist")) {
            this.strategy = "datalist"

            this.optionRoot = document.createElement("datalist")
            this.optionRoot.id = `${internalName}-list`

            this.inputElement = document.createElement("input")
            this.inputElement.type = "text"
            this.inputElement.id = internalName
            this.inputElement.setAttribute("list", this.optionRoot.id)

            if (init && init.preSelectedOption) {
                this.inputElement.defaultValue = init.preSelectedOption
            }

            this.div.appendChild(this.inputElement)
            this.div.appendChild(this.optionRoot)
        } else {
            this.strategy = "select"

            this.inputElement = null

            this.optionRoot = document.createElement("select")
            this.optionRoot.id = internalName

            this.div.appendChild(this.optionRoot)
        }

        for (const option of options) {
            const optionElement = document.createElement("option")

            if (this.strategy == "datalist") {
                optionElement.value = option.name
            } else if (this.strategy == "select") {
                optionElement.innerText = option.name
                optionElement.value = option.value
            }

            if (init && init.preSelectedOption == option.value) {
                optionElement.selected = true
            }

            this.optionRoot.appendChild(optionElement)
        }

        this.optionRoot.addEventListener("change", () => {
            this.div.dispatchEvent(new ComponentEvent("ml-change", this))
        })
    }

    reset() {
        if (this.strategy == "datalist") {
            const inputElement = (this.inputElement as HTMLInputElement)
            inputElement.value = ""
        } else {
            const selectElement = (this.optionRoot as HTMLSelectElement)
            selectElement.value = ""
        }
    }

    getValue(): string | null {
        if (this.strategy == "datalist") {
            const name = (this.inputElement as HTMLInputElement).value

            return this.options.find(option => option.name == name)?.value ?? ""
        } else if (this.strategy == "select") {
            return (this.optionRoot as HTMLSelectElement).value
        }

        throw "Invalid strategy for select input field"
    }

    setOptionEnabled(value: string, enabled: boolean) {
        for (const optionElement of this.optionRoot.options) {
            if (optionElement.value == value) {
                optionElement.disabled = !enabled
            }
        }
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