/**
 * The Storage page's navigation, in one bar over the page: Back and Forward
 * through the pages visited, a breadcrumb from Home (the Overview) down to
 * the page, Jump to (⌘K / Ctrl+K), the running transfers, Refresh and Add.
 * Doctor and Logs open their pages. Settings opens the setup modal.
 *
 * A crumb with siblings carries a switcher: the current page's crumb opens a
 * menu of the pages beside it (the other Stores, machines, projects in the
 * Store…), and an ancestor's chevron does the same for its level, so you move
 * sideways without going back. Jump to lists every machine, Store, project,
 * Sandbox, bucket and available session with where it lives, filtered as you type.
 */
import { useCallback, useEffect, useMemo, useRef, useState, useSyncExternalStore, type ReactNode } from 'react'
import clsx from 'clsx'
import {
  Button, IconChevronDownOutlineRegular, IconChevronLeftOutlineRegular, IconChevronRightOutlineRegular,
  IconPlusOutlineRegular, IconRefreshOutlineRegular, IconSearchOutlineRegular, IconSettingsOutlineRegular, IconShieldOutlineRegular, Menu, MenuItemButton, Modal, ShortcutKeys, StateDot,
  modalSelector,
} from '@deepseek-ai/dsh-client-ui-primitives'
import type { DoctorFinding } from '../../shared/doctor.ts'
import { LogsPanelIcon } from '../logs/index.tsx'
import { setupDialog } from '../setup/store.ts'
import type { SessionsSource } from '../views/hooks.ts'
import type { StorageT } from './locale.ts'
import { doingWords, shareOf } from './model.ts'
import { forgetPages, visitPage, type PageHistory, type TabId } from './tabs.ts'
import type { StorageJob } from './types.ts'
import css from './Nav.module.css'

/** Keep the last visit when DSH remounts Center on the same connection. */
const lastVisits = new WeakMap<object, PageHistory>()

/** Back and Forward over the pages visited. */
export interface NavHistory {
  current: TabId
  canBack: boolean
  canForward: boolean
  /** Show `id`, dropping anything Forward held (no-op when it already shows). */
  open: (id: TabId) => void
  back: () => void
  forward: () => void
  /** Drop pages that no longer exist (a removed Store and its projects); the page showing falls back to the one before. */
  forget: (gone: (id: TabId) => boolean) => void
}

export function useNavHistory(owner: object): NavHistory {
  const [h, setH] = useState<PageHistory>(() => lastVisits.get(owner) ?? { stack: ['overview'], at: 0 })
  useEffect(() => { lastVisits.set(owner, h) }, [owner, h])
  const open = useCallback((id: TabId) => setH((s) => visitPage(s, id)), [])
  const back = useCallback(() => setH((s) => (s.at > 0 ? { ...s, at: s.at - 1 } : s)), [])
  const forward = useCallback(() => setH((s) => (s.at < s.stack.length - 1 ? { ...s, at: s.at + 1 } : s)), [])
  const forget = useCallback((gone: (id: TabId) => boolean) => setH((s) => forgetPages(s, gone)), [])
  return {
    current: h.stack[h.at] ?? 'overview',
    canBack: h.at > 0,
    canForward: h.at < h.stack.length - 1,
    open, back, forward, forget,
  }
}

/** A page beside another, for a crumb's switcher. */
export interface Sibling {
  id: TabId
  label: string
  /** Where it lives or what it is, in a few words. */
  meta?: string | undefined
  /** Starts a group under this heading. */
  group?: string | undefined
}

/** One crumb: the last one is the page showing. */
export interface Crumb {
  key: string
  label: string
  icon?: ReactNode
  /** Where clicking the crumb leads (none on the page showing). */
  go?: (() => void) | undefined
  /** The pages beside this one, for its switcher; `id` marks which is this one. */
  siblings?: readonly Sibling[] | undefined
  id?: TabId | undefined
}

/** Something Jump to can open. */
export interface JumpItem {
  id: TabId
  /** Its group heading (Projects, Stores, …). */
  group: string
  label: string
  /** Where it lives, so a name reads without a tree around it. */
  where: string
  icon?: ReactNode
}

/** An entry in the Add menu. */
export interface AddItem {
  id: string
  label: string
  icon?: ReactNode
  run: () => void
  disabledReason?: string | undefined
}

/** A page of its own under Home (Doctor, Logs), also listed in Jump to. */
export interface NavPage {
  id: TabId
  label: string
}

export interface NavBarProps {
  t: StorageT
  history: NavHistory
  crumbs: readonly Crumb[]
  sessions: SessionsSource | undefined
  onSession: (id: string) => void
  jump: readonly JumpItem[]
  jobs: readonly StorageJob[]
  findings: readonly DoctorFinding[]
  refreshing: boolean
  /** None on Home, whose head card has Refresh. */
  onRefresh?: (() => void) | undefined
  add: readonly AddItem[]
  onOpen: (id: TabId) => void
}

