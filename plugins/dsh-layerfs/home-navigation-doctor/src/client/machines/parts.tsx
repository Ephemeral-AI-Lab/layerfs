/**
 * What a machine's card and its page share: its state chip, Start or Stop,
 * the `…` menu (Open, power, Update settings… for the built-ins, Remove… for
 * VMs, refused while it holds Stores or Sandboxes), the settings and remove
 * dialogs, and the startup progress of a create or start.
 */
import { useContext, useEffect, useMemo, useState, type ReactNode, type SyntheticEvent } from 'react'
import { Button, IconCheckOutlineRegular, IconChevronRightOutlineRegular, IconSettingsOutlineRegular, IconTrashOutlineRegular, IconWarningOutlineRegular, Input, MenuItemButton, Modal, StateDot } from '@deepseek-ai/dsh-client-ui-primitives'
import { CopyLine, Field, FixAction, FixContext, MoreMenu, Refusal, StateChip, Contained, usePressGuard } from '../ui/index.tsx'
import { runnableFix } from '../../shared/doctor.ts'
import { useMachinesContext } from './context.tsx'
import type { MachineAction } from './controller.ts'
import { PowerGlyph } from './icons.tsx'
import type { MachinesKey } from './locale.ts'
import { BUILT_IN, duration, fixAction, fixRemoves, forceStopOf, machineState, opWords, powerOf, stageWords, unreadableRecord, workerDown, type MachineInfo, type MachineLive } from './model.ts'
import { chipOpWords } from './notices.ts'
import { socketSetting, socketSourceWords, type SocketChoice, type SocketSetting } from './socket.ts'
import css from './machines.module.css'
import { clockTime } from '../ui/locale-sync.ts'

/**
 * Runs a machine action, one at a time: `pending` names it; LayerFS's refusal
 * is kept to show where the hook lives, or, with `{ rethrow: true }`, thrown to
 * the caller (a dialog) to show where it was asked.
 */
export type Run = (key: string, action: MachineAction, options?: { rethrow?: boolean }) => Promise<boolean>

export function useMachineRun(): { run: Run, pending: string | null, error: unknown, clear: () => void } {
  const { act } = useMachinesContext()
  const [error, setError] = useState<unknown>(null)
  const [pending, setPending] = useState<string | null>(null)
  const run: Run = async (key, action, options) => {
    setPending(key)
    setError(null)
    try {
      await act(action)
      return true
    } catch (e) {
      if (options?.rethrow) throw e
      setError(e)
      return false
    } finally {
      setPending(null)
    }
  }
  return { run, pending, error, clear: () => setError(null) }
}

/** The machine's state as the SDK gives it, or `Starting…` while a start runs here. */
/**
 * `blocked`: its platform is not ready here (needs.ts blockOf), so it cannot run whatever its record says;
 * `'engine'` when only the Docker Engine is not running, which Start launches (needs.ts engineStart).
 */
export function MachineStateChip({ m, live, pending, blocked }: { m: MachineInfo, live?: MachineLive | undefined, pending: string | null, blocked?: boolean | 'engine' | undefined }): ReactNode {
  const { t, stateWord } = useMachinesContext()
  if (pending === 'start') return <StateChip tone="warning" live label={t(m.platform === 'docker' ? 'startingWorker' : 'starting')} />
  if (blocked === 'engine') return <StateChip tone="neutral" dashed label={t('stateEngineDown')} />
  if (blocked) return <StateChip tone="warning" label={t('stateUnavailable')} />
  const look = machineState(m, live)
  // the op once: "Stopping" for a busy stop, its phase only when it adds something
  const op = chipOpWords(look)
  const full = opWords(look.op)
  return (
    <StateChip tone={look.tone} live={look.live} dashed={look.dashed} label={stateWord(look)} op={op}
      title={full ? t(look.op?.uncertain ? 'opTitleUncertain' : 'opTitle', { op: full }) : undefined} />
  )
}

