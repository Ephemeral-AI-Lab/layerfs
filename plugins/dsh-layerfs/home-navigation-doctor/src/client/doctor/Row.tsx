/**
 * One row of the Doctor page, and what opens under it.
 *
 * A row is: its name (the button that opens it), its signal (the plugin's
 * Signal: a dot with the reason in its hover card, then one of five status
 * words), then the finding's own word, the
 * root cause it waits on, its fix, facts from its tree node, one mark per
 * check, and its Check buttons. A button that cannot be pressed now keeps
 * `aria-disabled`, not `disabled`: it stays in the tab order, keeps focus and
 * says why in its tip. A row waiting on something else has no buttons, only
 * the link to its root cause.
 */
import { useEffect, useRef, useState, type CSSProperties, type KeyboardEvent, type MouseEvent, type ReactNode } from 'react'
import clsx from 'clsx'
import { Button } from '@deepseek-ai/dsh-client-ui-primitives'
import { quickChecks, whatOf } from './checks.ts'
import { causeOf, kidsOf, offWhy, running, stateOf, viewOf, waitingRun, type DoctorState, type Mark as CheckMark, type RowFix, type RowView, type Thing } from './model.ts'
import { markDot, TipBody, useTip, type MarkKind, type TipData } from './tips.tsx'
import { DOT, mmss, type Words } from './words.ts'
import { Signal, type SignalState } from '../storage/Signal.tsx'
import { usePressGuard } from '../ui/index.tsx'
import { BucketProviderGlyph, MachineGlyph, SandboxGlyph, StoreKindGlyph } from '../machines/icons.tsx'
import { LayerfsGlyph } from '../overview/glyph.tsx'
import { BucketGlyph } from '../storage/parts.tsx'
import { shortDigest } from '../../shared/doctor.ts'
import css from './DoctorPage.module.css'

export interface RowActions {
  toggle(key: string): void
  check(thing: Thing): void
  full(thing: Thing): void
  stop(thing: Thing): void
  fix(thing: Thing, fix: RowFix): void
  goTo(key: string): void
}

export interface RowContext {
  state: DoctorState
  w: Words
  open: Readonly<Record<string, boolean>>
  flash: string | null
  /** Ticks while a Full check runs (its elapsed time). */
  now: number
  on: RowActions
}

/** A Workspace is a Branch mounted in a Sandbox. */
function BranchGlyph({ size = 15 }: { size?: number }): ReactNode {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth={1.2} strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" focusable="false">
      <circle cx="4.6" cy="3.8" r="1.6" /><circle cx="4.6" cy="12.2" r="1.6" /><circle cx="11.4" cy="5.4" r="1.6" /><path d="M4.6 5.4v5.2M11.4 7c0 2.6-6.8 1.4-6.8 3.6" />
    </svg>
  )
}

/** A row's kind icon. Brand marks draw in the text colour: on this page colour is kept for state. */
export function KindGlyph({ thing }: { thing: Thing }): ReactNode {
  if (thing.kind === 'home') return <LayerfsGlyph size={16} />
  if (thing.kind === 'machine') return <MachineGlyph platform={thing.platform ?? ''} size={thing.platform === 'lima' ? 13 : 15} mono />
  if (thing.kind === 'store') return <StoreKindGlyph kind={thing.engine ?? 'sqlite'} size={15} mono />
  if (thing.kind === 'project') return <LayerfsGlyph size={15} mono />
  if (thing.kind === 'sandbox') return <SandboxGlyph size={15} />
  if (thing.kind === 'workspace') return <BranchGlyph />
  return thing.provider === 'r2' || thing.provider === 'minio' ? <BucketProviderGlyph provider={thing.provider} size={15} mono /> : <BucketGlyph size={15} />
}

