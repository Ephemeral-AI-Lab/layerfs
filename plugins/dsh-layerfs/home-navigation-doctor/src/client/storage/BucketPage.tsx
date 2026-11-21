/**
 * One bucket's page: the Store files it holds, the Store linked to each, their
 * state, size and when written, and the move each needs (Back up now, Restore…).
 * Before a bucket is listed its rows come from the Stores' links alone.
 *
 * One backup file's page (`BackupFilePage`): where the object is, the Stores
 * of this home linked to it, and what is inside. A file holds one whole SQLite
 * Store; LayerFS reads its projects only by restoring it, so "inside" is the
 * linked Store's projects when the copy is still that Store's, else a Restore.
 */
import { useEffect, useState, type ReactNode } from 'react'
import { Button, IconRefreshOutlineRegular, MenuItemButton, StateDot } from '@deepseek-ai/dsh-client-ui-primitives'
import type { LayerfsApi } from '../api.ts'
import { DoctorAction } from '../doctor/Doctor.tsx'
import type { DoctorT } from '../doctor/locale.ts'
import { ago, bytes, when } from '../files/format.ts'
import type { Translate } from '../files/types.ts'
import { Card, Empty, LinkButton, MoreMenu, StateChip } from '../ui/index.tsx'
import type { StorageT } from './locale.ts'
import { PaneBar } from './PaneBar.tsx'
import { autoList, backupChip, bucketFiles, listStatusWords, placeWords, reconnectOf, type Reconnect } from './model.ts'
import { BucketProviderGlyph } from '../machines/icons.tsx'
import { BucketGlyph } from './parts.tsx'
import { Signal } from './Signal.tsx'
import type { BucketRow, StorageView } from './types.ts'
import css from './Storage.module.css'

export interface BucketPageProps {
  name: string
  view: StorageView
  t: StorageT
  api: LayerfsApi
  doctorT: DoctorT
  now: number
  listing: boolean
  onList: () => void
  onRestore: (file: string, store: string | null) => void
  onBackup: (store: string, file: string) => void
  onDisconnect: () => void
  /** Connect again… (a refused key, or a record with no key): the Connect dialog with its name and URL. */
  onReconnect: (again: Reconnect) => void
  onOpenStore: (name: string) => void
  onOpenFile: (file: string) => void
}

/**
 * Asks LayerFS to list the bucket once when its page opens, never again on its
 * own: a refused or `not_listed` answer is kept as the result (F2: an effect
 * keyed on "no files" listed it again forever). One ask per open page.
 */
export function useListOnOpen(name: string, b: BucketRow | undefined, listing: boolean, onList: () => void): void {
  const [ask] = useState(autoList)
  useEffect(() => { if (ask(name, b, listing)) onList() })
}

/**
 * A bucket that did not list, or whose record is broken: a dot and its word,
 * LayerFS's sentence as the tip, and one action: Connect again… when the key
 * was refused or the record has none (List again is the bar's own button).
 */
function BucketStatus({ b, t, onReconnect }: { b: BucketRow, t: StorageT, onReconnect: (again: Reconnect) => void }): ReactNode {
  if (!b.error && b.state !== 'broken') return null
  const again = reconnectOf(b)
  const words = b.error ? listStatusWords(b.error.code, t) : t('bucketRecordBroken')
  const tip = b.error?.message ?? b.message ?? b.fix ?? undefined
  return (
    <div className={css.bucketStatus} data-tone="danger" data-bucket-status={b.error?.code ?? b.state ?? ''}>
      <Signal state="error" size={7} title={words} detail={tip} />
      {again ? <Button size="sm" variant="outline" onClick={() => onReconnect(again)}>{t('connectAgain')}</Button> : null}
    </div>
  )
}

/** A provider's words: Cloudflare R2, MinIO, else the record's own name. */
export function providerWords(provider: string | null | undefined, t: StorageT): string {
  return provider === 'r2' ? t('r2') : provider === 'minio' ? t('minio') : provider ?? t('providerOther')
}

