import "./polyfill/index.js"
import "./styles/index.js"
import { Api, apiGetUser, apiLogout, apiPostUser, FetchError, getApi } from "./api.js";
import { Component, ComponentEvent } from "./component/index.js";
import { showErrorPopup } from "./component/error.js";
import { setTouchContextMenuEnabled } from "./polyfill/ios_right_click.js";
import { UserList } from "./component/user/list.js";
import { AddUserModal } from "./component/user/add_modal.js";
import { showMessage, showModal } from "./component/modal/index.js";
import { buildUrl } from "./config_.js";
import { DetailedUserPage } from "./component/user/detailed_page.js";
import { User } from "./component/user/index.js";
import { DetailedUser } from "./api_bindings.js";

async function startApp() {
    setTouchContextMenuEnabled(true)

    const api = await getApi()

    checkPermissions(api)

    const rootElement = document.getElementById("root")
    if (rootElement == null) {
        showErrorPopup("couldn't find root element", true)
        return;
    }

    const app = new AdminApp(api)
    app.mount(rootElement)

    app.forceFetch()
}

async function checkPermissions(api: Api) {
    const user = await apiGetUser(api)

    if (user.role != "Admin") {
        await showMessage("You are not authorized to view this page!")

        window.location.href = buildUrl("/")
    }
}

startApp()

class AdminApp implements Component {

    private api: Api

    private root = document.createElement("div")

    // Top Line
    private topLine = document.createElement("div")

    private moonlightTextElement = document.createElement("h1")

    private topLineActions = document.createElement("div")
    private logoutButton = document.createElement("button")
    private userButton = document.createElement("button")

    // Content
    private content = document.createElement("div")

    // User Panel
    private userPanel = document.createElement("div")
    private addUserButton = document.createElement("button")
    private userSearch = document.createElement("input")
    private userList: UserList

    // User Info
    private userInfoPage: DetailedUserPage | null = null

    constructor(api: Api) {
        this.api = api

        // Top Line
        this.topLine.classList.add("top-line")

        this.moonlightTextElement.innerHTML =
            'Moonlight Web <span style="color:red; text-shadow: -1px -1px 0 #000, 1px -1px 0 #000, -1px 1px 0 #000, 1px 1px 0 #000; -webkit-text-stroke: 2px #000">Admin</span>'

        this.topLine.appendChild(this.moonlightTextElement)

        this.topLine.appendChild(this.topLineActions)
        this.topLineActions.classList.add("top-line-actions")

        this.logoutButton.addEventListener("click", async () => {
            await apiLogout(this.api)
            window.location.reload()
        })
        this.logoutButton.classList.add("logout-button")
        this.topLineActions.appendChild(this.logoutButton)

        this.userButton.addEventListener("click", async () => {
            window.location.href = buildUrl("/")
        })
        this.userButton.classList.add("user-button")
        this.topLineActions.appendChild(this.userButton)

        this.root.appendChild(this.topLine)

        // Content div
        this.content.classList.add("admin-panel-content")
        this.root.appendChild(this.content)

        // Select User Panel
        this.userPanel.classList.add("user-panel")
        this.content.appendChild(this.userPanel)

        this.addUserButton.innerText = "Add User"
        this.addUserButton.addEventListener("click", async () => {
            const addUserModal = new AddUserModal()

            const userRequest = await showModal(addUserModal)

            if (userRequest) {
                try {
                    const newUser = await apiPostUser(this.api, userRequest)

                    this.userList.insertList(newUser.id, newUser)
                } catch (e) {
                    // 409 = Conflict
                    if (e instanceof FetchError && e.getResponse()?.status == 409) {
                        // Name already exists
                        await showMessage(`A user with the name "${userRequest.name}" already exists!`)
                    } else {
                        throw e
                    }
                }
            }
        })
        this.userPanel.appendChild(this.addUserButton)

        this.userSearch.placeholder = "Search User"
        this.userSearch.type = "text"
        this.userSearch.addEventListener("input", this.onUserSearchChange.bind(this))
        this.userPanel.appendChild(this.userSearch)

        this.userList = new UserList(api)
        this.userList.addUserClickedListener(this.onUserClicked.bind(this))
        this.userList.addUserDeletedListener(this.onUserDeleted.bind(this))
        this.userList.mount(this.userPanel)
    }

    async forceFetch() {
        await this.userList.forceFetch()
    }

    private onUserSearchChange() {
        this.userList.setFilter(this.userSearch.value)
    }

    private async onUserClicked(event: ComponentEvent<User>) {
        const user = await apiGetUser(this.api, {
            user_id: event.component.getUserId(),
            name: null
        })

        this.setUserInfo(user)
    }
    private setUserInfo(user: DetailedUser | null) {
        if (this.userInfoPage) {
            this.userInfoPage.unmount(this.content)
            this.userInfoPage.removeDeletedListener(this.onUserDeleted.bind(this))
        }

        this.userInfoPage = null
        if (user) {
            this.userInfoPage = new DetailedUserPage(this.api, user)
            this.userInfoPage.addDeletedListener(this.onUserDeleted.bind(this))
            this.userInfoPage.mount(this.content)
        }
    }

    private onUserDeleted(event: ComponentEvent<User>) {
        if (this.userInfoPage?.getUserId() == event.component.getUserId()) {
            this.setUserInfo(null)
        }
        this.userList.removeUser(event.component.getUserId())
    }

    mount(parent: HTMLElement): void {
        parent.appendChild(this.root)
    }
    unmount(parent: HTMLElement): void {
        parent.removeChild(this.root)
    }
}
