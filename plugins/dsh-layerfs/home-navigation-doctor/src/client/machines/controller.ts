/**
 * The Machines page's owner: the machines view (`GET /api/machines`), polled
 * while the page is shown; the machine startups (create and start progress,
 * kept across board and detail navigation); and the pinned LayerFS runtime.
 * The page reads these through the slot inject `hooks` (bound by the DSH
 * renderer as `useMachines`, `useStartups`, `useRuntime`) and acts through
 * the face's callbacks. One controller per registration; nothing here is a
 * module-level singleton.
 *
 * Power is manual: nothing here starts or stops a machine on its own, and no
 * read starts one. A Start, Stop, Create or Remove re-reads the view after
 * success and after refusal (a refusal can follow a real side effect).
 */
import { HostError, enc, q, type HostEvent, type LayerfsApi } from '../api.ts'
import {
  mergeStage, staleView, startupFailure,
  type Creation, type MachineStartup, type MachinesView, type NewMachine, type PruneAnswer, type ProjectSummary, type RuntimeStatus, type SandboxImagesView, type SandboxInfo,
} from './model.ts'
import { MetricsPoller, type MetricsSnapshot, type MetricsView } from './metrics.ts'

/** A snapshot source the DSH slot renderer binds to a `use<Name>` hook. */
export interface Source<T> {
  getSnapshot(): T
  subscribe(listener: () => void): () => void
}

class Cell<T> implements Source<T> {
  private readonly listeners = new Set<() => void>()
  constructor(private value: T) {}
  getSnapshot = (): T => this.value
  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener)
    return () => { this.listeners.delete(listener) }
  }
  set(next: T): void {
    if (Object.is(next, this.value)) return
    this.value = next
    for (const l of [...this.listeners]) l()
  }
}

/** A read's state: the last observation stays while a refresh runs or fails. */
export interface ReadState<T> {
  readonly status: 'idle' | 'loading' | 'ready' | 'error'
  readonly data: T | null
  readonly error: unknown
  readonly observedAt: number | null
}

const idle = <T>(): ReadState<T> => ({ status: 'idle', data: null, error: null, observedAt: null })

/** A machine action, as the page asks it. */
export type MachineAction =
  | { kind: 'create', input: NewMachine }
  | { kind: 'remove', name: string }
  | { kind: 'start', name: string, platform: string }
  /** A broken machine's fix `machine stop <m> && machine start <m>` (no Sandbox on it). */
  | { kind: 'restart', name: string, platform: string }
  | { kind: 'stop', name: string, force?: boolean }
  | { kind: 'update', name: string, changes: { image?: string, docker?: string, wsl?: string } }

/**
 * Machine and Sandbox metrics (metrics.ts `MetricsPoller`, layerfs-project#182):
 * a page watches the machines it shows; one `GET /api/metrics` per 15 s
 * covers them all, from the host's memory, only while the page is on screen.
 */
export interface MetricsFace extends Source<MetricsSnapshot> {
  watch: (name: string) => () => void
}

/** What the page receives through its slot `inject`. */
export interface MachinesFace {
  hooks: {
    machines: Source<ReadState<MachinesView>>
    startups: Source<Readonly<Record<string, MachineStartup>>>
    runtime: Source<ReadState<RuntimeStatus>>
    connected: Source<boolean>
    /** Background Sandbox creates by job id: running, and failed ones until dismissed (host-kept, D10). */
    creations: Source<Readonly<Record<string, Creation>>>
  }
  /** Machine and Sandbox metrics from the host's sampler, for the machines a page watches. */
  metrics: MetricsFace
  /** The page is shown: read now and poll every 15 s while the window is visible. Returns the release. */
  activate: () => () => void
  reload: () => void
  /** Probe the platforms again now (Check again after an install or a start), then show the answer. */
  recheck: () => Promise<void>
  reloadRuntime: () => void
  act: (action: MachineAction) => Promise<void>
  inspectMachine: (name: string) => Promise<{ state?: string, observedAt?: number }>
  /** What a Sandbox on this machine can start from (GET /api/machines/<name>/sandbox-images). */
  sandboxImages: (machine: string) => Promise<SandboxImagesView>
  /** Removes cached images from a machine (POST /api/machines/<name>/prune-images, Stage 1). */
  pruneImages: (machine: string, ids: string[]) => Promise<PruneAnswer>
  projects: () => Promise<ProjectSummary[]>
  readSandbox: (name: string) => Promise<SandboxInfo>
  /** Starts a background create (`background: true`); resolves on admission with its job, or null from a host that created it at once. */
  createSandbox: (project: string, input: NewSandbox) => Promise<Creation | null>
  /** Sends a failed create's input again (a new job). */
  retryCreation: (creation: Creation) => Promise<Creation | null>
  /** Dismisses a failed create (DELETE /api/sandbox-creates/<id>). */
  dismissCreation: (id: string) => Promise<void>
  closeSandbox: (name: string, force: boolean) => Promise<string[]>
  /** Deletes a Store and every project in it (the guided machine Remove). */
  removeStore: (name: string) => Promise<void>
  saveRuntime: (pin: { cli: string | null, daemon: string | null, bundle?: string | null, cacheHome?: string | null }) => Promise<RuntimeStatus>
  clearStartup: (name: string) => void
  /** Open another dsh-layerfs page (Storage) by its panel id. */
  openPanel: (id: string) => void
}

