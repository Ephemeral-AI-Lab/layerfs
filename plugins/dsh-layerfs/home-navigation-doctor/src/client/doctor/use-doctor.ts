/**
 * The Doctor page's controller: reads the tree, follows the host's doctor
 * runs and fixes (`/api/doctor/…`, events `doctor-run` and `doctor-fix`), and
 * starts what the page's buttons ask for. The state itself is `model.ts`.
 *
 * Opening the page reads records and what the host already holds; it runs no
 * doctor. Every doctor call and every fix is one attempt: a failure is shown,
 * and the next try is the person's.
 */
import { useCallback, useEffect, useRef, useState } from 'react'
import type { HostError, LayerfsApi } from '../api.ts'
import { useHostEvents } from '../storage/hooks.ts'
import type { DoctorCall, DoctorFixRun, DoctorReport, DoctorResults, DoctorRun, TreeAnswer } from '../../shared/doctor.ts'
import { thingKey, thingOf } from '../../shared/doctor.ts'
import {
  fixOffered, addRuns, afterFix, allMore, allPlan, buildWorld, callOf, closeAll, dropRuns, emptyState, fixEnd, fixStep, forget, onFix, onRun, openAll, planCalls, running, seed, setTree,
  type DoctorState, type RowFix, type Thing, type World,
} from './model.ts'

/** What the page says in a toast: a key of the locale with its values. */
export type Say =
  | { kind: 'allDone' }
  | { kind: 'could', name: string, reason: 'timeout' | 'busy' | 'lost' }
  | { kind: 'fullStopped' | 'dataRead' | 'writeTested' | 'ready' | 'deleted' | 'backedUp', name: string }
  | { kind: 'refused' | 'lost', name: string, fix: RowFix['button'] }
  | { kind: 'notStarted', message: string }

export interface Doctor {
  state: DoctorState
  /** The tree could not be read (a HostError; `Unsupported` on a runtime without it). */
  error: unknown
  loading: boolean
  reload(): void
  check(thing: Thing): void
  full(thing: Thing): void
  stop(thing: Thing): void
  checkAll(): void
  fix(thing: Thing, fix: RowFix): void
}

const newId = (): string => {
  const c = globalThis.crypto
  return typeof c?.randomUUID === 'function' ? c.randomUUID().replaceAll('-', '') : `${Date.now().toString(36)}${Math.random().toString(36).slice(2, 12)}`.padEnd(16, '0')
}
const ENDED = ['done', 'failed', 'stopped']

/** The last state each host's Doctor page showed (M14: reopening the page keeps earlier results; the host's held results are read again over it). */
const KEPT = new WeakMap<object, DoctorState>()
export const keptState = (api: object): DoctorState | undefined => KEPT.get(api)
export const keepState = (api: object, state: DoctorState): void => { if (state.world) KEPT.set(api, state) }

/**
 * Whether a run's end reads the tree again: a row's own check that answered.
 * Its record may have changed behind the page (a machine record restored by
 * hand): the row's platform mark and facts come from the tree, not the
 * report. Check all and a fix's checks read it once when they end.
 */
export const rereadsTree = (run: Pick<DoctorRun, 'state' | 'group'>): boolean => run.state === 'done' && !run.group

