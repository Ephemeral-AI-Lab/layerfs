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
 *   state         branch.list metadata, sandbox.list and store.status: the
 *                 Store's facts, no Sandbox asked
 *   sandboxes     sandbox.list with each Sandbox's Workspaces (sandbox.get)
 *   createSandbox sandbox.create(key, name, machine, { image })
 *   closeSandbox  sandbox.close(name, { force })
 *   commits       commit.list, newest first, each Commit's changed paths from
 *                 commit.diff against its parent
 *   listDir/file  file.list, file.stat and file.read
 *   remove        project.remove (InUse while any Sandbox record exists)
 * Every LayerFS call is made once; a refusal passes through with its code.
 * Saved reads are pinned and paged. Store allocation and project tree
 * statistics are independent queries with separate scopes.
 */
import { randomUUID } from 'node:crypto'
import { mkdirSync, mkdtempSync, rmSync, statSync } from 'node:fs'
import { isAbsolute, join } from 'node:path'
import { StorageObservations } from './storage-observations.ts'
import { ProjectStorageLedgers } from './project-storage.ts'
import { pgMachineOf } from '../sessions/availability.ts'
import { PAGE, keyOf, pages, projectRef, refuse, sandboxObservation, type RouteHost } from './util.ts'

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

/** Tagged ids: a Layer's first byte is 0x32. */
const isLayer = (id: unknown) => typeof id === 'string' && id.startsWith('32')

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

/** A 2.0 Commit record -> CommitInfo. */
export const savedRef = (id: string | null | undefined, branch?: string | null) => id ? { id, kind: isLayer(id) ? 'Layer' : 'Commit', branch: branch ?? '' } : null
export const commitInfo = (c: any, branch?: string | null): any => ({
  id: c.id,
  branch: c.branch ?? c.ref?.branch ?? branch ?? '',
  message: c.message ?? '',
  at: typeof c.time === 'number' ? c.time : null,
  parent: c.parent ?? null,
  merged: c.second_parent ?? null,
  kind: c.via ?? null,
  base: c.base ?? null,
  sourceCommit: c.source_commit ?? null,
  sourceBranch: c.source_branch ?? null,
  sourceTime: c.source_time ?? null,
  logical: c.logical ?? null,
  savedKind: isLayer(c.id) ? 'layer' : 'commit',
  ref: c.ref ?? savedRef(c.id, c.branch ?? branch),
  parentRef: c.parent_ref ?? savedRef(c.parent ?? c.base, c.branch ?? branch),
  sourceRef: c.source_commit_ref ?? c.source_ref ?? savedRef(c.source_commit, c.source_branch),
  timeStatus: c.time_status ?? (typeof c.time === 'number' ? 'recorded' : 'unavailable'),
  newCanonical: c.new_canonical ?? null,
  newStored: c.new_stored ?? null,
  newMetadata: c.new_metadata ?? null,
})

/** An SDK `op` ({ id, name, phase, started, uncertain }), or null. */
export const opOf = (o: any) => (o && typeof o === 'object' && typeof o.name === 'string' ? { id: String(o.id ?? ''), name: o.name as string, phase: o.phase ?? '', started: o.started ?? null, uncertain: o.uncertain === true } : null)

/** A LayerFS refusal as the UI shows it: code, the SDK's text, and its detail (entity, phase, op, related). */
export const refusalOf = (error: any) => ({ code: error?.code ?? 'Failed', message: error?.message ?? String(error), detail: error?.detail ?? null })

/** Refusals a Store's operation gives every Store call: the page still renders what the home's records say. */
export const STORE_REFUSALS = new Set(['Busy', 'Unresolved', 'Unavailable'])