/** A button of the page: a DSH Button that says why when it cannot be pressed. */
export function Btn({ fid, tip, onPress, off, busy, variant = 'outline', width, label, className, settle, children }: {
  fid: string
  /** Click safety for a button that changes something (a fix): presses are keyed to this and dropped while it has just appeared or changed. */
  settle?: string | undefined
  tip: TipData
  onPress(): void
  off?: boolean
  busy?: boolean
  variant?: 'outline' | 'primary'
  width?: 'check' | 'full' | 'copy' | 'all'
  label?: string | undefined
  className?: string
  children: ReactNode
}): ReactNode {
  const { props, showNow, hide } = useTip(tip)
  const guard = usePressGuard(settle ?? fid, settle ? undefined : 0)
  return (
    <Button variant={variant} size="sm" className={clsx(css.btn, className)} data-w={width} data-variant={variant} data-fid={fid} aria-label={label}
      aria-busy={busy || undefined} aria-disabled={busy || off || guard.settling || undefined} data-settling={guard.settling ? '' : undefined} {...props}
      onClick={guard((e: MouseEvent<HTMLButtonElement>) => {
        e.stopPropagation()
        // it cannot be pressed now: its tip says why, and nothing happens
        if (busy || off) showNow(e.currentTarget)
        else { hide(); onPress() }
      })}>
      {children}
    </Button>
  )
}

/** A row's state (and the page's headline) as the plugin's Signal. */
export const SIGNAL: Record<string, SignalState> = { ok: 'ok', run: 'ongoing', nc: 'idle', notice: 'warning', problem: 'error' }

/**
 * A status: the plugin's Signal (its dot; the reason, checks and commands in
 * its hover card, opened by pointer or keyboard focus) and the status word.
 */
export function StatusSignal({ fid, st, tip, said, className, size, children }: { fid: string, st: string, tip: TipData, said?: string | undefined, className?: string, size?: number, children: ReactNode }): ReactNode {
  return (
    <span className={clsx(css.sig, className)} data-st={st}>
      <Signal state={SIGNAL[st] ?? 'idle'} size={size} fid={fid} title={tip.title} anchorText={children} label={said ? `${tip.title}: ${said}` : tip.title} detail={<span className={css.sigCard}><TipBody tip={tip} /></span>} />
    </span>
  )
}

function Marks({ fid, room, marks, w }: { fid: string, room: number, marks: readonly CheckMark[] | null, w: Words }): ReactNode {
  const tip: TipData | null = marks ? { title: w.n('dpChecks', marks.length), checks: w.tipChecks(marks) } : null
  const { props, showNow } = useTip(tip)
  const style = { '--n': Math.max(room, marks?.length ?? 0) } as CSSProperties
  if (!marks) return <span className={css.marks} style={style} />
  const label = `${w.n('dpChecks', marks.length)}: ${marks.map((c) => `${w.checkName(c)} ${w.checkText(c)}`).join(', ')}`
  return (
    <button type="button" className={css.marks} style={style} data-fid={fid} aria-label={label} {...props} onClick={(e) => { e.stopPropagation(); showNow(e.currentTarget) }}>
      {marks.map((c, i) => <span key={i} className={css.mkSlot}>{markDot(c.r)}</span>)}
    </button>
  )
}

function Copy({ fid, text, w }: { fid: string, text: string, w: Words }): ReactNode {
  const [copied, setCopied] = useState(false)
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)
  useEffect(() => () => clearTimeout(timer.current), [])
  return (
    <Btn fid={fid} width="copy" label={w.t('dpCopyLabel')} tip={{ title: w.t('dpCopyLabel'), lines: [w.t('dpCopyTip')] }}
      onPress={() => {
        void navigator.clipboard?.writeText(text).catch(() => {})
        setCopied(true)
        clearTimeout(timer.current)
        timer.current = setTimeout(() => setCopied(false), 1400)
      }}>
      {copied ? w.t('dpCopied') : w.t('dpCopy')}
    </Btn>
  )
}

/** One check of the detail: a small signal of its own, whose tip says what the check is. */
function CheckLine({ fid, c, w }: { fid: string, c: CheckMark, w: Words }): ReactNode {
  const { props, showNow } = useTip({ title: w.checkName(c), mark: c.r, lines: [whatOf(c.check)] })
  return (
    <li>
      <button type="button" className={css.chk} data-fid={fid} {...props} onClick={(e) => { e.stopPropagation(); showNow(e.currentTarget) }}>
        {markDot(c.r)}<code>{w.checkName(c)}</code>
      </button>
      <span className={css.res}>{w.checkText(c)}</span>
    </li>
  )
}

