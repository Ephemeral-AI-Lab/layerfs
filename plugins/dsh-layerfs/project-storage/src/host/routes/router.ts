/**
 * Connect's host route table (Connect `host/server.js` `route()`), the refusal
 * code -> HTTP status map and the error body `{ code, transport?, message,
 * detail?, conversion? }`. No HTTP server, token or CORS: the `http`
 * operation hands `{ method, path, body }` here and answers `{ status, body }`.
 */
import { DoctorManager } from './doctor.ts'
import { Machines } from './machines.ts'
import { Projects } from './projects.ts'
import { Storage } from './storage.ts'
import { wire, type RouteHost } from './util.ts'

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
  // the SDK's: an operation's outcome was lost; the message names the clearing call
  Unresolved: 409,
  // the host's own guards (one job per Store on this host)
  ConnectBusy: 409,
  Capacity: 507,
  Unknown: 500,
  Conflict: 409,
  Exists: 409,
  NameTaken: 409,
  HeadMoved: 409,
  Dirty: 409,
  ReadOnly: 409,
  InUse: 409,
  Denied: 403,
  Unavailable: 502,
  Unsupported: 501,
}

/** An error the routes throw: answered as { code, message } with `status`. */
export const httpError = (status: number, code: string, message: string) => Object.assign(new Error(message), { status, code })

/**
 * A layout 2 home's conversion refusal (#536 F2): `InUse` whose
 * `detail.related` names what is live with `why` (what ends it).
 */
export const isConversion = (code: string, detail: any) => code === 'InUse' && Array.isArray(detail?.related) && detail.related.some((r: any) => typeof r?.why === 'string')

/**
 * The status and body an error answers: { code, message, detail? } with the
 * SDK's refusal as it came (its text names any status or clearing call;
 * `detail` holds entity, phase, op and related), and `conversion: true` for
 * a layout 2 home's conversion refusal.
 */
export function errorAnswer(error: any): { status: number, body: Record<string, unknown> } {
  const code = typeof error?.code === 'string' ? error.code : 'failed'
  const status = Number.isInteger(error?.status) ? error.status : (STATUS[code] ?? 500)
  const detail = error?.detail && typeof error.detail === 'object' ? error.detail : null
  return { status, body: wire({ code, ...(error?.transport ? { transport: error.transport } : {}), message: error?.message ?? String(error), ...(detail ? { detail } : {}), ...(isConversion(code, detail) ? { conversion: true } : {}) }) }
}

/** The route groups behind the table. */
export interface Groups {
  readonly host: RouteHost
  readonly machines: Machines
  readonly storage: Storage
  readonly projects: Projects
  readonly doctor: DoctorManager
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
  const machines = new Machines(host, (store, machine) => storage.place(store, machine))
  const doctor = new DoctorManager(host)
  return { host, machines, storage, projects, doctor, runtime }
}