/** The power words for a machine; `engine`: its Start launches the Docker Engine first. */
function powerKey(m: MachineInfo, power: 'start' | 'stop', engine = false) {
  if (engine && power === 'start') return 'startEngine'
  return m.platform === 'docker' ? (power === 'stop' ? 'stopWorker' : 'startWorker') : (power === 'stop' ? 'stop' : 'start')
}

/** Start or Stop, when the machine is non-native and running or stopped. */
/** `compact`: an icon button (the board's rows) while nothing runs; the words come back for a start, a stop or a worker to restart. */
/** `engine`: the Docker Engine is not running and this Start launches it first (needs.ts engineStart). */
export function PowerButton({ m, pending, run, size = 'sm', compact, engine = false }: { m: MachineInfo, pending: string | null, run: Run, size?: 'sm' | 'md', compact?: boolean, engine?: boolean }): ReactNode {
  const { t, recheck, refusal } = useMachinesContext()
  const fixes = useContext(FixContext)
  const fix = engine ? null : fixAction(m)
  const power = engine || fix?.kind === 'engine' ? 'start' : powerOf(m)
  const elapsed = useElapsed(pending === 'stop')
  // the board's icon Stop asks a second press that names what it stops, like Home's head card (a stray click never stops a machine);
  // armed for this machine's state only, and for a few seconds (not ended on blur: the settle after arming disables, so blurs, the button)
  const armKey = `${m.name}:${m.state}:${power ?? ''}:${pending ?? ''}`
  const [armedFor, setArmedFor] = useState<string | null>(null)
  const armed = armedFor === armKey
  useEffect(() => {
    if (!armed) return
    const timer = setTimeout(() => setArmedFor(null), STOP_ARMED_MS)
    return () => clearTimeout(timer)
  }, [armed])
  // click safety (UI pass 3 F6): a press is keyed to this machine and what the button does now; one that lands
  // while the button has just appeared or changed (a re-render moved it under the pointer, or arming) does nothing
  const guard = usePressGuard(`${m.name}:${m.state}:${m.reason ?? ''}:${fix?.kind ?? power ?? ''}:${pending ?? ''}:${armed ? 'armed' : ''}`)
  const named = (action: string) => t('powerFor', { action, name: m.name })
  // a record with no vm_name: Remove its record (a confirm press), or LayerFS's chain (its Stores, then the record) step by step,
  // where Center runs fixes; else the line stays the tip to copy
  if (fix?.kind === 'remove' || fix?.kind === 'chain') {
    const runnable = fixes && m.fix ? runnableFix(m.fix) : null
    return runnable && fixes ? <span onClick={(e) => e.stopPropagation()}><FixAction line={m.fix!} fix={runnable} runner={fixes} labels={refusal} /></span> : null
  }
  // a broken or unknown machine: its one action, from its `fix` (UI rule: one action; a line the page does not run stays to copy in its detail)
  if (fix && fix.kind !== 'engine' && fix.kind !== 'copy') {
    const busy = pending !== null
    const words = pending === 'start' || pending === 'restart' ? t('starting') : pending === 'stop' ? t('stoppingFor', { s: String(elapsed) }) : fix.kind === 'check' ? t('fixCheck') : t('restartWorker')
    const title = fix.kind === 'forced' ? t(fix.sandboxes === 1 ? 'fixForcedOneTip' : 'fixForcedTip', { n: String(fix.sandboxes) })
      : fix.kind === 'check' ? t('fixCheckTitle', { name: m.name }) : m.fix ? t('fixCopyTitle', { fix: m.fix }) : t('restartWorkerTitle', { name: m.name })
    const act = () => {
      if (fix.kind === 'check') { void recheck(); return }
      if (fix.kind === 'restart') void run('restart', { kind: 'restart', name: m.name, platform: m.platform })
      else if (fix.kind === 'start') void run('start', { kind: 'start', name: m.name, platform: m.platform })
    }
    return (
      <Button size={size} variant={fix.kind === 'check' ? 'outline' : 'primary'} disabled={busy || fix.kind === 'forced' || guard.settling} aria-disabled={guard.settling || undefined} icon={<PowerGlyph size={14} />} title={title}
        aria-label={named(words)} data-machine={m.name} onClick={guard((e) => { e.stopPropagation(); act() })}>
        {words}
      </Button>
    )
  }
  if (!power) return null
  if (fix?.kind === 'engine') engine = true
  if (compact && pending === null) {
    const words = t(powerKey(m, power, engine))
    if (power === 'stop' && armed) {
      const confirm = t('powerConfirm', { action: words })
      return (
        <Button size={size} variant="primary" icon={<PowerGlyph size={14} />} aria-label={named(confirm)} data-machine={m.name} data-armed="" disabled={guard.settling} aria-disabled={guard.settling || undefined}
          onClick={guard((e) => { e.stopPropagation(); setArmedFor(null); void run('stop', { kind: 'stop', name: m.name }) })}>
          {confirm}
        </Button>
      )
    }
    return (
      <Button size={size} variant="ghost" icon={<PowerGlyph size={14} />} aria-label={named(words)} data-machine={m.name} disabled={guard.settling} aria-disabled={guard.settling || undefined}
        title={`${words} · ${t(engine ? 'engineStartTitle' : power === 'stop' ? 'powerRunningTitle' : 'powerStoppedTitle', { name: m.name })}`}
        onClick={guard((e) => { e.stopPropagation(); if (power === 'stop') setArmedFor(armKey); else void run(power, { kind: 'start', name: m.name, platform: m.platform }) })} />
    )
  }
  const label = pending === 'start' ? t('starting') : pending === 'stop' ? t('stoppingFor', { s: String(elapsed) }) : t(powerKey(m, power, engine))
  return (
    <Button size={size} variant={power === 'stop' ? 'outline' : 'primary'} disabled={pending !== null || guard.settling} aria-disabled={guard.settling || undefined} icon={<PowerGlyph size={14} />}
      title={t(engine ? 'engineStartTitle' : power === 'stop' ? 'powerRunningTitle' : 'powerStoppedTitle', { name: m.name })}
      aria-label={named(label)} data-machine={m.name}
      onClick={guard((e) => { e.stopPropagation(); void run(power, power === 'start' ? { kind: 'start', name: m.name, platform: m.platform } : { kind: 'stop', name: m.name }) })}>
      {label}
    </Button>
  )
}

