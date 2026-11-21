/**
 * The Logs page: the LayerFS call log (`logs.list`), newest first, with
 * filters, a "More" row for older pages, and Reset behind a confirm.
 * Reloads on the host's `logs` event, on reconnection and on window focus
 * (the native log is written by other processes too). A filter can arrive
 * from outside through `openLogs` (a checkpoint's Calls). A row opens the
 * call's detail (click or Enter; ↑/↓ move between rows).
 */
import { usePageReading } from '../storage/PaneBar.tsx'
import { useCallback, useEffect, useId, useMemo, useRef, useState, type KeyboardEvent, type MouseEvent, type ReactNode } from 'react'
import clsx from 'clsx'
import {
  Button, Checkbox, IconCloseOutlineRegular, IconRefreshOutlineRegular, IconSearchOutlineRegular, Input, Modal, SegmentedControl, Tag, Tooltip,
} from '@deepseek-ai/dsh-client-ui-primitives'
import type { LayerfsApi } from '../api.ts'
import {
  LOGS_EVENT, operationOf, parseTag,
  type CallClient, type CallLabel, type LabeledLine, type LogsListAnswer, type LogsListArgs, type LogsResetAnswer,
} from '../../shared/logs.ts'
import { Card, Empty, LinkButton, LoadError, Loading, MenuSelect, Page, PageHead, Refusal, type RefusalLabels } from '../ui/index.tsx'
import { CallDetail } from './CallDetail.tsx'
import { CommandCell } from './Command.tsx'
import { shortId } from '../files/format.ts'
import { clockTime, dateTime } from '../ui/locale-sync.ts'
import { absoluteTime, commandParts, duration, kindWords } from './format.ts'
import type { LogsT } from './locale.ts'
import { openLogs, onLogsRequest, peekLogsRequest, takeLogsRequest, type LogsRequest } from './request.ts'
import { sessionParts } from './session-words.ts'
import css from './logs.module.css'

type ClientChoice = 'all' | CallClient
type OutcomeChoice = 'all' | 'ok' | 'refused'

interface Filters {
  project: string
  op: string
  client: ClientChoice
  outcome: OutcomeChoice
  writes: boolean
  tag?: string | undefined
}

const DEFAULTS: Filters = { project: '', op: 'all', client: 'all', outcome: 'all', writes: true }
/** A pre-filtered open shows exactly the tagged calls: every other filter is cleared. */
const fromRequest = (request: LogsRequest): Filters => ({ ...DEFAULTS, writes: false, tag: request.tag })

interface Loaded {
  lines: LabeledLine[]
  next?: string | undefined
  hasMore: boolean
  skipped: number
  source: LogsListAnswer['source']
  /** Pages appended through More; a background reload keeps them. */
  pages: number
}

const PAGE = 100
const lineKey = (line: LabeledLine) => `${line.at}|${line.tag ?? ''}|${line.argv.join('\u0000')}`
const merge = (a: readonly LabeledLine[], b: readonly LabeledLine[]) => [...new Map([...a, ...b].map((line) => [lineKey(line), line])).values()]
/** A saved id as LayerFS labels it short (its tag kept: a prefix `commit get` accepts); the full id is the tip. */
const shortHex = (id: string | undefined) => id ? shortId(id) : ''

function argsOf(f: Filters): LogsListArgs {
  return {
    limit: PAGE,
    ...(f.project.trim() ? { project: f.project.trim() } : {}),
    ...(f.op !== 'all' ? { op: f.op } : {}),
    ...(f.client !== 'all' ? { client: f.client } : {}),
    // `refused` is every outcome but `ok`; the contract filters one code, so it is narrowed here.
    ...(f.outcome === 'ok' ? { outcome: 'ok' } : {}),
    ...(f.tag ? { tag: f.tag } : {}),
    ...(f.writes ? { writes: true } : {}),
  }
}

