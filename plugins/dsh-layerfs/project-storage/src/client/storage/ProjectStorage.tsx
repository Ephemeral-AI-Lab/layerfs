/**
 * A project's own storage (GET /api/projects/<key>/storage): its saves summed
 * from their Commit counters, drawn as the staged flow from saved snapshots
 * to packed bytes, its share of the Store, main's latest tree, the growth of
 * packed bytes and what each recent save added. The first read of a long
 * history runs on the host; the page shows its progress and polls.
 */
import { useEffect, useRef, useState, type CSSProperties, type ReactNode } from 'react'
import { Button, StateDot } from '@deepseek-ai/dsh-client-ui-primitives'
import { enc, type LayerfsApi } from '../api.ts'
import { ago, when } from '../files/format.ts'
import type { Translate } from '../files/types.ts'
import { useQuery } from '../files/use-query.ts'
import { LinkButton, LoadError, Loading, Refusal } from '../ui/index.tsx'
import { exactBytes, exactCount } from './exact-count.ts'
import type { StorageT } from './locale.ts'
import { refusalLabels } from './parts.tsx'
import { StagedStorageFlow, ratioStep, type StorageStage } from './StorageVisuals.tsx'
import type { ProjectStorageView } from './types.ts'
import css from './StorageVisuals.module.css'

/** How often the page asks again while the host reads the history. */
const POLL_MS = 1500

export interface ProjectStorageProps {
  api: LayerfsApi
  t: StorageT
  project: string
  /** Bumped by the page's Refresh. */
  tick: number
  /** Opens the owning Store's page. */
  onOpenStore: (store: string) => void
}

const ratio = (a: number, b: number): string | null => a > 0 && b > 0 ? (a / b).toFixed(a / b >= 10 ? 0 : 1) : null

export function ProjectStorage({ api, t, project, tick, onOpenStore }: ProjectStorageProps): ReactNode {
  const force = useRef(false)
  const q = useQuery(() => {
    const refresh = force.current
    force.current = false
    return api.call<ProjectStorageView>('GET', `/api/projects/${enc(project)}/storage${refresh ? '?refresh=1' : ''}`)
  }, [api, project, tick])
  const view = q.data?.project === project ? q.data : undefined
  const building = view?.building ?? null
  useEffect(() => {
    if (!building) return
    const timer = setTimeout(q.reload, POLL_MS)
    return () => clearTimeout(timer)
  }, [view, building, q.reload])
  const recount = () => { force.current = true; q.reload() }

  if (q.error && !view) return <LoadError title={t('projectStorageFailed')} error={q.error} labels={refusalLabels(t)} retryLabel={t('retry')} onRetry={q.reload} />
  if (!view) return <Loading label={t('loading')} />
  const s = view.totals
  return (
    <div className={css.dashboard}>
      {building ? <Progress t={t} building={building} first={!s} /> : null}
      {view.error ? <Refusal error={view.error} labels={refusalLabels(t)} /> : null}
      {s && s.saves === 0 ? (
        <>
          <section className={css.panel}>
            <div className={css.panelHead}><h3>{t('psEmptyTitle')}</h3></div>
            <p className={css.note}>{t('psEmptyBody')}</p>
          </section>
          <div className={css.facts}>
            <Fact label={t('psMainLatest')} value={view.main ? exactBytes(view.main.logical) : '—'}
              note={view.main ? t('psMainCounts', { files: exactCount(view.main.files), dirs: exactCount(view.main.directories) }) : t('psMainUnavailable')} />
            <Fact label={t('psPublications')} value={exactCount(s.publications)} note={t('psPublicationsNote', { bytes: exactBytes(s.publicationLogical) })} />
            <Fact label={t('psStore')} value={typeof view.store.file === 'number' ? exactBytes(view.store.file) : '—'} note={t('psStoreNote', { store: view.store.name })} />
          </div>
        </>
      ) : s ? (
        <>
          <div className={css.hero}>
            <strong>{exactBytes(s.stored)}</strong>
            <span>{t('psHero', { saves: exactCount(s.saves), ratio: ratio(s.commitLogical, s.stored) ?? '—' })}</span>
          </div>
          <Flow t={t} totals={s} />
          <Share t={t} view={view} stored={s.stored} onOpenStore={onOpenStore} />
          <div className={css.facts}>
            <Fact label={t('psMainLatest')} value={view.main ? exactBytes(view.main.logical) : '—'}
              note={view.main ? t('psMainCounts', { files: exactCount(view.main.files), dirs: exactCount(view.main.directories) }) : t('psMainUnavailable')} />
            <Fact label={t('psSaves')} value={exactCount(s.saves)} note={t('psSavesNote', { merges: exactCount(s.merges), branches: exactCount(s.branches) })} />
            <Fact label={t('psPublications')} value={exactCount(s.publications)} note={t('psPublicationsNote', { bytes: exactBytes(s.publicationLogical) })} />
          </div>
          <div className={css.pair}>
            <Growth t={t} growth={view.growth} saves={s.saves} />
            <Recent t={t} recent={view.recent} />
          </div>
        </>
      ) : null}
      <footer className={css.footer}>
        <span>{view.updatedAt ? t('psCounted', { ago: ago(view.updatedAt, Date.now(), t as Translate) }) : t('psCountedNever')}</span>
        <Button variant="outline" size="sm" disabled={!!building} icon={building ? <StateDot state="ongoing" size={13} /> : undefined} onClick={recount}>
          {building ? t('psCounting') : t('psRecount')}
        </Button>
      </footer>
    </div>
  )
}

