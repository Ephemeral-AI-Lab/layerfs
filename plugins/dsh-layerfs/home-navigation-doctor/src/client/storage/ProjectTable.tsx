/**
 * The project table, shared by Home › Projects (ProjectTable) and a Store's
 * Projects tab (StoreProjects.tsx, through ProjectGrid). A row per project:
 * the LayerFS mark, its name (with "N running" while a Sandbox of it runs,
 * and on Home a dot when its Store needs you) over its newest Commit as one
 * line; then on Home its Store; its sessions; its Sandboxes; when a session
 * of it last changed (the default order, held still while sessions run);
 * when it last committed; its
 * Commits; its Branches; and its size (on a Store's page with a thin bar for
 * its share of that Store). A row opens to one list at a time, the project's
 * sessions or its Sandboxes, with a quiet switch to the other; the choice and
 * the open rows are kept for this viewer. The name opens the project.
 * Sandboxes and sessions both belong to the project: nothing ties a session
 * to a Sandbox.
 *
 * Reads: the figures and newest Commit from each Store's projects answer
 * (`/api/stores/<store>/projects`, one read per Store, polled while a ledger
 * is still counting); on Home the size from the size route the finder sorts
 * by; each row's session count from the local session list (no Store walk),
 * reread on session events, coalesced; the full list only while a row shows it.
 */
import { useContext, useEffect, useRef, useState, type MouseEvent, type ReactNode } from 'react'
import clsx from 'clsx'
import {
  IconChevronRightOutlineRegular, IconPlusOutlineRegular, IconTrashOutlineRegular, MenuItemButton,
} from '@deepseek-ai/dsh-client-ui-primitives'
import { ago, bytes, when } from '../files/format.ts'
import type { Translate } from '../files/types.ts'
import { MachineGlyph, SandboxGlyph } from '../machines/icons.tsx'
import { shortSandbox, type AvailableView, type MachineInfo, type MachineSandbox } from '../machines/model.ts'
import { sandboxRuns } from '../machines/stats.ts'
import { useProjectSessions, type ProjectSession } from '../projects/hooks.ts'
import { Card, FixAction, FixContext, MoreMenu, resolveFix } from '../ui/index.tsx'
import { useQuery } from '../ui/use-query.ts'
import { enc, type LayerfsApi } from '../api.ts'
import { LayerfsGlyph } from '../overview/glyph.tsx'
import type { TabId } from './tabs.ts'
import type { StorageKey, StorageT } from './locale.ts'
import { useHostEvents } from './hooks.ts'
import { HomeGlyph, StoreGlyph, refusalLabels } from './parts.tsx'
import { Signal } from './Signal.tsx'
import { JobLine, SIZE_POLL_MS, sandboxesOfProject, storeTrouble, troubleAction, type Place, type Trouble } from './HomeCards.tsx'
import { doingWords, sessionsUnavailable } from './model.ts'
import { orderProjects, sessionOrder, sizeCacheKey, type ProjectSize, type ProjectSort } from './project-list.ts'
import { commitLine, stillCounting, type ProjectOrder, type StoreProject, type StoreProjectsView } from './store-page.ts'
import type { StorageJob, StorageView, StoreRow } from './types.ts'
import css from './ProjectTable.module.css'

type List = 'sessions' | 'sandboxes'

const OPEN_KEY = 'dsh-layerfs:projects-open'
const LIST_KEY = 'dsh-layerfs:projects-list'
/** While a Store's ledgers are still on their first read, its projects answer is asked again this often. */
const COUNTING_POLL_MS = 2000

/** Read a viewer convenience from storage; nothing breaks without it. */
function stored<T>(key: string, fallback: T): T {
  try { const v = localStorage.getItem(key); return v == null ? fallback : JSON.parse(v) as T } catch { return fallback }
}
function keep(key: string, value: unknown): void {
  try { localStorage.setItem(key, JSON.stringify(value)) } catch { /* storage unavailable */ }
}

/** A row while its Store's answer is not in yet: the name, no figures. */
export const placeholderProject = (name: string): StoreProject => ({ name, created: null, totals: null, last: null, files: null, partial: true, building: null, error: null })

