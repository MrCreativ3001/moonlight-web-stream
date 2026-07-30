import { readFileSync } from "node:fs"
import assert from "node:assert/strict"

const source = readFileSync(new URL("../web/component/modal/login.ts", import.meta.url), "utf8")
const mountFormMatch = source.match(/mountForm\(form: HTMLFormElement\): void \{(?<body>[\s\S]*?)\n    \}/)

assert.ok(mountFormMatch?.groups?.body, "ApiUserPasswordPrompt.mountForm was not found")

const body = mountFormMatch.groups.body
const usernameIndex = body.indexOf("this.name.mount(form)")
const passwordIndex = body.indexOf("this.password.mount(form)")
const passwordFileIndex = body.indexOf("this.passwordFile.mount(form)")
const oidcIndex = body.indexOf("form.appendChild(this.oidcButton)")

assert.notEqual(usernameIndex, -1, "username input is not mounted")
assert.notEqual(passwordIndex, -1, "password input is not mounted")
assert.notEqual(passwordFileIndex, -1, "password file input is not mounted")
assert.notEqual(oidcIndex, -1, "OIDC button is not mounted")

assert.ok(
    usernameIndex < passwordIndex && passwordIndex < passwordFileIndex,
    "local login controls must keep username, password, password file order",
)
assert.ok(
    passwordFileIndex < oidcIndex,
    "OIDC button must be mounted below all local login controls",
)
