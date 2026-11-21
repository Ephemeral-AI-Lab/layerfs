/**
 * The sidebar's LayerFS project list (docs/specs/layerfs-workspaces.md §4):
 * projects with sessions on this DSH, newest first. A row has the LayerFS mark
 * and the name, no status light (a few words when it can't run), folds its
 * sessions out, and on hover shows ⚙ (the project dialog) and + (new session;
 * Start while its machine is stopped). Also the project data the dialog's
 * picker reads (`useProjectData`, `useRecent`).
 */
import { useEffect, useRef, useState, useSyncExternalStore, type ReactNode } from 'react'
import clsx from 'clsx'
import { Button, IconArchiveOutlineRegular, IconPlusOutlineRegular, IconSettingsOutlineRegular, IconTriangleRightFillRegular } from '@deepseek-ai/dsh-client-ui-primitives'
import type { LayerfsApi } from '../api.ts'
import { LayerfsGlyph } from '../overview/glyph.tsx'
import { listProjects, type ProjectRow } from '../picker/NewProjectForm.tsx'
import { useQuery } from '../files/use-query.ts'
import { usePressGuard } from '../ui/index.tsx'
import { useHostEvents } from '../storage/hooks.ts'
import type { StorageView, StoreRow } from '../storage/types.ts'
import { projectAvailability, type ProjectAvailability } from './availability.ts'
import { EVENT_COALESCE_MS, SIDEBAR_POLL_MS, useProjectMachines, useProjectSessions, type ProjectSession } from './hooks.ts'
import type { ListT } from './list-locale.ts'
import css from './list.module.css'
import { openMachineHome } from './navigation.ts'
import { projectDialog } from './dialog.ts'
import { ReadDeadlineError, readWithDeadline, steadyRead } from './read-deadline.ts'
import { sidebarStatus } from './readiness.ts'
import { sidebarHidden, sidebarProjects, type RecentProject } from './recent.ts'
import { archivable, visibleProjectSessions } from './session-visibility.ts'
import { primaryMachine, runsOn } from './setup/model.ts'

interface SessionSummary { title?: string, blank?: boolean }
/** DSH's session summaries (`sessions.list`), for live titles and unused drafts. */
export interface SessionSource { getSnapshot(): { byId: Record<string, SessionSummary> }, subscribe(fn: () => void): () => void }

const EMPTY = { byId: {} as Record<string, SessionSummary> }
const NO_SOURCE: SessionSource = { getSnapshot: () => EMPTY, subscribe: () => () => {} }

const enc = encodeURIComponent
const nameOf = (p: ProjectRow) => p.name ?? p.key.split('/')[1] ?? p.key
const storeOf = (p: ProjectRow) => p.store ?? p.key.split('/')[0] ?? ''
const idOf = (p: ProjectRow) => p.projectId ?? p.key
const errorMessage = (error: unknown, t: ListT) => error instanceof ReadDeadlineError ? t('timedOut') : String((error as Error).message ?? error)

/**
 * The row's one leading slot, as in DSH's own Workspaces list: the LayerFS mark at
 * rest, the fold arrow while the row is hovered or focused (right closed, down open).
 */
function LeadingSlot({ open }: { open: boolean }): ReactNode {
  return <span className={css.slot} aria-hidden="true">
    <span className={css.mark}><LayerfsGlyph size={16} /></span>
    <span className={css.fold}><IconTriangleRightFillRegular size={12} className={clsx(css.arrow, open && css.arrowOpen)} /></span>
  </span>
}

function useSummaries(sessionSource?: SessionSource) {
  const source = sessionSource ?? NO_SOURCE
  return useSyncExternalStore(source.subscribe.bind(source), source.getSnapshot.bind(source))
}

/**
 * A read kept on screen while a refresh runs, reloaded on `events` (coalesced) and by
 * the slow poll while visible. The sidebar shows it on every page, so each poll costs
 * every open tab a catalog, Store and machine read (Quarry v2 FIX-LOG F22).
 */
function useSteady<T>(api: LayerfsApi, cache: string, load: () => Promise<T>, events: readonly string[]) {
  const last = useRef<{ value: T, failures: number } | null>(null)
  const read = useQuery(() => steadyRead(last, () => readWithDeadline(load())), [api], { cache })
  useHostEvents(api, events, read.reload, { coalesceMs: EVENT_COALESCE_MS })
  useEffect(() => {
    const timer = setInterval(() => { if (document.visibilityState === 'visible') read.reload() }, SIDEBAR_POLL_MS)
    return () => clearInterval(timer)
  }, [read.reload])
  return read
}

