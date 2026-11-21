/**
 * The LayerFS trajectory's timeline strip: four lanes (Input, Model, Tools,
 * LayerFS) across the session's time span, drawn like DSH's stock strip
 * (8px spans on a 14px lane pitch). A mark selects its record.
 */
import { memo, useMemo, type ReactNode } from 'react'
import clsx from 'clsx'
import type { TrajectoryTurn } from '../../shared/trajectory.ts'
import type { Translate } from './locale.ts'
import { LANES, laneItems, place, thinLane, type Selection } from './trajectory-model.ts'
import { durationText } from './TrajectoryParts.tsx'
import css from './trajectory.module.css'

export const TrajectoryTimeline = memo(function TrajectoryTimeline({ turns, now, span, selected, onSelect, t, layerfs = true }: {
  turns: readonly TrajectoryTurn[]
  /** Draw the LayerFS lane (a project session); a normal session has three lanes. */
  layerfs?: boolean
  now: number
  /** The strip's time span (`timeSpan` of the same turns), null without timing. */
  span: { start: number, end: number } | null
  selected: Selection | null
  onSelect: (key: Selection) => void
  t: Translate
}): ReactNode {
  const items = useMemo(() => laneItems(turns, now), [turns, now])
  const lanes = layerfs ? LANES : LANES.filter((lane) => lane !== 'layerfs')
  // Each lane at most LANE_BINS marks: nearby items share one (the selected one always shows).
  const marks = useMemo(() => new Map(lanes.map((lane) => [lane, span ? thinLane(items.filter((item) => item.lane === lane), span, selected) : []])),
    [items, span?.start, span?.end, selected, layerfs]) // eslint-disable-line react-hooks/exhaustive-deps
  if (!span) return <div className={css.timeline}><p className={css.timelineEmpty}>{t('trajNoTiming')}</p></div>
  return (
    <div className={css.timeline} role="group" aria-label={t('trajTimeline')}>
      <div className={css.axis} aria-hidden="true">
        {[0, 0.5, 1].map(fraction => <span key={fraction}>{durationText(t, (span.end - span.start) * fraction)}</span>)}
      </div>
      <div className={css.laneLabels} aria-hidden="true">
        {lanes.map((lane) => <span key={lane} className={css.laneLabel}>{t(`trajLane_${lane}`)}</span>)}
      </div>
      <div className={css.lanes}>
        {lanes.map((lane) => (
          <div key={lane} className={css.track}>
            {(marks.get(lane) ?? []).map((item) => {
              const { left, width } = place(item, span)
              const tick = lane === 'layerfs'
              const label = item.more ? t('trajMarkMore', { label: item.label || t(`trajLane_${lane}`), n: item.more }) : item.label
              return (
                <button key={item.key} type="button" tabIndex={-1}
                  className={clsx(tick ? css.tick : css.bar, css[`lane_${lane}`],
                    item.tone && css[`tone_${item.tone}`], item.select && item.select === selected && css.markSelected)}
                  style={tick ? { left: `${left}%` } : { left: `${left}%`, width: `${width}%` }}
                  title={label || undefined} aria-label={label || t(`trajLane_${lane}`)}
                  disabled={!item.select} onClick={() => item.select && onSelect(item.select)} />
              )
            })}
          </div>
        ))}
      </div>
    </div>
  )
})
