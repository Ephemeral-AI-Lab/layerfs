/**
 * A Store's Overview tab: four tiles (size or server, projects and Commits,
 * Sandboxes running, last backup or connecting machines), then what fills the
 * database file, how the Store grew, which projects wrote it, its health, and
 * its details with removal folded at the bottom. No activity feed: a job in
 * flight is the page's own panel.
 */
import type { ReactNode } from 'react'
import { Button, IconChevronRightOutlineRegular } from '@deepseek-ai/dsh-client-ui-primitives'
import { enc, type LayerfsApi } from '../api.ts'
import { ago, bytes, when } from '../files/format.ts'
import type { Translate } from '../files/types.ts'
import { useQuery } from '../ui/use-query.ts'
import { Card, Fact, Facts, refusalOf } from '../ui/index.tsx'
import type { StorageT } from './locale.ts'
import { backupStateOf, latestBackup, storeStateWords } from './model.ts'
import { commitTotal, commitTotalWords, compressionOf, fileParts, growthRuns, projectShares, type ObservationJournal, type StoreProject, type StoreStats } from './store-page.ts'
import type { StorageView, StoreRow } from './types.ts'
import css from './StorePage.module.css'
import storageCss from './Storage.module.css'

export interface StoreOverviewProps {
  store: StoreRow
  view: StorageView
  t: StorageT
  api: LayerfsApi
  now: number
  /** `store.stats` (the page reads it, as the Projects tab's shares need it too). */
  stats: StoreStats | undefined
  /** Why the last stats read was refused. */
  statsError: unknown
  projects: readonly StoreProject[] | undefined
  running: ReadonlyMap<string, number>
  machineState: string | null | undefined
  onTab: (tab: 'projects' | 'backups' | 'connections') => void
  onRemove: () => void
}

export function StoreOverview({ store: s, view, t, api, now, stats, statsError, projects, running, machineState, onTab, onRemove }: StoreOverviewProps): ReactNode {
  const pg = s.kind === 'postgresql'
  const tt = t as Translate
  // Each stats read is journaled by the host, so the journal is read again after one.
  const journal = useQuery(() => api.call<ObservationJournal>('GET', `/api/stores/${enc(s.name)}/observations`), [api, s.name], { refresh: [stats], cache: `store-observations:${s.name}` })
  const sandboxes = [...running.values()].reduce((a, n) => a + n, 0)
  const commits = projects ? commitTotal(projects) : null
  const latest = latestBackup(s)

  return (
    <div className={css.stack}>
      <dl className={css.tiles} aria-label={t('storeFacts')}>
        {pg ? (
          <Tile label={t('spServer')} value={s.state === 'ready' ? t('spReachable') : s.state === 'broken' ? t('spUnreachable') : t('unknown')}
            tone={s.state === 'broken' ? 'danger' : undefined} sub={s.state === 'broken' ? storeStateWords(s.state, t, s.reason) : undefined} title={s.fix ?? undefined} />
        ) : (
          <Tile label={t('spDatabaseFile')} value={s.size ? bytes(s.size.bytes) : t('sizeUnavailable')} sub={s.size?.stale && s.size.seen ? t('msSizeSeen', { when: ago(s.size.seen, now, tt) }) : undefined} />
        )}
        <Tile label={t('tabProjects')} value={String(s.projectsCount ?? s.projects.length)} onOpen={() => onTab('projects')}
          sub={commits ? t(commitTotalWords(commits).key, { n: commitTotalWords(commits).n }) : undefined} />
        <Tile label={t('spRunning')} value={t(sandboxes === 1 ? 'spSandboxOne' : 'spSandboxes', { n: String(sandboxes) })} />
        {pg ? (
          <Tile label={t('pgConnectedFromLabel')} value={String((s.connectedFrom ?? []).length)} onOpen={() => onTab('connections')}
            sub={(s.connectedFrom ?? []).map((c) => (c.machine === null ? t('pgThisComputer') : c.machine)).join(', ') || undefined} />
        ) : (
          <Tile label={t('factLastBackup')} value={latest !== null ? ago(latest, now, tt) : t('noBackup')} title={latest !== null ? when(latest) : undefined}
            onOpen={() => onTab('backups')} tone={latest === null ? 'warning' : undefined} sub={s.links[0] ? s.links[0].bucket : undefined} />
        )}
      </dl>

      <FileCard stats={stats} error={stats ? undefined : statsError} pg={pg} t={t} />
      {/* The host journals a read only when store.stats names the Store's identity
          (store_id, store_generation); a build without it records nothing, so no card. */}
      {growthRuns(journal.data).length ? <GrowthCard journal={journal.data} t={t} now={now} /> : null}
      <WhoWroteCard projects={projects} stats={stats} t={t} />
      <HealthCard store={s} view={view} t={t} now={now} stats={stats} machineState={machineState} />

      <details className={storageCss.storeDetails}>
        <summary className={storageCss.storeDetailsSummary}>
          <IconChevronRightOutlineRegular size={12} className={storageCss.storeDetailsChevron} />
          <span>{t('storeFacts')}</span>
        </summary>
        <div className={storageCss.storeDetailsBody}>
          <Card>
            <Facts>
              {s.created != null ? <Fact label={t('factCreated')}>{when(s.created)}</Fact> : null}
              {!pg ? <Fact label={t('factFile')}>{s.size ? bytes(s.size.bytes) : null}</Fact> : null}
              {pg ? <Fact label={t('factServer')} mono>{s.postgresql?.target}</Fact> : null}
            </Facts>
          </Card>
          <div className={storageCss.dangerRow}>
            <div className={storageCss.dangerText}>
              <p className={storageCss.dangerTitle}>{pg ? t('disconnectStoreTitle') : t('removeStoreTitle')}</p>
              <p className={storageCss.sectionNote}>{pg ? t('disconnectStoreBody') : t('removeStoreBody')}</p>
            </div>
            <Button variant="outline" size="sm" className={storageCss.dangerButton} onClick={onRemove}>{pg ? t('disconnect') : t('remove')}</Button>
          </div>
        </div>
      </details>
    </div>
  )
}

