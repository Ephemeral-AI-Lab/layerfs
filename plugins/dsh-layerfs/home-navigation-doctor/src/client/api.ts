/**
 * The browser's one seam to the dsh-layerfs host: `call(method, path, body)`
 * keeps LayerFS Connect's host API shape (served by the `http` operation),
 * `op(name, args)` runs a named operation, and `onEvent` observes host events
 * from the `layerfs/events` stream. Pages receive one `LayerfsApi` through
 * their slot `inject`; nothing here is a module-level singleton.
 */

/** A refused call: LayerFS's code, its message, detail and `fix` line, and the HTTP-like status. */
export class HostError extends Error {
  constructor(
    readonly code: string,
    message: string,
    readonly status: number,
    readonly detail: any = null,
    readonly conversion = false,
    readonly transport?: string,
    /** LayerFS's one runnable line that clears the refusal, when it gave one. */
    readonly fix?: string,
  ) {
    super(message)
    this.name = 'HostError'
  }
}

/** One host event. */
export interface HostEvent {
  readonly type: string
  readonly data?: any
}

/** What every page receives. */
export interface LayerfsApi {
  /** Connect's host API: resolves the 2xx body, throws {@link HostError} otherwise. */
  call<T = any>(method: string, path: string, body?: unknown): Promise<T>
  /** A named host operation (e.g. `runtime.status`, `session.state`). */
  op<T = any>(name: string, args?: unknown): Promise<T>
  /** Observe host events; `type` `*` receives all. Returns an unsubscribe. */
  onEvent(type: string, listener: (event: HostEvent) => void): () => void
  /** Whether the host event stream is connected. */
  connected(): boolean
  /** Select a DSH panel (sidebar main page) by id. */
  openPanel(id: string): void
  /** Open (or start) a session in a DSH workspace. */
  openWorkspace(workspaceId: string): Promise<void>
  /** Show a DSH session. */
  openSession(sessionId: string): void
  /** The workspace the client would open a new session in (current, then recent). */
  selectedWorkspaceId(): string | undefined
  /** Currently displayed conversation, including project-owned sessions. */
  selectedSessionId(): string | undefined
  /** Public DSH directory dialog; null when cancelled. */
  pickDirectory(): Promise<string | null>
  /** Open a project-owned session using the selected local workspace as scratch. */
  /** `machine`: where a PostgreSQL project's Sandbox runs (the person's pick; else the host's default, placement.ts). */
  openProjectSession(project: string, workspaceId?: string, projectId?: string, machine?: string): Promise<{ sessionId: string, workspaceId: string }>
}

/** Navigation the host may ask for in an operation result (`navigate`). */
export interface Navigator {
  createSession(workspaceId: string): Promise<string>
  currentSessionId?(): string | undefined
  openPanel(id: string): void
  openWorkspace(workspaceId: string): Promise<void>
  openSession(sessionId: string): void
}

/** Encode one path segment. */
export const enc = encodeURIComponent

/** A query string from defined params. */
export function q(params: Record<string, string | number | boolean | undefined | null>): string {
  const entries = Object.entries(params).filter(([, v]) => v !== undefined && v !== null).map(([k, v]) => [k, String(v)])
  const s = new URLSearchParams(entries).toString()
  return s ? `?${s}` : ''
}

type RemoteResult = { ok: true, value: any } | { ok: false, error: { code: string, message: string, details?: any } }

/**
 * How long a GET waits for its answer before it fails (and its page can offer
 * Retry). Longer than the host's own read budget (2 minutes) plus a queue, so it
 * only fires when an answer is lost, never on a slow but live read.
 */
export const GET_TIMEOUT_MS = 180_000
/** How long a GET whose connection dropped waits for the host to come back before it is sent again. */
export const RECONNECT_WAIT_MS = 30_000

/** A call whose answer cannot arrive: its connection dropped, or it outlived its budget. */
class LostCall extends Error {
  constructor(readonly why: 'dropped' | 'timeout') {
    super(why === 'dropped' ? 'The connection to the dsh-layerfs host dropped before it answered' : 'The dsh-layerfs host did not answer in time')
  }
}

