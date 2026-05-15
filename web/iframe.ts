
export function requestKeyboardLock(keys?: string[]): Promise<void> {
    const topWindow = window.top;
    let lockerFunc: ((keys?: string[]) => Promise<void>) | undefined;

    if (window.self === topWindow) {
        if (navigator.keyboard?.lock) {
            lockerFunc = navigator.keyboard.lock.bind(navigator.keyboard);
        }
    } else if (topWindow) {
        let sameOrigin = false;
        try {
            sameOrigin = window.location.origin === topWindow.location.origin;
        } catch (e) {
            sameOrigin = false;
        }

        if (sameOrigin) {
            if (topWindow.navigator.keyboard?.lock) {
                lockerFunc = topWindow.navigator.keyboard.lock.bind(topWindow.navigator.keyboard);
            }
        } else {
            lockerFunc = (k) => {
                const requestId = Math.random().toString(36).substring(2, 9);
                window.parent.postMessage({ type: "REQUEST_KEYBOARD_LOCK", requestId, keys: k }, "*");
                return Promise.resolve();
            };
        }
    }

    if (!lockerFunc) {
        return Promise.resolve();
    }

    return lockerFunc(keys);
}