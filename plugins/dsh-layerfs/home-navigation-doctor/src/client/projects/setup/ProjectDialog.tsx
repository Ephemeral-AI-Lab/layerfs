/**
 * The project dialog (docs/specs/layerfs-workspaces.md §2-§4): one modal that
 * creates or imports a project in three steps (Project, Store, Runs on, on a
 * left rail), configures where one runs, and picks a project to start a
 * session in. Both kinds run on one machine: a SQLite Store's file is placed
 * there (push / pull / move); a PostgreSQL Store is on no machine, so its
 * machine is only where its sessions run. Docker is the default.
 */
import { useEffect, useId, useMemo, useRef, useState, type ReactNode } from 'react'
import clsx from 'clsx'
import {
  Button, IconCheckOutlineRegular, IconChevronLeftOutlineRegular, IconCloseOutlineRegular, IconFolderOpenOutlineRegular,
  IconPlusOutlineRegular, IconSearchOutlineRegular, IconSettingsOutlineRegular, Input, Modal, SegmentedControl, StateDot,
} from '@deepseek-ai/dsh-client-ui-primitives'
import type { LayerfsApi } from '../../api.ts'
import { useQuery } from '../../files/use-query.ts'
import { MachineGlyph, StoreKindGlyph } from '../../machines/icons.tsx'
import { LayerfsGlyph } from '../../overview/glyph.tsx'
import { messageOf, NAME, type ProjectInitJob, type ProjectRow } from '../../picker/NewProjectForm.tsx'
import { slugify } from '../../picker/location.ts'
import { useHostEvents } from '../../storage/hooks.ts'
import { PG_URL_LOGIN } from '../../storage/pg-fields.tsx'
import { Signal } from '../../storage/Signal.tsx'
import type { PgAuth, StorageView, StoreRow } from '../../storage/types.ts'
import { refusalOf, usePressGuard } from '../../ui/index.tsx'
import { projectDialog, type DialogRequest, type ProjectRef } from '../dialog.ts'
import type { ListT } from '../list-locale.ts'
import { openStorage } from '../navigation.ts'
import { useProjectData, useRecent, type ProjectItem } from '../ProjectList.tsx'
import { sidebarStatus } from '../readiness.ts'
import { sidebarHidden } from '../recent.ts'
import { setupStateWord, type SetupT } from './locale.ts'
import type { MachinePlatformInfo } from '../../machines/model.ts'
import { createSteps, defaultMachine, machinesFor, needsStart, pgLoginProblem, pickable, runsOn, stoppedOf, tileBlocks, type CreatePlan, type PgLogin, type PgLoginProblem, type Step, type StoreKind } from './model.ts'
import { pgDefaultMachine, pgPlaceable } from '../../storage/model.ts'
import { LocationPicker, placeName, StartFirst, Tick, type LocationWords } from './MachineTiles.tsx'
import { placeStore, runStep, startMachine } from './run.ts'
import css from './dialog.module.css'

const PG_URL = /^postgres(?:ql)?:\/\//i
const enc = encodeURIComponent

/** `/api/storage`, refreshed on storage and machine events. */
export function useStorage(api: LayerfsApi) {
  const read = useQuery(() => api.call<StorageView>('GET', '/api/storage'), [api], { cache: 'GET /api/storage' })
  useHostEvents(api, ['storage', 'storage-job', 'machine-progress', 'connection', 'layerfs-availability'], read.reload)
  return read
}

/**
 * `GET /api/platforms`: whether each platform can run here now, so a machine
 * whose platform is not ready (Docker not answering) shows blocked. Refused
 * or absent (an older host): nothing is blocked, as before.
 */
export function usePlatforms(api: LayerfsApi) {
  const read = useQuery(() => api.call<{ host: string, platforms: MachinePlatformInfo[] }>('GET', '/api/platforms').then((v) => v.platforms, () => undefined), [api], { cache: 'GET /api/platforms' })
  useHostEvents(api, ['machine-progress', 'connection', 'layerfs-availability'], read.reload, { coalesceMs: 1000 })
  return read.data
}

/** Render one dialog request (set as the dialog renderer by the projects area). */
export function ProjectDialog({ api, t, tl, request }: { api: LayerfsApi, t: SetupT, tl: ListT, request: DialogRequest }): ReactNode {
  const [busy, setBusy] = useState(false)
  const close = () => { if (!busy) projectDialog.close() }
  const back = request.mode !== 'pick' ? request.back : undefined
  const body = request.mode === 'configure'
    ? <Configure api={api} t={t} project={request.project} back={back} onBusy={setBusy} />
    : request.mode === 'pick'
      ? <Pick api={api} t={t} tl={tl} />
      : <Create api={api} t={t} startFromFolder={request.mode === 'import'} back={back} onBusy={setBusy} />
  const title = request.mode === 'configure' ? request.project.name ?? request.project.key.split('/')[1] ?? request.project.key
    : request.mode === 'pick' ? t('pickTitle') : request.mode === 'import' ? t('importTitle') : t('newTitle')
  return <Modal open headless onClose={close} title={title} className={css.dialog}>
    {body}
  </Modal>
}

/** The dialog's head: Back (when opened from the picker), the LayerFS mark, the title, a line under it, and Close. */
function Head({ title, sub, t, back }: { title: string, sub: ReactNode, t: SetupT, back?: DialogRequest | undefined }): ReactNode {
  return <header className={css.head}>
    {back
      ? <button type="button" className={css.back} onClick={() => projectDialog.open(back)}>
        <IconChevronLeftOutlineRegular size={14} /><span>{t('backToProjects')}</span>
      </button>
      : null}
    <div className={css.headRow}>
      <span className={css.headTile} aria-hidden="true"><LayerfsGlyph size={20} /></span>
      <span className={css.headText}>
        <h2 className={css.title}>{title}</h2>
        <span className={css.sub}>{sub}</span>
      </span>
      <Button variant="ghost" size="sm" className={css.close} icon={<IconCloseOutlineRegular size={16} />} aria-label={t('close')} title={t('close')}
        onClick={() => projectDialog.close()} />
    </div>
  </header>
}