/** How long the board's icon Stop waits for its second press. */
export const STOP_ARMED_MS = 4000

/** Seconds since `on` became true. */
function useElapsed(on: boolean): number {
  const [since, setSince] = useState<number | null>(null)
  const [now, setNow] = useState(Date.now)
  useEffect(() => {
    if (!on) { setSince(null); return }
    setSince(Date.now())
    const timer = setInterval(() => setNow(Date.now()), 1000)
    return () => clearInterval(timer)
  }, [on])
  return since === null ? 0 : Math.max(0, Math.round((now - since) / 1000))
}

/** The `…` menu: Open (on the board), power, Update settings… (docker) and Remove… (VMs). */
export function MachineMenu({ m, pending, run, onOpen, onDialog }: { m: MachineInfo, pending: string | null, run: Run, onOpen?: () => void, onDialog: (d: 'settings' | 'remove' | 'force-stop') => void }): ReactNode {
  const { t, useMachines } = useMachinesContext()
  const hasSandboxes = useMachines((snapshot) => m.sandboxes.length > 0 || !!(snapshot.data?.available && snapshot.data.sandboxes.some((sandbox) => sandbox.machine === m.name)))
  // only a CLI whose machine.stop takes force offers Force stop
  const features = useMachines((snapshot) => (snapshot.data?.available ? snapshot.data.features : undefined))
  const power = powerOf(m)
  const builtIn = BUILT_IN.has(m.platform)
  const down = workerDown(m)
  const fix = fixAction(m)
  // its power items behind the press guard, like every power control: a press on an item that just changed under the pointer does nothing
  const guard = usePressGuard(`menu:${m.name}:${m.state}:${m.reason ?? ''}:${pending ?? ''}`)
  const guarded = (act: () => void) => () => guard(act)({ stopPropagation: () => {} } as unknown as SyntheticEvent)
  return (
    <MoreMenu label={t('moreFor', { name: m.name })}>
      {(close) => (
        <>
          {onOpen && <MenuItemButton icon={<IconChevronRightOutlineRegular size={14} />} onSelect={() => { close(); onOpen() }}>{t('open')}</MenuItemButton>}
          {down ? (
            <>
              <MenuItemButton icon={<PowerGlyph size={14} />} disabled={pending !== null || fix?.kind === 'forced' || guard.settling}
                disabledReason={fix?.kind === 'forced' ? t(fix.sandboxes === 1 ? 'fixForcedOneTip' : 'fixForcedTip', { n: String(fix.sandboxes) }) : undefined}
                onSelect={guarded(() => { close(); void run(fix?.kind === 'restart' ? 'restart' : 'start', { kind: fix?.kind === 'restart' ? 'restart' : 'start', name: m.name, platform: m.platform }) })}>
                {t('powerFor', { action: t('restartWorker'), name: m.name })}
              </MenuItemButton>
              <MenuItemButton icon={<PowerGlyph size={14} />} disabled={pending !== null || guard.settling}
                onSelect={guarded(() => { close(); void run('stop', { kind: 'stop', name: m.name }) })}>
                {t('powerFor', { action: t(m.platform === 'docker' ? 'stopWorker' : 'stopUnreachable'), name: m.name })}
              </MenuItemButton>
            </>
          ) : power && (
            <MenuItemButton icon={<PowerGlyph size={14} />} disabled={pending !== null || guard.settling}
              onSelect={guarded(() => { close(); void run(power, power === 'start' ? { kind: 'start', name: m.name, platform: m.platform } : { kind: 'stop', name: m.name }) })}>
              {t('powerFor', { action: t(powerKey(m, power)), name: m.name })}
            </MenuItemButton>
          )}
          {forceStopOf(m, hasSandboxes, features) && (
            <MenuItemButton danger icon={<PowerGlyph size={14} />} disabled={pending !== null}
              onSelect={() => { close(); onDialog('force-stop') }}>
              {t('forceStop')}
            </MenuItemButton>
          )}
          {builtIn && !fixRemoves(m)
            ? (
              <>
                {hasSettings(m) && <MenuItemButton icon={<IconSettingsOutlineRegular size={14} />} onSelect={() => { close(); onDialog('settings') }}>{t('updateSettings')}</MenuItemButton>}
                <MenuItemButton separatorBefore disabled disabledReason={t('builtInNoRemove')} icon={<IconTrashOutlineRegular size={14} />} onSelect={close}>{t('remove')}</MenuItemButton>
              </>
            )
            : (
              <>
                {/* LayerFS decides whether a built-in goes: its fix `machine remove <m>` (a record with no vm_name) offers Remove (U8) */}
                {builtIn && hasSettings(m) && <MenuItemButton icon={<IconSettingsOutlineRegular size={14} />} onSelect={() => { close(); onDialog('settings') }}>{t('updateSettings')}</MenuItemButton>}
                {/* a machine that still holds Stores or Sandboxes gets the guided Remove (RemoveDialog) */}
                {/* LayerFS refuses to remove a machine whose record it cannot read: the Doctor says what is wrong first */}
                <MenuItemButton separatorBefore danger icon={<IconTrashOutlineRegular size={14} />} disabled={pending !== null || unreadableRecord(m)}
                  disabledReason={unreadableRecord(m) ? t('recordUnreadableNoRemove') : pending !== null ? t('removeWaitPending') : undefined}
                  onSelect={() => { close(); onDialog('remove') }}>
                  {t('remove')}
                </MenuItemButton>
              </>
            )}
        </>
      )}
    </MoreMenu>
  )
}