const isMac = typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent)

export function NavBar({ t, history, crumbs, jump, sessions, onSession, jobs, findings, refreshing, onRefresh, add, onOpen }: NavBarProps): ReactNode {
  const [jumping, setJumping] = useState(false)
  const barRef = useRef<HTMLElement>(null)
  // ⌘K / Ctrl+K opens Jump to while this page shows and nothing modal is open over it.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key.toLowerCase() !== 'k' || !(isMac ? e.metaKey : e.ctrlKey) || e.altKey || e.shiftKey) return
      const bar = barRef.current
      if (!bar || bar.getClientRects().length === 0 || document.querySelector(modalSelector)) return
      e.preventDefault()
      setJumping(true)
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])
  const running = jobs.filter((j) => !j.ended && j.kind !== 'compact')
  return (
    <header ref={barRef} className={css.bar} aria-label={t('navLabel')}>
      <span className={css.history}>
        <Button variant="ghost" size="sm" className={css.iconButton} aria-label={t('navBack')} title={t('navBack')} disabled={!history.canBack}
          icon={<IconChevronLeftOutlineRegular size={16} />} onClick={history.back} />
        <Button variant="ghost" size="sm" className={css.iconButton} aria-label={t('navForward')} title={t('navForward')} disabled={!history.canForward}
          icon={<IconChevronRightOutlineRegular size={16} />} onClick={history.forward} />
      </span>
      <nav className={css.crumbs} aria-label={t('crumbsLabel')}>
        <ol className={css.crumbList}>
          {crumbs.map((c, i) => (
            <li key={c.key} className={css.crumbItem}>
              {i > 0 ? <IconChevronRightOutlineRegular size={12} className={css.crumbSep} aria-hidden="true" /> : null}
              <CrumbView crumb={c} current={i === crumbs.length - 1} t={t} onOpen={onOpen} />
            </li>
          ))}
        </ol>
      </nav>
      <span className={css.tools}>
        {running.length > 0 ? <Jobs jobs={running} t={t} onOpen={onOpen} /> : null}
        <button type="button" className={css.jumpField} onClick={() => setJumping(true)} aria-haspopup="dialog" aria-keyshortcuts={isMac ? 'Meta+K' : 'Control+K'}>
          <IconSearchOutlineRegular size={14} className={css.jumpIcon} aria-hidden="true" />
          <span className={css.jumpText}>{t('jumpTo')}</span>
          <ShortcutKeys keys={[isMac ? '⌘' : 'Ctrl', 'K']} className={css.jumpKeys} />
        </button>
        <Button variant="ghost" size="sm" className={findings.length ? css.countButton : css.iconButton} icon={<IconShieldOutlineRegular size={15} />}
          onClick={() => onOpen('health')} aria-label={t('navHealth')} title={findings.length ? t('navHealthFindings', { n: findings.length }) : t('navHealth')}>
          {findings.length ? <span className={css.toolCount} data-tone={findings.some((f) => f.severity === 'error') ? 'danger' : 'warning'}>{findings.length}</span> : null}
        </Button>
        <Button variant="ghost" size="sm" className={css.iconButton} icon={<LogsPanelIcon size={15} />} onClick={() => onOpen('logs')} aria-label={t('navLogs')} title={t('navLogs')} />
        <Button variant="ghost" size="sm" className={css.iconButton} icon={<IconSettingsOutlineRegular size={15} />} onClick={() => setupDialog.open()} aria-label={t('navSettings')} title={t('navSettings')} />
        {onRefresh ? (
          <Button variant="ghost" size="sm" className={css.iconButton} aria-label={t('refresh')} title={t('refresh')} aria-busy={refreshing}
            icon={refreshing ? <StateDot state="ongoing" size={13} /> : <IconRefreshOutlineRegular size={15} />} onClick={onRefresh} />
        ) : null}
        <AddMenu items={add} t={t} />
      </span>
      {jumping ? <JumpDialog items={jump} sessions={sessions} onSession={(id) => { setJumping(false); onSession(id) }} t={t} onClose={() => setJumping(false)} onOpen={(id) => { setJumping(false); onOpen(id) }} /> : null}
    </header>
  )
}

