/**
 * Connect's host route table (Connect `host/server.js` `route()`), the refusal
 * code -> HTTP status map and the error body `{ code, transport?, message,
 * detail?, conversion? }`. No HTTP server, token or CORS: the `http`
 * operation hands `{ method, path, body }` here and answers `{ status, body }`.
 */
import { SandboxCommands, sandboxOfWs } from './commands.ts'
import { DoctorChecks } from './doctor.ts'
import { Machines } from './machines.ts'
import { Projects } from './projects.ts'
import { Storage } from './storage.ts'
import { Answer, fixOf, invalidateSandbox, wire, type RouteHost } from './util.ts'
import type { SandboxCreates } from './sandbox-creates.ts'

/** LayerFS refusal codes and the HTTP status each answers. Anything else is 500. */
export const STATUS: Record<string, number> = {
  InvalidInput: 400,
  IsDirectory: 400,
  NotDirectory: 400,
  NotFile: 400,
  NotText: 400,
  Binary: 400,
  TooLarge: 400,
  NoSuchProject: 404,
  NoSuchPath: 404,
  NoSuchBranch: 404,
  NoSuchCommit: 404,
  NoSuchSandbox: 404,
  NoSuchStore: 404,
  NoSuchWorkspace: 404,
  NoMatch: 400,
  AmbiguousMatch: 400,
  NotFound: 404,
  // the SDK's: an operation holds the entity (detail names it, its phase and op)
  Busy: 409,
  // the host's own: one Sandbox-page command per Workspace (commands.ts)
  CommandRunning: 409,
  // the SDK's: an operation's outcome was lost; the message names the clearing call
  Unresolved: 409,
  // the host's own guards (one job per Store on this host)
  ConnectBusy: 409,
  // LayerFS's: not enough free disk (detail { needed_bytes, free_bytes, path }, fix)
  Capacity: 507,
  Unknown: 500,
  Conflict: 409,
  Exists: 409,
  NameTaken: 409,
  HeadMoved: 409,
  Dirty: 409,
  ReadOnly: 409,
  InUse: 409,
  // LayerFS's: a Sandbox closed while a Workspace is still mounted in it
  MountsActive: 409,
  Denied: 403,
  Unavailable: 502,
  // the host's own: no runtime chosen, or a LayerFS folder not set up yet (the setup modal fixes both)
  CliUnavailable: 503,
  Uninitialized: 503,
  MixedBuild: 409,
  Unsupported: 501,
  // a Workspace or Sandbox that is not (or no longer) what the caller named
  NotMounted: 409,
  StaleSandbox: 409,
  Cancelled: 409,
  NoStore: 404,
  NoSuchBucket: 404,
  // a call (or the host's wait for a Sandbox read) that did not answer in time
  Timeout: 504,
  // the host's own: a project name now names another project (sessions/catalog.ts)
  ProjectIdentityChanged: 409,
}

/** Expected refusals answered 5xx (a stopped machine, a full disk, a CLI without the feature): no stack logged. */
const EXPECTED = new Set(['Unavailable', 'Capacity', 'Unsupported'])

/** An error the routes throw: answered as { code, message } with `status`. */
export const httpError = (status: number, code: string, message: string) => Object.assign(new Error(message), { status, code })

/**
 * A layout 2 home's conversion refusal (#536 F2): `InUse` whose
 * `detail.related` names what is live with `why` (what ends it).
 */
export const isConversion = (code: string, detail: any) => code === 'InUse' && Array.isArray(detail?.related) && detail.related.some((r: any) => typeof r?.why === 'string')

/**
 * The status and body an error answers: { code, message, detail?, fix? } with
 * the SDK's refusal as it came (its text names any status or clearing call;
 * `detail` holds entity, phase, op and related; `fix` is LayerFS's one
 * runnable line, which the page offers as its action), and `conversion: true` for
 * a layout 2 home's conversion refusal.
 */