/** One project with its Store, where it runs and whether it can run now. */
export interface ProjectItem { row: ProjectRow, store: StoreRow | undefined, runsOn: string[], availability: ProjectAvailability }

/** Catalog, machines and Stores, joined per project (§8: readiness follows the machine it runs on). */
export function useProjectData(api: LayerfsApi) {
  const machines = useProjectMachines(api)
  const catalog = useSteady(api, 'projects.catalog', () => listProjects(api), ['storage', 'layerfs-availability', 'layerfs-workspaces-changed', 'machine-progress', 'connection'])
  const storage = useSteady(api, 'GET /api/storage', () => api.call<StorageView>('GET', '/api/storage'), ['storage', 'storage-job', 'machine-progress', 'connection'])
  const item = (row: ProjectRow): ProjectItem => {
    const store = storage.data?.stores.find((s) => s.name === storeOf(row))
    const machine = primaryMachine(store, storage.data?.machines) ?? row.machine ?? null
    return { row, store, runsOn: store ? runsOn(store) : row.machine ? [row.machine] : [], availability: projectAvailability({ ...row, machine }, machines.data, machines.error) }
  }
  return {
    catalog, item,
    /** A machine's platform (docker, lima, …) for its mark. */
    platformOf: (machine: string) => storage.data?.machines.find((m) => m.name === machine)?.platform ?? machine,
    liveStores: storage.data ? new Set(storage.data.stores.map((x) => x.name)) : undefined,
    scratchKey: catalog.data?.scratchKey ?? null,
    reload: () => { catalog.reload(); storage.reload(); machines.reload() },
  }
}

export function useRecent(api: LayerfsApi) {
  return useSteady(api, 'projects.recent', () => api.op<{ projects: RecentProject[] }>('projects.recent'), ['layerfs-session', 'layerfs-workspaces-changed', 'connection'])
}

/** Start a project's machine, or open a session in it. */
function useActions(api: LayerfsApi, onDone: () => void) {
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<{ kind: 'open' | 'start', message: string } | null>(null)
  const go = async (kind: 'open' | 'start', work: () => Promise<unknown>) => {
    setBusy(true); setError(null)
    try { await work(); onDone() } catch (e) { setError({ kind, message: String((e as Error).message ?? e) }) } finally { setBusy(false) }
  }
  return {
    busy, error,
    create: (p: ProjectRow) => go('open', () => api.openProjectSession(p.key, undefined, p.projectId)),
    start: (machine: string) => go('start', () => api.call('POST', `/api/machines/${enc(machine)}/start`)),
  }
}

function ActionError({ api, t, error }: { api: LayerfsApi, t: ListT, error: { kind: 'open' | 'start', message: string } | null }): ReactNode {
  if (!error) return null
  return <div className={clsx(css.notice, css.error)}>
    <span role="alert">{t(error.kind === 'start' ? 'startFailed' : 'actionFailed')}</span>
    <Button variant="ghost" size="sm" onClick={() => openMachineHome(api, null)}>{t('machines')}</Button>
    <details><summary>{t('details')}</summary><div className={css.errorDetails}>{error.message}</div></details>
  </div>
}

/** Archives one conversation in DSH (`sessions.archive`): null when archived, else DSH's refusal in its own words. */
export function archiveConversation(api: Pick<LayerfsApi, 'op'>, sessionId: string): Promise<string | null> {
  return api.op('sessions.archive', { sessionId }).then(() => null, (e: unknown) => String((e as Error)?.message ?? e))
}

/**
 * One conversation under a project: its title (a press opens it), Interrupted
 * when so, and Archive where DSH can archive it ({@link archivable}). A
 * refused Archive reads "Not archived" with DSH's message as the tip and one
 * action, Try again (RV4 FAIL 2: no refusal is dropped).
 */