function Loading({ read, t }: { read: { error: unknown, reload: () => void }, t: SetupT }): ReactNode {
  return read.error
    ? <div className={css.notice} role="alert">{t('loadFailed', { message: messageOf(read.error) })} <Button variant="ghost" size="sm" onClick={read.reload}>{t('retry')}</Button></div>
    : <div className={css.notice}>{t('loading')}</div>
}

/** A labelled control: every field in the dialog shares this label, hint and error. */
function Field({ label, hint, error, children }: { label: string, hint?: string | null | undefined, error?: string | null | undefined, children: ReactNode }): ReactNode {
  return <div className={css.field} data-invalid={error ? '' : undefined}>
    <label className={css.field}><span className={css.label}>{label}</span>{children}</label>
    {error ? <span className={css.error} role="alert">{error}</span> : hint ? <span className={css.hint}>{hint}</span> : null}
  </div>
}

/** A group with the same label style as a field. */
function Group({ label, hint, children }: { label: string, hint?: string | undefined, children: ReactNode }): ReactNode {
  return <section className={css.field}>
    <h3 className={css.label}>{label}</h3>
    {children}
    {hint ? <p className={css.hint}>{hint}</p> : null}
  </section>
}

// ---------- pieces ----------

/** One Store kind as a card: its mark, name and what it means. */
function KindCard({ kind, selected, t, onSelect }: { kind: StoreKind, selected: boolean, t: SetupT, onSelect: () => void }): ReactNode {
  return <button type="button" role="radio" aria-checked={selected} className={clsx(css.kind, selected && css.selected)} onClick={onSelect}>
    <span className={css.kindMark}><StoreKindGlyph kind={kind} size={20} /></span>
    <span className={css.kindText}>
      <span className={css.kindName}>{kind === 'sqlite' ? t('kindSqliteTitle') : t('kindPgTitle')}</span>
      <span className={css.kindBlurb}>{kind === 'sqlite' ? t('kindSqliteBlurb') : t('kindPgBlurb')}</span>
    </span>
    <Tick on={selected} />
  </button>
}

/** The tiles' words from this dialog's locale. */
/** The tiles' words; `os` names the native machine (This Mac, unisolated; This computer, sandboxed; This PC). */
const wordsOf = (t: SetupT, os?: string): LocationWords => ({ home: t('layerfsHome'), thisMac: t(os === 'linux' ? 'thisLinux' : os === 'windows' ? 'thisPc' : 'thisMac'), nativeOs: os === 'linux' ? 'linux' : os === 'windows' || os === 'other' ? 'pc' : 'mac', running: t('running'), stopped: t('stopped'), default: t('default'), here: t('here'), blocked: t('machineBlocked'), engineDown: t('engineDown'), state: (s) => setupStateWord(s, t) })

function NewMachineTile({ api, t }: { api: LayerfsApi, t: SetupT }): ReactNode {
  return <button type="button" className={clsx(css.machine, css.newMachine)} onClick={() => { projectDialog.close(); openStorage(api, 'machines:new') }}>
    <IconPlusOutlineRegular size={16} />
    <span>{t('newMachine')}</span>
  </button>
}

type StepState = { step: Step, state: 'pending' | 'running' | 'done' | 'failed' }

/** A path with the user's home folder shown as `~` (`/Users/me/code/shop` -> `~/code/shop`). */
export const shortPath = (path: string): string => path.replace(/\/+$/, '').replace(/^\/(?:Users|home)\/[^/]+(?=\/|$)/, '~')

/** StartFirst's words in this dialog's locale. */
const startWordsOf = (t: SetupT) => ({
  stopped: (machine: string) => t('stoppedFirst', { machine }),
  start: (machine: string) => t('startNamed', { machine }),
  refused: (machine: string) => t('startRefused', { machine }),
})

function stepLabel(step: Step, plan: CreatePlan, t: SetupT): string {
  const store = plan.store?.name ?? plan.newStore?.name ?? ''
  const id = step.id
  if (id === 'check') return t('step_check', { source: shortPath(plan.source ?? '') })
  if (id === 'store') return t('step_store', { store })
  if (id === 'connect') return t('step_connect', { store })
  if (id === 'init') return plan.source ? t('step_import', { source: shortPath(plan.source), name: plan.name }) : t('step_init', { name: plan.name })
  const machine = placeName(step.machine ?? '', wordsOf(t))
  if (id === 'open') return step.machine ? t('step_open_on', { machine }) : t('step_open')
  return t('step_place', { store, machine })
}

/** The running Init's phase and count: `hash · 8,412 / 12,930`. */
function initDetail(job: ProjectInitJob | null): string | null {
  const p = job?.progress
  if (!p?.phase) return null
  const n = (v: number | null) => (v === null ? null : v.toLocaleString())
  return p.total ? `${p.phase} · ${n(p.done ?? 0)} / ${n(p.total)}` : p.done !== null ? `${p.phase} · ${n(p.done)}` : p.phase
}

/** A stopped Create: the part of the form that holds what to change (the rail's three parts). */
const partOfStep = (step: Step): Part => (step.id === 'check' || step.id === 'init' ? 0 : step.id === 'store' || step.id === 'connect' ? 1 : 2)

/** Where a Create stopped: which step, and LayerFS's refusal (or the transport failure) as it came. */
interface Failure { step: Step, error: unknown }

/**
 * A stopped Create as a status (UI rule: dot and word, LayerFS's own words as
 * the tip, one action): Refused (LayerFS answered) or No answer,
 * the step, and Edit for the part that holds what to change.
 */