/** What New Sandbox sends: `image` is the picked row's id; `start` starts a stopped machine first (D5). */
export interface NewSandbox { name: string, machine: string, image?: string, start?: boolean }

const POLL_MS = 15_000
/** How long progress events settle before the machines view is read again (the data-loading rule). */
const SETTLE_MS = 500

const ended = (c: Creation): boolean => c.phase === 'done' || c.phase === 'failed'

export class MachinesController {
  readonly machines = new Cell<ReadState<MachinesView>>(idle())
  readonly startups = new Cell<Readonly<Record<string, MachineStartup>>>({})
  readonly runtime = new Cell<ReadState<RuntimeStatus>>(idle())
  readonly connected: Cell<boolean>
  readonly creations = new Cell<Readonly<Record<string, Creation>>>({})
  /** Finished creates, by id, with when they finished: dropped once a machines read that began after it lands. */
  private readonly finished = new Map<string, number>()
  private settleTimer: ReturnType<typeof setTimeout> | undefined
  private active = 0
  private timer: ReturnType<typeof setInterval> | undefined
  private reading: Promise<void> | null = null
  private again = false
  private readonly off: (() => void)[] = []
  private seq = 0
  /** The sampler's metrics for the machines a page watches; polls only while the window is visible and a reading page is the one shown. */
  readonly metrics: MetricsPoller

  /**
   * @param api - the dsh-layerfs host API.
   * @param shown - whether the Machines panel is the page shown now (polls run only then).
   */
  constructor(private readonly api: LayerfsApi, private readonly shown: () => boolean = () => true) {
    this.connected = new Cell(api.connected())
    const onScreen = () => (typeof document === 'undefined' || document.visibilityState === 'visible') && this.shown()
    this.metrics = new MetricsPoller((query) => this.api.call<MetricsView>('GET', `/api/metrics${query}`), onScreen)
    if (typeof document !== 'undefined') {
      const seen = () => { if (onScreen()) this.metrics.wake() }
      document.addEventListener('visibilitychange', seen)
      this.off.push(() => document.removeEventListener('visibilitychange', seen))
    }
    this.off.push(api.onEvent('connection', (e) => {
      const up = !!e.data?.connected
      this.connected.set(up)
      if (up && this.active) { this.reload(); this.reloadRuntime(); this.readCreations() }
    }))
    this.off.push(api.onEvent('sandbox-progress', (e) => this.creationEvent(e)))
    this.off.push(api.onEvent('runtime-changed', () => { this.reloadRuntime(); if (this.active) this.reload() }))
    this.off.push(api.onEvent('machine-progress', (e) => this.progress(e)))
  }

