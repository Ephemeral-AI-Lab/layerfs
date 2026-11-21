/**
 * dsh-layerfs shared page kit: the pieces LayerFS pages (Machines, Storage)
 * share so they read as native DSH pages. Every visible word arrives as a
 * prop from the caller's locale `t`; nothing here owns copy.
 */
import { createContext, useContext, useEffect, useRef, useState, type HTMLAttributes, type ReactNode, type SyntheticEvent } from 'react'
import clsx from 'clsx'
import {
  Button, IconCheckOutlineRegular, IconChevronDownOutlineRegular, IconCopyOutlineRegular, IconEllipsisOutlineRegular,
  IconRefreshOutlineRegular, IconWarningOutlineRegular, Menu, StateDot,
  type StateDotState,
} from '@deepseek-ai/dsh-client-ui-primitives'
import { removesSomething, runnableFix, typedNameOf, type RunnableFix } from '../../shared/doctor.ts'
import css from './ui.module.css'

export { css as uiCss }

// ---------- page shell ----------

/** The page's scroll container with its centered column. */
export function Page({ wide, children, label }: { wide?: boolean, children: ReactNode, label?: string }): ReactNode {
  return (
    <div className={css.page} data-wide={wide || undefined} role="region" aria-label={label}>
      <div className={css.column}>{children}</div>
    </div>
  )
}

/** One crumb step back. */
export interface CrumbStep {
  readonly label: string
  readonly onClick: () => void
  /** Accessible name, when the label alone is not enough ("Back to Machines"). */
  readonly ariaLabel?: string
}

/** The head every page shares: optional crumbs, the icon and title block, and actions at the end. */
export function PageHead({ title, intro, actions, crumbs, current, icon, badge, hideTitle }: {
  title: string
  /** The title is for assistive technology only (the tab or panel around the page already shows it). */
  hideTitle?: boolean
  intro?: ReactNode
  actions?: ReactNode
  /** Steps back, outermost first; `current` names the page itself after them. */
  crumbs?: readonly CrumbStep[]
  current?: string
  icon?: ReactNode
  /** Next to the title (a state chip). */
  badge?: ReactNode
}): ReactNode {
  return (
    <header className={css.head} data-window-drag>
      {/* The last crumb names the page; the title right below already does when they match. */}
      {crumbs && crumbs.length > 0 && <Crumbs steps={crumbs} current={current === title ? undefined : current} />}
      <div className={css.headRow}>
        <div className={css.heading}>
          {icon && <span className={css.headIcon} aria-hidden="true">{icon}</span>}
          <div className={css.headingText}>
            <div className={css.titleRow}>
              <h1 className={hideTitle ? css.srOnly : css.title} title={title}>{title}</h1>
              {badge}
            </div>
            {intro && <p className={css.intro}>{intro}</p>}
          </div>
        </div>
        {actions && <div className={css.toolbar}>{actions}</div>}
      </div>
    </header>
  )
}

/** Crumbs: each step leads back; the last names where the page is. */
export function Crumbs({ steps, current }: { steps: readonly CrumbStep[], current?: string }): ReactNode {
  return (
    <nav className={css.crumbs}>
      {steps.map((step, i) => (
        <span key={`${i}:${step.label}`} className={css.crumbs}>
          {i > 0 && <span className={css.crumbSep} aria-hidden="true">/</span>}
          <button type="button" className={css.crumb} aria-label={step.ariaLabel} onClick={step.onClick}>
            {i === 0 && <IconChevronDownOutlineRegular size={12} className={css.crumbIcon} aria-hidden="true" />}
            <span>{step.label}</span>
          </button>
        </span>
      ))}
      {current && <><span className={css.crumbSep} aria-hidden="true">/</span><span className={css.crumbCurrent}>{current}</span></>}
    </nav>
  )
}

/** A titled section with an optional count and actions at its end. */
export function Section({ title, count, actions, children, className, headless }: {
  title: string
  count?: ReactNode
  actions?: ReactNode
  children?: ReactNode
  className?: string
  /** Name the section for assistive technology only. */
  headless?: boolean
}): ReactNode {
  return (
    <section className={clsx(css.section, className)} aria-label={title}>
      {!headless && (
        <div className={css.sectionHead}>
          <h2 className={css.sectionTitle}>{title}</h2>
          {count != null && <span className={css.sectionCount}>{count}</span>}
          {actions && <div className={css.sectionActions}>{actions}</div>}
        </div>
      )}
      {children}
    </section>
  )
}