function refusalLabels(t: LogsT): RefusalLabels {
  return {
    category: (kind) => kind === 'usage' ? t('refusalUsage') : kind === 'unavailable' ? t('refusalUnavailable') : kind === 'unknown' ? t('refusalUnknown') : t('refusalRefusal'),
    inTheWay: t('inTheWay'),
    kind: (kind) => kind.charAt(0).toUpperCase() + kind.slice(1),
    phase: (phase) => t('phase', { phase }),
    operation: (op) => t('operation', { op }),
  }
}

export function LogsPage({ api, t, refreshKey = 0 }: { refreshKey?: number, api: LayerfsApi, t: LogsT }): ReactNode {
  const [filters, setFilters] = useState<Filters>(() => { const r = peekLogsRequest(); return r ? fromRequest(r) : DEFAULTS })
  const [search, setSearch] = useState(filters.project)
  const [data, setData] = useState<Loaded>()
  const [error, setError] = useState<unknown>()
  const [loading, setLoading] = useState(true)
  usePageReading(loading)
  const [more, setMore] = useState(false)
  const [ops, setOps] = useState<ReadonlySet<string>>(() => new Set())
  const [confirming, setConfirming] = useState(false)
  const [resetting, setResetting] = useState(false)
  const [resetError, setResetError] = useState<unknown>()
  const [removed, setRemoved] = useState<number>()
  const [now, setNow] = useState(() => Date.now())
  /** The row whose detail is open, by key (it survives reloads that keep the line). */
  const [detail, setDetail] = useState<string>()
  const tbody = useRef<HTMLTableSectionElement>(null)
  const generation = useRef(0)
  const current = useRef({ filters, data })
  current.current = { filters, data }

  /** `replace` starts over; `refresh` keeps appended pages and puts new lines on top. */
  // A refresh (one per `logs` event, about every second in a run) never cancels a read in
  // flight, or a page slower than a second would never land: it queues one more read.
  const inFlight = useRef(false)
  const queued = useRef(false)
  const load = useCallback(async (mode: 'replace' | 'refresh'): Promise<void> => {
    if (mode === 'refresh' && inFlight.current) { queued.current = true; return }
    if (mode === 'replace') queued.current = false
    inFlight.current = true
    const mine = ++generation.current
    const { filters: f, data: before } = current.current
    setLoading(true)
    try {
      const answer = await api.op<LogsListAnswer>('logs.list', argsOf(f))
      if (mine !== generation.current) return
      const keep = mode === 'refresh' && before && before.pages > 1
      setData({
        lines: keep ? merge(answer.lines, before.lines) : answer.lines,
        next: keep ? before.next : answer.next,
        hasMore: keep ? before.hasMore : answer.has_more,
        skipped: answer.skipped,
        source: answer.source,
        pages: keep ? before.pages : 1,
      })
      setError(undefined)
      setNow(Date.now())
    } catch (e) {
      if (mine === generation.current) setError(e)
    } finally {
      if (mine === generation.current) {
        inFlight.current = false
        if (queued.current) { queued.current = false; void load('refresh') }
        else setLoading(false)
      }
    }
  }, [api])

  const loadMore = async () => {
    const before = current.current.data
    if (!before?.next || more) return
    const mine = generation.current
    setMore(true)
    try {
      const answer = await api.op<LogsListAnswer>('logs.list', { ...argsOf(current.current.filters), cursor: before.next })
      // A reload or filter change since makes this page stale.
      if (mine !== generation.current) return
      setData({ lines: merge(before.lines, answer.lines), next: answer.next, hasMore: answer.has_more, skipped: before.skipped + answer.skipped, source: answer.source, pages: before.pages + 1 })
    } catch (e) {
      if (mine === generation.current) setError(e)
    } finally { setMore(false) }
  }

  useEffect(() => { void load('replace') }, [load, filters])
  const lastRefresh = useRef(refreshKey)
  useEffect(() => {
    if (lastRefresh.current !== refreshKey) { lastRefresh.current = refreshKey; void load('refresh') }
  }, [load, refreshKey])
  // The search box settles before it filters.
  useEffect(() => {
    if (search === filters.project) return
    const id = setTimeout(() => setFilters((f) => ({ ...f, project: search })), 300)
    return () => clearTimeout(id)
  }, [search, filters.project])
  useEffect(() => {
    const refresh = () => { void load('refresh') }
    // The request this page opened with was read in the first render; it is consumed now.
    takeLogsRequest()
    const offs = [
      api.onEvent(LOGS_EVENT, refresh),
      api.onEvent('connection', (e) => { if ((e.data as { connected?: boolean } | undefined)?.connected) refresh() }),
      onLogsRequest(() => { const r = takeLogsRequest(); if (r) { setFilters(fromRequest(r)); setSearch('') } }),
    ]
    window.addEventListener('focus', refresh)
    const tick = setInterval(() => setNow(Date.now()), 30_000)
    return () => { for (const off of offs) off(); window.removeEventListener('focus', refresh); clearInterval(tick) }
  }, [api, load])
  // Op choices come from what has been seen, so they stay while an op is chosen.
  useEffect(() => {
    if (!data) return
    setOps((seen) => {
      const next = new Set(seen)
      for (const line of data.lines) { next.add(line.argv[0] ?? ''); next.add(operationOf(line.argv)) }
      next.delete('')
      return next.size === seen.size ? seen : next
    })
  }, [data])

  const shown = useMemo(() => (data?.lines ?? []).filter((line) => filters.outcome !== 'refused' || line.outcome !== 'ok'), [data, filters.outcome])
  const set = (patch: Partial<Filters>) => setFilters((f) => ({ ...f, ...patch }))
  const detailIndex = detail === undefined ? -1 : shown.findIndex((line) => lineKey(line) === detail)
  const detailLine = detailIndex >= 0 ? shown[detailIndex] : undefined
  const labels = useMemo(() => refusalLabels(t), [t])
  /** Back on the table after the detail closes or moves: focus the row it showed. */
  const focusRow = (key: string | undefined) => {
    if (key === undefined) return
    requestAnimationFrame(() => {
      const row = [...(tbody.current?.querySelectorAll<HTMLElement>('tr[data-key]') ?? [])].find((el) => el.dataset.key === key)
      row?.focus({ preventScroll: false })
    })
  }
  // A detail whose line is gone (a filter change, Reset) closes instead of reopening later.
  useEffect(() => { if (detail !== undefined && data && detailIndex < 0) setDetail(undefined) }, [detail, data, detailIndex])
  const closeDetail = () => { const key = detail; setDetail(undefined); focusRow(key) }
  // Arrow keys move focus between rows.
  const onRowKey = (e: KeyboardEvent<HTMLTableRowElement>, key: string) => {
    if (e.target !== e.currentTarget || e.altKey || e.ctrlKey || e.metaKey) return
    if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); setDetail(key); return }
    if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return
    const row = e.key === 'ArrowDown' ? e.currentTarget.nextElementSibling : e.currentTarget.previousElementSibling
    if (row instanceof HTMLElement) { e.preventDefault(); row.focus() }
  }
  /** Filters other than Writes only (and the tag, which has its own empty state). */
  const filtered = filters.project.trim() !== '' || filters.op !== 'all' || filters.client !== 'all' || filters.outcome !== 'all'
  /**
   * Whether the whole log is empty: known from an unfiltered read, else asked
   * with a one-line probe when a filtered read shows nothing. `undefined` while unknown.
   */
  const [logEmpty, setLogEmpty] = useState<boolean>()
  useEffect(() => {
    if (!data) return
    const args = argsOf(filters)
    const unfiltered = Object.keys(args).every((k) => k === 'limit') && filters.outcome === 'all'
    if (data.lines.length > 0) { setLogEmpty(false); return }
    if (data.hasMore) return
    if (unfiltered) { setLogEmpty(true); return }
    let live = true
    api.op<LogsListAnswer>('logs.list', { limit: 1 })
      .then((answer) => { if (live) setLogEmpty(answer.lines.length === 0 && !answer.has_more) })
      .catch(() => { if (live) setLogEmpty(undefined) })
    return () => { live = false }
  }, [api, data, filters])

  const reset = async () => {
    setResetting(true); setResetError(undefined)
    try {
      const answer = await api.op<LogsResetAnswer>('logs.reset', {})
      setRemoved(answer.removed)
      setConfirming(false)
      await load('replace')
    } catch (e) { setResetError(e) } finally { setResetting(false) }
  }

  const opOptions = useMemo(() => [
    { value: 'all', label: t('opAll') },
    ...[...ops].sort().map((op) => ({ value: op, label: op })),
  ], [ops, t])

  return (
    <Page wide label={t('title')}>
      <PageHead title={t('title')}
        actions={(
          <>
            <Button variant="ghost" size="sm" icon={<IconRefreshOutlineRegular size={14} />} disabled={loading} onClick={() => { void load('replace') }}>{t('refresh')}</Button>
            <Button variant="outline" size="sm" disabled={resetting || !data || logEmpty === true} onClick={() => { setResetError(undefined); setConfirming(true) }}>{t('reset')}</Button>
          </>
        )} />
      {removed !== undefined && <p className={css.note} role="status">{t('resetDone', { n: removed })}</p>}

      <div className={css.filters} role="group" aria-label={t('filters')}>
        <Input className={css.search} icon={<IconSearchOutlineRegular size={14} />} placeholder={t('searchProject')} aria-label={t('searchProject')}
          value={search} onChange={(e) => setSearch(e.target.value)} onKeyDown={(e) => { if (e.key === 'Escape' && search) { e.preventDefault(); setSearch('') } }} />
        <MenuSelect value={filters.op} label={t('op')} options={opOptions} onChange={(op) => set({ op })} className={css.opSelect} />
        <SegmentedControl<ClientChoice> id="lfs-logs-client" value={filters.client} label={t('client')} onChange={(client) => set({ client })}
          options={[{ value: 'all', label: t('clientAll') }, { value: 'cli', label: 'Cli' }, { value: 'node', label: 'Node' }, { value: 'python', label: 'Python' }, { value: 'rust', label: 'Rust' }]} />
        <SegmentedControl<OutcomeChoice> id="lfs-logs-outcome" value={filters.outcome} label={t('outcome')} onChange={(outcome) => set({ outcome })}
          options={[{ value: 'all', label: t('outcomeAll') }, { value: 'ok', label: t('outcomeOk') }, { value: 'refused', label: t('outcomeRefused') }]} />
        <Checkbox checked={filters.writes} onChange={(writes) => set({ writes })} label={t('writesOnly')} />
        {filters.tag && <TagChip t={t} tag={filters.tag} onClear={() => set({ tag: undefined })} />}
      </div>

      {error !== undefined && !data && <LoadError title={t('loadFailed')} error={error} labels={refusalLabels(t)} retryLabel={t('retry')} onRetry={() => { void load('replace') }} />}
      {error !== undefined && data && <Refusal error={error} labels={refusalLabels(t)} compact />}
      {!data && loading && error === undefined && <Loading label={t('loading')} />}
      {data && (shown.length === 0 && !data.hasMore ? (
        filters.tag ? <Empty title={t('emptyTag')}>{t('emptyTagBody')}</Empty>
          : filtered ? <Empty title={t('emptyFiltered')}>{t('emptyFilteredBody')}</Empty>
            : filters.writes && logEmpty !== true ? (
              <Empty title={t('emptyWrites')}>
                {t('emptyWritesBody')} <LinkButton onClick={() => set({ writes: false })}>{t('showReads')}</LinkButton>
              </Empty>
            ) : <Empty title={t('empty')}>{t('emptyBody')}</Empty>
      ) : (
        <Card className={css.tableCard}>
          <table className={css.table} aria-busy={loading || more} aria-label={t('tableLabel')}>
            <thead>
              <tr>
                <th className={css.wTime}>{t('colTime')}</th>
                <th className={css.wClient}>{t('colClient')}</th>
                <th>{t('colCommand')}</th>
                <th className={css.wOutcome}>{t('colOutcome')}</th>
                <th className={clsx(css.wMs, css.num)}>{t('colMs')}</th>
                <th className={css.wSaved}>{t('colSaved')}</th>
                <th className={css.wSession}>{t('colSession')}</th>
              </tr>
            </thead>
            <tbody ref={tbody}>
              {shown.map((line) => {
                const key = lineKey(line)
                return <Row key={key} rowKey={key} api={api} t={t} line={line} now={now} detailOpen={detail !== undefined}
                  onOpen={() => setDetail(key)} onKeyDown={(e) => onRowKey(e, key)} />
              })}
            </tbody>
          </table>
          <div className={css.moreRow}>
            {data.next && data.hasMore
              ? <Button variant="outline" size="sm" disabled={more} aria-busy={more} onClick={() => { void loadMore() }}>{more ? t('loadingMore') : t('more')}</Button>
              : <p className={css.note}>{t('allLoaded')}</p>}
            {data.skipped > 0 && <p className={css.note}>{t('skipped', { n: data.skipped })}</p>}
          </div>
        </Card>
      ))}

      <CallDetail api={api} t={t} line={detailLine} labels={labels} onClose={closeDetail}
        onCalls={(tag) => { setDetail(undefined); openLogs(api, { tag }) }}
        nav={{
          index: detailIndex,
          total: shown.length,
          prev: detailIndex > 0 ? () => setDetail(lineKey(shown[detailIndex - 1]!)) : undefined,
          next: detailIndex >= 0 && detailIndex < shown.length - 1 ? () => setDetail(lineKey(shown[detailIndex + 1]!)) : undefined,
        }} />

      <Modal open={confirming} onClose={() => { if (!resetting) setConfirming(false) }} closeLabel={t('close')} className={css.confirmModal}
        title={t('confirmReset')}
        description={t(data?.source === 'layerfs' ? 'confirmResetLayerfs' : 'confirmResetPlugin')}
        footer={(
          <div className={css.modalFooter}>
            <Button variant="outline" disabled={resetting} data-modal-autofocus="" onClick={() => setConfirming(false)}>{t('cancel')}</Button>
            <Button variant="primary" className={css.dangerPrimary} disabled={resetting} aria-busy={resetting} onClick={() => { void reset() }}>
              {resetting ? t('resetting') : t('resetAction')}
            </Button>
          </div>
        )}>
        {resetError !== undefined ? <Refusal error={resetError} labels={refusalLabels(t)} /> : null}
      </Modal>
    </Page>
  )
}

