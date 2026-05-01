import "./polyfill/index.js"
import "./styles/index.js"
import { Api, apiGetRole, apiGetUser, apiLogout, apiPostRole, apiPostUser, FetchError, getApi } from "./api.js";
import { Component, ComponentEvent } from "./component/index.js";
import { showErrorPopup } from "./component/error.js";
import { setTouchContextMenuEnabled } from "./polyfill/ios_right_click.js";
import { UserList } from "./component/user/list.js";
import { AddUserModal } from "./component/user/add_modal.js";
import { showMessage, showModal } from "./component/modal/index.js";
import { buildUrl } from "./config_.js";
import { DetailedUserPage } from "./component/user/detailed_page.js";
import { User, UserEventListener } from "./component/user/index.js";
import { DetailedRole, DetailedUser } from "./api_bindings.js";
import { Role, RoleEventListener } from "./component/roles/index.js";
import { RoleList } from "./component/roles/list.js";
import { DetailedRolePage } from "./component/roles/detailed_page.js";
import { AddRoleModal } from "./component/roles/add_modal.js";
import { adoptRoleDefaultLanguage, getCurrentLanguage, getTranslations } from "./i18n.js";

let I = getTranslations(getCurrentLanguage())

async function startApp() {
    setTouchContextMenuEnabled(true)

    const api = await getApi()

    const bootstrapRole = await apiGetRole(api, { id: null })
    adoptRoleDefaultLanguage(bootstrapRole.role.default_settings)
    I = getTranslations(getCurrentLanguage())

    checkPermissions(api)

    const rootElement = document.getElementById("root")
    if (rootElement == null) {
        showErrorPopup(I.admin.rootNotFound, true)
        return;
    }

    const app = new AdminApp(api)
    app.mount(rootElement)

    app.forceFetch()

    // -- App states
    let lastAppState: AppState | null = null
    if (sessionStorage) {
        const lastStateText = sessionStorage.getItem("mlAdminState")
        if (lastStateText) {
            lastAppState = JSON.parse(lastStateText)
        }
    }

    window.addEventListener("popstate", event => {
        if (event.state) {
            app.setAppState(event.state, false)
        }
    })

    if (lastAppState) {
        app.setAppState(lastAppState)
    } else {
        // set default state
        app.setAppState({ tab: "users", user_id: null })
    }
}

async function checkPermissions(api: Api) {
    const user = await apiGetUser(api)

    if (user.role != "Admin") {
        await showMessage(I.admin.unauthorized)

        window.location.href = buildUrl("/")
    }
}

type AppState = { tab: "users", user_id: number | null } |
{ tab: "roles", role_id: number | null }
function pushAppState(state: AppState, pushHistory: boolean) {
    if (pushHistory) {
        history.pushState(state, "")
    }

    if (sessionStorage) {
        sessionStorage.setItem("mlAdminState", JSON.stringify(state))
    }
}
function backAppState() {
    history.back()
}

startApp()

class AdminApp implements Component {

    private api: Api

    private root = document.createElement("div")

    private currentState: AppState | null = null

    // Top Line
    private topLine = document.createElement("div")

    private moonlightTextElement = document.createElement("h1")

    private topLineActions = document.createElement("div")
    private logoutButton = document.createElement("button")

    private actionElement = document.createElement("div")
    private backButton = document.createElement("button")

    // Different tabs
    private tabs = document.createElement("div")
    private userTabButton = document.createElement("button")
    private rolesTabButton = document.createElement("button")

    // Content
    private content = document.createElement("div")

    // The actual content of the tabs
    private users: UserPanel | null = null
    private roles: RolePanel | null = null

    constructor(api: Api) {
        this.api = api

        // Top Line
        this.topLine.classList.add("top-line")

        this.moonlightTextElement.innerHTML =
            'Moonlight Web <span style="color:red; text-shadow: -1px -1px 0 #000, 1px -1px 0 #000, -1px 1px 0 #000, 1px 1px 0 #000; -webkit-text-stroke: 2px #000">Admin</span>'

        this.topLine.appendChild(this.moonlightTextElement)

        this.topLine.appendChild(this.topLineActions)
        this.topLineActions.classList.add("top-line-actions")

        // TODO: logout button doesn't work on default user
        this.logoutButton.addEventListener("click", async () => {
            await apiLogout(this.api)
            window.location.reload()
        })
        this.logoutButton.classList.add("logout-button")
        this.topLineActions.appendChild(this.logoutButton)

        this.root.appendChild(this.topLine)

        this.actionElement.classList.add("actions-list")
        this.backButton.classList.add("button-fit-content", "back-button")
        this.backButton.addEventListener("click", () => window.location.href = buildUrl("/"))
        this.actionElement.appendChild(this.backButton)
        this.root.appendChild(this.actionElement)

        // Tab div
        this.tabs.classList.add("admin-panel-tabs")
        this.root.appendChild(this.tabs)

        // Users tab
        this.userTabButton.innerText = I.admin.users
        this.userTabButton.addEventListener("click", () => {
            this.setAppState({ tab: "users", user_id: null })
        })
        this.tabs.appendChild(this.userTabButton)

        // Roles tab
        this.rolesTabButton.innerText = I.admin.roles
        this.rolesTabButton.addEventListener("click", () => {
            this.setAppState({ tab: "roles", role_id: null })
        })
        this.tabs.appendChild(this.rolesTabButton)

        // Content div
        this.content.classList.add("admin-panel-content")
        this.root.appendChild(this.content)
    }

