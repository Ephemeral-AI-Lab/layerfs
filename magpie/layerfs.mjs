// LayerFS backends for the magpie tools plugin.
//
// Two backends sit behind one small surface (provision, checkpoint, fork,
// history, resolve, status):
//
// - native: the real LayerFS, through the `@layerfs/sdk` JavaScript SDK
//   (Ephemeral-AI-Lab/layerfs, sdk/js). The SDK is not published to npm; it
//   ships in a layerfs checkout and needs its native binding plus a mounted
//   LayerFS store volume, so it is loaded dynamically from LAYERFS_SDK_PATH
//   and used only when MAGPIE_LAYERFS_BACKEND asks for it.
// - sim: a local content-addressed store that implements the same model
//   LayerFS is built on — immutable blobs named by their SHA-256, branches
//   as manifest pointers, checkpoints as deduplicated manifests — so the
//   plugin runs and can be tested anywhere. Forking a branch copies no
//   file bytes; a checkpoint stores only the blobs the store has not seen.
//
// Environment:
//   MAGPIE_LAYERFS_BACKEND  auto (default) | native | sim
//   MAGPIE_LAYERFS_HOME     state root for both backends (default ~/.magpie-layerfs)
//   LAYERFS_SDK_PATH        path to the SDK entry (sdk/js/src/index.js)
//   LAYERFS_STORE_PATH      native only, optional: the store file
//                           (<volume>/global/store.sqlite) anchoring the
//                           LayerFS volume; per-project stores live under
//                           <volume>/<project>/. Default: a volume under
//                           MAGPIE_LAYERFS_HOME/native/vol.
import { spawn } from "node:child_process"
import { createHash } from "node:crypto"
import {
  chmodSync, copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync,
  rmSync, statSync, writeFileSync,
} from "node:fs"
import { homedir } from "node:os"
import { dirname, join } from "node:path"
import { pathToFileURL } from "node:url"

const BINDING = ".layerfs.json"

// ---- names -------------------------------------------------------------------

// Project names follow the SDK's rule: 1-63 chars of lowercase ASCII letters,
// digits, `_`, `-`, `.`, starting and ending alphanumeric.
export function checkProject(name) {
  if (typeof name !== "string" || !/^[a-z0-9](?:[a-z0-9_.-]{0,61}[a-z0-9])?$/.test(name))
    throw new Error(`project is a LayerFS project name: 1-63 chars of a-z 0-9 _ - ., starting and ending with a letter or digit; got ${JSON.stringify(name)}`)
  return name
}

export function checkBranch(name) {
  if (typeof name !== "string" || !name || name.length > 100 || name.split("/").some((s) => !s || s === "." || s === ".." || !/^[A-Za-z0-9_.-]+$/.test(s)))
    throw new Error(`branch is a name like main or feat/x; got ${JSON.stringify(name)}`)
  return name
}