export function errorAnswer(error: any): { status: number, body: Record<string, unknown> } {
  const code = typeof error?.code === 'string' ? error.code : 'failed'
  const status = Number.isInteger(error?.status) ? error.status : (STATUS[code] ?? 500)
  const own = error?.detail && typeof error.detail === 'object' ? error.detail : null
  // LayerFS puts a blocked refusal's cause beside `error` (`blocked_by`, `detail` null): keep it in `detail` for the page
  const blocked = error?.blocked_by && typeof error.blocked_by === 'object' && !own?.blocked_by ? error.blocked_by : null
  const detail = blocked ? { ...own, blocked_by: blocked } : own
  return { status, body: wire({ code, ...(error?.transport ? { transport: error.transport } : {}), message: error?.message ?? String(error), ...(detail ? { detail } : {}), ...fixOf(error), ...(isConversion(code, detail) ? { conversion: true } : {}) }) }
}

/** The route groups behind the table. */
export interface Groups {
  readonly host: RouteHost
  readonly machines: Machines
  readonly storage: Storage
  readonly projects: Projects
  /** The Doctor page: the home's tree, the 2.0 doctors and their fixes (doctor.ts). */
  readonly checks: DoctorChecks
  /** Commands a Sandbox page runs in its Workspaces, and their log (commands.ts). */
  readonly commands: SandboxCommands
  /** Sandbox creations run in the background, with their progress (sandbox-creates.ts). */
  readonly sandboxCreates: SandboxCreates
  /** Runtime facts for `GET /api/status` (`ctx.layerfs.status()`). */
  readonly runtime: () => Promise<unknown>
  /** `/api/demo-presets…` (presets/manager.ts), once attached. */
  presets?: PresetRoutes
}

/** What the router needs from the presets manager. */
export interface PresetRoutes {
  route(method: string, parts: string[], body: unknown, query: URLSearchParams): Promise<unknown>
  /** A preset build or reset holds the home: other changes are refused, reads stay open. */
  exclusive(): boolean
}

/** Build the route groups over one host, wired as Connect's main.js wires them. */
export function createGroups(host: RouteHost, runtime: () => Promise<unknown>): Groups {
  const projects = new Projects(host)
  const storage = new Storage(host)
  // the project list shows each project's Store state from the Storage view
  projects.storeJoin = () => storage.projectJoin()
  // Compute's "Move to…" starts a job in Storage's table, so its progress shows there
  const machines = new Machines(host, (store, machine, made) => storage.place(store, machine, made))
  // a fix the Doctor page runs goes the way the Machines and Projects pages' own buttons go
  const checks = new DoctorChecks(host, undefined, { actions: { machineStart: (name) => machines.start(name), machineStop: (name) => machines.stop(name), projectDelete: (key) => projects.remove(key), backupCreate: (store, bucket) => storage.backupAndWait(store, bucket) } })
  const commands = new SandboxCommands(host)
  // a closed Sandbox's command logs go with it (its Workspace paths restart with a new daemon)
  projects.sandboxClosedHook = (name) => commands.closed(name)
  // sandbox.get's running commands: the ones a Sandbox page of this host started are its own
  projects.ownExec = (ws, id) => commands.started(ws, id)
  // "Start and create" starts the machine as the Machines page does (machine-progress events)
  projects.startMachine = (name) => machines.start(name)
  projects.sandboxMade = (name) => machines.metrics.created(name)
  // Stage 1: a Docker image that cannot fit on the machine is refused Capacity before its copy starts
  projects.spaceFor = (machine, image) => machines.spaceFor(machine, image)
  // a background create that ends changes the machines view: later GETs read anew
  projects.sandboxCreates.settled = () => machines.changed()
  return { host, machines, storage, projects, checks, commands, sandboxCreates: projects.sandboxCreates, runtime }
}