/** method + path segments -> the answer, or undefined when no route matches. */
export async function route(g: Groups, method: string, parts: string[], query: URLSearchParams, body: any): Promise<unknown> {
  const { machines, storage, projects, doctor } = g
  const [api, group, id, sub, ...rest] = parts
  if (api === 'api' && group === 'demo-presets') return g.presets?.route(method, parts.slice(2), body, query)
  if (api !== 'api' || rest.length) return undefined
  if (group === 'doctor-jobs' && !sub) {
    if (!id && method === 'POST') return doctor.launch(body ?? {})
    if (!id && method === 'GET') return doctor.list()
    if (id && method === 'GET') return doctor.get(id)
  }
  // Stores, buckets and the moves, backups and restores this host started
  if (group === 'storage' && !id && method === 'GET') return storage.view()
  // lists the buckets' files again (backup.list, one call per bucket): { bucket? }, empty for all
  if (group === 'storage' && id === 'check' && !sub && method === 'POST') return storage.check(body ?? {})
  if (group === 'stores') {
    if (!id && method === 'POST') return storage.createStore(body ?? {})
    // a PostgreSQL database: { name, url, password (a reference), synchronousCommit? }
    if (id === 'postgresql' && !sub && method === 'POST') return storage.connectPostgresql(body ?? {})
    if (id && sub === 'stats' && method === 'GET') return projects.storeStats(id)
    if (id && sub === 'observations' && method === 'GET') return projects.storeObservations(id)
    if (id && !sub && method === 'DELETE') return storage.removeStore(id, { confirm: query.get('confirm') })
    if (id && sub === 'disconnect' && method === 'POST') return storage.disconnectStore(id)
    // a PostgreSQL Store on a machine: { machine, url?, password? | passwordValue? }, checked from the machine
    if (id && sub === 'attach' && method === 'POST') return storage.attachStore(id, body ?? {})
    // { machine }
    if (id && sub === 'detach' && method === 'POST') return storage.detachStore(id, body ?? {})
    // a sqlite Store on native to a machine: { machine }
    if (id && sub === 'push' && method === 'POST') return storage.push(id, body?.machine)
    // a sqlite Store on a machine back to native
    if (id && sub === 'pull' && method === 'POST') return storage.pull(id)
    // between two machines, pull then push in one job: { machine }
    if (id && sub === 'move' && method === 'POST') return storage.move(id, body?.machine)
    // { bucket, file? }
    if (id && sub === 'backup' && method === 'POST') return storage.backup(id, body ?? {})
    return undefined
  }
  if (group === 'buckets') {
    // { name, url, key (a reference), region? }
    if (!id && method === 'POST') return storage.connectBucket(body ?? {})
    if (id && !sub && method === 'DELETE') return storage.disconnectBucket(id)
    if (id && sub === 'files' && method === 'GET') return storage.listBucket(id)
    if (id && sub === 'keys' && method === 'POST') return storage.replaceBucketKeys(id, body)
    // { store (the new Store's name), file? }: installs on native
    if (id && sub === 'restore' && method === 'POST') return storage.restore({ ...(body ?? {}), bucket: id })
    return undefined
  }
  if (group === 'storage-jobs' && id && !sub && method === 'GET') return storage.job(id)
  if (group === 'storage-jobs' && id && sub === 'inspect' && method === 'POST') return storage.inspect(id)
  if (group === 'storage-jobs' && id && sub === 'retry' && method === 'POST') return storage.retry(id)
  // layerfs doctor: report-only, starts nothing (#537 §8.7)
  if (group === 'doctor' && !id && method === 'GET') return storage.doctor()
  // Compute's machines. Power is manual (#535 D11): start and stop are their own calls, and no read starts one
  if (group === 'platforms' && !id && method === 'GET') return machines.platforms()
  if (group === 'machines') {
    if (!id && !sub && method === 'GET') return machines.view()
    if (id && sub === 'inspect' && method === 'GET') return machines.inspect(id)
    if (id && sub === 'images' && method === 'GET') return machines.images(id)
    if (id && sub === 'start' && method === 'POST') return machines.start(id)
    if (id && sub === 'stop' && method === 'POST') return machines.stop(id, { force: body?.force === true })
    if (!id && !sub && method === 'POST') return machines.create(body)
    if (id && !sub && method === 'DELETE') return machines.remove(id)
    if (id && sub === 'update' && method === 'POST') return machines.update(id, body)
    // puts a sqlite Store here (push, pull, or pull then push): { store }
    if (id && sub === 'stores' && method === 'POST') return machines.moveStore(body?.store, id)
  }
  if (group === 'status' && !id && method === 'GET') return projects.status(await g.runtime())
  // `id` is a project key, `<store>/<project>`, sent as one encoded segment
  if (group === 'projects') {
    if (!id) {
      if (method === 'GET') return projects.list()
      if (method === 'POST') return projects.create(body)
      return undefined
    }
    const projectId = query.get('projectId')
    if (projectId) await g.host.assertProjectIdentity?.(id, projectId)
    if (!sub && method === 'GET') return projects.state(id)
    if (!sub && method === 'DELETE') return projects.remove(id)
    // a new Sandbox: { name, image?, machine? (PostgreSQL projects only) }
    if (sub === 'sandboxes' && method === 'POST') return projects.createSandbox(id, body ?? {})
    // closes every Sandbox of the project: { force? }
    if (sub === 'close-sandboxes' && method === 'POST') return projects.closeSandboxes(id, { force: body?.force === true })
    if (method !== 'GET') return undefined
    const branch = query.get('branch') ?? 'main'
    if (sub === 'history') return projects.history(id, query)
    // this project's own storage from its saves' counters (see project-storage.ts): ?refresh=1
    if (sub === 'storage') return projects.projectStorage(id, query)
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
  // sandbox.close(name, { force }): ?force=1
  if (group === 'sandboxes' && id && !sub && method === 'DELETE') return projects.closeSandbox(id, { force: query.get('force') === '1' })
  // one Sandbox with its Workspaces (sandbox.get)
  if (group === 'sandboxes' && id && !sub && method === 'GET') return projects.sandbox(id)
  // a Workspace in it: { branch, fork?, name? } (only side Branches are mounted, D14)
  if (group === 'sandboxes' && id && sub === 'workspaces' && method === 'POST') return projects.mount(id, body ?? {})
  if (group === 'workspaces' && id && sub === 'status' && method === 'GET') return projects.lfs.workspace.status(id)
  // workspace.unmount: ?discard=1
  if (group === 'workspaces' && id && !sub && method === 'DELETE') return projects.unmount(id, { discard: query.get('discard') === '1' })
  if (group === 'project-inits' && !id && method === 'GET') return projects.initJobs()
  if (group === 'project-inits' && id && !sub && method === 'GET') return projects.init(id)
  return undefined
}

/** One request: `{ method, path, body }` -> `{ status, body }` (JSON, unsafe integers tagged `i64-v1`). */
export async function handle(g: Groups, request: { method?: string, path?: string, body?: unknown }): Promise<{ status: number, body: unknown }> {
  const method = String(request?.method ?? 'GET').toUpperCase()
  let url: URL
  try {
    url = new URL(String(request?.path ?? '/'), 'http://127.0.0.1')
  } catch {
    return { status: 400, body: { code: 'invalid-input', message: 'bad request URL' } }
  }
  let parts: string[]
  try {
    parts = url.pathname.split('/').filter(Boolean).map(decodeURIComponent)
  } catch {
    return { status: 400, body: { code: 'invalid-input', message: 'bad path encoding' } }
  }
  try {
    let body: unknown = {}
    if (method === 'POST' && request.body !== undefined && request.body !== null) {
      if (typeof request.body === 'string') {
        try { body = JSON.parse(request.body) } catch { throw httpError(400, 'invalid-input', 'Invalid JSON request body.') }
      } else body = request.body
    }
    if (method !== 'GET' && parts[1] !== 'demo-presets' && g.presets?.exclusive()) {
      throw httpError(409, 'ConnectBusy', 'A preset build or reset is changing this LayerFS home. Changes are available when it finishes; you can keep browsing.')
    }
    // Connect recorded the native daemon at boot; here once, before the first call that needs it
    await g.host.ensureNative()
    const answer = await route(g, method, parts, url.searchParams, body)
    if (answer === undefined) return { status: 404, body: { code: 'not-found', message: `no route ${method} ${url.pathname}` } }
    return { status: 200, body: wire(answer) ?? null }
  } catch (error: any) {
    const answer = errorAnswer(error)
    if (answer.status >= 500 && answer.status !== 503) g.host.log(`${method} ${url.pathname}: ${error?.stack ?? answer.body.message}`)
    return answer
  }
}