/** The bucket's mark: its provider's brand, else the plain bucket. */
export function BucketMark({ provider, size = 14 }: { provider: string | null | undefined, size?: number }): ReactNode {
  return provider === 'r2' || provider === 'minio' ? <BucketProviderGlyph provider={provider} size={size} /> : <BucketGlyph size={size} />
}

export function BucketPage({ name, view, t, api, doctorT, now, listing, onList, onRestore, onBackup, onDisconnect, onReconnect, onOpenStore, onOpenFile }: BucketPageProps): ReactNode {
  const b = view.buckets.find((x) => x.name === name)
  // backup.list asks the bucket: once when the page opens, then on List again
  useListOnOpen(name, b, listing, onList)
  if (!b) return <><PaneBar title={name} /><Empty title={t('bucketMissing', { name })}>{t('bucketMissingBody')}</Empty></>
  const rows = bucketFiles(b, view.stores)
  const listed = b.files !== null
  const total = (b.files ?? []).reduce((n, f) => n + (f.bytes ?? 0), 0)
  const linked = new Set(rows.flatMap((r) => (r.storeName ? [r.storeName] : []))).size
  const tt = t as Translate
  // bound to a Store of this home first, then the files not in this home
  const groups = [
    { key: 'bound', label: t('bucketBoundGroup'), rows: rows.filter((r) => r.state !== 'unlinked') },
    { key: 'other', label: t('bucketOtherGroup'), rows: rows.filter((r) => r.state === 'unlinked') },
  ].filter((g) => g.rows.length > 0)
  return (
    <>
      <PaneBar title={b.name}
        actions={(
          <>
            <DoctorAction api={api} t={doctorT} target="backup.connection" name={b.name} label={doctorT('testConnection')} />
            <Button variant="outline" size="sm" disabled={listing} onClick={onList}
              icon={listing ? <StateDot state="ongoing" size={13} /> : <IconRefreshOutlineRegular size={13} />}>{listed ? t('listAgain') : t('list')}</Button>
            <MoreMenu label={b.name}>{(close) => <MenuItemButton danger onSelect={() => { close(); onDisconnect() }}>{t('disconnectBucket')}</MenuItemButton>}</MoreMenu>
          </>
        )}>
        {[
          providerWords(b.provider, t),
          listed ? (b.files!.length === 1 ? t('bucketFile', { size: bytes(total) }) : t('bucketFiles', { n: b.files!.length, size: bytes(total) })) : t('bucketNotListed'),
          linked === 1 ? t('bucketLinkedOne') : t('bucketLinked', { n: linked }),
          listing ? t('bucketListing') : b.listed ? t('bucketListed', { ago: ago(b.listed, now, tt) }) : null,
        ].filter(Boolean).join(' · ')}
      </PaneBar>
      <BucketStatus b={b} t={t} onReconnect={onReconnect} />
      {rows.length === 0 ? (
        <Empty title={listed ? t('bucketEmptyListedTitle') : t('bucketEmptyUnlistedTitle')} icon={<BucketMark provider={b.provider} size={20} />}>
          {listed ? t('bucketEmptyListed') : listing ? t('bucketListing') : t('bucketEmptyUnlisted')}
        </Empty>
      ) : (
        <Card className={css.tableCard}>
          <table className={css.table}>
            <thead>
              <tr><th>{t('colFile')}</th><th>{t('colLinked')}</th><th>{t('colState')}</th><th className={`${css.num} ${css.wide}`}>{t('colSize')}</th><th className={css.wide}>{t('colWritten')}</th><th><span className={css.srOnly}>{t('storeActions', { name: b.name })}</span></th></tr>
            </thead>
            {groups.map((g) => (
              <tbody key={g.key}>
                <tr className={css.groupRow}><th colSpan={6} scope="colgroup">{g.label}<span className={css.groupCount}>{g.rows.length}</span></th></tr>
                {g.rows.map((r) => {
                  const chip = backupChip(r.state, t)
                  const s = r.store
                  const canBackUp = !!s && s.kind === 'sqlite' && s.state !== 'busy' && s.state !== 'down'
                  const backUp = (r.state === 'store_changed' || r.state === 'missing' || (r.state === null && !!r.link)) && canBackUp
                  const restore = r.state !== null && r.state !== 'missing'
                  return (
                    <tr key={`${r.file}:${r.storeName ?? ''}`}>
                      <td className={css.mono}><LinkButton onClick={() => onOpenFile(r.file)}>{r.file}</LinkButton><span className={css.muted}>/store.sqlite</span></td>
                      <td>{g.key === 'other' ? <span className={css.muted}>—</span> : r.storeName ? <span className={css.linkList}>{s ? <LinkButton onClick={() => onOpenStore(r.storeName!)}>{r.storeName}</LinkButton> : <span>{r.storeName}</span>}{s ? <span className={css.muted}>{s.kind === 'postgresql' ? t('postgresql') : placeWords(s.machine, t)}</span> : null}</span> : <span className={css.muted}>{t('notLinked')}</span>}</td>
                      <td title={r.reason ?? undefined}>{g.key === 'other' ? null : <StateChip tone={chip.tone} label={chip.label} dashed={chip.dashed} />}</td>
                      <td className={`${css.num} ${css.wide} ${css.muted}`}>{bytes(r.bytes)}</td>
                      <td className={`${css.wide} ${css.muted}`} title={when(r.modified)}>{r.modified ? ago(r.modified, now, tt) : '—'}</td>
                      <td className={css.actionsCell}>
                        {backUp ? <Button variant="outline" size="sm" onClick={() => onBackup(r.storeName!, r.file)}>{t('backupNow')}</Button> : null}
                        {restore ? <Button variant="outline" size="sm" onClick={() => onRestore(r.file, r.storeName)}>{t('restore')}</Button> : null}
                      </td>
                    </tr>
                  )
                })}
              </tbody>
            ))}
          </table>
        </Card>
      )}
    </>
  )
}

