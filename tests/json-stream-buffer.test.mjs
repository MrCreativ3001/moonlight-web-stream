import { readFileSync } from "node:fs"
import assert from "node:assert/strict"

const source = readFileSync(new URL("../web/api.ts", import.meta.url), "utf8")
const classMatch = source.match(/class StreamedJsonResponse[\s\S]*?\n}\n\nexport async function fetchApi/)

assert.ok(classMatch, "StreamedJsonResponse implementation was not found")
const implementation = classMatch[0]

assert.doesNotMatch(
    implementation,
    /\.split\("\\n",\s*2\)/,
    "limited split drops buffered JSON records after the second newline",
)
assert.match(implementation, /indexOf\("\\n"\)/, "stream parser must locate one newline without discarding the remaining buffer")
assert.match(implementation, /slice\(newlineIndex \+ 1\)/, "stream parser must preserve every byte after the consumed JSON line")