/** The settings-card surface (R20, card fill and hairline stroke). */
export function Card({ tone, className, children, ...rest }: {
  tone?: 'danger' | 'warning' | undefined
  className?: string
  children?: ReactNode
} & HTMLAttributes<HTMLDivElement>): ReactNode {
  return <div {...rest} className={clsx(css.card, className)} data-tone={tone}>{children}</div>
}

// ---------- facts ----------

/** One fact row: label at the start, value at the end. An empty value shows `empty`. */
export function Fact({ label, children, mono, tone, title, empty = '—' }: {
  label: string
  children?: ReactNode
  mono?: boolean
  tone?: 'danger' | 'muted' | undefined
  title?: string | undefined
  empty?: string
}): ReactNode {
  const none = children === null || children === undefined || children === '' || children === false
  return (
    <div className={css.fact} data-tone={tone} data-empty={none || undefined}>
      <dt>{label}</dt>
      <dd className={clsx(mono && !none && css.mono)} title={title}>{none ? empty : children}</dd>
    </div>
  )
}

/** A list of fact rows inside a card. */
export function Facts({ children, label }: { children: ReactNode, label?: string }): ReactNode {
  return <dl className={css.facts} aria-label={label}>{children}</dl>
}

// ---------- state chip ----------

export type Tone = 'success' | 'neutral' | 'warning' | 'danger' | 'info'

/** How a tone reads as a DSH state dot; a live (pulsing) state spins. */
function dotOf(tone: Tone, live: boolean): StateDotState {
  if (live) return 'ongoing'
  return tone === 'success' ? 'done' : tone === 'warning' ? 'warning' : tone === 'danger' ? 'error' : 'idle'
}

/**
 * A machine's, Store's or Sandbox's state as the SDK gives it: a state dot and
 * the word; an operation in flight follows in mono.
 */
export function StateChip({ tone, label, live, dashed, op, title, className }: {
  tone: Tone
  label: string
  live?: boolean | undefined
  dashed?: boolean | undefined
  /** The operation's words (`machine.start · boot`). */
  op?: string | null | undefined
  title?: string | undefined
  className?: string
}): ReactNode {
  return (
    <span className={clsx(css.chip, className)} data-tone={tone} data-dashed={dashed || undefined} title={title}>
      {live ? <span className={css.chipDot}><StateDot state={dotOf(tone, true)} /></span> : null}
      <span className={css.chipLabel}>{label}</span>
      {op && <span className={css.chipOp}>· {op}</span>}
    </span>
  )
}

// ---------- click safety ----------

/** How long a control that runs an action ignores presses after it appears or changes what it does (UI pass 3 F6). */
export const PRESS_SETTLE_MS = 600

/** A guarded onClick wrapper; `settling` is true while presses are dropped (render the control off then: aria-disabled and visibly). */
export interface PressGuard {
  <E extends SyntheticEvent>(handler: (e: E) => void): (e: E) => void
  readonly settling: boolean
}

/**
 * Click safety for a control that runs an action (a power action, a fix): its
 * press is keyed to `identity` (the thing it acts on and what it does now). A
 * press that lands within {@link PRESS_SETTLE_MS} of the control first showing
 * that identity (a row re-rendered, re-sorted or swapped under the pointer) is
 * dropped, and the handler always runs with the identity it was drawn for, so
 * a stale handler never acts on another row. The drop is never silent: while
 * it holds, `settling` is true (the control renders off) and one timer
 * re-renders it at the end. Enter and Space reach a button as a click, so the
 * keyboard goes through the same guard (UI pass 3 F6, review NOTE-1).
 */