/** Whether a machine has settings to update: the docker machine's socket and WSL distribution. */
export const hasSettings = (m: MachineInfo): boolean => m.platform === 'docker'

/** The menu's dialogs: the docker machine's settings, or removing a VM. */
export function MachineDialogs({ m, dialog, onClose, pending, run, onRemoved }: { m: MachineInfo, dialog: 'settings' | 'remove' | 'force-stop' | null, onClose: () => void, pending: string | null, run: Run, onRemoved?: () => void }): ReactNode {
  // a press inside the dialog stays there: never the row's toggle or a button under it (UI pass 3 F6)
  if (dialog === 'settings' && hasSettings(m)) return <Contained><SettingsDialog m={m} onClose={onClose} pending={pending} run={run} /></Contained>
  if (dialog === 'force-stop') return <Contained><ForceStopDialog m={m} onClose={onClose} pending={pending} run={run} /></Contained>
  if (dialog === 'remove') return <Contained><RemoveDialog m={m} onClose={onClose} pending={pending} run={run} onRemoved={onRemoved} /></Contained>
  return null
}

/** Force stop is an explicit escalation; normal Stop keeps its refusal behavior. */
function ForceStopDialog({ m, onClose, pending, run }: { m: MachineInfo, onClose: () => void, pending: string | null, run: Run }): ReactNode {
  const { t } = useMachinesContext()
  // the confirm press settles from when the dialog opens: a double click on Force stop… does not pass through it
  const guard = usePressGuard(`force-stop:${m.name}:${m.state}`)
  return (
    <Modal open onClose={() => { if (pending === null) onClose() }} title={t('forceStopTitle', { name: m.name })}
      description={t(m.platform === 'native' ? 'forceStopNativeBody' : 'forceStopBody', { name: m.name })} closeLabel={t('close')} className={css.dialog}
      footer={(
        <>
          <Button variant="outline" disabled={pending !== null} onClick={onClose}>{t('cancel')}</Button>
          <Button variant="primary" className={css.danger} disabled={pending !== null || guard.settling} aria-disabled={guard.settling || undefined}
            onClick={guard(() => void run('stop', { kind: 'stop', name: m.name, force: true }).then(() => onClose()))}>
            {pending === 'stop' ? t('forceStopping') : t('forceStopAction')}
          </Button>
        </>
      )} />
  )
}