/** A crumb: a link, the page showing, and a switcher when it has siblings. */
function CrumbView({ crumb: c, current, t, onOpen }: { crumb: Crumb, current: boolean, t: StorageT, onOpen: (id: TabId) => void }): ReactNode {
  const [open, setOpen] = useState(false)
  const switcher = !!c.siblings && c.siblings.length > 1
  const body = (
    <>
      {c.icon ? <span className={css.crumbIcon} aria-hidden="true">{c.icon}</span> : null}
      <span className={css.crumbLabel}>{c.label}</span>
    </>
  )
  const menu = (anchor: ReactNode) => (
    <Menu open={open} onClose={() => setOpen(false)} portal autoFocus dense anchor={anchor} listClassName={css.switchList}>
      {(c.siblings ?? []).map((s, i) => (
        <MenuItemButton key={s.id} current={s.id === c.id} separatorBefore={i > 0 && !!s.group && s.group !== c.siblings?.[i - 1]?.group}
          onSelect={() => { setOpen(false); if (s.id !== c.id) onOpen(s.id) }}>
          <span className={css.sibling}>
            <span className={css.siblingName}>{s.label}</span>
            {s.meta ? <span className={css.siblingMeta}>{s.meta}</span> : null}
          </span>
        </MenuItemButton>
      ))}
    </Menu>
  )
  if (current) {
    if (!switcher) return <span className={clsx(css.crumb, css.crumbCurrent)} aria-current="page">{body}</span>
    return menu(
      <button type="button" className={clsx(css.crumb, css.crumbCurrent)} aria-current="page" aria-haspopup="menu" aria-expanded={open}
        title={t('crumbSwitch', { name: c.label })} onClick={() => setOpen((v) => !v)}>
        {body}<IconChevronDownOutlineRegular size={12} className={css.crumbCaret} aria-hidden="true" />
      </button>,
    )
  }
  return (
    <span className={css.crumbPair}>
      <button type="button" className={css.crumb} onClick={c.go}>{body}</button>
      {switcher ? menu(
        <button type="button" className={css.crumbMore} aria-label={t('crumbSwitch', { name: c.label })} title={t('crumbSwitch', { name: c.label })}
          aria-haspopup="menu" aria-expanded={open} onClick={() => setOpen((v) => !v)}>
          <IconChevronDownOutlineRegular size={12} aria-hidden="true" />
        </button>,
      ) : null}
    </span>
  )
}

/** The running transfers: one opens its Store; several open a list. */
function Jobs({ jobs, t, onOpen }: { jobs: readonly StorageJob[], t: StorageT, onOpen: (id: TabId) => void }): ReactNode {
  const [open, setOpen] = useState(false)
  const first = jobs[0]!
  const pct = (j: StorageJob) => Math.round(shareOf(j) * 100)
  const label = jobs.length === 1 ? `${first.store} · ${doingWords(first, t)}` : t('jobsMore', { n: jobs.length })
  const button = (
    <button type="button" className={css.jobs} title={jobs.map((j) => `${j.store} · ${doingWords(j, t)} ${pct(j)}%`).join('\n')}
      aria-haspopup={jobs.length > 1 ? 'menu' : undefined} aria-expanded={jobs.length > 1 ? open : undefined}
      onClick={() => (jobs.length === 1 ? onOpen(`store:${first.store}`) : setOpen((v) => !v))}>
      <span className={css.jobsLabel}>{label}</span>
      <span className={css.jobsShare}>{`${pct(first)}%`}</span>
      <span className={css.jobsTrack} aria-hidden="true"><span className={css.jobsFill} style={{ width: `${pct(first)}%` }} /></span>
    </button>
  )
  if (jobs.length === 1) return button
  return (
    <Menu open={open} onClose={() => setOpen(false)} portal autoFocus align="end" anchor={button}>
      {jobs.map((j) => (
        <MenuItemButton key={j.id} onSelect={() => { setOpen(false); onOpen(`store:${j.store}`) }}>
          <span className={css.sibling}>
            <span className={css.siblingName}>{j.store}</span>
            <span className={css.siblingMeta}>{`${doingWords(j, t)} · ${pct(j)}%`}</span>
          </span>
        </MenuItemButton>
      ))}
    </Menu>
  )
}

function AddMenu({ items, t }: { items: readonly AddItem[], t: StorageT }): ReactNode {
  const [open, setOpen] = useState(false)
  return (
    <Menu open={open} onClose={() => setOpen(false)} portal autoFocus align="end"
      anchor={(
        <Button variant="outline" size="sm" className={css.addButton} icon={<IconPlusOutlineRegular size={13} />} aria-haspopup="menu" aria-expanded={open}
          onClick={() => setOpen((v) => !v)}>{t('addMenu')}</Button>
      )}>
      {items.map((x) => <MenuItemButton key={x.id} icon={x.icon} disabled={!!x.disabledReason} disabledReason={x.disabledReason}
        onSelect={() => { setOpen(false); x.run() }}>{x.label}</MenuItemButton>)}
    </Menu>
  )
}

/** The most results Jump to lists at once. */
const JUMP_MAX = 60

