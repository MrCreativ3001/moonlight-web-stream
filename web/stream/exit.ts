import { buildUrl } from "../config_"

/** Leave the viewer after an explicit exit or a graceful host termination. */
export function exitStreamPage() {
    if (window.matchMedia('(display-mode: standalone)').matches && history.length > 1) {
        history.back()
    } else {
        window.close()
        // Browsers may refuse to close a tab opened directly by the user.
        // In that case, return to the app list instead of leaving a dead viewer.
        window.setTimeout(() => {
            if (!window.closed) {
                window.location.replace(buildUrl("/"))
            }
        }, 0)
    }
}