/**
 * The docker machine's settings (machine.update): its Docker socket and WSL
 * distribution. With a CLI that lists the sockets it found (`candidates`), the
 * socket is chosen from them or left Automatic (`docker: ''` clears a recorded
 * one: LayerFS chooses again at the next start); an older CLI keeps the path
 * field.
 */
function SettingsDialog({ m, onClose, pending, run }: { m: MachineInfo, onClose: () => void, pending: string | null, run: Run }): ReactNode {
  const { t, refusal, useMachines } = useMachinesContext()
  const row = useMachines((snapshot) => (snapshot.data?.available ? snapshot.data.platforms.find((p) => p.platform === m.platform) : undefined))
  const setting = useMemo(() => socketSetting(row, m.docker), [row, m.docker])
  const [socket, setSocket] = useState(m.docker ?? '')
  const [choice, setChoice] = useState<string | null>(null)
  const [wsl, setWsl] = useState(m.wsl ?? '')
  const [error, setError] = useState<unknown>(null)
  const chosen = choice ?? setting?.current ?? ''
  const save = async () => {
    if (pending !== null) return
    setError(null)
    // the choice list sends a change only when it moved ('' clears the recorded socket); the path field as before
    const docker = setting ? (chosen !== setting.current ? chosen : undefined) : socket || undefined
    try {
      const ok = await run('update', { kind: 'update', name: m.name, changes: { docker, wsl: wsl || undefined } }, { rethrow: true })
      if (ok) onClose()
    } catch (e) { setError(e) }
  }
  return (
    <Modal open onClose={() => { if (pending === null) onClose() }} title={t('settingsTitle')} closeLabel={t('close')} className={css.dialog}
      footer={(
        <>
          <Button variant="outline" disabled={pending !== null} onClick={onClose}>{t('cancel')}</Button>
          <Button variant="primary" disabled={pending !== null} onClick={() => void save()}>{pending === 'update' ? t('saving') : t('save')}</Button>
        </>
      )}>
      <form className={css.form} onSubmit={(e) => { e.preventDefault(); void save() }}>
        {setting
          ? <SocketChoices setting={setting} value={chosen} onChange={setChoice} disabled={pending !== null} />
          : (
            <Field label={t('dockerSocket')}>
              <Input value={socket} onChange={(e) => setSocket(e.target.value)} placeholder="/var/run/docker.sock" spellCheck={false} />
            </Field>
          )}
        {m.wsl !== undefined && (
          <Field label={t('wslDistribution')}>
            <Input value={wsl} onChange={(e) => setWsl(e.target.value)} placeholder="Ubuntu-24.04" spellCheck={false} />
          </Field>
        )}
        {error != null && <Refusal error={error} labels={refusal} className={css.errorText} />}
      </form>
    </Modal>
  )
}

