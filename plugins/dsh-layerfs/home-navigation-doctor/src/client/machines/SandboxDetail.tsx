/**
 * One Sandbox's page. Its head (name, state) and its tabs are drawn from the
 * `sandbox.list` row the machines view already holds, so the page never
 * waits on one slow read (layerfs-project#179 D). Overview (SandboxOverview.tsx:
 * the icon, name and state with Diagnose, Refresh and the menu heading its
 * Details card, then usage, disk and daemon) is the first tab; Workspaces (the
 * Workspaces as terminals, SandboxTerminals.tsx) waits on sandbox.get. Files (InspectFiles.tsx) and Daemon log (LogPane.tsx) read on
 * their own, and each is there only when the pinned CLI has its call
 * (sandbox.files, sandbox.log).
 *
 * Delete is sandbox.remove, with an explicit Force delete action. A Sandbox
 * whose machine differs from the one it was opened from is refused here.
 */
import { usePageReading } from '../storage/PaneBar.tsx'
import { useId, type ReactNode } from 'react'
import { Button, IconRefreshOutlineRegular } from '@deepseek-ai/dsh-client-ui-primitives'
import { DoctorAction } from '../doctor/Doctor.tsx'
import { LinkButton, LoadError, Loading, Notice, Page, PageHead, Refusal, StateChip } from '../ui/index.tsx'
import { TabBar } from '../ui/TabBar.tsx'
import { useMachinesContext } from './context.tsx'
import { useDetailTab } from './detail-tabs.ts'
import { SandboxGlyph } from './icons.tsx'
import { useMachineTitle } from './Inventory.tsx'
import { InspectFiles } from './InspectFiles.tsx'
import { LogPane } from './LogPane.tsx'
import type { View } from './context.tsx'
import { bytes, clearingCall, opWords, sandboxGone, thingLook, unresolvedOf, type SandboxInfo } from './model.ts'
import { cpuTime, liveNote, reasonWords, sandboxLook } from './stats.ts'
import { SandboxMenu, useClose, useSandboxRead } from './Sandboxes.tsx'
import { SandboxTerminals } from './SandboxTerminals.tsx'
import { SandboxOverview } from './SandboxOverview.tsx'
import css from './machines.module.css'
import logCss from './log.module.css'
import term from './terminal.module.css'