/** method + path segments -> the answer, or undefined when no route matches. */
export async function route(g: Groups, method: string, parts: string[], query: URLSearchParams, body: any, signal?: AbortSignal): Promise<unknown> {
  const { machines, storage, projects } = g
  const [api, group, id, sub, ...rest] = parts
  // any write may change the machines view: a GET after it must not join a read started before
  if (method !== 'GET') machines.changed()
  if (api === 'api' && group === 'demo-presets') return g.presets?.route(method, parts.slice(2), body, query)
  // the Doctor page: /api/doctor/tree, /results, /runs, /runs/:id/stop, /fixes 
  if (api === 'api' && group === 'doctor' && id) return g.checks.route(method, parts.slice(2), body)
  if (api !== 'api' || rest.length) return undefined
  // Stores, buckets and the moves, backups and restores this host started
  if (group === 'storage' && !id && method === 'GET') return storage.view(query.get('fresh') === '1')
  // lists the buckets' files again (backup.list, one call per bucket): { bucket? }, empty for all
  if (group === 'storage' && id === 'check' && !sub && method === 'POST') return storage.check(body ?? {})
  if (group === 'stores') {
    if (!id && method === 'POST') return storage.createStore(body ?? {})
    // a PostgreSQL database: { name, url, auth | passwordFrom | user + passwordValue + authName? (a new login), synchronousCommit? }
    if (id === 'postgresql' && !sub && method === 'POST') return storage.connectPostgresql(body ?? {})
    if (id && sub === 'stats' && method === 'GET') return projects.storeStats(id)
    if (id && sub === 'observations' && method === 'GET') return projects.storeObservations(id)
    if (id && sub === 'projects' && method === 'GET') return projects.storeProjects(id)
    if (id && !sub && method === 'DELETE') return storage.removeStore(id, { confirm: query.get('confirm') })
    if (id && sub === 'disconnect' && method === 'POST') return storage.disconnectStore(id)
    // a PostgreSQL Store's connection, or one machine's own address: { machine?, url?, clear?, auth? | passwordFrom? | passwordValue?, synchronousCommit? }
    if (id && sub === 'connection' && method === 'POST') return storage.updateStore(id, body ?? {})
    // the connection check from a machine (store doctor --machine): ?machine=<name>
    if (id && sub === 'check' && method === 'GET') return storage.checkStore(id, query.get('machine'))
    // the machine a PostgreSQL Store's sessions run on by default: { machine }
    // a sqlite Store on native to a machine (store move): { machine }
    if (id && sub === 'push' && method === 'POST') return storage.push(id, body?.machine)
    // a sqlite Store on a machine back to native (store move <s> native)
    if (id && sub === 'pull' && method === 'POST') return storage.pull(id)
    // between two machines, two moves through native in one job: { machine }
    if (id && sub === 'move' && method === 'POST') return storage.move(id, body?.machine)
    // { bucket, file? }
    if (id && sub === 'backup' && method === 'POST') return storage.backup(id, body ?? {})
    // reclaim the Store's dead bytes (store.compact), as a job
    if (id && sub === 'compact' && method === 'POST') return storage.compact(id)
    return undefined
  }
  if (group === 'buckets') {
    // { name, url | parts, accessKey, secretKey }: the secret goes to LayerFS on stdin
    if (!id && method === 'POST') return storage.connectBucket(body ?? {})
    if (id && !sub && method === 'DELETE') return storage.disconnectBucket(id)
    if (id && sub === 'files' && method === 'GET') return storage.listBucket(id)
    // { store (the new Store's name), file? }: installs on native
    if (id && sub === 'restore' && method === 'POST') return storage.restore({ ...(body ?? {}), bucket: id })
    return undefined
  }
  // the home's saved PostgreSQL logins (store auths): never a password
  if (group === 'store-auths' && !id && method === 'GET') return storage.auths()
  // a login's new password (store save-auth under its own name), then every Store naming it checked; a login no Store names removed
  if (group === 'store-auths' && id && sub === 'rotate' && method === 'POST') return storage.rotateLogin(id, body ?? {})
  if (group === 'store-auths' && id && !sub && method === 'DELETE') return storage.removeLogin(id)
  if (group === 'storage-jobs' && id && !sub && method === 'GET') return storage.job(id)
  if (group === 'storage-jobs' && id && sub === 'inspect' && method === 'POST') return storage.inspect(id)
  if (group === 'storage-jobs' && id && sub === 'retry' && method === 'POST') return storage.retry(id)
  // Compute's machines. Power is manual (#535 D11): start and stop are their own calls, and no read starts one
  if (group === 'platforms' && !id && method === 'GET') return machines.platforms()
  // machine and Sandbox metrics: ?machines=a,b&since=<seq>, answered from the sampler's memory (no LayerFS call, layerfs-project#182 D2)
  if (group === 'metrics' && !id && method === 'GET') return machines.metrics.view(query)
  // Sandbox creations run in the background (`background: true`): running and failed jobs; dismiss a failed one
  if (group === 'sandbox-creates' && !sub) {
    if (!id && method === 'GET') return g.sandboxCreates.list()
    if (id && method === 'DELETE') return g.sandboxCreates.dismiss(id)
  }
  // image copies run in the background: running and failed copies; dismiss a failed one
  if (group === 'image-loads' && !sub) {
    if (!id && method === 'GET') return machines.loads.list()
    if (id && method === 'DELETE') return machines.loads.dismiss(id)
  }
  if (group === 'machines') {
    if (!id && !sub && method === 'GET') return machines.view(query.get('fresh') === '1')
    if (id && sub === 'inspect' && method === 'GET') return machines.inspect(id)
    // the machine's worker log (machine.log): ?before=, ?limit=, ?rid= (a request the page can cancel); refusals answer as `stale`
    if (id && sub === 'log' && method === 'GET') return machines.machineLog(id, query, signal)
    // the machine's files (machine.files): its roots without ?path=; a folder page (?cursor=, ?depth=, ?limit=) or a file window (?offset=, ?length=); ?rid= as the log
    if (id && sub === 'files' && method === 'GET') return machines.machineFiles(id, query, signal)
    // what the machine has and uses now, as the metrics sampler last read it (no LayerFS call); a gap answers as `stale`
    if (id && sub === 'stats' && method === 'GET') return machines.machineStats(id)
    // where its data disk goes (machine.stats --with space): kept a minute per machine, ?fresh=1 reads now; a refusal answers as `stale`
    if (id && sub === 'space' && method === 'GET') return machines.machineSpace(id, query)
    // what a Sandbox on it can run (the New Sandbox menu, the Images section); never starts it
    if (id && sub === 'sandbox-images' && method === 'GET') return machines.sandboxImages(id)
    // { images?: id[], unused?: boolean } -> { removed, freed, refused }: 501 while the pinned CLI has no machine.prune
    if (id && sub === 'prune-images' && method === 'POST') return machines.pruneImages(id, body ?? {})
    // { image } -> 202 { load }: machine.load_image in the background (`image-load` events); 501 while the pinned CLI has no machine.load_image
    if (id && sub === 'load-image' && method === 'POST') return machines.loadImage(id, body ?? {})
    if (id && sub === 'start' && method === 'POST') return machines.start(id)
    // an outdated worker's fix: machine stop then machine start, refused InUse while Sandboxes are on it
    if (id && sub === 'restart' && method === 'POST') return machines.restart(id)
    if (id && sub === 'stop' && method === 'POST') return machines.stop(id, { force: body?.force === true })
    if (!id && !sub && method === 'POST') return machines.create(body)
    if (id && !sub && method === 'DELETE') return machines.remove(id)
    if (id && sub === 'update' && method === 'POST') return machines.update(id, body)
    // puts a sqlite Store here (push, pull, or pull then push): { store }
    if (id && sub === 'stores' && method === 'POST') return machines.moveStore(body?.store, id, body?.made === true)
  }
  if (group === 'status' && !id && method === 'GET') return projects.status(await g.runtime())
  // `id` is a project key, `<store>/<project>`, sent as one encoded segment
  if (group === 'projects') {
    if (!id) {
      if (method === 'GET') return projects.list(query.get('fresh') === '1')
      if (method === 'POST') return projects.create(body)
      return undefined
    }
    const projectId = query.get('projectId')
    if (projectId) await g.host.assertProjectIdentity?.(id, projectId)
    if (!sub && method === 'GET') return projects.state(id)
    if (!sub && method === 'DELETE') return projects.remove(id)
    // a new Sandbox: { name, image? (a sha256 digest until Stage 1), machine? (PostgreSQL projects only), start?, background? }
    if (sub === 'sandboxes' && method === 'POST') return projects.createSandbox(id, body ?? {})
    // closes every Sandbox of the project: { force? }
    if (sub === 'close-sandboxes' && method === 'POST') return projects.closeSandboxes(id, { force: body?.force === true })
    if (method !== 'GET') return undefined
    const branch = query.get('branch') ?? 'main'
    if (sub === 'history') return projects.history(id, query)
    // this project's own storage from its saves' counters (see project-storage.ts): ?refresh=1
    if (sub === 'storage') return projects.projectStorage(id, query)
    // the Overview card's size: packed bytes (the Store's when alone in it: ?alone=1) and main's raw bytes
    if (sub === 'size') return projects.projectSize(id, query)
    if (sub === 'branches') return projects.branches(id, query)
    if (sub === 'file-query') return projects.fileQuery(id, query)
    if (sub === 'stats') return projects.projectStats(id, query)
    if (sub === 'commits') return projects.commits(id, branch, query.get('limit'))
    // several Branches' Commits in one call: ?branch=a&branch=b
    if (sub === 'branch-commits') return projects.branchCommits(id, query.getAll('branch'))
    // the Sandboxes (records; Workspaces on demand)
    if (sub === 'sandboxes') return projects.sandboxes(id)
    if (sub === 'fs') return projects.listDir(id, branch, query.get('path') ?? '/', query.get('pin'), query.get('pos') ?? 0)
    if (sub === 'file') return projects.file(id, branch, query.get('path'), query.get('pin'), query.get('line') ?? 1)
    return undefined
  }
  // sandbox.remove(name, { force }): ?force=1; drops the Sandbox's command logs once removed
  if (group === 'sandboxes' && id && !sub && method === 'DELETE') return projects.closeSandbox(id, { force: query.get('force') === '1' })
  // the Sandbox's daemon log (sandbox.log), as the machine's; never waits on sandbox.get
  if (group === 'sandboxes' && id && sub === 'log' && method === 'GET') return machines.sandboxLog(id, query, signal)
  // the Sandbox's files (sandbox.files), as the machine's; never waits on sandbox.get
  if (group === 'sandboxes' && id && sub === 'files' && method === 'GET') return machines.sandboxFiles(id, query, signal)
  // one Sandbox with its Workspaces (sandbox.get)
  if (group === 'sandboxes' && id && !sub && method === 'GET') return projects.sandbox(id)
  // the commands this host ran in the Sandbox's Workspaces, newest first, at most 50 (commands.ts)
  if (group === 'sandboxes' && id && sub === 'commands' && method === 'GET') return g.commands.list(id)
  // one Workspace's commands, newest first
  if (group === 'workspaces' && id && sub === 'commands' && method === 'GET') return g.commands.listWorkspace(id)
  // workspace.exec in the background: { command } -> the running entry; 409 CommandRunning while one runs there
  if (group === 'workspaces' && id && sub === 'commands' && method === 'POST') return g.commands.start(id, body ?? {})
  // workspace.cancel --exec of a command started here: { id }
  if (group === 'workspaces' && id && sub === 'cancel' && method === 'POST') return g.commands.cancel(id, body ?? {})
  // workspace.commit: captures the overlay as a new Commit on the Workspace's Branch: { message? }
  if (group === 'workspaces' && id && sub === 'commit' && method === 'POST') {
    const sandbox = sandboxOfWs(id)
    const message = typeof body?.message === 'string' && body.message.trim() ? body.message.trim().slice(0, 4096) : undefined
    const lfs = projects.lfs
    try { return await lfs.workspace.commit(id, { detail: true, ...(message ? { message } : {}) }) } finally { invalidateSandbox(lfs, sandbox) }
  }
  // a Workspace in it: { branch, fork?, name? } (only side Branches are mounted, D14)
  if (group === 'sandboxes' && id && sub === 'workspaces' && method === 'POST') return projects.mount(id, body ?? {})
  // workspace.status, the long form: changes, branch_moved, state and companions, overlay counts, pending_merge
  if (group === 'workspaces' && id && sub === 'status' && method === 'GET') return projects.lfs.workspace.status(id, { detail: true })
  // workspace.unmount: ?discard=1&force=1; without force, 409 CommandRunning while this host runs a command there.
  // LayerFS's refusals (Dirty with detail.changes, Busy with detail.running, Unknown) pass through; once unmounted, its command log goes.
  if (group === 'workspaces' && id && !sub && method === 'DELETE') {
    const force = query.get('force') === '1'
    sandboxOfWs(id)
    if (!force) g.commands.assertIdle(id, 'Cancel it first, or force the unmount.')
    const result = await projects.unmount(id, { discard: query.get('discard') === '1', force })
    g.commands.unmounted(id)
    return result
  }
  // a folder an Init may copy from, checked before a Store is made: { source } -> { source (real path) }
  if (group === 'project-sources' && id === 'check' && !sub && method === 'POST') return projects.checkSource(body ?? {})
  if (group === 'project-inits' && !id && method === 'GET') return projects.initJobs()
  if (group === 'project-inits' && id && !sub && method === 'GET') return projects.init(id)
  return undefined
}

