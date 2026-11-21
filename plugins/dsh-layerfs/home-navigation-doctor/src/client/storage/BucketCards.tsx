/**
 * The Backups tab: one card per bucket, its Store files listed inside, in a
 * wrapping grid (many providers and buckets fill the row; a provider switch
 * narrows it when there are several).
 *
 *   head     the provider's mark, the bucket's name (opens it), its dot, ⋯
 *            (List files, Open bucket)
 *   line     where it is · how many Store files and their size · when listed
 *            (or "could not be read", "not listed yet")
 *   files    one list, the same row for every file: name, size, then either
 *            the Store of this home it backs up (its dot and date) or a
 *            quiet Restore…; this home's first; room for five rows on every
 *            card, a running backup or restore on top
 *   foot     how many are from this home · "+N files · Open bucket"
 *
 * A bucket lists only LayerFS Store files (`<prefix>/<file>/store.sqlite`):
 * LayerFS's backup.list skips every other object, so nothing is filtered here.
 */
import { useState, type ReactNode } from 'react'
import { MenuItemButton } from '@deepseek-ai/dsh-client-ui-primitives'
import { ago, bytes, when } from '../files/format.ts'
import type { Translate } from '../files/types.ts'
import { MoreMenu } from '../ui/index.tsx'
import { BucketMark, providerWords } from './BucketPage.tsx'
import { JobLine } from './HomeCards.tsx'
import type { StorageT } from './locale.ts'
import { backupChip, bucketFiles, doingWords, listStatusWords, reconnectOf, type Reconnect } from './model.ts'
import { BucketGlyph } from './parts.tsx'
import { Signal } from './Signal.tsx'
import { healthOf } from './store-backups.ts'
import type { TabId } from './tabs.ts'
import type { BucketRow, StorageJob, StorageView } from './types.ts'
import css from './BucketCards.module.css'
import { dateOnly } from '../ui/locale-sync.ts'

/** File rows a card keeps room for; more show as "+N files" in the foot. */
const ROWS = 5

/** Providers in the order the switch shows them; any other provider follows. */
const PROVIDER_ORDER = ['minio', 'r2']
const providerRank = (p: string | null) => { const i = PROVIDER_ORDER.indexOf(p ?? ''); return i < 0 ? PROVIDER_ORDER.length : i }

const shortDate = (at: number) => dateOnly(at, { month: 'short', day: 'numeric' })

/** Where a bucket is: its provider (2.0's home tree names a bucket and its provider, not its address). */
function whereOf(b: BucketRow, t: StorageT): string {
  return providerWords(b.provider, t)
}

export function BucketCards({ view, t, now, jobs, onOpen, onList, onReconnect, onRestore }: {
  view: StorageView, t: StorageT, now: number
  /** Running jobs. */
  jobs: readonly StorageJob[]
  onOpen: (id: TabId) => void
  /** Lists a bucket's files again. */
  onList: (bucket: string) => void
  /** Connect again… (a refused key, or a record with no key): the Connect dialog with its name and URL. */
  onReconnect: (again: Reconnect) => void
  /** Restores a bucket file as a new Store. */
  onRestore: (bucket: string, file: string) => void
}): ReactNode {
  const [provider, setProvider] = useState<string | null | 'all'>('all')
  if (view.buckets.length === 0) {
    return (
      <div className={css.emptyCard}>
        <span className={css.emptyIcon}><BucketGlyph size={16} /></span>
        <span className={css.emptyText}>
          <span className={css.emptyTitle}>{t('ovNoBuckets')}</span>
        </span>
      </div>
    )
  }
  // A bucket that could not be read first, then by provider and name.
  const all = [...view.buckets].sort((a, b) => Number(!!b.error || b.state === 'broken') - Number(!!a.error || a.state === 'broken') || providerRank(a.provider) - providerRank(b.provider)
    || (a.provider ?? '').localeCompare(b.provider ?? '') || a.name.localeCompare(b.name))
  const providers = [...new Set(all.map((b) => b.provider))].sort((a, b) => providerRank(a) - providerRank(b) || (a ?? '').localeCompare(b ?? ''))
  const picked = provider !== 'all' && providers.includes(provider) ? provider : 'all'
  const shown = picked === 'all' ? all : all.filter((b) => b.provider === picked)
  return (
    <div className={css.stack}>
      {providers.length > 1 ? (
        <div className={css.seg} role="tablist" aria-label={t('bcProviders')}>
          <button type="button" role="tab" aria-selected={picked === 'all'} onClick={() => setProvider('all')}>
            {t('bcAll')}<span className={css.count}>{all.length}</span>
          </button>
          {providers.map((p) => (
            <button key={p ?? ''} type="button" role="tab" aria-selected={picked === p} onClick={() => setProvider(p)}>
              <BucketMark provider={p} size={13} />{providerWords(p, t)}<span className={css.count}>{all.filter((b) => b.provider === p).length}</span>
            </button>
          ))}
        </div>
      ) : null}
      <div className={css.grid}>
        {shown.map((b) => <BucketCard key={b.name} b={b} view={view} t={t} now={now} jobs={jobs} onOpen={onOpen} onList={onList} onReconnect={onReconnect} onRestore={onRestore} />)}
      </div>
    </div>
  )
}