const SOCKET_STATE: Record<string, MachinesKey> = { answering: 'socketAnswering', absent: 'socketAbsent', denied: 'socketDenied', silent: 'socketSilent' }

/**
 * docker's socket: Automatic (naming the socket in use while none is
 * recorded) or one of the sockets LayerFS found; one that is not a local
 * socket is shown, disabled, with why. A socket this user may not open gets
 * the docker group fix.
 */
function SocketChoices({ setting, value, onChange, disabled }: { setting: SocketSetting, value: string, onChange: (value: string) => void, disabled: boolean }): ReactNode {
  const { t, tw } = useMachinesContext()
  const source = (s: string | null | undefined) => { const words = socketSourceWords(s); return words ? tw(words) : null }
  const inUse = setting.inUse
  const stateOf = (path: string | null) => (inUse?.path && path === inUse.path && inUse.state && SOCKET_STATE[inUse.state] ? t(SOCKET_STATE[inUse.state]!) : null)
  const automaticMeta = !setting.recorded && inUse?.path
    ? [t('socketAutomaticUses', { path: inUse.path }), source(inUse.source), stateOf(inUse.path)].filter(Boolean).join(' · ')
    : t('socketAutomaticNext')
  const row = (key: string, c: Pick<SocketChoice, 'value' | 'usable'>, main: ReactNode, meta: string | null) => (
    <button key={key} type="button" role="radio" aria-checked={value === c.value && c.usable} disabled={disabled || !c.usable}
      className={css.socketRow} onClick={() => onChange(c.value)}>
      <span className={css.socketRadio} aria-hidden="true" />
      <span className={css.socketText}>
        {main}
        {meta ? <span className={css.socketMeta}>{meta}</span> : null}
      </span>
    </button>
  )
  return (
    <Field label={t('dockerSocket')} group>
      <div className={css.socketList} role="radiogroup" aria-label={t('dockerSocket')}>
        {row('automatic', { value: '', usable: true }, <span>{t('socketAutomatic')}</span>, automaticMeta)}
        {setting.choices.map((c, i) => row(`${i}:${c.path ?? c.source ?? ''}`, c, <span className={css.socketPath} title={c.path ?? undefined}>{c.path ?? c.host ?? t('socketNotLocal')}</span>,
          [source(c.source), c.usable ? stateOf(c.path) : t('socketNotUsable')].filter(Boolean).join(' · ') || null))}
      </div>
      <div className={css.socketNote}>
        {inUse?.state === 'denied' && (
          <>
            <p>{t('socketDeniedFix', { path: inUse.path ?? '' })}</p>
            <CopyLine text="sudo usermod -aG docker $USER" copyLabel={t('copy')} copiedLabel={t('copied')} />
            <p>{t('socketDeniedThen')}</p>
          </>
        )}
        {setting.recorded && <p className={css.muted}>{t('socketMoveNote')}</p>}
      </div>
    </Field>
  )
}

