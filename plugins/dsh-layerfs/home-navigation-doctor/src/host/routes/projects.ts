/**
 * The host's project reads and changes on LayerFS 2.0. Port of Connect
 * `host/projects.js` and `host/project-reads.js` (without demo-run coupling:
 * a project's `demo`/`run` are always null here).
 *
 * Every project is `<store>/<project>` and every call names it.
 *   status        the linked LayerFS, the home, its machines and Stores
 *   list          project.list(), each with its Store's state
 *   create/init   project.init(key, folder, { on_progress }) into an existing
 *                 Store; the host keeps its own table of the Inits it started
 *   state         branch.list metadata, sandbox.list and store.get: the
 *                 Store's facts, no Sandbox asked
 *   sandboxes     sandbox.list; Workspaces are not read (`unread`), the client asks
 *                 GET /api/sandboxes/:name (sandbox.get) for each it shows
 *   createSandbox sandbox.create(key, name, { machine, image }), after
 *                 machine.start with `start`, as a job with `background`
 *   closeSandbox  sandbox.remove(name, { force })
 *   commits       commit.list, newest first, each Commit's changed paths from
 *                 commit.diff against its parent
 *   listDir/file  file.list, file.stat and file.read (pinned with --at)
 *   fileQuery     file.blame (+ file.read for the lines), file.history, and
 *                 commit.diff for a path or a Branch
 *   remove        project.delete
 * Every LayerFS call is made once; a refusal passes through with its code.
 * Saved reads are pinned (`--at`) and paged by cursor. Store allocation and
 * project tree statistics are independent queries with separate scopes.
 */
import { randomUUID } from 'node:crypto'
import { mkdirSync, mkdtempSync, realpathSync, rmSync, statSync } from 'node:fs'
import { isAbsolute, join, relative, sep } from 'node:path'
import { StorageObservations } from './storage-observations.ts'
import { ProjectStorageLedgers } from './project-storage.ts'
import { lastUsed, pgPlacement, reachedFrom } from '../placement.ts'
import { READ_TIMEOUT_MS } from '../layerfs/cli.ts'
import { companionsOf, machineRows, msAt, opOf } from '../layerfs/machine-records.ts'
import { Answer, PAGE, cliFeatures, fixOf, invalidateSandbox, keyOf, pages, projectRef, refuse, sandboxObservation, type RouteHost } from './util.ts'
import { processOf, usageOf, type Usage } from './observed.ts'
import { SandboxCreates, type Report } from './sandbox-creates.ts'

/** Calls in flight at once for one answer (stats, diffs). */
const WIDTH = 4
/** Commits whose changed paths are kept: a Commit never changes. */
const KEEP_CHANGES = 5000
/** Default and largest Commits per answer. */
const LIMIT = 200
/** Init jobs kept after they end. */
const KEEP_INITS = 50
/** Saved-query replies have a byte budget as well as an item limit. */
export const HISTORY_PAGE_SIZE = 20
/** History pages kept per host (Projects.historyPages). */
const HISTORY_PAGES_KEPT = 512
/** A wide page (Load all, counting) asks for this many (2.0 pages take up to 1000; one attempt, no shrinking retry). */
const HISTORY_WIDE_PAGE = 100
/** Branch Commit counts kept per host (Projects.historyCounts). */
const HISTORY_COUNTS_KEPT = 2048
/** Commits whose Branch is kept (`Projects.madeOn`). */
const MADE_ON_KEPT = 20000

/** Tagged ids: a Layer's first byte is 0x32. */
const isLayer = (id: unknown) => typeof id === 'string' && id.startsWith('32')
/** A full image digest: what the pinned CLI takes as `sandbox.create --image` before Stage 1. */
const DIGEST = /^sha256:[a-f0-9]{64}$/

/** The create phases `sandbox.create --progress` reports (2.0 spec v19: `lock` while another copy holds the image). */
const CREATE_PHASES = new Set(['inspect', 'lock', 'export', 'send', 'unpack', 'start', 'mount'])
const byteCount = (v: unknown): number | undefined => (typeof v === 'number' && Number.isFinite(v) && v >= 0 ? v : undefined)

/** One `sandbox.create` progress event `{ phase, done?, total? }` as a job update; anything else is no update. */
export function createPhase(progress: unknown): { phase: 'inspect' | 'lock' | 'export' | 'send' | 'unpack' | 'start' | 'mount', done?: number, total?: number } | null {
  if (!progress || typeof progress !== 'object') return null
  const p = progress as { phase?: unknown, done?: unknown, total?: unknown }
  if (typeof p.phase !== 'string' || !CREATE_PHASES.has(p.phase)) return null
  const done = byteCount(p.done), total = byteCount(p.total)
  return { phase: p.phase as 'inspect', ...(done !== undefined ? { done } : {}), ...(total !== undefined ? { total } : {}) }
}

function checkBranch(branch: unknown): asserts branch is string {
  if (typeof branch !== 'string' || !branch || branch.includes('\0')) throw refuse(400, 'InvalidInput', 'branch must be a Branch name, main or a saved id')
}

/** Runs `fn` over `items`, `width` at a time. */
async function each<T>(items: T[], width: number, fn: (item: T, index: number) => Promise<void>): Promise<void> {
  let next = 0
  const lane = async () => {
    while (next < items.length) {
      const index = next++
      await fn(items[index], index)
    }
  }
  await Promise.all(Array.from({ length: Math.min(width, items.length) }, lane))
}

/** Sets `key` as the newest entry of `map`, dropping the oldest past `most`. */
function keep<K, V>(map: Map<K, V>, key: K, value: V, most: number): void {
  map.delete(key)
  map.set(key, value)
  while (map.size > most) map.delete(map.keys().next().value as K)
}

/** A saved id as the file views name it: a publication on main is a Layer, else a Commit. */
export const savedRef = (id: string | null | undefined, branch?: string | null) => id ? { id, kind: isLayer(id) ? 'Layer' : 'Commit', branch: branch ?? null } : null
/**
 * A 2.0 Commit row (`commit list --detail`, `commit get --detail`) ->
 * CommitInfo: `commit`, `created_at`, `size`, `new_*_bytes`; a publication on
 * main carries `source_branch`/`source_commit` and no message.
 */
export const commitInfo = (c: any, branch?: string | null): any => {
  const id = c.commit ?? c.id
  const at = msAt(c.created_at)
  return {
    id,
    branch: c.branch ?? branch ?? '',
    message: c.message ?? '',
    at,
    parent: c.parent ?? null,
    merged: c.second_parent ?? null,
    kind: c.via ?? null,
    base: c.base ?? null,
    sourceCommit: c.source_commit ?? null,
    sourceBranch: c.source_branch ?? null,
    sourceTime: null,
    logical: c.size ?? null,
    savedKind: c.source_branch || isLayer(id) ? 'layer' : 'commit',
    ref: savedRef(id, c.branch ?? branch),
    parentRef: savedRef(c.parent ?? c.base, c.branch ?? branch),
    sourceRef: savedRef(c.source_commit, c.source_branch),
    timeStatus: at !== null ? 'recorded' : 'unavailable',
    newCanonical: c.new_canonical_bytes ?? null,
    newStored: c.new_stored_bytes ?? null,
    newMetadata: c.new_metadata_bytes ?? null,
  }
}

/** A 2.0 diff row's `change` (added, removed, modified) as the views' kind word. */
const changeKind = (change: unknown): string => { const c = String(change ?? ''); return c ? c[0]!.toUpperCase() + c.slice(1) : 'Modified' }

/** A 2.0 `op` companion as the page's operation (machine-records.ts). */
export { companionsOf, msAt, opOf }

/** A LayerFS refusal as the UI shows it: code, the SDK's text, and its detail (entity, phase, op, related). */
export const refusalOf = (error: any) => ({ code: error?.code ?? 'Failed', message: error?.message ?? String(error), detail: error?.detail ?? null, ...fixOf(error) })

/** Refusals a Store's operation gives every Store call: the page still renders what the home's records say. */
export const STORE_REFUSALS = new Set(['Busy', 'Unresolved', 'Unavailable'])

