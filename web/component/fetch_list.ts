import { Component, FetchComponent } from "./index.js"
import { ListComponent, ListComponentInit } from "./list.js"

export abstract class FetchListComponent<Data, T extends Component> implements FetchComponent<Data>, Component {
    protected list: ListComponent<T>

    constructor(listInit?: ListComponentInit) {
        this.list = new ListComponent<T>([], listInit)
    }

    protected abstract updateComponentData(component: T, data: Data): void

    protected abstract getComponentDataId(component: T): number
    protected abstract getDataId(data: Data): number

    abstract forceFetch(forceServerRefresh?: boolean): Promise<void>

    updateCache(cache: Array<Data>) {
        // Remove all non existing new data
        // Update all already existing components
        for (let i = 0; i < this.list.get().length; i++) {
            let component = this.list.get()[i]

            const dataId = this.getComponentDataId(component)

            const cacheIndex = cache.findIndex(data => this.getDataId(data) == dataId)
            if (cacheIndex == -1) {
                this.removeList(i)

                // removing an element will shift the array to the left
                // -> this means that we need to decr to get the next value because we incr in the loop
                i--
            } else {
                this.updateComponentData(component, cache[cacheIndex])
            }
        }

        // All all newly created data
        for (let i = 0; i < cache.length; i++) {
            let data = cache[i]

            const dataId = this.getDataId(data)

            const listIndex = this.list.get().findIndex(component => this.getComponentDataId(component) == dataId)
            if (listIndex == -1) {
                this.insertList(dataId, data)
            }
        }
    }

    protected abstract insertList(dataId: number, data: Data): void
    protected removeList(listIndex: number) {
        this.list.remove(listIndex)
    }

    mount(parent: Element): void {
        this.list.mount(parent)
    }
    unmount(parent: Element): void {
        this.list.unmount(parent)
    }
}