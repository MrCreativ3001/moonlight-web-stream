/// Contains code for the on screen keyboard

export type TextEvent = CustomEvent<{ text: string }>
export type KeyboardModeChangeEvent = CustomEvent<{
    enabled: boolean,
    /// The keyboard height, when the keyboard is at the bottom based on the visualViewport
    keyboardHeight: number
}>

export interface ScreenKeyboardListener {
    onKeyDown(event: KeyboardEvent): void
    onKeyUp(event: KeyboardEvent): void

    onText(event: TextEvent): void

    onKeyboardModeChange(event: KeyboardModeChangeEvent): void
}

export interface ScreenKeyboard {
    /// The main element for the keyboard
    getHiddenElement(): HTMLElement

    /// If the keyboard is currently visible
    isVisible(): boolean
    /// Force the keyboard to be visible
    setVisible(enabled: boolean): void

    /// Returns the visible viewport without the keyboard or null if it's not supported
    getVisibleViewport(): DOMRect | null

    // -- Events
    addListener(listener: ScreenKeyboardListener): void
    removeListener(listener: ScreenKeyboardListener): void
}
