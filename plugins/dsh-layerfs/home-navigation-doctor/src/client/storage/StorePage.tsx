/**
 * One Store, laid out like a machine page: its head (glyph, name, state,
 * family and place, checks and the Store menu), alerts and a job in flight,
 * then three tabs. Overview: tiles, what fills the database file, how the
 * Store grew, which projects wrote it, its health and details. Projects: one
 * card per project. Backups (SQLite) or Connection (PostgreSQL).
 */
import { useEffect, useId, useState, type ReactNode } from 'react'
import { Button, SegmentedTabs } from '@deepseek-ai/dsh-client-ui-primitives'
import { enc, type LayerfsApi } from '../api.ts'
import { DoctorAction } from '../doctor/Doctor.tsx'
import type { DoctorT } from '../doctor/locale.ts'
import { useDetailTab } from '../machines/detail-tabs.ts'
import { useProjectMachines } from '../projects/hooks.ts'
import { useQuery } from '../ui/use-query.ts'
import { Empty, Notice, PageHead, StateChip } from '../ui/index.tsx'
import { useHostEvents } from './hooks.ts'
import { JobPanel, OpPanel } from './JobProgress.tsx'
import type { StorageT } from './locale.ts'
import { canAddProject, storeStateWords, storeTip, storeTone } from './model.ts'
import { StoreGlyph } from './parts.tsx'
import { Location, MachineStartAction, StoreAction, StoreMenu, storeNotice, useStoreTip, type StoreActions } from './StoreCard.tsx'
import { StoreBackups, StoreConnection } from './StoreLinks.tsx'
import { StoreOverview } from './StoreOverview.tsx'
import { StoreProjects } from './StoreProjects.tsx'
import type { TabId } from './tabs.ts'
import { runningByProject, stillCounting, storeTabs, type StoreProjectsView, type StoreStats, type StoreTab } from './store-page.ts'
import type { ProjectSummary, StorageJob, StorageView, StoreRow } from './types.ts'
import css from './Storage.module.css'
import tabCss from './StorePage.module.css'

/** While a project's ledger is still on its first read, the Projects answer is asked again this often. */
const COUNTING_POLL_MS = 2000

export interface StorePageProps {
  refreshKey?: number
  name: string
  view: StorageView
  t: StorageT
  api: LayerfsApi
  doctorT: DoctorT
  now: number
  job: StorageJob | undefined
  jobs: readonly StorageJob[]
  catalog: ReadonlyMap<string, ProjectSummary>
  on: StoreActions | undefined
  onOpenProject: (key: string) => void
  /** Open any page by its id (a Store, a Sandbox, a project), for the project table's links. */
  onOpen?: (id: TabId) => void
}

/** The notice's words: the state word and its reason, with LayerFS's sentence (or the blocking machine's) as the tip. */
function StoreNoticeWords({ s, t, notice }: { s: StoreRow, t: StorageT, notice: string }): ReactNode {
  return <span title={useStoreTip(s, t)}>{notice}</span>
}