/**
 * A 2.0 Sandbox (a `sandbox list --detail` row, or `sandbox get --detail
 * --with stats`) -> SandboxInfo. `state` is one of the six with its
 * companions; `workspaces` are read only from `sandbox get` (a list row
 * leaves them `unread`). Each Workspace is `{ workspace: "<sandbox>:<name>",
 * branch, running, state, op, path, commit, base_layer, branch_moved,
 * pending_merge, commands }`; `stats.workspaces` adds its overlay and usage.
 */
export function sandboxInfo(s: any, workspaces: any[] | null = null) {
  const name = String(s.sandbox ?? s.name)
  const stats = s.stats && typeof s.stats === 'object' ? s.stats : null
  const perWorkspace = new Map<string, any>((Array.isArray(stats?.workspaces) ? stats.workspaces : []).map((w: any) => [String(w?.workspace), w]))
  return {
    id: name,
    name,
    state: typeof s.state === 'string' ? s.state : 'ready',
    op: opOf(s.op),
    ...companionsOf(s),
    backend: s.backend ?? null,
    route: s.route ?? null,
    project: s.project ?? null,
    image: s.image ?? null,
    machine: s.machine ?? null,
    created: msAt(s.created_at),
    // the daemon's control admission and retained failures (`--with stats`, best effort)
    daemonCapacity: stats?.daemon_capacity ?? null,
    ...(s.platform ? { platform: s.platform } : {}),
    workspaces: (workspaces ?? []).filter((w) => typeof w?.workspace === 'string').map((w) => {
      const handle = String(w.workspace)
      return {
        ws: handle, name: handle.slice(handle.indexOf(':') + 1), branch: w.branch ?? '', path: w.path ?? null,
        state: typeof w.state === 'string' ? w.state : 'ready', op: opOf(w.op), ...companionsOf(w),
        base: w.commit ?? null, baseLayer: w.base_layer ?? null, headMovedSinceMount: w.branch_moved ?? null, pendingMerge: w.pending_merge ?? null,
        mountedAt: msAt(w.mounted_at),
        ...runningOf(w), ...overlayBytesOf(perWorkspace.get(handle), !!stats), ...workspaceUsageOf(perWorkspace.get(handle), !!stats),
      }
    }),
    // the daemon process and the Sandbox's cgroup usage (`--with stats`): absent when not asked, null when not observed
    ...processOf(s),
    ...(Number.isSafeInteger(s.workspace_capacity) && s.workspace_capacity > 0 ? { workspaceCapacity: s.workspace_capacity as number } : {}),
    ...(workspaces ? {} : { unread: true }),
  }
}

/** One command LayerFS says is running in a Workspace (sandbox.get `running`), whoever started it. */
export interface RunningCommand {
  execId: string
  /** When it started, epoch ms. */
  started: number
  /** The log tag of the call that started it; null when it had none. */
  tag: string | null
  /** The command's first word; null when LayerFS did not record it. */
  program: string | null
  /** This host started it from a Sandbox page (its terminal shows it). */
  own?: boolean
  /** The project session its tag names, as the Logs page labels a call. */
  session?: unknown
}

/**
 * A Workspace's running commands (2.0 `sandbox get --detail`: `commands`,
 * each `{ exec, started_at, program, tag }`; the short form's `running` holds
 * the exec ids alone), oldest first; absent when LayerFS listed neither.
 */
function runningOf(w: any): { running?: RunningCommand[] | null } {
  if (!w || !('commands' in w || 'running' in w)) return {}
  const source = Array.isArray(w.commands) ? w.commands : Array.isArray(w.running) ? w.running.map((exec: unknown) => ({ exec })) : null
  if (!source) return { running: [] }
  const running: RunningCommand[] = []
  for (const r of source) {
    if (!r || typeof r.exec !== 'string' || !r.exec) continue
    running.push({
      execId: r.exec, started: msAt(r.started_at) ?? 0,
      tag: typeof r.tag === 'string' && r.tag ? r.tag : null, program: typeof r.program === 'string' && r.program ? r.program : null,
    })
  }
  return { running: running.sort((a, b) => a.started - b.started) }
}

/** A Workspace's `overlay_bytes` (`--with stats` row): absent when stats were not read, `null` when not observed. */
function overlayBytesOf(w: any, read: boolean): { overlayBytes?: number | null } {
  if (!read) return {}
  return { overlayBytes: typeof w?.overlay_bytes === 'number' && Number.isFinite(w.overlay_bytes) && w.overlay_bytes >= 0 ? w.overlay_bytes : null }
}

/**
 * A Workspace item's `usage` (its `ws-<workspace>` cgroup: `{ cpu_usec,
 * memory_current, memory_peak, pids }`, as the Sandbox's own) and
 * `usage_reason`, by presence: absent from a worker that does not report
 * usage per Workspace (the page shows a dash), `null` with the reason when
 * there was nothing to read (no command runs, so LayerFS has removed the group).
 */
function workspaceUsageOf(w: any, read: boolean): { usage?: Usage | null, usageReason?: string } {
  if (!read) return {}
  if (!w) return { usage: null }
  const reason = typeof w.usage_reason === 'string' && w.usage_reason ? w.usage_reason : null
  return { usage: usageOf(w.usage), ...(reason ? { usageReason: reason } : {}) }
}

/** BranchInfo[] from branch.list records and the Commits read (by id). */
/**
 * BranchInfo[] from 2.0 `branch list --detail` rows (`branch`, `commit`,
 * `forked_from`, `base`) and the Commits read (by id). A Branch with no
 * Commit of its own answers the Commit it was forked at as `commit`.
 */
export function branchInfos(listed: any[], byId: Map<string, any> | null = null) {
  const merges = byId ? [...byId.values()].filter((c) => c.via === 'merge') : []
  return listed.map((b) => {
    const name = String(b.branch ?? b.name)
    if (name === 'main') return { name: 'main', head: b.commit ?? null, parent: null, forkedFrom: null, state: 'open', commits: 0 }
    const head = b.commit && b.commit !== b.forked_from ? b.commit : null
    const parent = b.forked_from ? (isLayer(b.forked_from) ? 'main' : (byId?.get(b.forked_from)?.branch ?? null)) : null
    let commits: number | null = byId ? 0 : null
    if (byId) for (const c of byId.values()) if (c.branch === name) commits!++
    // which Branches were published has no 2.0 read (§9 Q3): a Branch is open or merged
    const merged = !!head && merges.some((c) => c.branch !== name && (c.parent === head || c.second_parent === head || c.merged_from === head))
    return { name, head: b.commit ?? null, parent, forkedFrom: b.forked_from ?? null, state: merged ? 'merged' : 'open', commits }
  })
}

// ---------- saved reads (project-reads.js) ----------

const fail = (message: string) => refuse(400, 'InvalidInput', message)
const integer = (q: URLSearchParams, k: string, fallback: number, max = 2000) => {
  const n = Number(q.get(k) ?? fallback)
  if (!Number.isSafeInteger(n) || n < 1 || n > max) throw fail(`${k} must be an integer from 1 to ${max}`)
  return n
}
/** A Branch's saved pin: 2.0 `commit` (its head, or the Commit it was forked at). */
const pinOf = (b: any) => b.commit
const validate = (branch: unknown, path: unknown) => {
  if (typeof branch !== 'string' || !branch || branch.includes('\0')) throw fail('Name a Branch.')
  if (typeof path !== 'string' || !path.startsWith('/') || path.includes('\0') || path.split('/').includes('..')) throw fail('Use an absolute path.')
}

/** One Init job (Connect's ProjectInitJob). */
export interface ProjectInitJob {
  id: string
  name: string
  source: string | null
  started: number
  ended: number | null
  step: string
  progress: { phase: string | null, done: number | null, total: number | null } | null
  project: any
  error: any
}