export function usePressGuard(identity: string, settleMs = PRESS_SETTLE_MS): PressGuard {
  const seen = useRef({ identity, at: Date.now() })
  if (seen.current.identity !== identity) seen.current = { identity, at: Date.now() }
  const [, rerender] = useState(0)
  const settling = settleMs > 0 && !pressAllowed(seen.current, identity, Date.now(), settleMs)
  useEffect(() => {
    if (!settling) return undefined
    const timer = setTimeout(() => rerender((n) => n + 1), Math.max(0, seen.current.at + settleMs - Date.now()) + 5)
    return () => clearTimeout(timer)
  }, [identity, settling, settleMs])
  const guard = (<E extends SyntheticEvent>(handler: (e: E) => void) => guardedPress(seen, identity, settleMs, handler)) as PressGuard
  Object.defineProperty(guard, 'settling', { value: settling })
  return guard
}

/**
 * The guarded handler itself (pure, for tests): runs `handler` only when the
 * control still shows `drawn` and has for `settleMs`; any press, pointer or
 * keyboard (`detail` 0), is judged the same way.
 */
export function guardedPress<E extends { stopPropagation(): void }>(seen: { current: { identity: string, at: number } }, drawn: string, settleMs: number, handler: (e: E) => void, now: () => number = Date.now): (e: E) => void {
  // the control as it is when pressed (the ref), against what this handler was drawn for: a stale handler never acts
  return (e) => {
    if (!pressAllowed(seen.current, drawn, now(), settleMs)) { e.stopPropagation(); return }
    handler(e)
  }
}

/** Whether a press drawn for `drawn` may act now: the control still shows that identity, and has shown it for `settleMs`. */
export const pressAllowed = (seen: { identity: string, at: number }, drawn: string, now: number, settleMs = PRESS_SETTLE_MS): boolean =>
  seen.identity === drawn && now - seen.at >= settleMs

/**
 * Keeps a dialog's clicks and keys inside it: a React portal passes its events
 * up the component tree, so a press in a confirm dialog would otherwise also
 * reach the row or card that rendered it (UI pass 3 F6). Wrap a Modal in it.
 */
export function Contained({ children }: { children: ReactNode }): ReactNode {
  // clicks only (Enter and Space on a button arrive as a click): Escape and the dialog's own keys stay as they are
  return <span style={{ display: 'contents' }} onClick={(e) => e.stopPropagation()}>{children}</span>
}

// ---------- notice ----------

/** A tinted strip for reasons, refusals and clearing calls. */
export function Notice({ tone, children, className, role }: { tone?: 'danger' | 'warning' | undefined, children: ReactNode, className?: string, role?: string }): ReactNode {
  return <div className={clsx(css.notice, className)} data-tone={tone} role={role}>{children}</div>
}

// ---------- refusal ----------

/** A refusal: LayerFS's code, its own words, what is in the way, and its `fix` line. */
export interface RefusalShape {
  readonly code: string
  readonly message: string
  readonly detail?: any
  readonly transport?: string | undefined
  readonly fix?: string | undefined
}

/**
 * Runs a refusal's fix line for the page (UI rule: the fix line becomes the one
 * action where the plugin can run it): the Center page provides it with its
 * host API and words. Without one a fix stays the hover tip.
 */
export interface FixRunner {
  run(fix: RunnableFix): Promise<unknown>
  /** The button's words (`Start machine`). */
  label(fix: RunnableFix): string
  /** The second press of a fix that removes something (`delete`, `prune`). */
  confirm(fix: RunnableFix): string
  /** The tip on the button and on a fix it cannot run: the line it runs or that clears the refusal. */
  tip(line: string): string
  /** After a fix ran. */
  done: string
  /**
   * A chain with a step that removes data (review FAIL-1): the page's typed-name
   * confirm, listing every step; resolves true once the name was typed and
   * confirmed, false when it was cancelled. Absent: such a chain is not run here.
   */
  confirmChain?(fix: Extract<RunnableFix, { kind: 'chain' }>, typed: string): Promise<boolean>
  /** The button's accessible name: its words and the thing it acts on (`Update (compact-demo-pg)`); absent: the words. */
  named?(fix: RunnableFix, words: string): string
  /** A chain's step while it runs (`Step 1 of 2: Remove x’s record`); absent: `1/2`. */
  step?(index: number, count: number, fix: RunnableFix): string
  /**
   * The fix a refusal's line stands for on this page: a start, stop or restart
   * of a machine whose own record is broken is that machine's own fix instead
   * (or none): a broken machine is never advised `machine start` (UI pass 2 F1).
   * Absent: the line as it is.
   */
  resolve?(line: string): { line: string, fix: RunnableFix | null } | null
}
export const FixContext = createContext<FixRunner | null>(null)

