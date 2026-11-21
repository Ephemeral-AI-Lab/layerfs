import { useEffect, useRef } from 'react'
import type { LayerfsApi } from '../api.ts'
import { useQuery } from '../files/use-query.ts'
import type { MachinesView } from '../machines/model.ts'
import { useHostEvents } from '../storage/hooks.ts'
import { readWithDeadline, steadyRead } from './read-deadline.ts'

/**
 * The sidebar's reads (catalog, Storage, machines) are on every page: host events
 * carry their changes, so the poll only catches what no event reports (a VM that died).
 */
export const SIDEBAR_POLL_MS = 60_000
/** Host events arrive many a second during a team run: one reread per burst. */
export const EVENT_COALESCE_MS = 2_000

export function useProjectMachines(api: LayerfsApi) {
  const last = useRef<{ value: MachinesView, failures: number } | null>(null)
  const read = useQuery(() => steadyRead(last, () => readWithDeadline(api.call<MachinesView>('GET', '/api/machines'))), [api], { cache: 'GET /api/machines' })
  useHostEvents(api, ['machine-progress', 'layerfs-availability', 'storage', 'runtime-changed', 'connection'], read.reload, { coalesceMs: EVENT_COALESCE_MS })
  useEffect(() => {
    const timer = setInterval(() => { if (document.visibilityState === 'visible') read.reload() }, SIDEBAR_POLL_MS)
    return () => clearInterval(timer)
  }, [read.reload])
  return read
}

export interface ProjectSession {
  sessionId: string, title: string | null, local: boolean, updatedAt: number
  /** `sessions.list`: a binding's state (`bound`, `closed`, `failed`), else `pending`, `local` or `stored` (only in the Store). */
  state?: string
  /** Local bindings only: `interrupted` when its Store became unavailable or its context was lost. */
  executionState?: 'inactive' | 'activating' | 'active' | 'interrupted'
  error?: string | null
}
interface SessionList { sessions: ProjectSession[], discoveryError?: { code: string, message: string } | null }

/** The local session list (this DSH's own logs, no Store walk). */
const LOCAL_POLL_MS = 15_000
/** The portable archive: a Store walk, so slow (F22). */
const ARCHIVE_POLL_MS = 60_000

/** Local logs remain visible while the portable archive loads or fails. */
export function useProjectSessions(api: LayerfsApi, project: string, enabled = true, projectId?: string) {
  const load = (localOnly: boolean) => enabled
    ? readWithDeadline(api.op<SessionList>('sessions.list', { project, projectId, localOnly }))
    : Promise.resolve<SessionList>({ sessions: [] })
  const key = `${project}:${projectId ?? ''}:${enabled}`
  const local = useQuery(() => load(true), [api, project, projectId, enabled], { cache: `sessions-local:${key}` })
  const archive = useQuery(() => load(false), [api, project, projectId, enabled], { cache: `sessions-archive:${key}` })
  const reload = () => { local.reload(); archive.reload() }
  // Session, storage and machine events come many a second during a run: they reread the
  // local list only. The portable archive (a Store walk: a Branch list plus one manifest
  // read per stored session) is reread on reconnect and by its own slow poll; under a team
  // run a 15 s archive poll per open project stalled host reads (Quarry v2 FIX-LOG F22).
  useHostEvents(api, ['layerfs-session', 'layerfs-workspaces-changed', 'layerfs-availability', 'storage', 'machine-progress'], () => { if (enabled) local.reload() }, { coalesceMs: 1_000 })
  useHostEvents(api, ['connection'], () => { if (enabled) reload() })
  useEffect(() => {
    if (!enabled) return
    const visible = () => document.visibilityState === 'visible'
    const fast = setInterval(() => { if (visible()) local.reload() }, LOCAL_POLL_MS)
    const slow = setInterval(() => { if (visible()) archive.reload() }, ARCHIVE_POLL_MS)
    return () => { clearInterval(fast); clearInterval(slow) }
  }, [enabled, local.reload, archive.reload])
  const rows = new Map((archive.data?.sessions ?? []).map((s) => [s.sessionId, s]))
  for (const s of local.data?.sessions ?? []) rows.set(s.sessionId, { ...rows.get(s.sessionId), ...s, title: rows.get(s.sessionId)?.title ?? s.title })
  return {
    sessions: [...rows.values()].sort((a, b) => b.updatedAt - a.updatedAt),
    loading: local.loading || archive.loading,
    error: local.error ?? archive.error ?? (archive.data?.discoveryError ? new Error(archive.data.discoveryError.message) : undefined),
    reload,
  }
}