function Progress({ t, building, first }: { t: StorageT, building: NonNullable<ProjectStorageView['building']>, first: boolean }): ReactNode {
  const pct = building.branchesTotal > 0 ? Math.round(building.branches / building.branchesTotal * 100) : 0
  return (
    <div className={css.progress} role="status">
      <div><StateDot state="ongoing" size={13} /><span>{first ? t('psReadingFirst') : t('psReadingUpdate')}</span></div>
      <div className={css.track}><span style={{ width: `${pct}%` }} /></div>
      <span>{t('psReadingCounts', { branches: exactCount(building.branches), total: exactCount(building.branchesTotal), saves: exactCount(building.saves) })}</span>
    </div>
  )
}

function Flow({ t, totals }: { t: StorageT, totals: NonNullable<ProjectStorageView['totals']> }): ReactNode {
  const stages: StorageStage[] = [
    { label: t('psStageSnapshots'), caption: t('psStageSnapshotsCaption'), value: totals.commitLogical, tone: 'referenced' },
    { label: t('psStageUnique'), caption: t('psStageUniqueCaption'), value: totals.canonical, tone: 'canonical' },
    { label: t('psStagePacked'), caption: t('psStagePackedCaption', { bytes: exactBytes(totals.metadata) }), value: totals.stored, tone: 'stored' },
  ]
  return (
    <section className={css.panel} aria-label={t('psFlowTitle')}>
      <div className={css.panelHead}><h3>{t('psFlowTitle')}</h3></div>
      <StagedStorageFlow t={t} stages={stages} label={t('psFlowTitle')}
        steps={[ratioStep(t, totals.commitLogical, totals.canonical, t('dedupeLabel')), ratioStep(t, totals.canonical, totals.stored, t('compressionLabel'))]} />
    </section>
  )
}

function Share({ t, view, stored, onOpenStore }: { t: StorageT, view: ProjectStorageView, stored: number, onOpenStore: (store: string) => void }): ReactNode {
  const store = view.store
  const total = typeof store.stored === 'number' && store.stored > 0 ? Math.max(store.stored, stored) : null
  const pct = total ? Math.round(stored / total * 1000) / 10 : null
  const name = view.project.slice(view.project.indexOf('/') + 1)
  return (
    <section className={css.panel} aria-label={t('psShareTitle', { store: store.name })}>
      <div className={css.panelHead}>
        <h3>{t('psShareTitle', { store: store.name })}</h3>
        <span>{typeof store.file === 'number' ? t('psStoreFile', { bytes: exactBytes(store.file) }) : null}</span>
      </div>
      {store.refusal ? <Refusal error={store.refusal} labels={refusalLabels(t)} compact /> : total && pct !== null ? (
        <>
          <div className={css.share} role="img" aria-label={t('psShareAria', { project: name, pct })}>
            <span style={{ width: `${pct}%`, background: 'var(--storage-stored)' }} />
          </div>
          <div className={css.legend}>
            <span><i style={{ background: 'var(--storage-stored)' }} /><b>{name}</b> {t('psShareThis', { bytes: exactBytes(stored), pct })}</span>
            <span><i style={{ background: 'var(--dsw-alias-border-l2)' }} />{store.projects && store.projects > 1 ? t(store.projects === 2 ? 'psShareRestProject' : 'psShareRestProjects', { bytes: exactBytes(total - stored), n: store.projects - 1 }) : t('psShareRest', { bytes: exactBytes(total - stored) })}</span>
            <LinkButton onClick={() => onOpenStore(store.name)}>{t('viewStoreUsage')}</LinkButton>
          </div>
        </>
      ) : <p className={css.note}>{t('psShareUnavailable')}</p>}
    </section>
  )
}