/** A thing's key as an element id (no spaces: `aria-controls` lists ids). */
const idOf = (key: string): string => key.replace(/[^A-Za-z0-9_-]/g, (c) => `.${c.charCodeAt(0).toString(16)}`)
export const detailId = (key: string): string => `lfs-doc-d-${idOf(key)}`
export const kidsId = (key: string): string => `lfs-doc-k-${idOf(key)}`

/** A row's detail: the call, its checks, and the fix to copy. */
export function Detail({ v, ctx }: { v: RowView, ctx: RowContext }): ReactNode {
  const { w } = ctx
  const x = v.thing, r = v.seen.r
  // before any doctor has run: the checks its doctor makes, none made yet
  const list: readonly CheckMark[] = r?.checks ?? quickChecks(x.kind).map((c) => ({ check: c.check, from: null, r: 'nc' as const, message: null, reason: null }))
  const cmd = w.callFor(x, r?.coverage === 'full')
  // the fix as LayerFS gave it: one line that runs as it is
  const fix = v.fix && (v.ownTrouble || v.seen.st === 'run') ? v.fix.line : null
  return (
    <div className={css.detail} style={{ '--depth': v.depth } as CSSProperties} id={detailId(x.key)}>
      <div className={css.cmdLine}>
        <span className={css.lbl}>{w.t('dpDetailCall')}</span><code>{cmd}</code><Copy fid={`copycall:${x.key}`} text={cmd} w={w} />
      </div>
      <ul className={css.checks} aria-label={w.t('dpDetailChecks')}>
        {list.map((c, i) => <CheckLine key={`${c.check}:${c.from ?? ''}:${i}`} fid={`chk:${x.key}:${i}`} c={c} w={w} />)}
      </ul>
      {fix ? (
        <div className={css.cmdLine}>
          <span className={css.lbl}>{w.t('dpDetailFix')}</span><code>{fix}</code><Copy fid={`copyfix:${x.key}`} text={fix} w={w} />
        </div>
      ) : null}
    </div>
  )
}

/** The link from a waiting row to its root cause. */
function Cause({ fid, by, ctx }: { fid: string, by: Thing, ctx: RowContext }): ReactNode {
  const { w } = ctx
  const name = w.nameOf(by)
  return (
    <a className={css.cause} href="#" data-fid={fid} aria-label={w.t('dpGoTo', { name })} onClick={(e) => { e.preventDefault(); e.stopPropagation(); ctx.on.goTo(by.key) }}>
      <span className={css.dot}>{markDot(DOT[stateOf(ctx.state, by).st])}</span><span className={css.causeName}>{name}</span>
    </a>
  )
}

