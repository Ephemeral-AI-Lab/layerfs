/**
 * A machine's worker log or a Sandbox's daemon log (layerfs-project#178 "Log
 * pane"), read through `GET /api/machines/:id/log` or `/api/sandboxes/:name/log`.
 *
 * It follows the log while the reader is at the bottom and says "↓ n new
 * lines" otherwise; finds text in the loaded lines; wraps or not; copies the
 * selection or everything loaded; pages back with "Earlier lines"; and keeps
 * a bounded window of lines in the browser (log-model.ts).
 *
 * Reads: one at a time, only while this tab is mounted and the window is
 * visible, every 2 s backing off to 10 s while nothing is new or a read was
 * refused. A refused read never empties the pane: the lines read before stay,
 * under a banner with their time and "Read again". Polling stops on
 * `Unsupported` and on a target that is gone; a cursor refused `InvalidInput`
 * (the log was rotated or replaced) reads the tail once without a cursor and
 * shows a "log replaced" divider. Leaving the tab cancels a read in flight.
 */
import { memo, useCallback, useEffect, useLayoutEffect, useMemo, useReducer, useRef, useState, type KeyboardEvent, type ReactNode } from 'react'
import clsx from 'clsx'
import {
  Button, IconCheckOutlineRegular, IconChevronDownOutlineRegular, IconChevronUpOutlineRegular, IconCopyOutlineRegular, IconRefreshOutlineRegular, IconSearchOutlineRegular, Input,
} from '@deepseek-ai/dsh-client-ui-primitives'
import { enc, q, type LayerfsApi } from '../api.ts'
import { EmptyState, Loading } from '../files/States.tsx'
import { Notice } from '../ui/index.tsx'
import { errorCode, errorText } from '../ui/use-query.ts'
import { useMachinesContext } from './context.tsx'
import {
  EARLIER_LIMIT, FORWARD_LIMIT, MORE_MS, POLL_MS, applyEarlier, applyEarlierRefused, applyForward, applyRefused, applyReplaced, applyTail, emptyLog, findRows, logText, matchParts, nextInterval, stopsPolling,
  type LogRow, type LogState, type LogView,
} from './log-model.ts'
import css from './log.module.css'
import { clockTime } from '../ui/locale-sync.ts'

/** How close to the bottom still counts as "at the bottom" (TerminalPane's). */
const STICK_PX = 24
/** How long the page waits for one read before it says so (the host's own budget is 15 s plus its queue). */
const ANSWER_MS = 25_000
const WRAP_KEY = 'dsh-layerfs:log-wrap'

const readWrap = (): boolean => { try { return localStorage.getItem(WRAP_KEY) === '1' } catch { return false } }
const writeWrap = (on: boolean): void => { try { localStorage.setItem(WRAP_KEY, on ? '1' : '0') } catch { /* the toggle still works */ } }

type Action =
  | { type: 'tail' | 'forward' | 'earlier' | 'replaced' | 'refused' | 'earlierRefused', view: LogView }
  | { type: 'reset' }

function reduce(state: LogState, action: Action): LogState {
  switch (action.type) {
    case 'tail': return applyTail(state, action.view)
    case 'forward': return applyForward(state, action.view)
    case 'earlier': return applyEarlier(state, action.view)
    case 'replaced': return applyReplaced(state, action.view)
    case 'refused': return applyRefused(state, action.view)
    case 'earlierRefused': return applyEarlierRefused(state, action.view)
    case 'reset': return emptyLog()
  }
}

/** A refusal that never reached the host's log route (the connection, the route itself), as a stale answer. */
const lostView = (error: unknown, cursor: string | null, before: string | null): LogView => ({
  lines: [], cursor, before, hasMore: false, observedAt: Date.now(),
  stale: { at: null, code: errorCode(error) ?? 'Unknown', message: errorText(error) },
})

let requests = 0