// A path inside a workspace: relative, and it must stay in.
export function checkInnerPath(p) {
  if (typeof p !== "string" || !p) throw new Error("path is a non-empty relative path")
  const norm = p.replace(/\\/g, "/").replace(/^\.\//, "")
  if (norm.startsWith("/") || norm.split("/").some((s) => s === ".."))
    throw new Error(`path must stay inside the workspace; got ${JSON.stringify(p)}`)
  return norm
}

const sha256 = (buf) => createHash("sha256").update(buf).digest("hex")
const nowIso = () => new Date().toISOString()

// Walk a directory into a manifest: { "rel/path": { sha, size } }.
function scanTree(dir, skip = new Set()) {
  const out = {}
  const walk = (d, rel) => {
    for (const e of readdirSync(d, { withFileTypes: true })) {
      const r = rel ? `${rel}/${e.name}` : e.name
      if (skip.has(r)) continue
      const full = join(d, e.name)
      if (e.isDirectory()) walk(full, r)
      else if (e.isFile()) {
        const buf = readFileSync(full)
        out[r] = { sha: sha256(buf), size: buf.length }
      }
    }
  }
  walk(dir, "")
  return out
}

function manifestBytes(manifest) {
  let n = 0
  for (const f of Object.values(manifest)) n += f.size
  return n
}

function sameManifest(a, b) {
  const ka = Object.keys(a)
  if (ka.length !== Object.keys(b).length) return false
  return ka.every((k) => b[k] && b[k].sha === a[k].sha)
}

const clip = (s, n) => (s.length > n ? s.slice(0, n) + `\n… [${s.length - n} more bytes]` : s)

// Run a shell command in dir, capturing bounded output. Resolves with
// { code, stdout, stderr, timedOut }; never rejects on a non-zero exit —
// the caller reports the code, as a shell would.
function runShell(command, dir, timeoutMs) {
  return new Promise((resolvePromise) => {
    const child = spawn("/bin/sh", ["-c", command], { cwd: dir })
    let out = "", err = "", timedOut = false
    const timer = setTimeout(() => { timedOut = true; child.kill("SIGKILL") }, timeoutMs)
    child.stdout.on("data", (d) => { if (out.length < 65536) out += d })
    child.stderr.on("data", (d) => { if (err.length < 65536) err += d })
    child.on("error", (e) => { clearTimeout(timer); resolvePromise({ code: -1, stdout: out, stderr: String(e), timedOut }) })
    child.on("close", (code) => { clearTimeout(timer); resolvePromise({ code: code ?? -1, stdout: clip(out, 8192), stderr: clip(err, 8192), timedOut }) })
  })
}

// ---- sim backend ---------------------------------------------------------------

export class SimBackend {
  kind = "sim"

  constructor(root) {
    this.root = root || process.env.MAGPIE_LAYERFS_HOME || join(homedir(), ".magpie-layerfs")
    this.objectsDir = join(this.root, "store", "objects")
    this.projectsDir = join(this.root, "projects")
    this.workspacesDir = join(this.root, "workspaces")
    for (const d of [this.objectsDir, this.projectsDir, this.workspacesDir]) mkdirSync(d, { recursive: true })
  }

  // -- store primitives
  blobPath(sha) { return join(this.objectsDir, sha.slice(0, 2), sha) }
  hasBlob(sha) { return existsSync(this.blobPath(sha)) }
  putBlob(buf) {
    const sha = sha256(buf)
    const p = this.blobPath(sha)
    if (!existsSync(p)) {
      mkdirSync(dirname(p), { recursive: true })
      writeFileSync(p, buf)
      return { sha, fresh: buf.length }
    }
    return { sha, fresh: 0 }
  }
  storedBytes() {
    let n = 0
    const walk = (d) => {
      for (const e of readdirSync(d, { withFileTypes: true })) {
        const full = join(d, e.name)
        if (e.isDirectory()) walk(full)
        else n += statSync(full).size
      }
    }
    walk(this.objectsDir)
    return n
  }
  // Store every blob of a manifest that the store has not seen; returns the
  // number of genuinely new bytes written.
  storeManifestFiles(baseDir, manifest) {
    let fresh = 0
    for (const [rel, f] of Object.entries(manifest)) {
      if (!this.hasBlob(f.sha)) fresh += this.putBlob(readFileSync(join(baseDir, ...rel.split("/")))).fresh
    }
    return fresh
  }

  // -- project / branch / commit records
  projectDir(project) { return join(this.projectsDir, project) }
  projectExists(project) { return existsSync(join(this.projectDir(project), "project.json")) }
  commitPath(project, id) { return join(this.projectDir(project), "commits", `${id}.json`) }
  readCommit(project, id) { return JSON.parse(readFileSync(this.commitPath(project, id), "utf8")) }
  writeCommit(project, commit) {
    mkdirSync(join(this.projectDir(project), "commits"), { recursive: true })
    writeFileSync(this.commitPath(project, commit.id), JSON.stringify(commit, null, 1))
  }
  branchHeadPath(project, branch) { return join(this.projectDir(project), "branches", `${branch}.json`) }
  branchExists(project, branch) { return existsSync(this.branchHeadPath(project, branch)) }
  head(project, branch) {
    if (!this.branchExists(project, branch)) throw new Error(`no branch ${branch} in project ${project}`)
    return JSON.parse(readFileSync(this.branchHeadPath(project, branch), "utf8")).commit
  }
  setHead(project, branch, commitId) {
    mkdirSync(dirname(this.branchHeadPath(project, branch)), { recursive: true })
    writeFileSync(this.branchHeadPath(project, branch), JSON.stringify({ commit: commitId }))
  }
  listBranches(project) {
    const dir = join(this.projectDir(project), "branches")
    if (!existsSync(dir)) return []
    return readdirSync(dir).filter((f) => f.endsWith(".json")).map((f) => f.slice(0, -5)).sort()
  }

  makeCommit(project, branch, parent, manifest, message, newBytes, delta = { added: 0, modified: 0, deleted: 0 }) {
    const seed = JSON.stringify({ parent, branch, message, manifest }) + nowIso() + Math.random()
    const id = sha256(Buffer.from(seed)).slice(0, 12)
    const commit = {
      id, branch, parent, message, time: nowIso(),
      manifest, files: Object.keys(manifest).length,
      logicalBytes: manifestBytes(manifest), newBytes, delta,
    }
    this.writeCommit(project, commit)
    this.setHead(project, branch, id)
    return commit
  }

  // An append-only event log (provision / fork / checkpoint), the raw
  // material for the trajectory the magpie page replays.
  recordEvent(event) {
    const path = join(this.root, "events.json")
    let events = []
    try { events = JSON.parse(readFileSync(path, "utf8")) } catch { /* first event */ }
    events.push({ t: nowIso(), ...event })
    if (events.length > 500) events = events.slice(-500)
    writeFileSync(path, JSON.stringify(events))
  }
  readEvents() {
    try { return JSON.parse(readFileSync(join(this.root, "events.json"), "utf8")) } catch { return [] }
  }

  async ensureProject(project, sourceDir) {
    if (this.projectExists(project)) return { created: false }
    mkdirSync(this.projectDir(project), { recursive: true })
    writeFileSync(join(this.projectDir(project), "project.json"),
      JSON.stringify({ name: project, created: nowIso(), backend: "sim" }))
    let manifest = {}
    let newBytes = 0
    if (sourceDir) {
      manifest = scanTree(sourceDir)
      newBytes = this.storeManifestFiles(sourceDir, manifest)
    }
    const commit = this.makeCommit(project, "main", null, manifest,
      sourceDir ? `Import ${sourceDir}` : "Empty project", newBytes,
      { added: Object.keys(manifest).length, modified: 0, deleted: 0 })
    return { created: true, commit: commit.id }
  }

  workspaceDir(project, branch) { return join(this.workspacesDir, `${project}--${branch.replace(/\//g, "__")}`) }

  async provision(project, branch = "main", sourceDir) {
    const created = await this.ensureProject(project, sourceDir)
    if (!this.branchExists(project, branch)) throw new Error(`no branch ${branch} in project ${project}`)
    const headId = this.head(project, branch)
    const dir = this.workspaceDir(project, branch)
    if (existsSync(join(dir, BINDING))) {
      const binding = JSON.parse(readFileSync(join(dir, BINDING), "utf8"))
      return {
        path: dir, project, branch, backend: this.kind,
        commit: binding.baseCommit, reused: true, stale: binding.baseCommit !== headId,
      }
    }
    const commit = this.readCommit(project, headId)
    mkdirSync(dir, { recursive: true })
    for (const [rel, f] of Object.entries(commit.manifest)) {
      const out = join(dir, ...rel.split("/"))
      mkdirSync(dirname(out), { recursive: true })
      copyFileSync(this.blobPath(f.sha), out)
    }
    writeFileSync(join(dir, BINDING), JSON.stringify({ project, branch, baseCommit: headId }, null, 1))
    this.recordEvent({ kind: "provision", project, branch, path: dir, commit: headId })
    return {
      path: dir, project, branch, backend: this.kind, commit: headId,
      reused: false, files: commit.files, logicalBytes: commit.logicalBytes,
      ...(created.created ? { projectCreated: true } : {}),
    }
  }

  async checkpoint(project, branch = "main", message = "") {
    const dir = this.workspaceDir(project, branch)
    if (!existsSync(join(dir, BINDING))) throw new Error(`no workspace for ${project}/${branch}; provision it first`)
    const parent = this.head(project, branch)
    const prev = this.readCommit(project, parent)
    const manifest = scanTree(dir, new Set([BINDING]))
    const newBytes = this.storeManifestFiles(dir, manifest)
    let added = 0, modified = 0, deleted = 0
    for (const [rel, f] of Object.entries(manifest)) {
      if (!prev.manifest[rel]) added++
      else if (prev.manifest[rel].sha !== f.sha) modified++
    }
    for (const rel of Object.keys(prev.manifest)) if (!manifest[rel]) deleted++
    const commit = this.makeCommit(project, branch, parent, manifest, message || "Checkpoint", newBytes, { added, modified, deleted })
    this.recordEvent({ kind: "checkpoint", project, branch, commit: commit.id, message: commit.message })
    writeFileSync(join(dir, BINDING), JSON.stringify({ project, branch, baseCommit: commit.id }, null, 1))
    const stored = this.storedBytes()
    return {
      commit: commit.id, parent, branch, message: commit.message,
      added, modified, deleted, files: commit.files,
      logicalBytes: commit.logicalBytes, newBytes,
      storedBytes: stored, savedBytes: commit.logicalBytes - newBytes,
    }
  }

  async fork(project, branch, from = "main") {
    if (!this.projectExists(project)) throw new Error(`no project ${project}`)
    if (this.branchExists(project, branch)) throw new Error(`branch ${branch} already exists in project ${project}`)
    const headId = this.head(project, from)
    this.setHead(project, branch, headId)
    this.recordEvent({ kind: "fork", project, branch, from, commit: headId })
    return { project, branch, from, commit: headId, copiedBytes: 0 }
  }

  // Publish a branch's head as main's new Layer. The guard is the lesson
  // of a real dsh run, where publishing from a stale branch replaced
  // main and dropped it from 238 tests to 38: by default main must be
  // an ancestor of the branch head, so publishing can only ever move
  // main forward. force says "I know main has commits this branch never
  // saw" — the result then reports what was overwritten.
  async publish(project, branch, { force = false } = {}) {
    checkProject(project); checkBranch(branch)
    if (branch === "main") throw new Error("publish names a branch other than main; main is what gets published to")
    const branchHead = this.head(project, branch)
    const mainHead = this.head(project, "main")
    const layers = this.readEvents().filter((e) => e.kind === "publish" && e.project === project).length
    if (branchHead === mainHead)
      return { project, branch, layer: layers, commit: branchHead, alreadyPublished: true, overwrote: false }
    const chain = new Set()
    let id = branchHead
    while (id) { chain.add(id); id = this.readCommit(project, id).parent }
    const fastForward = chain.has(mainHead)
    if (!fastForward && !force)
      throw new Error(`main has moved since ${branch} forked: main is at ${mainHead}, which is not in ${branch}'s history. Publishing would drop main's newer checkpoints. Fork a fresh branch from main, or pass force: true to overwrite deliberately.`)
    this.setHead(project, "main", branchHead)
    const layer = layers + 1
    this.recordEvent({ kind: "publish", project, branch, commit: branchHead, layer, overwrote: !fastForward })
    return { project, branch, layer, commit: branchHead, previousMain: mainHead, overwrote: !fastForward }
  }

  // Run a command in the branch's workspace and checkpoint whatever it
  // changed as one commit — the dsh shape, where every bash call is
  // mount, run, commit. A command that changes nothing commits nothing;
  // a failing command's changes are still checkpointed (they are
  // history, and history can be rewound), with its exit code reported.
  async exec(project, branch = "main", command, message = "", timeoutMs = 50000) {
    checkProject(project); checkBranch(branch)
    if (typeof command !== "string" || !command.trim()) throw new Error("command is a non-empty string")
    const dir = this.workspaceDir(project, branch)
    if (!existsSync(join(dir, BINDING))) throw new Error(`no workspace for ${project}/${branch}; provision it first`)
    const prev = this.readCommit(project, this.head(project, branch))
    const r = await runShell(command, dir, Math.min(Math.max(100, timeoutMs || 50000), 55000))
    const manifest = scanTree(dir, new Set([BINDING]))
    const changed = !sameManifest(prev.manifest, manifest)
    let commit = null
    if (changed) {
      const c = await this.checkpoint(project, branch, message || `exec: ${command.trim().slice(0, 60)}`)
      commit = c.commit
    }
    return {
      project, branch, exitCode: r.code, ...(r.timedOut ? { timedOut: true } : {}),
      stdout: r.stdout, stderr: r.stderr, changed, commit,
    }
  }

  async history(project, branch = "main", limit = 20) {
    let id = this.head(project, branch)
    const out = []
    while (id && out.length < limit) {
      const c = this.readCommit(project, id)
      out.push({
        commit: c.id, message: c.message, time: c.time,
        files: c.files, logicalBytes: c.logicalBytes, newBytes: c.newBytes,
      })
      id = c.parent
    }
    return { project, branch, commits: out }
  }

  // The whole recorded trajectory, oldest first per branch: what the
  // magpie page replays. Commits carry their delta and byte accounting.
  async trajectory() {
    const projects = []
    if (existsSync(this.projectsDir)) {
      for (const name of readdirSync(this.projectsDir).sort()) {
        if (!this.projectExists(name)) continue
        const branches = []
        for (const branch of this.listBranches(name)) {
          // the branch's OWN checkpoints: walk its whole chain (a publish
          // may have moved main's head onto another branch's work) and
          // keep only the commits made on this branch — ancestors belong
          // to the bough they were committed on
          const commits = []
          let id = this.head(name, branch)
          while (id) {
            const c = this.readCommit(name, id)
            if (c.branch === branch) {
              commits.push({
                id: c.id, parent: c.parent, message: c.message, time: c.time,
                files: c.files, logicalBytes: c.logicalBytes, newBytes: c.newBytes,
                delta: c.delta ?? { added: 0, modified: 0, deleted: 0 },
              })
            }
            id = c.parent
          }
          branches.push({ name: branch, commits: commits.reverse() })
        }
        projects.push({ name, branches })
      }
    }
    const st = await this.status()
    return {
      generatedAt: nowIso(), backend: this.kind, projects,
      events: this.readEvents(),
      totals: {
        logicalBytes: st.logicalBytes, storedBytes: st.storedBytes,
        savedBytes: st.savedBytes, dedupRatio: st.dedupRatio, workspaces: st.workspaces,
      },
    }
  }

  async resolve(project, branch = "main", inner = "") {
    const headId = this.head(project, branch)
    const dir = this.workspaceDir(project, branch)
    if (!existsSync(join(dir, BINDING)))
      return { path: null, project, branch, commit: headId, provisioned: false, hint: "call provision to materialize this workspace" }
    const out = { path: inner ? join(dir, ...inner.split("/")) : dir, project, branch, commit: headId, provisioned: true }
    if (inner) {
      const commit = this.readCommit(project, headId)
      const f = commit.manifest[inner]
      if (f) { out.sha = f.sha; out.size = f.size; out.inHead = true }
      else out.inHead = existsSync(out.path)
    }
    return out
  }

  async status() {
    const projects = []
    let logical = 0
    if (existsSync(this.projectsDir)) {
      for (const name of readdirSync(this.projectsDir).sort()) {
        if (!this.projectExists(name)) continue
        const branches = this.listBranches(name)
        let commits = 0
        const cdir = join(this.projectDir(name), "commits")
        if (existsSync(cdir)) {
          const files = readdirSync(cdir)
          commits = files.length
          for (const f of files) logical += JSON.parse(readFileSync(join(cdir, f), "utf8")).logicalBytes
        }
        projects.push({ name, branches, commits })
      }
    }
    const stored = this.storedBytes()
    const workspaces = existsSync(this.workspacesDir) ? readdirSync(this.workspacesDir).length : 0
    return {
      backend: this.kind, root: this.root, projects, workspaces,
      logicalBytes: logical, storedBytes: stored,
      savedBytes: Math.max(0, logical - stored),
      dedupRatio: stored ? +(logical / stored).toFixed(2) : 1,
    }
  }
}

// ---- native backend --------------------------------------------------------------
//
// The real LayerFS, adapted to what the SDK actually does in-process
// (verified against a locally built binding, layerfs 1.0.0, 2026-10-22):
//
// - Store calls (project/branch/fs) run in this process against a global
//   SQLite store per project; project entries live at LAYERFS_HOME/<name>,
//   each project's store at <volume>/<project>/global/store.sqlite, where
//   <volume> is derived from LAYERFS_STORE_PATH. The store's filesystem
//   must carry SQLite locks (ext4, xfs or btrfs; tmpfs is refused).
// - project.init imports a source tree as main's first Layer. A source on
//   overlayfs is refused by the engine with an opaque Project(InvalidInput)
//   (reported upstream); other filesystems import fine.
// - workspace.* calls (mount/exec/commit) need a deployment (Docker,
//   LAYERFS_IMAGE, a daemon) that a magpie host does not have, so this
//   backend works in checkout mode instead: provision materializes a
//   branch into a local directory through fs.list/fs.read, and checkpoint
//   diffs that directory against the branch and lands the changes as ONE
//   fs.apply commit — the same shape as a workspace commit, through the
//   store surface.
// - The fs surface is text-only in practice: fs.read refuses binary files
//   (error code Binary) and no JavaScript value reaches the binding as
//   bytes for fs.write/fs.apply, so binary files enter a project only via
//   the initial import. A checkout lists the binary files it could not
//   materialize, and a checkpoint lists the binary files it could not
//   commit, instead of silently dropping them.
// - One process holds one open store per project name (refcounted by
//   the binding; the last client closes it), and a client captures
//   LAYERFS_HOME and its storePath at construction — so one backend
//   keeps one client per project, built after the env is set.
// - Publish is a Layer operation: branch.publish(branch, head) appends
//   the branch tip's tree as main's next Layer. The plugin guards it the
//   same way as the sim backend: the branch must have forked from main's
//   current top Layer, or the caller must force.

const isTextBuf = (buf) => !buf.includes(0) && Buffer.from(buf.toString("utf8"), "utf8").equals(buf)

export class NativeBackend {
  kind = "native"

  constructor(root) {
    this.root = root || process.env.MAGPIE_LAYERFS_HOME || join(homedir(), ".magpie-layerfs")
    this.nativeRoot = join(this.root, "native")
    this.homeDir = join(this.nativeRoot, "home") // LAYERFS_HOME for the SDK
    const envStore = process.env.LAYERFS_STORE_PATH
    this.volDir = envStore ? dirname(dirname(envStore)) : join(this.nativeRoot, "vol")
    this.storePathEnv = envStore || join(this.volDir, "global", "store.sqlite")
    this.workspacesDir = join(this.nativeRoot, "workspaces")
    this.stateDir = join(this.nativeRoot, "projects")
    this.clients = new Map() // project -> SDK client
    for (const d of [this.homeDir, join(this.volDir, "global"), this.workspacesDir, this.stateDir])
      mkdirSync(d, { recursive: true })
    // The engine refuses a LAYERFS_HOME (and store directories) that are
    // not private to the user: "must be owned by this user and private".
    for (const d of [this.homeDir, this.volDir, join(this.volDir, "global")])
      chmodSync(d, 0o700)
  }

  async sdk() {
    const entry = process.env.LAYERFS_SDK_PATH
    if (!entry) throw new Error(
      "native backend needs LAYERFS_SDK_PATH pointing at a layerfs checkout's sdk/js/src/index.js " +
      "(the @layerfs/sdk package is not on npm; it ships with Ephemeral-AI-Lab/layerfs), " +
      "with its native binding built (sdk/js/native/layerfs.node; see the layerfs repo's build-native script)")
    if (!this._sdk) {
      const { resolve } = await import("node:path")
      this._sdk = await import(pathToFileURL(resolve(entry)).href)
    }
    return this._sdk
  }

  async client(project) {
    if (!this.clients.has(project)) {
      const { LayerFS } = await this.sdk()
      // The SDK reads both variables when a client is constructed; the
      // store path it is handed installs that project's own store file
      // under the shared volume.
      process.env.LAYERFS_HOME = this.homeDir
      process.env.LAYERFS_STORE_PATH = this.storePathEnv
      const storePath = join(this.volDir, project, "global", "store.sqlite")
      mkdirSync(dirname(storePath), { recursive: true })
      chmodSync(join(this.volDir, project), 0o700)
      chmodSync(dirname(storePath), 0o700)
      this.clients.set(project, new LayerFS(project, { storePath }))
    }
    return this.clients.get(project)
  }

  // -- plugin-side records (import layer, publishes): the engine keeps
  // layers, not a publish log, so the plugin records its own publishes.
  statePath(project) { return join(this.stateDir, `${project}.json`) }
  readState(project) {
    try { return JSON.parse(readFileSync(this.statePath(project), "utf8")) }
    catch { return { importLayer: null, publishes: [] } }
  }
  writeState(project, state) {
    mkdirSync(this.stateDir, { recursive: true })
    writeFileSync(this.statePath(project), JSON.stringify(state, null, 1))
  }

  workspaceDir(project, branch) { return join(this.workspacesDir, `${project}--${branch.replace(/\//g, "__")}`) }
  readBinding(dir) {
    try { return JSON.parse(readFileSync(join(dir, BINDING), "utf8")) } catch { return null }
  }

  // -- paginated SDK reads
  async branchMap(lfs) {
    const map = new Map()
    let pos = 0
    for (;;) {
      const r = await lfs.project.list_branches(200, pos)
      for (const b of r.items) map.set(b.name, b)
      if (!r.hasMore) return map
      pos = r.nextPagePos
    }
  }

  // Newest first, as the SDK returns them; a branch with no commits (a
  // fresh main is a Layer stack, not a commit chain) reads as empty.
  async commits(lfs, branch, limit = 1000) {
    const out = []
    let pos = 0
    try {
      for (;;) {
        const r = await lfs.branch.list_commits(branch, 200, pos)
        out.push(...r.items)
        if (!r.hasMore || out.length >= limit) return out.slice(0, limit)
        pos = r.nextPagePos
      }
    } catch (e) {
      if (e?.code === "NoSuchBranch") return out
      throw e
    }
  }

  async listAll(lfs, branch, path) {
    const items = []
    let pos = 0
    for (;;) {
      const r = await lfs.fs.list(branch, path, 200, pos)
      items.push(...r.items)
      if (!r.hasMore) return items
      pos = r.nextPagePos
    }
  }

  // The branch's tree: Map "/rel/path" -> { type, size }.
  async remoteTree(lfs, branch) {
    const out = new Map()
    const walk = async (dir) => {
      for (const item of await this.listAll(lfs, branch, dir)) {
        if (item.type === "dir") { out.set(item.path, { type: "dir" }); await walk(item.path) }
        else {
          const st = await lfs.fs.stat(branch, item.path)
          out.set(item.path, { type: item.type, size: st.size })
        }
      }
    }
    await walk("/")
    return out
  }

  // Full text of a store file, following the line pagination; null when
  // the file is binary (the fs surface refuses those with code Binary).
  async readText(lfs, branch, path) {
    try {
      let out = "", start = 1
      for (;;) {
        const r = await lfs.fs.read(branch, path, start)
        out += r.text
        if (r.nextLine == null) return out
        start = r.nextLine
      }
    } catch (e) {
      if (e?.code === "Binary") return null
      throw e
    }
  }

  async ensureProject(project, sourceDir) {
    const lfs = await this.client(project)
    const existing = await this.branchMap(lfs).catch(() => new Map())
    if (existing.size) return { lfs, created: false }
    if (!sourceDir) throw new Error(
      `project ${project} is not initialized in LayerFS yet; provision it once with a source directory to import`)
    let result
    try {
      result = await lfs.project.init(sourceDir)
    } catch (e) {
      if (e?.code === "Exists") return { lfs, created: false }
      if (e?.code === "Failed" && /InvalidInput/.test(String(e?.message ?? ""))) throw new Error(
        `LayerFS refused to import ${sourceDir}: the engine rejects this source directory ` +
        `(known upstream: a source tree on overlayfs fails with Project(InvalidInput) and no further detail). ` +
        `Import from a plain ext4/xfs/btrfs or tmpfs directory instead. Raw error: ${String(e.message).slice(0, 200)}`)
      throw e
    }
    const state = this.readState(project)
    state.importLayer = result.layer
    this.writeState(project, state)
    return { lfs, created: true }
  }

  // Materialize a branch into its checkout directory through the fs
  // surface. Binary files cannot be read back (see the header), so they
  // are named in the binding and the result, never silently dropped.
  async materialize(project, branch, lfs, head) {
    const dir = this.workspaceDir(project, branch)
    const tree = await this.remoteTree(lfs, branch)
    const skippedBinary = []
    rmSync(dir, { recursive: true, force: true })
    mkdirSync(dir, { recursive: true })
    let files = 0
    for (const [path, entry] of [...tree].sort()) {
      const local = join(dir, ...path.split("/").filter(Boolean))
      if (entry.type === "dir") { mkdirSync(local, { recursive: true }); continue }
      if (entry.type !== "file") { skippedBinary.push(path.slice(1)); continue }
      const text = await this.readText(lfs, branch, path)
      if (text === null) { skippedBinary.push(path.slice(1)); continue }
      mkdirSync(dirname(local), { recursive: true })
      writeFileSync(local, text)
      files++
    }
    writeFileSync(join(dir, BINDING), JSON.stringify(
      { project, branch, backend: "native", mode: "checkout", head, skippedBinary }, null, 1))
    return { dir, files, skippedBinary }
  }

  async provision(project, branch = "main", sourceDir) {
    checkProject(project); checkBranch(branch)
    const { lfs, created } = await this.ensureProject(project, sourceDir)
    const branches = await this.branchMap(lfs)
    const info = branches.get(branch)
    if (!info) throw new Error(`no branch ${branch} in project ${project}; fork it first`)
    const dir = this.workspaceDir(project, branch)
    const binding = this.readBinding(dir)
    if (binding && binding.head === info.head && binding.mode === "checkout")
      return {
        path: dir, project, branch, backend: this.kind, mode: "checkout",
        commit: info.head, reused: true, skippedBinary: binding.skippedBinary ?? [],
      }
    const m = await this.materialize(project, branch, lfs, info.head)
    return {
      path: dir, project, branch, backend: this.kind, mode: "checkout",
      commit: info.head, reused: false, files: m.files, skippedBinary: m.skippedBinary,
      ...(created ? { projectCreated: true } : {}),
    }
  }

  async checkpoint(project, branch = "main", message = "") {
    checkProject(project); checkBranch(branch)
    const dir = this.workspaceDir(project, branch)
    const binding = this.readBinding(dir)
    if (!binding) throw new Error(`no workspace for ${project}/${branch}; provision it first`)
    const lfs = await this.client(project)
    const local = scanTree(dir, new Set([BINDING]))
    const remote = await this.remoteTree(lfs, branch)
    const changes = [], skippedBinary = []
    let added = 0, modified = 0, deleted = 0
    const remoteFiles = new Map([...remote].filter(([, e]) => e.type === "file")
      .map(([p, e]) => [p.slice(1), e]))
    // directories the branch does not have yet
    const localDirs = new Set()
    for (const rel of Object.keys(local)) {
      const parts = rel.split("/")
      for (let i = 1; i < parts.length; i++) localDirs.add(parts.slice(0, i).join("/"))
    }
    for (const d of [...localDirs].sort())
      if (!remote.has(`/${d}`)) changes.push({ op: "mkdir", path: `/${d}` })
    for (const [rel, f] of Object.entries(local)) {
      const buf = readFileSync(join(dir, ...rel.split("/")))
      const r = remoteFiles.get(rel)
      if (!isTextBuf(buf)) {
        // binary content cannot be written through the fs surface; an
        // unchanged size against a store copy counts as unchanged
        if (!r || r.size !== f.size) skippedBinary.push(rel)
        continue
      }
      const text = buf.toString("utf8")
      if (!r) { changes.push({ op: "write", path: `/${rel}`, content: text }); added++; continue }
      const remoteText = await this.readText(lfs, branch, `/${rel}`)
      if (remoteText === null) { if (r.size !== f.size) { changes.push({ op: "write", path: `/${rel}`, content: text }); modified++ } continue }
      if (remoteText !== text) { changes.push({ op: "write", path: `/${rel}`, content: text }); modified++ }
    }
    for (const rel of remoteFiles.keys())
      if (!local[rel]) { changes.push({ op: "rm", path: `/${rel}` }); deleted++ }
    const headBefore = (await this.branchMap(lfs)).get(branch)?.head ?? null
    if (!changes.length)
      return {
        project, branch, commit: headBefore, changed: false,
        added: 0, modified: 0, deleted: 0, skippedBinary,
        note: "nothing to commit (UpToDate)",
      }
    const result = await lfs.fs.apply(branch, changes, message || "Checkpoint")
    const c = result.commit
    writeFileSync(join(dir, BINDING), JSON.stringify(
      { ...binding, head: c.id, skippedBinary }, null, 1))
    return {
      project, branch, commit: c.id, parent: c.parent, changed: true,
      message: c.message, added, modified, deleted,
      logicalBytes: c.logical, newBytes: (c.newStored ?? 0) + (c.newMetadata ?? 0),
      skippedBinary,
    }
  }

  async fork(project, branch, from = "main") {
    checkProject(project); checkBranch(branch); checkBranch(from)
    const { lfs } = await this.ensureProject(project)
    const branches = await this.branchMap(lfs)
    if (branches.has(branch)) throw new Error(`branch ${branch} already exists in project ${project}`)
    if (!branches.has(from)) throw new Error(`no branch ${from} in project ${project}`)
    await lfs.branch.fork(branch, from)
    const p = await this.provision(project, branch)
    return { project, branch, from, commit: p.commit, copiedBytes: 0, path: p.path, skippedBinary: p.skippedBinary }
  }

  // Publish appends the branch tip's tree as main's next Layer. Guard:
  // the branch must have forked from main's current top Layer — a stale
  // branch's tree would silently replace newer work on main, the same
  // failure the sim backend guards against in commit terms.
  async publish(project, branch, { force = false } = {}) {
    checkProject(project); checkBranch(branch)
    if (branch === "main") throw new Error("publish names a branch other than main; main is what gets published to")
    const { lfs } = await this.ensureProject(project)
    const branches = await this.branchMap(lfs)
    const info = branches.get(branch)
    if (!info) throw new Error(`no branch ${branch} in project ${project}`)
    if (!info.head) throw new Error(`branch ${branch} has no commits to publish`)
    const state = this.readState(project)
    if (state.publishes.some((p) => p.commit === info.head))
      return { project, branch, commit: info.head, alreadyPublished: true, overwrote: false }
    const mainTop = branches.get("main")?.base ?? null
    const stale = info.forkedFrom != null && mainTop != null && info.forkedFrom !== mainTop
    if (stale && !force)
      throw new Error(`main has moved since ${branch} forked: main's top Layer is ${mainTop}, but ${branch} forked from ${info.forkedFrom}. Publishing would replace main's newer Layer content. Fork a fresh branch from main, or pass force: true to overwrite deliberately.`)
    const result = await lfs.branch.publish(branch, info.head)
    const layer = state.publishes.length + 1
    const overwrote = stale || (result.publishedLayer?.overwroteCount ?? 0) > 0
    state.publishes.push({
      branch, commit: info.head, layer, layerId: result.publishedLayer?.id ?? null,
      parent: result.publishedLayer?.parent ?? null, overwrote, time: nowIso(),
    })
    this.writeState(project, state)
    return {
      project, branch, layer, commit: info.head,
      previousMain: result.publishedLayer?.parent ?? mainTop, overwrote, engine: result,
    }
  }

  async exec(project, branch = "main", command, message = "", timeoutMs = 50000) {
    checkProject(project); checkBranch(branch)
    if (typeof command !== "string" || !command.trim()) throw new Error("command is a non-empty string")
    const dir = this.workspaceDir(project, branch)
    if (!this.readBinding(dir)) throw new Error(`no workspace for ${project}/${branch}; provision it first`)
    const r = await runShell(command, dir, Math.min(Math.max(100, timeoutMs || 50000), 55000))
    const c = await this.checkpoint(project, branch, message || `exec: ${command.trim().slice(0, 60)}`)
    return {
      project, branch, exitCode: r.code, ...(r.timedOut ? { timedOut: true } : {}),
      stdout: r.stdout, stderr: r.stderr,
      changed: c.changed === true, commit: c.changed ? c.commit : null,
      ...(c.skippedBinary?.length ? { skippedBinary: c.skippedBinary } : {}),
    }
  }

  async history(project, branch = "main", limit = 20) {
    checkProject(project); checkBranch(branch)
    const { lfs } = await this.ensureProject(project)
    const commits = await this.commits(lfs, branch, limit)
    return {
      project, branch,
      commits: commits.map((c) => ({
        commit: c.id, message: c.message, time: new Date(c.time).toISOString(),
        logicalBytes: c.logical, newBytes: (c.newStored ?? 0) + (c.newMetadata ?? 0),
      })),
    }
  }

  async resolve(project, branch = "main", inner = "") {
    checkProject(project); checkBranch(branch)
    const { lfs } = await this.ensureProject(project)
    const branches = await this.branchMap(lfs)
    const info = branches.get(branch)
    if (!info) throw new Error(`no branch ${branch} in project ${project}`)
    const dir = this.workspaceDir(project, branch)
    const provisioned = !!this.readBinding(dir)
    const out = {
      path: provisioned ? (inner ? join(dir, ...inner.split("/")) : dir) : null,
      project, branch, commit: info.head, provisioned,
      ...(provisioned ? {} : { hint: "call provision to materialize this workspace" }),
    }
    if (inner) {
      inner = checkInnerPath(inner)
      try {
        const st = await lfs.fs.stat(branch, `/${inner}`)
        out.inHead = true; out.size = st.size; out.type = st.type; out.root = st.root
      } catch { out.inHead = false }
    }
    return out
  }

  async projectNames() {
    const names = new Set()
    if (existsSync(this.stateDir))
      for (const f of readdirSync(this.stateDir)) if (f.endsWith(".json")) names.add(f.slice(0, -5))
    if (existsSync(this.homeDir))
      for (const e of readdirSync(this.homeDir, { withFileTypes: true })) if (e.isDirectory()) names.add(e.name)
    return [...names].sort()
  }

  async status() {
    await this.sdk() // surfaces the configuration errors, if any
    const projects = []
    let logical = 0, stored = 0
    for (const name of await this.projectNames()) {
      const lfs = await this.client(name)
      const branches = await this.branchMap(lfs).catch(() => new Map())
      let commits = 0
      for (const b of branches.keys()) {
        const cs = await this.commits(lfs, b).catch(() => [])
        commits += cs.length
        for (const c of cs) { logical += c.logical ?? 0; stored += (c.newStored ?? 0) + (c.newMetadata ?? 0) }
      }
      const state = this.readState(name)
      projects.push({
        name, branches: [...branches.keys()], commits,
        layers: 1 + state.publishes.length,
      })
    }
    const workspaces = existsSync(this.workspacesDir) ? readdirSync(this.workspacesDir).length : 0
    return {
      backend: this.kind, root: this.nativeRoot, storePath: this.storePathEnv,
      projects, workspaces,
      logicalBytes: logical, storedBytes: stored,
      savedBytes: Math.max(0, logical - stored),
      dedupRatio: stored ? +(logical / stored).toFixed(2) : 1,
      note: "byte totals sum the branch commits; the initial import layer is not a commit and is not counted",
    }
  }

  // Same shape as the sim backend's trajectory, from the engine's own
  // records: commits per branch (the import is a Layer, synthesized as
  // main's first node from the recorded import), publishes from the
  // plugin's own publish log with their Layer numbers.
  async trajectory() {
    const projects = []
    for (const name of await this.projectNames()) {
      const lfs = await this.client(name)
      const branches = await this.branchMap(lfs).catch(() => new Map())
      const state = this.readState(name)
      const out = []
      for (const b of branches.keys()) {
        const cs = await this.commits(lfs, b).catch(() => [])
        const commits = cs.reverse().map((c) => ({
          id: c.id, parent: c.parent, message: c.message,
          time: new Date(c.time).toISOString(), files: null,
          logicalBytes: c.logical ?? 0, newBytes: (c.newStored ?? 0) + (c.newMetadata ?? 0),
          delta: { added: 0, modified: 0, deleted: 0 },
        }))
        if (b === "main" && state.importLayer)
          commits.unshift({
            id: state.importLayer, parent: null, message: "Import", time: null,
            files: null, logicalBytes: 0, newBytes: 0, delta: { added: 0, modified: 0, deleted: 0 },
          })
        out.push({ name: b, commits })
      }
      projects.push({ name, branches: out })
    }
    const st = await this.status()
    const events = []
    for (const name of await this.projectNames())
      for (const p of this.readState(name).publishes)
        events.push({ t: p.time, kind: "publish", project: name, branch: p.branch, commit: p.commit, layer: p.layer, overwrote: p.overwrote })
    return {
      generatedAt: nowIso(), backend: this.kind, projects, events,
      totals: {
        logicalBytes: st.logicalBytes, storedBytes: st.storedBytes,
        savedBytes: st.savedBytes, dedupRatio: st.dedupRatio, workspaces: st.workspaces,
      },
    }
  }
}

// ---- selection ---------------------------------------------------------------------

export async function createBackend(env = process.env) {
  const want = (env.MAGPIE_LAYERFS_BACKEND || "auto").toLowerCase()
  if (want === "sim") return new SimBackend(env.MAGPIE_LAYERFS_HOME)
  if (want === "native") return new NativeBackend()
  if (want !== "auto") throw new Error(`MAGPIE_LAYERFS_BACKEND is auto, native or sim; got ${JSON.stringify(want)}`)
  // auto: native when the SDK is configured, sim otherwise (the native
  // backend derives its store volume itself; LAYERFS_STORE_PATH only
  // anchors it elsewhere when set)
  if (env.LAYERFS_SDK_PATH) return new NativeBackend(env.MAGPIE_LAYERFS_HOME)
  return new SimBackend(env.MAGPIE_LAYERFS_HOME)
}