export class Projects {
  /** Commit id -> its changed paths, or null when LayerFS cannot compare it. */
  readonly changes = new Map<string, any[] | null>()
  /**
   * History pages read at a named Commit (`at`/`head`), newest last: a Branch's
   * Commits before a given Commit never change, so reopening History rereads
   * only the Branches that moved.
   */
  readonly historyPages = new Map<string, unknown>()
  /**
   * Each Branch's Commit count at its last counted head: a head that moved is
   * counted only back to that head, so a live Branch costs one read per refresh.
   */
  readonly historyCounts = new Map<string, { head: string, count: number }>()
  readonly counting = new Map<string, Promise<{ head: string | null, count: number }>>()
  /**
   * `<project> <commit>` -> the Branch the Commit was made on (`commit get`
   * answers it; file history and blame do not). A Commit never moves, so the
   * answer is kept, the oldest dropped past {@link MADE_ON_KEPT}.
   */
  readonly madeOn = new Map<string, string | null>()
  /** Init job id -> ProjectInitJob: the Inits this host started (2.0 keeps no job registry). */
  readonly inits = new Map<string, ProjectInitJob>()
  readonly creations = new Map<string, Promise<unknown>>()
  readonly emptySources: string
  readonly storageObservations: StorageObservations
  readonly storageLedgers: ProjectStorageLedgers
  /** Sandbox creations run in the background (`background: true`), with their progress events. */
  readonly sandboxCreates: SandboxCreates
  /** Store name -> { state, op }: set by the host (Storage.projectJoin). */
  storeJoin: (() => Promise<Map<string, { state: string, op: unknown }>>) | null = null

  constructor(private readonly host: RouteHost) {
    this.emptySources = join(host.root, 'project-sources')
    this.sandboxCreates = new SandboxCreates((type, data) => host.emit(type, data))
    // statistics journals and ledgers describe this home's Stores: they live in it
    this.storageObservations = new StorageObservations(host.stats, host.home)
    this.storageLedgers = new ProjectStorageLedgers(host.stats, host.home, () => host.lfs(), (line) => host.log(line))
  }

  get lfs() { return this.host.lfs() }

  private log(line: string): void { this.host.log(line) }

  /**
   * The Branch each of `ids` was made on: those in `own` are `branch`'s own;
   * the rest are asked once each (`commit get`, at most 4 at a time) and kept.
   * A Commit LayerFS cannot read answers null (the view names no Branch).
   */
  private async branchesOf(project: string, ids: readonly string[], branch: string, own: ReadonlySet<string> = new Set()): Promise<Map<string, string | null>> {
    const out = new Map<string, string | null>()
    const ask: string[] = []
    for (const id of new Set(ids)) {
      if (own.has(id)) out.set(id, branch)
      else if (this.madeOn.has(`${project} ${id}`)) out.set(id, this.madeOn.get(`${project} ${id}`)!)
      else ask.push(id)
    }
    for (let i = 0; i < ask.length; i += 4) {
      await Promise.all(ask.slice(i, i + 4).map(async (id) => {
        let made: string | null = null
        try { const c: any = await this.lfs.commit.get(project, id); made = typeof c?.branch === 'string' && c.branch ? c.branch : null } catch { out.set(id, null); return }
        out.set(id, made)
        this.madeOn.set(`${project} ${id}`, made)
        if (this.madeOn.size > MADE_ON_KEPT) this.madeOn.delete(this.madeOn.keys().next().value!)
      }))
    }
    return out
  }

  private async saved(project: string, branch: string, pin: string | null | undefined): Promise<string> {
    if (pin) {
      if (!/^[0-9a-f]{66}$/.test(pin)) throw fail('A saved pin must be a full 66-hex id.')
      return pin
    }
    return pinOf(await this.lfs.branch.get(project, branch))
  }

  // ---------- status ----------

  /**
   * HostStatus: the linked LayerFS, the home and what it holds, plus the
   * runtime facts (`runtime`, from `ctx.layerfs.status()`).
   */
  async status(runtime: any) {
    const platform = process.platform === 'darwin' ? 'macos' : process.platform === 'win32' ? 'windows' : 'linux'
    const base = {
      platform,
      mode: 'real',
      layerfs: runtime?.version ?? null,
      settings: { state: this.host.root, home: this.host.home, cli: this.host.cli() },
      runtime,
    }
    const [machines, stores] = await Promise.all([machineRows(this.lfs), this.lfs.store.list()])
    return {
      ...base,
      daemon: await this.daemonStatus(),
      machines: machines.map((m) => ({ name: m.name, platform: m.platform, state: m.state })),
      stores: (stores.items ?? []).length,
    }
  }

  /** macOS: whether the native machine has its daemon recorded (null elsewhere). */
  private async daemonStatus(): Promise<{ recorded: boolean, why: string | null } | null> {
    if (process.platform !== 'darwin') return null
    try {
      const recorded = (await this.lfs.machine.get('native', { detail: true }))?.daemon ?? null
      return recorded ? { recorded: true, why: null } : { recorded: false, why: this.host.daemon() ? 'the native daemon was not recorded' : 'no configured CLI or daemon' }
    } catch (error: any) {
      return { recorded: false, why: `the native daemon was not recorded: ${error?.message ?? error}` }
    }
  }

  // ---------- projects ----------

  /** ProjectSummary of a project.list/get record. `join` (Store name -> { state, op }) adds the Store's state. */
  summary(p: any, join: Map<string, { state: string, op: unknown }> | null = null) {
    const key = keyOf(p)
    return {
      name: key.slice(key.indexOf('/') + 1),
      key,
      store: p.store ?? key.split('/')[0],
      kind: p.engine ?? null,
      machine: p.machine ?? null,
      created: msAt(p.created_at),
      // the project's own 2.0 state (busy init/delete, broken unresolved), when not ready
      ...(typeof p.state === 'string' && p.state !== 'ready' ? { state: p.state, op: opOf(p.op), ...companionsOf(p) } : {}),
      demo: null,
      run: null,
      ...(join ? { storeState: join.get(p.store)?.state ?? null, storeOp: join.get(p.store)?.op ?? null } : {}),
    }
  }

  /**
   * Starts an Init of `<store>/<name>` from a folder or an empty source, into
   * the existing Store `store` (2.0 never makes a Store at Init). Answers the
   * job; Init goes on while the window that asked stops polling.
   */
  async create(input: any) {
    const { name, source, store } = input ?? {}
    if (typeof store !== 'string' || !store) throw refuse(400, 'InvalidInput', 'Choose the Store the project goes into.')
    if (typeof name !== 'string' || !/^[a-z0-9](?:[a-z0-9_.-]{0,61}[a-z0-9])?$/.test(name)) throw refuse(400, 'InvalidInput', 'Project names use lowercase letters, numbers, dots, underscores and hyphens, with a letter or number at each end (maximum 63 characters).')
    const { key } = projectRef(`${store}/${name}`)
    if ([...this.inits.values()].some((j) => j.name === key && !j.ended)) throw refuse(409, 'Exists', `${key} is being created`)
    if (source !== undefined) this.checkSource({ source })
    let emptySource: string | undefined
    if (source === undefined) {
      mkdirSync(this.emptySources, { recursive: true, mode: 0o700 })
      emptySource = mkdtempSync(join(this.emptySources, 'empty-'))
    }
    const job: ProjectInitJob = { id: randomUUID(), name: key, source: source ?? null, started: Date.now(), ended: null, step: 'scan', progress: null, project: null, error: null }
    keep(this.inits, job.id, job, KEEP_INITS)
    const on_progress = (p: any) => {
      if (typeof p?.phase === 'string') job.step = p.phase
      job.progress = { phase: p?.phase ?? null, done: p?.done ?? null, total: p?.total ?? null }
    }
    const completion = Promise.resolve()
      .then(() => this.lfs.project.init(key, source ?? emptySource, { on_progress }))
      .then(
        (made) => Object.assign(job, { step: 'done', project: this.summary({ ...made, project: made?.project ?? key }) }),
        (error: any) => {
          Object.assign(job, { step: 'failed', error: { ...refusalOf(error), ...(error?.transport ? { transport: error.transport } : {}) } })
          this.log(`[host] creating ${key}: ${error?.message ?? error}`)
        },
      )
      .finally(() => {
        job.ended = Date.now()
        if (emptySource) rmSync(emptySource, { recursive: true, force: true })
        this.creations.delete(job.id)
      })
    this.creations.set(job.id, completion)
    return { ...job }
  }