function BucketCard({ b, view, t, now, jobs, onOpen, onList, onReconnect, onRestore }: {
  b: BucketRow, view: StorageView, t: StorageT, now: number, jobs: readonly StorageJob[]
  onOpen: (id: TabId) => void, onList: (bucket: string) => void, onReconnect: (again: Reconnect) => void, onRestore: (bucket: string, file: string) => void
}): ReactNode {
  const tt = t as Translate
  // a bucket that did not list, or whose record is broken: its word, LayerFS's sentence as the tip, one action
  const failed = b.error ? listStatusWords(b.error.code, t) : b.state === 'broken' ? t('bucketRecordBroken') : null
  const tip = b.error?.message ?? b.message ?? b.fix ?? undefined
  const again = failed ? reconnectOf(b) : null
  const unlisted = b.files === null && !failed
  // this home's copies first, then the rest, each by name
  const files = failed ? [] : [...bucketFiles(b, view.stores)].sort((x, y) => Number(x.state === 'unlinked') - Number(y.state === 'unlinked') || x.file.localeCompare(y.file))
  const fromHome = files.filter((f) => f.state !== 'unlinked').length
  const size = files.reduce((n, f) => n + (f.bytes ?? 0), 0)
  const running = jobs.filter((j) => !j.ended && (j.kind === 'backup' || j.kind === 'restore') && j.target === b.name)
  const room = Math.max(1, ROWS - running.length)
  const rows = files.slice(0, room)
  const open = () => onOpen(`bucket:${b.name}`)

  const state = failed ? 'error' : unlisted ? 'idle' : 'ok'
  const facts = failed ? failed
    : unlisted ? t('ovBucketUnlisted')
      : [files.length === 1 ? t('bcStoreFileOne') : t('bcStoreFiles', { n: files.length }), size ? bytes(size) : null, b.listed ? t('ovListedAgo', { ago: ago(b.listed, now, tt) }) : null].filter(Boolean).join(' · ')

  return (
    <article className={css.card} data-bucket={b.name} data-state={failed ? 'error' : undefined}>
      <header className={css.head}>
        <span className={css.mark}><BucketMark provider={b.provider} size={15} /></span>
        <button type="button" className={css.name} title={b.name} onClick={open}>{b.name}</button>
        {failed ? <Signal state={state} size={6} title={failed} detail={tip} /> : null}
        <span className={css.menu}>
          <MoreMenu label={t('bcMore', { bucket: b.name })}>
            {(close) => (
              <>
                <MenuItemButton onSelect={() => { close(); onList(b.name) }}>{t('ovListFiles')}</MenuItemButton>
                <MenuItemButton onSelect={() => { close(); open() }}>{t('bcOpenBucket')}</MenuItemButton>
              </>
            )}
          </MoreMenu>
        </span>
      </header>
      <p className={css.line} data-tone={failed ? 'danger' : undefined} title={failed ? tip : undefined}>{whereOf(b, t)} · {facts}</p>

      <div className={css.files}>
        {running.map((j) => <div key={j.id} className={css.job}><JobLine job={j} t={t} label={`${j.store} · ${doingWords(j, t)}`} /></div>)}
        {failed ? (
          <p className={css.none}>{t('bcFilesUnknown')} · {again
            ? <button type="button" className={css.inline} data-reconnect={b.name} onClick={() => onReconnect(again)}>{t('connectAgain')}</button>
            : <button type="button" className={css.inline} onClick={() => onList(b.name)}>{t('listAgain')}</button>}</p>
        ) : unlisted ? (
          <p className={css.none}>{t('ovBucketUnlisted')} · <button type="button" className={css.inline} onClick={() => onList(b.name)}>{t('ovListFiles')}</button></p>
        ) : files.length === 0 && running.length === 0 ? (
          <p className={css.none}>{t('bcNoFiles')}</p>
        ) : (
          <ul className={css.list}>
            {rows.map((f) => {
              const chip = backupChip(f.state, t)
              const health = healthOf(f.state)
              return (
                <li key={`${f.file}:${f.storeName ?? ''}`} className={css.row}>
                  <span className={css.file} title={f.reason ?? f.file}>{f.file}</span>
                  <span className={css.size}>{f.bytes != null ? bytes(f.bytes) : ''}</span>
                  {f.state === 'unlinked' ? (
                    <button type="button" className={css.restore} onClick={() => onRestore(b.name, f.file)}>{t('ovRestore')}</button>
                  ) : (
                    <span className={css.store} title={[chip.label, f.modified ? when(f.modified) : null, f.reason].filter(Boolean).join(' · ')}>
                      {f.storeName && f.store
                        ? <button type="button" className={css.storeName} onClick={() => onOpen(`store:${f.storeName}`)}>{f.storeName}</button>
                        : <span className={css.storeName}>{f.storeName ?? '—'}</span>}
                      <span className={css.when} data-tone={health === 'problem' ? 'danger' : undefined}>· {health === 'problem' || !f.modified ? chip.label : shortDate(f.modified)}</span>
                    </span>
                  )}
                </li>
              )
            })}
          </ul>
        )}
      </div>

      <footer className={css.foot}>
        <span>{!failed && !unlisted && files.length ? t('bcFromHome', { n: fromHome }) : ''}</span>
        <button type="button" className={css.inline} onClick={open}>
          {files.length > rows.length ? `${t(files.length - rows.length === 1 ? 'bcMoreFileOne' : 'bcMoreFiles', { n: files.length - rows.length })} · ${t('bcOpenBucket')}` : t('bcOpenBucket')}
        </button>
      </footer>
    </article>
  )
}