/**
 * Build the API over the `remote.layerfs` namespace. The namespace is resolved
 * per call (it remounts when the connection resets), so the API and the pages
 * holding it outlive remounts.
 * @param remote - the mounted namespace, or undefined while it is (re)mounting.
 * @param nav - DSH navigation (panels, workspaces, sessions).
 * @param workspace - the `uiWorkspace` service, or undefined while it mounts.
 */
export function createApi(remote: () => any, nav: Navigator, workspace: () => any): LayerfsApi & { dispose(): void } {
  const listeners = new Map<string, Set<(e: HostEvent) => void>>()
  /** Calls waiting on an answer: each fails when the event stream drops (its connection is gone). */
  const inFlight = new Set<(e: LostCall) => void>()
  let live = false
  let disposed = false
  let handle: any

  const dispatch = (event: HostEvent) => {
    for (const key of [event.type, '*']) for (const l of listeners.get(key) ?? []) {
      try { l(event) } catch (error) { console.error('[dsh-layerfs] event listener failed', error) }
    }
  }
  const subscribe = (type: string, listener: (event: HostEvent) => void): (() => void) => {
    if (!listeners.has(type)) listeners.set(type, new Set())
    listeners.get(type)!.add(listener)
    return () => { listeners.get(type)?.delete(listener) }
  }

  const fail = (r: Extract<RemoteResult, { ok: false }>): never => {
    const d = r.error.details ?? {}
    throw new HostError(d.code ?? r.error.code, r.error.message, 500, d.detail ?? null, false, undefined, typeof d.fix === 'string' && d.fix ? d.fix : undefined)
  }

  const namespace = async (): Promise<any> => {
    for (let waited = 0; ; waited += 100) {
      const ns = remote()
      if (ns) return ns
      if (disposed || waited > 15000) throw new HostError('no-host', 'The dsh-layerfs host is not connected', 0)
      await new Promise((r) => setTimeout(r, 100))
    }
  }

  /** One remote call that fails with {@link LostCall} when the connection drops under it or `timeout` passes. */
  async function send(name: string, args: unknown, timeout?: number): Promise<RemoteResult> {
    const ns = await namespace()
    let drop: ((e: LostCall) => void) | undefined
    let timer: ReturnType<typeof setTimeout> | undefined
    const lost = new Promise<never>((_resolve, reject) => {
      drop = reject
      inFlight.add(reject)
      if (timeout) timer = setTimeout(() => reject(new LostCall('timeout')), timeout)
    })
    try {
      return await Promise.race([ns.call({ op: name, args: args ?? {} }) as Promise<RemoteResult>, lost])
    } finally {
      inFlight.delete(drop!)
      if (timer) clearTimeout(timer)
    }
  }

  /** Resolves once the event stream is connected again, or after `ms`. */
  const reconnected = (ms: number) => new Promise<void>((resolve) => {
    if (live || disposed) return resolve()
    const done = () => { clearTimeout(timer); off(); resolve() }
    const timer = setTimeout(done, ms)
    const off = subscribe('connection', (e) => { if (e.data?.connected) done() })
  })

  async function op<T>(name: string, args?: unknown, timeout?: number): Promise<T> {
    const r: RemoteResult = await send(name, args, timeout)
    if (!r.ok) fail(r)
    const value = (r as { value: any }).value
    // A host answer may ask the client to navigate (e.g. open the new session).
    const go = value && typeof value === 'object' ? value.navigate : undefined
    if (go?.sessionId) nav.openSession(String(go.sessionId))
    else if (go?.workspaceId) await nav.openWorkspace(String(go.workspaceId))
    return value as T
  }

  async function call<T>(method: string, path: string, body?: unknown): Promise<T> {
    let reply: { status: number, body: any }
    const get = method.toUpperCase() === 'GET'
    const args = { method, path, ...(body === undefined ? {} : { body }) }
    try {
      try {
        reply = await op('http', args, get ? GET_TIMEOUT_MS : undefined)
      } catch (error) {
        // A read whose connection dropped (the host restarted) is sent once more when it is back.
        // A write never is: it may already have happened.
        if (!(get && error instanceof LostCall && error.why === 'dropped')) throw error
        await reconnected(RECONNECT_WAIT_MS)
        reply = await op('http', args, GET_TIMEOUT_MS)
      }
    } catch (error) {
      if (error instanceof HostError) throw error
      if (error instanceof LostCall && error.why === 'timeout') throw new HostError('timeout', error.message, 0, null, false, 'transport-unknown')
      // The request was sent and its reply lost: a write may or may not have happened.
      throw new HostError('no-host', error instanceof Error ? error.message : String(error), 0, null, false, 'transport-unknown')
    }
    const value = reply?.body ?? {}
    if (!reply || reply.status < 200 || reply.status >= 300) {
      throw new HostError(value.code ?? 'failed', value.message ?? `${method} ${path}: ${reply?.status}`, reply?.status ?? 0, value.detail ?? null, value.conversion === true, value.transport, typeof value.fix === 'string' && value.fix ? value.fix : undefined)
    }
    return value as T
  }

  // One logical event stream, reopened after a drop until disposal.
  const connect = async () => {
    let delay = 500
    while (!disposed) {
      try {
        handle = (await namespace()).events()
        for await (const item of handle) {
          if (disposed) break
          const event = (item && typeof item === 'object' && 'value' in item && !('type' in item)) ? item.value : item
          if (event?.type === 'ready') { live = true; delay = 500; dispatch({ type: 'connection', data: { connected: true } }); continue }
          if (event?.type) dispatch(event)
        }
      } catch { /* reconnect below */ }
      if (live) { live = false; dispatch({ type: 'connection', data: { connected: false } }) }
      // Answers to calls sent on the dropped connection cannot arrive: fail them now rather than leave pages waiting.
      for (const fail of [...inFlight]) fail(new LostCall('dropped'))
      if (disposed) break
      await new Promise((r) => setTimeout(r, delay))
      delay = Math.min(delay * 2, 8000)
    }
  }
  void connect()

  // The workspace a new session opens in: the one holding the current session,
  // else the first listed (ui-workspace's "recent" order follows).
  const selectedWorkspaceId = (): string | undefined => {
    const ws = workspace()
    const snap = ws?.workspaces?.list?.getSnapshot?.()
    const items = snap?.items ?? []
    const current = nav.currentSessionId?.()
    return items.find((item: any) => item.sessionIds?.includes?.(current))?.workspaceId ?? items[0]?.workspaceId
  }

  return {
    call,
    // A named operation's lost answer reads like any lost send: no host, outcome unknown.
    op: <T>(name: string, args?: unknown) => op<T>(name, args).catch((error: unknown) => {
      if (error instanceof LostCall) throw new HostError('no-host', error.message, 0, null, false, 'transport-unknown')
      throw error
    }),
    openPanel: (id) => nav.openPanel(id),
    openWorkspace: (id) => nav.openWorkspace(id),
    openSession: (id) => nav.openSession(id),
    selectedWorkspaceId,
    pickDirectory: async () => {
      const picker = workspace()
      if (!picker?.pickDirectory) throw new HostError('no-picker', 'The local folder picker is unavailable', 0)
      return picker.pickDirectory()
    },
    selectedSessionId: () => nav.currentSessionId?.(),
    async openProjectSession(project: string, workspaceId?: string, projectId?: string, machine?: string) {
      const target = workspaceId ?? selectedWorkspaceId()
        ?? (await op<{ workspaceId: string }>('sessions.prepareScratch', { navigate: false })).workspaceId
      if (!target) throw new HostError('no-workspace', 'Select a local folder to use as scratch for the project session', 0)
      // A workspace blank may already belong to another project. Request a fresh identity before binding it.
      const sessionId = await nav.createSession(target)
      await op('sessions.choose', { sessionId, project, projectId, ...(machine ? { machine } : {}) })
      nav.openSession(sessionId)
      return { sessionId, workspaceId: target }
    },
    connected: () => live,
    onEvent: subscribe,
    dispose() {
      disposed = true
      try { handle?.dispose?.() } catch { /* already closed */ }
    },
  }
}
