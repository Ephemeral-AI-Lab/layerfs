/**
 * One machine's page: its tabs (MachineTabs.tsx), with startup progress and
 * notices on every tab. The head (name as the tree shows it, state, power,
 * menu) opens the Overview tab.
 * Overview (Overview.tsx) is a status line or what needs you, processor,
 * memory and disk tiles with their hour, what is on the machine and its
 * Details. Sandboxes and Stores list them all, with New Sandbox
 * and Add Store; where the disk goes is a dialog from the Overview
 * (SpaceDialog.tsx, only with machine stats --with space), Files (InspectFiles.tsx) only with machine.files, and
 * Worker log (LogPane.tsx) only with machine.log and a machine LayerFS says
 * has a worker log (`supports.log`).
 */
import { useEffect, useId, useState, type ReactNode } from 'react'
import { Button, IconSettingsOutlineRegular } from '@deepseek-ai/dsh-client-ui-primitives'
import { DoctorAction } from '../doctor/Doctor.tsx'
import { Empty, LinkButton, Notice, Page, PageHead, Refusal, refusalOf } from '../ui/index.tsx'
import { TabBar } from '../ui/TabBar.tsx'
import { DiskLine, attentionOf, blockedOf, glyphOs } from './Board.tsx'
import { useMachinesContext } from './context.tsx'
import { MachineGlyph } from './icons.tsx'
import { useDetailTab } from './detail-tabs.ts'
import { InspectFiles } from './InspectFiles.tsx'
import { sandboxOfRunFolder } from './inspect-files.ts'
import { LogPane } from './LogPane.tsx'
import type { View } from './context.tsx'
import { useMachineTitle } from './Inventory.tsx'
import { BUILT_IN, clearingCall, downReason, fixAction, gib, runsFix, machineState, recordOutdated, vmGone, runtimeOf, sandboxesOn, heldStores, supports, unreadableRecord, type AvailableView, type MachineInfo } from './model.ts'
import { MachineDialogs, MachineMenu, MachineProgress, MachineStateChip, PowerButton, hasSettings, useMachineRun } from './parts.tsx'
import { engineStart, osOfView, stepReasons } from './needs.ts'
import { FixPanel } from './Readiness.tsx'
import { closeFirstKey, inUseNow } from './notices.ts'
import { onSocketRequest, takeSocketRequest } from './socket.ts'
import { NewSandboxDialog } from './Sandboxes.tsx'
import { MachineOverview } from './Overview.tsx'
import { ObservedCpuMemory, useMachineStats, workerLine } from './MachineStats.tsx'
import { machineTabs, type MachineTab } from './machine-tabs.ts'
import { StoresTab } from './MachineTabs.tsx'
import { SpaceDialog } from './SpaceDialog.tsx'
import { ImagesTab } from './MachineImages.tsx'
import { SandboxesTab } from './SandboxCards.tsx'
import { useSpaceOffered } from './SpaceViews.tsx'
import { storeLines } from './Inventory.tsx'
import css from './machines.module.css'
import logCss from './log.module.css'

/** `onAddStore`: the page's Add Store for this machine (Storage's dialog); without it the Stores column has no action. `storesTab`: the Stores tab Storage draws (its Store cards and dialogs), else the plain list. */
export function MachineDetail({ name, go, onAddStore, storesTab }: { name: string, go: (view: View) => void, onAddStore?: ((machine: string) => void) | undefined, storesTab?: ((machine: string) => ReactNode) | undefined }): ReactNode {
  const { t, useMachines, refusal } = useMachinesContext()
  const title = useMachineTitle()
  const read = useMachines((s) => s)
  const view = read.data?.available ? read.data : null
  const m = view?.machines.find((x) => x.name === name)
  const label = title({ name, platform: m?.platform ?? (name === 'native' ? 'native' : '') })
  if (!view) {
    return (
      <Page wide label={label}>
        <PageHead title={label} />
        {read.status === 'error' ? <Refusal error={read.error} labels={refusal} /> : <p className={css.muted}>{t('readingMachines')}</p>}
      </Page>
    )
  }
  if (!m) {
    return (
      <Page wide label={label}>
        <PageHead title={label} />
        <Empty title={t('noSuchMachine', { name })}>{t('noSuchMachineBody')}</Empty>
      </Page>
    )
  }
  return <MachineView key={m.name} view={view} m={m} go={go} onAddStore={onAddStore} storesTab={storesTab} />
}

