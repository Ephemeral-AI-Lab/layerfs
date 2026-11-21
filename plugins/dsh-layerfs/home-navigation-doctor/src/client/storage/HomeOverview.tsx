/** Home inventory, ordered notices and the selected frame. Center owns navigation and Doctor results. */
import { useContext, useId, useRef, useState, type ReactNode } from 'react'
import { Button, IconAgentPresetOutlineRegular, IconInspectOutlineRegular, IconPlusOutlineRegular, IconRefreshOutlineRegular, StateDot } from '@deepseek-ai/dsh-client-ui-primitives'
import { ago, bytes } from '../files/format.ts'
import type { Query } from '../files/use-query.ts'
import type { Translate } from '../files/types.ts'
import { useMachinesContext } from '../machines/context.tsx'
import { VM_KINDS, downReason, machineOrder, machineState, recordOutdated, vmGone, workerDown, type AvailableView } from '../machines/model.ts'
import { machineBlock, stepReasons } from '../machines/needs.ts'
import type { SetupModalT } from '../setup/locale.ts'
import { SetupCard, unset } from '../setup/SetupCard.tsx'
import { missingWords } from '../setup/SetupDialog.tsx'
import { setupDialog, useSetupStatus } from '../setup/store.ts'
import { EmptyState } from '../files/States.tsx'
import { Card, FixContext, LoadError, Loading, Refusal, resolveFix, usePressGuard } from '../ui/index.tsx'
import { isOverviewTab, type OverviewTab, type TabId } from './tabs.ts'
import { BucketCards } from './BucketCards.tsx'
import { HomeStores } from './MachineStores.tsx'
import { ProjectTable } from './ProjectTable.tsx'
import { ProjectFinderBar, useProjectFinder } from './ProjectFinder.tsx'
import { projectMatches } from './project-list.ts'
import { Board } from '../machines/Board.tsx'
import { useStartDoctor } from '../doctor/Doctor.tsx'
import { LayerfsGlyph } from '../overview/glyph.tsx'
import { projectDialog } from '../projects/dialog.ts'
import { HomeHero, panelId } from './HomeHero.tsx'
import { StoreMenu, type LoginActions, type StoreActions } from './StoreCard.tsx'
import { compareNeeds, visibleFailures, worstTone } from './home-notices.ts'
import { storeTrouble } from './HomeCards.tsx'
import { Signal } from './Signal.tsx'
import type { StorageT } from './locale.ts'
import { brokenWords, endedWords, holderTab, jobEnd, needsLogin, orderStores, type Reconnect } from './model.ts'
import { refusalLabels, StoreGlyph } from './parts.tsx'
import type { StorageJob, StorageView, StoreRow } from './types.ts'
import type { DoctorCount } from '../../shared/doctor.ts'
import css from './HomeOverview.module.css'
import heroCss from './HomeHero.module.css'

export interface HomeOverviewProps {
  view: StorageView | undefined
  error: unknown
  t: StorageT
  /** The setup modal's copy (the setup cards Center shows). */
  setup: SetupModalT
  now: number
  doctor: Query<DoctorCount>
  jobs: readonly StorageJob[]
  dismissed: ReadonlySet<string>
  onOpen: (id: TabId) => void
  onRefresh: () => void
  onCheckAgain: () => void
  onNewStore: () => void
  /** Connect a database (a new PostgreSQL Store). */
  onConnect: () => void
  /** Opens a SQLite Store's Backups dialog. */
  onBackups: (store: string) => void
  onNewMachine: () => void
  /** Open the Presets dialog (sample libraries to build into this home). */
  onPresets: () => void
  /** "Try a preset", from the Presets copy. */
  presetsLabel: string
  /** A machine's Add Store dialog (move a SQLite Store there, or connect a PostgreSQL one). */
  onAddStore: (machine: string) => void
  /** Delete a project, through the page's confirm dialog. */
  onRemoveProject: (store: string, project: string) => void
  onRetry: (job: StorageJob) => void
  onDismiss: (job: StorageJob) => void
  onBackup: (store: string) => void
  /** A Store's actions (the Store menu, Back up, Edit connection), through the page's dialogs. */
  actionsFor: (store: StoreRow) => StoreActions
  /** A saved PostgreSQL login's own actions (the PostgreSQL tab's login headers). */
  loginActions: LoginActions
  /** The Sandboxes the Machines view knows: a Connected from chip counts the machine's Sandboxes of the Store. */
  sandboxes?: readonly { machine?: string | null, project?: string | null }[]
  onAddBucket: () => void
  /** Lists a bucket's files. */
  onListBucket: (bucket: string) => void
  /** Connect again… on a bucket whose key was refused. */
  onReconnect: (again: Reconnect) => void
  /** Restores a bucket file not from this home as a new Store. */
  onRestore: (bucket: string, file: string) => void
  /** Bumps on Refresh and Check again: PostgreSQL sizes are read again. */
  refreshKey: number
  /** The section showing (the page owns it, so the breadcrumb can open one). */
  tab: OverviewTab
  onTab: (tab: OverviewTab) => void
  /** Lists every bucket's files. */
  onListAll: () => void
  /** Every bucket is being listed. */
  listingAll: boolean
}