interface Follow {
  state: LogState
  /** A read is running now. */
  reading: boolean
  /** Reading earlier lines now. */
  paging: boolean
  /** Polling ended on this refusal code (`Unsupported`, a lost target). */
  stopped: string | null
  /** Read now (the tail again when `fresh`), and poll again if polling had stopped. */
  again: (fresh?: boolean) => void
  earlier: () => void
}

/**
 * Follow one log.
 * @param path - the log route (`/api/machines/<id>/log`).
 * @param paused - read once, then do not poll (the machine is stopped).
 * @param final - any refusal ends polling (the Sandbox is lost or gone).
 */
function useFollow(api: LayerfsApi, path: string, paused: boolean, final: boolean): Follow {
  const [state, dispatch] = useReducer(reduce, undefined, emptyLog)
  const [reading, setReading] = useState(false)
  const [paging, setPaging] = useState(false)
  const [stopped, setStopped] = useState<string | null>(null)
  const live = useRef({ state, paused, final, stopped, interval: POLL_MS, flight: null as string | null, timer: undefined as ReturnType<typeof setTimeout> | undefined, mounted: true, first: true })
  live.current.state = state
  live.current.paused = paused
  live.current.final = final
  live.current.stopped = stopped

  /** One GET of the route; a read that outlives ANSWER_MS, or a lost connection, answers as a refusal. */
  const get = useCallback(async (params: { cursor?: string | null, before?: string | null, limit?: number }): Promise<LogView> => {
    const rid = `log-${Date.now().toString(36)}-${++requests}`
    live.current.flight = rid
    let timer: ReturnType<typeof setTimeout> | undefined
    try {
      const late = new Promise<LogView>((resolve) => {
        timer = setTimeout(() => {
          void api.op('inspect.cancel', { rid }).catch(() => {})
          resolve(lostView({ code: 'Timeout', message: `No answer in ${ANSWER_MS / 1000} s` }, params.cursor ?? null, params.before ?? null))
        }, ANSWER_MS)
      })
      return await Promise.race([api.call<LogView>('GET', `${path}${q({ ...params, rid })}`), late])
    } catch (error) {
      return lostView(error, params.cursor ?? null, params.before ?? null)
    } finally {
      clearTimeout(timer)
      if (live.current.flight === rid) live.current.flight = null
    }
  }, [api, path])

  const tick = useCallback(async (fresh = false) => {
    const now = live.current
    clearTimeout(now.timer)
    now.timer = undefined
    if (!now.mounted || now.flight) return
    if (document.visibilityState !== 'visible') return
    // a window that no longer reaches the log's end is not followed: only "Back to latest" (a fresh tail) reads
    if (!fresh && !now.state.live) return
    const cursor = fresh ? null : now.state.cursor
    setReading(true)
    let view = await get(cursor ? { cursor, limit: FORWARD_LIMIT } : {})
    if (!live.current.mounted) return
    let brought = 0
    let more = false
    if (view.stale && view.stale.code === 'Cancelled') {
      // dropped on the way (not by this pane, which is still mounted): nothing learned, ask again later
      setReading(false)
      if (!live.current.paused) live.current.timer = setTimeout(() => void tick(), live.current.interval)
      return
    }
    if (view.stale && cursor && view.stale.code === 'InvalidInput') {
      // the log was rotated or replaced: its tail, once, without a cursor (a new read, not a retry)
      const tail = await get({})
      if (!live.current.mounted) return
      if (tail.stale) dispatch({ type: 'refused', view: tail })
      else { dispatch({ type: 'replaced', view: tail }); brought = tail.lines.length + 1 }
      view = tail
    } else if (view.stale) dispatch({ type: 'refused', view })
    else {
      dispatch({ type: cursor ? 'forward' : 'tail', view })
      brought = view.lines.length
      more = !!cursor && view.hasMore && view.lines.length > 0
    }
    setReading(false)
    const after = live.current
    after.first = false
    if (view.stale && stopsPolling(view.stale.code, after.final)) { setStopped(view.stale.code); return }
    if (view.stale) after.interval = nextInterval(after.interval, false)
    else { setStopped(null); after.interval = nextInterval(after.interval, brought > 0) }
    if (after.paused) return
    after.timer = setTimeout(() => void tick(), more ? MORE_MS : after.interval)
  }, [get])

  // mount: read; unmount (the tab is left): stop, and drop a read still in flight
  useEffect(() => {
    const now = live.current
    now.mounted = true
    now.interval = POLL_MS
    now.first = true
    dispatch({ type: 'reset' })
    setStopped(null)
    void tick(true)
    const seen = () => { if (document.visibilityState === 'visible') { if (!live.current.stopped && !(live.current.paused && !live.current.first)) void tick() } else { clearTimeout(live.current.timer); live.current.timer = undefined } }
    document.addEventListener('visibilitychange', seen)
    return () => {
      now.mounted = false
      clearTimeout(now.timer)
      document.removeEventListener('visibilitychange', seen)
      if (now.flight) void api.op('inspect.cancel', { rid: now.flight }).catch(() => {})
    }
  }, [api, path, tick])

  // the machine started again: follow again
  const wasPaused = useRef(paused)
  useEffect(() => {
    if (wasPaused.current && !paused && !live.current.stopped) { live.current.interval = POLL_MS; void tick() }
    wasPaused.current = paused
  }, [paused, tick])

  const again = useCallback((fresh = false) => {
    live.current.interval = POLL_MS
    setStopped(null)
    live.current.stopped = null
    void tick(fresh)
  }, [tick])

  const earlier = useCallback(() => {
    const now = live.current
    const before = now.state.before
    if (!before || now.flight) return
    clearTimeout(now.timer)
    setPaging(true)
    void get({ before, limit: EARLIER_LIMIT }).then((view) => {
      if (!live.current.mounted) return
      if (view.stale && view.stale.code === 'Cancelled') return
      if (view.stale) dispatch({ type: 'earlierRefused', view })
      else dispatch({ type: 'earlier', view })
    }).finally(() => {
      if (!live.current.mounted) return
      setPaging(false)
      const after = live.current
      if (!after.paused && !after.stopped) after.timer = setTimeout(() => void tick(), after.interval)
    })
  }, [get, tick])

  return { state, reading, paging, stopped, again, earlier }
}