/** One row of the shared table. */
export interface GridRow {
  /** `store/project` */
  key: string
  store: { name: string, kind: string, state?: string | null }
  p: StoreProject
  boxes: readonly MachineSandbox[]
  /** Its share of its Store's bytes (a Store's page), for the bar under its size. */
  share?: number | null
  /** Its Store's trouble (Home), as a dot beside the name. */
  trouble?: Trouble | null
  job?: StorageJob | undefined
  /** Read the size from the size route (Home, which the finder sorts by): alone in its Store or not. */
  sizeRoute?: { alone: boolean } | undefined
  /** Its sessions in this DSH (useSessionLists), or null before the first answer. */
  sessions: readonly ProjectSession[] | null
}

const COLUMNS: readonly { order: ProjectOrder, label: StorageKey, cls: string, title?: StorageKey }[] = [
  { order: 'session', label: 'ptLastSession', cls: 'colSession', title: 'ptLastSessionTitle' },
  { order: 'recent', label: 'spColUpdated', cls: 'colWhen' },
  { order: 'commits', label: 'spColCommits', cls: 'colCommits' },
  { order: 'branches', label: 'spColBranches', cls: 'colBranches', title: 'spColBranchesTitle' },
  { order: 'size', label: 'spColSize', cls: 'colSize' },
]

/**
 * The shared table. `showStore` adds the Store column (Home); `order` and
 * `onOrder` make the figure headings sort (a Store's page; Home sorts with
 * its finder). `onOpen` opens a page by its id (a Store, a Sandbox, a
 * project); `onRemove` adds Delete to a row's menu.
 */
export function ProjectGrid({ api, t, now, rows, label, showStore, order, onOrder, fleet, place, tick, onOpen, onRemove, onRestart }: {
  api: LayerfsApi, t: StorageT, now: number, rows: readonly GridRow[], label: string
  showStore: boolean
  order?: ProjectOrder | undefined, onOrder?: ((order: ProjectOrder) => void) | undefined
  fleet: AvailableView | null, place: Place, tick: number
  onOpen: (id: TabId) => void
  onRemove?: ((store: string, project: string) => void) | undefined
  onRestart?: ((m: MachineInfo) => Promise<unknown>) | undefined
}): ReactNode {
  const [opened, setOpened] = useState<Record<string, List | false>>(() => stored(OPEN_KEY, {}))
  const [lastList, setLastList] = useState<List>(() => stored<List>(LIST_KEY, 'sessions'))
  const setOpen = (key: string, list: List | false) => {
    setOpened((all) => { const next = { ...all, [key]: list }; keep(OPEN_KEY, next); return next })
    if (list) { setLastList(list); keep(LIST_KEY, list) }
  }
  const heading = (c: typeof COLUMNS[number]) => {
    const text = t(c.label)
    const title = c.title ? t(c.title) : undefined
    if (!onOrder) return <span key={c.order} role="columnheader" className={clsx(css.num, css[c.cls])} title={title}>{text}</span>
    return (
      <span key={c.order} role="columnheader" className={clsx(css.num, css[c.cls])}>
        <button type="button" className={css.sort} data-active={order === c.order || undefined} aria-pressed={order === c.order}
          title={[t('spSortBy', { column: text }), title].filter(Boolean).join(' · ')} onClick={() => onOrder(c.order)}>{text}</button>
      </span>
    )
  }
  return (
    <Card className={css.card}>
      <div className={css.table} role="table" aria-label={label} data-store-column={showStore || undefined}>
        <div className={clsx(css.row, css.head)} role="row">
          <span role="columnheader" aria-hidden="true" />
          <span role="columnheader">
            {onOrder
              ? <button type="button" className={css.sort} data-active={order === 'name' || undefined} aria-pressed={order === 'name'}
                  title={t('spSortBy', { column: t('ptProject') })} onClick={() => onOrder('name')}>{t('ptProject')}</button>
              : t('ptProject')}
          </span>
          {showStore ? <span role="columnheader" className={css.colStore}>{t('ptStore')}</span> : null}
          <span role="columnheader" className={css.colSessions}>{t('ptSessions')}</span>
          <span role="columnheader" className={css.colSandboxes}>{t('ptSandboxes')}</span>
          {COLUMNS.map(heading)}
          <span role="columnheader" aria-hidden="true" />
        </div>
        {rows.map((r) => (
          <ProjectRow key={r.key} api={api} r={r} t={t} now={now} tick={tick} showStore={showStore}
            fleet={fleet} place={place} open={opened[r.key] ?? false} lastList={lastList} onSetOpen={(list) => setOpen(r.key, list)}
            onOpen={onOpen} onRemove={onRemove} onRestart={onRestart} />
        ))}
      </div>
    </Card>
  )
}

