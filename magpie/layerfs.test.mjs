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

test("gives eight tools, each with a JSON Schema and an execute", async () => {
  const { tool } = await LayerfsTools({})
  expect(Object.keys(tool).sort()).toEqual(["checkpoint", "exec", "fork", "history", "provision", "publish", "resolve", "status"])
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

test("publish moves main forward to the branch head, as a numbered Layer", async () => {
  const { call } = await simTools()
  const ws = await call("provision", { project: "website" })
  await call("fork", { project: "website", branch: "feat" })
  const feat = await call("provision", { project: "website", branch: "feat" })
  writeFileSync(join(feat.path, "feature.js"), "export const f = 1\n")
  const cp = await call("checkpoint", { project: "website", branch: "feat", message: "add feature" })
  const pub = await call("publish", { project: "website", branch: "feat" })
  expect(pub.layer).toBe(1)
  expect(pub.commit).toBe(cp.commit)
  expect(pub.overwrote).toBe(false)
  const main = await call("history", { project: "website", branch: "main" })
  expect(main.commits[0].commit).toBe(cp.commit) // main now answers for the feature
  expect(main.commits[0].message).toBe("add feature")
  const again = await call("publish", { project: "website", branch: "feat" })
  expect(again.alreadyPublished).toBe(true)
  expect(ws.path).toBeTruthy()
})

test("publish refuses to drop a main that moved, unless forced", async () => {
  const { call } = await simTools()
  await call("provision", { project: "website" })
  await call("fork", { project: "website", branch: "feat" })
  const feat = await call("provision", { project: "website", branch: "feat" })
  writeFileSync(join(feat.path, "feature.js"), "f\n")
  await call("checkpoint", { project: "website", branch: "feat", message: "feature" })
  // main advances on its own after the fork: the branch no longer contains it
  const mainWs = await call("provision", { project: "website", branch: "main" })
  writeFileSync(join(mainWs.path, "hotfix.js"), "h\n")
  const hot = await call("checkpoint", { project: "website", branch: "main", message: "hotfix" })
  await expect(call("publish", { project: "website", branch: "feat" })).rejects.toThrow("main has moved")
  const forced = await call("publish", { project: "website", branch: "feat", force: true })
  expect(forced.overwrote).toBe(true)
  expect(forced.previousMain).toBe(hot.commit)
  await expect(call("publish", { project: "website", branch: "main" })).rejects.toThrow("other than main")
})

test("exec runs in the workspace and checkpoints what changed", async () => {
  const { call } = await simTools()
  await call("provision", { project: "website" })
  const before = await call("history", { project: "website" })
  const r = await call("exec", { project: "website", command: "echo built > build.log && echo hello" })
  expect(r.exitCode).toBe(0)
  expect(r.stdout).toContain("hello")
  expect(r.changed).toBe(true)
  expect(r.commit).toBeTruthy()
  const after = await call("history", { project: "website" })
  expect(after.commits.length).toBe(before.commits.length + 1)
  expect(after.commits[0].message).toContain("exec:")
  const noop = await call("exec", { project: "website", command: "true" })
  expect(noop.changed).toBe(false)
  expect(noop.commit).toBeNull()
  const failed = await call("exec", { project: "website", command: "echo x > partial.txt; exit 3" })
  expect(failed.exitCode).toBe(3)
  expect(failed.changed).toBe(true) // a failing command's edits are still history
})

// ---- native backend against the real SDK -------------------------------------
// Opt-in: these run only when MAGPIE_LAYERFS_NATIVE_TEST=1 and
// LAYERFS_SDK_PATH points at a layerfs checkout whose native binding is
// built. The backend root sits on the working disk (its store volume must
// carry SQLite locks), while the imported source is a tmpdir fixture —
// the engine refuses to import a source tree from overlayfs (upstream,
// Project(InvalidInput)), and tmpfs imports fine.
const NATIVE_SDK = process.env.LAYERFS_SDK_PATH // captured before the sim tests scrub the env
const nativeIt = process.env.MAGPIE_LAYERFS_NATIVE_TEST === "1" && NATIVE_SDK
  ? test : test.skip

nativeIt("native: provision imports, fork and checkpoint commit through the real store", async () => {
  process.env.LAYERFS_SDK_PATH = NATIVE_SDK
  const root = mkdtempSync(join(process.env.HOME, "workspace", ".tmp-native-test-"))
  const b = new NativeBackend(root)
  const src = fixture()
  writeFileSync(join(src, "bin.dat"), Buffer.from([0, 1, 2, 3, 255, 254]))
  const p = await b.provision("demo", "main", src)
  expect(p.backend).toBe("native")
  expect(p.mode).toBe("checkout")
  expect(p.projectCreated).toBe(true)
  // the import captured bin.dat, but the fs surface cannot read binary
  // files back, so the checkout names it instead of dropping it silently
  expect(p.skippedBinary).toContain("bin.dat")
  expect(readFileSync(join(p.path, "README.md"), "utf8")).toBe("hello layerfs\n")

  const f = await b.fork("demo", "feat")
  expect(f.copiedBytes).toBe(0)
  writeFileSync(join(f.path, "feat.txt"), "from the fork\n")
  const c = await b.checkpoint("demo", "feat", "add feat.txt")
  expect(c.changed).toBe(true)
  expect(c.added).toBe(1)
  const h = await b.history("demo", "feat")
  expect(h.commits[0].message).toBe("add feat.txt")
  const again = await b.checkpoint("demo", "feat", "nothing changed")
  expect(again.changed).toBe(false)
}, 60000)

nativeIt("native: publish appends a Layer, and the trajectory replays it", async () => {
  process.env.LAYERFS_SDK_PATH = NATIVE_SDK
  const root = mkdtempSync(join(process.env.HOME, "workspace", ".tmp-native-test-"))
  const b = new NativeBackend(root)
  await b.provision("site", "main", fixture())
  const f = await b.fork("site", "feat")
  writeFileSync(join(f.path, "feat.txt"), "shipped\n")
  await b.checkpoint("site", "feat", "feat work")
  const pub = await b.publish("site", "feat")
  expect(pub.layer).toBe(1)
  expect(pub.overwrote).toBe(false)
  const repub = await b.publish("site", "feat")
  expect(repub.alreadyPublished).toBe(true)
  const t = await b.trajectory()
  const proj = t.projects.find((x) => x.name === "site")
  expect(proj.branches.map((x) => x.name).sort()).toEqual(["feat", "main"])
  const main = proj.branches.find((x) => x.name === "main")
  expect(main.commits[0].message).toBe("Import") // the import Layer, synthesized
  expect(t.events.filter((e) => e.kind === "publish")).toHaveLength(1)
  const e = await b.exec("site", "feat", "echo via-exec > exec.txt")
  expect(e.exitCode).toBe(0)
  expect(e.changed).toBe(true)
  const st = await b.status()
  expect(st.projects[0].layers).toBe(2) // import + one publish
}, 60000)