/** A refusal's fix line on this page ({@link FixRunner.resolve}): the line to show as its tip (null: none) and the fix the page runs for it. */
export function resolveFix(runner: FixRunner | null, line: string | null | undefined): { line: string, fix: RunnableFix | null } | null {
  if (typeof line !== 'string' || !line) return null
  if (runner?.resolve) return runner.resolve(line)
  return { line, fix: runner ? runnableFix(line) : null }
}

/**
 * One refusal's fix as its one action: a removal asks a second press; a refused fix shows its own refusal (without an action).
 * A chain (LayerFS's multi-command fix) runs its steps in order behind that confirm press, saying the step it is at;
 * one attempt per step, and the first refusal ends it with that step's refusal (nothing after it runs).
 * The press is keyed to its fix line: a line that changes under the button (a re-render, a re-read) starts it idle again,
 * so a confirm armed for one fix never runs another.
 */
export function FixAction({ line, fix, runner, labels }: { line: string, fix: RunnableFix, runner: FixRunner, labels: RefusalLabels }): ReactNode {
  const guard = usePressGuard(line)
  const [step, setStep] = useState<'idle' | 'confirm' | 'busy' | 'done'>('idle')
  const [failed, setFailed] = useState<unknown>(null)
  const [at, setAt] = useState<number | null>(null)
  // the line this button's state belongs to: a different line resets it
  const [armedFor, setArmedFor] = useState(line)
  if (armedFor !== line) { setArmedFor(line); setStep('idle'); setFailed(null); setAt(null) }
  const removes = removesSomething(fix)
  // a fix that opens something (the save-login dialog, the Doctor page) is not done when it opens
  const opens = fix.kind === 'saveAuth' || fix.kind === 'doctor' || fix.kind === 'connectBucket'
  const steps: RunnableFix[] = fix.kind === 'chain' ? fix.steps : [fix]
  // a chain that removes data asks the typed-name confirm (every step listed) instead of the second press
  const typed = typedNameOf(fix)
  const press = async () => {
    if (step === 'busy') return
    if (fix.kind === 'chain' && typed) {
      if (!runner.confirmChain || !(await runner.confirmChain(fix, typed))) return
    } else if (removes && step === 'idle') { setStep('confirm'); return }
    setStep('busy'); setFailed(null)
    try {
      for (let i = 0; i < steps.length; i++) { setAt(i); await runner.run(steps[i]!) }
      setStep(opens ? 'idle' : 'done')
    } catch (error) { setFailed(error); setStep('idle') } finally { setAt(null) }
  }
  if (step === 'done') return <p className={css.refusalFix} role="status">{runner.done}</p>
  const running = step === 'busy' && at !== null && steps.length > 1
  const words = running ? (runner.step?.(at + 1, steps.length, steps[at]!) ?? `${at + 1}/${steps.length}`) : step === 'confirm' ? runner.confirm(fix) : runner.label(fix)
  return (
    <div className={css.refusalFix}>
      {/* its name is its words; the CLI line it runs is only the tip */}
      <Button size="sm" variant={step === 'confirm' ? 'primary' : 'outline'} disabled={step === 'busy' || guard.settling} aria-disabled={guard.settling || undefined} title={runner.tip(line)} aria-label={runner.named?.(fix, words) ?? words} data-fid="refusal-fix"
        aria-busy={step === 'busy' || undefined} onClick={guard((e) => { e.stopPropagation(); void press() })}>
        {words}
      </Button>
      {failed != null && <Refusal error={failed} labels={labels} compact noFix />}
    </div>
  )
}

/** The words a refusal needs; the caller's locale supplies them. */
export interface RefusalLabels {
  /** The heading for each transport class. */
  readonly category: (kind: 'refusal' | 'usage' | 'unavailable' | 'unknown') => string
  readonly inTheWay: string
  /** An entity kind in words (`sandbox` -> `Sandbox`). */
  readonly kind: (kind: string) => string
  readonly phase: (phase: string) => string
  readonly operation: (op: string) => string
}