function Stopped({ failure, plan, t, part, onEdit }: { failure: Failure, plan: CreatePlan, t: SetupT, part: string, onEdit: () => void }): ReactNode {
  const r = refusalOf(failure.error)
  const word = r.transport === 'transport-unknown' || r.transport === 'unavailable' || r.code === 'no-host' ? t('noAnswer') : t('refused')
  const step = stepLabel(failure.step, plan, t)
  return <div className={css.stopped} role="alert" data-refusal={r.code}>
    <Signal state="error" title={word} label={`${word}: ${r.message}`} detail={<>{r.message}{r.fix ? <><br /><code>{r.fix}</code></> : null}</>} />
    <span className={css.stoppedWord}>· {step}</span>
    <Button type="button" variant="outline" size="sm" onClick={onEdit}>{t('editPart', { part })}</Button>
  </div>
}

function Progress({ steps, plan, t, failure, initJob, partTitle, onEdit }: {
  steps: StepState[], plan: CreatePlan, t: SetupT, failure: Failure | null, initJob: ProjectInitJob | null, partTitle: (part: Part) => string, onEdit: (part: Part) => void
}): ReactNode {
  return <div className={css.progress} aria-live="polite">
    <h3 className={css.label}>{failure ? t('createStopped') : t('creating')}</h3>
    <ol className={css.steps}>
      {steps.map(({ step, state }) => <li key={step.id} className={css.step} data-state={state}>
        <span className={css.stepMark} aria-hidden="true">{state === 'running' ? <StateDot state="ongoing" size={10} /> : state === 'done' ? <IconCheckOutlineRegular size={11} /> : state === 'failed' ? <IconCloseOutlineRegular size={11} /> : null}</span>
        <span>{stepLabel(step, plan, t)}</span>
        {step.id === 'init' && state === 'running' && initDetail(initJob) ? <span className={css.stepDetail}>{initDetail(initJob)}</span> : null}
      </li>)}
    </ol>
    {failure ? <Stopped failure={failure} plan={plan} t={t} part={partTitle(partOfStep(failure.step))} onEdit={() => onEdit(partOfStep(failure.step))} /> : null}
  </div>
}

// ---------- create ----------

type Part = 0 | 1 | 2
const PARTS = [0, 1, 2] as const
type StoreMode = 'new' | 'existing'
/** A new PostgreSQL Store's login: a new one saved now, or one saved before. */
type LoginMode = 'new' | 'saved'

/** What a new PostgreSQL connection still needs, for the rail: its URL, else its login's problem. */
function pgMissing(problem: PgLoginProblem | 'url' | null, t: SetupT): string {
  switch (problem) {
    case 'url': return t('needUrl')
    case 'pick': return t('needLogin')
    case 'name': return t('needLoginName')
    case 'nameInvalid': case 'nameTaken': return t('renameLogin')
    case 'user': return t('needUser')
    case 'password': return t('needPassword')
    default: return t('needUrl')
  }
}
/** What the project starts from: nothing, or a copy of a folder on this computer. */
type StartFrom = 'empty' | 'folder'
/** Folder on this Mac, PC or computer, by the native machine's OS (Mac for an older host). */
const fromFolderKey = (os?: string): 'fromFolderPc' | 'fromFolderMac' | 'fromFolderLinux' => (os === 'windows' ? 'fromFolderPc' : os === 'macos' || os === undefined ? 'fromFolderMac' : 'fromFolderLinux')

/**
 * Create: Project, Store and Runs on on one scrolling page; the rail shows
 * whether each is ready and jumps to it. The project starts empty or from a
 * folder (Start from); `startFromFolder` opens it on Folder (Import a folder).
 * Create runs the steps and opens a session.
 */
