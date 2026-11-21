/**
 * The plugin's tab bar (the Overview's tabs and the Doctor page's): each tab's
 * label, its count and a dot when something in it needs you, with the
 * selected tab's actions at the right end. It stays at the top of the scroll;
 * when the tabs do not fit (a phone) the tabs scroll sideways and a fade marks
 * each edge that cuts tabs off. Arrow keys, Home and End move between the tabs,
 * as a tablist does; only the selected tab is in the tab order.
 */
import { useCallback, useEffect, useLayoutEffect, useRef, type KeyboardEvent, type ReactNode } from 'react'
import clsx from 'clsx'
import { StateDot } from '@deepseek-ai/dsh-client-ui-primitives'
import css from './TabBar.module.css'

const TONE = { warning: 'warning', danger: 'error', idle: 'idle', ongoing: 'ongoing' } as const

export interface TabItem {
  key: string
  label: string
  count?: ReactNode
  /** A dot when something in this tab needs you (`idle`: something in it is stopped; `ongoing`: a check runs). */
  tone?: 'warning' | 'danger' | 'idle' | 'ongoing' | undefined
  /** A glyph before the label. */
  icon?: ReactNode
  /** What the tab's dot says, in words: its tip, and read after its name. */
  hint?: string | undefined
  /** The tab's own id (for a panel's `aria-labelledby`). */
  id?: string
  /** The id of the panel it shows (`aria-controls`). */
  controls?: string
  /** A focus key the page restores focus by (`data-fid`). */
  fid?: string
}

/** A tab's accessible name: its label, its count and what its dot says ("Sandboxes, 2"). */
export function tabName(x: Pick<TabItem, 'label' | 'count' | 'hint'>): string {
  const count = typeof x.count === 'number' || typeof x.count === 'string' ? String(x.count) : null
  return [x.label, count, x.hint].filter(Boolean).join(', ')
}

/**
 * Where the bar scrolls so the selected tab is in sight (28 px of room past its
 * edge), or null when it already is: a tab left of the view, or right of it.
 */
export function keptInSight(tab: { left: number, width: number }, bar: { scrollLeft: number, clientWidth: number }): number | null {
  const right = tab.left + tab.width
  if (tab.left < bar.scrollLeft) return Math.max(0, tab.left - 28)
  if (right > bar.scrollLeft + bar.clientWidth) return right - bar.clientWidth + 28
  return null
}

export function TabBar({ tabs, active, onSelect, label, actions, className }: {
  tabs: readonly TabItem[]
  active: string
  onSelect: (key: string) => void
  label: string
  /** The selected tab's actions, at the right end of the bar. */
  actions?: ReactNode
  /** The bar's own spacing on a page (its top inset). */
  className?: string | undefined
}): ReactNode {
  const scroll = useRef<HTMLDivElement | null>(null)
  const fades = useCallback(() => {
    const bar = scroll.current
    if (!bar) return
    const max = bar.scrollWidth - bar.clientWidth
    bar.toggleAttribute('data-fade-l', bar.scrollLeft > 6)
    bar.toggleAttribute('data-fade-r', bar.scrollLeft < max - 6)
  }, [])
  // the selected tab is kept in sight (one restored from before, or picked off-screen), without scrolling the page
  const activeRef = useRef(active)
  activeRef.current = active
  const inSight = useCallback(() => {
    const bar = scroll.current
    const el = bar?.querySelector<HTMLElement>(`[data-key="${CSS.escape(activeRef.current)}"]`)
    if (!bar || !el) return
    const at = el.getBoundingClientRect(), box = bar.getBoundingClientRect()
    const scrolled = keptInSight({ left: at.left - box.left + bar.scrollLeft, width: at.width }, { scrollLeft: bar.scrollLeft, clientWidth: bar.clientWidth })
    if (scrolled !== null) bar.scrollLeft = scrolled
    fades()
  }, [fades])
  useLayoutEffect(() => {
    // the bar or its tabs change width (a count lands, the pane narrows, the window is resized):
    // the selected tab is brought back in sight and the fades follow
    inSight()
    window.addEventListener('resize', inSight)
    const bar = scroll.current
    const watch = typeof ResizeObserver === 'function' && bar ? new ResizeObserver(inSight) : null
    if (watch && bar) { watch.observe(bar); if (bar.firstElementChild) watch.observe(bar.firstElementChild) }
    return () => { window.removeEventListener('resize', inSight); watch?.disconnect() }
  }, [inSight, tabs.length])
  useEffect(() => { inSight() }, [active, inSight])
  const onKey = (e: KeyboardEvent<HTMLDivElement>) => {
    const at = tabs.findIndex((x) => x.key === active)
    const to = e.key === 'ArrowRight' ? at + 1 : e.key === 'ArrowLeft' ? at - 1 : e.key === 'Home' ? 0 : e.key === 'End' ? tabs.length - 1 : null
    if (to === null || !tabs.length) return
    e.preventDefault()
    const next = tabs[(to + tabs.length) % tabs.length]!
    if (next.key !== active) onSelect(next.key)
    const el = scroll.current?.querySelector<HTMLElement>(`[data-key="${CSS.escape(next.key)}"]`)
    el?.focus()
    el?.scrollIntoView({ block: 'nearest', inline: 'nearest' })
    fades()
  }
  return (
    <div className={clsx(css.bar, className)}>
      <div className={css.scroll} ref={scroll} onScroll={fades} data-tip-keep="">
        <div className={css.tabs} role="tablist" aria-label={label} onKeyDown={onKey}>
          {tabs.map((x) => {
            const on = x.key === active
            return (
              <button type="button" key={x.key} id={x.id} data-key={x.key} data-fid={x.fid} className={css.tab} data-active={on || undefined}
                role="tab" aria-selected={on} aria-controls={x.controls} tabIndex={on ? 0 : -1} title={x.hint} aria-label={tabName(x)} onClick={() => onSelect(x.key)}>
                {x.icon ? <span className={css.icon} aria-hidden="true">{x.icon}</span> : null}
                <span className={css.label} data-text={x.label}><span>{x.label}</span></span>
                {x.count != null ? <span className={css.count}>{x.count}</span> : null}
                {x.tone === 'ongoing' ? <span className={css.dot} aria-hidden="true"><StateDot state={TONE[x.tone]} size={9} /></span> : null}
                {x.hint ? <span className={css.sr}>, {x.hint}</span> : null}
              </button>
            )
          })}
        </div>
      </div>
      {actions ? <div className={css.actions}>{actions}</div> : null}
    </div>
  )
}
