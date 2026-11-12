/**
 * The storage flow (LayerFS Connect's `components/storage/StagedStorageFlow`):
 * stages on one byte scale whose pillars, bands and counting values arrive in
 * turn, with a Replay. Motion settles under prefers-reduced-motion.
 */
import { useEffect, useId, useState, type CSSProperties, type ReactNode } from 'react'
import { CountUp, useReducedMotion } from './motion.tsx'
import { exactBytes, type WireInteger } from './exact-count.ts'
import { coordinate, encodingRatio, integer, maximum } from './measures.ts'
import type { StorageT } from './locale.ts'
import css from './StorageVisuals.module.css'

const delay = (ms: number): CSSProperties => ({ '--storage-delay': `${ms}ms` }) as CSSProperties

// ---------- staged flow ----------

export interface StorageStage {
  label: string
  caption: string
  value: WireInteger | null | undefined
  tone: 'referenced' | 'canonical' | 'stored' | 'allocation'
}

const tone = (name: StorageStage['tone']): string => `var(--storage-${name === 'allocation' ? 'overhead' : name})`

function StageBytes({ value, wait }: { value: StorageStage['value'], wait: number }): ReactNode {
  const reduced = useReducedMotion()
  const [arrived, setArrived] = useState(reduced || wait === 0)
  useEffect(() => {
    if (reduced) return
    const timer = setTimeout(() => setArrived(true), wait)
    return () => clearTimeout(timer)
  }, [reduced, wait])
  if (!reduced && !arrived) return <span aria-hidden="true">{exactBytes(value)}</span>
  return typeof value === 'number' && integer(value) !== null
    ? <CountUp value={value} format={(shown) => exactBytes(Math.round(shown))} ms={650} />
    : <span className={integer(value) === null ? css.stageUnknown : undefined}>{exactBytes(value)}</span>
}

/** Stages share one linear byte scale. Missing values never produce a band. */
export function StagedStorageFlow({ stages, steps, label, t }: { stages: StorageStage[], steps: string[], label: string, t: StorageT }): ReactNode {
  const id = useId().replace(/[^\w-]/g, '')
  const [replay, setReplay] = useState(0)
  const max = maximum(stages.map((stage) => stage.value))
  const center = (index: number) => (index + .5) * 900 / stages.length
  const height = (index: number) => (coordinate(stages[index].value, max) ?? 0) * 120
  return (
    <div className={css.stagedFlow}>
      <div key={replay} className={css.flowSequence}>
        <div className={css.flowLabels} style={{ gridTemplateColumns: `repeat(${stages.length}, minmax(0, 1fr))` }}>
          {stages.map((stage, index) => (
            <div className={css.stageLabel} key={stage.label} style={delay(index * 260)}>
              <span><i style={{ background: tone(stage.tone) }} />{stage.label}</span>
              <strong><StageBytes value={stage.value} wait={index * 260} /></strong>
              <small>{stage.caption}</small>
            </div>
          ))}
        </div>
        <svg className={css.flowSvg} viewBox="0 0 900 150" preserveAspectRatio="none" role="img"
          aria-label={`${label}. ${stages.map((stage) => `${stage.label}: ${exactBytes(stage.value)}`).join('; ')}`}>
          <defs>
            {stages.slice(1).map((stage, index) => (
              <linearGradient key={stage.label} id={`${id}-${index}`}>
                <stop stopColor={tone(stages[index].tone)} stopOpacity=".65" />
                <stop offset="1" stopColor={tone(stage.tone)} stopOpacity=".75" />
              </linearGradient>
            ))}
          </defs>
          {stages.slice(1).map((stage, index) => {
            if (integer(stages[index].value) === null || integer(stage.value) === null || max === 0n) return null
            const x0 = center(index) + 3, x1 = center(index + 1) - 3, mid = (x0 + x1) / 2
            const h0 = height(index), h1 = height(index + 1)
            const d = `M${x0},${75 - h0 / 2} C${mid},${75 - h0 / 2} ${mid},${75 - h1 / 2} ${x1},${75 - h1 / 2} L${x1},${75 + h1 / 2} C${mid},${75 + h1 / 2} ${mid},${75 + h0 / 2} ${x0},${75 + h0 / 2} Z`
            return (
              <g className={css.flowBand} key={stage.label} style={delay(index * 260 + 130)}>
                <path d={d} fill={`url(#${id}-${index})`} />
                {[-.24, 0, .24].map((offset, k) => (
                  <path key={offset} className={css.flowStroke}
                    d={`M${x0},${75 + offset * h0} C${mid},${75 + offset * h0} ${mid},${75 + offset * h1} ${x1},${75 + offset * h1}`}
                    fill="none" stroke={tone(stage.tone)} strokeWidth="1.2" pathLength="1" vectorEffect="non-scaling-stroke"
                    style={delay(index * 260 + k * 100 + 300)} />
                ))}
              </g>
            )
          })}
          {stages.map((stage, index) => integer(stage.value) === null ? null : (
            <rect className={css.flowPillar} key={stage.label} style={delay(index * 260)}
              x={center(index) - 3} y={75 - height(index) / 2} width="6" height={height(index)} rx="3" fill={tone(stage.tone)} />
          ))}
        </svg>
        <div className={css.flowSteps} style={{ gridTemplateColumns: `repeat(${steps.length}, minmax(0, 1fr))`, marginInline: `${50 / stages.length}%` }}>
          {steps.map((step, index) => <span className={css.stageLabel} key={index} style={delay(index * 260 + 400)}>{step}</span>)}
        </div>
      </div>
      <div className={css.flowReplay}>
        <button type="button" onClick={() => setReplay((value) => value + 1)} aria-label={t('replayAria', { label: label.toLowerCase() })}>{t('replay')}</button>
      </div>
    </div>
  )
}

/** "3.4× deduplication", or the step marked unavailable. */
export function ratioStep(t: StorageT, left: unknown, right: unknown, label: string): string {
  const ratio = encodingRatio(left, right)
  return ratio ? t('stepRatio', { ratio, label }) : t('stepUnavailable', { label })
}