/** The outside filter, removable. */
/** What a tag filter holds, in words: one call, a session (its id without DSH's `session-` prefix), else the tag. */
export function tagWords(tag: string, t: LogsT): string {
  const parsed = parseTag(tag)
  return parsed ? t('tagCall', { id: parsed.receiptId.slice(0, 8) })
    : tag.startsWith('dsh:') && tag.endsWith(':') ? t('tagSession', { id: tag.slice(4, -1).replace(/^session-/, '').slice(0, 12) }) : tag
}

function TagChip({ t, tag, onClear }: { t: LogsT, tag: string, onClear: () => void }): ReactNode {
  const what = tagWords(tag, t)
  return (
    <span className={css.chip} title={tag}>
      <span className={css.chipLabel}>{t('tagFilter', { what })}</span>
      <button type="button" className={css.chipClear} aria-label={t('clearTag')} title={t('clearTag')} onClick={onClear}>
        <IconCloseOutlineRegular size={12} />
      </button>
    </span>
  )
}

function when(at: number, now: number, t: LogsT): string {
  const s = Math.max(0, Math.round((now - at) / 1000))
  if (s < 45) return t('justNow')
  if (s < 3600) return t('minutesAgo', { n: Math.max(1, Math.round(s / 60)) })
  if (s < 6 * 3600) return t('hoursAgo', { n: Math.round(s / 3600) })
  const date = new Date(at)
  const today = new Date(now).toDateString() === date.toDateString()
  return today ? clockTime(date, { timeStyle: 'short' }) : dateTime(date, { dateStyle: 'medium', timeStyle: 'short' })
}

