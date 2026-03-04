
export interface Component {
    mount(parent: HTMLElement): void

    unmount(parent: HTMLElement): void
}

export class ComponentEvent<T extends Component> extends Event {
    component: T

    constructor(type: string, component: T) {
        super(type)

        this.component = component
    }
}

export interface FetchComponent<Data> extends Component {
    forceFetch(): Promise<void>

    updateCache(data: Array<Data>): void
}