  /** The face handed to the page; callbacks are stable for the controller's life. */
  face(): MachinesFace {
    const api = this.api
    return {
      hooks: { machines: this.machines, startups: this.startups, runtime: this.runtime, connected: this.connected, creations: this.creations },
      metrics: { getSnapshot: this.metrics.getSnapshot, subscribe: this.metrics.subscribe, watch: (name) => this.metrics.watch(name) },
      activate: () => this.activate(),
      reload: () => this.reload(),
      recheck: async () => {
        try {
          const data = await api.call<MachinesView>('GET', '/api/machines?fresh=1')
          this.machines.set({ status: 'ready', data, error: null, observedAt: Date.now() })
        } catch (error) {
          const prev = this.machines.getSnapshot()
          this.machines.set({ status: prev.data ? 'ready' : 'error', data: staleView(prev.data, error), error, observedAt: prev.observedAt })
        }
      },
      reloadRuntime: () => this.reloadRuntime(),
      act: (action) => this.act(action),
      inspectMachine: (name) => api.call('GET', `/api/machines/${enc(name)}/inspect`),
      sandboxImages: (machine) => api.call('GET', `/api/machines/${enc(machine)}/sandbox-images`),
      pruneImages: async (machine, ids) => {
        try { return await api.call<PruneAnswer>('POST', `/api/machines/${enc(machine)}/prune-images`, { images: ids }) } finally { this.reload() }
      },
      projects: () => api.call('GET', '/api/projects'),
      readSandbox: (name) => api.call('GET', `/api/sandboxes/${enc(name)}`),
      createSandbox: (project, input) => this.create(project, input),
      retryCreation: async (creation) => {
        const { project, ...input } = creation.input
        const next = await this.create(project, input)
        this.drop(creation.id)
        return next
      },
      dismissCreation: async (id) => {
        try { await api.call('DELETE', `/api/sandbox-creates/${enc(id)}`) } catch (error) {
          // already gone on the host (expired, or created again): nothing left to dismiss
          if ((error as { status?: number }).status !== 404) throw error
        }
        this.drop(id)
      },
      closeSandbox: async (name, force) => {
        try {
          const done = await api.call<{ unmounted?: string[] }>('DELETE', `/api/sandboxes/${enc(name)}${q({ force: force ? '1' : undefined })}`)
          return done?.unmounted ?? []
        } finally { this.reload() }
      },
      removeStore: async (name) => {
        try { await api.call('DELETE', `/api/stores/${enc(name)}${q({ confirm: name })}`) } finally { this.reload() }
      },
      saveRuntime: async (pin) => {
        const status = await api.op<RuntimeStatus>('runtime.save', pin)
        this.runtime.set({ status: 'ready', data: status, error: null, observedAt: Date.now() })
        this.reload()
        return status
      },
      clearStartup: (name) => this.clearStartup(name),
      openPanel: (id) => api.openPanel(id),
    }
  }

  private activate(): () => void {
    this.active++
    if (this.active === 1) {
      this.reload()
      this.reloadRuntime()
      this.readCreations()
      this.timer = setInterval(() => {
        if ((typeof document === 'undefined' || document.visibilityState === 'visible') && this.shown()) this.reload()
      }, POLL_MS)
    }
    let released = false
    return () => {
      if (released) return
      released = true
      this.active--
      if (this.active === 0 && this.timer) { clearInterval(this.timer); this.timer = undefined }
    }
  }

  /** One read at a time; a reload asked during a read runs once after it. */
  reload(): void {
    if (this.reading) { this.again = true; return }
    const current = this.machines.getSnapshot()
    if (current.status === 'idle') this.machines.set({ ...current, status: 'loading' })
    const began = Date.now()
    this.reading = (async () => {
      try {
        const data = await this.api.call<MachinesView>('GET', '/api/machines')
        this.machines.set({ status: 'ready', data, error: null, observedAt: Date.now() })
        this.forgetFinished(began)
      } catch (error) {
        const prev = this.machines.getSnapshot()
        // a record that cannot be read shows as such, never as the Running it was (UI pass 3 F9)
        this.machines.set({ status: prev.data ? 'ready' : 'error', data: staleView(prev.data, error), error, observedAt: prev.observedAt })
      } finally {
        this.reading = null
        if (this.again) { this.again = false; this.reload() }
      }
    })()
  }