    async forceFetch() {
        if (this.currentState?.tab == "users") {
            await this.users?.forceFetch()
        } else if (this.currentState?.tab == "roles") {
            await this.roles?.forceFetch()
        }
    }

    setAppState(state: AppState, pushIntoHistory?: boolean) {
        // check if tab changed and mount accordingly
        if (this.currentState?.tab != state.tab) {
            // Unmount old tab
            if (this.currentState?.tab == "users") {
                this.users?.unmount(this.content)
            } else if (this.currentState?.tab == "roles") {
                this.roles?.unmount(this.content)
            }

            // Mount and create (if necessary) new tab
            if (state.tab == "users") {
                if (!this.users) {
                    this.users = new UserPanel(this.api)
                    this.users.addUserChangedListener(event => {
                        pushAppState({ tab: "users", user_id: event.component.getUserId() }, true)
                    })
                }

                this.users.mount(this.content)
            } else if (state.tab == "roles") {
                if (!this.roles) {
                    this.roles = new RolePanel(this.api)
                    this.roles.addRoleChangedListener(event => {
                        pushAppState({ tab: "roles", role_id: event.component.getRoleId() }, true)
                    })
                }

                this.roles.mount(this.content)
            }
        }

        // Save app state to browser history
        pushAppState(state, pushIntoHistory ?? true)

        // Change the content (e.g. user / role) of the tab
        this.currentState = state
        if (state.tab == "users" && state.user_id != null) {
            this.users?.setUserId(state.user_id)
        } else if (state.tab == "roles" && state.role_id != null) {
            this.roles?.setRoleId(state.role_id)
        }

        // Force fetch self to update data
        this.forceFetch()
    }

    mount(parent: HTMLElement): void {
        parent.appendChild(this.root)
    }
    unmount(parent: HTMLElement): void {
        parent.removeChild(this.root)
    }
}

class UserPanel implements Component {

    private api: Api

    private rootDiv = document.createElement("div")

    private userPanel = document.createElement("div")
    private addUserButton = document.createElement("button")
    private userSearch = document.createElement("input")
    private userList: UserList

    private userInfoPage: DetailedUserPage | null = null

    constructor(api: Api) {
        this.api = api

        this.rootDiv.classList.add("admin-panel-users", "settings-menu")

        // Select User Panel
        this.userPanel.classList.add("user-panel", "settings-group")
        this.rootDiv.appendChild(this.userPanel)

        const header = document.createElement("div")
        header.classList.add("settings-group-header", "color-blue")
        header.innerText = I.admin.users
        this.userPanel.appendChild(header)

        this.addUserButton.innerText = I.admin.addUser
        this.addUserButton.addEventListener("click", async () => {
            const addUserModal = new AddUserModal(api)

            const userRequest = await showModal(addUserModal)

            if (userRequest) {
                try {
                    const newUser = await apiPostUser(this.api, userRequest)

                    this.userList.insertList(newUser.id, newUser)
                } catch (e) {
                    // 409 = Conflict
                    if (e instanceof FetchError && e.getResponse()?.status == 409) {
                        // Name already exists
                        await showMessage(I.admin.userExists(userRequest.name))
                    } else {
                        throw e
                    }
                }
            }
        })
        this.userPanel.appendChild(this.addUserButton)

        this.userSearch.placeholder = I.admin.searchUser
        this.userSearch.type = "text"
        this.userSearch.addEventListener("input", this.onUserSearchChange.bind(this))
        this.userPanel.appendChild(this.userSearch)

        this.userList = new UserList(api)
        this.userList.addUserClickedListener(this.onUserClicked.bind(this))
        this.userList.addUserDeletedListener(this.onUserDeleted.bind(this))
        this.userList.mount(this.userPanel)
    }

    addUserChangedListener(listener: UserEventListener) {
        this.userList.addUserClickedListener(listener)
    }

    getCurrentUserId(): number | null {
        return this.userInfoPage?.getUserId() ?? null
    }

    async forceFetch() {
        await this.userList.forceFetch()
    }

    private onUserSearchChange() {
        this.userList.setFilter(this.userSearch.value)
    }