function Create({ api, t, startFromFolder, back, onBusy }: { api: LayerfsApi, t: SetupT, startFromFolder: boolean, back?: DialogRequest | undefined, onBusy: (busy: boolean) => void }): ReactNode {
  const id = useId()
  const view = useStorage(api)
  const scratch = useQuery(() => api.op<{ scratchKey?: string | null }>('projects.catalog').catch(() => ({ scratchKey: null })), [api])
  const pane = useRef<HTMLFieldSetElement>(null)
  const sections = useRef<(HTMLElement | null)[]>([])
  const [name, setName] = useState('')
  // the name follows the folder until it is typed in
  const [nameTyped, setNameTyped] = useState(false)
  const [from, setFrom] = useState<StartFrom>(startFromFolder ? 'folder' : 'empty')
  const importing = from === 'folder'
  const [source, setSource] = useState('')
  const [initJob, setInitJob] = useState<ProjectInitJob | null>(null)
  const [pickerMissing, setPickerMissing] = useState(false)
  const [kind, setKind] = useState<StoreKind>('sqlite')
  const [mode, setMode] = useState<StoreMode>('new')
  const [picked, setPicked] = useState<string | null>(null)
  const [query, setQuery] = useState('')
  const [url, setUrl] = useState('')
  // a PostgreSQL Store's login: a new one (name, user, password typed once) or one saved before
  const [loginMode, setLoginMode] = useState<LoginMode>('new')
  const [pickedAuth, setPickedAuth] = useState<string | null>(null)
  const [loginName, setLoginName] = useState<string | null>(null)
  const [pgUser, setPgUser] = useState('')
  const [pgPassword, setPgPassword] = useState('')
  const [sqliteMachine, setSqliteMachine] = useState<string | null>(null)
  const [pgChoice, setPgChoice] = useState<string | null>(null)
  const [steps, setSteps] = useState<StepState[] | null>(null)
  const [shown, setShown] = useState<CreatePlan | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [failure, setFailure] = useState<Failure | null>(null)
  const scratchStore = scratch.data?.scratchKey?.split('/')[0]
  // the home's saved logins (store auths, never a password), read only for PostgreSQL
  const authsRead = useQuery(() => (kind === 'postgresql' ? api.call<{ items: PgAuth[] }>('GET', '/api/store-auths').then((r) => (Array.isArray(r?.items) ? r.items : [])) : Promise.resolve([] as PgAuth[])),
    [api, kind], { cache: kind === 'postgresql' ? 'GET /api/store-auths' : 'setup-auths-off' })
  const auths = authsRead.data ?? []
  const authNames = auths.map((a) => a.auth)

  const data = view.data
  const platforms = usePlatforms(api)
  const machines = machinesFor(data, kind)
  // a machine whose platform is not ready here shows blocked; the default is native when docker cannot run now
  const blocks = useMemo(() => tileBlocks(data?.machines ?? [], platforms, data?.computer?.os), [data, platforms])
  // PostgreSQL (V23-11): only machines that run now, are not broken or blocked here and run PostgreSQL Sandboxes are offered; never a stopped one
  const offered = kind === 'postgresql' ? machines.filter((m) => pgPlaceable(m) && !blocks.has(m.name)) : machines
  const fallback = defaultMachine(offered, blocks)
  const stores = (data?.stores ?? []).filter((s) => s.kind === kind && s.name !== scratchStore)
  const existing = mode === 'existing' ? stores.find((s) => s.name === picked) ?? null : null
  const slug = slugify(name)
  const chosenSqlite = sqliteMachine ?? (existing?.machine ?? fallback)
  // PostgreSQL: the first session's own machine (spec v23: nothing Store-wide), by default where the database was reached from
  const pgPicked = pgChoice && offered.some((m) => m.name === pgChoice) ? pgChoice : null
  const chosenPg = pgPicked ?? (existing ? pgDefaultMachine(existing, (data?.machines ?? []).filter((m) => !blocks.has(m.name))) : fallback)
  const chosen = [kind === 'sqlite' ? chosenSqlite : chosenPg]
  const running = steps !== null && !error && !failure
  useEffect(() => { onBusy(running) }, [running])
  const words = wordsOf(t, data?.computer?.os)
  const parts: readonly Part[] = PARTS

  const folderName = (path: string) => path.replace(/\/+$/, '').split('/').pop() ?? ''
  const pickSource = (path: string) => { setSource(path); if (!nameTyped) setName(folderName(path)) }
  // a picker answer counts only if the path was not typed and Start from not changed since it opened
  const edits = useRef(0)
  const typeSource = (path: string) => { edits.current++; pickSource(path) }
  const browse = async () => {
    const at = edits.current
    try { const path = await api.pickDirectory(); if (path && edits.current === at) pickSource(path) }
    catch (e) { if ((e as { code?: string })?.code === 'no-picker') setPickerMissing(true); else setError(messageOf(e)) }
  }
  useEffect(() => { if (startFromFolder) void browse() }, [])
  const chooseFrom = (next: StartFrom) => {
    edits.current++
    setFrom(next)
    if (next === 'folder' && !source.trim() && !pickerMissing) void browse()
    if (next === 'empty' && !nameTyped) setName('')
  }

  const storeTaken = mode === 'new' && data?.stores.some((store) => store.name === slug)
  const nameError = name.trim() && !NAME.test(slug) ? t('nameInvalid') : storeTaken ? t('storeTaken', { store: slug }) : null
  const folderError = importing && source.trim() && !source.trim().startsWith('/') ? t('folderInvalid') : null
  const urlError = kind === 'postgresql' && mode === 'new' && url.trim() ? (!PG_URL.test(url.trim()) || PG_URL_LOGIN.test(url.trim()) ? t('urlInvalid') : null) : null
  // a new login is named after the Store unless a saved login has that name; it is never saved over one
  const newLoginName = loginName ?? (authNames.includes(slug) ? '' : slug)
  const login: PgLogin = loginMode === 'saved' ? { auth: pickedAuth ?? '' } : { authName: newLoginName, user: pgUser, password: pgPassword }
  const loginProblem = kind === 'postgresql' && mode === 'new' ? pgLoginProblem(login, authNames) : null
  const loginNameError = loginMode === 'new' && loginProblem === 'nameInvalid' ? t('loginNameInvalid') : loginMode === 'new' && loginProblem === 'nameTaken' ? t('loginNameTaken', { auth: newLoginName.trim() }) : null
  // the machines Create needs that are not running: each is started by its own Start below, never by Create
  const stopped = needsStart({ kind, store: existing, machines: chosen }, data)
  const startWords = startWordsOf(t)
  const ok: Record<Part, boolean> = {
    0: NAME.test(slug) && !nameError && !folderError && (!importing || !!source.trim()),
    1: mode === 'existing' ? !!existing : NAME.test(slug) && (kind === 'sqlite' || (!!url.trim() && !urlError && !loginProblem)),
    2: chosen.length > 0 && chosen.every((m) => offered.some((x) => x.name === m) && pickable(m, blocks)) && stopped.length === 0,
  }
  const valid = !!data && parts.every((i) => ok[i])

  const plan = (): CreatePlan => ({
    name: slug, kind, store: existing,
    ...(existing ? {} : { newStore: { name: slug, ...(kind === 'postgresql' ? { url: url.trim(), login } : {}) } }),
    machines: chosen,
    ...(importing && source.trim() ? { source: source.trim() } : {}),
  })

  const create = async () => {
    if (!valid || running) return
    const p = plan()
    const list = createSteps(p, data).map((step) => ({ step, state: 'pending' as const }))
    setShown(p); setError(null); setFailure(null); setSteps(list); setInitJob(null)
    pane.current?.scrollTo({ top: 0 })
    const mark = (i: number, state: StepState['state']) => setSteps((had) => had?.map((s, j) => (j === i ? { ...s, state } : s)) ?? null)
    const key = `${p.store?.name ?? p.newStore!.name}/${p.name}`
    for (let i = 0; i < list.length; i++) {
      const step = list[i]!.step
      mark(i, 'running')
      try {
        if (step.id === 'open') await api.openProjectSession(key, undefined, undefined, step.machine)
        else await runStep(api, p, step, setInitJob)
        mark(i, 'done')
      } catch (e) {
        mark(i, 'failed')
        setFailure({ step, error: e })
        view.reload()
        // a login saved before an unknown outcome stays: its name reads taken until another is given
        if (step.id === 'connect') authsRead.reload()
        return
      }
    }
    onBusy(false)
    projectDialog.close()
  }

  const chooseKind = (next: StoreKind) => { setKind(next); setMode('new'); setPicked(null); setQuery(''); setPgChoice(null); setSqliteMachine(null) }
  const chooseLoginMode = (next: LoginMode) => { setLoginMode(next); if (next === 'saved' && !authNames.includes(pickedAuth ?? '')) setPickedAuth(authNames[0] ?? null) }
  const chooseMode = (next: StoreMode) => {
    setMode(next); setPgChoice(null); setSqliteMachine(null)
    if (next === 'existing' && !stores.some((s) => s.name === picked)) setPicked(stores[0]?.name ?? null)
  }
  const head = <Head title={importing ? t('importTitle') : t('newTitle')} sub={importing ? t('importSub') : t('newSub')} t={t} back={running ? undefined : back} />
  if (!data) return <>{head}<div className={css.body}><Loading read={view} t={t} /></div></>

  const kindWord = kind === 'sqlite' ? t('kindSqliteTitle') : t('kindPgTitle')
  const summaries: Record<Part, string> = {
    0: ok[0] ? (importing ? t('fromSummary', { name: slug, source: shortPath(source.trim()) }) : slug) : importing && !source.trim() ? t('needFolder') : t('notNamed'),
    1: !ok[1] ? (mode === 'existing' ? t('pickStore') : kind === 'postgresql' && NAME.test(slug) ? pgMissing(url.trim() && !urlError ? loginProblem : 'url', t) : t('needStoreName'))
      : existing ? `${kindWord} · ${existing.name}` : `${kindWord} · ${t('newWord')} · ${slug}`,
    2: ok[2] ? chosen.map((m) => placeName(m, words)).join(', ') : stopped.length ? t('needsStart', { machine: stopped.join(', ') }) : t('noneChosen'),
  }
  const railTitles: Record<Part, string> = { 0: t('stepProject'), 1: t('store'), 2: t('stepMachine') }
  const titles: Record<Part, string> = { 0: t('stepProject'), 1: t('store'), 2: kind === 'sqlite' ? t('stepMachine') : t('runsOn') }
  const q = query.trim().toLowerCase()
  const listed = stores.filter((s) => !q || s.name.toLowerCase().includes(q) || (s.postgresql?.target ?? '').toLowerCase().includes(q))
  const moving = kind === 'sqlite' && existing && (existing.machine ?? 'native') !== chosenSqlite
  const jump = (i: Part) => {
    const root = pane.current, target = sections.current[i]
    if (!root || !target) return
    root.scrollTo({ top: Math.max(0, target.offsetTop - 12), behavior: 'smooth' })
  }
  const firstMissing = parts.find((i) => !ok[i])

  return <form className={css.form} aria-busy={running}
    onSubmit={(e) => { e.preventDefault(); if (firstMissing !== undefined) jump(firstMissing); else void create() }}>
    {head}
    <div className={css.wizard}>
      <fieldset ref={pane} className={css.pane} disabled={running}>
        {steps && shown ? <Progress steps={steps} plan={shown} t={t} failure={failure} initJob={initJob} partTitle={(i) => railTitles[i]} onEdit={jump} /> : null}
        <section ref={(el) => { sections.current[0] = el }} className={css.part} aria-labelledby={`${id}-p0`}>
          <h3 id={`${id}-p0`} className={css.partTitle}>{titles[0]}</h3>
          <div className={css.fields}>
          <Group label={t('startFrom')}>
            <SegmentedControl<StartFrom> id={`${id}-from`} className={css.modes} value={from} label={t('startFrom')} onChange={chooseFrom}
              options={[
                { value: 'empty', label: t('fromEmpty') },
                { value: 'folder', label: t(fromFolderKey(data.computer?.os)) },
              ]} />
          </Group>
          {importing ? <Field label={t('folder')} hint={t('folderHint')} error={folderError}>
            <span className={css.inline}>
              <Input value={source} className={css.mono} placeholder="/Users/me/code/shop" autoComplete="off" spellCheck={false} onChange={(e) => typeSource(e.target.value)} />
              {pickerMissing ? null : <Button type="button" variant="outline" size="sm" icon={<IconFolderOpenOutlineRegular size={14} />} onClick={() => void browse()}>{t('browse')}</Button>}
            </span>
          </Field> : null}
          <Field label={t('name')} error={nameError} hint={!nameError && slug && slug !== name.trim() ? t('savedAs', { slug }) : importing && !nameTyped ? t('nameFollows') : t('nameHint')}>
            <Input value={name} placeholder={t('namePlaceholder')} autoComplete="off" spellCheck={false} data-modal-autofocus="" onChange={(e) => { setName(e.target.value); setNameTyped(e.target.value.trim() !== '') }} />
          </Field>
          </div>
        </section>

        <section ref={(el) => { sections.current[1] = el }} className={css.part} aria-labelledby={`${id}-p1`}>
          <h3 id={`${id}-p1`} className={css.partTitle}>{titles[1]}</h3>
          <div className={css.fields}>
          <Group label={t('storeKind')}>
            <div className={css.kinds} role="radiogroup" aria-label={t('storeKind')}>
              {(['sqlite', 'postgresql'] as const).map((k) => <KindCard key={k} kind={k} t={t} selected={kind === k} onSelect={() => chooseKind(k)} />)}
            </div>
          </Group>
          <Group label={t('store')}>
            <SegmentedControl<StoreMode> id={`${id}-mode`} className={css.modes} value={mode} label={t('store')} onChange={chooseMode}
              options={[
                { value: 'new', label: kind === 'sqlite' ? t('newStore') : t('newConnection') },
                { value: 'existing', label: t('existingCount', { n: stores.length }) },
              ]} />
          </Group>
          {mode === 'new' ? <>
            {kind === 'postgresql' ? <>
              <Field label={t('url')} hint={t('urlHint')} error={urlError}>
                <Input value={url} className={css.mono} placeholder="postgres://db.lan:5432/app" autoComplete="off" spellCheck={false} onChange={(e) => setUrl(e.target.value)} />
              </Field>
              <Group label={t('login')} hint={loginMode === 'saved' && auths.length ? t('loginSavedHint') : undefined}>
                <SegmentedControl<LoginMode> id={`${id}-login`} className={css.modes} value={loginMode} label={t('login')} onChange={chooseLoginMode}
                  options={[
                    { value: 'new', label: t('loginNew') },
                    { value: 'saved', label: t('loginSaved', { n: auths.length }) },
                  ]} />
              </Group>
              {loginMode === 'saved'
                ? auths.length === 0
                  ? <p className={css.empty}>{t('noLogins')}</p>
                  : <div className={css.storeList}>
                    <div role="radiogroup" aria-label={t('login')}>
                      {auths.map((a) => <button key={a.auth} type="button" role="radio" aria-checked={pickedAuth === a.auth} data-login={a.auth}
                        className={clsx(css.storeRow, pickedAuth === a.auth && css.storeRowOn)} onClick={() => setPickedAuth(a.auth)}>
                        <span className={css.storeName}>{a.auth}</span>
                        <span className={css.storeNote}>
                          {[a.user, a.stores.length === 1 ? t('loginStoresOne') : a.stores.length ? t('loginStores', { n: a.stores.length }) : t('loginUnused')].filter(Boolean).join(' · ')}
                        </span>
                        {pickedAuth === a.auth ? <IconCheckOutlineRegular size={14} className={css.storeCheck} /> : null}
                      </button>)}
                    </div>
                  </div>
                : <>
                  <Field label={t('loginName')} hint={t('loginNameHint')} error={loginNameError}>
                    <Input value={newLoginName} className={css.mono} autoComplete="off" spellCheck={false} onChange={(e) => setLoginName(e.target.value)} />
                  </Field>
                  <Field label={t('user')}>
                    <Input value={pgUser} className={css.mono} placeholder="app" autoComplete="off" spellCheck={false} onChange={(e) => setPgUser(e.target.value)} />
                  </Field>
                  <Field label={t('password')} hint={t('passwordHint')}>
                    <Input value={pgPassword} type="password" autoComplete="new-password" onChange={(e) => setPgPassword(e.target.value)} />
                  </Field>
                </>}
            </> : null}
          </> : stores.length === 0
            ? <p className={css.empty}>{kind === 'sqlite' ? t('noSqlite') : t('noPg')}</p>
            : <div className={css.storeList}>
              <div className={css.storeSearch}>
                <IconSearchOutlineRegular size={14} className={css.searchIcon} />
                <input className={css.search} value={query} placeholder={t('searchStores', { n: stores.length })} aria-label={t('searchStores', { n: stores.length })}
                  onChange={(e) => setQuery(e.target.value)} />
              </div>
              <div role="radiogroup" aria-label={t('store')}>
                {listed.map((s) => <button key={s.name} type="button" role="radio" aria-checked={existing?.name === s.name}
                  className={clsx(css.storeRow, existing?.name === s.name && css.storeRowOn)}
                  onClick={() => { setPicked(s.name); setPgChoice(null); setSqliteMachine(null) }}>
                  <span className={css.storeName}>{s.name}</span>
                  <span className={css.storeNote}>
                    {kind === 'sqlite' ? placeName(s.machine ?? 'native', wordsOf(t)) : s.postgresql?.target ?? ''}
                    {' · '}{s.projects.length === 1 ? t('oneProject') : t('projects', { n: s.projects.length })}
                  </span>
                  {existing?.name === s.name ? <IconCheckOutlineRegular size={14} className={css.storeCheck} /> : null}
                </button>)}
                {listed.length === 0 ? <p className={css.empty}>{t('noStoreMatch')}</p> : null}
              </div>
            </div>}
          </div>
        </section>

        <section ref={(el) => { sections.current[2] = el }} className={css.part} aria-labelledby={`${id}-p2`}>
          <h3 id={`${id}-p2`} className={css.partTitle}>{titles[2]}</h3>
          <div className={css.fields}>
          {kind === 'sqlite'
            ? <Group label={t('machines')} hint={t('locationHint')}>
              <LocationPicker machines={machines} value={chosenSqlite} onChange={setSqliteMachine} words={words} label={t('stepMachine')}
                current={existing ? existing.machine ?? 'native' : null} fallback={fallback} blocks={blocks}>
                <NewMachineTile api={api} t={t} />
              </LocationPicker>
            </Group>
            : <Group label={t('machines')} hint={t('runsOnPg')}>
              <LocationPicker machines={offered} value={chosenPg} onChange={setPgChoice} words={words} label={t('runsOn')}
                current={null} fallback={fallback} blocks={blocks}>
                <NewMachineTile api={api} t={t} />
              </LocationPicker>
            </Group>}
          {offered.length === 0 ? <p className={css.notice}>{t('noMachines')}</p> : null}
          {/* Create starts no machine: each stopped one it needs has its own Start here */}
          <StartFirst names={stopped} words={startWords} start={(m) => startMachine(api, m)} onStarted={view.reload} />
          {moving ? <p className={css.warn}>{t('moveWarn', { store: existing!.name, machine: placeName(chosenSqlite, words), n: Math.max(0, existing!.projects.length) })}</p> : null}
          </div>
        </section>
      </fieldset>
    </div>
    <footer className={css.foot}>
      <span className={css.footNote}>{firstMissing !== undefined && !running ? t('missing', { part: railTitles[firstMissing], what: summaries[firstMissing] }) : null}</span>
      <div className={css.footActions}>
        <Button type="button" variant="outline" disabled={running} onClick={() => projectDialog.close()}>{t('cancel')}</Button>
        <Button type="submit" variant="primary" disabled={running || !data} aria-busy={running} icon={running ? <StateDot state="ongoing" size={13} /> : undefined}>
          {running ? t('creating') : importing ? t('import') : t('create')}
        </Button>
      </div>
    </footer>
  </form>
}