/** The refusal an error carries, or a plain error's words. */
export function refusalOf(error: unknown): RefusalShape {
  if (error && typeof error === 'object' && 'code' in error && 'message' in error) {
    const e = error as { code: unknown, message: unknown, detail?: unknown, transport?: unknown, fix?: unknown }
    return { code: String(e.code), message: String(e.message), detail: e.detail ?? null, transport: typeof e.transport === 'string' ? e.transport : undefined, fix: typeof e.fix === 'string' && e.fix ? e.fix : undefined }
  }
  return { code: 'Failed', message: error instanceof Error ? error.message : String(error), detail: null }
}

const EXTRA = ['authority', 'source', 'retired', 'needed', 'free', 'missing', 'vm_exists', 'execs']

/**
 * A LayerFS refusal as it came: nothing is reworded. Its `fix` line is the one
 * action where the Center page can run it ({@link FixContext}), else the tip on its code.
 */
export function Refusal({ error, labels, compact, className, noFix }: { error: unknown, labels: RefusalLabels, compact?: boolean, className?: string, noFix?: boolean }): ReactNode {
  const r = refusalOf(error)
  const runner = useContext(FixContext)
  const resolved = !noFix && r.fix ? resolveFix(runner, r.fix) : null
  const fix = resolved?.line || null
  const runnable = fix && runner ? resolved!.fix : null
  const d = r.detail && typeof r.detail === 'object' ? r.detail : null
  const related: { kind: string, name: string, why?: string }[] = Array.isArray(d?.related) ? d.related : []
  const category = r.transport === 'usage-incompatible' ? 'usage' : r.transport === 'unavailable' ? 'unavailable' : r.transport === 'transport-unknown' ? 'unknown' : 'refusal'
  const where = [
    d?.entity && `${labels.kind(String(d.entity.kind))} ${d.entity.name}`,
    d?.phase && labels.phase(String(d.phase)),
    typeof d?.op === 'string' && labels.operation(d.op),
  ].filter(Boolean)
  const extra = d ? Object.entries(d).filter(([key]) => EXTRA.includes(key)) : []
  return (
    <div role="alert" className={clsx(css.refusal, className)}>
      {!compact && <p className={css.refusalCategory}>{labels.category(category)}</p>}
      <p><span className={css.refusalCode} data-fix={fix && !runnable ? true : undefined} title={fix && !runnable ? (runner ? runner.tip(fix) : fix) : undefined}>{r.code}</span>: {r.message}</p>
      {!compact && where.length > 0 && <p className={css.refusalWhere}>{where.join(' · ')}</p>}
      {!compact && extra.map(([key, value]) => <p key={key} className={css.refusalExtra}>{key}: {typeof value === 'object' ? JSON.stringify(value) : String(value)}</p>)}
      {related.length > 0 && (
        <>
          <p className={css.refusalWhere}>{labels.inTheWay}</p>
          <ul className={css.refusalRelated}>
            {related.map((x) => <li key={`${x.kind}:${x.name}`}>{labels.kind(x.kind)} {x.name}{x.why ? ` — ${x.why}` : ''}</li>)}
          </ul>
        </>
      )}
      {fix && runnable && runner && <FixAction line={fix} fix={runnable} runner={runner} labels={labels} />}
    </div>
  )
}

// ---------- loading / empty / error ----------

/** A read in flight. */
export function Loading({ label }: { label: string }): ReactNode {
  return <p className={css.status} role="status"><StateDot state="ongoing" />{label}</p>
}

/** Nothing to show, in a card. */
export function Empty({ title, children, icon }: { title: string, children?: ReactNode, icon?: ReactNode }): ReactNode {
  return (
    <Card className={css.empty}>
      {icon && <span className={css.emptyIcon} aria-hidden="true">{icon}</span>}
      <p className={css.emptyTitle}>{title}</p>
      {children && <p className={css.emptyBody}>{children}</p>}
    </Card>
  )
}