/** Each Store's projects answer, read one Store at a time as it lands (kept across remounts), polled while counting. */
const storeAnswers = new Map<string, StoreProjectsView>()
function useStoresProjects(api: LayerfsApi, names: readonly string[], tick: number): ReadonlyMap<string, StoreProjectsView> {
  const [answers, setAnswers] = useState<ReadonlyMap<string, StoreProjectsView>>(() => new Map(names.flatMap((n) => (storeAnswers.has(n) ? [[n, storeAnswers.get(n)!] as const] : []))))
  const [poll, setPoll] = useState(0)
  const key = names.join('\n')
  // one read per Store at a time; a refresh asked for while one runs queues one more after it (never dropped)
  const running = useRef(new Set<string>())
  const again = useRef(new Set<string>())
  const live = useRef(true)
  useEffect(() => () => { live.current = false }, [])
  useEffect(() => {
    live.current = true
    const read = (name: string) => {
      if (running.current.has(name)) { again.current.add(name); return }
      running.current.add(name)
      void api.call<StoreProjectsView>('GET', `/api/stores/${enc(name)}/projects`).then((view) => {
        storeAnswers.set(name, view)
        if (live.current) setAnswers((all) => new Map(all).set(name, view))
      }, () => {}).finally(() => {
        running.current.delete(name)
        if (again.current.delete(name)) read(name)
      })
    }
    for (const name of names) read(name)
  }, [api, key, tick, poll])
  const counting = [...answers.values()].some((v) => v.projects.some(stillCounting))
  useEffect(() => {
    if (!counting) return
    const timer = setTimeout(() => setPoll((n) => n + 1), COUNTING_POLL_MS)
    return () => clearTimeout(timer)
  }, [counting, answers])
  return answers
}

/** The newest `updatedAt` among a project's sessions, or null. */
export const lastSessionOf = (sessions: readonly ProjectSession[] | null | undefined): number | null =>
  sessions?.length ? Math.max(...sessions.map((x) => x.updatedAt || 0)) || null : null

/** The session lists, kept across remounts (a tab switch draws the last answer at once). */
const sessionAnswers = new Map<string, ProjectSession[]>()

/**
 * Each project's sessions in this DSH (`sessions.list`, local only: no Store
 * walk), read once per key and again after each burst of session events
 * (coalesced). `ready` once every key has answered (or failed) once.
 */
export function useSessionLists(api: LayerfsApi, keys: readonly string[]): { lists: ReadonlyMap<string, readonly ProjectSession[]>, ready: boolean } {
  const [lists, setLists] = useState<ReadonlyMap<string, readonly ProjectSession[]>>(() => new Map(keys.flatMap((k) => (sessionAnswers.has(k) ? [[k, sessionAnswers.get(k)!] as const] : []))))
  const [answered, setAnswered] = useState<ReadonlySet<string>>(() => new Set(keys.filter((k) => sessionAnswers.has(k))))
  const [tick, setTick] = useState(0)
  useHostEvents(api, ['layerfs-session', 'layerfs-workspaces-changed', 'connection'], () => setTick((n) => n + 1), { coalesceMs: 2_000 })
  const sig = keys.join('\n')
  // one read per key at a time; a refresh asked for while one runs queues one more after it (never dropped)
  const running = useRef(new Set<string>())
  const again = useRef(new Set<string>())
  const live = useRef(true)
  useEffect(() => () => { live.current = false }, [])
  useEffect(() => {
    live.current = true
    const read = (key: string) => {
      if (running.current.has(key)) { again.current.add(key); return }
      running.current.add(key)
      void api.op<{ sessions: ProjectSession[] }>('sessions.list', { project: key, localOnly: true }).then((value) => {
        const rows = value?.sessions ?? []
        sessionAnswers.set(key, rows)
        if (live.current) setLists((all) => new Map(all).set(key, rows))
      }, () => {}).finally(() => {
        running.current.delete(key)
        if (live.current) setAnswered((all) => (all.has(key) ? all : new Set(all).add(key)))
        if (again.current.delete(key)) read(key)
      })
    }
    for (const key of keys) read(key)
  }, [api, sig, tick])
  return { lists, ready: keys.every((k) => answered.has(k)) }
}

/** The Last session orders worked out, per table, kept across remounts. */
const sessionOrders = new Map<string, { sig: string, reset: number, order: readonly string[] }>()

