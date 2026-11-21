/**
 * The top of a Center detail page. The global breadcrumb already names
 * the page, so the bar shows no title: facts and state on the left, the
 * actions found nowhere else on the right. The title stays for assistive
 * technology.
 */
import { createContext, useContext, useEffect, type ReactNode } from 'react'
import css from './Storage.module.css'

/** A mounted detail reports its own reads to the global Refresh button. */
export const PageReading = createContext<((reading: boolean) => void) | null>(null)
export function usePageReading(reading: boolean): void {
  const report = useContext(PageReading)
  useEffect(() => { report?.(reading); return () => report?.(false) }, [report, reading])
}

export function PaneBar({ title, children, actions }: {
  /** The page's name, read by assistive technology only. */
  title: string
  /** Facts and state chips, or the page's own view switch. */
  children?: ReactNode
  actions?: ReactNode
}): ReactNode {
  return (
    <header className={css.paneBar}>
      <h1 className={css.srOnly}>{title}</h1>
      {children ? <div className={css.paneBarMeta}>{children}</div> : null}
      {actions ? <div className={css.paneBarActions}>{actions}</div> : null}
    </header>
  )
}