export interface LogPaneProps {
  kind: 'machine' | 'sandbox'
  name: string
  /** The scroller's name for assistive technology ("Worker log of docker"). */
  label: string
  /** The machine is stopped (or cannot run here): its log cannot be read. Shown above the last lines, with `action` (Start). */
  unavailable?: { message: string, action?: ReactNode } | null
  /** The target is lost or gone: a refusal ends polling. */
  final?: boolean
}

export function LogPane({ kind, name, label, unavailable = null, final = false }: LogPaneProps): ReactNode {
  const { api, t } = useMachinesContext()
  const path = kind === 'machine' ? `/api/machines/${enc(name)}/log` : `/api/sandboxes/${enc(name)}/log`
  const { state, reading, paging, stopped, again, earlier } = useFollow(api, path, !!unavailable, final)
  const { rows } = state
  const [wrap, setWrap] = useState(readWrap)
  const [find, setFind] = useState('')
  const [at, setAt] = useState(0)
  const [behind, setBehind] = useState(0)
  const [copied, setCopied] = useState(false)
  const scroller = useRef<HTMLDivElement>(null)
  const stick = useRef(true)
  const seen = useRef({ last: 0, first: 0, height: 0, top: 0 })

  const hits = useMemo(() => findRows(rows, find), [rows, find])
  const current = hits.length ? hits[Math.min(at, hits.length - 1)] : -1
  useEffect(() => { setAt(0) }, [find])

  // follow while the reader is at the bottom; else count what arrived below, and keep the place when lines are put in front
  useLayoutEffect(() => {
    const el = scroller.current
    if (!el) return
    const last = rows.at(-1)?.id ?? 0, first = rows[0]?.id ?? 0
    const before = seen.current
    if (first !== before.first && before.first !== 0 && last === before.last) {
      // earlier lines went in front: the line the reader was on stays where it was
      el.scrollTop = before.top + (el.scrollHeight - before.height)
    } else if (stick.current) { el.scrollTop = el.scrollHeight; setBehind(0) }
    else if (last > before.last) setBehind((n) => n + rows.filter((r) => r.id > before.last).length)
    seen.current = { last, first, height: el.scrollHeight, top: el.scrollTop }
  }, [rows])
  const onScroll = () => {
    const el = scroller.current
    if (!el) return
    stick.current = el.scrollHeight - el.scrollTop - el.clientHeight <= STICK_PX
    seen.current.top = el.scrollTop
    seen.current.height = el.scrollHeight
    if (stick.current) setBehind(0)
  }
  const toBottom = () => {
    const el = scroller.current
    if (el) el.scrollTop = el.scrollHeight
    stick.current = true
    setBehind(0)
  }
  // wrapping changes every height: stay at the bottom when following
  useLayoutEffect(() => { const el = scroller.current; if (el && stick.current) el.scrollTop = el.scrollHeight }, [wrap])

  // bring the current match into view
  useEffect(() => {
    if (current < 0) return
    const row = scroller.current?.querySelector<HTMLElement>(`[data-row="${rows[current]?.id}"]`)
    if (!row) return
    stick.current = false
    row.scrollIntoView({ block: 'center' })
  }, [current, find]) // eslint-disable-line react-hooks/exhaustive-deps
  const step = (by: number) => { if (hits.length) setAt((n) => (Math.min(n, hits.length - 1) + by + hits.length) % hits.length) }
  const onFindKey = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Enter') { e.preventDefault(); step(e.shiftKey ? -1 : 1) } else if (e.key === 'Escape' && find) { e.preventDefault(); setFind('') }
  }

  const copy = () => {
    const el = scroller.current
    const selection = typeof window.getSelection === 'function' ? window.getSelection() : null
    const picked = selection && !selection.isCollapsed && el && selection.anchorNode && el.contains(selection.anchorNode) ? selection.toString() : ''
    const text = picked || logText(rows, t('logReplaced'))
    void navigator.clipboard?.writeText(text).then(() => { setCopied(true); setTimeout(() => setCopied(false), 1200) }, () => {})
  }

  const stale = state.stale
  const time = (ms: number | null) => (ms == null ? null : clockTime(ms))
  const lines = rows.filter((r) => r.kind === 'line').length
  return (
    <section className={css.pane} aria-label={label} data-log={kind} data-log-state={!state.loaded ? 'reading' : stale ? 'stale' : 'ready'} data-log-lines={lines}>
      <div className={css.toolbar}>
        <Input className={css.find} icon={<IconSearchOutlineRegular size={14} />} placeholder={t('logFind')} aria-label={t('logFind')}
          value={find} onChange={(e) => setFind(e.target.value)} onKeyDown={onFindKey} />
        {find && (
          <span className={css.findCount} role="status" aria-live="polite">
            {hits.length ? t('logFindCount', { at: String(Math.min(at, hits.length - 1) + 1), n: String(hits.length) }) : t('logFindNone')}
          </span>
        )}
        {find && hits.length > 1 && (
          <>
            <Button variant="ghost" size="sm" className={css.iconButton} aria-label={t('logFindPrev')} title={t('logFindPrev')} icon={<IconChevronUpOutlineRegular size={14} />} onClick={() => step(-1)} />
            <Button variant="ghost" size="sm" className={css.iconButton} aria-label={t('logFindNext')} title={t('logFindNext')} icon={<IconChevronDownOutlineRegular size={14} />} onClick={() => step(1)} />
          </>
        )}
        <span className={css.spacer} />
        <span className={css.status} role="status" aria-live="polite" data-log-status>
          {!state.loaded ? '' : stopped ? t('logStopped') : unavailable ? t('logPaused') : !state.live ? t('logNotLive') : stale ? t('logStaleShort') : t('logFollowing', { n: String(lines) })}
        </span>
        <button type="button" className={css.toggle} aria-pressed={wrap} onClick={() => { setWrap((on) => { writeWrap(!on); return !on }) }}>{t('logWrap')}</button>
        <Button variant="ghost" size="sm" icon={copied ? <IconCheckOutlineRegular size={14} /> : <IconCopyOutlineRegular size={14} />} onClick={copy} disabled={!rows.length} title={t('logCopyTitle')}>
          {copied ? t('copied') : t('copy')}
        </Button>
        <Button variant="ghost" size="sm" className={css.iconButton} aria-label={t('logReadAgain')} title={t('logReadAgain')} disabled={reading || paging}
          icon={<IconRefreshOutlineRegular size={14} />} onClick={() => again(!state.live)} />
      </div>

      {unavailable && (
        <Notice tone="warning" className={css.banner}>
          <div className={css.bannerRow} data-log-unavailable>
            <p>Unavailable · {unavailable.message}</p>
            {unavailable.action}
          </div>
        </Notice>
      )}
      {stale && !(unavailable && stale.code === 'Unavailable') && (
        <Notice tone={stopped ? 'danger' : 'warning'} className={css.banner} role="status">
          <div className={css.bannerRow} data-log-stale={stale.code}>
            <p>
              {t(stopped ? 'logRefused' : 'logStale')} · {stale.code}{stale.message ? ` · ${stale.message}` : ''}
              {rows.length > 0 && time(state.readAt) ? <> · {t('logShownFrom', { time: time(state.readAt)! })}</> : null}
            </p>
            <Button variant="outline" size="sm" disabled={reading} onClick={() => again(false)}>{reading ? t('logReading') : t('logReadAgain')}</Button>
          </div>
        </Notice>
      )}
      {!state.live && (
        <Notice className={css.banner}>
          <div className={css.bannerRow}>
            <p>{t('logNotLiveBody')}</p>
            <Button variant="outline" size="sm" disabled={reading} onClick={() => { stick.current = true; again(true) }}>{t('logBackToLatest')}</Button>
          </div>
        </Notice>
      )}

      <div className={css.body}>
        <div ref={scroller} className={clsx(css.scroll, wrap && css.wrap)} onScroll={onScroll} role="log" aria-live="off" aria-label={label} tabIndex={0} data-log-scroll>
          {!state.loaded ? <Loading label={t('logReading')} />
            : rows.length === 0 ? (stale || unavailable ? null : <EmptyState title={t('logEmpty')}>{t('logEmptyBody')}</EmptyState>)
              : (
                <>
                  {state.before
                    ? <button type="button" className={css.earlier} disabled={paging || reading} onClick={earlier} data-log-earlier>{paging ? t('logReading') : t('logEarlier')}</button>
                    : state.cutTop ? <p className={css.note} data-log-cut>{t('logCutTop')}</p> : <p className={css.note} data-log-start>{t('logStart')}</p>}
                  {rows.map((row, i) => <Row key={row.id} row={row} find={hits.length ? find : ''} current={i === current} replaced={t('logReplaced')} />)}
                </>
              )}
        </div>
        {behind > 0 && <button type="button" className={css.jump} onClick={toBottom} data-log-jump>↓ {t(behind === 1 ? 'logNewOne' : 'logNewMany', { n: String(behind) })}</button>}
      </div>
    </section>
  )
}

/** One visual line; rendered again only when it or the search changes. */
const Row = memo(function Row({ row, find, current, replaced }: { row: LogRow, find: string, current: boolean, replaced: string }): ReactNode {
  if (row.kind === 'replaced') return <div className={css.divider} role="separator" data-row={row.id} data-log-replaced><span>{replaced}</span></div>
  if (!find) return <div className={css.line} data-row={row.id}>{row.text}</div>
  const parts = matchParts(row.text, find)
  return (
    <div className={css.line} data-row={row.id} data-current={current || undefined}>
      {parts.map((p, i) => (p.hit ? <mark key={i} className={css.hit}>{p.text}</mark> : p.text))}
    </div>
  )
})
