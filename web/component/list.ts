import { Component } from "./index.js"

export type ListComponentInit = {
    listClasses?: string[],
    elementLiClasses?: string[]
    remountIsInsert?: boolean
}

export type ListFilter<T extends Component> = (component: T) => boolean

export class ListComponent<T extends Component> implements Component {

    private unfilteredList: Array<T>
    private list: Array<T>

    private mounted: number = 0
    private remountIsInsertTransition: boolean

    private listElement = document.createElement("ul")
    private liElements: Array<HTMLLIElement> = []
    private liClasses: string[]

    constructor(list?: Array<T>, init?: ListComponentInit) {
        this.unfilteredList = list ?? []
        this.list = []

        this.listElement.classList.add("list-like")
        if (init?.listClasses) {
            this.listElement.classList.add(...init?.listClasses)
        }
        this.liClasses = init?.elementLiClasses ?? []

        this.remountIsInsertTransition = init?.remountIsInsert ?? true

        this.syncLists()
    }

    private elementAt(index: number): HTMLLIElement {
        let li = this.liElements[index]
        if (!li) {
            li = document.createElement("li")
            li.classList.add(...this.liClasses)

            this.liElements[index] = li
        }

        return li
    }

    private onAnimElementInserted(index: number) {
        const element = this.liElements[index]

        // let the element render and then add "list-show" for transitions :)
        setTimeout(() => {
            element.classList.add("list-show")
        }, 0)
    }
    private onAnimElementRemoved(index: number) {
        let element
        while ((element = this.liElements[index]).classList.contains("list-show")) {
            element.classList.remove("list-show")
        }
    }

    private currentFilter: ListFilter<T> = () => true
    setFilter(filter?: ListFilter<T>) {
        if (!filter) {
            filter = () => true
        }

        this.currentFilter = filter

        this.syncLists()
    }
    private syncLists() {
        const newList = this.unfilteredList.filter(this.currentFilter)

        // Unmount all old components
        for (let index = 0; index < this.list.length; index++) {
            const oldComponent = this.list[index]
            const element = this.elementAt(index)

            oldComponent.unmount(element)

            if (index >= newList.length) {
                this.listElement.removeChild(element)
            }
        }

        // Mount all new components and unmount old
        for (let index = 0; index < newList.length; index++) {
            const newComponent = newList[index]
            const element = this.elementAt(index)

            if (this.list.length <= index) {
                this.listElement.appendChild(element)
            }

            newComponent.mount(element)
        }

        this.list = newList
    }

    insert(index: number, value: T) {
        if (index == this.unfilteredList.length) {
            this.unfilteredList.push(value)
        } else {
            this.unfilteredList.splice(index, 0, value)
        }

        this.syncLists()

        this.onAnimElementInserted(index)
    }
    remove(index: number): T | null {
        if (index >= this.unfilteredList.length) {
            this.onAnimElementRemoved(index)
        }

        const value = this.unfilteredList.splice(index, 1)

        this.syncLists()

        return value[0]
    }

    append(value: T) {
        this.insert(this.get().length, value)
    }
    removeValue(value: T) {
        const index = this.get().indexOf(value)
        if (index != -1) {
            this.remove(index)
        }
    }

    clear() {
        this.unfilteredList.splice(0, this.unfilteredList.length)

        this.syncLists()
    }

    get(): readonly T[] {
        return this.unfilteredList
    }

    mount(parent: Element): void {
        this.mounted++

        parent.appendChild(this.listElement)

        // Mount all elements
        if (this.mounted == 1) {
            this.syncLists()

            if (this.remountIsInsertTransition) {
                for (let i = 0; i < this.list.length; i++) {
                    this.onAnimElementInserted(i)
                }
            }
        }
    }
    unmount(parent: Element): void {
        this.mounted--

        parent.removeChild(this.listElement)

        // Unmount all elements
        if (this.mounted == 0) {
            if (this.remountIsInsertTransition) {
                for (let i = 0; i < this.list.length; i++) {
                    this.onAnimElementRemoved(i)
                }
            }

            for (let index = this.list.length - 1; index >= 0; index--) {
                const element = this.elementAt(index)
                this.listElement.removeChild(element)
            }
            this.list = []
        }
    }
}
