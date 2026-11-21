import type { ReactNode } from 'react'
import { Button, Modal } from '@deepseek-ai/dsh-client-ui-primitives'
import { useMachinesContext } from '../machines/context.tsx'
import { type AvailableView, type MachineInfo, machineState, supports } from '../machines/model.ts'
import { machineBlock } from '../machines/needs.ts'
import type { StorageT } from './locale.ts'
import type { ProjectSummary } from './types.ts'
import css from './Storage.module.css'

/** Mirror NewSandboxDialog's project filter. A stopped machine can start as part of creation. */
export function sandboxMachines(view: AvailableView, projects: readonly ProjectSummary[]): MachineInfo[] {
  const sqliteHosts = new Set(projects.filter((p) => p.kind === 'sqlite').map((p) => p.machine))
  const hasPostgres = projects.some((p) => p.kind === 'postgresql')
  return view.machines.filter((m) => (m.state === 'ready' || m.state === 'stopped') && !machineBlock(m, view) && (
    (sqliteHosts.has(m.name) && supports(m, 'sqlite') !== false) || (hasPostgres && supports(m, 'postgresql_sandbox') === true)
  ))
}

export function SandboxMachinePicker({ t, machines, reason, onPick, onClose }: {
  t: StorageT, machines: readonly MachineInfo[], reason: string | undefined, onPick: (m: MachineInfo) => void, onClose: () => void
}): ReactNode {
  const mc = useMachinesContext()
  return (
    <Modal open title={t('newSandbox')} description={t('chooseMachine')} closeLabel={t('close')} onClose={onClose} className={css.modal}>
      <div className={css.form}>
        {reason ? <p role="status">{reason}</p> : machines.map((m) => (
          <Button key={m.name} variant="outline" onClick={() => onPick(m)}>
            {m.name === 'native' ? t('groupHome') : m.name} · {mc.kind(m.platform)} · {mc.stateWord(machineState(m))}
          </Button>
        ))}
      </div>
    </Modal>
  )
}