/** CPU and memory in words when nothing was observed: the size the record was created with, labelled "allocated" (keyed on what is there, never on the platform). */
function resourcesOf(m: MachineInfo, t: ReturnType<typeof useMachinesContext>['t']): string {
  if (m.cpus == null && m.memory == null) return t('notReported')
  const cpu = m.cpus != null ? t(m.cpus === 1 ? 'coresOne' : 'cores', { n: String(m.cpus) }) : t('notReported')
  return t('mxAllocated', { size: `${cpu} · ${m.memory != null ? gib(m.memory) : t('notReported')}` })
}

function MachineView({ view, m, go, onAddStore, storesTab }: { view: AvailableView, m: MachineInfo, go: (view: View) => void, onAddStore?: ((machine: string) => void) | undefined, storesTab?: ((machine: string) => ReactNode) | undefined }): ReactNode {
  const { t, tw, kind, api, doctorT, useStartups, refusal, stateWord } = useMachinesContext()
  const title = useMachineTitle()
  const startup = useStartups((all) => all[m.name])
  const { run, pending: localPending, error: lastError, clear: clearError } = useMachineRun()
  const pending = localPending ?? (startup?.status === 'running' ? startup.kind : null)
  const [dialog, setDialog] = useState<'settings' | 'remove' | 'force-stop' | null>(null)
  // "Choose another socket" elsewhere (Docker isn't answering): docker's page opens its socket setting
  useEffect(() => {
    if (!hasSettings(m)) return undefined
    const take = () => { if (takeSocketRequest()) setDialog('settings') }
    take()
    return onSocketRequest(take)
  }, [m.platform])
  const [creating, setCreating] = useState(false)
  const [spaceOpen, setSpaceOpen] = useState(false)
  const live = view.live[m.name]
  const state = machineState(m, live)
  const reason = startup?.status === 'running' ? null : downReason(m, live)
  const attention = startup?.status === 'running' ? null : attentionOf(m, view)
  const block = startup?.status === 'running' ? null : blockedOf(m, view)
  // only the Docker Engine is not running: Start launches it, then the machine (needs.ts engineStart)
  const engine = engineStart(m, block, osOfView(view))
  const power = !block || engine ? <PowerButton m={m} pending={pending} run={run} engine={engine} /> : undefined
  const calls = clearingCall('machine', m.name, state.op)
  // New Sandbox also works on a stopped machine (Start and create); it waits only while a power action runs
  const canSandbox = pending === null
  const name = title(m)
  // Worker log: the pinned CLI has the call, and LayerFS says this machine has a worker log (no platform test here)
  const hasLog = view.features?.inspect?.machineLog === true && supports(m, 'log') === true
  // Files: the pinned CLI has machine.files (which views it has is LayerFS's answer, never a platform test here)
  const hasFiles = view.features?.inspect?.machineFiles === true
  // observed values (machine.stats), read every 10 s while this page is on screen; without the call the facts are the record's, as before
  const observed = useMachineStats(m)
  const hasDisk = useSpaceOffered()
  // Images: Stage 1 machines keep Sandbox image copies (not macOS native, whose Sandboxes run on the Mac itself)
  const hasImages = view.features?.imageStage === 1 && m.backend !== 'native'
  const sandboxCount = sandboxesOn(view, m).length
  const storeCount = storeLines(view, m).length
  // the shared tab bar: labels never squeeze, a count beside its label; on a phone the bar scrolls with fades at its cut edges
  const labels: Record<MachineTab, string> = {
    overview: t('tabOverview'), sandboxes: t('sandboxes'), stores: t('stores'),
    images: t('tabImages'), files: t('tabFiles'), log: t('tabWorkerLog'),
  }
  const counts: Partial<Record<MachineTab, number>> = { sandboxes: sandboxCount, stores: storeCount }
  const tabs = machineTabs({ images: hasImages, files: hasFiles, log: hasLog }).map((value) => ({ value, label: labels[value] }))
  const [tab, setTab] = useDetailTab<MachineTab>('machine', m.name, tabs.map((x) => x.value), 'overview')
  const ids = useId()
  const worker = observed.offered ? workerLine(observed.read?.stats, t) : null
  const logLink = hasLog && tab !== 'log' ? <LinkButton onClick={() => setTab('log')}>{t('openWorkerLog')}</LinkButton> : null
  // a stop refused InUse because Sandboxes run: what to do in words, their names, and the Sandboxes tab as its one action (LayerFS's message is the tip)
  // a stop refused InUse follows the Sandbox list: the names still there, and no panel once they are all gone (UI pass 3 F7)
  const held = lastError !== null ? inUseNow(lastError, sandboxesOn(view, m).map((b) => b.name)) : null
  const error = held?.cleared ? null : lastError
  const inUse = held && !held.cleared && held.names.length ? held.names : null
  // once its Sandboxes are gone the refusal is spent: a new Sandbox of the same name does not bring the panel back
  useEffect(() => { if (held?.cleared) clearError() }, [held?.cleared])
  const refused = error === null ? null : inUse ? (
    <div className={css.inUse} data-in-use={inUse.length} title={refusalOf(error).message}>
      <p className={css.inUseTitle}>{t(closeFirstKey(inUse.length), { n: String(inUse.length) })}</p>
      <ul className={css.inUseList}>{inUse.map((x) => <li key={x} className={css.mono}>{x}</li>)}</ul>
      {tab !== 'sandboxes' && <Button size="sm" variant="outline" onClick={() => setTab('sandboxes')}>{t('inUseOpenSandboxes')}</Button>}
    </div>
  ) : <Refusal error={error} labels={refusal} />
  // startup progress and notices: on every tab, where today's page has them
  const notices = (
    <>
      <MachineProgress name={m.name} failedAction={logLink} />
      {block && (
        <Notice tone="warning">
          <p>{t('unavailableWhy', { kind: kind(m.platform), reason: stepReasons(block, tw) })}</p>
          {engine && <p>{t('engineStartHint')}</p>}
          {heldStores(m) > 0 && <p>{t(heldStores(m) === 1 ? 'storeBlockedOne' : 'storesBlocked', { n: String(heldStores(m)) })}</p>}
          <FixPanel fixes={block.fixes} className={css.blockedHelp} />
        </Notice>
      )}
      {(reason || error !== null || state.state === 'unresolved') && (
        <Notice tone={attention === 'danger' || (error !== null && !inUse) ? 'danger' : inUse ? 'warning' : undefined}>
          {/* why: the state's word with LayerFS's message as the tip; off the Overview (whose head has it) its fix is the one action */}
          {reason && (unreadableRecord(m) ? <p title={reason}>{t('recordUnreadable')}</p> : (
            <p className={css.reasonLine} data-reason-word>
              <span title={reason}>{stateWord(state)}</span>
              {m.platform !== 'docker' && tab !== 'overview' && fixAction(m) ? power : null}
            </p>
          ))}
          {state.state === 'unresolved' && <p><code>{t('clearingCall', { status: calls.status, clear: calls.clear })}</code></p>}
          {/* an outdated record no call repairs: its Stores wait for the remove, not for a worker to answer (UI pass 3 R FAIL 2) */}
          {heldStores(m) > 0 && attention === 'danger' && <p>{t(recordOutdated(m) || vmGone(m) ? (heldStores(m) === 1 ? 'storeBlockedRecordOne' : 'storesBlockedRecord') : heldStores(m) === 1 ? 'storeBlockedOne' : 'storesBlocked', { n: String(heldStores(m)) })}</p>}
          {refused}
          {/* the worker log only when LayerFS gave no fix the page runs (one action) */}
          {logLink && (reason || state.state === 'unresolved') && !runsFix(m) ? <p>{logLink}</p> : null}
        </Notice>
      )}
    </>
  )
  const docker = m.platform === 'docker'
  const machineHead = (
    <PageHead
      icon={<MachineGlyph platform={m.platform} os={glyphOs(m, view)} size={22} />}
      title={name}
      badge={<MachineStateChip m={m} live={live} pending={pending} blocked={engine ? 'engine' : !!block} />}
      actions={(
        <>
          {power}
          <MachineMenu m={m} pending={pending} run={run} onDialog={setDialog} />
        </>
      )}
    />
  )
  return (
    <Page wide label={name}>
      {docker && machineHead}
      <TabBar className={docker ? undefined : logCss.tabsTop} label={t('machineTabs', { name })} active={tab} onSelect={(key) => setTab(key as MachineTab)}
        tabs={tabs.map((x) => ({ key: x.value, label: x.label, count: counts[x.value], id: `${ids}-${x.value}`, controls: `${ids}-${x.value}-panel` }))} />

      {tab === 'files' ? (
        <div className={logCss.tabPanel} role="tabpanel" id={`${ids}-files-panel`} aria-labelledby={`${ids}-files`}>
          {notices}
          <InspectFiles kind="machine" name={m.name} label={t('filesOf', { name })} onOpenLog={hasLog ? () => setTab('log') : undefined} logTab={hasLog ? t('tabWorkerLog') : undefined}
            onOpenSandboxLog={view.features?.inspect?.sandboxLog === true ? (folder) => {
              // a daemon.log in a run folder: the Daemon log tab of the Sandbox that folder is of, when this machine has it
              const sandbox = sandboxOfRunFolder(folder)
              return sandbox && sandboxesOn(view, m).some((sb) => sb.name === sandbox) ? () => go({ kind: 'sandbox', machine: m.name, name: sandbox, tab: 'log' }) : null
            } : undefined}
            unavailableAction={state.state === 'stopped' || engine ? power : undefined} />
        </div>
      ) : tab === 'log' ? (
        <div className={logCss.tabPanel} role="tabpanel" id={`${ids}-log-panel`} aria-labelledby={`${ids}-log`}>
          {notices}
          <LogPane kind="machine" name={m.name} label={t('workerLogOf', { name })}
            unavailable={state.state === 'stopped' ? { message: t('logMachineStopped'), action: power } : null} />
        </div>
      ) : tab !== 'overview' ? (
        <div className={logCss.tabPanel} role="tabpanel" id={`${ids}-${tab}-panel`} aria-labelledby={`${ids}-${tab}`}>
          {notices}
          {tab === 'sandboxes' ? <SandboxesTab m={m} view={view} canCreate={canSandbox} onCreate={() => setCreating(true)} onImages={hasImages ? () => setTab('images') : undefined} />
            : tab === 'stores' ? (storesTab ? storesTab(m.name) : <StoresTab m={m} view={view} onAddStore={onAddStore} />)
            : <ImagesTab m={m} taken={view.sandboxes.map((b) => b.name)} canCreate={canSandbox} />}
        </div>
      ) : (
      <div className={logCss.tabPanel} role="tabpanel" id={`${ids}-overview-panel`} aria-labelledby={`${ids}-overview`}>
        {!docker && <div className={logCss.panelHead}>{machineHead}</div>}
        <MachineOverview m={m} view={view} live={live} pending={pending} onTab={setTab} onDisk={hasDisk ? () => setSpaceOpen(true) : null}
          troubled={!!(reason || block || error !== null || state.state === 'unresolved')}
          notices={notices}
          fallback={(
            // without the sampler's read (no machine.stats, or not read yet): the facts as the record has them
            <dl data-docker={docker || undefined} className={css.factRow} aria-label={t('resources')}>
              <div><dt>{t(observed.offered ? 'disk' : 'freeDisk')}</dt><dd><DiskLine m={m} view={view} /></dd></div>
              <div><dt>{t('cpuMemory')}</dt><dd>{observed.offered && observed.read?.stats ? <ObservedCpuMemory read={observed.read} /> : resourcesOf(m, t)}</dd></div>
              {live?.toolVersion && <div><dt>{t('toolVersionOf', { kind: kind(m.platform) })}</dt><dd className={css.mono}>{live.toolVersion}</dd></div>}
              {worker && <div><dt>{t('worker')}</dt><dd data-worker-line>{worker}</dd></div>}
            </dl>
          )}
          actions={(
            <>
              <DoctorAction api={api} t={doctorT} target="machine" name={m.name} label={doctorT('runDiagnostics')} />
              {BUILT_IN.has(m.platform) && hasSettings(m) && (
                <Button size="sm" variant="outline" icon={<IconSettingsOutlineRegular size={13} />} onClick={() => setDialog('settings')}>{t('updateSettings')}</Button>
              )}
            </>
          )}
          onRemove={() => setDialog('remove')} />
      </div>
      )}

      {spaceOpen && <SpaceDialog m={m} onClose={() => setSpaceOpen(false)} onTab={(x) => { setSpaceOpen(false); setTab(x) }} />}
      {creating && <NewSandboxDialog machine={m} taken={view.sandboxes.map((b) => b.name)} onClose={() => setCreating(false)} />}
      <MachineDialogs m={m} dialog={dialog} onClose={() => setDialog(null)} pending={pending} run={run} onRemoved={() => go({ kind: 'board' })} />
    </Page>
  )
}