function Tile({ label, value, sub, onOpen, tone, title }: { label: string, value: string, sub?: string | undefined, onOpen?: () => void, tone?: 'warning' | 'danger' | undefined, title?: string | undefined }): ReactNode {
  const body = (
    <>
      <dt>{label}</dt>
      <dd className={css.tileValue} data-tone={tone} title={title}>{value}</dd>
      {sub ? <dd className={css.tileSub}>{sub}</dd> : null}
    </>
  )
  return onOpen
    ? <div className={css.tile}><button type="button" className={css.tileButton} onClick={onOpen}><dl className={css.tileInner}>{body}</dl></button></div>
    : <div className={css.tile}><dl className={css.tileInner}>{body}</dl></div>
}

/** The database file's pages by part, and unique content over its packed bytes. */
function FileCard({ stats, error, pg, t }: { stats: StoreStats | undefined, error: unknown, pg: boolean, t: StorageT }): ReactNode {
  const parts = fileParts(stats)
  const ratio = compressionOf(stats)
  const words: Record<string, string> = { stored: t('spPartStored'), metadata: t('spPartMetadata'), free: t('spPartFree') }
  return (
    <Card className={css.panel} data-store-file>
      <header className={css.panelHead}>
        <h2 className={css.panelTitle}>{pg ? t('spFileTitlePg') : t('spFileTitle')}</h2>
        {stats?.file_bytes != null ? <span className={css.panelFigure}>{bytes(stats.file_bytes)}</span> : null}
      </header>
      {parts ? (
        <>
          <div className={css.partsBar} role="img" aria-label={parts.map((p) => `${words[p.key]} ${bytes(p.bytes)}`).join(', ')}>
            {parts.map((p) => <span key={p.key} data-part={p.key} style={{ width: `${(p.share * 100).toFixed(2)}%` }} />)}
          </div>
          <ul className={css.partsLegend}>
            {parts.map((p) => (
              <li key={p.key}>
                <span className={css.legendName}><i data-part={p.key} />{words[p.key]}</span>
                <span className={css.legendValue}>{bytes(p.bytes)} <span className={css.muted}>{Math.round(p.share * 100)}%</span></span>
              </li>
            ))}
          </ul>
          {ratio !== null && stats?.canonical_bytes != null && stats.stored_bytes != null ? (
            <p className={css.ratioLine}>
              <span>{t('spCompression', { canonical: bytes(stats.canonical_bytes), stored: bytes(stats.stored_bytes) })}</span>
              <strong>{t('spRatio', { n: ratio >= 10 ? Math.round(ratio) : ratio.toFixed(1) })}</strong>
            </p>
          ) : null}
        </>
      ) : <p className={css.muted}>{stats ? t('spFileUnknown') : error ? t('spNotRead', { message: refusalOf(error).message }) : t('spReading')}</p>}
    </Card>
  )
}