/**
 * The Last session order for one table (`scope`), worked out once the session
 * lists are all in, and then held: sessions writing at different speeds never
 * move a row. It is worked out again only when this order is picked, the
 * page's Refresh is pressed (`reset`), or the projects themselves change.
 * Null until the first one exists (the rows stay in their given order).
 */
export function useSessionOrder(scope: string, keys: readonly string[], sessions: { lists: ReadonlyMap<string, readonly ProjectSession[]>, ready: boolean }, active: boolean, reset: number): readonly string[] | null {
  const sig = [...keys].sort().join('\n')
  const [order, setOrder] = useState<readonly string[] | null>(() => {
    const kept = sessionOrders.get(scope)
    return kept && kept.sig === sig ? kept.order : null
  })
  const was = useRef(active)
  useEffect(() => {
    const picked = active && !was.current
    was.current = active
    if (!active || !sessions.ready) return
    const kept = sessionOrders.get(scope)
    if (!picked && kept && kept.sig === sig && kept.reset === reset) { if (order !== kept.order) setOrder(kept.order); return }
    const next = sessionOrder(keys, (k) => lastSessionOf(sessions.lists.get(k)))
    sessionOrders.set(scope, { sig, reset, order: next })
    setOrder(next)
  }, [scope, sig, active, sessions.ready, reset])
  return active ? order : null
}

/** Home › Projects: every Store's projects in the finder's order (project-list.ts: search, then Default / Size / Name). */
export function ProjectTable({ api, tick, query, sort, frozen, stores, view, fleet, t, now, jobs, place, onOpen, onRemove, onRestart }: {
  api: LayerfsApi
  /** The search and sort above the table (project-list.ts); `frozen` is the Size order. */
  query: string, sort: ProjectSort, frozen: readonly string[] | null
  /** Bumped by the Overview's refresh: each row reads again. */
  tick: number
  stores: readonly StoreRow[], view: StorageView, fleet: AvailableView | null, t: StorageT, now: number
  jobs: readonly StorageJob[], place: Place, onOpen: (id: TabId) => void
  /** Delete a project (the page's confirm dialog). */
  onRemove: (store: string, project: string) => void
  /** Start a machine's worker again (a Store whose machine does not answer). */
  onRestart?: (m: MachineInfo) => Promise<unknown>
}): ReactNode {
  const sandboxes = fleet?.sandboxes ?? []
  const answers = useStoresProjects(api, stores.map((s) => s.name), tick)
  const keys = stores.flatMap((s) => s.projects.map((p) => `${s.name}/${p}`))
  const sessions = useSessionLists(api, keys)
  const sessionFrozen = useSessionOrder('home', keys, sessions, sort === 'session', tick)
  const rank = (x: Trouble | null) => (x?.state === 'error' ? 0 : x?.state === 'warning' ? 1 : 2)
  // Default: trouble first, then the Stores' own order; then the search and sort.
  const projects = orderProjects(stores
    .flatMap((s, order) => s.projects.map((p) => ({ s, p, store: s.name, project: p, order, trouble: storeTrouble(s, view, t, place, fleet) })))
    .sort((a, b) => rank(a.trouble) - rank(b.trouble) || a.order - b.order || a.p.localeCompare(b.p)), query, sort, sort === 'session' ? sessionFrozen : frozen)
  if (projects.length === 0) {
    return (
      <div className={css.noMatch} role="status">
        <span>{t('ovNoMatch', { query: query.trim() })}</span>
      </div>
    )
  }
  const rows: GridRow[] = projects.map(({ s, p, trouble }) => ({
    key: `${s.name}/${p}`,
    store: { name: s.name, kind: s.kind, state: s.state },
    p: answers.get(s.name)?.projects.find((x) => x.name === p) ?? placeholderProject(p),
    boxes: sandboxesOfProject(sandboxes, stores, s.name, p),
    trouble,
    job: jobs.find((j) => j.store === s.name && !j.ended),
    sizeRoute: { alone: s.projects.length === 1 },
    sessions: sessions.lists.get(`${s.name}/${p}`) ?? null,
  }))
  return <ProjectGrid api={api} t={t} now={now} rows={rows} label={t('ovProjects')} showStore fleet={fleet} place={place} tick={tick}
    onOpen={onOpen} onRemove={onRemove} onRestart={onRestart} />
}