export function SandboxDetail({ machine, name, go, refreshKey }: { refreshKey?: number, machine: string, name: string, go: (view: View) => void }): ReactNode {
  const { t, tw, stateWord, refusal, kind, api, doctorT, useMachines } = useMachinesContext()
  const listed = useMachines((s) => (s.data?.available ? s.data.sandboxes.find((b) => b.name === name) : undefined))
  const parent = useMachines((s) => (s.data?.available ? s.data.machines.find((m) => m.name === machine) : undefined))
  const hasLog = useMachines((s) => (s.data?.available ? s.data.features?.inspect?.sandboxLog === true : false))
  const hasFiles = useMachines((s) => (s.data?.available ? s.data.features?.inspect?.files === true : false))
  // the Workspace whose terminal Files asked for ("Open terminal")
  const detail = useSandboxRead(name, listed?.state, listed?.workspaces)
  usePageReading(detail.loading)
  const closing = useClose(name)
  const title = useMachineTitle()
  const machineTitle = title({ name: machine, platform: parent?.platform ?? (machine === 'native' ? 'native' : '') })
  // 'terminals' keeps its stored value; it reads "Workspaces"
  const tabs: { value: 'overview' | 'terminals' | 'files' | 'log', label: string }[] = [
    { value: 'overview', label: t('tabOverview') },
    { value: 'terminals', label: t('tabWorkspaces') },
    ...(hasFiles ? [{ value: 'files' as const, label: t('tabFiles') }] : []),
    ...(hasLog ? [{ value: 'log' as const, label: t('tabDaemonLog') }] : []),
  ]
  const [tab, setTab] = useDetailTab('sandbox', name, tabs.map((x) => x.value), 'overview')
  const ids = useId()
  const info = detail.data
  const targetMachine = info?.machine ?? listed?.machine
  const wrong = !!targetMachine && targetMachine !== machine
  // neither the list row nor sandbox.get knows it (yet): nothing to draw a head from
  if ((!info && !listed) || wrong) {
    return (
      <Page wide label={name}>
        <PageHead title={name} icon={<SandboxGlyph size={22} />}
          actions={!wrong ? <SandboxMenu name={name} op={listed?.op} closing={closing} onDeleted={() => go({ kind: 'machine', name: machine })} /> : undefined} />
        {wrong
          ? <Notice tone="danger"><p>{t('sandboxWrongMachine', { name, machine })}</p></Notice>
          : detail.error != null
            ? <LoadError title={t('readSandboxFailed')} error={detail.error} labels={refusal} retryLabel={t('retry')} onRetry={detail.reload} />
            : <Loading label={t('readingSandbox')} />}
        {closing.error != null && <Refusal error={closing.error} labels={refusal} />}
      </Page>
    )
  }
  // the head: sandbox.get's answer once it is in, the list row until then (and when it never answers)
  const state = info?.state ?? listed?.state ?? 'unknown'
  const currentOp = info ? info.op ?? null : listed?.op ?? null
  // Whether its daemon runs: what sandbox.get saw of the process (a pid with no start time has ended), else what the list saw.
  const live = info?.daemon ? info.daemon.startedAt != null : listed?.live
  const reason = info ? info.reason : listed?.reason
  const fix = info ? info.fix : listed?.fix
  const seen = { state, live, liveReason: listed?.liveReason, op: currentOp, reason }
  const look = sandboxLook(seen, (x) => thingLook('sandbox', x))
  const liveWhy = liveNote(seen, tw)
  const op = opWords(currentOp)
  const calls = unresolvedOf({ state, reason }) ? clearingCall('sandbox', name, currentOp, fix) : null
  const project = info?.project ?? listed?.project ?? null
  const machineWithKind = t('sbovMachineKind', { machine: machineTitle, kind: parent ? kind(parent.platform) : info?.platform ?? '—' })
  const logLink = hasLog && tab !== 'log' ? <LinkButton onClick={() => setTab('log')}>{t('openDaemonLog')}</LinkButton> : null
  // a Sandbox that ended behind LayerFS's back, or whose record has no runtime: a refused log read will not get better by waiting
  const ended = sandboxGone({ state, reason })
  return (
    <Page wide label={name}>
      {/* the tabs come first, as on the machine page: the top bar's breadcrumb shows where this is, the name, icon and state are on the Overview tab (its Details card), and the page region carries the name */}
      <TabBar className={logCss.tabsTop} label={t('machineTabs', { name })} active={tab} onSelect={(key) => setTab(key as typeof tab)}
        tabs={tabs.map((x) => ({ key: x.value, label: x.label, id: `${ids}-${x.value}`, controls: `${ids}-${x.value}-panel` }))} />

      {((info && (info.refusal || (info.state !== 'ready' && info.message))) || calls || closing.error !== null || !!closing.unknown?.length || (info && detail.error != null)) && (
        <Notice tone="danger">
          {info && info.state !== 'ready' && info.message && <p>{t('wsUnavailable', { reason: info.message })}{logLink ? <> · {logLink}</> : null}</p>}
          {info?.refusal && <Refusal error={info.refusal} labels={refusal} compact />}
          {info && detail.error != null && <Refusal error={detail.error} labels={refusal} compact />}
          {calls && <p><code>{t('clearingCall', { status: calls.status, clear: calls.clear })}</code></p>}
          {closing.error !== null && <Refusal error={closing.error} labels={refusal} />}
          {!!closing.unknown?.length && <p>{t('forceClosedUnknown', { list: closing.unknown.join(', ') })}</p>}
        </Notice>
      )}

      {tab === 'overview' ? (
        <div className={logCss.tabPanel} role="tabpanel" id={`${ids}-overview-panel`} aria-labelledby={`${ids}-overview`}>
          <SandboxOverview name={name} machine={machine} machineTitle={machineWithKind} info={info} listed={listed}
            loading={detail.error == null} fallback={info ? <UsageLine info={info} /> : null}
            onMachine={() => go({ kind: 'machine', name: machine })} onWorkspaces={() => setTab('terminals')}
            icon={<SandboxGlyph size={22} />}
            badge={<StateChip tone={look.tone} live={look.live} dashed={look.dashed} label={stateWord(look)} op={op} title={op ? t(currentOp?.uncertain ? 'opTitleUncertain' : 'opTitle', { op }) : liveWhy ?? undefined} />}
            actions={(
              <>
                <DoctorAction api={api} t={doctorT} target="sandbox" name={name} label={doctorT('diagnose')} />
                <Button variant="outline" size="sm" icon={<IconRefreshOutlineRegular size={13} />} onClick={detail.reload}>{t('refresh')}</Button>
                <SandboxMenu name={name} op={currentOp} closing={closing} onDeleted={() => go({ kind: 'machine', name: machine })} />
              </>
            )} />
        </div>
      ) : tab === 'files' ? (
        // reads on its own: the Workspace rows take their names and chips from sandbox.get when it has answered
        <div className={logCss.tabPanel} role="tabpanel" id={`${ids}-files-panel`} aria-labelledby={`${ids}-files`}>
          <InspectFiles kind="sandbox" name={name} label={t('filesOf', { name })} workspaces={info?.workspaces ?? null} project={project}
            onOpenLog={hasLog ? () => setTab('log') : undefined} logTab={hasLog ? t('tabDaemonLog') : undefined} />
        </div>
      ) : tab === 'log' ? (
        <div className={logCss.tabPanel} role="tabpanel" id={`${ids}-log-panel`} aria-labelledby={`${ids}-log`}>
          <LogPane kind="sandbox" name={name} label={t('daemonLogOf', { name })} final={ended} />
        </div>
      ) : (
        <div className={logCss.tabPanel} role="tabpanel" id={`${ids}-terminals-panel`} aria-labelledby={`${ids}-terminals`} data-sandbox-terminals>
          {!info
            // only this tab waits on sandbox.get
            ? detail.error != null
              ? <LoadError title={t('readSandboxFailed')} error={detail.error} labels={refusal} retryLabel={t('retry')} onRetry={detail.reload} />
              : <Loading label={t('readingSandbox')} />
            : <SandboxTerminals info={info} onChanged={detail.reload} onOpenFiles={hasFiles ? () => setTab('files') : undefined} />}
        </div>
      )}
    </Page>
  )
}