  /**
   * A folder an Init may copy from: an existing absolute directory outside the
   * LayerFS home. Reads no file in it; answers
   * `{ source }` (the real path) or refuses. The dialog calls it before it
   * makes a Store, so a wrong folder leaves no empty Store behind.
   */
  checkSource(input: any): { source: string } {
    const source = input?.source
    if (typeof source !== 'string' || !isAbsolute(source) || source.includes('\0') || !statSync(source, { throwIfNoEntry: false })?.isDirectory()) {
      throw refuse(400, 'InvalidInput', 'Choose an existing absolute source folder.')
    }
    const real = realpathSync(source)
    // the home holds the Stores: copying it into one of them is never meant
    const home = safeReal(this.host.home)
    const rel = home ? relative(home, real) : '..'
    if (!(rel === '..' || rel.startsWith(`..${sep}`) || isAbsolute(rel))) throw refuse(400, 'InvalidInput', `Choose a folder outside the LayerFS home (${this.host.home}).`)
    return { source: real }
  }

  /** One of this host's Init jobs. */
  async init(id: string) {
    const job = this.inits.get(id)
    if (!job) throw refuse(404, 'NotFound', 'The project creation task is no longer available. Refresh Projects to check its result.')
    return { ...job }
  }

  /** The Inits this host started, newest first. */
  initJobs() {
    return [...this.inits.values()].reverse().map((j) => ({ ...j }))
  }

  /** Whether an Init is running (for runtime admission). */
  busy(): boolean { return [...this.inits.values()].some((j) => !j.ended) || this.creations.size > 0 }

  /** ProjectSummary[]. */
  async list(fresh = false) {
    const records = await pages((p) => this.lfs.project.list({ ...p, detail: true, ...(fresh ? { timeout_ms: READ_TIMEOUT_MS } : {}) }))
    // set by the host (Storage.projectJoin); a failure leaves the field out
    const join = this.storeJoin ? await this.storeJoin().catch(() => null) : null
    return records.map((p) => this.summary(p, join))
  }

  /** `project get --detail` of a key (the long form names the project, its engine, machine and created_at). */
  async record(key: string): Promise<any> {
    const project = projectRef(key).key
    const got: any = await this.lfs.project.get(project, { detail: true })
    return { ...got, project: got?.project ?? project }
  }

  /**
   * ProjectState from the Store alone: its Branch metadata, its Sandbox
   * records and the Store's state. No Sandbox is asked. While its Store is
   * busy or broken, Store calls are refused (Busy, Unresolved, Unavailable);
   * while it is blocked (its machine is not ready) none is made (U3): the
   * page then gets what the home's records say, the Store's state and its
   * companions, and the refusal in `refusal`, with no Branches.
   */
  async state(key: string) {
    const record = await this.record(key)
    const project = keyOf(record)
    // store.get first (records only): on a stopped machine no Store call is made (U3); the banner offers Start
    const status = await this.lfs.store.get(record.store, { detail: true }).catch(() => null)
    const down = status?.state === 'blocked'
    const [listed, sandboxes] = await Promise.all([
      down
        ? { refused: null }
        : pages((p) => this.lfs.branch.list(project, { ...p, detail: true })).catch((error: any) => {
            if (STORE_REFUSALS.has(error?.code)) return { refused: refusalOf(error) }
            throw error
          }),
      pages((p) => this.lfs.sandbox.list({ project, ...p, detail: true })).catch((error: any) => (this.log(`[host] sandbox.list of ${project}: ${error?.message ?? error}`), [] as any[])),
    ])
    const facts = companionsOf(status)
    const storeFacts = { storeState: status?.state ?? null, storeOp: opOf(status?.op), storeBlockedBy: facts.blockedBy ?? null, storeReason: facts.reason ?? null, storeFix: facts.fix ?? null }
    // the Store's size is `store get --with stats` (it opens the Store): the Storage tab reads it
    const store = { path: null, bytes: null }
    if (!Array.isArray(listed)) {
      return {
        project: { ...this.summary(record), ...storeFacts, refusal: listed.refused },
        branches: [],
        layers: [],
        sandboxes: sandboxes.map((s) => sandboxInfo(s)),
        last: null,
        commits: null,
        store,
        storage: null,
      }
    }
    // Every project tab needs this shell. History must never gate Files,
    // Storage or Sandboxes; load saved revisions through their paged routes.
    const main = listed.find((b: any) => b.branch === 'main')
    return {
      project: { ...this.summary(record), ...storeFacts },
      branches: branchInfos(listed),
      // 2.0 lists none of main's Layers but its newest (§9 Q3)
      layers: main?.commit ? [{ id: main.commit, message: '', at: null, from: null }] : [],
      sandboxes: sandboxes.map((s) => sandboxInfo(s)),
      last: null,
      commits: null,
      store,
      // the Store's totals have no 2.0 call (§9 Q4)
      storage: null,
    }
  }

  /** SandboxInfo[] of the project's records (Workspaces are read on demand). */
  async sandboxes(key: string) {
    const { key: project } = projectRef(key)
    const listed = await pages((p) => this.lfs.sandbox.list({ project, ...p, detail: true }))
    return listed.map((s) => sandboxInfo(s))
  }

  /**
   * A new Sandbox of the project: `{ name, machine?, image?, start?, background? }`
   * (docs/specs/sandbox-images.md §4.1). A sqlite project's Sandboxes run on
   * its Store's machine (another is Unsupported); a PostgreSQL project's on
   * `machine` when one is named (any machine whose platform runs PostgreSQL
   * Sandboxes; LayerFS refuses the rest), else placement.ts's order (its
   * last machine, a machine its database was reached from, native; never a
   * broken or stopped one).
   *
   * `image` is a string. Until the pinned CLI declares references (Stage 1,
   * `imageRefs`), only a full `sha256:` digest is taken: anything else is
   * refused 501 `Unsupported` with `detail.need: 'image-refs'`, since the CLI
   * would drop it silently. `start: true` runs machine.start first (the
   * "Start and create" button). `background: true` answers `202 { creation }`
   * and runs the create as a job ({@link SandboxCreates}); without it the
   * route waits and answers sandbox.create's result.
   */
  async createSandbox(key: string, body: any = {}) {
    const record = await this.record(key)
    const { name, image } = body ?? {}
    if (typeof name !== 'string' || !name) throw refuse(400, 'InvalidInput', 'A Sandbox needs a name.')
    // a sqlite project's Sandboxes run on its Store's machine (LayerFS's default); a PostgreSQL one's on the machine named, else placement.ts's
    const storeName = String(record.store ?? keyOf(record).split('/')[0])
    const storeRecord: any = await this.lfs.store.get(storeName, { detail: true }).catch(() => null)
    const postgresql = (storeRecord?.engine ?? record.engine) === 'postgresql'
    let machine = storeRecord?.machine ?? record.machine
    if (postgresql) {
      const named = typeof body.machine === 'string' && body.machine ? body.machine : null
      if (!named) {
        const sandboxes = await this.lfs.sandbox.list({ project: keyOf(record), detail: true }).then((r: any) => r?.items ?? [], () => [])
        machine = pgPlacement(await machineRows(this.lfs), { last: lastUsed(sandboxes), reached: reachedFrom(storeRecord) })
      } else machine = named
    }
    if (image !== undefined && image !== null && image !== '' && typeof image !== 'string') throw refuse(400, 'InvalidInput', 'Name the image as a string: its id (a sha256 digest) or a reference.')
    const picked = typeof image === 'string' && image ? image : undefined
    const features = await cliFeatures(this.host)
    if (picked && !DIGEST.test(picked) && features.imageStage < 1) {
      throw Object.assign(refuse(501, 'Unsupported', `The pinned LayerFS CLI takes a Sandbox image only by its full sha256 digest, not ${JSON.stringify(picked)}. Pick an image from the list, or pin a LayerFS that takes image references.`), { detail: { need: 'image-refs' } })
    }
    // Stage 1: a Docker image whose copy cannot fit is refused before the export starts (D8), in LayerFS's own `Capacity` shape (its refusal is the real answer)
    if (picked && features.imageStage === 1) {
      const short = await this.spaceFor(String(machine), picked)
      if (short) throw Object.assign(refuse(507, 'Capacity', `${picked} needs ${short.need} bytes on ${machine}; it has ${short.free} free.`), { detail: { needed_bytes: short.need, free_bytes: short.free, path: null }, fix: `layerfs machine prune ${machine} --unused` })
    }
    const start = body.start === true
    const project = keyOf(record)
    const input = { project, name, machine: String(machine), ...(picked ? { image: picked } : {}), ...(start ? { start: true } : {}) }
    const run = async (report: Report) => {
      if (start) {
        report({ phase: 'machine' })
        await this.startMachine(input.machine)
        report({ phase: 'queued' })
      }
      await this.host.ensureNative()
      // the CLI's create phases (inspect, lock, export, send, unpack, start) as the job's, bytes while a phase reports them
      const progress = features.createProgress ? { on_progress: (p: unknown) => { const u = createPhase(p); if (u) report(u) } } : {}
      const made = await this.lfs.sandbox.create(project, name, { ...(postgresql ? { machine: String(machine) } : {}), ...(picked ? { image: picked } : {}), ...progress })
      this.sandboxCreates.created(project, name)
      this.sandboxMade(name)
      return made
    }
    if (body.background === true) return new Answer(202, { creation: this.sandboxCreates.admit(input, run) })
    return run(() => {})
  }