/** A read that failed: what was read, the refusal, and a retry. */
export function LoadError({ title, error, labels, retryLabel, onRetry }: { title: string, error: unknown, labels: RefusalLabels, retryLabel: string, onRetry?: () => void }): ReactNode {
  return (
    <Card tone="danger" className={css.error}>
      <p className={css.errorTitle}><IconWarningOutlineRegular size={16} />{title}</p>
      <Refusal error={error} labels={labels} />
      {onRetry && <Button variant="outline" size="sm" icon={<IconRefreshOutlineRegular size={13} />} onClick={onRetry}>{retryLabel}</Button>}
    </Card>
  )
}

// ---------- meter ----------

/** A share from 0 to 1 as a thin bar; its tone warns past 75 % and 90 % unless `tone="neutral"` (progress, not use). */
export function Meter({ share, label, className, tone: fixed }: { share: number, label: string, className?: string, tone?: 'neutral' | undefined }): ReactNode {
  const clamped = Math.max(0, Math.min(1, share))
  const tone = fixed === 'neutral' ? undefined : clamped > 0.9 ? 'danger' : clamped > 0.75 ? 'warning' : undefined
  return (
    <span className={clsx(css.meter, className)} data-tone={tone} role="meter" aria-label={label} aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(clamped * 100)}>
      <span className={css.meterFill} style={{ width: `${clamped * 100}%` }} />
    </span>
  )
}

// ---------- forms and menus ----------

/** A labelled form field: its label, the control, then its error or hint. */
export function Field({ label, error, hint, children, className, group }: {
  label: string, error?: string | null | undefined, hint?: string | undefined, children: ReactNode, className?: string
  /** Several controls (a set of tiles): a div, so a click on the label or hint does not press the first one. */
  group?: boolean
}): ReactNode {
  const Tag = group ? 'div' : 'label'
  return (
    <Tag className={clsx(css.field, className)} data-invalid={error ? '' : undefined}>
      <span className={css.fieldLabel}>{label}</span>
      {children}
      {error ? <span className={css.fieldError} role="alert">{error}</span> : hint ? <span className={css.fieldHint}>{hint}</span> : null}
    </Tag>
  )
}

/** One choice of a {@link MenuSelect}. */
export interface Choice<V extends string> { readonly value: V, readonly label: string }

/**
 * The outline button a selection menu hangs from: the chosen value (or a
 * muted placeholder) and a chevron. Shared by {@link MenuSelect} and menus
 * whose rows are richer than a label (the Image menu).
 */
export function SelectAnchor({ label, value, placeholder, open, disabled, onToggle, title, className }: {
  /** Accessible name. */
  label: string
  /** What the button shows; empty shows `placeholder`. */
  value: ReactNode
  placeholder?: string | undefined
  open: boolean
  disabled?: boolean | undefined
  onToggle: (open: boolean) => void
  title?: string | undefined
  className?: string | undefined
}): ReactNode {
  const empty = value === null || value === undefined || value === ''
  return (
    <button type="button" className={clsx(css.select, className)} disabled={disabled} aria-label={label} aria-haspopup="listbox" aria-expanded={open} title={title}
      onClick={() => onToggle(!open)}
      onKeyDown={(e) => { if (!open && e.key === 'ArrowDown') { e.preventDefault(); onToggle(true) } }}>
      <span className={clsx(css.selectValue, empty && css.selectPlaceholder)}>{empty ? placeholder ?? '' : value}</span>
      <IconChevronDownOutlineRegular size={12} aria-hidden="true" />
    </button>
  )
}

/** A selection: an outline button showing the choice, opening a DSH `Menu` of the options. */
export function MenuSelect<V extends string>({ value, options, onChange, label, disabled, placeholder, className }: {
  value: V | ''
  options: readonly Choice<V>[]
  onChange: (value: V) => void
  /** Accessible name. */
  label: string
  disabled?: boolean
  placeholder?: string
  className?: string
}): ReactNode {
  const [open, setOpen] = useState(false)
  const chosen = options.find((o) => o.value === value)
  return (
    <Menu open={open} onClose={() => setOpen(false)} portal autoFocus minWidthFromAnchor selection="check" selectedId={value || undefined}
      items={options.map((o) => ({ id: o.value, label: o.label }))}
      onSelect={(id) => { setOpen(false); onChange(id as V) }}
      anchor={<SelectAnchor label={label} value={chosen?.label} placeholder={placeholder} open={open} disabled={disabled} onToggle={setOpen} className={className} />} />
  )
}

