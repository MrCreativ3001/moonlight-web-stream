import { ConfigJs } from "./api_bindings"

declare global {
    interface Window {
        __CONFIG_JS__: ConfigJs
    }
}