/** Clicks on the row's own controls, or ending a text selection, do not open the detail. */
function opensDetail(e: MouseEvent<HTMLTableRowElement>): boolean {
  if ((e.target as HTMLElement).closest('button, a, input, [role=tooltip]')) return false
  const selection = window.getSelection()
  return !(selection && !selection.isCollapsed && e.currentTarget.contains(selection.anchorNode))
}

function Row({ api, t, line, now, rowKey, detailOpen, onOpen, onKeyDown }: {
  api: LayerfsApi
  t: LogsT
  line: LabeledLine
  now: number
  rowKey: string
  detailOpen: boolean
  onOpen: () => void
  onKeyDown: (e: KeyboardEvent<HTMLTableRowElement>) => void
}): ReactNode {
  const parts = commandParts(line.argv)
  const ok = line.outcome === 'ok'
  const version = line.commit ?? line.layer
  const [focused, setFocused] = useState(false)
  const popoverId = useId()
  return (
    <tr className={css.row} tabIndex={0} data-key={rowKey} aria-describedby={focused ? popoverId : undefined}
      onClick={(e) => { if (opensDetail(e)) onOpen() }} onKeyDown={onKeyDown}
      onFocus={(e) => { if (e.target === e.currentTarget && e.currentTarget.matches(':focus-visible')) setFocused(true) }}
      onBlur={(e) => { if (e.target === e.currentTarget) setFocused(false) }}>
      <td>
        <Tooltip label={absoluteTime(line.at)} side="top" portal delayMs={400}>
          <span className={css.time}>{when(line.at, now, t)}</span>
        </Tooltip>
      </td>
      <td className={clsx(css.muted, css.wClient)}>{line.client}</td>
      <td className={css.commandCell}><CommandCell parts={parts} focusOpen={focused} popoverId={popoverId} disabled={detailOpen} /></td>
      <td title={ok ? undefined : line.outcome}>{ok ? <span className={css.ok}>{t('outcomeOk')}</span> : <Tag tone="danger" className={css.codeTag}>{line.outcome}</Tag>}</td>
      <td className={clsx(css.num, css.duration)}>{duration(line.ms, t)}</td>
      <td className={clsx(css.saved, css.wSaved)}>
        {version ? <>{line.kind && <span className={css.kind} title={line.kind}>{kindWords(line.kind, t)}</span>}<code className={css.hex} title={version}>{shortHex(version)}</code></> : <span className={css.muted}>—</span>}
      </td>
      <td>{line.dsh ? <SessionLabel api={api} t={t} label={line.dsh} /> : <span className={css.muted}>—</span>}</td>
    </tr>
  )
}

/** Which session, turn and checkpoint a line belongs to; the name opens the session. */
function SessionLabel({ api, t, label }: { api: LayerfsApi, t: LogsT, label: CallLabel }): ReactNode {
  const name = label.title?.trim() || label.sessionId.slice(0, 8)
  const parts = sessionParts(label, t)
  return (
    <span className={css.sessionLabel}>
      <LinkButton className={css.sessionName} title={t('openSession', { name: label.title?.trim() || label.sessionId })} onClick={() => api.openSession(label.sessionId)}>{name}</LinkButton>
      {parts.length > 0 && <span className={css.muted}>{parts.join(' · ')}</span>}
    </span>
  )
}