  /** Stage 1's disk precheck of a create; the router wires Machines.spaceFor (none: no precheck). */
  spaceFor: (machine: string, image: string) => Promise<{ need: number, free: number } | null> = async () => null

  /** A Sandbox was made: the router wires the metrics sampler's `created` (no stale "daemon not running" point). */
  sandboxMade: (name: string) => void = () => {}

  /** machine.start of `name` before a create (`start: true`); the router wires Machines.start (machine-progress events). */
  startMachine: (name: string) => Promise<unknown> = (name) => this.lfs.machine.start(name)

  /** One Sandbox with its Workspaces (sandbox.get). */
  async sandbox(name: string) {
    if (typeof name !== 'string' || !name) throw refuse(400, 'InvalidInput', 'Name the Sandbox.')
    const record = await sandboxObservation(this.lfs, name)
    const info = sandboxInfo({ ...record, sandbox: record?.sandbox ?? name }, Array.isArray(record?.workspaces) ? record.workspaces : [])
    // running commands: which ones this host's Sandbox pages started (by exact exec id), and whose session a tag names
    const tags = new Set<string>()
    for (const w of info.workspaces) for (const r of w.running ?? []) if (r.tag) tags.add(r.tag)
    let labels: Record<string, unknown> = {}
    if (tags.size) { try { labels = await this.host.labelTags?.([...tags]) ?? {} } catch { /* a label is optional */ } }
    for (const w of info.workspaces) {
      for (const r of w.running ?? []) {
        r.own = this.ownExec?.(w.ws, r.execId) ?? false
        if (r.tag && labels[r.tag]) r.session = labels[r.tag]
      }
    }
    return info
  }

  /** Whether this host's Sandbox pages started command `execId` in Workspace `ws` (set by the router from commands.ts). */
  ownExec?: (ws: string, execId: string) => boolean

  /**
   * Mounts a Workspace in Sandbox `name`: `{ branch, fork?, name? }`. Only
   * side Branches are mounted (owner D14: mounting main is refused), so
   * `fork: true` first forks `branch` from main. Answers workspace.mount's result.
   */
  async mount(sandbox: string, { branch, fork = false, name }: any = {}) {
    if (typeof sandbox !== 'string' || !sandbox) throw refuse(400, 'InvalidInput', 'Name the Sandbox.')
    checkBranch(branch)
    if (branch === 'main') throw refuse(400, 'InvalidInput', 'Only side Branches are mounted: fork a Branch from main and mount that.')
    const lfs = this.lfs
    if (fork) {
      const { project } = await lfs.sandbox.get(sandbox, { detail: true })
      await lfs.branch.fork(project, branch, 'main')
    }
    try { return await lfs.workspace.mount(sandbox, branch, name ? { name } : {}) } finally { invalidateSandbox(lfs, sandbox) }
  }

  /** workspace.unmount(ws, { discard, force }): refused while it has changes unless discarded, and while busy unless forced. */
  async unmount(ws: string, { discard = false, force = false } = {}) {
    if (typeof ws !== 'string' || !ws.includes(':')) throw refuse(400, 'InvalidInput', 'A Workspace is named <sandbox>:<name or path>.')
    const lfs = this.lfs
    try { return await lfs.workspace.unmount(ws, { ...(discard ? { discard: true } : {}), ...(force ? { force: true } : {}) }) } finally { invalidateSandbox(lfs, ws.slice(0, ws.indexOf(':'))) }
  }

  /** sandbox.remove(name, { force }); answers its long form (`unmounted`: the Workspaces a forced removal ended). */
  async closeSandbox(name: string, { force = false } = {}) {
    if (typeof name !== 'string' || !name) throw refuse(400, 'InvalidInput', 'Name the Sandbox to close.')
    // A session's Sandbox holds its mounted Workspace: save a checkpoint and close it as the session does.
    if (!force && await this.host.closeSessionSandbox?.(name).finally(() => this.forget(name))) {
      this.sandboxClosedHook?.(name)
      return { ok: true, outcome: 'removed', sandbox: name, unmounted: [], session: true }
    }
    const result = await this.lfs.sandbox.remove(name, { detail: true, ...(force ? { force: true } : {}) }).finally(() => this.forget(name))
    await this.host.sandboxClosed?.(name)
    this.sandboxClosedHook?.(name)
    return result
  }

  /** A write on `sandbox` (its outcome known or not): its next read asks LayerFS again. */
  private forget(sandbox: string) {
    try { invalidateSandbox(this.lfs, sandbox) } catch { /* no CLI pinned: nothing is cached for it */ }
  }

  /** Called after each successful close (the router drops the Sandbox's command logs). */
  sandboxClosedHook?: (name: string) => void

  /** Closes every Sandbox of the project (`force` passes on); answers each close. */
  async closeSandboxes(key: string, { force = false } = {}) {
    const { key: project } = projectRef(key)
    const listed = await pages((p) => this.lfs.sandbox.list({ project, ...p }))
    const out = []
    for (const s of listed) out.push(await this.closeSandbox(String(s.sandbox), { force }))
    return out
  }

  /**
   * project.delete: refused while a mount of the project is live; the project's data stays in its Store until
   * store.compact collects it.
   */
  async remove(key: string) {
    const { key: project } = projectRef(key)
    // DSH sessions of the project hold Sandboxes and mounts: save and close them first.
    await this.host.projectRemoving?.(project)
    await this.lfs.project.delete(project)
    await this.host.projectRemoved?.(project)
    return { removed: true }
  }

  // ---------- Store statistics ----------

  /**
   * A Store's statistics: `store get --with stats` (it opens the Store where it
   * lives), its engine and the `stats` section. The observation journal needs
   * a Store identity 2.0 does not answer, so its append is refused and the
   * answer says why (`observationError`).
   */
  async storeStats(name: string) {
    const record: any = await this.lfs.store.get(name, { detail: true, with: ['stats'] })
    const stats = { engine: record?.engine ?? null, ...(record?.stats ?? {}) }
    try {
      await this.storageObservations.record(name, stats, record)
      return stats
    } catch (error: any) {
      // Measurement success remains visible even if local observation persistence fails.
      return { ...stats, observationError: String(error?.message ?? error) }
    }
  }

  async storeObservations(name: string) { return this.storageObservations.read(name) }