// ---------- configure ----------

/** Configure: the Store, where it runs (a SQLite Store moves; a PostgreSQL Store's sessions run there), and Hide from sidebar. */
function Configure({ api, t, project, back, onBusy }: { api: LayerfsApi, t: SetupT, project: ProjectRef, back?: DialogRequest | undefined, onBusy: (busy: boolean) => void }): ReactNode {
  const view = useStorage(api)
  const platforms = usePlatforms(api)
  const [target, setTarget] = useState<string | null>(null)
  const [busy, setBusyState] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [hidden, setHidden] = useState(false)
  const name = project.name ?? project.key.split('/')[1] ?? project.key
  const storeName = project.store ?? project.key.split('/')[0] ?? ''
  const store: StoreRow | undefined = view.data?.stores.find((s) => s.name === storeName)
  const setBusy = (value: string | null) => { setBusyState(value); onBusy(!!value) }
  const run = async (label: string, work: () => Promise<unknown>) => {
    setBusy(label); setError(null)
    try { await work(); setTarget(null) } catch (e) { setError(messageOf(e)) } finally { setBusy(null); view.reload() }
  }
  const head = <Head title={name} sub={<span className={css.mono}>{project.key}</span>} t={t} back={busy ? undefined : back} />
  const hide = hidden ? <span className={css.footNote}>{t('hidden')}</span>
    : <Button variant="ghost" size="sm" disabled={!!busy} onClick={() => { sidebarHidden.hide(project.projectId ?? project.key); setHidden(true) }}>{t('hide')}</Button>
  if (!view.data) return <>{head}<div className={css.body}><Loading read={view} t={t} /></div></>
  if (!store) {
    return <>{head}
      <div className={css.body}><p className={css.notice}>{t('storeMissing')}</p></div>
      <footer className={css.foot}>{hide}<div className={css.footActions}>
        <Button variant="outline" onClick={() => { projectDialog.close(); openStorage(api) }}>{t('openResources')}</Button>
        <Button variant="primary" onClick={() => projectDialog.close()}>{t('done')}</Button>
      </div></footer>
    </>
  }
  const others = Math.max(0, store.projects.length - 1)
  const pg = store.kind === 'postgresql'
  const machines = machinesFor(view.data, pg ? 'postgresql' : 'sqlite')
  const blocks = tileBlocks(view.data.machines, platforms, view.data.computer?.os)
  const startWords = startWordsOf(t)
  const kindWords = pg ? t('kindPg') : t('kindSqlite')
  const storeCard = <Group label={t('store')}>
    <div className={css.storeCard}>
      <span className={css.kindMark}><StoreKindGlyph kind={store.kind} size={20} /></span>
      <span className={css.kindText}>
        <span className={css.kindName}>{store.name}</span>
        <span className={css.kindBlurb}>{[kindWords, pg ? store.postgresql?.target : null, others === 1 ? t('sharedOne') : others ? t('shared', { n: others }) : t('alone')].filter(Boolean).join(' · ')}</span>
      </span>
    </div>
  </Group>

  const words = wordsOf(t, view.data?.computer?.os)
  if (!pg) {
    const current = store.machine ?? 'native'
    const chosen = target ?? current
    return <>{head}
      <div className={css.body}><div className={css.fields}>
        {storeCard}
        <Group label={t('stepMachine')} hint={t('locationHint')}>
          <LocationPicker machines={machines} value={chosen} onChange={setTarget} words={words} label={t('stepMachine')} current={current} disabled={!!busy} blocks={blocks}>
            <NewMachineTile api={api} t={t} />
          </LocationPicker>
        </Group>
        {chosen !== current ? <p className={css.warn}>{t('moveWarn', { store: store.name, machine: placeName(chosen, words), n: others })}</p> : null}
        {/* moving starts no machine: each stopped one it needs has its own Start here */}
        {chosen !== current ? <StartFirst names={stoppedOf([current, chosen], view.data)} words={startWords} start={(m) => startMachine(api, m)} onStarted={view.reload} /> : null}
        {error ? <p className={css.error} role="alert">{error}</p> : null}
      </div></div>
      <footer className={css.foot}>
        {hide}
        <div className={css.footActions}>
          {chosen !== current
            ? <>
              <Button variant="outline" disabled={!!busy} onClick={() => setTarget(null)}>{t('cancel')}</Button>
              <Button variant="primary" disabled={!!busy || stoppedOf([current, chosen], view.data).length > 0} icon={busy ? <StateDot state="ongoing" size={13} /> : undefined}
                onClick={() => void run('move', () => placeStore(api, store.name, chosen))}>
                {busy ? t('moving') : t('moveTo', { machine: placeName(chosen, words) })}
              </Button>
            </>
            : <Button variant="primary" onClick={() => projectDialog.close()}>{t('done')}</Button>}
        </div>
      </footer>
    </>
  }
  // a PostgreSQL Store has no machine of its own: each session picks one when it starts (spec v23)
  return <>{head}
    <div className={css.body}><div className={css.fields}>
      {storeCard}
      <Group label={t('runsOn')} hint={t('pgPlacedPerSession')}>
        <p className={css.notice}>{runsOn(store).length ? t('pgConnectedFrom', { machines: runsOn(store).map((m) => placeName(m, words)).join(', ') }) : t('pgNotConnectedYet')}</p>
      </Group>
      {error ? <p className={css.error} role="alert">{error}</p> : null}
    </div></div>
    <footer className={css.foot}>
      {hide}
      <div className={css.footActions}>
        <Button variant="primary" disabled={!!busy} onClick={() => projectDialog.close()}>{t('done')}</Button>
      </div>
    </footer>
  </>
}