/** A sandbox.list/get record (and its Workspaces, when read) -> SandboxInfo. States are the SDK's (#537 U4). */
export function sandboxInfo(s: any, workspaces: any[] | null = null) {
  return {
    id: String(s.name),
    name: String(s.name),
    state: s.state ?? 'unknown',
    op: opOf(s.op),
    backend: s.backend ?? null,
    route: s.route ?? null,
    project: s.project ?? null,
    image: s.image ?? null,
    machine: s.machine ?? null,
    runtime: s.runtime ?? null,
    runtimeId: s.id ?? null, created: s.created ?? null, downReason: s.down_reason ?? null,
    ...(s.platform ? { platform: s.platform } : {}),
    workspaces: (workspaces ?? [])
      .filter((w) => w.mounted !== false && (w.directory || w.path))
      .map((w) => ({ ws: w.ws ?? `${s.name}:${w.name ?? w.directory}`, name: w.name ?? null, branch: w.branch ?? '', path: w.directory ?? w.path, activity: w.activity ?? null, base: w.base ?? null, baseLayer: w.base_layer ?? null, root: w.root ?? null, headMovedSinceMount: w.head_moved_since_mount ?? null, dirty: w.dirty ?? null, pendingMerge: w.pending_merge ?? null, revision: w.revision ?? null })),
    ...(workspaces ? {} : { unread: true }),
  }
}

/** BranchInfo[] from branch.list records and the Commits read (by id). */
export function branchInfos(listed: any[], byId: Map<string, any> | null = null) {
  const merges = byId ? [...byId.values()].filter((c) => c.via === 'merge') : []
  return listed.map((b) => {
    if (b.name === 'main') return { name: 'main', head: b.base ?? null, parent: null, forkedFrom: null, state: 'open', commits: 0 }
    const head = b.head ?? null
    const parent = b.forked_from ? (isLayer(b.forked_from) ? 'main' : (byId?.get(b.forked_from)?.branch ?? null)) : null
    let commits: number | null = byId ? 0 : null
    if (byId) for (const c of byId.values()) if (c.branch === b.name) commits!++
    // which Branches were published has no 2.0 read (§9 Q3): a Branch is open or merged
    const merged = !!head && merges.some((c) => c.branch !== b.name && (c.parent === head || c.second_parent === head || c.merged_from === head))
    return { name: b.name, head: head ?? b.base ?? null, parent, forkedFrom: b.forked_from ?? null, state: merged ? 'merged' : 'open', commits }
  })
}

// ---------- saved reads (project-reads.js) ----------

