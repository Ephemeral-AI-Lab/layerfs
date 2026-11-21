import type { ReactNode } from 'react'
import { HoverCard } from '@deepseek-ai/dsh-client-ui-primitives'
import css from './Signal.module.css'

export type SignalState = 'ok' | 'warning' | 'error' | 'idle' | 'ongoing'

export function Signal({ state, title, anchorText, detail, action, label, fid }: {
  state: SignalState
  /** The state in a few words (also the dot's accessible name unless `label` names it). */
  title: string
  /** Optional visible text for the anchor button when different from the hover card title. */
  anchorText?: ReactNode
  /** The dot's accessible name when it says more than `title` (the reason read after the word). */
  label?: string | undefined
  /** A focus key the page restores focus by (`data-fid`). */
  fid?: string | undefined
  /** The longer reason, usually LayerFS's own words. */
  detail?: ReactNode
  /** The fix, as a button inside the hover card. */
  action?: ReactNode
  size?: number
}): ReactNode {
  return (
    <span className={css.root} onClick={(e) => e.stopPropagation()}>
      <HoverCard inline fitContent openDelayMs={120}
        anchor={(
          <button type="button" className={css.signal} data-state={state} aria-label={label ?? title} data-fid={fid}>
            {anchorText ?? title}
          </button>
        )}
        content={(
          <span className={css.body}>
            <span className={css.title} data-state={state}>{title}</span>
            {detail ? <span className={css.detail}>{detail}</span> : null}
            {action ? <span className={css.action}>{action}</span> : null}
          </span>
        )} />
    </span>
  )
}