export function SessionLine({ session, title, selected, t, onOpen, onArchive }: {
  session: ProjectSession, title: string, selected: boolean, t: ListT
  onOpen: () => void
  /** Archives it: null when done, else the refusal's message. */
  onArchive: () => Promise<string | null>
}): ReactNode {
  const [busy, setBusy] = useState(false)
  const [refusal, setRefusal] = useState<string | null>(null)
  const archive = () => { setBusy(true); setRefusal(null); void onArchive().then(setRefusal).finally(() => setBusy(false)) }
  return <>
    <li className={css.sessionItem}>
      <button type="button" className={css.session} data-selected={selected || undefined} onClick={onOpen}>
        <span className={css.slot} aria-hidden="true" />
        <span className={css.title}>{title}</span>
        {session.executionState === 'interrupted' ? <span className={css.interrupted} title={session.error ?? t('interruptedHint')}>{t('interrupted')}</span> : null}
      </button>
      {/* B2 #7: the row's one action, Archive (DSH's archive set; DSH's archived list unarchives it), only where DSH can */}
      {archivable(session) ? <Button className={clsx(css.iconButton, css.sessionAction)} variant="ghost" size="sm" icon={<IconArchiveOutlineRegular size={14} />} disabled={busy}
        aria-label={t('archiveSession', { session: title })} title={t('archiveSession', { session: title })} data-archive-session={session.sessionId}
        onClick={archive} /> : null}
    </li>
    {refusal !== null ? <li className={clsx(css.notice, css.error)} data-archive-refused={session.sessionId}>
      <span role="alert" title={refusal}>{t('archiveRefused')}</span>
      <Button variant="ghost" size="sm" disabled={busy} onClick={archive}>{t('archiveAgain')}</Button>
    </li> : null}
  </>
}

/** One sidebar row (§4). */
function SidebarRow({ item, showStore, api, titles, t, onChanged }: { item: ProjectItem, showStore: boolean, api: LayerfsApi, titles: Record<string, SessionSummary>, t: ListT, onChanged: () => void }): ReactNode {
  const { row: project, availability } = item
  const [open, setOpen] = useState(false)
  const actions = useActions(api, onChanged)
  // A name two listed projects share carries its Store, on screen and in every accessible name.
  const label = showStore ? `${nameOf(project)} · ${storeOf(project)}` : nameOf(project)
  const read = useProjectSessions(api, project.key, open, project.projectId)
  // its Start <machine> behind the press guard, like every power control
  const startGuard = usePressGuard(`start:${availability.machine ?? ''}:${availability.state}`)
  const selected = api.selectedSessionId()
  const sessions = visibleProjectSessions(read.sessions, titles, selected)
  const status = sidebarStatus(project, availability)
  const reason = availability.reason || t(status.key)
  const blockedTip = t('cannotRun', { reason })
  const sessionReason = !availability.canRun ? blockedTip : actions.busy ? t('openingSession') : undefined
  const startReason = actions.busy ? t('starting') : startGuard.settling ? t('reviewStart') : undefined
  const resume = (sessionId: string) => { void api.op('sessions.open', { sessionId, project: project.key }).catch(() => {}) }
  const where = item.runsOn.join(', ')
  return <li data-layerfs-project={idOf(project)} data-ready={availability.canRun || undefined} className={css.projectItem}>
    <div className={clsx(css.projectRow, !availability.canRun && css.inactive)} data-open={open || undefined} data-actions={availability.canStart ? 1 : 2}>
      <button className={css.projectToggle} type="button"
        title={`${nameOf(project)} · ${storeOf(project)}${where ? ` · ${where}` : ''}`} aria-expanded={open} aria-label={t('expandProject', { project: label })} onClick={() => setOpen(!open)}>
        <LeadingSlot open={open} />
        <span className={css.title}>{nameOf(project)}{showStore ? <span className={css.store}> · {storeOf(project)}</span> : null}</span>
        {!availability.canRun && availability.state !== 'checking' ? <span className={css.why} title={blockedTip}>{t(status.key)}</span> : null}
      </button>
      <span className={css.rowActions}>
        <Button className={css.iconButton} variant="ghost" size="sm" icon={<IconSettingsOutlineRegular size={14} />} aria-label={t('configure', { project: label })} title={t('configure', { project: label })}
          onClick={() => projectDialog.open({ mode: 'configure', project: { key: project.key, projectId: project.projectId, name: nameOf(project), store: storeOf(project) } })} />
        {availability.canStart ? null : <Button className={css.iconButton} variant="ghost" size="sm" disabled={actions.busy || !availability.canRun} icon={<IconPlusOutlineRegular size={14} />}
          aria-label={t('newSession', { project: label })} aria-description={sessionReason} title={sessionReason ?? t('newSession', { project: label })}
          onClick={() => { setOpen(true); void actions.create(project) }} />}
        {availability.canStart
          ? <Button className={css.start} variant="ghost" size="sm" disabled={actions.busy || startGuard.settling} aria-disabled={startGuard.settling || undefined} title={startReason} aria-description={startReason} aria-label={t('startMachineFor', { machine: availability.machine ?? '', project: label })} onClick={startGuard(() => void actions.start(availability.machine!))}>{t('startNamed', { machine: availability.machine ?? '' })}</Button>
          : null}
      </span>
    </div>
    <ActionError api={api} t={t} error={actions.error} />
    {open ? <ul className={css.sessions}>
      {read.error ? <li className={css.notice}>
        <span role="alert">{t('archiveFailed', { message: errorMessage(read.error, t) })}</span>
        <Button variant="ghost" size="sm" onClick={read.reload}>{t('retry')}</Button>
      </li> : read.loading ? <li className={css.notice}>{t('archiveLoading')}</li> : null}
      {sessions.length ? sessions.map((session) => {
        const title = titles[session.sessionId]?.title || session.title || t('untitled')
        return <SessionLine key={session.sessionId} session={session} title={title} selected={selected === session.sessionId} t={t}
          onOpen={() => resume(session.sessionId)}
          onArchive={() => archiveConversation(api, session.sessionId).then((refusal) => { if (refusal === null) read.reload(); return refusal })} />
      }) : !read.error && !read.loading ? <li className={css.notice}>{t('noSessions')}</li> : null}
    </ul> : null}
  </li>
}