/**
 * What the Sandbox's own cgroup has used (sandbox.get `usage`): CPU time,
 * memory now and at its peak, and its process count; each value LayerFS
 * could not read says so. Without a cgroup to read, the reason in plain
 * words. Nothing with a CLI that has no such field.
 */
function UsageLine({ info }: { info: SandboxInfo }): ReactNode {
  const { t, tw } = useMachinesContext()
  if (info.usage === undefined) return null
  const u = info.usage
  if (!u) {
    return (
      <p className={term.usageLine} data-sandbox-usage="none" data-usage-reason={info.usageReason ?? ''}>
        <span>{t('sbUsage')}</span>
        <span className={css.muted}>{t('sbUsageNone', { reason: tw(reasonWords(info.usageReason ?? 'not_observed')) })}</span>
      </p>
    )
  }
  const none = t('notObserved')
  // a usage with null counts and LayerFS's reason for them (`no_controller`): not measured on that machine, never a 0
  const why = info.usageReason ? tw(reasonWords(info.usageReason)) : null
  const noMemory = u.memoryCurrent == null && u.memoryPeak == null, noPids = u.pids == null
  const memory = <span data-usage-memory>· {t('sbUsageMemory', { now: u.memoryCurrent != null ? bytes(u.memoryCurrent) : none, peak: u.memoryPeak != null ? bytes(u.memoryPeak) : none })}</span>
  const pids = <span data-usage-pids>· {u.pids == null ? t('sbUsagePidsNone') : u.pids === 1 ? t('sbUsagePidsOne') : t('sbUsagePids', { n: String(u.pids) })}</span>
  return (
    <p className={term.usageLine} data-sandbox-usage="observed" data-usage-reason={info.usageReason ?? ''}>
      <span>{t('sbUsage')}</span>
      <span data-usage-cpu>{t('sbUsageCpu', { time: u.cpuUsec != null ? cpuTime(u.cpuUsec) : none })}</span>
      {why && noMemory && noPids ? <span className={css.muted} data-usage-not-measured="memory pids">· {t('sbUsageNotMeasured', { reason: why })}</span>
        : why && noMemory ? <><span className={css.muted} data-usage-not-measured="memory">· {t('sbUsageMemoryNotMeasured', { reason: why })}</span>{pids}</>
          : why && noPids ? <>{memory}<span className={css.muted} data-usage-not-measured="pids">· {t('sbUsagePidsNotMeasured', { reason: why })}</span></>
            : <>{memory}{pids}</>}
    </p>
  )
}