/** Whether a click on the row came from one of its own controls (or a menu portalled out of it). */
function fromControl(e: MouseEvent<HTMLElement>): boolean {
  const target = e.target as Element
  if (!e.currentTarget.contains(target)) return true
  return target.closest('button, a, input, select, [role="menu"], [role="menuitem"], [data-signal]') !== null
}

function ProjectRow({ api, r, t, now, tick, showStore, fleet, place, open, lastList, onSetOpen, onOpen, onRemove, onRestart }: {
  api: LayerfsApi, r: GridRow, t: StorageT, now: number, tick: number, showStore: boolean
  fleet: AvailableView | null, place: Place
  open: List | false, lastList: List, onSetOpen: (list: List | false) => void
  onOpen: (id: TabId) => void, onRemove?: ((store: string, project: string) => void) | undefined, onRestart?: ((m: MachineInfo) => Promise<unknown>) | undefined
}): ReactNode {
  const { key, p, store, boxes, trouble, job, share } = r
  const tt = t as Translate
  const routed = useProjectSize(api, key, r.sizeRoute?.alone ?? false, tick, !!r.sizeRoute)
  const [sessionError, setSessionError] = useState<string | null>(null)
  const blockedSession = sessionsUnavailable(store) ? t('ovSessionBlocked') : undefined
  const local = r.sessions
  const lastSession = lastSessionOf(local)
  const active = local?.filter((x) => x.executionState === 'active').length ?? 0
  const running = boxes.filter(sandboxRuns).length
  const line = p.last ? commitLine(p.last) : null
  const at = p.last?.time ?? null
  const counting = p.partial && p.building
  // a project whose history was not read (its Store broken, or a refusal): its figures are unknown, "—", never 0
  const unread = !!trouble?.store || !!p.error
  const totals = unread ? null : p.totals
  const size = unread ? null : r.sizeRoute ? routed.stored ?? null : p.totals && p.totals.stored > 0 ? p.totals.stored : null
  const sizeWaiting = r.sizeRoute ? routed.counting : !!counting
  const detailId = `project-detail-${key}`
  const openProject = () => onOpen(`project:${key}`)
  const toggle = () => onSetOpen(open ? false : lastList)
  return (
    <div className={css.group} data-project={key} data-project-card={p.name} data-open={open || undefined} role="rowgroup" aria-label={p.name}>
      <div className={css.row} role="row" onClick={(e) => { if (!fromControl(e)) toggle() }}>
        <button type="button" className={css.toggle} aria-expanded={!!open} aria-controls={detailId}
          aria-label={t(open ? 'ptCollapse' : 'ptExpand', { name: p.name })} title={t(open ? 'ptCollapse' : 'ptExpand', { name: p.name })} onClick={toggle}>
          <IconChevronRightOutlineRegular size={12} />
        </button>
        <span className={css.identity} role="cell">
          <span className={css.mark} aria-hidden="true"><LayerfsGlyph size={20} /></span>
          <span className={css.main}>
            <span className={css.nameLine}>
              <button type="button" className={css.name} onClick={openProject} title={t('ptOpenProject', { name: p.name })}>{p.name}</button>
              {running ? <span className={css.runningPill}>{t('spRunningN', { n: String(running) })}</span> : null}
              {trouble ? (
                <span data-signal>
                  <Signal state={trouble.state} title={trouble.reason} detail={trouble.detail}
                    action={troubleAction(trouble, t, onRestart)} />
                </span>
              ) : null}
            </span>
            <span className={css.commitLine} title={line?.message || undefined}>
              {line ? (
                <>
                  <span className={css.ellipsis}>{line.session ? t('spSessionSave', { id: line.session.id, range: line.session.range }) : line.message || t('spNoMessage')}</span>
                  {line.branch ? <code className={css.branch}>{line.branch}</code> : null}
                </>
              ) : (
                p.totals ? null
                  : counting ? <span className={css.ellipsis}>{t('spCounting', { n: String(p.building!.saves) })}</span>
                  : p.error ? <HistoryUnread error={p.error} t={t} />
                  : <span className={css.ellipsis}>{t('spReading')}</span>
              )}
            </span>
          </span>
        </span>
        {showStore ? (
          <span role="cell" className={css.colStore}>
            <button type="button" className={css.store} title={store.name} onClick={() => onOpen(`store:${store.name}`)}>
              <StoreGlyph kind={store.kind} size={13} /><span className={css.ellipsis}>{store.name}</span>
            </button>
          </span>
        ) : null}
        <span role="cell" className={css.colSessions}>
          {local == null || local.length === 0
            ? <span className={css.muted}>{local == null ? '' : '—'}</span>
            : (
              <button type="button" className={css.cellLink} onClick={() => onSetOpen('sessions')}
                title={active > 0 ? t('ptSessionsActive', { n: local.length, active }) : undefined}>
                <span>{String(local.length)}</span>
              </button>
            )}
        </span>
        <span role="cell" className={css.colSandboxes}>
          {boxes.length === 0
            ? <span className={css.muted}>—</span>
            : (
              <button type="button" className={css.cellLink} onClick={() => onSetOpen('sandboxes')}
                title={[running > 0 ? t('ptRunning', { n: running }) : null, ...boxes.map((b) => `${b.name} · ${place(b.machine)}`)].filter(Boolean).join('\n')}>
                <span>{String(boxes.length)}</span>
              </button>
            )}
        </span>
        <span role="cell" className={clsx(css.num, css.colSession, lastSession === null && css.muted)} title={lastSession !== null ? when(lastSession) : undefined}>
          {lastSession !== null ? ago(lastSession, now, tt) : local == null ? '' : '—'}
        </span>
        <span role="cell" className={clsx(css.num, css.colWhen, css.muted)} title={when(at ?? p.created)}>
          {at !== null ? ago(at, now, tt) : p.created !== null ? t('createdAgo', { ago: ago(p.created, now, tt) }) : '—'}
        </span>
        <span role="cell" className={clsx(css.num, css.colCommits, !totals && css.muted)}>{totals ? totals.saves.toLocaleString() : '—'}</span>
        <span role="cell" className={clsx(css.num, css.colBranches, !totals && css.muted)}>{totals ? totals.branches.toLocaleString() : '—'}</span>
        <span role="cell" className={clsx(css.num, css.colSize)}
          title={share != null ? t('spShareOfStore', { n: share < 0.01 && share > 0 ? '<1' : String(Math.round(share * 100)) }) : r.sizeRoute ? (r.sizeRoute.alone ? t('ovSizeTitleAlone') : t('ovSizeTitle')) : undefined}>
          <span className={clsx(size === null && css.muted)}>{size !== null ? bytes(size) : sizeWaiting && !unread ? t('ovSizeCounting') : '—'}</span>
          {share != null ? <span className={css.share} aria-hidden="true"><span style={{ width: `${Math.max(share * 100, share > 0 ? 2 : 0).toFixed(1)}%` }} /></span> : null}
        </span>
        <span role="cell" className={css.actions}>
          <MoreMenu label={t('projectActions', { name: p.name })}>
            {(close) => (
              <>
                <MenuItemButton icon={<IconChevronRightOutlineRegular size={14} />} onSelect={() => { close(); openProject() }}>{t('ovOpen')}</MenuItemButton>
                <span title={blockedSession}><MenuItemButton disabled={!!blockedSession} icon={<IconPlusOutlineRegular size={14} />} onSelect={() => {
                  close(); setSessionError(null)
                  void api.openProjectSession(key).catch((e: unknown) => setSessionError(String((e as Error)?.message ?? e)))
                }}>{t('ptNewSession')}</MenuItemButton></span>
                {onRemove ? (
                  <MenuItemButton separatorBefore danger icon={<IconTrashOutlineRegular size={14} />} onSelect={() => { close(); onRemove(store.name, p.name) }}>
                    {t('removeProject')}
                  </MenuItemButton>
                ) : null}
              </>
            )}
          </MoreMenu>
        </span>
      </div>
      {sessionError && <p role="alert" className={css.error}>{sessionError}</p>}
      {job ? <div className={css.job}><JobLine job={job} t={t} label={`${store.name} · ${doingWords(job, t)}`} /></div> : null}
      {open && (
        <div className={css.detail} id={detailId}>
          <div className={css.panel}>
            {open === 'sessions'
              ? <SessionList api={api} project={key} t={t} other={boxes.length} onSwitch={() => onSetOpen('sandboxes')} down={sessionsUnavailable(store)} />
              : <SandboxList boxes={boxes} fleet={fleet} place={place} t={t} other={local?.length ?? null} onSwitch={() => onSetOpen('sessions')} onOpen={onOpen} />}
          </div>
        </div>
      )}
    </div>
  )
}