// ---------- pick ----------

const nameOf = (p: ProjectRow) => p.name ?? p.key.split('/')[1] ?? p.key
const storeOf = (p: ProjectRow) => p.store ?? p.key.split('/')[0] ?? ''
const idOf = (p: ProjectRow) => p.projectId ?? p.key
const PICK: DialogRequest = { mode: 'pick' }

/** Pick: every project, searchable; a row starts a session, ⚙ sets it up. */
function Pick({ api, t, tl }: { api: LayerfsApi, t: SetupT, tl: ListT }): ReactNode {
  const data = useProjectData(api)
  const recent = useRecent(api)
  const [query, setQuery] = useState('')
  const counts = useMemo(() => {
    const out = new Map<string, number>()
    for (const r of recent.data?.projects ?? []) for (const k of [r.projectId, r.project]) if (k) out.set(k, (out.get(k) ?? 0) + r.sessionIds.length)
    return out
  }, [recent.data])
  const q = query.trim().toLowerCase()
  const rows = (data.catalog.data?.rows ?? []).filter((p) => p.key !== data.scratchKey)
  const items = rows.filter((p) => !q || nameOf(p).toLowerCase().includes(q) || storeOf(p).toLowerCase().includes(q)).map(data.item)
  const sessionsOf = (p: ProjectRow) => counts.get(p.projectId ?? '') ?? counts.get(p.key) ?? 0
  // Projects that can run first, most used first, then by name.
  const rank = (i: ProjectItem) => (i.availability.canRun || i.availability.canStart ? 0 : i.row.catalogState === 'removed' || i.row.catalogState === 'identity-mismatch' ? 2 : 1)
  items.sort((a, b) => rank(a) - rank(b) || sessionsOf(b.row) - sessionsOf(a.row) || nameOf(a.row).localeCompare(nameOf(b.row)))
  return <>
    <Head title={t('pickTitle')} sub={t('pickSub')} t={t} />
    <div className={css.searchRow}>
      <IconSearchOutlineRegular size={15} className={css.searchIcon} />
      <input className={css.search} value={query} placeholder={t('search')} aria-label={t('search')} data-modal-autofocus="" onChange={(e) => setQuery(e.target.value)} />
    </div>
    <div className={clsx(css.body, css.pickBody)}>
      {data.catalog.error && !data.catalog.data ? <Loading read={{ error: data.catalog.error, reload: data.reload }} t={t} />
        : !data.catalog.data ? <div className={css.notice}>{t('loading')}</div>
          : !items.length ? <div className={css.notice}>{q ? t('noMatch') : t('noProjects')}</div>
            : <ul className={css.picks} aria-label={t('pickTitle')}>
              {items.map((item) => <PickRow key={idOf(item.row)} api={api} t={t} tl={tl} item={item} sessions={sessionsOf(item.row)} onChanged={data.reload} platformOf={data.platformOf} />)}
            </ul>}
    </div>
    <footer className={css.foot}>
      <div className={css.footActions}>
        <Button variant="ghost" icon={<IconFolderOpenOutlineRegular size={14} />} onClick={() => projectDialog.open({ mode: 'import', back: PICK })}>{t('importFolder')}</Button>
      </div>
      <div className={css.footActions}>
        <Button variant="primary" icon={<IconPlusOutlineRegular size={14} />} onClick={() => projectDialog.open({ mode: 'new', back: PICK })}>{t('newProject')}</Button>
      </div>
    </footer>
  </>
}

