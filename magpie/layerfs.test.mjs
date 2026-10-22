import { expect, test } from "bun:test"
import { createHash } from "node:crypto"
import { existsSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { LayerfsTools } from "./index.mjs"
import { NativeBackend, SimBackend, createBackend } from "./layerfs.mjs"

const sha = (s) => createHash("sha256").update(s).digest("hex")

function fixture() {
  const src = mkdtempSync(join(tmpdir(), "layerfs-src-"))
  mkdirSync(join(src, "docs"), { recursive: true })
  writeFileSync(join(src, "README.md"), "hello layerfs\n")
  writeFileSync(join(src, "docs", "a.txt"), "A".repeat(1000))
  writeFileSync(join(src, "docs", "b.txt"), "A".repeat(1000)) // identical to a.txt on purpose
  writeFileSync(join(src, "code.js"), "console.log(1)\n")
  return src
}

async function simTools() {
  const home = mkdtempSync(join(tmpdir(), "layerfs-home-"))
  process.env.MAGPIE_LAYERFS_HOME = home
  process.env.MAGPIE_LAYERFS_BACKEND = "sim"
  delete process.env.LAYERFS_SDK_PATH
  delete process.env.LAYERFS_STORE_PATH
  const { tool } = await LayerfsTools({ directory: fixture() })
  const call = (name, args) => tool[name].execute(args, { agent: "test", abort: AbortSignal.timeout(5000) })
  return { call, home }
}

test("gives six tools, each with a JSON Schema and an execute", async () => {
  const { tool } = await LayerfsTools({})
  expect(Object.keys(tool).sort()).toEqual(["checkpoint", "fork", "history", "provision", "resolve", "status"])
  for (const [name, t] of Object.entries(tool)) {
    expect(name).toMatch(/^[A-Za-z0-9_-]{1,48}$/)
    expect(t.description.length).toBeGreaterThan(40)
    expect(typeof t.execute).toBe("function")
    expect(t.args.type).toBe("object")
  }
})

test("provision imports the source once, then answers the same path without copying", async () => {
  const { call } = await simTools()
  const first = await call("provision", { project: "website" })
  expect(first.projectCreated).toBe(true)
  expect(first.reused).toBe(false)
  expect(first.files).toBe(4)
  expect(readFileSync(join(first.path, "README.md"), "utf8")).toBe("hello layerfs\n")
  const again = await call("provision", { project: "website" })
  expect(again.path).toBe(first.path)
  expect(again.reused).toBe(true)
  expect(again.stale).toBe(false)
})

test("checkpoint stores only the blobs that changed", async () => {
  const { call } = await simTools()
  const ws = await call("provision", { project: "website" })
  writeFileSync(join(ws.path, "code.js"), "console.log(2)\n") // modified, 15 bytes
  writeFileSync(join(ws.path, "new.txt"), "brand new\n") // added, 10 bytes
  const cp = await call("checkpoint", { project: "website", message: "second take" })
  expect(cp.added).toBe(1)
  expect(cp.modified).toBe(1)
  expect(cp.deleted).toBe(0)
  expect(cp.newBytes).toBe(15 + 10)
  expect(cp.logicalBytes).toBe(14 + 1000 + 1000 + 15 + 10)
  expect(cp.savedBytes).toBe(cp.logicalBytes - cp.newBytes)
})

test("fork copies no bytes and shares the history", async () => {
  const { call } = await simTools()
  await call("provision", { project: "website" })
  const before = await call("status", {})
  const f = await call("fork", { project: "website", branch: "agent-2" })
  expect(f.copiedBytes).toBe(0)
  const after = await call("status", {})
  expect(after.storedBytes).toBe(before.storedBytes)
  const ws2 = await call("provision", { project: "website", branch: "agent-2" })
  expect(readFileSync(join(ws2.path, "docs", "a.txt"), "utf8")).toBe("A".repeat(1000))
  const hist = await call("history", { project: "website", branch: "agent-2" })
  expect(hist.commits.length).toBe(1)
  expect(hist.commits[0].message).toContain("Import")
  await expect(call("fork", { project: "website", branch: "agent-2" })).rejects.toThrow("already exists")
})

test("identical files are stored once (dedup at import)", async () => {
  const { call } = await simTools()
  await call("provision", { project: "website" })
  const st = await call("status", {})
  // unique contents: "hello layerfs\n" (14) + "A"*1000 (1000) + "console.log(1)\n" (15)
  expect(st.storedBytes).toBe(14 + 1000 + 15)
  expect(st.logicalBytes).toBe(14 + 1000 + 1000 + 15)
  expect(st.dedupRatio).toBeGreaterThan(1)
})

test("resolve answers a path and a file's hash without copying", async () => {
  const { call } = await simTools()
  const ws = await call("provision", { project: "website" })
  const root = await call("resolve", { project: "website" })
  expect(root.path).toBe(ws.path)
  const file = await call("resolve", { project: "website", path: "docs/a.txt" })
  expect(file.path).toBe(join(ws.path, "docs", "a.txt"))
  expect(file.sha).toBe(sha("A".repeat(1000)))
  expect(file.size).toBe(1000)
})

test("names and paths are validated", async () => {
  const { call } = await simTools()
  await expect(call("provision", { project: "Bad Name" })).rejects.toThrow("project name")
  await call("provision", { project: "website" })
  await expect(call("resolve", { project: "website", path: "../escape" })).rejects.toThrow("stay inside")
})

test("backends: sim by default, native requires the SDK env", async () => {
  const auto = await createBackend({ MAGPIE_LAYERFS_HOME: mkdtempSync(join(tmpdir(), "layerfs-auto-")) })
  expect(auto).toBeInstanceOf(SimBackend)
  const native = await createBackend({ MAGPIE_LAYERFS_BACKEND: "native" })
  expect(native).toBeInstanceOf(NativeBackend)
  await expect(native.status()).rejects.toThrow("LAYERFS_SDK_PATH")
  const configured = await createBackend({ LAYERFS_SDK_PATH: "/x/index.js", LAYERFS_STORE_PATH: "/y/store.sqlite" })
  expect(configured).toBeInstanceOf(NativeBackend)
  await expect(createBackend({ MAGPIE_LAYERFS_BACKEND: "bogus" })).rejects.toThrow("auto, native or sim")
})

test("a workspace file survives on disk where resolve says it is", async () => {
  const { call } = await simTools()
  await call("provision", { project: "website" })
  const file = await call("resolve", { project: "website", path: "code.js" })
  expect(existsSync(file.path)).toBe(true)
})