  reloadRuntime(): void {
    const prev = this.runtime.getSnapshot()
    if (prev.status === 'idle') this.runtime.set({ ...prev, status: 'loading' })
    this.api.op<RuntimeStatus>('runtime.status').then(
      (data) => this.runtime.set({ status: 'ready', data, error: null, observedAt: Date.now() }),
      (error) => { const p = this.runtime.getSnapshot(); this.runtime.set({ status: p.data ? 'ready' : 'error', data: p.data, error, observedAt: p.observedAt }) },
    )
  }

  private async act(action: MachineAction): Promise<void> {
    const api = this.api
    try {
      switch (action.kind) {
        case 'create':
          await this.startup({ kind: 'create', name: action.input.name, platform: action.input.platform }, () => api.call('POST', '/api/machines', action.input))
          break
        case 'start':
          await this.startup({ kind: 'start', name: action.name, platform: action.platform }, () => api.call('POST', `/api/machines/${enc(action.name)}/start`))
          break
        case 'restart':
          await this.startup({ kind: 'start', name: action.name, platform: action.platform }, () => api.call('POST', `/api/machines/${enc(action.name)}/restart`))
          break
        case 'stop':
          await api.call('POST', `/api/machines/${enc(action.name)}/stop`, action.force ? { force: true } : undefined)
          this.clearStartup(action.name)
          break
        case 'remove':
          await api.call('DELETE', `/api/machines/${enc(action.name)}`)
          this.clearStartup(action.name)
          break
        case 'update':
          await api.call('POST', `/api/machines/${enc(action.name)}/update`, action.changes)
          break
      }
    } finally {
      this.reload()
    }
  }

  private publish(s: MachineStartup): void {
    this.startups.set({ ...this.startups.getSnapshot(), [s.name]: s })
  }

  private update(name: string, id: string, change: (s: MachineStartup) => MachineStartup): void {
    const current = this.startups.getSnapshot()[name]
    if (current?.id === id) this.publish(change(current))
  }

  /** A later Stop/Remove clears the retained result, but never hides active work. */
  private clearStartup(name: string): void {
    const all = this.startups.getSnapshot()
    if (!all[name] || all[name].status === 'running') return
    const { [name]: _gone, ...rest } = all
    this.startups.set(rest)
  }

  private async startup(input: Pick<MachineStartup, 'kind' | 'name' | 'platform'>, request: () => Promise<unknown>): Promise<void> {
    if (this.startups.getSnapshot()[input.name]?.status === 'running') {
      throw new HostError('Busy', 'This machine already has a startup operation in progress.', 409)
    }
    const id = `startup-${Date.now()}-${++this.seq}`
    this.publish({ ...input, id, startedAt: Date.now(), status: 'running', stages: [] })
    try {
      await request()
      this.update(input.name, id, (s) => ({ ...s, status: 'success', endedAt: Date.now() }))
      // a started machine's facts changed: read its stats now (F14)
      this.metrics.refresh()
    } catch (error) {
      this.update(input.name, id, (s) => ({ ...s, status: startupFailure(error), endedAt: Date.now(), error }))
      throw error
    }
  }

  /**
   * A `machine-progress` host event: `{ name, kind?, progress }`, LayerFS 2.0's `{ phase, done, total }`
   * (or the host's own Docker Engine launch, `{ stage, state }`), with `elapsed_ms`
   * (or the progress fields at the top level). It updates the running startup of that machine.
   */
  private progress(event: HostEvent): void {
    const data = event.data ?? {}
    const name = typeof data.name === 'string' ? data.name : typeof data.machine === 'string' ? data.machine : null
    if (!name) return
    const current = this.startups.getSnapshot()[name]
    if (!current || current.status !== 'running') return
    // a restart's stop phases (check, stop) belong to its start's progress; any other kind's do not
    if (typeof data.kind === 'string' && data.kind !== current.kind && !(data.kind === 'stop' && current.kind === 'start')) return
    const progress = data.progress && typeof data.progress === 'object' ? data.progress : data
    const at = typeof data.at === 'number' ? data.at : Date.now()
    const stages = mergeStage(current.stages, progress, at)
    if (stages !== current.stages) this.publish({ ...current, stages })
  }

  // ---------- background Sandbox creates ----------