function PickRow({ api, t, tl, item, sessions, onChanged, platformOf }: { api: LayerfsApi, t: SetupT, tl: ListT, item: ProjectItem, sessions: number, onChanged: () => void, platformOf: (machine: string) => string }): ReactNode {
  const { row, availability } = item
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const name = nameOf(row)
  const status = sidebarStatus(row, availability)
  const startGuard = usePressGuard(`start:${availability.machine ?? ''}:${availability.state}`)
  const configure = () => projectDialog.open({ mode: 'configure', project: { key: row.key, projectId: row.projectId, name, store: storeOf(row) }, back: PICK })
  const go = async (work: () => Promise<unknown>, message: (m: string) => string, close: boolean) => {
    setBusy(true); setError(null)
    try { await work(); if (close) projectDialog.close(); else onChanged() } catch (e) { setError(message(messageOf(e))) } finally { setBusy(false) }
  }
  const open = () => { if (availability.canRun && !busy) void go(() => api.openProjectSession(row.key, undefined, row.projectId), (m) => t('openFailed', { message: m }), true) }
  return <li className={css.pick} data-layerfs-project={idOf(row)}>
    <button type="button" className={css.pickMain} disabled={!availability.canRun || busy} onClick={open}
      aria-label={t('newSession', { project: `${name} · ${storeOf(row)}` })} title={`${name} · ${storeOf(row)}`}>
      <span className={css.pickMark}><LayerfsGlyph size={18} /></span>
      <span className={css.pickText}>
        <span className={css.pickName}>{name}</span>
        <span className={css.pickMeta}>
          <span>{storeOf(row)}</span>
          {item.runsOn.length ? <span className={css.pickSep} aria-hidden="true">·</span> : null}
          {item.runsOn.map((m) => <span key={m} className={css.pickMachine}><MachineGlyph platform={platformOf(m)} size={11} mono />{m}</span>)}
        </span>
      </span>
      {!availability.canRun && availability.state !== 'checking'
        ? <span className={css.why}>{tl(status.key)}</span>
        : <span className={css.count}>{sessions === 1 ? t('oneSession') : t('sessions', { n: sessions })}</span>}
    </button>
    {availability.canStart
      ? <Button variant="outline" size="sm" disabled={busy || startGuard.settling} aria-disabled={startGuard.settling || undefined} aria-label={t('startFor', { machine: availability.machine ?? '', project: name })}
        onClick={startGuard(() => void go(() => api.call('POST', `/api/machines/${enc(availability.machine!)}/start`), (m) => t('startFailed', { machine: availability.machine ?? '', message: m }), false))}>{t('startNamed', { machine: availability.machine ?? '' })}</Button>
      : null}
    <Button variant="ghost" size="sm" className={css.pickGear} icon={<IconSettingsOutlineRegular size={15} />} aria-label={t('configure', { project: name })} title={t('configure', { project: name })} onClick={configure} />
    {error ? <p className={css.pickError} role="alert">{error}</p> : null}
  </li>
}