/** One thing that needs you: how bad, what, why, and the one verb that fixes it. */
interface Need {
  key: string
  tone: 'danger' | 'warning' | 'neutral'
  title: ReactNode
  detail?: ReactNode
  action?: { label: string, run: () => void | Promise<unknown> } | undefined
  /** A second, quieter action (e.g. Stop beside Restart worker); runs and refuses like `action`. */
  secondary?: { label: string, title?: string, run: () => void | Promise<unknown>, confirm?: string } | undefined
  extra?: ReactNode
}


export function HomeOverview(props: HomeOverviewProps): ReactNode {
  const { view, error, t, now, doctor, onRefresh } = props
  const fixes = useContext(FixContext)
  const mc = useMachinesContext()
  const fleetRead = mc.useMachines((s) => s)
  const startups = mc.useStartups((s) => s)
  const fleet: AvailableView | null = fleetRead.data?.available ? fleetRead.data : null
  const setupRead = useSetupStatus(mc.api)
  const setup = setupRead.data
  const startDoctor = useStartDoctor(mc.api)
  const dockerDetailsId = useId()
  const [dockerExpanded, setDockerExpanded] = useState(false)
  const [storeActionsHost, setStoreActionsHost] = useState<HTMLSpanElement | null>(null)
  const [showNeeds, setShowNeeds] = useState(false)
  const tab = props.tab
  const setTab = (key: string) => { if (isOverviewTab(key)) props.onTab(key) }
  const finder = useProjectFinder(view?.stores ?? [], props.refreshKey)

  if (!view || (setup && unset(setup) && !setupDialog.deferred() && !view.stores.length)) {
    return (
      <div className={css.page}>
        {setup && unset(setup)
          ? <SetupCard s={setup} t={props.setup} first />
          : error
          ? <LoadError title={t('storageFailed')} error={error} labels={refusalLabels(t)} retryLabel={t('retry')} onRetry={onRefresh} />
          : <Loading label={t('readingStores')} />}
      </div>
    )
  }

  const place = (machine: string | null | undefined) => (machine === 'native' ? t('groupHome') : machine ?? '?')
  const stores = orderStores(view.stores)
  const sqlite = stores.filter((s) => s.kind === 'sqlite')
  const projectCount = stores.reduce((n, s) => n + s.projects.length, 0)
  const searching = finder.query.trim() !== ''
  const projectsShown = searching ? stores.reduce((n, s) => n + s.projects.filter((p) => projectMatches(s.name, p, finder.query)).length, 0) : projectCount
  const machines = fleet ? machineOrder(fleet.machines) : []
  const runningJobs = props.jobs.filter((j) => !j.ended)

  // ---------- what needs you, most pressing first ----------
  const needs: Need[] = []
  const failures = visibleFailures(props.jobs, view.stores, props.dismissed)
  for (const j of failures) {
    // the card's word is how the job ended; LayerFS's own words are the tip (UI rule: no raw errors on cards);
    // its fix line, where the page runs it, is the one action, else Retry
    const fix = fixes ? resolveFix(fixes, j.error?.fix)?.fix ?? null : null
    // in use (F10): the tip names what holds the Store (its Sandboxes), the one action opens the first of them
    const { holders: [holder], tip } = jobEnd(j, fleet?.sandboxes ?? [], t, fixes ? (line) => fixes.tip(line) : undefined)
    needs.push({
      key: `job:${j.id}`, tone: 'danger',
      title: <StoreLink name={j.store} onOpen={props.onOpen}>{endedWords(j, t, bytes)}</StoreLink>,
      detail: <>{tip}<br />{ago(j.ended, now, t as Translate)}</>,
      action: fix && fixes ? { label: fixes.label(fix), run: () => fixes.run(fix) }
        : holder ? { label: t('jobOpenSandbox', { name: holder.name }), run: () => props.onOpen(holderTab(holder)) }
          : { label: t('jobRetry'), run: () => props.onRetry(j) },
      extra: <Button variant="ghost" size="sm" onClick={() => props.onDismiss(j)}>{t('dismiss')}</Button>,
    })
  }
  // Machines whose worker does not answer: one need each, with the fix, standing in for their Stores' "down".
  // A machine whose platform is not ready here (Docker quit, Lima removed): its fix, not a worker restart that would fail.
  const blocked = new Map(fleet ? machines.flatMap((m) => { const b = machineBlock(m, fleet); return b ? [[m.name, b] as const] : [] }) : [])
  const unreachable = new Set(machines.filter((m) => !blocked.has(m.name) && workerDown(m, fleet?.live[m.name])).map((m) => m.name))
  for (const m of machines) {
    const look = machineState(m, fleet?.live[m.name])
    const block = blocked.get(m.name)
    if (block && m.platform === 'docker') continue
    if (block) {
      needs.push({
        key: `unavailable:${m.name}`, tone: 'warning',
        title: <Link onClick={() => props.onOpen(`machine:${m.name}`)}>{mc.t('unavailableWhy', { kind: place(m.name), reason: stepReasons(block, mc.tw) })}</Link>,
        detail: m.stores.length ? m.stores.join(', ') : undefined,
        action: { label: t('ovOpen'), run: () => props.onOpen(`machine:${m.name}`) },
      })
    } else if (unreachable.has(m.name)) {
      needs.push({
        key: `unreachable:${m.name}`, tone: 'danger',
        title: <Link onClick={() => props.onOpen(`machine:${m.name}`)}>{m.stores.length
          ? t('ovWorkerDownStores', { machine: place(m.name), stores: m.stores.join(', ') })
          : t('ovWorkerDown', { machine: place(m.name) })}</Link>,
        detail: downReason(m, fleet?.live[m.name]) ?? undefined,
        // the head's one slot changes with what is most pressing: it never starts a machine,
        // it opens that machine's page, where its own Start (or fix) is pressed (UI pass 2 audit);
        // Stop <machine> beside it stays a press that names the machine (RV4 NOTE 14)
        action: { label: t('ovOpen'), run: () => props.onOpen(`machine:${m.name}`) },
        // a power action from the head asks a second press that names the machine (UI pass 3 F6): a stray click never stops it
        secondary: { label: t('ovStopUnreachable', { machine: place(m.name) }), title: t('ovStopUnreachableTitle', { machine: place(m.name) }), confirm: t('ovStopConfirm', { machine: place(m.name) }), run: () => mc.act({ kind: 'stop', name: m.name }) },
      })
    } else if (vmGone(m)) {
      // its VM is gone (LayerFS `missing`, M FAIL 9): not "not answering", no Stop or Restart; LayerFS's fix is the one action
      const fix = fixes ? resolveFix(fixes, m.fix)?.fix ?? null : null
      needs.push({
        key: `gone:${m.name}`, tone: 'danger',
        title: <Link onClick={() => props.onOpen(`machine:${m.name}`)}>{m.stores.length
          ? t('ovVmGoneStores', { machine: place(m.name), stores: m.stores.join(', ') })
          : t('ovVmGone', { machine: place(m.name) })}</Link>,
        detail: <span title={[m.message, m.fix].filter(Boolean).join('\n') || undefined}>{mc.stateWord(look)}</span>,
        action: fix && fixes ? { label: fixes.label(fix), run: () => fixes.run(fix) } : { label: t('ovOpen'), run: () => props.onOpen(`machine:${m.name}`) },
      })
    } else if (recordOutdated(m)) {
      // an outdated record no call repairs (UI pass 3 R FAIL 2): not a worker that does not answer, and no Stop;
      // its fix (remove, after its Stores) runs behind a confirm on the machine's page, so the head opens it
      needs.push({
        key: `record:${m.name}`, tone: 'danger',
        title: <Link onClick={() => props.onOpen(`machine:${m.name}`)}>{m.stores.length
          ? t('ovRecordOutdatedStores', { machine: place(m.name), stores: m.stores.join(', ') })
          : t('ovRecordOutdated', { machine: place(m.name) })}</Link>,
        detail: m.message ?? m.fix ?? undefined,
        action: { label: t('ovOpen'), run: () => props.onOpen(`machine:${m.name}`) },
      })
    } else if (look.key === 'needs') {
      needs.push({
        key: `needs:${m.name}`, tone: 'danger',
        title: <Link onClick={() => props.onOpen(`machine:${m.name}`)}>{t('ovMachineNeeds', { machine: place(m.name) })}</Link>,
        detail: downReason(m, fleet?.live[m.name]) ?? mc.stateWord(look),
        action: { label: t('ovOpen'), run: () => props.onOpen(`machine:${m.name}`) },
      })
    } else if (look.key === 'stopped' && m.stores.length > 0 && startups[m.name]?.status !== 'running') {
      needs.push({
        key: `stopped:${m.name}`, tone: 'warning',
        title: <Link onClick={() => props.onOpen(`machine:${m.name}`)}>{t('ovMachineStoppedStores', { machine: place(m.name), stores: m.stores.join(', ') })}</Link>,
        detail: t('ovMachineStoppedBody'),
        action: { label: t('ovOpen'), run: () => props.onOpen(`machine:${m.name}`) },
      })
    }
  }
  for (const s of stores) {
    // a broken Store (2.0): its word, its fix line as the detail; a machine already listed above is not repeated
    if (s.state === 'broken' && !(s.kind === 'sqlite' && unreachable.has(s.machine ?? ''))) {
      const pgDown = s.kind === 'postgresql' && s.reason === 'unreachable'
      // a PostgreSQL Store that is only outdated or names a missing login is reachable: it needs its update, not "down" (R1 F1);
      // its state word with LayerFS's fix as the tip, and that fix as the one action where the page runs it
      const needsKey = s.kind !== 'postgresql' ? null : needsLogin(s) || s.reason === 'missing' ? 'ovStoreNeedsLogin' : s.reason === 'outdated' ? 'ovStoreNeedsUpdate' : null
      const fix = needsKey && fixes ? resolveFix(fixes, s.fix)?.fix ?? null : null
      needs.push({
        key: `${pgDown ? 'pg' : needsKey ? 'update' : 'down'}:${s.name}`, tone: 'danger',
        title: <Link onClick={() => props.onOpen(`store:${s.name}`)}>{t(pgDown ? 'ovPgUnreachable' : needsKey ?? 'ovStoreDown', { store: s.name })}</Link>,
        detail: needsKey ? <span title={s.fix ?? undefined}>{brokenWords(s, t)}</span> : s.fix ?? (pgDown ? s.postgresql?.target ?? undefined : undefined),
        action: fix && fixes ? { label: fixes.label(fix), run: () => fixes.run(fix) } : { label: t('ovOpen'), run: () => props.onOpen(`store:${s.name}`) },
      })
    }
  }
  // Setup still missing something required (after Later): the head's first need, with Set up LayerFS… (one place, no second card).
  const setupLeft = setup && setup.needs.length ? missingWords(setup, props.setup) : null
  if (setupLeft) {
    needs.push({
      key: 'setup', tone: 'warning', title: props.setup('cardFinishTitle'), detail: setupLeft,
      action: { label: props.setup('openSetup'), run: () => setupDialog.open(setup!.needs.includes('folder') && setup!.runtime.ok ? 'folder' : 'runtime') },
    })
  }
  // A Store never backed up is not a problem: its card says so quietly.
  needs.sort(compareNeeds)

  // ---------- head ----------
  const dockerBlock = fleet && machineBlock({ platform: 'docker' }, fleet)
  const dockerMachines = machines.filter(m => m.platform === 'docker')
  // The head names the most pressing need and carries its fix, and counts the rest (their cards and tabs carry their signals).
  const top = needs[0]
  const rest = needs.slice(1)
  // A tab's dot: the worst state among what it shows.
  const rowTones = stores.map(store => {
    const state = storeTrouble(store, view, t, place, fleet)?.state
    return { store, tone: state === 'error' ? 'danger' as const : state === 'warning' ? 'warning' as const : undefined }
  })
  const projectTone = worstTone(rowTones.filter(({ store }) => store.projects.length).map(row => row.tone))
  const storeTone = failures.length ? 'danger' : worstTone(rowTones.map(row => row.tone))
  const machineTone = worstTone(needs.filter(n => /^(unavailable|unreachable|gone|record|needs|stopped):/.test(n.key))
    .map(n => n.tone === 'neutral' ? undefined : n.tone))
  const machineReason = !fleet ? t(fleetRead.status === 'error' || fleetRead.data?.available === false ? 'addFleetUnavailable' : 'readingMachines')
    : fleet.platforms.some(p => p.how === 'created' && p.ready && !p.missing.length && (VM_KINDS as readonly string[]).includes(p.platform))
      ? undefined : t('addNoMachinePlatform')
  const refreshAll = () => { if (!doctor.loading) { onRefresh(); props.onCheckAgain() } }
  const newProject = () => projectDialog.open({ mode: 'new' })
  // The selected tab's actions, at the right end of the tab bar (an empty tab carries its own call to action instead).
  const plus = <IconPlusOutlineRegular size={12} />
  const tabActions = tab === 'projects' ? (projectCount > 0 ? <Button variant="outline" size="sm" className={heroCss.actionButton} icon={plus} onClick={newProject}>{t('newProject')}</Button> : null)
    : tab === 'machines' ? (
      <>
        <Button title={machineReason} disabled={!!machineReason} variant="outline" size="sm" className={heroCss.actionButton} icon={plus} onClick={props.onNewMachine}>{t('newMachine')}</Button>
        <Button variant="ghost" size="sm" icon={<IconInspectOutlineRegular size={14} />}
          onClick={() => { void startDoctor('host').catch(() => props.onOpen('health')) }}>{mc.doctorT('diagnoseHost')}</Button>
      </>
    )
     : <span className={heroCss.storeActions} ref={setStoreActionsHost}>
        {stores.length === 0 && view.buckets.length === 0 && <Button variant="outline" size="sm" className={heroCss.actionButton} icon={plus} onClick={props.onNewStore}>{t('newStore')}</Button>}
      </span>
  // SQLite's second view: the backup buckets (once the Backups tab). PostgreSQL has one view: its databases under their server.
  const storesAlt = {
    main: { label: t('hsViewStores'), count: sqlite.length },
    other: { label: t('hsViewBuckets'), count: view.buckets.length },
    view: <BucketCards view={view} t={t} now={now} jobs={runningJobs} onOpen={props.onOpen} onList={props.onListBucket} onReconnect={props.onReconnect} onRestore={props.onRestore} />,
    add: (
      <>
        {view.buckets.length > 0 ? <Button variant="outline" size="sm" disabled={props.listingAll} onClick={props.onListAll}>{t('ovListBuckets')}</Button> : null}
        <Button variant="outline" size="sm" icon={plus} onClick={props.onAddBucket}>{t('ovAddBucket')}</Button>
      </>
    ),
  }
  // A Store row's menu on the board: the Store card's own menu (a PostgreSQL Store's checks from this machine).
  const storeMenuOn = (machine: string) => ({ name }: { name: string }) => {
    const s = stores.find((x) => x.name === name)
    return s ? <StoreMenu store={s} on={props.actionsFor(s)} t={t} machine={s.kind === 'postgresql' ? machine : undefined} /> : null
  }
  const checked = doctor.data?.at ? t('ovChecked', { ago: ago(doctor.data.at, now, t as Translate) }) : null

  return (
    <div className={css.page}>
      <HomeHero
        title={t('overview')}
        tone={top?.tone}
        signal={top
          ? <Signal state={top.tone === 'danger' ? 'error' : top.tone === 'warning' ? 'warning' : 'idle'} title={top.tone === 'danger' ? t('ovSignalProblem') : top.tone === 'warning' ? t('ovSignalWarning') : t('ovSignalNote')} detail={top.detail} />
          : null}
        headline={top ? top.title : dockerBlock ? null : t('ovHealthAllGood')}
        actions={top ? (
          <NeedActions key={top.key} need={top} dismissLabel={t('dismiss')} labels={refusalLabels(t)} role={top.tone === 'danger' ? 'alert' : undefined}
            more={rest.length ? <Button variant="ghost" size="sm" aria-controls="ov-more-needs" aria-expanded={showNeeds} onClick={() => setShowNeeds(!showNeeds)}>{t('ovMoreProjects', { n: rest.length })}</Button> : null} />
        ) : null}
        toolsLabel={t('ovToolsLabel')}
        tools={(
          <>
            <Button variant="ghost" size="sm" className={heroCss.toolButton} icon={<IconAgentPresetOutlineRegular size={15} />} onClick={props.onPresets} aria-label={props.presetsLabel} title={props.presetsLabel} />
            <Button variant="ghost" size="sm" className={heroCss.toolButton} icon={doctor.loading ? <StateDot state="ongoing" size={13} /> : <IconRefreshOutlineRegular size={15} />}
              disabled={doctor.loading} onClick={refreshAll} aria-label={t('refresh')} title={[t(doctor.loading ? 'ovChecksRunning' : 'ovRefreshTitle'), checked].filter(Boolean).join(' · ')} />
          </>
        )}
        tabs={[
          { key: 'projects', label: t('ovProjects'), count: searching ? t('ovCountOf', { n: projectsShown, total: projectCount }) : projectCount, tone: projectTone },
          { key: 'stores', label: t('ovStores'), count: stores.length, tone: storeTone },
          { key: 'machines', label: t('ovMachines'), count: fleet ? machines.length : '—', tone: machineTone ?? (dockerBlock ? 'warning' : undefined) },
        ]}
        titleAside={dockerBlock && (
          <button type="button" className={heroCss.dockerNotice} aria-expanded={dockerExpanded}
            aria-controls={dockerDetailsId} onClick={() => setDockerExpanded(open => !open)}>
            {t('ovDockerUnavailable')}
          </button>
        )}
        titleDetail={dockerBlock && (
          <div id={dockerDetailsId} hidden={!dockerExpanded} className={heroCss.dockerDetail}>
            <div>{stepReasons(dockerBlock, mc.tw) || t('addNoMachinePlatform')}</div>
            {dockerMachines.length > 0 && <div className={heroCss.dockerMachines}>
              {dockerMachines.map(m => <Link key={m.name} onClick={() => props.onOpen(`machine:${m.name}`)}>{place(m.name)}</Link>)}
            </div>}
          </div>
        )}
        notices={<>
          {showNeeds && rest.length > 0 && (
          <ul id="ov-more-needs" className={heroCss.notices}>
            {rest.map(n => (
              <li key={n.key} className={heroCss.notice}>
                <Signal state={n.tone === 'danger' ? 'error' : 'warning'} title={t(n.tone === 'danger' ? 'ovSignalProblem' : 'ovSignalWarning')} detail={n.detail} />
                <span className={heroCss.noticeTitle}>{n.title}</span>
                <NeedActions need={n} dismissLabel={t('dismiss')} labels={refusalLabels(t)} more={null} role={undefined} />
              </li>
            ))}
          </ul>
        )}</>}
        active={tab} onSelect={setTab} tabsLabel={t('ovTabs')} tabActions={tabActions} />

      {/* ---------- 1. projects, each with its Store and machines ---------- */}
      {tab === 'projects' ? (
        <section id={panelId('projects')} role="tabpanel" className={css.section} aria-label={t('ovProjects')}>
          {projectCount === 0 ? (
            <Card>
              <EmptyState title={t('ovNoProjects')} icon={<LayerfsGlyph size={20} />}
                action={(
                  <>
                    <Button variant="primary" size="sm" icon={<IconPlusOutlineRegular size={13} />} onClick={newProject}>{t('newProject')}</Button>
                    <Button variant="outline" size="sm" icon={<IconAgentPresetOutlineRegular size={13} />} onClick={props.onPresets}>{props.presetsLabel}</Button>
                  </>
                )} />
            </Card>
          ) : (
            <>
              <ProjectFinderBar finder={finder} t={t} />
              <ProjectTable api={mc.api} now={now} tick={props.refreshKey} query={finder.query} sort={finder.sort} frozen={finder.frozen} stores={stores} view={view} fleet={fleet} t={t} jobs={runningJobs} place={place} onOpen={props.onOpen} onRemove={props.onRemoveProject}
              onRestart={(m) => mc.act({ kind: 'start', name: m.name, platform: m.platform })} />
            </>
          )}
        </section>
      ) : null}

      {/* ---------- 2. Stores, each with its projects ---------- */}
      {tab === 'stores' ? (
        <section id={panelId('stores')} role="tabpanel" className={css.section} aria-label={t('ovStores')}>
          {stores.length === 0 && view.buckets.length === 0 ? (
            <Card>
              <EmptyState title={t('noStores')} icon={<StoreGlyph kind="sqlite" size={20} />} />
            </Card>
          ) : (
            <HomeStores actionHost={storeActionsHost} api={mc.api} stores={stores} view={view} t={t} now={now} jobs={props.jobs} dismissed={props.dismissed} actionsFor={props.actionsFor} loginActions={props.loginActions} sandboxes={props.sandboxes}
              onOpenStore={(name) => props.onOpen(`store:${name}`)} onOpenProject={(key) => props.onOpen(`project:${key}`)} onBackups={props.onBackups}
              onNewStore={props.onNewStore} onConnect={props.onConnect} alt={storesAlt} />
          )}
        </section>
      ) : null}

      {/* ---------- 3. machines: the board, each over its Stores and Sandboxes ---------- */}
      {tab === 'machines' ? (
        <section id={panelId('machines')} role="tabpanel" className={css.section} aria-label={t('ovMachines')}>
          <Board go={mc.go} onAddStore={props.onAddStore} storeMenu={storeMenuOn} />
        </section>
      ) : null}

    </div>
  )
}