  /**
   * StoreProjects (the Store page's Projects tab and Overview): every project of
   * a Store with its ledger totals (project-storage.ts), its newest Commit, and
   * main's file count (project.stats). Ledgers refresh in the background as for
   * one project; reads go one project at a time, as the Store takes them. A
   * project.stats answer is reused while main's head is unchanged, so a reread
   * walks no tree again.
   */
  async storeProjects(name: string) {
    // every project of the Store, page by page (`store get`'s `projects` is cut at 20)
    const records = await pages((p) => this.lfs.project.list({ store: name, ...p, detail: true }))
    const rows = [...new Map(records.map((r: any) => [keyOf(r), r])).entries()]
      .map(([project, record]) => ({ short: project.slice(project.indexOf('/') + 1), record }))
      .sort((a, b) => (a.short < b.short ? -1 : a.short > b.short ? 1 : 0))
    const projects = []
    for (const { short, record } of rows) {
      const project = keyOf(record), created = msAt(record.created_at)
      const view = await this.storageLedgers.view(project, created)
      const last = view.recent.length ? view.recent[view.recent.length - 1] : null
      projects.push({ name: short, created, totals: view.totals, last, files: await this.mainFiles(project), partial: view.partial, building: view.building, error: view.error })
    }
    return { store: name, projects }
  }

  /** main's file count, by main's head (one project.stats walk per head). */
  private readonly filesByHead = new Map<string, { head: string, files: number | null, complete: boolean }>()
  private async mainFiles(project: string): Promise<{ files: number | null, complete: boolean } | null> {
    const head = await this.lfs.branch.get(project, 'main').then((b: any) => String(b?.commit ?? ''), () => '')
    const known = this.filesByHead.get(project)
    if (head && known?.head === head) return { files: known.files, complete: known.complete }
    const stats = await this.lfs.project.stats(project, head ? { at: head } : {}).catch(() => null)
    if (!stats) return null
    const row = { head, files: typeof stats.file_count === 'number' ? stats.file_count : null, complete: stats.partial !== true }
    if (head) this.filesByHead.set(project, row)
    return { files: row.files, complete: row.complete }
  }

  // ---------- a project's own storage ----------

  /**
   * ProjectStorage: this project's saves summed from their Commit counters
   * (the ledger, refreshed in the background), its Store's totals for the
   * share, and main's latest tree. `?refresh=1` starts a refresh now.
   */
  async projectStorage(key: string, q: URLSearchParams) {
    const record = await this.record(key)
    const project = keyOf(record), created = msAt(record.created_at)
    const [view, store, storeRecord, main] = await Promise.all([
      this.storageLedgers.view(project, created, { refresh: q.get('refresh') === '1' }),
      this.lfs.store.get(record.store, { with: ['stats'] }).then((r: any) => r?.stats ?? {}, (error: any) => ({ refusal: refusalOf(error) })),
      this.lfs.store.get(record.store).catch(() => null),
      this.lfs.project.stats(project).catch(() => null),
    ])
    // `projects_count` sits beside a list cut at 20
    const projects = typeof storeRecord?.projects_count === 'number' ? storeRecord.projects_count : Array.isArray(storeRecord?.projects) ? new Set(storeRecord.projects).size : null
    return {
      ...view,
      store: { name: record.store, projects, ...('refusal' in store ? { refusal: store.refusal } : { historyLogical: store.logical_bytes ?? null, canonical: store.canonical_bytes ?? null, stored: store.stored_bytes ?? null, metadata: store.metadata_bytes ?? null, file: store.file_bytes ?? null }) },
      main: main ? { logical: main.size ?? null, files: main.file_count ?? null, directories: main.dir_count ?? null, complete: main.partial !== true } : null,
    }
  }

  /**
   * A project's size for its Overview card, and the raw bytes of main's latest
   * tree. Alone in its Store (`?alone=1`, the caller knows the Store's
   * projects) the project's size is the Store's packed bytes, which count every
   * write, publications to main included. In a shared Store it is the packed
   * bytes its saves wrote first (the ledger above, refreshed in the background;
   * `counting` until a first read finishes or while one runs).
   */
  async projectSize(key: string, q: URLSearchParams) {
    const record = await this.record(key)
    const project = keyOf(record), created = msAt(record.created_at)
    const files = this.lfs.project.stats(project).then((main: any) => typeof main?.size === 'number' ? main.size : null, () => null)
    if (q.get('alone') === '1') {
      const [store, logical] = await Promise.all([this.lfs.store.get(record.store, { with: ['stats'] }).then((r: any) => r?.stats ?? null, () => null), files])
      return { project, stored: typeof store?.stored_bytes === 'number' ? store.stored_bytes : null, counting: false, files: logical }
    }
    const [view, logical] = await Promise.all([this.storageLedgers.view(project, created), files])
    return { project, stored: view.totals && !view.partial ? view.totals.stored : null, counting: view.partial || view.building !== null, files: logical }
  }

  async history(key: string, q: URLSearchParams) {
    const project = projectRef(key).key
    const branch = q.get('branch')
    if (!branch) {
      const page = await this.branchPage(project, q.get('pos'))
      return { branches: page.items.map((b: any) => ({ name: String(b.branch), head: pinOf(b), base: b.base ?? null, forkedFrom: b.forked_from ?? null })), next: page.next }
    }
    const commit = q.get('commit')
    if (commit) {
      const c = await this.lfs.commit.get(project, commit, { branch, detail: true })
      const id = String(c?.commit ?? commit)
      const from = q.get('from') ?? c.parent ?? c.base
      if (!from) return { item: commitInfo({ ...c, commit: id }, branch), changes: [], nextChanges: null }
      const diff = await this.lfs.commit.diff(project, id, { from, limit: 100, ...(q.get('cursor') ? { cursor: q.get('cursor') } : {}) })
      return { item: commitInfo({ ...c, commit: id }, branch), changes: (diff.items ?? []).map(({ path, change }: any) => ({ path, kind: String(change) })), nextChanges: diff.next_cursor ?? null }
    }
    // Resume from the immutable last parent, rather than re-reading a moving head.
    const at = q.get('at') ?? q.get('head')
    // the caller's fork point from the Branch list (`fork`) saves a branch.get per Branch
    const fork = branch === 'main' ? null : q.get('fork') || (await this.lfs.branch.get(project, branch, { detail: true })).forked_from
    if (q.get('count')) return this.historyCount(project, branch, at, fork)
    const wide = !!q.get('wide')
    const cacheKey = at ? JSON.stringify([project, branch, at, fork, wide]) : null
    const cached = cacheKey ? this.historyPages.get(cacheKey) : undefined
    if (cached) { this.historyPages.delete(cacheKey!); this.historyPages.set(cacheKey!, cached); return cached }
    const page = await this.commitPage(project, branch, at, wide ? HISTORY_WIDE_PAGE : HISTORY_PAGE_SIZE)
    const all: any[] = page.items
    const boundary = fork ? all.findIndex((c) => c.id === fork) : -1
    const items = boundary < 0 ? all : all.slice(0, boundary)
    const parent = items.at(-1)?.parent
    const answer = { items: items.map((c) => commitInfo(c, branch)), head: at ?? items[0]?.id ?? null, next: page.more && boundary < 0 && parent !== fork ? parent ?? null : null }
    if (cacheKey) {
      this.historyPages.set(cacheKey, answer)
      if (this.historyPages.size > HISTORY_PAGES_KEPT) this.historyPages.delete(this.historyPages.keys().next().value!)
    }
    return answer
  }

  /**
   * How many Commits a side Branch has from `head` back to its fork (`main` has Layers, not
   * Commits: null). Counted once in wide pages, then kept: a moved head reads back only to
   * the head it was last counted at. One count runs at a time per Branch and head.
   */
  async historyCount(project: string, branch: string, head: string | null, fork: string | null): Promise<{ head: string | null, count: number | null }> {
    if (branch === 'main') return { head, count: null }
    if (!head || head === fork) return { head, count: 0 }
    const key = JSON.stringify([project, branch])
    const known = this.historyCounts.get(key)
    if (known?.head === head) return known
    const flight = JSON.stringify([project, branch, head])
    const running = this.counting.get(flight)
    if (running) return running
    const run = (async () => {
      let count = 0
      let at: string = head
      for (;;) {
        const page = await this.commitPage(project, branch, at, HISTORY_WIDE_PAGE)
        const rows: any[] = page.items
        let done = false
        for (const c of rows) {
          if (fork && c.id === fork) { done = true; break }
          if (known && c.id === known.head) { count += known.count; done = true; break }
          count++
        }
        const parent = rows.at(-1)?.parent
        if (done || !page.more || !rows.length || !parent || parent === fork || parent === at) break
        at = parent
      }
      const answer = { head, count }
      this.historyCounts.delete(key)
      this.historyCounts.set(key, answer)
      if (this.historyCounts.size > HISTORY_COUNTS_KEPT) this.historyCounts.delete(this.historyCounts.keys().next().value!)
      return answer
    })()
    this.counting.set(flight, run)
    try { return await run } finally { this.counting.delete(flight) }
  }