export function Row({ v, open, ctx }: { v: RowView, open: boolean, ctx: RowContext }): ReactNode {
  const { state, w, on } = ctx
  const x = v.thing, o = v.seen, key = x.key
  const sigTip = w.sigTip(state, v)
  const fixing = state.fixes[key]
  const phase = w.stepWord(fixing?.step)
  const waitsOn = causeOf(o, key)
  const refusal = state.refusals[key]
  const full = running(state, 'full', key)
  const name = v.depth > 0 && x.kind === 'project' ? x.label : w.nameOf(x)
  const fixLabel = w.fixLabel(v.fix)
  const fact = o.st === 'run' || phase ? '' : w.fact(state, v)
  const inside = w.inside(v)
  const more: ReactNode[] = []
  const acts: ReactNode[] = []
  const rowWord = w.rowWord(v.word)
  if (rowWord) more.push(<span key="word" className={css.word}>{rowWord}</span>)
  if (waitsOn) more.push(<Cause key="cause" fid={`goto:${key}`} by={waitsOn} ctx={ctx} />)
  else if (o.st === 'nc' && (o.why === 'waiting' || o.why === 'stopped') && o.byName) more.push(<span key="cause" className={css.fact}>{w.t('dpWaitingOn', { name: o.byName })}</span>)
  if (v.fix?.button && !v.blocked) {
    const off = fixing ? null : offWhy(state, x, 'fix', v.fix)
    // an off button shows no command when the only fix stops by force: that line is copied from the detail, never run here
    const forced = off?.why === 'sandboxes'
    const after = kidsOf(state.world!, x).filter((k) => k.kind === 'store' || k.kind === 'sandbox').map((k) => w.callFor(k))
    acts.push(
      <Btn key="fix" variant="primary" fid={`fix:${key}`} busy={!!fixing} off={!!off} onPress={() => on.fix(x, v.fix!)}
        label={w.t('dpFixNamed', { fix: fixLabel, name: w.nameOf(x) })} settle={`${key}:${v.fix.line}:${o.st}:${off?.why ?? ''}`}
        tip={{ title: fixLabel, lines: [w.offText(off) ?? w.t(x.kind === 'machine' ? 'dpFixThenMachine' : 'dpFixThen')], cmds: forced ? [] : [v.fix.line, `# ${w.t('dpCmdThen')}`, w.callFor(x.kind === 'project' && x.parent ? state.world!.by[x.parent] ?? x : x), ...(x.kind === 'machine' ? after : [])] }}>
        {fixLabel}
      </Btn>,
    )
    // the step the fix is at, after the button so the button never moves
    if (phase) more.push(<span key="phase" className={css.phase}>{phase}…</span>)
  }
  // a refused fix: the refusal as a signal beside the button, which is back and can be pressed again
  if (refusal) {
    const title = w.t('dpRefusedTitle', { fix: fixLabel || w.t('dpDetailFix') })
    const said = [refusal.code, refusal.message].filter(Boolean).join(': ')
    more.push(
      <StatusSignal key="refused" fid={`refused:${key}`} st="notice" said={said} tip={{ title, lines: [said, w.t('dpRefusedNothing', { fix: fixLabel || w.t('dpDetailFix') })] }}>
        {w.t('dpRefused')}
      </StatusSignal>,
    )
  }
  // a call the host has not started yet (UI pass 3 F12): it says it waits for its turn, with Stop, never a silent Checking
  const waiting = full ? undefined : waitingRun(state, key, ctx.now)
  if (waiting) {
    more.push(
      <span key="wait" className={css.live} data-waiting="">
        <StatusSignal fid={`wait:${key}`} st="notice" tip={{ title: w.t('dpWaitingTurn'), lines: [w.t('dpWaitingTip')] }}>{w.t('dpWaitingTurn')}</StatusSignal>
        <Btn fid={`stopwait:${key}`} label={w.t('dpStopWaitLabel', { name: w.nameOf(x) })} tip={{ title: w.t('dpStop'), lines: [w.t('dpStopWaitTip')] }} onPress={() => on.stop(x)}>{w.t('dpStop')}</Btn>
      </span>,
    )
  }
  if (full) {
    more.push(
      <span key="live" className={css.live}>
        <span className={css.sr}>{w.t('dpElapsed')} </span><span className={css.elapsed}>{mmss(ctx.now - full.started)}</span>
        <Btn fid={`stop:${key}`} label={w.t('dpStopLabel', { name: w.nameOf(x) })} tip={{ title: w.t('dpStop'), lines: [w.t('dpStopTip')] }} onPress={() => on.stop(x)}>{w.t('dpStop')}</Btn>
      </span>,
    )
  }
  const thin = more.length === 0 && !inside
  if (fact) more.push(<span key="fact" className={css.fact} title={v.thing.image && fact.includes(shortDigest(v.thing.image)) ? v.thing.image : undefined}>{fact}</span>)
  if (inside) more.push(<span key="inside" className={css.inside}>{markDot(v.inside.bad ? 'bad' : 'warn')}{inside}</span>)

  if (v.canFull) {
    const off = full ? null : offWhy(state, x, 'full')
    acts.push(
      <Btn key="full" width="full" fid={`full:${key}`} label={w.t('dpFullLabel', { name: w.nameOf(x) })} busy={!!full} off={!!off} onPress={() => on.full(x)}
        tip={{ title: w.t('dpFull'), lines: [w.offText(off) ?? w.t(x.kind === 'project' ? 'dpFullProject' : 'dpFullBucket')], cmds: [w.callFor(x, true)] }}>
        {w.t('dpFull')}
      </Btn>,
    )
  }
  if (v.canCheck) {
    const mine = running(state, 'check', key)
    const off = mine ? null : offWhy(state, x, 'check')
    acts.push(
      <Btn key="check" width="check" fid={`check:${key}`} label={w.t('dpCheckLabel', { name: w.nameOf(x) })} busy={!!mine} off={!!off} onPress={() => on.check(x)}
        tip={{ title: w.t('dpCheck'), lines: [w.offText(off) ?? w.t(`dpCovers${x.kind.charAt(0).toUpperCase()}${x.kind.slice(1)}`)], cmds: [w.callFor(x)] }}>
        {w.t('dpCheck')}
      </Btn>,
    )
  }
  const controls = open ? (v.folder ? `${kidsId(key)}${v.ownTrouble ? ` ${detailId(key)}` : ''}` : detailId(key)) : undefined
  const room = (quickChecks(x.kind).length || 3) + (x.kind === 'project' || x.kind === 'bucket' ? 1 : 0)

  const onKey = (e: KeyboardEvent<HTMLButtonElement>) => {
    // the tree: up, down, Home, End; right opens or goes to the first thing on it; left closes or goes to its parent
    const panel = e.currentTarget.closest('[data-rows]')
    if (!panel) return
    const rows = Array.from(panel.querySelectorAll<HTMLElement>('[data-row]'))
    const i = rows.indexOf(e.currentTarget)
    const to = (el: HTMLElement | undefined | null) => { if (el) { el.focus(); el.scrollIntoView({ block: 'nearest' }) } }
    if (e.key === 'ArrowDown') to(rows[i + 1])
    else if (e.key === 'ArrowUp') to(rows[i - 1])
    else if (e.key === 'Home') to(rows[0])
    else if (e.key === 'End') to(rows[rows.length - 1])
    else if (e.key === 'ArrowRight') { if (!open) on.toggle(key); else if (v.folder) to(rows[i + 1]) }
    else if (e.key === 'ArrowLeft') { if (open) on.toggle(key); else if (x.parent) to(rows.find((el) => el.dataset.row === x.parent)) }
    else return
    e.preventDefault()
  }

  return (
    <div className={css.line} data-key={key} data-depth={v.depth} data-st={o.st} style={{ '--depth': v.depth } as CSSProperties}
      data-bare={!v.marks && !acts.length ? '' : undefined} data-flash={ctx.flash === key ? '' : undefined}
      onClick={(e) => {
        // the rest of a row opens it, as its name does
        if ((e.target as HTMLElement).closest('button, a') || window.getSelection()?.toString()) return
        on.toggle(key)
      }}>
      <button type="button" className={css.rowBtn} data-row={key} aria-label={`${name}, ${w.said(v, phase)}`} data-fid={`row:${key}`} aria-expanded={open} aria-controls={controls} onKeyDown={onKey}
        onClick={(e) => { e.stopPropagation(); on.toggle(key) }}>
        <span className={css.chev} aria-hidden="true" /><span className={css.ico}><KindGlyph thing={x} /></span>
        <span className={css.name}>{name}</span>
      </button>
      <StatusSignal fid={`sig:${key}`} st={o.st} tip={sigTip} said={(sigTip.lines ?? []).filter(Boolean).join(' ')}>{w.word(o.st)}</StatusSignal>
      <span className={css.more} data-thin={thin ? '' : undefined}>{more}</span>
      <Marks fid={`marks:${key}`} room={room} marks={v.marks} w={w} />
      <span className={css.acts}>{acts}</span>
    </div>
  )
}

/** A row, and under it when open: its own detail if it has trouble of its own, then what is on it. */
export function Node({ thing, depth, ctx }: { thing: Thing, depth: number, ctx: RowContext }): ReactNode {
  const open = !!ctx.open[thing.key]
  const v = viewOf(ctx.state, thing, depth, open)
  if (!open) return <Row v={v} open={false} ctx={ctx} />
  if (!v.folder) return <><Row v={v} open ctx={ctx} /><Detail v={v} ctx={ctx} /></>
  return (
    <>
      <Row v={v} open ctx={ctx} />
      {v.ownTrouble ? <Detail v={v} ctx={ctx} /> : null}
      <div className={css.kids} role="group" id={kidsId(thing.key)} aria-label={ctx.w.t('dpOnGroup', { name: ctx.w.nameOf(thing) })}>
        {v.kids.map((kid) => <Node key={kid.key} thing={kid} depth={depth + 1} ctx={ctx} />)}
      </div>
    </>
  )
}