const fail = (message: string) => refuse(400, 'InvalidInput', message)
const integer = (q: URLSearchParams, k: string, fallback: number, max = 2000) => {
  const n = Number(q.get(k) ?? fallback)
  if (!Number.isSafeInteger(n) || n < 1 || n > max) throw fail(`${k} must be an integer from 1 to ${max}`)
  return n
}
const pinOf = (b: any) => b.saved_head?.id ?? (b.name === 'main' ? b.base : b.head ?? b.base)
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
  /** Init job id -> ProjectInitJob: the Inits this host started (2.0 keeps no job registry). */
  readonly inits = new Map<string, ProjectInitJob>()
  readonly creations = new Map<string, Promise<unknown>>()
  readonly emptySources: string
  readonly storageObservations: StorageObservations
  readonly storageLedgers: ProjectStorageLedgers
  /** Store name -> { state, op }: set by the host (Storage.projectJoin). */
  storeJoin: (() => Promise<Map<string, { state: string, op: unknown }>>) | null = null

  constructor(private readonly host: RouteHost) {
    this.emptySources = join(host.root, 'project-sources')
    this.storageObservations = new StorageObservations(host.root, host.home)
    this.storageLedgers = new ProjectStorageLedgers(host.root, host.home, () => host.lfs(), (line) => host.log(line))
  }

  get lfs() { return this.host.lfs() }

  private log(line: string): void { this.host.log(line) }

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
    const [machines, stores] = await Promise.all([this.lfs.machine.list(), this.lfs.store.list()])
    return {
      ...base,
      daemon: await this.daemonStatus(),
      machines: (machines.machines ?? []).map((m: any) => ({ name: m.name, platform: m.platform, state: m.state ?? null })),
      stores: (stores.items ?? []).length,
    }
  }

  /** macOS: whether the native machine has its daemon recorded (null elsewhere). */
  private async daemonStatus(): Promise<{ recorded: boolean, why: string | null } | null> {
    if (process.platform !== 'darwin') return null
    try {
      const recorded = (await this.lfs.machine.get('native'))?.daemon ?? null
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
      name: p.name,
      key,
      store: p.store,
      kind: p.kind ?? null,
      machine: p.machine ?? null,
      created: p.created ?? null,
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
    if (source !== undefined && (typeof source !== 'string' || !isAbsolute(source) || source.includes('\0') || !statSync(source, { throwIfNoEntry: false })?.isDirectory())) {
      throw refuse(400, 'InvalidInput', 'Choose an existing absolute source folder.')
    }
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
        (made) => Object.assign(job, { step: 'done', project: this.summary(made) }),
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
  async list() {
    const records = await pages((p) => this.lfs.project.list(p))
    // set by the host (Storage.projectJoin); a failure leaves the field out
    const join = this.storeJoin ? await this.storeJoin().catch(() => null) : null
    return records.map((p) => this.summary(p, join))
  }

  /** project.get of a key. */
  record(key: string): Promise<any> {
    return this.lfs.project.get(projectRef(key).key)
  }

  /**
   * ProjectState from the Store alone: its Branch metadata, its Sandbox
   * records and the Store file's size. No Sandbox is asked. While its Store
   * is busy (a push or pull) or unresolved, Store calls are refused (Busy,
   * Unresolved); while it is down none is made (U3): the page then gets what
   * the home's records say, the Store's state and op, and the refusal in
   * `refusal`, with no Branches.
   */
  async state(key: string) {
    const record = await this.record(key)
    const project = keyOf(record)
    // store.status first: on a stopped machine no Store call is made (U3); the banner offers Start
    const status = await this.lfs.store.status(record.store).catch(() => null)
    const down = status?.state === 'down'
    const [listed, sandboxes] = await Promise.all([
      down
        ? { refused: null }
        : pages((p) => this.lfs.branch.list(project, p)).catch((error: any) => {
            if (STORE_REFUSALS.has(error?.code)) return { refused: refusalOf(error) }
            throw error
          }),
      pages((p) => this.lfs.sandbox.list(project, p)).catch((error: any) => (this.log(`[host] sandbox.list of ${project}: ${error?.message ?? error}`), [] as any[])),
    ])
    const storeFacts = { storeState: status?.state ?? null, storeOp: opOf(status?.op), storeDown: status?.down_reason ?? null }
    const file = status?.file
    const store = { path: null, bytes: file?.exists ? (file.bytes ?? 0) + (file.wal_bytes ?? 0) : null }
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
    const main = listed.find((b: any) => b.name === 'main')
    return {
      project: { ...this.summary(record), ...storeFacts },
      branches: branchInfos(listed),
      // 2.0 lists none of main's Layers but its newest (§9 Q3)
      layers: main?.base ? [{ id: main.base, message: '', at: null, from: null }] : [],
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
    const listed = await pages((p) => this.lfs.sandbox.list(project, p))
    return listed.map((s) => sandboxInfo(s))
  }

  /**
   * A new Sandbox of the project: `{ name, machine?, image? }`. A sqlite
   * project's Sandboxes run on its Store's machine (another is Unsupported);
   * a PostgreSQL project's on a machine its Store is attached to
   * (`store.attach`), `machine` naming it (the default of `pgMachineOf`
   * when none is named); an unattached machine is refused here, in
   * LayerFS's words, before any LayerFS call.
   */
  async createSandbox(key: string, body: any = {}) {
    const record = await this.record(key)
    const { name, image } = body ?? {}
    if (typeof name !== 'string' || !name) throw refuse(400, 'InvalidInput', 'A Sandbox needs a name.')
    let machine = record.machine
    if (record.kind === 'postgresql') {
      const store = String(record.store ?? keyOf(record).split('/')[0])
      const attached = pgMachineOf(await this.lfs.store.get(store), body.machine)
      if (!attached) throw refuse(400, 'Unsupported', `Attach ${store} to a machine to run its projects.`)
      if (body.machine && body.machine !== attached) throw refuse(400, 'Unsupported', `PostgreSQL Store ${store} is not attached to machine ${body.machine}; \`layerfs store attach ${store} ${body.machine}\``)
      machine = attached
    }
    await this.host.ensureNative()
    return this.lfs.sandbox.create(keyOf(record), name, machine, image ? { image } : {})
  }

  /** One Sandbox with its Workspaces (sandbox.get). */
  async sandbox(name: string) {
    if (typeof name !== 'string' || !name) throw refuse(400, 'InvalidInput', 'Name the Sandbox.')
    const record = await sandboxObservation(this.lfs, name)
    return sandboxInfo({ ...record, name }, record.workspaces)
  }

  /**
   * Mounts a Workspace in Sandbox `name`: `{ branch, fork?, name? }`. Only
   * side Branches are mounted (owner D14: mounting main is refused), so
   * `fork: true` first forks `branch` from main. Answers workspace.mount's result.
   */
  async mount(sandbox: string, { branch, fork = false, name }: any = {}) {
    if (typeof sandbox !== 'string' || !sandbox) throw refuse(400, 'InvalidInput', 'Name the Sandbox.')
    checkBranch(branch)
    if (branch === 'main') throw refuse(400, 'InvalidInput', 'Only side Branches are mounted: fork a Branch from main and mount that.')
    if (fork) {
      const { project } = await this.lfs.sandbox.get(sandbox, { page_size: 1, page_pos: 0 })
      await this.lfs.branch.fork(project, branch, 'main')
    }
    return this.lfs.workspace.mount(sandbox, branch, name ? { name } : {})
  }

  /** workspace.unmount(ws, { discard }): refused while it has changes unless discarded. */
  async unmount(ws: string, { discard = false } = {}) {
    if (typeof ws !== 'string' || !ws.includes(':')) throw refuse(400, 'InvalidInput', 'A Workspace is named <sandbox>:<name or path>.')
    return this.lfs.workspace.unmount(ws, discard ? { discard: true } : {})
  }

  /** sandbox.close(name, { force }); answers its result, with the `unknown` Workspaces a forced close left. */
  async closeSandbox(name: string, { force = false } = {}) {
    if (typeof name !== 'string' || !name) throw refuse(400, 'InvalidInput', 'Name the Sandbox to close.')
    // A session's Sandbox holds its mounted Workspace: save a checkpoint and close it as the session does.
    if (!force && await this.host.closeSessionSandbox?.(name)) return { kind: 'Closed', sandbox: name, closed: name, session: true }
    const result = await this.lfs.sandbox.close(name, force ? { force: true } : {})
    await this.host.sandboxClosed?.(name)
    return result
  }

  /** Closes every Sandbox of the project (`force` passes on); answers each close. */
  async closeSandboxes(key: string, { force = false } = {}) {
    const { key: project } = projectRef(key)
    const listed = await pages((p) => this.lfs.sandbox.list(project, p))
    const out = []
    for (const s of listed) out.push(await this.closeSandbox(s.name, { force }))
    return out
  }

  /** project.remove: refused InUse while any of its Sandbox records exists, Unsupported for a PostgreSQL project. */
  async remove(key: string) {
    const { key: project } = projectRef(key)
    // DSH sessions of the project hold Sandboxes (InUse): save and close them first.
    await this.host.projectRemoving?.(project)
    await this.lfs.project.remove(project)
    await this.host.projectRemoved?.(project)
    return { removed: true }
  }

  // ---------- Store statistics ----------

  async storeStats(name: string) {
    let stats = await this.lfs.store.stats(name)
    if (stats.kind === 'sqlite') {
      try {
        const status = await this.lfs.store.status(name), file = status.file
        stats = { ...stats, fileMeasurements: file?.exists === true ? { observedAt: Date.now(), bytes: file.bytes ?? null, walBytes: file.wal_bytes ?? null, incomingBytes: file.incoming_bytes ?? null } : null }
        if (file?.unavailable || status.down_reason) stats.fileMeasurementError = file?.unavailable ?? status.down_reason
      } catch (error: any) {
        stats = { ...stats, fileMeasurements: null, fileMeasurementError: String(error?.message ?? error) }
      }
    }
    try {
      await this.storageObservations.record(name, stats, await this.lfs.store.get(name))
      return stats
    } catch (error: any) {
      // Measurement success remains visible even if local observation persistence fails.
      return { ...stats, observationError: String(error?.message ?? error) }
    }
  }

  async storeObservations(name: string) { return this.storageObservations.read(name) }

  // ---------- a project's own storage ----------

  /**
   * ProjectStorage: this project's saves summed from their Commit counters
   * (the ledger, refreshed in the background), its Store's totals for the
   * share, and main's latest tree. `?refresh=1` starts a refresh now.
   */
  async projectStorage(key: string, q: URLSearchParams) {
    const record = await this.record(key)
    const project = keyOf(record), created = typeof record.created === 'number' ? record.created : null
    const [view, store, storeRecord, main] = await Promise.all([
      this.storageLedgers.view(project, created, { refresh: q.get('refresh') === '1' }),
      this.lfs.store.stats(record.store).catch((error: any) => ({ refusal: refusalOf(error) })),
      this.lfs.store.get(record.store).catch(() => null),
      this.lfs.project.stats(project).catch(() => null),
    ])
    const projects = Array.isArray(storeRecord?.projects) ? new Set(storeRecord.projects).size : null
    return {
      ...view,
      store: { name: record.store, projects, ...('refusal' in store ? { refusal: store.refusal } : { historyLogical: store.history_logical ?? null, canonical: store.canonical ?? null, stored: store.stored ?? null, metadata: store.metadata ?? null, file: store.file ?? null }) },
      main: main ? { logical: main.logical ?? null, files: main.files ?? null, directories: main.directories ?? null, complete: main.complete !== false } : null,
    }
  }

  async history(key: string, q: URLSearchParams) {
    const project = projectRef(key).key
    const branch = q.get('branch')
    if (!branch) {
      const pos = Number(q.get('pos') ?? 0)
      if (!Number.isSafeInteger(pos) || pos < 0) throw fail('pos must be a nonnegative integer')
      const page = await this.lfs.branch.list(project, { page_size: 100, page_pos: pos })
      const items = [...page.items]
      if (pos === 0 && !items.some((b: any) => b.name === 'main')) items.unshift(await this.lfs.branch.get(project, 'main'))
      return { branches: items.map((b: any) => ({ name: b.name, head: pinOf(b), base: b.base, forkedFrom: b.forked_from ?? null })), next: page.next_page_pos ?? null }
    }
    const commit = q.get('commit')
    if (commit) {
      const c = await this.lfs.branch.get_commit(project, branch, commit)
      const from = q.get('from') ?? c.parent ?? c.base
      if (!from) return { item: commitInfo(c, branch), changes: [], nextChanges: null }
      const diff = await this.lfs.commit.diff(project, from, c.id, { paths_only: true, page_size: 100, ...(q.get('cursor') ? { cursor: q.get('cursor') } : {}) })
      return { item: commitInfo(c, branch), changes: diff.items ?? [], nextChanges: diff.next_cursor ?? null }
    }
    // Resume from the immutable last parent, rather than re-reading a moving head.
    const at = q.get('at') ?? q.get('head')
    const fork = branch === 'main' ? null : (await this.lfs.branch.get(project, branch)).forked_from
    const page = await this.lfs.branch.commits(project, branch, { page: 1, page_size: HISTORY_PAGE_SIZE, ...(at ? { at_commit: at } : {}) })
    const all: any[] = page.items ?? []
    const boundary = fork ? all.findIndex((c) => c.id === fork) : -1
    const items = boundary < 0 ? all : all.slice(0, boundary)
    const parent = items.at(-1)?.parent
    return { items: items.map((c) => commitInfo(c, branch)), head: page.at_commit?.id ?? (typeof page.at_commit === 'string' ? page.at_commit : null) ?? at ?? items[0]?.id ?? null, next: page.has_more && boundary < 0 && parent !== fork ? parent ?? null : null }
  }

  /** One bounded immutable directory page; metadata is read only for opened files. */
  async listDir(key: string, branch: string, path: string, pin: string | null, pos: string | number = 0) {
    validate(branch, path)
    const project = projectRef(key).key
    const at = await this.saved(project, branch, pin)
    if (!Number.isSafeInteger(Number(pos)) || Number(pos) < 0) throw fail('pos must be a nonnegative integer')
    const page = await this.lfs.file.list(project, branch, path, at, { page_size: 1000, page_pos: Number(pos) })
    return { items: page.items.map((i: any) => ({ name: i.name, path: i.path, kind: i.type ?? 'file', size: i.size ?? null, mode: i.mode ?? null })), next: page.has_more ? page.next_page_pos ?? null : null, at: page.at ?? { id: at } }
  }

  async file(key: string, branch: string, path: string | null, pin: string | null, line: string | number = 1) {
    validate(branch, path)
    const project = projectRef(key).key
    const at = await this.saved(project, branch, pin)
    if (!Number.isSafeInteger(Number(line)) || Number(line) < 1) throw fail('line must be a positive integer')
    const stat = await this.lfs.file.stat(project, branch, path, at)
    try {
      const window = await this.lfs.file.read(project, branch, path, at, { start_line: Number(line), end_line: Number(line) + 999 })
      return { ...window, path, size: stat.size, mode: stat.mode, binary: false, at: window.at ?? { id: at } }
    } catch (e: any) {
      if (e?.code === 'Binary') return { path, size: stat.size, binary: true, text: null, at: { id: at } }
      throw e
    }
  }

  async fileQuery(key: string, q: URLSearchParams) {
    const project = projectRef(key).key, branch = q.get('branch') ?? 'main', path = q.get('path')
    validate(branch, path)
    const at = await this.saved(project, branch, q.get('pin'))
    const kind = q.get('kind'), pageSize = integer(q, 'size', 50, 1000)
    if (kind === 'blame') return this.lfs.file.blame(project, branch, path, at, { from_line: integer(q, 'line', 1, Number.MAX_SAFE_INTEGER), line_count: 1000 })
    if (kind === 'revisions') return this.lfs.file.revision(project, branch, path, { at_commit: at, traversal: q.get('traversal') ?? 'provenance', page_size: pageSize, ...(q.get('continuation') ? { continuation: q.get('continuation') } : { page: integer(q, 'page', 1) }) })
    if (kind === 'compare' || kind === 'attribution') {
      const left = q.get('left'), rightBranch = q.get('rightBranch') ?? branch
      const leftPin = await this.saved(project, rightBranch, left)
      return this.lfs.file.diff(project, rightBranch, path, branch, path, { left_commit: leftPin, right_commit: at, allow_missing: true, include_attribution: kind === 'attribution', page: integer(q, 'page', 1), page_size: pageSize })
    }
    if (kind === 'branch-diff') {
      const left = await this.saved(project, q.get('rightBranch') ?? branch, q.get('left'))
      return this.lfs.branch.diff(project, q.get('rightBranch') ?? branch, branch, { from_commit: left, to_commit: at, include_attribution: true, path_prefix: path, page_size: pageSize, ...(q.get('continuation') ? { continuation: q.get('continuation') } : { page: integer(q, 'page', 1) }) })
    }
    throw fail('Unknown saved-file query.')
  }

  async branches(key: string, q: URLSearchParams) {
    const project = projectRef(key).key
    if (q.get('branch')) return this.lfs.branch.get(project, q.get('branch'))
    const pos = Number(q.get('pos') ?? 0)
    if (!Number.isSafeInteger(pos) || pos < 0) throw fail('pos must be a nonnegative integer')
    const page = await this.lfs.branch.list(project, { page_size: 100, page_pos: pos })
    return { items: page.items.map((b: any) => ({ name: b.name, head: pinOf(b), parent: null, forkedFrom: b.forked_from ?? null, state: 'open', commits: null })), next: page.next_page_pos ?? null }
  }

  async projectStats(key: string, q: URLSearchParams) {
    const project = projectRef(key).key, branch = q.get('branch') ?? 'main'
    const at = await this.saved(project, branch, q.get('pin'))
    return { ...await this.lfs.project.stats(project, branch, at), projectKey: project }
  }

  // ---------- Commits ----------

  async commits(key: string, branch: string, limitText: string | null) {
    checkBranch(branch)
    const limit = limitText == null || limitText === '' ? LIMIT : Number(limitText)
    if (!Number.isSafeInteger(limit) || limit < 1 || limit > PAGE) throw refuse(400, 'InvalidInput', `limit must be a whole number from 1 to ${PAGE}`)
    const { key: project } = projectRef(key)
    // main is a chain of Layers, not a Branch: commit.list refuses it
    if (branch === 'main') return []
    const b = await this.lfs.branch.get(project, branch)
    return this.withChanges(project, branch, await this.ownCommits(project, b, limit))
  }

  /** `{ [branch]: CommitInfo[] }` for several Branches (main's are []); a Branch gone since the caller saw it is left out. */
  async branchCommits(key: string, branches: string[]) {
    const { key: project } = projectRef(key)
    if (!Array.isArray(branches) || branches.length === 0) return {}
    if (branches.length > PAGE) throw refuse(400, 'InvalidInput', `at most ${PAGE} Branches at once`)
    for (const b of branches) checkBranch(b)
    const wanted = [...new Set(branches)]
    const out: Record<string, unknown> = Object.fromEntries(wanted.filter((b) => b === 'main').map((b) => [b, []]))
    const listed = new Map((await pages((p) => this.lfs.branch.list(project, p))).map((b: any) => [b.name, b]))
    await each(wanted.filter((b) => b !== 'main' && listed.has(b)), WIDTH, async (branch) => {
      try {
        out[branch] = await this.withChanges(project, branch, await this.ownCommits(project, listed.get(branch), LIMIT))
      } catch (error: any) {
        if (error?.code !== 'NoSuchBranch') throw error
      }
    })
    return out
  }

  /** A Branch's own Commits (records), newest first: commit.list stops at the fork. */
  async ownCommits(project: string, branch: any, limit: number) {
    const items: any[] = []
    let at: string | undefined
    while (items.length < limit) {
      const result = await this.lfs.branch.commits(project, branch.name, { page: 1, page_size: Math.min(HISTORY_PAGE_SIZE, limit - items.length), ...(at ? { at_commit: at } : {}) })
      const rows = (result.items ?? []).map((c: any) => ({ ...c, branch: c.branch ?? c.ref?.branch ?? branch.name }))
      const boundary = rows.findIndex((c: any) => c.id === branch.forked_from)
      items.push(...(boundary < 0 ? rows : rows.slice(0, boundary)))
      const parent = rows.at(-1)?.parent
      if (!result.has_more || boundary >= 0 || !parent || parent === branch.forked_from || parent === at || !rows.length) break
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
   * One Commit's changed paths: commit.diff from its parent. A Branch's first
   * Commit has no parent Commit and 2.0 cannot diff a Layer with a Commit
   * (§9 Q3), so its changes are left out. A failed diff is logged.
   */
  async changesOf(project: string, c: any) {
    if (this.changes.has(c.id)) return this.changes.get(c.id)
    if (!c.parent) return null
    try {
      const items: any[] = []
      for (let cursor: string | null = null; ;) {
        const page: any = await this.lfs.commit.diff(project, c.parent, c.id, { paths_only: true, page_size: PAGE, ...(cursor ? { cursor } : {}) })
        items.push(...page.items.map(({ path, kind }: any) => ({ path, kind: String(kind).toLowerCase() })))
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