/** The projects with sessions the catalog does not list (sorted keys joined), or '' when there are none. */
export function missingKey(rows: readonly { row: { key: string, catalogState?: unknown } }[], catalog: readonly { key: string }[] | null): string {
  return rows.filter((r) => !r.row.catalogState && !catalog?.some((p) => p.key === r.row.key)).map((r) => r.row.key).sort().join('\n')
}

/** The sidebar's projects with their Store, machine and availability, and the catalog's size. */
export function useSidebarProjects(api: LayerfsApi, sessionSource?: SessionSource) {
  const summaries = useSummaries(sessionSource)
  const hidden = useSyncExternalStore(sidebarHidden.subscribe, sidebarHidden.get)
  const data = useProjectData(api)
  const recent = useRecent(api)
  const rows = recent.data ? sidebarProjects(recent.data.projects, data.catalog.data ? data.catalog.data.rows : null, hidden, data.liveStores) : null
  // A project with sessions the catalog doesn't list yet (just created): read the catalog again,
  // once per such set of projects. A catalog read that still lacks them (or was refused) is a
  // result, not a reason to read again on the next sessions poll.
  const missing = missingKey(rows ?? [], data.catalog.data?.rows ?? null)
  const asked = useRef<string | null>(null)
  useEffect(() => { if (missing && asked.current !== missing) { asked.current = missing; data.catalog.reload() } }, [missing])
  return {
    items: rows?.map((r) => ({ ...data.item(r.row), showStore: r.showStore, hideKey: r.hideKey })) ?? null,
    scratchKey: data.scratchKey,
    error: recent.error ?? data.catalog.error,
    titles: summaries.byId,
    reload: () => { recent.reload(); data.reload() },
  }
}
export type SidebarData = ReturnType<typeof useSidebarProjects>

/** The sidebar's LayerFS list (§4): project rows. */
export function SidebarProjects({ api, t, data }: { api: LayerfsApi, t: ListT, data: SidebarData }): ReactNode {
  const { items } = data
  return <div className={css.tree}>
    {data.error ? <div className={css.notice} role="alert">{t('failed', { message: errorMessage(data.error, t) })}<Button variant="ghost" size="sm" onClick={data.reload}>{t('retry')}</Button></div> : null}
    {!items && !data.error && <div className={css.notice}>{t('loading')}</div>}
    {items && (!items.length ? <div className={css.empty}>
      <div className={css.notice}>{t('emptyRecent')}</div>
      <div className={css.emptyActions}>
        <Button variant="outline" size="sm" icon={<IconPlusOutlineRegular size={14} />} onClick={() => projectDialog.open({ mode: 'new' })}>{t('newProject')}</Button>
      </div>
    </div> : <ul className={css.projects}>{items.map((item) => <SidebarRow key={idOf(item.row)} item={item} showStore={item.showStore} api={api} titles={data.titles} t={t} onChanged={data.reload} />)}</ul>)}
  </div>
}