/** One request: `{ method, path, body }` -> `{ status, body }` (JSON, unsafe integers tagged `i64-v1`). */
export async function handle(g: Groups, request: { method?: string, path?: string, body?: unknown }, signal?: AbortSignal): Promise<{ status: number, body: unknown }> {
  const method = String(request?.method ?? 'GET').toUpperCase()
  let url: URL
  try {
    url = new URL(String(request?.path ?? '/'), 'http://127.0.0.1')
  } catch {
    return { status: 400, body: { code: 'InvalidInput', message: 'bad request URL' } }
  }
  let parts: string[]
  try {
    parts = url.pathname.split('/').filter(Boolean).map(decodeURIComponent)
  } catch {
    return { status: 400, body: { code: 'InvalidInput', message: 'bad path encoding' } }
  }
  try {
    let body: unknown = {}
    if (method === 'POST' && request.body !== undefined && request.body !== null) {
      if (typeof request.body === 'string') {
        try { body = JSON.parse(request.body) } catch { throw httpError(400, 'InvalidInput', 'Invalid JSON request body.') }
      } else body = request.body
    }
    if (method !== 'GET' && parts[1] !== 'demo-presets' && g.presets?.exclusive()) {
      throw httpError(409, 'ConnectBusy', 'A preset build or reset is changing this LayerFS home. Changes are available when it finishes; you can keep browsing.')
    }
    // Connect recorded the native daemon at boot; here once, before the first call that needs it
    await g.host.ensureNative()
    const answer = await route(g, method, parts, url.searchParams, body, signal)
    if (answer === undefined) return { status: 404, body: { code: 'NotFound', message: `no route ${method} ${url.pathname}` } }
    if (answer instanceof Answer) return { status: answer.status, body: wire(answer.body) ?? null }
    return { status: 200, body: wire(answer) ?? null }
  } catch (error: any) {
    const answer = errorAnswer(error)
    if (answer.status >= 500 && answer.status !== 503 && !EXPECTED.has(String(answer.body.code))) g.host.log(`${method} ${url.pathname}: ${error?.stack ?? answer.body.message}`)
    return answer
  }
}