  /**
   * One page of a Branch's Commits from `at` (2.0 `commit list --at --detail`),
   * newest first, each row with its `id`; `more` when LayerFS has another page.
   * Pages after the first resume at the last row's parent, so each is an
   * immutable read the host can keep.
   */
  async commitPage(project: string, branch: string, at: string | null, limit: number): Promise<{ items: any[], more: boolean }> {
    const page: any = await this.lfs.commit.list(project, branch, { limit, ...(at ? { at } : {}), detail: true })
    return { items: (page?.items ?? []).map((c: any) => ({ ...c, id: c.commit })), more: !!page?.next_cursor }
  }

  /** One bounded immutable directory page (2.0 `file list --at`, by cursor); metadata is read only for opened files. */
  async listDir(key: string, branch: string, path: string, pin: string | null, pos: string | number = 0) {
    validate(branch, path)
    const project = projectRef(key).key
    const at = await this.saved(project, branch, pin)
    const cursor = pos === 0 || pos === '0' || pos === '' ? null : String(pos)
    const page: any = await this.lfs.file.list(project, branch, path, { at, limit: 1000, ...(cursor ? { cursor } : {}) })
    const items = (page?.items ?? []).map((i: any) => ({ name: String(i.path).split('/').filter(Boolean).at(-1) ?? String(i.path), path: i.path, kind: i.type ?? 'file', size: null, mode: null }))
    return { items, next: page?.next_cursor ?? null, at: { id: at } }
  }

  /** A window of a saved file (`file stat` and `file read --at --line --limit --detail`; a file that is not text answers `binary`). */
  async file(key: string, branch: string, path: string | null, pin: string | null, line: string | number = 1) {
    validate(branch, path)
    const project = projectRef(key).key
    const at = await this.saved(project, branch, pin)
    if (!Number.isSafeInteger(Number(line)) || Number(line) < 1) throw fail('line must be a positive integer')
    const stat: any = await this.lfs.file.stat(project, branch, path, { at })
    const window: any = await this.lfs.file.read(project, branch, path, { at, line: Number(line), limit: 1000, detail: true })
    if (window?.binary === true) return { path, size: stat.size, binary: true, text: null, at: { id: at } }
    return {
      path, size: stat.size, mode: stat.mode, binary: false, text: typeof window?.text === 'string' ? window.text : '',
      next_line: window?.next_line ?? null, line_truncated: window?.line_truncated === true, at: { id: at },
    }
  }

  /**
   * The saved-file queries: `blame` (file blame's ranges with file read's
   * lines for the same window), `revisions` (file history), `compare` (commit
   * diff of one path, `--with patch`) and `branch-diff` (commit diff under a
   * path). 2.0 has no line ownership across a diff: `attribution` answers the
   * comparison with its attribution unavailable (file blame names owners).
   */
  async fileQuery(key: string, q: URLSearchParams) {
    const project = projectRef(key).key, branch = q.get('branch') ?? 'main', path = q.get('path')
    validate(branch, path)
    const at = await this.saved(project, branch, q.get('pin'))
    const kind = q.get('kind'), pageSize = integer(q, 'size', 50, 1000)
    if (kind === 'blame') {
      const line = integer(q, 'line', 1, Number.MAX_SAFE_INTEGER)
      const [window, blame]: any[] = await Promise.all([
        this.lfs.file.read(project, branch, path, { at, line, limit: 1000, detail: true }),
        this.lfs.file.blame(project, branch, path, { at, line, limit: 1000, detail: true }),
      ])
      const messages = new Map<string, string | null>((Array.isArray(blame?.commits) ? blame.commits : []).map((c: any) => [String(c.commit), c.message ?? null]))
      const ranges: any[] = Array.isArray(blame?.ranges) ? blame.ranges : []
      const madeOn = await this.branchesOf(project, ranges.map((r) => r.commit).filter(Boolean).map(String), branch)
      const text: string = typeof window?.text === 'string' ? window.text : ''
      const rows = text === '' ? [] : text.replace(/\n$/, '').split('\n')
      const lines = rows.map((t, i) => {
        const n = line + i
        const owner = ranges.find((r) => n >= r.start && n <= r.end)?.commit
        return { n, text: t, owner: owner ? { ref: savedRef(owner, madeOn.get(String(owner)))!, message: messages.get(String(owner)) ?? null } : null }
      })
      return { at: savedRef(at, branch), lines, coverage: { status: blame?.partial ? 'partial' : 'complete' }, next_line: blame?.next_line ?? window?.next_line ?? null }
    }
    if (kind === 'revisions') {
      const traversal = q.get('traversal') ?? 'provenance'
      const page: any = await this.lfs.file.history(project, branch, path, { at, traversal, limit: pageSize, ...(q.get('continuation') ? { cursor: q.get('continuation') } : {}), detail: true })
      const items: any[] = page?.items ?? []
      // A provenance page also lists Commits made on the Branches the path came from: the same
      // page on this Branch's own line says which are its own, `commit get` names the others.
      let own = new Set<string>(traversal === 'branch' ? items.map((r) => String(r.commit)) : [])
      if (traversal !== 'branch' && items.length) {
        const line: any = await this.lfs.file.history(project, branch, path, { at, traversal: 'branch', limit: q.get('continuation') ? 1000 : pageSize }).catch(() => null)
        own = new Set((line?.items ?? []).map((r: any) => String(r.commit)))
      }
      // a Commit's parent is on its Branch (a Branch's first Commit has none)
      const madeOn = await this.branchesOf(project, items.map((r) => String(r.commit)), branch, own)
      return {
        items: items.map((r: any) => {
          const on = madeOn.get(String(r.commit))
          return {
            ref: savedRef(r.commit, on), compared_to: savedRef(r.parent, on), kind: changeKind(r.change), message: r.message ?? null,
            recorded_at: msAt(r.created_at), after: typeof r.size === 'number' ? { size: r.size } : null,
            changes: { content: r.content ?? undefined, metadata: r.metadata ?? undefined, identity: r.identity ?? undefined },
          }
        }),
        has_more: !!page?.next_cursor, continuation: page?.next_cursor ?? null,
      }
    }
    if (kind === 'compare' || kind === 'attribution') {
      const rightBranch = q.get('rightBranch') ?? branch
      const leftPin = await this.saved(project, rightBranch, q.get('left'))
      const diff: any = await this.lfs.commit.diff(project, at, { from: leftPin, path, with: ['patch'], limit: 1, detail: true })
      const item = (diff?.items ?? []).find((i: any) => i.path === path) ?? diff?.items?.[0] ?? null
      let left = 0, right = 0
      const rows = (Array.isArray(item?.hunks) ? item.hunks : []).map((h: any) => {
        const op = h.op === '+' || h.op === '-' ? h.op : ' '
        if (op !== '+') left++
        if (op !== '-') right++
        return { op, text: String(h.text ?? ''), ...(op !== '+' ? { left_line: left } : {}), ...(op !== '-' ? { right_line: right } : {}) }
      })
      // `--with patch` lists every line of a changed text file (kept ones too); its sides are
      // what the change says: an added path has no before, a removed one no after. A changed
      // file with no lines and none omitted is not text: its sides' sizes say what changed.
      const change = String(item?.change ?? '')
      const binary = !!item && !Array.isArray(item.hunks) && item.hunks_omitted !== true && item.content !== false && (item.type ?? 'file') === 'file'
      const sizeAt = async (onBranch: string, pin: string) => {
        if (!binary) return {}
        try { const s: any = await this.lfs.file.stat(project, onBranch, path, { at: pin }); return typeof s?.size === 'number' ? { size: s.size } : {} } catch { return {} }
      }
      const [before, after] = await Promise.all([
        change === 'added' ? null : sizeAt(rightBranch, leftPin),
        change === 'removed' ? null : sizeAt(branch, at),
      ])
      return {
        rows, has_more: item?.hunks_omitted === true, change_kind: item ? changeKind(item.change) : 'Unchanged',
        before, after,
        changes: item ? { content: item.content, metadata: item.metadata, identity: item.identity, attribution: null } : undefined,
        ...(kind === 'attribution' ? { attribution_coverage: { status: 'unavailable', reasons: ['LayerFS 2.0 names line owners with file blame, not across a diff'] } } : {}),
      }
    }
    if (kind === 'branch-diff') {
      const left = await this.saved(project, q.get('rightBranch') ?? branch, q.get('left'))
      const diff: any = await this.lfs.commit.diff(project, at, { from: left, ...(path && path !== '/' ? { path } : {}), limit: pageSize, ...(q.get('continuation') ? { cursor: q.get('continuation') } : {}), detail: true })
      return {
        from: savedRef(diff?.from ?? left), to: savedRef(diff?.to ?? at, branch),
        items: (diff?.items ?? []).map((i: any) => ({ path: i.path, kind: changeKind(i.change), type: i.type ?? 'file', content: i.content, metadata: i.metadata, attribution: null, changed_owner_lines: null })),
        has_more: !!diff?.next_cursor, continuation: diff?.next_cursor ?? null, coverage: { complete: true },
      }
    }
    throw fail('Unknown saved-file query.')
  }