/** The head's need: the verb that fixes it, how many more there are, and a refusal on its own line. */
function NeedActions({ need: n, labels, more, role, dismissLabel }: {
  dismissLabel: string, need: Need, labels: ReturnType<typeof refusalLabels>, more: ReactNode, role: 'alert' | undefined }): ReactNode {
  // one action at a time; a refusal stays until the next try
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<unknown>(null)
  const [armed, arm] = useState(false)
  const setArmed = (value: boolean) => { if (value) lastPress.current = Date.now(); arm(value) }
  // click safety (UI pass 3 F6): presses are keyed to this need and dropped while its buttons have just appeared or changed
  const guard = usePressGuard(`${n.key}:${n.action?.label ?? ''}:${n.secondary?.label ?? ''}`)
  const lastPress = useRef(0)
  const go = (run: () => void | Promise<unknown>) => {
    if (busy || Date.now() - lastPress.current < 500) return
    lastPress.current = Date.now()
    setBusy(true); setError(null); setArmed(false)
    void Promise.resolve().then(run).catch(setError).finally(() => setBusy(false))
  }
  return (
    <>
      <span className={heroCss.statusActions}>
        <span className={heroCss.acts} role={role}>
          {n.action ? <Button variant="outline" size="sm" disabled={busy || guard.settling} aria-disabled={guard.settling || undefined} onClick={guard(() => go(n.action!.run))}>{n.action.label}</Button> : null}
          {n.secondary ? (
            <Button variant={armed ? 'primary' : 'ghost'} size="sm" disabled={busy || guard.settling} aria-disabled={guard.settling || undefined} title={n.secondary.title}
              onClick={guard(() => { if (n.secondary!.confirm && !armed) setArmed(true); else go(n.secondary!.run) })}>
              {armed && n.secondary.confirm ? n.secondary.confirm : n.secondary.label}
            </Button>
          ) : null}
          {n.extra}
        </span>
        {more}
      </span>
      {error != null ? <div className={heroCss.refusal}><Refusal error={error} labels={labels} /><Button size="sm" variant="ghost" onClick={() => setError(null)}>{dismissLabel}</Button></div> : null}
    </>
  )
}

function Link({ children, onClick }: { children: ReactNode, onClick: () => void }): ReactNode {
  return <button type="button" className={css.link} onClick={(e) => { e.stopPropagation(); onClick() }}>{children}</button>
}

function StoreLink({ name, onOpen, children }: { name: string, onOpen: (id: TabId) => void, children: ReactNode }): ReactNode {
  return <Link onClick={() => onOpen(`store:${name}`)}>{children}</Link>
}