/**
 * A project whose history LayerFS could not read: a dot and the word under its
 * name; LayerFS's own message is the dot's tip, and its fix line (Start the
 * machine, say) the one action where the page can run it, else part of the tip.
 */
function HistoryUnread({ error, t }: { error: { code: string, message: string, fix?: string }, t: StorageT }): ReactNode {
  const fixes = useContext(FixContext)
  const resolved = resolveFix(fixes, error.fix)
  const line = resolved?.line || null
  const fix = fixes ? resolved?.fix ?? null : null
  const word = t('spHistoryUnread')
  return (
    <span className={css.unread} data-history-unread={error.code}>
      <span data-signal>
        <Signal state="warning" title={word} label={`${word}: ${error.message}`}
          detail={<>{error.message}{line && !fix ? <><br /><code>{fixes ? fixes.tip(line) : line}</code></> : null}</>}
          action={fix && fixes && line ? <FixAction line={line} fix={fix} runner={fixes} labels={refusalLabels(t)} /> : undefined} />
      </span>
    </span>
  )
}

/** A project's size, from the route the finder sorts by: the last answer at once, polled while its first count runs. Nothing is read unless `enabled`. */
function useProjectSize(api: LayerfsApi, key: string, alone: boolean, tick: number, enabled: boolean): { stored: number | undefined, files: number | null, counting: boolean } {
  const q = useQuery<ProjectSize | null>(() => (enabled ? api.call<ProjectSize>('GET', `/api/projects/${enc(key)}/size${alone ? '?alone=1' : ''}`) : Promise.resolve(null)), [api, key, alone, enabled],
    { refresh: [tick], cache: enabled ? sizeCacheKey(key) : `pt-size-off:${key}`, persist: enabled })
  const z = q.data
  const waiting = !!z && z.stored === null && z.counting
  useEffect(() => {
    if (!waiting) return
    const timer = setTimeout(q.reload, SIZE_POLL_MS)
    return () => clearTimeout(timer)
  }, [z, waiting, q.reload])
  return { stored: z?.stored ?? undefined, files: z?.files ?? null, counting: !z ? !q.error : z.counting }
}

