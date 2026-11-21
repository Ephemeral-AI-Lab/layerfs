/**
 * The Storage page: one page at a time under a navigation bar (`Nav.tsx`).
 * The Overview is home; every machine, Sandbox, Store, project, bucket or
 * backup file it links to opens in its place, Back and Forward walk the pages
 * visited, the breadcrumb leads back up (and sideways through its switchers),
 * and Jump to opens anything by name. There is no sidebar: what the tree did
 * lives on the pages (Store menus push, pull and move; machine cards power and
 * add Sandboxes) and in the bar (running transfers, Refresh, Add).
 * The view is read again on the host's `storage` events, after each action
 * and on a poll; jobs move on `storage-job` events. Doctor and Logs open from
 * the bar (and Jump to). Settings opens the setup modal. Machines and
 * Sandboxes come from the Machines controller, polled while the page shows.
 * A link from another page (`openStorage`) opens its page here.
 */
import { useEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import { Button } from '@deepseek-ai/dsh-client-ui-primitives'
import { enc, type LayerfsApi } from '../api.ts'
import type { DoctorT } from '../doctor/locale.ts'
import { useStartDoctor } from '../doctor/Doctor.tsx'
import { useQuery } from '../files/use-query.ts'
import { HealthView } from '../health/index.tsx'
import { LogsPage, type LogsT } from '../logs/index.tsx'
import { AddMachineDialog } from '../machines/AddMachineDialog.tsx'
import { MachinesProvider, useMachinesContext, type MachinesPageProps, type Translate as MachinesT } from '../machines/context.tsx'
import { MachineDetail } from '../machines/MachineDetail.tsx'
import { NewSandboxDialog } from '../machines/Sandboxes.tsx'
import { shortSandbox, VM_KINDS, type MachineInfo } from '../machines/model.ts'
import { SandboxDetail } from '../machines/SandboxDetail.tsx'
import { PresetsDialog, type PresetsT } from '../presets/index.tsx'
import { projectDialog } from '../projects/dialog.ts'
import { takeStorageRequest, useStorageRequest } from '../projects/navigation.ts'
import type { SetupModalT } from '../setup/locale.ts'
import { FixContext, Loading, Page, Refusal, type FixRunner } from '../ui/index.tsx'
import { fleetFix, type RunnableFix } from '../../shared/doctor.ts'
import { fixCall } from './fix-call.ts'
import { BackupFilePage, BucketPage, providerWords } from './BucketPage.tsx'
import { StorageDialogs, type StorageDialog } from './dialogs.tsx'
import { SandboxMachinePicker, sandboxMachines } from './SandboxMachinePicker.tsx'
import { HomeOverview } from './HomeOverview.tsx'
import { useHostEvents, useNow, useStorageJobs } from './hooks.ts'
import type { StorageT } from './locale.ts'
import { MachineStores } from './MachineStores.tsx'
import { serverKey } from './machine-stores.ts'
import { bucketFiles, cardJob, connectedMachines, inHome, orderStores, pgPlaceable, type Reconnect } from './model.ts'
import { NavBar, useNavHistory, type AddItem, type Crumb, type JumpItem, type NavPage, type Sibling } from './Nav.tsx'
import { refusalLabels } from './parts.tsx'
import { ProjectPage } from './ProjectPage.tsx'
import type { ProjectMapShare } from './ProjectSandboxes.tsx'
import type { LoginActions, StoreActions } from './StoreCard.tsx'
import { PageReading } from './PaneBar.tsx'
import { StorePage } from './StorePage.tsx'
import { isTabId, underPage, openHomeStores, type OverviewTab, type TabId } from './tabs.ts'
import type { ProjectSummary, StorageJob, StorageView, StoreRow } from './types.ts'
import { useDoctorCount } from '../doctor/use-doctor-count.ts'
import css from './Storage.module.css'

const POLL_MS = 15_000

/** The props the DSH `main` slot renderer gives the Storage page: Storage's `t`, and the Machines controller's face with its `t`. */
export interface StorageMainProps extends Omit<MachinesPageProps, 't'> {
  t: StorageT
  /** The Machines namespace's `t`, for machine and Sandbox pages. */
  tm: MachinesT
  /** What a project's Sandboxes map needs of the conversation views. */
  map: ProjectMapShare
  /** The copy of the pages Center hosts from other areas (Logs, Presets, the setup modal). */
  more: MoreT
}

/** The `t`s of the pages Center hosts from other areas. */
export interface MoreT {
  logs: LogsT
  presets: PresetsT
  setup: SetupModalT
}

export function StoragePage(props: StorageMainProps): ReactNode {
  const { t, tm, api, doctorT, map, more, ...rest } = props
  // Machine pages open their links here; the ref reaches the page's latest `open`.
  const openRef = useRef<(id: TabId) => void>(() => {})
  const machines = useMemo<MachinesPageProps>(() => ({
    ...rest, api, doctorT, logsT: more.logs, t: tm, openTab: (id: string) => { if (isTabId(id)) openRef.current(id) },
  }), [props])
  // The save-login fix opens the page's own dialog; the ref reaches the page's latest `setDialog`.
  const loginRef = useRef<(store: string) => void>(() => {})
  // The connect-again fix (a bucket whose key was refused) opens the Connect dialog the same way.
  const reconnectRef = useRef<(again: Reconnect) => void>(() => {})
  // a fix chain that removes data asks the Store Remove's typed-name confirm (review FAIL-1); StorageIde owns the dialog
  const confirmRef = useRef<(dialog: StorageDialog) => void>(() => {})
  return (
    <MachinesProvider props={machines}>
      <CenterFixes api={api} t={t} loginRef={loginRef} reconnectRef={reconnectRef} confirmRef={confirmRef}>
        <StorageIde api={api} doctorT={doctorT} t={t} openRef={openRef} loginRef={loginRef} reconnectRef={reconnectRef} confirmRef={confirmRef} map={map} more={more} />
      </CenterFixes>
    </MachinesProvider>
  )
}

/**
 * A fix chain's steps for its confirm, numbered in order (review FAIL-1): a step
 * that removes data says so (`Remove Store S and its data`), the rest by their button words.
 */
export function chainSteps(fix: Extract<RunnableFix, { kind: 'chain' }>, label: (fix: RunnableFix) => string, t: (key: 'fixStepRemoveStore' | 'fixStepDelete', vars: Record<string, string>) => string): string[] {
  return fix.steps.map((s, i) => `${i + 1}. ${s.kind === 'removeStore' ? t('fixStepRemoveStore', { name: s.store }) : s.kind === 'delete' ? t('fixStepDelete', { name: s.project }) : label(s)}`)
}

/**
 * Every refusal under Center offers its fix line as its one action
 * ({@link FixContext}). A machine's doctor fix starts that doctor and opens the
 * Doctor page; a PostgreSQL Store's save-login fix opens the save-login dialog
 * for that Store (the person gives the login and its password there).
 */
function CenterFixes({ api, t, loginRef, reconnectRef, confirmRef, children }: { api: LayerfsApi, t: StorageT, loginRef: { current: (store: string) => void }, reconnectRef: { current: (again: Reconnect) => void }, confirmRef: { current: (dialog: StorageDialog) => void }, children: ReactNode }): ReactNode {
  const { recheck, useMachines } = useMachinesContext()
  const fleet = useMachines((snapshot) => (snapshot.data?.available ? snapshot.data.machines : null))
  const startDoctor = useStartDoctor(api)
  const doctorRef = useRef(startDoctor)
  doctorRef.current = startDoctor
  const runner = useMemo<FixRunner>(() => {
    const name = (fix: RunnableFix) => ('machine' in fix ? fix.machine : 'store' in fix ? fix.store : 'project' in fix ? fix.project : 'sandbox' in fix ? fix.sandbox : 'bucket' in fix ? fix.bucket : '')
    const label = (fix: RunnableFix): string => {
      switch (fix.kind) {
        case 'chain': return t('fixRunChain', { n: fix.steps.length, first: label(fix.steps[0]!) })
        case 'start': return t('fixRunStart', { name: fix.machine })
        case 'stop': return t('fixRunStop', { name: fix.machine })
        case 'restart': return t('fixRunRestart', { name: fix.machine })
        case 'prune': return t('fixRunPrune')
        case 'doctor': return t('fixRunDoctor', { name: fix.machine })
        case 'removeMachine': return t('fixRunRemoveRecord', { name: fix.machine })
        case 'compact': return t('fixRunCompact', { name: fix.store })
        case 'removeStore': return t('fixRunRemoveRecord', { name: fix.store })
        case 'removeSandbox': return t('fixRunCloseSandbox', { name: fix.sandbox })
        case 'delete': return t('fixRunDelete', { name: fix.project })
        case 'backup': return t('fixRunBackup', { bucket: fix.bucket })
        case 'homeInit': return t('fixRunHomeInit')
        case 'saveAuth': return t('fixRunSaveLogin')
        case 'updateAuth': return t('fixRunUpdateAuth')
        case 'connectBucket': return t('fixRunConnectBucket')
      }
    }
    return {
      run: async (fix) => {
        if (fix.kind === 'saveAuth') { loginRef.current(fix.store); return }
        if (fix.kind === 'connectBucket') { reconnectRef.current({ bucket: fix.bucket, provider: null, url: fix.url }); return }
        if (fix.kind === 'doctor') return doctorRef.current('machine', fix.machine)
        try { return await fixCall(api, fix) } finally { if ('machine' in fix || (fix.kind === 'chain' && fix.steps.some((s) => 'machine' in s))) void recheck() }
      },
      label: (fix) => label(fix),
      confirm: (fix) => fix.kind === 'chain' ? t('fixConfirmChain', { n: fix.steps.length })
        : t(fix.kind === 'delete' ? 'fixConfirmDelete' : fix.kind === 'removeMachine' || fix.kind === 'removeStore' ? 'fixConfirmRemoveRecord'
          : fix.kind === 'removeSandbox' ? 'fixConfirmCloseSandbox' : 'fixConfirmPrune', { name: name(fix) }),
      step: (i, n, fix) => t('fixChainStep', { i, n, step: label(fix) }),
      confirmChain: (fix, typed) => new Promise<boolean>((resolve) => {
        confirmRef.current({
          kind: 'confirm', title: t('fixChainTitle', { n: fix.steps.length }), body: t('fixChainBody'), action: t('fixChainAction'), name: typed,
          steps: chainSteps(fix, label, t), cancelled: () => resolve(false), run: async () => { resolve(true) },
        })
      }),
      // the accessible name names what it acts on (UI pass 3 F5): `Update (compact-demo-pg)`; words that already name it stay
      named: (fix, words) => {
        const target = fix.kind === 'chain' ? fix.steps.map(name).filter(Boolean).join(', ') : name(fix)
        return target && !words.includes(target) ? t('ariaFor', { action: words, name: target }) : words
      },
      tip: (line) => t('fixTip', { line }),
      done: t('fixDone'),
      // a refusal that says to start, stop or restart a machine whose own record is broken: that machine's fix instead, else none
      resolve: (line) => fleetFix(line, fleet),
    }
  }, [api, t, recheck, loginRef, reconnectRef, confirmRef, fleet])
  return <FixContext.Provider value={runner}>{children}</FixContext.Provider>
}

function StorageIde({ api, doctorT, t, openRef, loginRef, reconnectRef, confirmRef, map, more }: { api: LayerfsApi, doctorT: DoctorT, t: StorageT, openRef: { current: (id: TabId) => void }, loginRef: { current: (store: string) => void }, reconnectRef: { current: (again: Reconnect) => void }, confirmRef: { current: (dialog: StorageDialog) => void }, map: ProjectMapShare, more: MoreT }): ReactNode {
  const mc = useMachinesContext()

  const fleetRead = mc.useMachines((s) => s)
  const fleet = fleetRead.data?.available ? fleetRead.data : null
  const [detailReading, setDetailReading] = useState(false)
  const [pageRefresh, setPageRefresh] = useState(0)
  const [checks, setChecks] = useState(0)
  const freshInventory = useRef(false)
  const freshCatalog = useRef(false)
  const storage = useQuery(() => {
    const path = freshInventory.current ? '/api/storage?fresh=1' : '/api/storage'
    freshInventory.current = false
    return api.call<StorageView>('GET', path)
  }, [api], { cache: 'GET /api/storage' })
  const catalogRead = useQuery(() => {
    const path = freshCatalog.current ? '/api/projects?fresh=1' : '/api/projects'
    freshCatalog.current = false
    return api.call<ProjectSummary[]>('GET', path)
  }, [api], { cache: 'GET /api/projects' })
  const doctor = useDoctorCount(api, checks)
  const { jobs, dismissed, dismiss, take } = useStorageJobs(api, storage.data?.jobs)
  const now = useNow(10_000)
  const [dialog, setDialog] = useState<StorageDialog | null>(null)
  loginRef.current = (store) => setDialog({ kind: 'pg-edit', store })
  confirmRef.current = setDialog
  const reconnect = (again: Reconnect) => setDialog({ kind: 'bucket', again })
  reconnectRef.current = reconnect
  const [failed, setFailed] = useState<unknown>(undefined)
  const [listing, setListing] = useState<string | null>(null)
  const [listingAll, setListingAll] = useState(false)
  const [sandbox, setSandbox] = useState<'pick' | MachineInfo | null>(null)
  const [newMachine, setNewMachine] = useState(false)
  const [presetsOpen, setPresetsOpen] = useState(false)

  // One page at a time: Back and Forward over the pages visited, and the Overview's section.
  const history = useNavHistory(api)
  const active = history.current
  const [section, setSection] = useState<OverviewTab>('projects')

  const reload = storage.reload
  const reloadCatalog = catalogRead.reload
  const inventoryPage = active === 'overview' || /^(store|project|bucket|backup|machine):/.test(active)
  const fleetPage = active === 'overview' || active === 'machines' || /^(machine|sandbox):/.test(active)
  useEffect(() => { if (fleetPage) return mc.activate() }, [mc.activate, fleetPage])
  useEffect(() => { if (!fleetPage && !fleetRead.data && fleetRead.status === 'idle') mc.reload() }, [fleetPage, fleetRead.data, fleetRead.status, mc.reload])
  useHostEvents(api, ['storage', 'storage-changed'], reload, { coalesceMs: 500 })
  useHostEvents(api, ['projects', 'projects-changed'], () => { reload(); reloadCatalog() }, { coalesceMs: 500 })
  useHostEvents(api, ['connection'], (event) => {
    if ((event.data as { connected?: boolean })?.connected) { reload(); reloadCatalog() }
  })
  useEffect(() => {
    const id = setInterval(() => {
      if (document.visibilityState !== 'visible') return
      if (inventoryPage) reload()
      if (active === 'overview') reloadCatalog()
    }, POLL_MS)
    return () => clearInterval(id)
  }, [active, inventoryPage, reload, reloadCatalog])
  const completed = jobs.find((j) => j.ended)?.id
  useEffect(() => { if (completed) reload() }, [completed, reload])
  const view = storage.data
  const catalog = useMemo(() => new Map((catalogRead.data ?? []).map((p) => [p.key, p])), [catalogRead.data])
  const refresh = () => {
    setPageRefresh((n) => n + 1)
    if (inventoryPage) {
      freshInventory.current = true
      reload()
    }
    if (active === 'overview') {
      freshCatalog.current = true
      reloadCatalog()
    }
    if (fleetPage) mc.reload()
  }

  // The machines board is the Overview's Machines tab: `machines` opens that tab.
  const open = (id: TabId) => { if (id === 'machines') { setSection('machines'); history.open('overview') } else { if (id === 'overview') setSection('projects'); history.open(id) } }
  openRef.current = open
  // A link from another page asked for a page: open it once.
  const request = useStorageRequest(api)
  useEffect(() => {
    if (request === null) return
    // `machines:new`: the Overview's Machines tab with Add machine open (the Setup card's New machine…).
    if (request === 'machines:new') { open('machines'); setNewMachine(true) }
    else if (isTabId(request)) open(request)
    takeStorageRequest(api)
  }, [request])
  // A page that went away (a removed Store, bucket or project) leaves the history; the one before it shows.
  const { forget } = history
  const close = (id: TabId) => forget((page) => underPage(page, id))
  const closeStore = (name: string) => close(`store:${name}`)
  const previous = useRef(new Map<string, Set<TabId>>())
  useEffect(() => {
    const removed: TabId[] = []
    const compare = (kind: string, ids: TabId[]) => {
      const next = new Set(ids)
      for (const id of previous.current.get(kind) ?? []) if (!next.has(id)) removed.push(id)
      previous.current.set(kind, next)
    }
    if (view) {
      compare('stores', view.stores.map<TabId>((s) => `store:${s.name}`))
      if (view.bucketInventoryComplete) compare('buckets', view.buckets.map<TabId>((b) => `bucket:${b.name}`))
      for (const b of view.buckets) if (b.files && !b.error) compare(`files:${b.name}`, b.files.map<TabId>((f) => `backup:${b.name}/${f.file}`))
    }
    if (catalogRead.data && !catalogRead.error) compare('projects', catalogRead.data.map<TabId>((p) => `project:${p.key}`))
    if (fleet && !fleetRead.error) {
      compare('machines', fleet.machines.map<TabId>((m) => `machine:${m.name}`))
      if (!fleet.readErrors?.sandboxes) compare('sandboxes', fleet.sandboxes.map<TabId>((s) => `sandbox:${s.machine}/${s.name}`))
    }
    if (removed.length) forget((id) => removed.some((parent) => underPage(id, parent)))
  }, [view, catalogRead.data, catalogRead.error, fleet, fleetRead.error, forget])

  /** An action that answers at once (a job starts): its refusal shows on the page. */
  const act = (work: () => Promise<unknown>) => {
    setFailed(undefined)
    work().then((v) => { if (v && typeof v === 'object' && 'id' in v && 'kind' in v && 'store' in v) take([v as StorageJob]); reload() }, (e: unknown) => setFailed(e))
  }
  const retry = (j: StorageJob): Promise<unknown> => {
    if (j.kind === 'push' || (j.kind === 'move' && j.error?.note === 'pulled')) return api.call('POST', `/api/stores/${enc(j.store)}/push`, { machine: j.target })
    if (j.kind === 'pull') return api.call('POST', `/api/stores/${enc(j.store)}/pull`)
    if (j.kind === 'move') return api.call('POST', `/api/stores/${enc(j.store)}/move`, { machine: j.target })
    if (j.kind === 'backup') return api.call('POST', `/api/stores/${enc(j.store)}/backup`, { bucket: j.target, ...(j.file ? { file: j.file } : {}) })
    // a new run: it settles what the failed one left before it marks again
    if (j.kind === 'compact') return api.call('POST', `/api/stores/${enc(j.store)}/compact`)
    return api.call('POST', `/api/buckets/${enc(j.target)}/restore`, { store: j.store, ...(j.file ? { file: j.file } : {}) })
  }
  const openMachine = (name: string) => open(`machine:${name}`)
  const storeNamed = (name: string) => view?.stores.find((x) => x.name === name)
  const actionsFor = (s: StoreRow): StoreActions => ({
    push: () => setDialog({ kind: 'push', store: s }),
    pull: () => act(() => api.call('POST', `/api/stores/${enc(s.name)}/pull`)),
    move: () => setDialog({ kind: 'move', store: s }),
    // With no bucket yet there is nowhere to back up to: add one first.
    backup: () => setDialog(view && view.buckets.length === 0 ? { kind: 'bucket', then: { kind: 'backup', store: s, known: [] } } : { kind: 'backup', store: s }),
    backupTo: (l) => act(() => api.call('POST', `/api/stores/${enc(s.name)}/backup`, { bucket: l.bucket, file: l.file })),
    restore: (bucket, file) => setDialog({ kind: 'restore', bucket, file, store: s.name }),
    // only a runtime that has store.compact gets the menu row
    ...(view?.compact ? {
      compact: () => setDialog({
        kind: 'confirm', title: t('confirmTitleCompact', { name: s.name }), body: t('confirmCompactBody'), action: t('compactAction'), plain: true,
        run: () => api.call('POST', `/api/stores/${enc(s.name)}/compact`).then((job) => { if (job && typeof job === 'object' && 'id' in job && 'kind' in job) take([job as StorageJob]) }),
      }),
    } : {}),
    remove: () => setDialog({
      kind: 'confirm', title: t('confirmTitleRemove', { name: s.name }), body: t('confirmRemoveBody', { n: s.projects.length }), action: t('removeAction'), name: s.name,
      run: () => api.call('DELETE', `/api/stores/${enc(s.name)}?confirm=${enc(s.name)}`).then(() => closeStore(s.name)),
    }),
    disconnect: () => setDialog({
      kind: 'confirm', title: t('confirmTitlePg', { name: s.name }), body: t('confirmPgBody'), action: t('disconnectAction'),
      run: () => api.call('POST', `/api/stores/${enc(s.name)}/disconnect`).then(() => closeStore(s.name)),
    }),
    start: () => act(() => api.call('POST', `/api/machines/${enc(s.machine ?? '')}/start`)),
    addProject: () => setDialog({ kind: 'add-project', store: s.name }),
    editConnection: (machine) => setDialog({ kind: 'pg-edit', store: s.name, ...(machine ? { machine } : {}) }),
    check: (machine) => setDialog({ kind: 'connect', check: { store: s.name, ...(machine ? { machine } : {}) } }),
    // a machine that runs and is not connected yet, else this computer; the check's From picks another
    connectMachine: () => {
      const connected = new Set(connectedMachines(s))
      const next = (view?.machines ?? []).find((m) => !connected.has(m.name) && pgPlaceable(m))?.name ?? 'native'
      setDialog({ kind: 'connect', check: { store: s.name, machine: next } })
    },
    removeOwn: (machine) => setDialog({
      kind: 'confirm', title: t('pgRemoveOwnTitle', { store: s.name, machine: machine === 'native' ? t('layerfsHome') : machine }),
      body: t('pgRemoveOwnBody', { machine: machine === 'native' ? t('layerfsHome') : machine }), action: t('pgRemoveOwn'),
      run: () => api.call('POST', `/api/stores/${enc(s.name)}/connection`, { machine, clear: true }),
    }),
    newOnServer: () => setDialog({ kind: 'connect', server: serverKey(s) }),
    retry: (j) => { dismiss(j.id); act(() => retry(j)) },
    dismiss: (j) => dismiss(j.id),
    openMachine,
  })
  // a saved PostgreSQL login's own actions (its group's header): a new password; Remove while no Store names it
  const loginActions: LoginActions = {
    rotate: (auth) => setDialog({ kind: 'pg-rotate', auth }),
    remove: (auth) => setDialog({
      kind: 'confirm', title: t('pgRemoveLoginTitle', { auth }), body: t('pgRemoveLoginBody', { auth }), action: t('pgRemoveLogin'), name: auth,
      run: () => api.call('DELETE', `/api/store-auths/${enc(auth)}`),
    }),
  }
  const listBucket = (name: string) => {
    setListing(name)
    api.call('GET', `/api/buckets/${enc(name)}/files`).catch(setFailed).finally(() => { setListing(null); reload() })
  }
  const listAll = () => {
    setListingAll(true)
    api.call('POST', '/api/storage/check', {}).catch(setFailed).finally(() => { setListingAll(false); reload() })
  }

  // ---------- what the bar shows: crumbs, Jump to, Add ----------

  const machineWord = (name: string) => (name === 'native' ? t('groupHome') : name)
  /** Every machine once: the home, Docker, then created machines by name. */
  const places = useMemo(() => {
    const byName = new Map<string, string>()
    for (const m of view?.machines ?? []) byName.set(m.name, m.platform)
    for (const m of fleet?.machines ?? []) byName.set(m.name, m.platform)
    const rank = (name: string, platform: string) => (name === 'native' || platform === 'native' ? 0 : platform === 'docker' ? 1 : 2)
    return [...byName].map(([name, platform]) => ({ name, platform }))
      .sort((a, b) => rank(a.name, a.platform) - rank(b.name, b.platform) || a.name.localeCompare(b.name))
  }, [view?.machines, fleet?.machines])
  const kindWord = (s: StoreRow) => (s.kind === 'postgresql' ? 'PostgreSQL' : 'SQLite')
  /** Where a Store is: its machine (SQLite), or its server (PostgreSQL: on no machine). */
  const storeWhere = (s: StoreRow): string => s.kind === 'postgresql'
    ? s.postgresql?.server ?? t('postgresql')
    : machineWord(inHome(s) ? 'native' : s.machine ?? '')
  const stores = orderStores(view?.stores ?? [])
  const sandboxes = (fleet?.sandboxes ?? []).filter((s) => s.machine)

  const root: Crumb = { key: 'root', label: t('title'), go: () => open('overview') }
  const sectionCrumb = (key: OverviewTab, label: string): Crumb => ({ key: `section:${key}`, label, go: () => { setSection(key); history.open('overview') } })
  // a bucket's way back: Home › Stores on SQLite's Buckets view (once the Backups tab)
  const bucketsCrumb: Crumb = { key: 'section:buckets', label: t('hsViewBuckets'), go: () => { openHomeStores('sqlite', true); setSection('stores'); history.open('overview') } }
  const machineSiblings: Sibling[] = places.map((p) => ({ id: `machine:${p.name}`, label: machineWord(p.name), meta: mc.kind(p.platform) }))
  const storeSiblings: Sibling[] = stores.map((s) => ({ id: `store:${s.name}`, label: s.name, meta: `${kindWord(s)} · ${storeWhere(s)}`, group: s.kind }))
  const bucketSiblings: Sibling[] = (view?.buckets ?? []).map((b) => ({ id: `bucket:${b.name}`, label: b.name, meta: providerWords(b.provider, t) }))
  // Health and Logs: pages under Home, opened from the navigation bar (and Jump to); Settings is the Set up LayerFS modal.
  const pages: NavPage[] = [
    { id: 'health', label: t('navHealth') },
    { id: 'logs', label: t('navLogs') },
  ]
  const crumbsOf = (id: TabId): Crumb[] => {
    if (id === 'overview') return [{ key: 'root', label: t('title') }]
    const extra = pages.find((p) => p.id === id)
    if (extra) return [root, { key: id, label: extra.label }]
    const body = id.slice(id.indexOf(':') + 1)
    const machineCrumb = (name: string, here: boolean): Crumb => ({
      key: `machine:${name}`, label: machineWord(name), id: `machine:${name}`, siblings: machineSiblings, ...(here ? {} : { go: () => openMachine(name) }),
    })
    if (id.startsWith('machine:')) return [root, sectionCrumb('machines', t('ovMachines')), machineCrumb(body, true)]
    if (id.startsWith('sandbox:')) {
      const machine = body.slice(0, body.indexOf('/'))
      const name = body.slice(body.indexOf('/') + 1)
      const beside: Sibling[] = sandboxes.filter((b) => (b.machine ?? '') === machine)
        .map((b) => ({ id: `sandbox:${machine}/${b.name}`, label: shortSandbox(b.name), meta: b.project ? t('jumpBound', { project: b.project }) : undefined }))
      return [root, sectionCrumb('machines', t('ovMachines')), machineCrumb(machine, false), { key: id, label: shortSandbox(name), id, siblings: beside }]
    }
    if (id.startsWith('store:') || id.startsWith('project:')) {
      const storeName = id.startsWith('project:') ? body.slice(0, body.indexOf('/')) : body
      const storeCrumb: Crumb = { key: `store:${storeName}`, label: storeName, id: `store:${storeName}`, siblings: storeSiblings }
      if (id.startsWith('store:')) return [root, sectionCrumb('stores', t('ovStores')), storeCrumb]
      const project = body.slice(body.indexOf('/') + 1)
      const beside: Sibling[] = (storeNamed(storeName)?.projects ?? []).map((p) => ({ id: `project:${storeName}/${p}`, label: p, meta: storeName }))
      return [root, sectionCrumb('stores', t('ovStores')), { ...storeCrumb, go: () => open(`store:${storeName}`) }, { key: id, label: project, id, siblings: beside }]
    }
    const bucket = id.startsWith('backup:') ? body.slice(0, body.indexOf('/')) : body
    const bucketCrumb: Crumb = { key: `bucket:${bucket}`, label: bucket, id: `bucket:${bucket}`, siblings: bucketSiblings }
    if (id.startsWith('bucket:')) return [root, bucketsCrumb, bucketCrumb]
    const file = body.slice(body.indexOf('/') + 1)
    const b = view?.buckets.find((x) => x.name === bucket)
    const files = b ? [...new Set(bucketFiles(b, view?.stores ?? []).map((r) => r.file))] : []
    const beside: Sibling[] = files.map((f) => ({ id: `backup:${bucket}/${f}`, label: f, meta: bucket }))
    return [root, bucketsCrumb, { ...bucketCrumb, go: () => open(`bucket:${bucket}`) }, { key: id, label: file, id, siblings: beside }]
  }

  const jump: JumpItem[] = [
    ...stores.flatMap((s) => s.projects.map((p) => ({ id: `project:${s.name}/${p}` as TabId, group: t('jumpProjects'), label: p, where: `${s.name} · ${storeWhere(s)}` }))),
    ...stores.map((s) => ({ id: `store:${s.name}` as TabId, group: t('jumpStores'), label: s.name, where: `${kindWord(s)} · ${storeWhere(s)}` })),
    ...places.map((p) => ({ id: `machine:${p.name}` as TabId, group: t('jumpMachines'), label: machineWord(p.name), where: mc.kind(p.platform) })),
    ...sandboxes.map((b) => ({
      id: `sandbox:${b.machine ?? ''}/${b.name}` as TabId, group: t('jumpSandboxes'), label: b.name,
      where: [machineWord(b.machine ?? ''), b.project ? t('jumpBound', { project: b.project }) : null].filter(Boolean).join(' · '),
    })),
    ...(view?.buckets ?? []).map((b) => ({ id: `bucket:${b.name}` as TabId, group: t('jumpBuckets'), label: b.name, where: providerWords(b.provider, t) })),
    ...pages.map((p) => ({ id: p.id, group: t('jumpPages'), label: p.label, where: t('title') })),
  ]

  const fleetReason = fleetRead.status === 'error' ? t('addFleetUnavailable')
    : fleetRead.data?.available === false ? fleetRead.data.reason || t('addFleetUnavailable')
    : !fleet ? t('readingMachines') : undefined
  const canCreate = fleet?.platforms.some((p) => p.how === 'created' && p.ready && !p.missing.length && (VM_KINDS as readonly string[]).includes(p.platform))
  const machineReason = fleetReason ?? (canCreate ? undefined : t('addNoMachinePlatform'))
  const candidates = useMemo(() => fleet && catalogRead.data ? sandboxMachines(fleet, catalogRead.data) : [], [fleet, catalogRead.data])
  const sandboxReason = fleetReason ?? (catalogRead.error ? t('addProjectsUnavailable') : !catalogRead.data ? t('loading')
    : !candidates.length ? t('addNoSandboxMachine') : undefined)
  const add: AddItem[] = [
    { id: 'session', label: t('addNewSession'), run: () => projectDialog.open({ mode: 'pick' }) },
    { id: 'project', label: t('newProject'), run: () => projectDialog.open({ mode: 'new' }) },
    { id: 'import', label: t('addImportFolder'), run: () => projectDialog.open({ mode: 'import' }) },
    { id: 'sandbox', label: t('newSandbox'), disabledReason: sandboxReason, run: () => { if (!sandboxReason) setSandbox('pick') } },
    // from a machine's page the new Store defaults to that machine (when it runs and is not broken)
    { id: 'store', label: t('newStore'), run: () => setDialog({ kind: 'new-store', machine: active.startsWith('machine:') ? active.slice(8) : undefined }) },
    { id: 'database', label: t('connectDatabaseRow'), run: () => setDialog({ kind: 'connect' }) },
    { id: 'machine', label: t('newMachine'), disabledReason: machineReason, run: () => { if (!machineReason) setNewMachine(true) } },
    { id: 'bucket', label: t('addBucket'), run: () => setDialog({ kind: 'bucket' }) },
    { id: 'presets', label: more.presets('openTitle'), run: () => setPresetsOpen(true) },
  ]

  const dialogs = (
    <>
      <PresetsDialog api={api} t={more.presets} open={presetsOpen} onClose={() => setPresetsOpen(false)} />
      <StorageDialogs api={api} t={t} dialog={dialog} view={view} onClose={() => setDialog(null)} done={refresh} onDialog={setDialog} />
      {sandbox === 'pick' ? (
        <SandboxMachinePicker t={t} machines={candidates} reason={sandboxReason} onPick={setSandbox} onClose={() => setSandbox(null)} />
      ) : sandbox ? (
        <NewSandboxDialog machine={sandbox} taken={sandboxes.map((b) => b.name)} onClose={() => setSandbox(null)} />
      ) : null}
      {newMachine && fleet ? (
        <AddMachineDialog view={fleet} platforms={fleet.platforms} onClose={() => setNewMachine(false)} onMade={(name) => { setNewMachine(false); open(`machine:${name}`) }} />
      ) : null}
    </>
  )
  const refusal = failed !== undefined ? (
    <div className={css.refusalBar}>
      <Refusal error={failed} labels={refusalLabels(t)} />
      <Button variant="ghost" size="sm" onClick={() => setFailed(undefined)}>{t('dismiss')}</Button>
    </div>
  ) : null

  const pageOf = (id: TabId): ReactNode => {
    if (id === 'health') return <HealthView api={api} t={doctorT} ts={t} refreshKey={pageRefresh} />
    if (id === 'logs') return <LogsPage refreshKey={pageRefresh} api={api} t={more.logs} />
    if (id.startsWith('machine:')) {
      return (
        <MachineDetail key={id} name={id.slice(8)} go={mc.go} onAddStore={(machine) => setDialog({ kind: 'to-machine', machine })}
          // before the Stores are read the tab says so (no empty list that the cards then replace)
          storesTab={(machine) => (!view ? <Loading label={t('readingStores')} /> : (
            <MachineStores api={api} machine={machine} view={view} t={t} jobs={jobs} dismissed={dismissed} now={now} actionsFor={actionsFor} loginActions={loginActions} sandboxes={sandboxes}
              busy={new Set(sandboxes.filter((b) => b.machine === machine && b.project).map((b) => b.project!.split('/')[0]!))}
              onPushHere={(m) => setDialog({ kind: 'to-machine', machine: m })} onConnect={(m) => setDialog({ kind: 'connect', machine: m })}
              onOpenStore={(name) => open(`store:${name}`)} onOpenProject={(key) => open(`project:${key}`)} onBackups={(store) => setDialog({ kind: 'backups', store })} />
          ))} />
      )
    }
    if (id.startsWith('sandbox:')) {
      const body = id.slice(8)
      return <SandboxDetail refreshKey={pageRefresh} key={id} machine={body.slice(0, body.indexOf('/'))} name={body.slice(body.indexOf('/') + 1)} go={mc.go} />
    }
    if (id === 'overview') {
      return (
        <Page wide label={t('title')}>
          <HomeOverview view={view} error={storage.error} t={t} setup={more.setup} now={now} doctor={doctor} jobs={jobs} dismissed={dismissed} loginActions={loginActions} sandboxes={sandboxes}
            onOpen={open} onRefresh={refresh} onCheckAgain={() => setChecks((n) => n + 1)}
            onNewStore={() => setDialog({ kind: 'new-store' })} onConnect={() => setDialog({ kind: 'connect' })}
            onBackups={(store) => setDialog({ kind: 'backups', store })} onNewMachine={() => setNewMachine(true)} onPresets={() => setPresetsOpen(true)} presetsLabel={more.presets('openTitle')}
            onAddStore={(machine) => setDialog({ kind: 'to-machine', machine })}
            onRemoveProject={(store, project) => setDialog({
              kind: 'confirm', title: t('removeProjectTitle', { name: project }), body: t('removeProjectBody'), action: t('deleteAction'), name: project,
              run: () => api.call('DELETE', `/api/projects/${enc(`${store}/${project}`)}`).then(() => { refresh(); close(`project:${store}/${project}`) }),
            })}
            onRetry={(j) => { dismiss(j.id); act(() => retry(j)) }} onDismiss={(j) => dismiss(j.id)}
            onBackup={(name) => { const s = storeNamed(name); if (s) setDialog({ kind: 'backup', store: s }) }}
            actionsFor={actionsFor} onAddBucket={() => setDialog({ kind: 'bucket' })} onListBucket={listBucket} onReconnect={reconnect} onRestore={(bucket, file) => setDialog({ kind: 'restore', bucket, file, store: null })} refreshKey={pageRefresh}
            tab={section} onTab={setSection} onListAll={listAll} listingAll={listingAll} />
        </Page>
      )
    }
    if (id.startsWith('store:')) {
      const name = id.slice(6)
      const s = view?.stores.find((x) => x.name === name)
      if (!view) return <Page label={t('title')}><Loading label={t('readingStores')} /></Page>
      return (
        <Page wide label={t('title')}>
          <StorePage refreshKey={pageRefresh} name={name} view={view} t={t} api={api} doctorT={doctorT} now={now} job={cardJob(jobs, name, dismissed, now)} jobs={jobs.filter((j) => j.store === name)}
            catalog={catalog} on={s ? actionsFor(s) : undefined} onOpenProject={(key) => open(`project:${key}`)} onOpen={open} />
        </Page>
      )
    }
    if (id.startsWith('project:')) {
      const key = id.slice(8)
      const store = key.slice(0, key.indexOf('/'))
      return (
        <Page wide label={t('title')}>
          <ProjectPage refreshKey={pageRefresh} api={api} t={t} map={map} project={key} store={view?.stores.find((x) => x.name === store)} onDialog={setDialog} onOpenStore={(store) => open(`store:${store}`)}
            onRemoved={() => { reload(); reloadCatalog(); close(id) }} />
        </Page>
      )
    }
    if (id.startsWith('backup:')) {
      const body = id.slice(7)
      const bucket = body.slice(0, body.indexOf('/'))
      const file = body.slice(body.indexOf('/') + 1)
      if (!view) return <Page label={t('title')}><Loading label={t('readingStores')} /></Page>
      return (
        <Page label={t('title')}>
          <BackupFilePage bucket={bucket} file={file} view={view} t={t} now={now} listing={listingAll || listing === bucket}
            onList={() => listBucket(bucket)}
            onRestore={(f, store) => setDialog({ kind: 'restore', bucket, file: f, store })}
            onBackup={(store, f) => act(() => api.call('POST', `/api/stores/${enc(store)}/backup`, { bucket, file: f }))}
            onOpenStore={(store) => open(`store:${store}`)} onOpenProject={(key) => open(`project:${key}`)}
            onOpenBucket={() => open(`bucket:${bucket}`)} onReconnect={reconnect} />
        </Page>
      )
    }
    const name = id.slice(7)
    if (!view) return <Page label={t('title')}><Loading label={t('readingStores')} /></Page>
    return (
      <Page label={t('title')}>
        <BucketPage name={name} view={view} t={t} api={api} doctorT={doctorT} now={now} listing={listingAll || listing === name}
          onList={() => listBucket(name)}
          onRestore={(file, store) => setDialog({ kind: 'restore', bucket: name, file, store })}
          onBackup={(store, file) => act(() => api.call('POST', `/api/stores/${enc(store)}/backup`, { bucket: name, file }))}
          onDisconnect={() => setDialog({
            kind: 'confirm', title: t('confirmTitleBucket', { name }), body: t('confirmBucketBody'), action: t('disconnectAction'),
            run: () => api.call('DELETE', `/api/buckets/${enc(name)}`).then(() => close(`bucket:${name}`)),
          })}
          onOpenStore={(store) => open(`store:${store}`)} onOpenFile={(file) => open(`backup:${name}/${file}`)} onReconnect={reconnect} />
      </Page>
    )
  }

  return (
    <div className={css.shell}>
      <NavBar t={t} history={history} crumbs={crumbsOf(active)} jump={jump} sessions={map.sessions} onSession={api.openSession} jobs={jobs} findings={doctor.data?.findings ?? []} refreshing={detailReading || (fleetPage && fleetRead.status === 'loading') || (inventoryPage && storage.loading) || (active === 'overview' && catalogRead.loading)}
        onRefresh={active === 'overview' || active === 'health' || active === 'logs' ? undefined : refresh} add={add} onOpen={open} />
      {refusal}
      <div className={css.panels}>
        <div key={active} className={css.tabBody} data-storage-page={active}><PageReading.Provider value={setDetailReading}>{pageOf(active)}</PageReading.Provider></div>
      </div>
      {dialogs}
    </div>
  )
}