/** `refreshKey`: bumped by the page's Refresh (the top bar's); each new value reads the tree and the host's results again. */
export function useDoctor(api: LayerfsApi, say: (what: Say) => void, refreshKey = 0): Doctor {
  // a page opened again draws what it showed before (its tree, outcomes and notes) at once, then reads anew
  const [state, setState] = useState<DoctorState>(() => keptState(api) ?? emptyState())
  const [error, setError] = useState<unknown>(undefined)
  const [loading, setLoading] = useState(true)
  const ref = useRef(state)
  ref.current = state
  useEffect(() => { keepState(api, state) }, [api, state])
  const sayRef = useRef(say)
  sayRef.current = say
  const alive = useRef(true)
  /** Runs and fixes this page waits on: resolved when they end. */
  const runWaits = useRef(new Map<string, () => void>())
  const fixWaits = useRef(new Map<string, (end: DoctorFixRun | null) => void>())

  const nameOf = (kind: string, name: string | null | undefined): string => {
    const world = ref.current.world
    const thing = world?.by[kind === 'home' ? world.homeKey : thingKey(kind, name ?? '')]
    return thing ? (thing.kind === 'home' ? 'LayerFS' : thing.name) : (name ?? 'LayerFS')
  }

  /** Reads the tree; returns the world it makes. */
  const readTree = useCallback(async (): Promise<World | null> => {
    const tree = await api.call<TreeAnswer>('GET', '/api/doctor/tree')
    if (!alive.current) return null
    setState((s) => setTree(s, tree))
    return buildWorld(tree, ref.current.discovered)
  }, [api])

  /** What the host holds: the last outcomes, what runs now. Ends the waits on anything it no longer runs. */
  const resync = useCallback(async (): Promise<void> => {
    const held = await api.call<DoctorResults>('GET', '/api/doctor/results')
    if (!alive.current) return
    setState((s) => seed(s, held, Date.now()))
    const live = new Set(held.running.map((r) => r.id))
    for (const [id, done] of [...runWaits.current]) if (!live.has(id)) { runWaits.current.delete(id); done() }
    const fixing = new Set(held.fixes.filter((f) => f.state === 'running').map((f) => f.id))
    for (const [id, done] of [...fixWaits.current]) if (!fixing.has(id)) { fixWaits.current.delete(id); done(null) }
  }, [api])

  const inFlight = useRef(false)
  const queued = useRef(false)
  const load = useCallback(async (): Promise<void> => {
    if (inFlight.current) { queued.current = true; return }
    inFlight.current = true
    setLoading(true)
    try {
      do {
        queued.current = false
        await readTree()
        await resync()
      } while (alive.current && queued.current)
      if (alive.current) setError(undefined)
    } catch (e) {
      if (alive.current) setError(e)
    } finally {
      inFlight.current = false
      if (alive.current) setLoading(false)
    }
  }, [readTree, resync])

  useEffect(() => {
    alive.current = true
    void load()
    return () => { alive.current = false }
  }, [load])

  /** The tree again, quietly: after something changed it. A failure keeps the rows the page has. */
  const refresh = useCallback(() => { void readTree().catch(() => {}) }, [readTree])

  // the page's Refresh: the records may have changed behind it (a machine record rewritten, say), so the tree is read again
  const lastKey = useRef(refreshKey)
  useEffect(() => {
    if (lastKey.current === refreshKey) return
    lastKey.current = refreshKey
    void readTree().then(() => resync()).catch(() => {})
  }, [refreshKey, readTree, resync])

  useHostEvents(api, ['doctor-run'], (e) => {
    const run = e.data as DoctorRun | undefined
    if (!run || typeof run.id !== 'string') return
    setState((s) => onRun(s, run, Date.now()))
    if (!ENDED.includes(run.state)) return
    const done = runWaits.current.get(run.id)
    if (done) { runWaits.current.delete(run.id); done() }
    if (rereadsTree(run)) refresh()
    const name = nameOf(run.kind, run.name)
    if (run.state === 'failed') sayRef.current({ kind: 'could', name, reason: run.failure?.reason ?? 'lost' })
    else if (run.state === 'stopped') sayRef.current({ kind: 'fullStopped', name })
    else if (run.report) {
      if (run.full && run.report.tree.status !== 'not_checked') sayRef.current({ kind: run.kind === 'bucket' ? 'writeTested' : 'dataRead', name })
      // things of the answer that came back without a verdict (R3)
      for (const miss of could(run.report).slice(0, 3)) sayRef.current({ kind: 'could', name: miss.name, reason: miss.reason })
    }
  })

  useHostEvents(api, ['doctor-fix'], (e) => {
    const fix = e.data as DoctorFixRun | undefined
    if (!fix || typeof fix.id !== 'string') return
    if (fix.state === 'running') { setState((s) => onFix(s, fix)); return }
    const mine = fixWaits.current.get(fix.id)
    if (mine) { fixWaits.current.delete(fix.id); mine(fix); return }
    // another page's fix ended: its row is free again, and the records changed
    const key = thingKey(fix.kind, fix.name)
    setState((s) => (s.fixes[key]?.id === fix.id ? fixEnd(s, key) : s))
    refresh()
  })

  // reconnected: what ran meanwhile is in the host's results
  useHostEvents(api, ['connection'], (e) => {
    if ((e.data as { connected?: boolean } | undefined)?.connected === false) return
    void load()
  })
  // the home changed elsewhere (a machine, a Store, a project): the rows follow the records
  useHostEvents(api, ['storage', 'projects-changed'], () => { if (ref.current.world) refresh() }, { coalesceMs: 1500 })

  // Check all ended: say so, and read the records again
  const all = state.all?.id ?? null
  const lastAll = useRef<string | null>(null)
  useEffect(() => {
    if (lastAll.current && !all) { sayRef.current({ kind: 'allDone' }); refresh() }
    lastAll.current = all
  }, [all, refresh])

  /** Starts doctor calls; resolves when every one of them has ended (or was never started). */
  const start = useCallback(async (calls: readonly DoctorCall[], group: string | null): Promise<void> => {
    if (!calls.length) return
    const runs = calls.map((c) => ({ id: newId(), kind: c.kind, name: c.name ?? null, full: c.full === true, group }))
    // followed before the POST, so no event can outrun it
    const waits = runs.map((r) => new Promise<void>((resolve) => { runWaits.current.set(r.id, resolve) }))
    const drop = (ids: readonly string[]) => {
      setState((s) => dropRuns(s, ids))
      for (const id of ids) { const done = runWaits.current.get(id); if (done) { runWaits.current.delete(id); done() } }
    }
    setState((s) => addRuns(s, runs, Date.now()))
    try {
      const answer = await api.call<{ runs: DoctorRun[], already: string[] }>('POST', '/api/doctor/runs', {
        runs: runs.map((r) => ({ id: r.id, kind: r.kind, ...(r.name ? { name: r.name } : {}), ...(r.full ? { full: true } : {}) })),
        ...(group ? { group } : {}),
      })
      // already being checked by another call: its events are followed instead
      if (answer.already?.length) drop(answer.already)
    } catch (e) {
      drop(runs.map((r) => r.id))
      sayRef.current({ kind: 'notStarted', message: (e as HostError)?.message ?? String(e) })
    }
    await Promise.all(waits)
  }, [api])

  const check = useCallback((thing: Thing) => { void start([callOf(thing)], null) }, [start])

  const full = useCallback((thing: Thing) => {
    setState((s) => {
      if (!s.notes[thing.key]?.stopped) return s
      const { stopped: _stopped, ...rest } = s.notes[thing.key]!
      return { ...s, notes: { ...s.notes, [thing.key]: rest } }
    })
    void start([callOf(thing, true)], null)
  }, [start])

  const stop = useCallback((thing: Thing) => {
    // a Full check, or a call still waiting for its turn at the host (F12): the host ends it, nothing is written
    const run = running(ref.current, 'full', thing.key) ?? Object.values(ref.current.runs).find((r) => r.target === thing.key && !r.spawned)
    // a call that already ended answers NotFound: its end is on its way as an event
    if (run) void api.call('POST', `/api/doctor/runs/${run.id}/stop`).catch(() => {})
  }, [api])

  const checkAll = useCallback(() => {
    const plan = allPlan(ref.current)
    if (!plan) return
    const group = `all-${newId()}`
    const asked = planCalls(plan)
    void (async () => {
      setState((s) => openAll(s, group))
      try {
        await start(asked, group)
        // the rows have what the calls said once the page has drawn them
        await new Promise((resolve) => setTimeout(resolve, 0))
        // a machine the page thought failed that the home doctor found working: what is on it was left out, and is asked now, once
        const more = alive.current ? allMore(ref.current, asked) : []
        if (more.length) await start(more, group)
      } finally {
        if (alive.current) setState((s) => closeAll(s, group))
      }
    })()
  }, [start])

  const fix = useCallback((thing: Thing, row: RowFix) => {
    if (!row.fix || ref.current.fixes[thing.key]) return
    // click safety (UI pass 3 F6): the press runs only the fix its row offers now, for that very thing,
    // and not while the row is being checked or re-read (a stale handler never runs another row's fix)
    if (!fixOffered(ref.current, thing.key, row.line)) return
    const id = newId()
    const key = thing.key
    const name = thing.name
    void (async () => {
      setState((s) => fixStep(s, key, id, row.fix!.steps[0]!.call))
      const ended = new Promise<DoctorFixRun | null>((resolve) => { fixWaits.current.set(id, resolve) })
      try {
        await api.call('POST', '/api/doctor/fixes', { id, kind: thing.kind, name, fix: row.line })
      } catch (e) {
        fixWaits.current.delete(id)
        const refused = e as HostError
        setState((s) => fixEnd(s, key, { code: refused?.code ?? 'Unknown', message: refused?.message ?? String(e) }))
        sayRef.current({ kind: 'refused', name, fix: row.button })
        return
      }
      const end = await ended
      if (!alive.current) return
      if (end?.state === 'refused') {
        setState((s) => fixEnd(s, key, end.refusal ?? { code: 'Unknown', message: '' }))
        sayRef.current({ kind: 'refused', name, fix: row.button })
        return
      }
      if (end?.state !== 'done') {
        // its reply was lost: it may or may not have worked, and nothing here runs it again
        setState((s) => fixEnd(s, key))
        sayRef.current({ kind: 'lost', name, fix: row.button })
        refresh()
        return
      }
      sayRef.current({ kind: row.fix!.kind === 'delete' ? 'deleted' : row.fix!.kind === 'backup' ? 'backedUp' : 'ready', name })
      // what the doctors said before the fix no longer holds; the rows wait for the checks that follow
      setState((s) => fixStep(forget(s, key), key, id, null))
      try {
        const world = (await readTree().catch(() => null)) ?? ref.current.world
        if (world) {
          const group = `fix-${id}`
          for (const wave of afterFix(world, key, thing.parent)) {
            if (!alive.current) return
            await start(wave.map((x) => callOf(x)), group)
          }
        }
      } finally {
        // the checks after a fix may have changed what the records say (a machine started): the rows follow
        if (alive.current) { setState((s) => fixEnd(s, key)); refresh() }
      }
    })()
  }, [api, readTree, refresh, start])

  return { state, error, loading, reload: () => { void load() }, check, full, stop, checkAll, fix }
}

/** The things of a report that were tried and gave no verdict, with the reason. */
function could(report: DoctorReport): { name: string, reason: 'timeout' | 'busy' | 'lost' }[] {
  const out: { name: string, reason: 'timeout' | 'busy' | 'lost' }[] = []
  for (const n of report.not_checked) {
    const thing = thingOf(n)
    if (thing && !n.check && n.reason) out.push({ name: thing.name, reason: n.reason })
  }
  return out
}