const noSubscription = () => () => {}
const noCatalog = () => undefined

/** Jump to: a filtered list of everything, opened with Enter or a click. */
function JumpDialog({ items, sessions, onSession, t, onClose, onOpen }: { items: readonly JumpItem[], sessions: SessionsSource | undefined, onSession: (id: string) => void, t: StorageT, onClose: () => void, onOpen: (id: TabId) => void }): ReactNode {
  const [q, setQ] = useState('')
  const [cursor, setAt] = useState(0)
  const catalog = useSyncExternalStore(sessions?.subscribe ?? noSubscription, sessions?.getSnapshot ?? noCatalog)
  const listRef = useRef<HTMLDivElement>(null)
  const hits = useMemo(() => {
    const words = q.trim().toLowerCase().split(/\s+/).filter(Boolean)
    // byId may retain summaries outside the selectable catalog (e.g. a focused session).
    const available = (catalog?.ids ?? []).flatMap(id => {
      const row = catalog?.byId?.[id]
      if (!row) return []
      const projected = catalog?.projectionsBySession?.[id]?.values?.title
      const label = typeof projected === 'string' && projected.trim() ? projected : row.displayTitle || row.title || id
      return [{ sessionId: id, id, group: t('jumpSessions'), label, where: id }]
    })
    const entries = [...items, ...available]
    const scored = entries
      .filter((x) => { const hay = `${x.label} ${x.where}`.toLowerCase(); return words.every((w) => hay.includes(w)) })
      // a name that starts with the query first, then the rest in their groups' order
      .map((x, i) => ({ x, i, lead: words.length > 0 && x.label.toLowerCase().startsWith(words[0]!) ? 0 : 1 }))
    const groups = [...new Set(entries.map((x) => x.group))]
    return scored
      .sort((a, b) => groups.indexOf(a.x.group) - groups.indexOf(b.x.group) || a.lead - b.lead || a.i - b.i)
      .slice(0, JUMP_MAX).map((s) => s.x)
  }, [items, catalog, q, t])
  const at = Math.max(0, Math.min(cursor, hits.length - 1))
  const activate = (hit: (typeof hits)[number]) => 'sessionId' in hit ? onSession(hit.sessionId) : onOpen(hit.id)
  useEffect(() => {
    listRef.current?.querySelector<HTMLElement>(`[data-index="${at}"]`)?.scrollIntoView({ block: 'nearest' })
  }, [at])
  const optionId = (i: number) => `lfs-jump-${i}`
  return (
    <Modal open headless onClose={onClose} title={t('jumpTitle')} className={css.jumpDialog}>
      <div className={css.jumpQuery}>
        <IconSearchOutlineRegular size={16} className={css.jumpIcon} aria-hidden="true" />
        <input className={css.jumpInput} value={q} autoFocus spellCheck={false} autoComplete="off" placeholder={t('jumpPlaceholder')}
          role="combobox" aria-expanded="true" aria-controls="lfs-jump-list" aria-label={t('jumpTitle')}
          aria-activedescendant={hits.length ? optionId(at) : undefined}
          onChange={(e) => { setQ(e.target.value); setAt(0) }}
          onKeyDown={(e) => {
            if (e.key === 'ArrowDown') { e.preventDefault(); setAt(Math.min(at + 1, Math.max(0, hits.length - 1))) }
            else if (e.key === 'ArrowUp') { e.preventDefault(); setAt(Math.max(0, at - 1)) }
            else if (e.key === 'Enter') { e.preventDefault(); const hit = hits[at]; if (hit) activate(hit) }
          }} />
      </div>
      <div ref={listRef} id="lfs-jump-list" className={css.jumpList} role="listbox" aria-label={t('jumpTitle')}>
        {hits.length === 0 && q.trim() ? <p className={css.jumpNothing}>{t('jumpNothing', { q: q.trim() })}</p> : null}
        {hits.map((x, i) => (
          <div key={`${x.group}:${x.id}`} role="presentation">
            {i === 0 || hits[i - 1]!.group !== x.group ? <div className={css.jumpGroup} role="presentation">{x.group}</div> : null}
            <div id={optionId(i)} data-index={i} role="option" aria-selected={i === at} className={css.jumpItem} data-active={i === at || undefined}
              onMouseMove={() => { if (i !== at) setAt(i) }} onClick={() => activate(x)}>
              {'icon' in x && x.icon ? <span className={css.jumpItemIcon} aria-hidden="true">{x.icon}</span> : null}
              <span className={css.jumpItemName}>{x.label}</span>
              <span className={css.jumpItemWhere}>{x.where}</span>
            </div>
          </div>
        ))}
      </div>
      <div className={css.jumpFoot} aria-hidden="true">{t('jumpHint')}</div>
    </Modal>
  )
}