/** Remove a VM and its disk; LayerFS's refusal shows in the dialog, where it was asked. */
function RemoveDialog({ m, onClose, pending, run, onRemoved }: { m: MachineInfo, onClose: () => void, pending: string | null, run: Run, onRemoved?: () => void }): ReactNode {
  const { t, refusal, useMachines, closeSandbox, removeStore } = useMachinesContext()
  const data = useMachines((snapshot) => (snapshot.data?.available ? snapshot.data : null))
  // LayerFS's own fix `machine remove <m>` (a record with no vm_name): only the record goes, so nothing it holds is closed first
  const recordOnly = fixAction(m)?.kind === 'remove'
  // What the machine still holds: LayerFS refuses to remove it until they are gone.
  const held = useMemo(() => (recordOnly ? { stores: [], sandboxes: [] } : {
    stores: (data?.stores ?? []).filter((s) => s.machine === m.name || m.stores.includes(s.name)),
    sandboxes: (data?.sandboxes ?? []).filter((s) => s.machine === m.name),
  }), [data, m.name, m.stores, recordOnly])
  const occupied = held.stores.length > 0 || held.sandboxes.length > 0
  const [typed, setTyped] = useState('')
  const [step, setStep] = useState<string | null>(null)
  const [error, setError] = useState<{ step: string | null, error: unknown } | null>(null)
  const busy = pending !== null || step !== null
  const remove = async () => {
    if (busy) return
    setError(null)
    let at: string | null = null
    try {
      // in order: the Sandboxes (forced: a dead daemon cannot close cleanly), the Stores, then the machine
      for (const box of held.sandboxes) { at = t('removeStepSandbox', { name: box.name }); setStep(at); await closeSandbox(box.name, true) }
      for (const store of held.stores) { at = t('removeStepStore', { name: store.name }); setStep(at); await removeStore(store.name) }
      at = null; setStep(null)
      if (await run('remove', { kind: 'remove', name: m.name }, { rethrow: true })) { onClose(); onRemoved?.() }
    } catch (e) { setError({ step: at, error: e }) } finally { setStep(null) }
  }
  const confirmed = !occupied || typed === m.name
  return (
    <Modal open onClose={() => { if (!busy) onClose() }} title={t('removeTitle', { name: m.name })} description={recordOnly ? (m.stores.length ? t('removeRecordHeldBody', { name: m.name, stores: m.stores.join(', ') }) : t('removeRecordBody', { name: m.name })) : occupied ? t('removeHeldBody', { name: m.name }) : t('removeBody')} closeLabel={t('close')} className={css.dialog}
      footer={(
        <>
          <Button variant="outline" disabled={busy} onClick={onClose}>{t('cancel')}</Button>
          <Button variant="primary" className={css.danger} disabled={busy || !confirmed} onClick={() => void remove()}>
            {step ?? (pending === 'remove' ? t('removing') : occupied ? t('removeEverything') : t('remove'))}
          </Button>
        </>
      )}>
      {occupied ? (
        <div className={css.removeHeld}>
          {held.sandboxes.length ? (
            <section>
              <h3 className={css.removeHeldTitle}>{t('removeHeldSandboxes', { n: String(held.sandboxes.length) })}</h3>
              <ul className={css.removeHeldList}>{held.sandboxes.map((b) => <li key={b.name}><code>{b.name}</code>{b.project ? <span className={css.muted}> · {b.project}</span> : null}</li>)}</ul>
              <p className={css.muted}>{t('removeHeldSandboxesNote')}</p>
            </section>
          ) : null}
          {held.stores.length ? (
            <section>
              <h3 className={css.removeHeldTitle}>{t('removeHeldStores', { n: String(held.stores.length) })}</h3>
              <ul className={css.removeHeldList}>{held.stores.map((s) => (
                <li key={s.name}><code>{s.name}</code><span className={css.muted}> · {s.names?.length ? s.names.join(', ') : t('removeHeldProjects', { n: String(s.projects) })}</span></li>
              ))}</ul>
              <p className={css.muted}>{t('removeHeldStoresNote')}</p>
            </section>
          ) : null}
          <Field label={t('removeTypeName', { name: m.name })}>
            <Input value={typed} onChange={(e) => setTyped(e.target.value)} placeholder={m.name} autoComplete="off" spellCheck={false} disabled={busy} />
          </Field>
        </div>
      ) : null}
      {error != null && (
        <div className={css.errorText}>
          {error.step ? <p>{t('removeStoppedAt', { step: error.step })}</p> : null}
          <Refusal error={error.error} labels={refusal} />
        </div>
      )}
    </Modal>
  )
}