const W = 600, H = 120

/** File, content and metadata over the host's reads; gaps longer than the journal's are breaks. Drawn once one read is recorded. */
function GrowthCard({ journal, t, now }: { journal: ObservationJournal | undefined, t: StorageT, now: number }): ReactNode {
  const runs = growthRuns(journal)
  const points = runs.flat()
  const from = points[0]?.at ?? now, to = Math.max(points[points.length - 1]?.at ?? now, from + 1)
  const max = Math.max(1, ...points.map((p) => Math.max(p.file ?? 0, (p.stored ?? 0) + (p.metadata ?? 0))))
  const x = (at: number) => ((at - from) / (to - from)) * W
  const y = (v: number) => H - 4 - (v / max) * (H - 10)
  const path = (run: typeof points, value: (p: (typeof points)[number]) => number) => run.map((p, i) => `${i ? 'L' : 'M'}${x(p.at).toFixed(1)} ${y(value(p)).toFixed(1)}`).join(' ')
  return (
    <Card className={css.panel} data-store-growth>
      <header className={css.panelHead}>
        <h2 className={css.panelTitle}>{t('spGrowthTitle')}</h2>
        <span className={css.legendInline}>
          <span><i data-series="file" />{t('spDatabaseFile')}</span>
          <span><i data-part="stored" />{t('spPartStored')}</span>
          <span><i data-part="metadata" />{t('spPartMetadata')}</span>
        </span>
      </header>
      {points.length >= 2 ? (
        <>
          <svg className={css.growth} viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" role="img" aria-label={t('spGrowthLabel', { n: String(points.length) })}>
            {runs.map((run, i) => run.length > 1 ? (
              <g key={i}>
                <path data-part="stored" d={`${path(run, (p) => p.stored ?? 0)} L${x(run[run.length - 1]!.at)} ${H} L${x(run[0]!.at)} ${H} Z`} />
                <path data-part="metadata" d={`${path(run, (p) => (p.stored ?? 0) + (p.metadata ?? 0))} L${[...run].reverse().map((p) => `${x(p.at).toFixed(1)} ${y(p.stored ?? 0).toFixed(1)}`).join(' L')} Z`} />
                <path data-series="file" d={path(run, (p) => p.file ?? 0)} vectorEffect="non-scaling-stroke" />
              </g>
            ) : (
              <circle key={i} data-series="file" cx={x(run[0]!.at)} cy={y(run[0]!.file ?? 0)} r={2} />
            ))}
          </svg>
          <div className={css.axis}><span>{when(from)}</span><span>{when(to)}</span></div>
        </>
      ) : <p className={css.muted}>{t('spGrowthFew')}</p>}
    </Card>
  )
}