    private async onUserClicked(event: ComponentEvent<User>) {
        await this.setUserId(event.component.getUserId())
    }
    async setUserId(userId: number) {
        const user = await apiGetUser(this.api, {
            user_id: userId,
            name: null
        })

        this.setUserInfo(user)
    }
    private setUserInfo(user: DetailedUser | null) {
        if (this.userInfoPage) {
            this.userInfoPage.unmount(this.rootDiv)
            this.userInfoPage.removeDeletedListener(this.onUserDeleted.bind(this))
        }

        this.userInfoPage = null
        if (user) {
            this.userInfoPage = new DetailedUserPage(this.api, user)
            this.userInfoPage.addDeletedListener(this.onUserDeleted.bind(this))
            this.userInfoPage.mount(this.rootDiv)
        }
    }

    private onUserDeleted(event: ComponentEvent<User>) {
        if (this.userInfoPage?.getUserId() == event.component.getUserId()) {
            this.setUserInfo(null)
        }
        this.userList.removeUser(event.component.getUserId())
    }

    mount(parent: HTMLElement): void {
        parent.appendChild(this.rootDiv)
    }
    unmount(parent: HTMLElement): void {
        parent.removeChild(this.rootDiv)
    }
}

class RolePanel implements Component {

    private api: Api

    private rootDiv = document.createElement("div")

    private rolePanel = document.createElement("div")
    private addRoleButton = document.createElement("button")
    private roleSearch = document.createElement("input")
    private roleList: RoleList

    private roleInfoPage: DetailedRolePage | null = null

    constructor(api: Api) {
        this.api = api

        this.rootDiv.classList.add("admin-panel-roles", "settings-menu")

        // Select Role Panel
        this.rolePanel.classList.add("role-panel", "settings-group")
        this.rootDiv.appendChild(this.rolePanel)

        const header = document.createElement("div")
        header.classList.add("settings-group-header", "color-purple")
        header.innerText = I.admin.roles
        this.rolePanel.appendChild(header)

        this.addRoleButton.innerText = I.admin.addRole
        this.addRoleButton.addEventListener("click", async () => {
            const addRoleModal = new AddRoleModal()

            const roleRequest = await showModal(addRoleModal)

            if (roleRequest) {
                try {
                    const newRole = await apiPostRole(this.api, roleRequest)

                    this.roleList.insertList(newRole.role.id, newRole.role)
                } catch (e) {
                    // 409 = Conflict
                    if (e instanceof FetchError && e.getResponse()?.status == 409) {
                        // Name already exists
                        await showMessage(I.admin.roleExists(roleRequest.name))
                    } else {
                        throw e
                    }
                }
            }
        })
        this.rolePanel.appendChild(this.addRoleButton)

        this.roleSearch.placeholder = I.admin.searchRole
        this.roleSearch.type = "text"
        this.roleSearch.addEventListener("input", this.onRoleSearchChange.bind(this))
        this.rolePanel.appendChild(this.roleSearch)

        this.roleList = new RoleList(api)
        this.roleList.addRoleClickedListener(this.onRoleClicked.bind(this))
        this.roleList.addRoleDeletedListener(this.onRoleDeleted.bind(this))
        this.roleList.mount(this.rolePanel)
    }

    addRoleChangedListener(listener: RoleEventListener) {
        this.roleList.addRoleClickedListener(listener)
    }

    getCurrentRoleId(): number | null {
        return this.roleInfoPage?.getRoleId() ?? null
    }

    async forceFetch() {
        await this.roleList.forceFetch()
    }

    private onRoleSearchChange() {
        this.roleList.setFilter(this.roleSearch.value)
    }

    private async onRoleClicked(event: ComponentEvent<Role>) {
        await this.setRoleId(event.component.getRoleId())
    }
    async setRoleId(roleId: number) {
        const role = await apiGetRole(this.api, {
            id: roleId,
        })

        this.setRoleInfo(role.role)
    }
    private setRoleInfo(role: DetailedRole | null) {
        if (this.roleInfoPage) {
            this.roleInfoPage.unmount(this.rootDiv)
            this.roleInfoPage.removeDeletedListener(this.onRoleDeleted.bind(this))
        }

        this.roleInfoPage = null
        if (role) {
            this.roleInfoPage = new DetailedRolePage(this.api, role)
            this.roleInfoPage.addDeletedListener(this.onRoleDeleted.bind(this))
            this.roleInfoPage.mount(this.rootDiv)
        }
    }

    private onRoleDeleted(event: ComponentEvent<Role>) {
        if (this.roleInfoPage?.getRoleId() == event.component.getRoleId()) {
            this.setRoleInfo(null)
        }
        this.roleList.removeRole(event.component.getRoleId())
    }

    mount(parent: HTMLElement): void {
        parent.appendChild(this.rootDiv)
    }
    unmount(parent: HTMLElement): void {
        parent.removeChild(this.rootDiv)
    }
}
