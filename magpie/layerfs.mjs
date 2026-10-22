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
//   MAGPIE_LAYERFS_HOME     sim store root (default ~/.magpie-layerfs)
//   LAYERFS_SDK_PATH        path to the SDK entry (sdk/js/src/index.js)
//   LAYERFS_STORE_PATH      native store file (global/store.sqlite)
import { createHash } from "node:crypto"
import {
  copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync,
  statSync, writeFileSync,
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

  makeCommit(project, branch, parent, manifest, message, newBytes) {
    const seed = JSON.stringify({ parent, branch, message, manifest }) + nowIso() + Math.random()
    const id = sha256(Buffer.from(seed)).slice(0, 12)
    const commit = {
      id, branch, parent, message, time: nowIso(),
      manifest, files: Object.keys(manifest).length,
      logicalBytes: manifestBytes(manifest), newBytes,
    }
    this.writeCommit(project, commit)
    this.setHead(project, branch, id)
    return commit
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
      sourceDir ? `Import ${sourceDir}` : "Empty project", newBytes)
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
    const commit = this.makeCommit(project, branch, parent, manifest, message || "Checkpoint", newBytes)
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
    return { project, branch, from, commit: headId, copiedBytes: 0 }
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

export class NativeBackend {
  kind = "native"

  constructor() {
    this.clients = new Map() // project -> SDK client
    this.mounts = new Map() // project\0branch -> workspace handle
  }

  async sdk() {
    const entry = process.env.LAYERFS_SDK_PATH
    if (!entry) throw new Error(
      "native backend needs LAYERFS_SDK_PATH pointing at a layerfs checkout's sdk/js/src/index.js " +
      "(the @layerfs/sdk package is not on npm; it ships with Ephemeral-AI-Lab/layerfs)")
    if (!process.env.LAYERFS_STORE_PATH) throw new Error(
      "native backend needs LAYERFS_STORE_PATH pointing at the LayerFS store file (global/store.sqlite on the mounted LayerFS volume)")
    if (!this._sdk) {
      const { resolve } = await import("node:path")
      this._sdk = await import(pathToFileURL(resolve(entry)).href)
    }
    return this._sdk
  }

  async client(project) {
    if (!this.clients.has(project)) {
      const { LayerFS } = await this.sdk()
      this.clients.set(project, new LayerFS(project, { storePath: process.env.LAYERFS_STORE_PATH }))
    }
    return this.clients.get(project)
  }

  async provision(project, branch = "main", sourceDir) {
    const lfs = await this.client(project)
    try { await lfs.project.init(sourceDir ?? ".") } catch (e) {
      if (!/Exist/i.test(String(e?.message ?? e))) throw e
    }
    const branches = await lfs.project.list_branches()
    if (!branches.includes(branch)) await lfs.branch.fork(branch)
    const ws = await lfs.workspace.mount(branch)
    this.mounts.set(`${project}\0${branch}`, ws)
    return { path: ws.path, project, branch, backend: this.kind, sandbox: ws.sandbox, reused: false }
  }

  async checkpoint(project, branch = "main", message = "") {
    const lfs = await this.client(project)
    const ws = this.mounts.get(`${project}\0${branch}`)
    if (!ws) throw new Error(`no mounted workspace for ${project}/${branch} in this process; provision it first`)
    await lfs.workspace.commit(ws, message || "Checkpoint")
    const page = await lfs.branch.list_commits(branch, 1, 0)
    return { project, branch, message: message || "Checkpoint", latest: page?.[0] ?? null }
  }

  async fork(project, branch, from = "main") {
    const lfs = await this.client(project)
    await lfs.branch.fork(branch) // the SDK forks the current branch lineage
    return { project, branch, from }
  }

  async history(project, branch = "main", limit = 20) {
    const lfs = await this.client(project)
    const commits = await lfs.branch.list_commits(branch, limit, 0)
    return { project, branch, commits }
  }

  async resolve(project, branch = "main", inner = "") {
    const ws = this.mounts.get(`${project}\0${branch}`)
    if (!ws) return { path: null, project, branch, provisioned: false, hint: "call provision to mount this workspace" }
    return { path: inner ? join(ws.path, ...inner.split("/")) : ws.path, project, branch, provisioned: true }
  }

  async status() {
    await this.sdk() // surfaces the configuration errors, if any
    return { backend: this.kind, storePath: process.env.LAYERFS_STORE_PATH, mounted: [...this.mounts.keys()].map((k) => k.replace("\0", "/")) }
  }
}

// ---- selection ---------------------------------------------------------------------

export async function createBackend(env = process.env) {
  const want = (env.MAGPIE_LAYERFS_BACKEND || "auto").toLowerCase()
  if (want === "sim") return new SimBackend(env.MAGPIE_LAYERFS_HOME)
  if (want === "native") return new NativeBackend()
  if (want !== "auto") throw new Error(`MAGPIE_LAYERFS_BACKEND is auto, native or sim; got ${JSON.stringify(want)}`)
  // auto: native when the SDK is configured, sim otherwise
  if (env.LAYERFS_SDK_PATH && env.LAYERFS_STORE_PATH) return new NativeBackend()
  return new SimBackend(env.MAGPIE_LAYERFS_HOME)
}