/** Include bytes from before commit accounting so the shares add up to the stored content. */
function WhoWroteCard({ projects, stats, t }: { projects: readonly StoreProject[] | undefined, stats: StoreStats | undefined, t: StorageT }): ReactNode {
  const shares = projects ? projectShares(projects) : []
  const counted = shares.reduce((a, p) => a + p.bytes, 0)
  const stored = typeof stats?.stored_bytes === 'number' ? stats.stored_bytes : null
  const rest = stored !== null && stored > counted ? stored - counted : 0
  const whole = counted + rest
  const refused = projects ? commitTotal(projects).refused : null
  return (
    <Card className={css.panel} data-store-shares>
      <header className={css.panelHead}><h2 className={css.panelTitle}>{t('spSharesTitle')}</h2></header>
      {whole > 0 ? (
        <ul className={css.shares}>
          {shares.map((p) => (
            <li key={p.name} className={css.shareBlock} title={`${p.name} ${bytes(p.bytes)}`}>
              <span className={css.shareName}>{p.name}</span>
              <span className={css.shareValue}>{bytes(p.bytes)} · {Math.round(p.bytes / whole * 100)}%</span>
            </li>
          ))}
          {rest > 0 ? (
            <li className={css.shareBlock} title={`${t('spSharesRest')} ${bytes(rest)}`}>
              <span className={css.shareName}>{t('spSharesRest')}</span>
              <span className={css.shareValue}>{bytes(rest)} · {Math.round(rest / whole * 100)}%</span>
            </li>
          ) : null}
        </ul>
      ) : <p className={css.muted}>{!projects ? t('spReading') : refused ? t('spNotRead', { message: refused }) : t('spSharesNone')}</p>}
    </Card>
  )
}

type HealthTone = 'ok' | 'warning' | 'danger' | 'info'

function HealthCard({ store: s, view, t, now, stats, machineState }: { store: StoreRow, view: StorageView, t: StorageT, now: number, stats: StoreStats | undefined, machineState: string | null | undefined }): ReactNode {
  const tt = t as Translate
  const rows: { key: string, tone: HealthTone, label: string, text: string }[] = []
  if (s.kind === 'postgresql') {
    const r = s.state === 'ready' ? true : s.state === 'broken' ? false : null
    rows.push({ key: 'server', tone: r === false ? 'danger' : r ? 'ok' : 'info', label: t('spServer'), text: r === false ? storeStateWords(s.state, t, s.reason) : r ? storeStateWords(s.state, t) : t('unknown') })
    rows.push({ key: 'backups', tone: 'info', label: t('backupsSection'), text: t('spPgBackups') })
  } else {
    const place = s.machine === 'native' ? t('layerfsHome') : (s.machine ?? '?')
    rows.push({ key: 'machine', tone: machineState === 'stopped' ? 'danger' : machineState === 'ready' ? 'ok' : 'info', label: t('spMachine'), text: t(machineState === 'stopped' ? 'spMachineStopped' : 'spMachineRunning', { machine: place }) })
    const latest = latestBackup(s)
    const states = s.links.map((link) => backupStateOf(view.buckets.find((b) => b.name === link.bucket)?.files?.find((f) => f.file === link.file && f.store === s.name))).filter(Boolean)
    const behind = states.some((x) => x === 'store_changed' || x === 'both_changed')
    rows.push({ key: 'backup', tone: latest === null ? 'warning' : behind ? 'warning' : 'ok', label: t('backupsSection'),
      text: latest === null ? t('noBackup') : behind ? t('spBackupBehind', { ago: ago(latest, now, tt) }) : t('spBackupOk', { ago: ago(latest, now, tt) }) })
    if (stats?.reusable_bytes != null && stats.file_bytes) rows.push({ key: 'free', tone: 'ok', label: t('spPartFree'), text: t('spFreeText', { free: bytes(stats.reusable_bytes), n: String(Math.round(stats.reusable_bytes / stats.file_bytes * 100)) }) })
  }
  return (
    <Card className={css.panel} data-store-health>
      <header className={css.panelHead}><h2 className={css.panelTitle}>{t('spHealthTitle')}</h2></header>
      <ul className={css.health}>
        {rows.map((r) => (
          <li key={r.key} data-tone={r.tone}>
            <i aria-hidden="true" />
            <span className={css.healthLabel}>{r.label}</span>
            <span className={css.healthText}>{r.text}</span>
          </li>
        ))}
      </ul>
    </Card>
  )
}