  async branches(key: string, q: URLSearchParams) {
    const project = projectRef(key).key
    if (q.get('branch')) return this.lfs.branch.get(project, q.get('branch'), { detail: true })
    const page = await this.branchPage(project, q.get('pos'))
    return { items: page.items.map((b: any) => ({ name: String(b.branch), head: pinOf(b), parent: null, forkedFrom: b.forked_from ?? null, state: 'open', commits: null })), next: page.next }
  }

  /**
   * One page of a project's Branches (`branch list --detail`, 100 at a time,
   * by cursor; `pos` is the cursor, `0` or none the first page), main first:
   * main is read on its own for the first page and left out of every listed
   * page, since name order puts it on any page (`dsh.*` sorts before it). The
   * first page is never empty, so a client that hides internal Branches still
   * has one to show.
   */
  private async branchPage(project: string, pos: string | null): Promise<{ items: any[], next: string | null }> {
    const cursor = pos && pos !== '0' ? pos : null
    const [page, main]: any[] = await Promise.all([
      this.lfs.branch.list(project, { limit: 100, ...(cursor ? { cursor } : {}), detail: true }),
      cursor ? null : this.lfs.branch.get(project, 'main', { detail: true }),
    ])
    const items = (page?.items ?? []).filter((b: any) => b.branch !== 'main')
    return { items: main ? [{ ...main, branch: 'main' }, ...items] : items, next: page?.next_cursor ?? null }
  }

  /** `project stats --branch --at`: main's (or a Branch's) tree at a pin, in the view's words. */
  async projectStats(key: string, q: URLSearchParams) {
    const project = projectRef(key).key, branch = q.get('branch') ?? 'main'
    const at = await this.saved(project, branch, q.get('pin'))
    const s: any = await this.lfs.project.stats(project, { branch, at })
    return { files: s?.file_count ?? null, directories: s?.dir_count ?? null, symlinks: s?.symlink_count ?? null, logical: s?.size ?? null, complete: s?.partial !== true, projectKey: project }
  }

  // ---------- Commits ----------

  async commits(key: string, branch: string, limitText: string | null) {
    checkBranch(branch)
    const limit = limitText == null || limitText === '' ? LIMIT : Number(limitText)
    if (!Number.isSafeInteger(limit) || limit < 1 || limit > PAGE) throw refuse(400, 'InvalidInput', `limit must be a whole number from 1 to ${PAGE}`)
    const { key: project } = projectRef(key)
    // main's rows are its publications: the Commits list reads side Branches
    if (branch === 'main') return []
    const b: any = await this.lfs.branch.get(project, branch, { detail: true })
    return this.withChanges(project, branch, await this.ownCommits(project, { name: branch, forked_from: b?.forked_from ?? null }, limit))
  }

  /** `{ [branch]: CommitInfo[] }` for several Branches (main's are []); a Branch gone since the caller saw it is left out. */
  async branchCommits(key: string, branches: string[]) {
    const { key: project } = projectRef(key)
    if (!Array.isArray(branches) || branches.length === 0) return {}
    if (branches.length > PAGE) throw refuse(400, 'InvalidInput', `at most ${PAGE} Branches at once`)
    for (const b of branches) checkBranch(b)
    const wanted = [...new Set(branches)]
    const out: Record<string, unknown> = Object.fromEntries(wanted.filter((b) => b === 'main').map((b) => [b, []]))
    const listed = new Map((await pages((p) => this.lfs.branch.list(project, { ...p, detail: true }))).map((b: any) => [String(b.branch), b]))
    await each(wanted.filter((b) => b !== 'main' && listed.has(b)), WIDTH, async (branch) => {
      try {
        out[branch] = await this.withChanges(project, branch, await this.ownCommits(project, { name: branch, forked_from: listed.get(branch).forked_from ?? null }, LIMIT))
      } catch (error: any) {
        if (error?.code !== 'NotFound') throw error
      }
    })
    return out
  }

  /** A Branch's own Commits (records), newest first: the walk stops at the fork. */
  async ownCommits(project: string, branch: { name: string, forked_from: string | null }, limit: number) {
    const items: any[] = []
    let at: string | null = null
    while (items.length < limit) {
      const page = await this.commitPage(project, branch.name, at, Math.min(HISTORY_PAGE_SIZE, limit - items.length))
      const rows = page.items.map((c: any) => ({ ...c, branch: c.branch ?? branch.name }))
      const boundary = rows.findIndex((c: any) => c.id === branch.forked_from)
      items.push(...(boundary < 0 ? rows : rows.slice(0, boundary)))
      const parent = rows.at(-1)?.parent
      if (!page.more || boundary >= 0 || !parent || parent === branch.forked_from || parent === at || !rows.length) break
      at = parent
    }
    return items
  }

  /** CommitInfo[] of records, each with its changed paths. */
  async withChanges(project: string, branch: string, records: any[]) {
    const commits = records.map((c) => commitInfo(c, branch))
    await each(records, WIDTH, async (c, index) => {
      const changes = await this.changesOf(project, c)
      if (changes) commits[index].changes = changes
    })
    return commits
  }

  /**
   * One Commit's changed paths: `commit diff <commit> --from <parent>`, or for
   * a Branch's first Commit from its base (as history() reads it). A failed
   * diff is logged.
   */
  async changesOf(project: string, c: any) {
    if (this.changes.has(c.id)) return this.changes.get(c.id)
    const from = c.parent ?? c.base
    if (!from) return null
    try {
      const items: any[] = []
      for (let cursor: string | null = null; ;) {
        const page: any = await this.lfs.commit.diff(project, c.id, { from, limit: PAGE, ...(cursor ? { cursor } : {}) })
        items.push(...(page.items ?? []).map(({ path, change }: any) => ({ path, kind: String(change) })))
        if (page.next_cursor) cursor = page.next_cursor
        else break
      }
      keep(this.changes, c.id, items, KEEP_CHANGES)
      return items
    } catch (error: any) {
      this.log(`[host] changes of ${c.id}: ${error?.message ?? error}`)
      return null
    }
  }

  /** Lets active Inits finish. */
  async close() {
    await Promise.all([...this.creations.values(), this.storageLedgers.close()])
  }
}

/** A path's real path, or null when it does not exist. */
function safeReal(path: string): string | null {
  try { return realpathSync(path) } catch { return null }
}