/** The object a file names under the bucket's prefix: `<file>/store.sqlite` (2.0's home tree does not give the bucket's address or prefix). */
export function objectOf(_b: BucketRow, file: string): string {
  return `${file}/store.sqlite`
}

export interface BackupFilePageProps {
  bucket: string
  file: string
  view: StorageView
  t: StorageT
  now: number
  listing: boolean
  onList: () => void
  onRestore: (file: string, store: string | null) => void
  onBackup: (store: string, file: string) => void
  onOpenStore: (name: string) => void
  onOpenProject: (key: string) => void
  onOpenBucket: () => void
  onReconnect: (again: Reconnect) => void
}

export function BackupFilePage({ bucket, file, view, t, now, listing, onList, onRestore, onBackup, onOpenStore, onOpenProject, onOpenBucket, onReconnect }: BackupFilePageProps): ReactNode {
  const b = view.buckets.find((x) => x.name === bucket)
  useListOnOpen(bucket, b, listing, onList)
  if (!b) return <><PaneBar title={file} /><Empty title={t('bucketMissing', { name: bucket })}>{t('bucketMissingBody')}</Empty></>
  const tt = t as Translate
  // one row per Store of this home linked to the file (LayerFS lists a file once per linked Store)
  const rows = bucketFiles(b, view.stores).filter((r) => r.file === file)
  if (rows.length === 0) {
    return (
      <>
        <PaneBar title={file} actions={<Button variant="outline" size="sm" onClick={onOpenBucket}>{t('openBucket')}</Button>} />
        <Empty title={t('backupFileMissing', { file, bucket })} icon={<BucketMark provider={b.provider} size={20} />}>{listing ? t('bucketListing') : t('backupFileMissingBody')}</Empty>
      </>
    )
  }
  const linked = rows.filter((r) => r.store)
  const size = rows.find((r) => r.bytes !== null)?.bytes ?? null
  const modified = rows.find((r) => r.modified !== null)?.modified ?? null
  // what is inside: the linked Store's projects while the copy is still that Store's
  const own = linked.find((r) => r.state === 'current') ?? linked.find((r) => r.state === 'store_changed') ?? null
  const rewritten = linked.find((r) => r.state === 'bucket_changed' || r.state === 'both_changed') ?? null
  const projects = own?.store?.projects ?? []
  return (
    <>
      <PaneBar title={file}
        actions={(
          <>
            <Button variant="outline" size="sm" disabled={listing} onClick={onList}
              icon={listing ? <StateDot state="ongoing" size={13} /> : <IconRefreshOutlineRegular size={13} />}>{b.files !== null ? t('listAgain') : t('list')}</Button>
            <Button variant="outline" size="sm" onClick={() => onRestore(file, null)}>{t('restoreAsNewStore')}</Button>
          </>
        )}>
        {[
          providerWords(b.provider, t),
          b.name,
          size !== null ? bytes(size) : null,
          modified ? ago(modified, now, tt) : null,
          listing ? t('bucketListing') : b.listed ? t('bucketListed', { ago: ago(b.listed, now, tt) }) : t('bucketNotListed'),
        ].filter(Boolean).join(' · ')}
      </PaneBar>
      <BucketStatus b={b} t={t} onReconnect={onReconnect} />
      <Card className={css.backupFacts}>
        <dl className={css.backupFactList}>
          <dt>{t('backupObject')}</dt>
          <dd className={css.mono}>{objectOf(b, file)}</dd>
          <dt>{t('backupInside')}</dt>
          <dd>
            {own ? (
              <>
                <p>{own.state === 'current'
                  ? (projects.length === 1 ? t('backupInsideCurrentOne', { store: own.storeName! }) : t('backupInsideCurrent', { store: own.storeName!, n: projects.length }))
                  : t('backupInsideChanged', { store: own.storeName!, n: projects.length })}</p>
                {projects.length === 0 ? <p className={css.muted}>{t('backupNoProjects')}</p> : (
                  <ul className={css.backupProjects}>
                    {projects.map((p) => (
                      <li key={p}><LinkButton onClick={() => onOpenProject(`${own.storeName}/${p}`)}>{p}</LinkButton></li>
                    ))}
                  </ul>
                )}
              </>
            ) : rewritten ? <p>{t('backupInsideBucketChanged', { store: rewritten.storeName! })}</p> : <p>{t('backupInsideUnread')}</p>}
          </dd>
        </dl>
      </Card>
      <h3 className={css.backupHeading}>{t('backupLinkedStores')}</h3>
      {linked.length === 0 ? <p className={`${css.muted} ${css.backupNote}`}>{t('backupNoLinked')}</p> : (
        <Card className={css.tableCard}>
          <table className={css.table}>
            <thead>
              <tr><th>{t('colLinked')}</th><th>{t('colState')}</th><th className={css.wide}>{t('colWritten')}</th><th><span className={css.srOnly}>{t('storeActions', { name: file })}</span></th></tr>
            </thead>
            <tbody>
              {linked.map((r) => {
                const chip = backupChip(r.state, t)
                const s = r.store!
                const last = r.link?.last
                const canBackUp = s.kind === 'sqlite' && s.state !== 'busy' && s.state !== 'down'
                const backUp = (r.state === 'store_changed' || r.state === 'missing' || (r.state === null && !!r.link)) && canBackUp
                return (
                  <tr key={s.name}>
                    <td><span className={css.linkList}><LinkButton onClick={() => onOpenStore(s.name)}>{s.name}</LinkButton><span className={css.muted}>{placeWords(s.machine, t)}</span></span></td>
                    <td title={r.reason ?? undefined}><StateChip tone={chip.tone} label={chip.label} dashed={chip.dashed} /></td>
                    <td className={`${css.wide} ${css.muted}`} title={when(last?.at ?? null)}>
                      {last?.at ? (last.op === 'restore' ? t('backupLastRestore', { ago: ago(last.at, now, tt) }) : t('backupLastCreate', { ago: ago(last.at, now, tt) })) : t('backupLastNone')}
                    </td>
                    <td className={css.actionsCell}>
                      {backUp ? <Button variant="outline" size="sm" onClick={() => onBackup(s.name, file)}>{t('backupNow')}</Button> : null}
                      {r.state !== null && r.state !== 'missing' ? <Button variant="outline" size="sm" onClick={() => onRestore(file, s.name)}>{t('restore')}</Button> : null}
                    </td>
                  </tr>
                )
              })}
            </tbody>
          </table>
        </Card>
      )}
    </>
  )
}