/** The open row's head: the list it shows and its count; at its end, a quiet switch naming the other list, then the add verb. */
function ListHead({ title, count, other, onSwitch, action }: { title: string, count: number | null, other: string, onSwitch: () => void, action?: ReactNode }): ReactNode {
  return (
    <div className={css.listHead}>
      <h3 className={css.listTitle}>{title}</h3>
      {count != null ? <span className={css.muted}>{count}</span> : null}
      <span className={css.spacer} />
      <button type="button" className={css.add} onClick={onSwitch} data-list-switch>
        <SwapGlyph />{other}
      </button>
      {action ? <span className={css.headRule} aria-hidden="true" /> : null}
      {action}
    </div>
  )
}

function SwapGlyph(): ReactNode {
  return (
    <svg width="11" height="11" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="M3 5h10l-3-3M13 11H3l3 3" />
    </svg>
  )
}

/** A project's sessions, newest first: this DSH's own and those only in the Store. */
/** `down`: its Store is broken or blocked, so its sessions are listed but neither open nor start (owner Q-R3). */
function SessionList({ api, project, t, other, onSwitch, down }: { api: LayerfsApi, project: string, t: StorageT, other: number, onSwitch: () => void, down: boolean }): ReactNode {
  const { sessions, loading, error } = useProjectSessions(api, project)
  const [failed, setFailed] = useState<string | null>(null)
  const now = Date.now()
  const state = (x: ProjectSession): { words: string, live: boolean, quiet?: boolean } =>
    !x.local ? { words: t('ptInStore'), live: false }
      : x.executionState === 'active' ? { words: t('ptActive'), live: true }
        : x.executionState === 'interrupted' ? { words: t('ptInterrupted'), live: false }
          : { words: t('ptIdle'), live: false, quiet: true }
  const openSession = (sessionId: string) => {
    setFailed(null)
    void api.op('sessions.open', { sessionId, project }).catch((e: unknown) => setFailed(String((e as Error)?.message ?? e)))
  }
  const create = () => {
    setFailed(null)
    void api.openProjectSession(project).catch((e: unknown) => setFailed(String((e as Error)?.message ?? e)))
  }
  return (
    <>
      <ListHead title={t('ptSessions')} count={sessions.length || (loading ? null : 0)} other={t('ptShowSandboxes', { n: other })} onSwitch={onSwitch}
        action={<button type="button" disabled={down} title={down ? t('ovSessionBlocked') : undefined} className={css.add} onClick={create}><IconPlusOutlineRegular size={11} />{t('ptNewSession')}</button>} />
      {failed ? <p className={css.error} role="alert">{failed}</p> : null}
      {sessions.length === 0
        ? <p className={css.empty}>{loading ? t('ptReadingSessions') : error ? t('ptSessionsUnread') : t('ptNoSessions')}</p>
        : (
          <ul className={css.lines}>
            {sessions.map((x) => {
              const look = state(x)
              return (
                <li key={x.sessionId}>
                  <button type="button" className={clsx(css.line, css.sessionLine)} disabled={down} onClick={() => openSession(x.sessionId)} data-session={x.sessionId}
                    title={down ? t('ovSessionBlocked') : x.error ?? x.title ?? x.sessionId}>
                    <span className={css.lineIcon} aria-hidden="true">{x.local ? <ChatGlyph /> : <CloudGlyph />}</span>
                    <span className={clsx(css.ellipsis, (!x.local || !x.title) && css.dim)}>
                      {x.title || t('ptUntitled')}{x.title ? null : <span className={css.sessionId}>{x.sessionId.replace(/^session-/, '').slice(0, 8)}</span>}
                    </span>
                    <span className={css.lineState} data-tone={look.live ? 'success' : x.executionState === 'interrupted' ? 'warning' : undefined}>
                      {look.quiet ? null : look.words}
                    </span>
                    <span className={clsx(css.muted, css.num)}>{x.updatedAt ? ago(x.updatedAt, now, t as Translate) : ''}</span>
                  </button>
                </li>
              )
            })}
          </ul>
        )}
    </>
  )
}

