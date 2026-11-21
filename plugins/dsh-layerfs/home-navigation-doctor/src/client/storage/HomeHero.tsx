/** Home identity, primary need, disclosure and underline inventory tabs. */
import type { KeyboardEvent, ReactNode } from 'react'
import css from './HomeHero.module.css'

export interface HeroTab {
  key: string
  label: string
  count: ReactNode
  /** Semantic tone when something in this tab needs attention. */
  tone?: 'warning' | 'danger' | undefined
}

/** The id of a tab's panel, for `aria-controls`. */
export const panelId = (key: string) => `ov-panel-${key}`

export function HomeHero({ title, identity, titleAside, titleDetail, tone, signal, headline, actions, tools, toolsLabel, tabs, active, onSelect, tabsLabel, tabActions, notices }: {
  title: string
  /** Optional runtime version and setup disclosure. */
  identity?: ReactNode
  /** Optional inline disclosure beside the title (e.g. Docker status). */
  titleAside?: ReactNode
  titleDetail?: ReactNode
  /** Semantic severity of the active primary need, when present. */
  tone?: 'danger' | 'warning' | 'neutral' | undefined
  /** Interactive problem/warning indicator (omitted when healthy). */
  signal?: ReactNode
  /** What needs you most, or that all is well. */
  headline?: ReactNode
  /** The verb that fixes the headline, and how many more need you. */
  actions?: ReactNode
  /** The home's pages and Refresh, as icon buttons. */
  tools: ReactNode
  toolsLabel: string
  tabs: readonly [HeroTab, ...HeroTab[]]
  active: string
  onSelect: (key: string) => void
  tabsLabel: string
  /** The selected tab's actions, at the right end of the tab bar. */
  tabActions?: ReactNode
  /** Remaining notices, immediately after the disclosure button. */
  notices?: ReactNode
}): ReactNode {
  const onTabKeyDown = (e: KeyboardEvent<HTMLButtonElement>, index: number) => {
    if (e.key !== 'ArrowRight' && e.key !== 'ArrowLeft' && e.key !== 'Home' && e.key !== 'End') return
    e.preventDefault()
    const next = e.key === 'Home' ? 0
      : e.key === 'End' ? tabs.length - 1
        : (index + (e.key === 'ArrowRight' ? 1 : -1) + tabs.length) % tabs.length
    const target = tabs[next]
    if (!target) return
    onSelect(target.key)
    document.getElementById(`ov-tab-${target.key}`)?.focus()
  }

  const showStatus = !!(signal || headline || actions)

  return (
    <>
      <header className={css.hero} data-tone={tone && tone !== 'neutral' ? tone : undefined}>
        <div className={css.heroTop}>
          <div className={css.heading}>
            <h1 className={css.title}>{title}</h1>
            {identity}
            {titleAside}
          </div>
          <div role="group" className={css.tools} aria-label={toolsLabel}>{tools}</div>
        </div>
        {titleDetail}
        {showStatus ? (
          <div className={css.status} data-alert={tone && tone !== 'neutral' ? tone : undefined}>
            {signal ? <span className={css.signalWrap}>{signal}</span> : null}
            {headline ? <span className={css.headline}>{headline}</span> : null}
            {actions}
          </div>
        ) : null}
      </header>
      {notices}
      <div className={css.toolbar}>
        <div className={css.tabList} role="tablist" aria-label={tabsLabel}>
          {tabs.map((tab, i) => {
            const selected = tab.key === active
            return (
              <button
                key={tab.key}
                id={`ov-tab-${tab.key}`}
                type="button"
                role="tab"
                aria-selected={selected}
                aria-controls={panelId(tab.key)}
                tabIndex={selected ? 0 : -1}
                className={css.tab}
                data-active={selected || undefined}
                onClick={() => onSelect(tab.key)}
                onKeyDown={(e) => onTabKeyDown(e, i)}
              >
                <span className={css.tabLabel}>
                  <span>{tab.label}</span>
                  <span className={css.count} data-tone={tab.tone}>{tab.count}</span>
                </span>
              </button>
            )
          })}
        </div>
        {tabActions ? <div className={css.actions}>{tabActions}</div> : null}
      </div>
    </>
  )
}