/** A `…` toolbar button opening a DSH `Menu` of `MenuItemButton` children. */
export function MoreMenu({ label, disabled, children, className }: { label: string, disabled?: boolean, children: (close: () => void) => ReactNode, className?: string }): ReactNode {
  const [open, setOpen] = useState(false)
  const close = () => setOpen(false)
  return (
    <Menu open={open} onClose={close} align="end" portal autoFocus
      anchor={(
        <Button variant="ghost" size="sm" className={clsx(css.iconButton, className)} disabled={disabled} aria-label={label} title={label}
          aria-haspopup="menu" aria-expanded={open} icon={<IconEllipsisOutlineRegular size={16} />}
          onClick={(e) => { e.stopPropagation(); setOpen((v) => !v) }} />
      )}>
      {children(close)}
    </Menu>
  )
}

/** A copyable command line. */
export function CopyLine({ text, copyLabel, copiedLabel }: { text: string, copyLabel: string, copiedLabel: string }): ReactNode {
  const [copied, setCopied] = useState(false)
  return (
    <span className={css.copyLine}>
      <code className={css.copyText}>{text}</code>
      <Button variant="ghost" size="sm" className={css.iconButton} aria-label={copied ? copiedLabel : copyLabel} title={copied ? copiedLabel : copyLabel}
        icon={copied ? <IconCheckOutlineRegular size={14} /> : <IconCopyOutlineRegular size={14} />}
        onClick={() => { void navigator.clipboard?.writeText(text).then(() => { setCopied(true); setTimeout(() => setCopied(false), 1000) }) }} />
    </span>
  )
}

/**
 * Copies `id` to the clipboard: `copied` once written; `failed` when the
 * write is refused (an unfocused page, no permission) or there is no
 * clipboard, never an unhandled rejection (RV4 NOTE 11).
 */
export function copyText(id: string, clipboard: Pick<Clipboard, 'writeText'> | undefined = globalThis.navigator?.clipboard): Promise<'copied' | 'failed'> {
  if (!clipboard?.writeText) return Promise.resolve('failed')
  try { return clipboard.writeText(id).then(() => 'copied' as const, () => 'failed' as const) } catch { return Promise.resolve('failed') }
}

/**
 * A saved id as its short label (files/format.ts shortId): the full id is the
 * tip, and a press copies the full id. A refused copy says so in the tip and
 * selects the shown id, so it can be copied by hand.
 */
export function CopyId({ id, short, copyLabel, copiedLabel, failedLabel }: { id: string, short: string, copyLabel: string, copiedLabel: string, failedLabel: string }): ReactNode {
  const [outcome, setOutcome] = useState<'copied' | 'failed' | null>(null)
  const word = outcome === 'copied' ? copiedLabel : outcome === 'failed' ? failedLabel : copyLabel
  const tip = `${id}\n${word}`
  return (
    <button type="button" className={css.copyId} title={tip} aria-label={`${short}, ${word}`} data-copy-id={id} data-copy={outcome ?? undefined}
      onClick={(e) => {
        e.stopPropagation()
        const code = e.currentTarget.querySelector('code')
        void copyText(id).then((done) => {
          setOutcome(done)
          if (done === 'failed' && code) { try { window.getSelection()?.selectAllChildren(code) } catch { /* nothing to select */ } }
          if (done === 'copied') setTimeout(() => setOutcome(null), 1000)
        })
      }}>
      <code>{short}</code>{outcome === 'copied' ? <IconCheckOutlineRegular size={11} /> : outcome === 'failed' ? <IconWarningOutlineRegular size={11} /> : <IconCopyOutlineRegular size={11} />}
    </button>
  )
}

/** A text button styled as a link. */
export function LinkButton({ children, onClick, title, className }: { children: ReactNode, onClick: () => void, title?: string, className?: string }): ReactNode {
  return <button type="button" className={clsx(css.link, className)} onClick={onClick} title={title}>{children}</button>
}