/** A project's Sandboxes: each with its machine and its state. */
function SandboxList({ boxes, fleet, place, t, other, onSwitch, onOpen }: {
  boxes: readonly MachineSandbox[], fleet: AvailableView | null, place: Place, t: StorageT, other: number | null, onSwitch: () => void, onOpen: (id: TabId) => void
}): ReactNode {
  const platformOf = (machine: string | null | undefined) => fleet?.machines.find((m) => m.name === machine)?.platform
  const rows = [...boxes].sort((a, b) => Number(sandboxRuns(b)) - Number(sandboxRuns(a)) || a.name.localeCompare(b.name))
  return (
    <>
      <ListHead title={t('ptSandboxes')} count={boxes.length} other={other == null ? t('ptSessions') : t('ptShowSessions', { n: other })} onSwitch={onSwitch} />
      {rows.length === 0
        ? <p className={css.empty}>{t('ptNoSandboxes')}</p>
        : (
          <ul className={css.lines}>
            {rows.map((b) => {
              const runs = sandboxRuns(b)
              return (
                <li key={`${b.machine ?? ''}/${b.name}`}>
                  <button type="button" className={clsx(css.line, css.sandboxLine)} onClick={() => onOpen(`sandbox:${b.machine ?? ''}/${b.name}`)} title={b.name}>
                    <span className={css.lineIcon} aria-hidden="true"><SandboxGlyph size={13} /></span>
                    <span className={clsx(css.ellipsis, css.mono)}>{shortSandbox(b.name)}</span>
                    <span className={css.machine}>
                      <span className={css.lineIcon} aria-hidden="true">{b.machine === 'native' ? <HomeGlyph size={12} /> : <MachineGlyph platform={platformOf(b.machine) ?? 'unknown'} size={12} />}</span>
                      <span className={css.ellipsis}>{place(b.machine)}</span>
                    </span>
                    <span className={css.lineState} data-tone={runs ? 'success' : undefined}>{runs ? t('ptSandboxRunning') : t('ptSandboxStopped')}</span>
                  </button>
                </li>
              )
            })}
          </ul>
        )}
    </>
  )
}

function ChatGlyph(): ReactNode {
  return (
    <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinejoin="round" aria-hidden="true">
      <path d="M2.5 3.5h11v7h-6l-3 2.5v-2.5h-2z" />
    </svg>
  )
}

function CloudGlyph(): ReactNode {
  return (
    <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinejoin="round" aria-hidden="true">
      <path d="M4.5 12.5h7a2.5 2.5 0 0 0 .3-5 3.8 3.8 0 0 0-7.3-.9 2.9 2.9 0 0 0 0 5.9z" />
    </svg>
  )
}