export function StorePage({ refreshKey = 0, name, view, t, api, doctorT, now, job, on, onOpenProject, onOpen }: StorePageProps): ReactNode {
  const ids = useId()
  const s = view.stores.find((x) => x.name === name)
  const kind = s?.kind ?? 'sqlite'
  const tabs = storeTabs(kind)
  const [tab, setTab] = useDetailTab<StoreTab>('store', name, tabs, 'overview')
  const [eventTick, setTick] = useState(0)
  const tick = eventTick + refreshKey
  useHostEvents(api, ['storage', 'storage-job', 'layerfs-workspaces-changed'], () => setTick((n) => n + 1), { coalesceMs: 2000 })
  const projects = useQuery(() => api.call<StoreProjectsView>('GET', `/api/stores/${enc(name)}/projects`), [api, name], { refresh: [tick, s?.projects.length], cache: `store-projects:${name}` })
  const counting = projects.data?.projects.some(stillCounting) ?? false
  useEffect(() => {
    if (!counting) return
    const timer = setTimeout(projects.reload, COUNTING_POLL_MS)
    return () => clearTimeout(timer)
  }, [counting, projects.data, projects.reload])
  const machines = useProjectMachines(api)
  const stats = useQuery(() => api.call<StoreStats>('GET', `/api/stores/${enc(name)}/stats`), [api, name], { refresh: [tick], cache: `store-stats:${name}` })
  if (!s || !on) return <><PageHead title={name} /><Empty title={t('storeMissing', { name })}>{t('storeMissingBody')}</Empty></>
  const pg = s.kind === 'postgresql'
  const machineState = view.machines.find((m) => m.name === s.machine)?.state
  const notice = storeNotice(s, machineState, t)
  const running = runningByProject(s.name, machines.data?.available ? machines.data.sandboxes : null)
  // A job of an earlier Store that had this name (over before this one was created) is not this Store's.
  const current = job && (s.created == null || (job.ended ?? Infinity) >= s.created) ? job : undefined
  const labels: Record<StoreTab, string> = {
    overview: t('tabOverview'),
    projects: `${t('tabProjects')} ${s.projects.length}`,
    backups: `${t('backupsSection')} ${s.links.length}`,
    connections: t('connection'),
  }
  return (
    <>
      <PageHead title={s.name} icon={<StoreGlyph kind={s.kind} size={22} />}
        badge={s.state !== 'ready' ? <StateChip tone={storeTone(s.state)} label={storeStateWords(s.state, t)} live={s.state === 'busy'} /> : undefined}
        intro={(
          <span className={css.cardSub}>
            <span>{pg ? t('postgresql') : t('sqlite')}</span>
            <span className={css.dotSep} aria-hidden="true">·</span>
            <Location store={s} t={t} onMachine={on.openMachine} />
          </span>
        )}
        actions={(
          <>
            <DoctorAction api={api} t={doctorT} target="store.integrity" name={s.name} label={doctorT('checkIntegrity')} />
            {pg ? <DoctorAction api={api} t={doctorT} target="store.connection" name={s.name} label={doctorT('testConnection')} /> : null}
            <StoreMenu store={s} on={on} t={t} />
          </>
        )} />

      {notice ? (
        <Notice tone="danger" role="alert">
          {/* the state word and its reason; LayerFS's sentence is the tip, the fix the one action */}
          <StoreNoticeWords s={s} t={t} notice={notice} />
          {/* the machine's Start behind the press guard, like every power control (MachineStartAction) */}
          {s.kind === 'sqlite' && machineState === 'stopped' ? <MachineStartAction machine={s.machine ?? ''} t={t} onStart={on.start} /> : <StoreAction store={s} t={t} />}
        </Notice>
      ) : null}
      {current ? <JobPanel job={current} t={t} onRetry={() => on.retry(current)} onDismiss={() => on.dismiss(current)} /> : s.state === 'busy' && s.op ? <OpPanel store={s} t={t} /> : null}

      <SegmentedTabs size="sm" className={tabCss.tabs} label={t('spTabs', { name: s.name })} value={tab} onChange={(v) => setTab(v as StoreTab)}
        items={tabs.map((x) => ({ value: x, label: labels[x], id: `${ids}-${x}`, panelId: `${ids}-${x}-panel` })) as [never, ...never[]]} />

      <div className={tabCss.panelArea} role="tabpanel" id={`${ids}-${tab}-panel`} aria-labelledby={`${ids}-${tab}`}>
        {tab === 'projects' ? (
          <StoreProjects api={api} store={{ name: s.name, kind: s.kind, state: s.state }} names={s.projects} projects={projects.data?.projects} stored={typeof stats.data?.stored_bytes === 'number' ? stats.data.stored_bytes : null}
            stores={view.stores} fleet={machines.data?.available ? machines.data : null} t={t} now={now} tick={tick}
            canAdd={canAddProject(s)} onAdd={on.addProject} onOpen={(id) => (onOpen ? onOpen(id) : id.startsWith('project:') ? onOpenProject(id.slice('project:'.length)) : undefined)} />
        ) : tab === 'backups' ? (
          <StoreBackups store={s} view={view} t={t} now={now} on={on} />
        ) : tab === 'connections' ? (
          <StoreConnection store={s} view={view} t={t} on={on} now={now} sandboxes={machines.data?.available ? machines.data.sandboxes : undefined} />
        ) : (
          <StoreOverview store={s} view={view} t={t} api={api} now={now} stats={stats.data} statsError={stats.error} projects={projects.data?.projects} running={running}
            machineState={machineState} onTab={setTab} onRemove={pg ? on.disconnect : on.remove} />
        )}
      </div>
    </>
  )
}