/**
 * A create or start as it was observed: the stage now, completed stages, the
 * elapsed time, and the reported error. Completion shows only after the call
 * answered; a lost reply says the outcome needs inspection.
 */
export function MachineProgress({ name, compact, failedAction }: { name: string, compact?: boolean, /** Shown under a failed or uncertain start (a link to the worker log). */ failedAction?: ReactNode }): ReactNode {
  const { t, tw, useStartups, refusal } = useMachinesContext()
  const operation = useStartups((all) => all[name])
  const running = operation?.status === 'running'
  const [now, setNow] = useState(Date.now)
  const [open, setOpen] = useState(false)
  useEffect(() => {
    if (!running) return
    setNow(Date.now())
    const timer = setInterval(() => setNow(Date.now()), 1000)
    return () => clearInterval(timer)
  }, [running, operation?.id])
  if (!operation) return null
  const { status, platform, stages } = operation
  const latest = stages.at(-1)
  const completed = stages.filter((s) => s.state === 'completed')
  const title = status === 'success' ? t('progressDone')
    : status === 'uncertain' ? t('progressUncertain')
      : status === 'failed' ? t('progressFailed')
        : operation.kind === 'create' ? t('progressCreating') : platform === 'docker' ? t('startingWorkerTitle') : t('startingMachine')
  const current = latest ? tw(stageWords(latest.stage, platform)) : t('progressPreparing')
  const elapsed = duration((operation.endedAt ?? now) - operation.startedAt)
  return (
    <section className={css.progress} data-compact={compact || undefined} data-status={status} aria-label={t('progressFor', { name })}>
      <div className={css.progressHead}>
        <span className={css.progressTitle} role="status" aria-live="polite">
          {running ? <StateDot state="ongoing" /> : status === 'success' ? <IconCheckOutlineRegular size={14} /> : <IconWarningOutlineRegular size={14} />}
          <span>{title}</span>
        </span>
        <span className={css.progressElapsed}>{elapsed}</span>
      </div>
      {status !== 'success' && (
        <p className={css.progressCurrent}>
          {current}{latest?.state === 'completed' ? ` · ${t('stageComplete')}` : latest?.state === 'failed' ? ` · ${t('stageFailed')}` : ''}
        </p>
      )}
      {status === 'uncertain' && <p className={css.progressNote}>{t('progressUncertainNote')}</p>}
      {status === 'failed' && <p className={css.progressNote}>{t('progressFailedNote')}</p>}
      {completed.length > 0 && !compact && (
        <>
          <button type="button" className={css.progressToggle} aria-expanded={open} onClick={() => setOpen((v) => !v)}>
            <IconChevronRightOutlineRegular size={12} className={css.progressChevron} />
            {t(completed.length === 1 ? 'stepsOne' : 'stepsMany', { n: String(completed.length) })}
          </button>
          {open && (
            <ul className={css.progressSteps}>
              {completed.map((s) => (
                <li key={s.stage}>
                  <IconCheckOutlineRegular size={12} />
                  <span>{tw(stageWords(s.stage, platform))}</span>
                  {s.elapsedMs != null && <span className={css.progressElapsed}>{duration(s.elapsedMs)}</span>}
                </li>
              ))}
            </ul>
          )}
        </>
      )}
      {(status === 'failed' || status === 'uncertain') && operation.error != null && !compact && <Refusal error={operation.error} labels={refusal} compact />}
      {(status === 'failed' || status === 'uncertain') && !compact && failedAction ? <p className={css.progressNote}>{failedAction}</p> : null}
    </section>
  )
}

/** "Observed 14:02: running" after an explicit inspect. */
export function useInspect(name: string): { inspection: string | null, inspect: () => Promise<void> } {
  const { t, inspectMachine } = useMachinesContext()
  const [inspection, setInspection] = useState<string | null>(null)
  const inspect = async () => {
    try {
      const answer = await inspectMachine(name)
      setInspection(t('observedAt', { time: clockTime(Date.now()), state: answer.state ?? t('stateUnknown') }))
    } catch (e) {
      setInspection(e instanceof Error ? e.message : String(e))
    }
  }
  return { inspection, inspect }
}