function Fact({ label, value, note }: { label: string, value: string, note: string }): ReactNode {
  return <div className={css.fact}><span>{label}</span><strong>{value}</strong><span>{note}</span></div>
}

function Growth({ t, growth, saves }: { t: StorageT, growth: number[], saves: number }): ReactNode {
  const top = growth.at(-1) ?? 0
  const points = growth.map((v, i) => `${growth.length > 1 ? (i / (growth.length - 1) * 300).toFixed(1) : 150},${(96 - (top > 0 ? v / top : 0) * 88).toFixed(1)}`)
  return (
    <section className={css.panel} aria-label={t('psGrowthTitle')}>
      <div className={css.panelHead}><h3>{t('psGrowthTitle')}</h3></div>
      {growth.length > 1 ? (
        <>
          <svg className={css.growth} viewBox="0 0 300 100" preserveAspectRatio="none" role="img" aria-label={t('psGrowthAria', { bytes: exactBytes(top), saves: exactCount(saves) })}>
            <polygon className={css.growthArea} points={`0,96 ${points.join(' ')} 300,96`} fill="var(--storage-canonical)" fillOpacity=".08" />
            <polyline className={css.growthLine} points={points.join(' ')} fill="none" stroke="var(--storage-canonical)" strokeWidth="2" vectorEffect="non-scaling-stroke" />
          </svg>
          <div className={css.ends}><span>{t('psFirstSave')}</span><span><b>{exactBytes(top)}</b> {t('psAfterSaves', { saves: exactCount(saves) })}</span></div>
        </>
      ) : <p className={css.note}>{t('psNoSaves')}</p>}
    </section>
  )
}

function Recent({ t, recent }: { t: StorageT, recent: ProjectStorageView['recent'] }): ReactNode {
  const [picked, setPicked] = useState<string | null>(null)
  const max = Math.max(1, ...recent.map((r) => r.stored))
  const sum = recent.reduce((total, r) => total + r.stored, 0)
  const shown = recent.find((r) => r.id === picked)
  const describe = (r: ProjectStorageView['recent'][number]) => [r.branch, r.message || null, r.time ? when(r.time) : null, t('psAdded', { bytes: exactBytes(r.stored) })].filter(Boolean).join(' · ')
  return (
    <section className={css.panel} aria-label={t('psRecentTitle')}>
      <div className={css.panelHead}><h3>{t('psRecentTitle')}</h3><span>{t('psRecentRange', { n: recent.length })}</span></div>
      {recent.length ? (
        <>
          <div className={css.summary}>
            <span><b>{exactBytes(Math.round(sum / recent.length))}</b> {t('psAverage')}</span>
            <span><b>{exactBytes(max)}</b> {t('psLargest')}</span>
            {recent.some((r) => r.via === 'merge') ? (
              <span className={css.legend}>
                <span><i style={{ background: 'var(--storage-stored)' }} />{t('psSave')}</span>
                <span><i style={{ background: 'var(--storage-canonical)' }} />{t('psMerge')}</span>
              </span>
            ) : null}
          </div>
          <div className={css.bars} onMouseLeave={() => setPicked(null)}>
            {recent.map((r, i) => (
              <button key={r.id} type="button" className={css.barSlot} aria-label={describe(r)} aria-pressed={picked === r.id}
                onMouseEnter={() => setPicked(r.id)} onFocus={() => setPicked(r.id)} onBlur={() => setPicked(null)}>
                <span className={css.bar} data-merge={r.via === 'merge'}
                  style={{ height: `${Math.max(3, r.stored / max * 100)}%`, '--storage-delay': `${300 + i * 20}ms` } as CSSProperties} />
              </button>
            ))}
          </div>
          <div className={css.tip} aria-live="polite">{shown ? describe(shown) : t('psRecentHint')}</div>
        </>
      ) : <p className={css.note}>{t('psNoSaves')}</p>}
    </section>
  )
}
