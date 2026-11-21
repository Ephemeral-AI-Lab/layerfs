/**
 * A platform's fixes as the page shows them (needs.ts): FixPanel, what to do
 * for each need with commands to copy and Check again; and Blocked, the line
 * a machine block shows when its platform stopped being ready here (Docker
 * quit, Lima removed): why it cannot run now and the one step that fixes it,
 * in place of a Start that would only fail. A missing Linux bundle is fixed
 * in the Set up LayerFS modal ("Fix in setup"); Docker's socket that does not
 * answer links to docker's socket setting when LayerFS found others.
 */
import { useState, type ReactNode } from 'react'
import { Button, IconChevronDownOutlineRegular } from '@deepseek-ai/dsh-client-ui-primitives'
import { setupDialog } from '../setup/store.ts'
import { CopyLine } from '../ui/index.tsx'
import { useMachinesContext } from './context.tsx'
import type { MachineInfo } from './model.ts'
import { STEP_WORDS, setupOf, stepReasons, type Fix, type Readiness } from './needs.ts'
import { requestSocketSetting } from './socket.ts'
import css from './machines.module.css'

const URL_LINE = /^https?:\/\//

/**
 * What to do for each need, in ladder order, with actions consolidated into a
 * single bar ordered shortest-label first to longest-label last.
 */
export function FixPanel({ fixes, className, onLeave }: { fixes: Fix[], className?: string | undefined, onLeave?: (() => void) | undefined }): ReactNode {
  const { t, tw, recheck, go } = useMachinesContext()
  const [checking, setChecking] = useState(false)
  const shown = fixes.filter((f) => f.step !== 'unsupported')
  const setupStep = shown.find((f) => f.setup)?.setup
  const hasSocket = shown.some((f) => f.socket)
  const buttons: { key: string, label: string, disabled?: boolean, onClick: () => void }[] = [
    { key: 'check', label: checking ? t('checking') : t('checkAgain'), disabled: checking, onClick: () => { setChecking(true); void recheck().finally(() => setChecking(false)) } },
    ...(setupStep ? [{ key: 'setup', label: t('fixInSetup'), onClick: () => { onLeave?.(); setupDialog.open(setupStep) } }] : []),
    ...(hasSocket ? [{ key: 'socket', label: t('chooseSocket'), onClick: () => { onLeave?.(); requestSocketSetting(); go({ kind: 'machine', name: 'docker' }) } }] : []),
  ].sort((a, b) => a.label.length - b.label.length)

  return (
    <div className={className ?? css.platformHelp}>
      {shown.map((f) => (
        <div key={f.need} className={css.fixStep}>
          <p>{f.how ? tw(f.how) : tw(f.why)}</p>
          {f.commands.map((line) => URL_LINE.test(line)
            ? <a key={line} href={line} target="_blank" rel="noreferrer" className={css.fixLink}>{line}</a>
            : <CopyLine key={line} text={line} copyLabel={t('copy')} copiedLabel={t('copied')} />)}
        </div>
      ))}
      <div className={css.fixActions}>
        {buttons.map((b) => (
          <Button key={b.key} size="sm" variant="outline" disabled={b.disabled} onClick={b.onClick}>
            {b.label}
          </Button>
        ))}
      </div>
    </div>
  )
}

/**
 * Why `m` cannot run now (its platform is not ready here) and the step that fixes it.
 * `action`: a control that fixes it by itself (Start that launches the Docker Engine first), shown in place of the step's how-to.
 */
export function Blocked({ m, block, action }: { m: MachineInfo, block: Readiness, action?: ReactNode }): ReactNode {
  const { t, tw, kind } = useMachinesContext()
  const [open, setOpen] = useState(false)
  const step = block.step === 'ready' || block.step === 'unsupported' ? null : STEP_WORDS[block.step]
  // a missing Linux bundle: the setup modal fixes it, so its button opens it
  const setup = setupOf(block)
  const reason = stepReasons(block, tw)
  return (
    <div className={css.blocked} data-step={block.step}>
      <div className={css.blockedLine}>
        <span className={css.blockReasonText} title={reason}>{t('unavailableWhy', { kind: kind(m.platform), reason })}</span>
        {action}
        {step && !action && setup && <Button size="sm" variant="outline" onClick={() => setupDialog.open(setup)}>{t(step.button)}</Button>}
        {step && !action && !setup && (
          <Button size="sm" variant="outline" aria-expanded={open} onClick={() => setOpen(!open)}>
            {t(step.button)}<IconChevronDownOutlineRegular size={12} className={open ? css.flip : undefined} />
          </Button>
        )}
      </div>
      {open && <FixPanel fixes={block.fixes} className={css.blockedHelp} />}
    </div>
  )
}