  /** POST the create in the background; the job's row shows at once, before its first event. */
  private async create(project: string, input: NewSandbox): Promise<Creation | null> {
    try {
      const answer = await this.api.call<{ creation?: Creation } | null>('POST', `/api/projects/${enc(project)}/sandboxes`, { ...input, background: true })
      const creation = answer && typeof answer === 'object' && answer.creation && typeof answer.creation.id === 'string' ? answer.creation : null
      // the job's first events can arrive before this answer: the admission (`queued`) never moves a row back
      if (creation && !this.creations.getSnapshot()[creation.id] && !this.finished.has(creation.id)) this.put(creation)
      return creation
    } finally { this.reload() }
  }

  /** GET /api/sandbox-creates: the host's running and failed creates replace what this page knew. */
  private readCreations(): void {
    this.api.call<{ creations?: Creation[] }>('GET', '/api/sandbox-creates').then((answer) => {
      const list = Array.isArray(answer?.creations) ? answer.creations : []
      const next: Record<string, Creation> = {}
      for (const c of list) if (c && typeof c.id === 'string' && c.phase !== 'done') next[c.id] = c
      this.creations.set(next)
    }, () => { /* an older host has no creates list: rows come from admissions and events */ })
  }

  /** A `sandbox-progress { creation }` event: the job's row moves in place; an end reads the machines again. */
  private creationEvent(event: HostEvent): void {
    const creation = (event.data as { creation?: Creation } | undefined)?.creation
    if (!creation || typeof creation.id !== 'string') return
    const known = this.creations.getSnapshot()[creation.id]
    // an event older than what is shown (the admission answer can overtake it) changes nothing
    if (known && known.updatedAt > creation.updatedAt) return
    this.put(creation)
    this.createStartup(creation)
    if (ended(creation)) this.settle(creation)
  }

  /**
   * "Start and create": the host starts the machine (phase `machine`, D5). The
   * machine shows Starting with its `machine-progress` stages, as after its own
   * Start, so its Start button is not offered while that start runs.
   */
  private createStartup(c: Creation): void {
    const name = c.input.machine
    const id = `create-${c.id}`
    const current = this.startups.getSnapshot()[name]
    if (c.phase === 'machine') {
      if (current?.status === 'running') return
      const view = this.machines.getSnapshot().data
      const platform = (view?.available ? view.machines.find((m) => m.name === name)?.platform : undefined) ?? name
      this.publish({ kind: 'start', name, platform, id, startedAt: c.updatedAt, status: 'running', stages: [] })
      return
    }
    if (current?.id !== id || current.status !== 'running') return
    // a failure straight from `machine` is the start's own
    if (c.phase === 'failed' && c.error) {
      const error = new HostError(c.error.code, c.error.message, 500, c.error.detail ?? null)
      this.update(name, id, (s) => ({ ...s, status: startupFailure(error), endedAt: c.updatedAt, error }))
    } else this.update(name, id, (s) => ({ ...s, status: 'success', endedAt: c.updatedAt }))
  }

  /** Shows a job; as on the host, a new job for the same project and name replaces that name's failed ones. */
  private put(creation: Creation): void {
    const next: Record<string, Creation> = {}
    for (const [id, c] of Object.entries(this.creations.getSnapshot())) {
      if (id !== creation.id && c.phase === 'failed' && c.input.project === creation.input.project && c.input.name === creation.input.name) continue
      next[id] = c
    }
    next[creation.id] = creation
    this.creations.set(next)
  }

  private drop(id: string): void {
    const all = this.creations.getSnapshot()
    if (!all[id]) return
    const { [id]: _gone, ...rest } = all
    this.creations.set(rest)
  }

  /** One machines read 500 ms after the last ended create; a done row leaves once that read shows its Sandbox. */
  private settle(creation: Creation): void {
    if (creation.phase === 'done') this.finished.set(creation.id, Date.now())
    if (this.settleTimer) return
    this.settleTimer = setTimeout(() => { this.settleTimer = undefined; this.reload() }, SETTLE_MS)
  }

  private forgetFinished(began: number): void {
    for (const [id, at] of this.finished) {
      if (at > began) continue
      this.finished.delete(id)
      this.drop(id)
    }
  }

  dispose(): void {
    for (const off of this.off) off()
    this.metrics.dispose()
    if (this.timer) clearInterval(this.timer)
    if (this.settleTimer) clearTimeout(this.settleTimer)
  }
}
