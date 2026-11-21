import { useState, type ReactNode } from 'react'
import { Button, StateDot } from '@deepseek-ai/dsh-client-ui-primitives'
import { downReason, workerDown, type AvailableView, type MachineInfo, type MachineSandbox } from '../machines/model.ts'
import { machineBlock } from '../machines/needs.ts'
import { usePressGuard } from '../ui/index.tsx'
import type { StorageT } from './locale.ts'
import { blockedWords, brokenWords, doingWords, shareOf, storeStateWords, storeTip, type RowSignal } from './model.ts'
import { brokenBlock, MachineRepair, StoreFixAction } from './StoreCard.tsx'
import type { StorageJob, StorageView, StoreRow } from './types.ts'
import css from './HomeCards.module.css'

export type Place = (machine: string | null | undefined) => string

/** How often a project card asks again while its first size count runs. */
export const SIZE_POLL_MS = 3000

/** A Store's trouble, short; `detail` is LayerFS's full words, `restart` the machine whose worker would fix it, `store` the broken Store whose fix is the one action. */
export type Trouble = RowSignal & { detail?: string, restart?: MachineInfo, store?: StoreRow, repair?: { machine: string, record: MachineInfo | null } }

export function storeTrouble(s: StoreRow, view: StorageView, t: StorageT, place: Place, fleet?: AvailableView | null): Trouble | null {
  // its machine's worker does not answer: say that, and offer the fix, instead of LayerFS's long refusal
  const host = s.kind === 'sqlite' ? fleet?.machines.find((m) => m.name === s.machine) : undefined
  // its machine cannot run here at all (Docker quit, Lima removed): say so; a restart would only fail
  if (host && fleet && machineBlock(host, fleet)) return { state: 'warning', reason: t('machineUnavailable', { machine: place(host.name) }) }
  if (host && workerDown(host, fleet?.live[host.name])) {
    return { state: 'error', reason: t('ovStoreWorkerDown', { machine: place(host.name) }), detail: downReason(host, fleet?.live[host.name]) ?? undefined, restart: host }
  }
  // the six-state word and its reason; `detail` is LayerFS's sentence (else its fix line), the fix the one action
  if (s.state === 'broken') {
    const tip = storeTip(s, t)
    return { state: 'error', reason: brokenWords(s, t), ...(tip ? { detail: tip } : {}), store: s }
  }
  // blocked by a broken machine (F3): the word, that machine's sentence as the tip, its fix (else Repair <m>) as the one action
  const repair = brokenBlock(s)
  if (repair) {
    const record = fleet?.machines.find((m) => m.name === repair) ?? null
    const tip = record?.message ?? record?.fix ?? s.blockedBy?.reason
    return { state: 'warning', reason: blockedWords(s.blockedBy!, t, place), ...(tip ? { detail: tip } : {}), repair: { machine: repair, record } }
  }
  if (s.state === 'blocked') return { state: 'warning', reason: s.blockedBy?.machine ? blockedWords(s.blockedBy, t, place) : storeStateWords(s.state, t, s.blockedBy?.reason) }
  if (s.state === 'unknown') return { state: 'warning', reason: storeStateWords(s.state, t) }
  if (s.kind === 'sqlite' && view.machines.find((m) => m.name === s.machine)?.state === 'stopped') return { state: 'warning', reason: t('machineStopped', { machine: place(s.machine) }) }
  return null
}

/** A trouble's one action: Restart worker, else the broken Store's fix where Center runs it. */
export function troubleAction(trouble: Trouble | null, t: StorageT, onRestart?: (m: MachineInfo) => Promise<unknown>): ReactNode {
  if (trouble?.restart && onRestart) return <RestartButton m={trouble.restart} t={t} onRestart={onRestart} />
  if (trouble?.repair) return <MachineRepair machine={trouble.repair.machine} record={trouble.repair.record} t={t} />
  return trouble?.store ? <StoreFixAction store={trouble.store} t={t} /> : undefined
}

/** Restart <machine>'s worker inside a signal's hover card (a press that names the machine it starts): busy while it runs, LayerFS's refusal as its tooltip. */
export function RestartButton({ m, t, onRestart }: { m: MachineInfo, t: StorageT, onRestart: (m: MachineInfo) => Promise<unknown> }): ReactNode {
  const [busy, setBusy] = useState(false)
  const [failed, setFailed] = useState<string | null>(null)
  // click safety (UI pass 3 F6): keyed to this machine, dropped while the card has just appeared or changed under the pointer
  const guard = usePressGuard(`restart:${m.name}:${m.state}:${m.reason ?? ''}`)
  return (
    <Button variant="outline" size="sm" disabled={busy || guard.settling} aria-disabled={guard.settling || undefined} title={failed ?? t('ovRestartWorkerTitle', { machine: m.name })} data-machine={m.name}
      onClick={guard(() => { setBusy(true); setFailed(null); void onRestart(m).catch((e: unknown) => setFailed(String((e as Error)?.message ?? e))).finally(() => setBusy(false)) })}>
      {busy ? t('ovRestarting') : failed ? t('ovRestartFailed') : t('ovRestartWorker', { machine: m.name })}
    </Button>
  )
}

/** A running push, pull, move, backup or restore, with its progress. */
export function JobLine({ job, t, label }: { job: StorageJob, t: StorageT, label?: string }): ReactNode {
  const share = Math.round(shareOf(job) * 100)
  return (
    <span className={css.job}>
      <span className={css.jobLine}><StateDot state="ongoing" size={10} /><span className={css.statusText}>{label ?? doingWords(job, t)}</span><span className={css.num}>{`${share}%`}</span></span>
      <span className={css.bar}><span style={{ width: `${share}%` }} /></span>
    </span>
  )
}

export function sandboxesOfProject(sandboxes: readonly MachineSandbox[], stores: readonly StoreRow[], store: string, project: string): MachineSandbox[] {
  const unique = stores.filter((x) => x.projects.includes(project)).length === 1
  return sandboxes.filter((b) => b.project === `${store}/${project}` || (unique && b.project === project))
}
