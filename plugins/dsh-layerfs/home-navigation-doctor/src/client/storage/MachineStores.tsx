/**
 * The Stores boards: a machine page's Stores tab (`MachineStores`) and Home ›
 * Stores (`HomeStores`) draw the same tiles and cards (machine-stores.ts
 * holds their data).
 *
 * Two tiles at the top switch between SQLite Stores (the file lives on a
 * machine's disk: push and pull move it) and PostgreSQL Stores (the data
 * stays on its server; a Store is on no machine, and each session picks its
 * own). The choice is remembered per viewer and machine. A machine page
 * lists the PostgreSQL Stores connected from it.
 *
 * Both kinds draw the same card, split by the Store's projects:
 *
 *   head      name (opens the Store) · ⋯
 *   figure    one large figure: the file's size (SQLite), else the projects'
 *             packed bytes, else their count (PostgreSQL); small words beside
 *             it (last write; or the database)
 *   where     Stored on its machine (SQLite, Home only); Connected from: one
 *             chip per machine that connects, then Connect a machine
 *             (PostgreSQL, spec v23)
 *   rows      up to three projects (the LayerFS mark), each with its bytes and
 *             its own bar: its share of the file, or of the projects
 *   status    only when something is happening: a transfer, in use, down
 *   foot      the backups badge and one action (SQLite); a PostgreSQL card
 *             has a foot only for "+N projects" or a status its server's
 *             header does not already say
 *
 * PostgreSQL cards sit under one header per saved login (spec v23 V23-1):
 * the login, its user, its Store count, its worst state, Rotate password…
 * and Remove login. There is one view of them.
 *
 * The backups badge counts the Store's bucket copies, marks their providers
 * and dots the worst state; it opens Backups (BackupDialogs.tsx).
 */
import { createPortal } from 'react-dom'
import { useEffect, useState, type ReactNode } from 'react'
import { Button, IconPlusOutlineRegular, MenuItemButton, StateDot } from '@deepseek-ai/dsh-client-ui-primitives'
import { enc, type LayerfsApi } from '../api.ts'
import { ago, bytes } from '../files/format.ts'
import type { Translate } from '../files/types.ts'
import { Card, LinkButton, MoreMenu } from '../ui/index.tsx'
import { useQuery } from '../ui/use-query.ts'
import { BucketMark } from './BucketPage.tsx'
import { JobPanel, OpPanel } from './JobProgress.tsx'
import type { StorageT } from './locale.ts'
import { loginGroups, machineStores, projectSplit, startKind, type PgLoginGroup, type ProjectSplit, type StoreKind } from './machine-stores.ts'
import { cardJob, pgStoreBytes, rowMachine, storeStateWords, storeTip } from './model.ts'
import { MachineGlyph } from '../machines/icons.tsx'
import { HomeGlyph, StoreGlyph } from './parts.tsx'
import { StoreAction, StoreMenu, stoppedBlock, storeNotice, useStoreTip, type LoginActions, type StoreActions } from './StoreCard.tsx'
import { backupCounts, backupProviders, storeBackups } from './store-backups.ts'
import type { StoreProjectsView, StoreStats } from './store-page.ts'
import { HOME_SCOPE, readStoreKind, readStoresAlt, watchHomeStores, writeStoreKind, writeStoresAlt } from './tabs.ts'
import type { StorageJob, StorageView, StoreRow } from './types.ts'
import css from './MachineStores.module.css'

const COUNTING_POLL_MS = 2000
const ROWS = 3

export interface MachineStoresProps {
  api: LayerfsApi
  machine: string
  view: StorageView
  t: StorageT
  jobs: readonly StorageJob[]
  dismissed: ReadonlySet<string>
  now: number
  actionsFor: (s: StoreRow) => StoreActions
  /** A saved PostgreSQL login's own actions (the PostgreSQL tab's login headers). */
  loginActions?: LoginActions | undefined
  /** The Sandboxes the Machines view knows: a Connected from chip counts the machine's Sandboxes of the Store. */
  sandboxes?: readonly { machine?: string | null, project?: string | null }[] | undefined
  /** Add Store's SQLite side: push a Store to this machine. */
  onPushHere: (machine: string) => void
  /** Connect a database: a new one checked from here, or one already connected checked from here. */
  onConnect: (machine: string) => void
  /** The PostgreSQL Stores with a Sandbox on this machine now (the Machines view knows): listed beside those connected from here. */
  busy?: ReadonlySet<string> | undefined
  onOpenStore: (name: string) => void
  /** Opens a project, by `store/project`. */
  onOpenProject: (key: string) => void
  /** Opens the Backups dialog of a SQLite Store. */
  onBackups: (store: string) => void
}

/** What every Store card needs; `machine` is the machine page the card is on, null on Home. */
interface CardProps {
  api: LayerfsApi
  machine: string | null
  view: StorageView
  t: StorageT
  jobs: readonly StorageJob[]
  dismissed: ReadonlySet<string>
  now: number
  actionsFor: (s: StoreRow) => StoreActions
  loginActions?: LoginActions | undefined
  sandboxes?: readonly { machine?: string | null, project?: string | null }[] | undefined
  onOpenStore: (name: string) => void
  onOpenProject: (key: string) => void
  onBackups: (store: string) => void
}

/** A board's two kinds: their Stores, the words around them, and each kind's add button. */
interface BoardProps extends CardProps {
  /** Tile ids and the remembered tile (per machine; `(home)` on Home). */
  actionHost?: HTMLSpanElement | null
  scope: string
  sqlite: StoreRow[]
  sqliteBytes: number | null
  postgresql: StoreRow[]
  noSqlite: string | null
  noPostgresql: string | null
  /** Per kind: the tile's small role word, the panel's note, its add button and its empty line. */
  words: Record<StoreKind, { role: string, note: string, add: ReactNode, empty: string }>
  /**
   * Home only, SQLite only: a second view the bar's small switch swaps in for
   * the Store cards (the backup buckets), with the switch's two labels and
   * counts and the view's own actions.
   */
  alt?: AltView | undefined
}

export interface AltView {
  /** The Store cards' label and count, and the second view's. */
  main: { label: string, count: number }
  other: { label: string, count: number }
  view: ReactNode
  /** The bar's actions while the second view shows. */
  add: ReactNode
}

function StoresBoard(props: BoardProps): ReactNode {
  const { scope, t, words } = props
  const [kind, setKindState] = useState<StoreKind>(() => startKind(props, readStoreKind(scope)))
  const setKind = (k: StoreKind) => { setKindState(k); writeStoreKind(scope, k) }
  const [buckets, setBucketsState] = useState(() => readStoresAlt())
  const setBuckets = (on: boolean) => { setBucketsState(on); writeStoresAlt(on) }
  // a link elsewhere (a bucket page's crumb, an old Backups link) points Home's board at a kind and view
  useEffect(() => (scope !== HOME_SCOPE ? undefined : watchHomeStores(() => {
    setKindState(readStoreKind(HOME_SCOPE) ?? 'sqlite')
    setBucketsState(readStoresAlt())
  })), [scope])
  const alt = kind === 'sqlite' ? props.alt : undefined
  const showAlt = !!alt && buckets
  const tiles: { kind: StoreKind, label: string, figure: string, why: string | null }[] = [
    {
      kind: 'sqlite', label: t('sqlite'),
      figure: props.sqlite.length ? [t(props.sqlite.length === 1 ? 'msStoreOne' : 'msStores', { n: props.sqlite.length }), props.sqliteBytes != null && bytes(props.sqliteBytes)].filter(Boolean).join(' · ') : t('msStores', { n: 0 }),
      why: props.sqlite.length ? null : props.noSqlite,
    },
    {
      kind: 'postgresql', label: t('postgresql'),
      figure: props.postgresql.length ? t(props.postgresql.length === 1 ? 'msDatabaseOne' : 'msDatabases', { n: props.postgresql.length }) : t('msDatabases', { n: 0 }),
      why: props.postgresql.length ? null : props.noPostgresql,
    },
  ]
  const onKey = (e: React.KeyboardEvent) => {
    if (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight') return
    const next = kind === 'sqlite' ? 'postgresql' : 'sqlite'
    setKind(next)
    document.getElementById(`lfs-ms-${scope}-${next}`)?.focus()
  }
  const sqlite = kind === 'sqlite'
  return (
    <div className={css.stack} data-machine-stores={scope}>
      {/* One line: the kind switch (its note and any reason on hover) and the kind's add button. */}
      <div className={css.bar}>
        <div className={css.tiles} role="tablist" aria-label={t('msKindLabel')} onKeyDown={onKey}>
          {tiles.map((tile) => (
            <button key={tile.kind} id={`lfs-ms-${scope}-${tile.kind}`} type="button" role="tab" className={css.tile}
              aria-selected={kind === tile.kind} tabIndex={kind === tile.kind ? 0 : -1} aria-controls={`lfs-ms-${scope}-panel`}
              onClick={() => setKind(tile.kind)} data-store-kind={tile.kind}
              title={[words[tile.kind].note, tile.why].filter(Boolean).join(' · ')}>
              <StoreGlyph kind={tile.kind} size={14} />
              <span className={css.tileLabel}>{tile.label}</span>
              <span className={css.tileFigure}>{tile.figure}</span>
            </button>
          ))}
        </div>
        {alt ? (
          <div className={css.views} role="tablist" aria-label={t('hsViewLabel')}>
            {([false, true] as const).map((on) => {
              const v = on ? alt.other : alt.main
              return (
                <button key={String(on)} type="button" role="tab" className={css.view} aria-selected={showAlt === on}
                  aria-controls={`lfs-ms-${scope}-panel`} onClick={() => setBuckets(on)} data-stores-view={on ? 'alt' : 'stores'}>
                  {v.label}<span className={css.viewCount}>{v.count}</span>
                </button>
              )
            })}
          </div>
        ) : null}
        {props.actionHost ? createPortal(showAlt ? alt.add : words[kind].add, props.actionHost) :
          <span className={css.barAction}>{showAlt ? alt.add : words[kind].add}</span>}
      </div>
      <section id={`lfs-ms-${scope}-panel`} role="tabpanel" aria-labelledby={`lfs-ms-${scope}-${kind}`} className={css.panel}>
        {showAlt ? alt.view
          : sqlite
          ? props.sqlite.length
            ? <div className={css.grid}>{props.sqlite.map((s) => <SqliteCard key={s.name} s={s} {...props} />)}</div>
            : <p className={css.empty}>{words.sqlite.empty}</p>
          : props.postgresql.length
            ? <PgGroups {...props} stores={props.postgresql} />
            : <p className={css.empty}>{words.postgresql.empty}</p>}
      </section>
    </div>
  )
}

export function MachineStores(props: MachineStoresProps): ReactNode {
  const { machine, view, t } = props
  const stores = machineStores(view, machine, t, props.busy)
  const place = machine === 'native' ? t('layerfsHome') : machine
  const plus = <IconPlusOutlineRegular size={12} />
  return (
    <StoresBoard {...props} {...stores} scope={machine} words={{
      sqlite: {
        role: t('msHeldHere'),
        note: machine === 'native' ? t('msSqliteBodyHome') : t('msSqliteBody', { machine: place }),
        add: <Button variant="outline" size="sm" icon={plus} disabled={!!stores.noSqlite} title={stores.noSqlite ?? undefined}
          onClick={() => props.onPushHere(machine)}>{machine === 'native' ? t('msAddSqliteHome') : t('msPushHere')}</Button>,
        empty: stores.noSqlite ?? t('msSqliteEmpty', { machine: place }),
      },
      postgresql: {
        role: t('msRunsHere'),
        note: t('msPgBody', { machine: place }),
        add: <Button variant="outline" size="sm" icon={plus} disabled={!!stores.noPostgresql} title={stores.noPostgresql ?? undefined}
          onClick={() => props.onConnect(machine)}>{t('connectDatabase')}</Button>,
        empty: stores.noPostgresql ?? t('msPgEmpty', { machine: place }),
      },
    }} />
  )
}

export interface HomeStoresProps extends Omit<CardProps, 'machine'> {
  /** Every Store, in the Overview's order. */
  stores: readonly StoreRow[]
  actionHost: HTMLSpanElement | null
  onNewStore: () => void
  /** Connect a database (a new PostgreSQL Store). */
  onConnect: () => void
  /** SQLite's second view: the backup buckets. */
  alt: AltView
}

/** Home › Stores: every Store, on the machine page's tiles and cards. */
export function HomeStores(props: HomeStoresProps): ReactNode {
  const { t } = props
  const sqlite = props.stores.filter((s) => s.kind === 'sqlite')
  const sizes = sqlite.map((s) => s.size?.bytes ?? null).filter((n): n is number => n != null)
  const plus = <IconPlusOutlineRegular size={12} />
  return (
    <StoresBoard {...props} machine={null} scope={HOME_SCOPE} sqlite={sqlite} sqliteBytes={sizes.length ? sizes.reduce((a, n) => a + n, 0) : null}
      postgresql={props.stores.filter((s) => s.kind === 'postgresql')}
      noSqlite={null} noPostgresql={null} words={{
        sqlite: {
          role: t('hsSqliteRole'), note: t('hsSqliteBody'), empty: t('hsSqliteEmpty'),
          add: <Button variant="outline" size="sm" icon={plus} onClick={props.onNewStore}>{t('newStore')}</Button>,
        },
        postgresql: {
          role: t('hsPgRole'), note: t('hsPgBody'), empty: t('hsPgEmpty'),
          add: <Button variant="outline" size="sm" icon={plus} onClick={props.onConnect}>{t('connectDatabase')}</Button>,
        },
      }} />
  )
}

/**
 * The Store's projects, as the Store page reads them (shared cache), read
 * again only while a ledger is being counted (`building`): a project that
 * could not be read (no job, perhaps an error) is not polled.
 */
function useSplit(api: LayerfsApi, s: StoreRow, total: number | null): ProjectSplit | null {
  const q = useQuery(() => api.call<StoreProjectsView>('GET', `/api/stores/${enc(s.name)}/projects`), [api, s.name],
    { refresh: [s.projects.length, s.size?.bytes], cache: `store-projects:${s.name}` })
  const counting = q.data?.projects.some((p) => p.building) ?? false
  useEffect(() => {
    if (!counting) return
    const timer = setTimeout(q.reload, COUNTING_POLL_MS)
    return () => clearTimeout(timer)
  }, [counting, q.data, q.reload])
  return q.data ? projectSplit(q.data.projects, total) : null
}

function Head({ s, on, t, machine, onOpenStore }: { s: StoreRow, on: StoreActions, t: StorageT, machine?: string, onOpenStore: (name: string) => void }): ReactNode {
  return (
    <div className={css.head}>
      <span className={css.glyph}><StoreGlyph kind={s.kind} size={18} /></span>
      <LinkButton className={css.name} onClick={() => onOpenStore(s.name)} title={s.name}>{s.name}</LinkButton>
      <StoreMenu store={s} on={on} t={t} machine={machine} />
    </div>
  )
}

/** The card's figure and the small words beside it (where the file is, last write; or the server). */
/** `tone`: `stale` a last-seen figure, `none` no figure (said in words, small). */
function Figure({ big, meta, title, tone }: { big: ReactNode, meta: ReactNode, title?: string | undefined, tone?: 'stale' | 'none' | undefined }): ReactNode {
  return (
    <div className={css.figure} title={title}>
      <span className={css.big} data-tone={tone}>{big}</span>
      {meta ? <span className={css.meta}>{meta}</span> : null}
    </div>
  )
}

/** A machine as a chip: its mark, its name and a dot (running or not); it opens the machine. */
function MachineChip({ name, view, t, health, title, onOpen }: {
  name: string | null, view: StorageView, t: StorageT, health: 'current' | 'problem' | 'unknown' | null, title?: string | undefined, onOpen: (name: string) => void
}): ReactNode {
  const platform = view.machines.find((m) => m.name === name)?.platform
  const label = name === 'native' ? t('layerfsHome') : name ?? '?'
  return (
    <button type="button" className={css.chip} disabled={!name} title={title ?? label} onClick={() => name && onOpen(name)}>
      <span className={css.metaMark}>{name === 'native' ? <HomeGlyph size={12} /> : <MachineGlyph platform={platform ?? ''} size={12} />}</span>
      <span className={css.chipName}>{label}</span>
      {health ? <span className={css.dot} data-health={health} /> : null}
    </button>
  )
}

/** Where a SQLite Store is, on one line: "Stored on" its one machine; the dot says whether the machine runs. */
export function MachineLine({ s, view, t, on }: { s: StoreRow, view: StorageView, t: StorageT, on: StoreActions }): ReactNode {
  const machine = s.machine
  const state = view.machines.find((m) => m.name === machine)?.state ?? null
  const health = machine === 'native' || state === 'ready' ? 'current' : state ? 'unknown' : null
  return (
    <div className={css.machines}>
      <span className={css.machinesLabel}>{t('hsStoredOn')}</span>
      <MachineChip name={machine} view={view} t={t} health={health} onOpen={on.openMachine}
        title={[machine === 'native' ? t('layerfsHome') : machine ?? '?', state ? storeStateWords(state, t) : null].filter(Boolean).join(' · ')} />
    </div>
  )
}

/**
 * Who connects to a PostgreSQL Store's database (spec v23 §5.1): one chip
 * per `connections` row, this computer first. The dot is the last reach
 * (reached, failed, not checked yet); a machine that is broken shows its
 * own state. The machine's Sandboxes of the Store show as `n ▣`. The tip
 * says the address (its own, or the Store's), when it was checked, the
 * server's version and latency, or that the check failed; never a raw
 * error. A chip checks from its machine again; the last chip is Connect a
 * machine.
 */
function ConnectedFrom({ s, view, t, on, now, sandboxes }: { s: StoreRow, view: StorageView, t: StorageT, on: StoreActions, now: number, sandboxes?: CardProps['sandboxes'] }): ReactNode {
  const rows = s.connectedFrom ?? []
  return (
    <div className={css.machines} data-connected-from={s.name}>
      <span className={css.machinesLabel}>{t('pgConnectedFromLabel')}</span>
      <div className={css.chips}>
        {rows.map((c) => {
          const name = rowMachine(c)
          const record = view.machines.find((m) => m.name === name)
          const broken = record?.state === 'broken'
          const health = broken || c.reached === 'failed' ? 'problem' : c.reached === 'ok' ? 'current' : 'unknown'
          const boxes = (sandboxes ?? []).filter((b) => (b.machine ?? 'native') === name && b.project?.startsWith(`${s.name}/`)).length
          const reach = c.reach
          const tip = [
            c.machine === null ? t('pgThisComputer') : name,
            c.own ? t('pgOwnAddress', { server: c.server ? `${c.server.host}:${c.server.port ?? 5432}` : '' }) : t('pgDefaultAddress'),
            reach?.at ? t('pgCheckedAgo', { when: ago(reach.at, now, t as Translate) }) : t('pgNotChecked'),
            reach?.ok ? [reach.version ? t('msPgShort', { version: reach.version.split(' ')[0]! }) : null, reach.latencyMs != null ? t('msReachableMs', { latency: reach.latencyMs }) : null].filter(Boolean).join(' · ') || null : reach ? t('pgCheckFailed') : null,
            broken && record ? storeStateWords(record.state ?? 'broken', t) : null,
            boxes ? t(boxes === 1 ? 'pgSandboxOne' : 'pgSandboxes', { n: boxes }) : null,
          ].filter(Boolean).join(' · ')
          return (
            <button key={c.machine ?? '(here)'} type="button" className={css.chip} data-connected={name} data-reached={c.reached ?? 'none'} title={tip}
              onClick={() => (broken ? on.openMachine(name) : on.check(c.machine ?? undefined))}>
              <span className={css.metaMark}>{c.machine === null ? <HomeGlyph size={12} /> : <MachineGlyph platform={record?.platform ?? ''} size={12} />}</span>
              <span className={css.chipName}>{c.machine === null ? t('pgThisComputer') : name}</span>
              <span className={css.dot} data-health={health} />
              {boxes ? <span className={css.chipCount} aria-label={t(boxes === 1 ? 'pgSandboxOne' : 'pgSandboxes', { n: boxes })}>{boxes} ▣</span> : null}
            </button>
          )
        })}
        <button type="button" className={css.chip} data-add data-connect-machine={s.name} onClick={on.connectMachine} title={t('pgConnectMachineTip')}>
          <IconPlusOutlineRegular size={10} />{t('pgConnectMachine')}
        </button>
      </div>
    </div>
  )
}

const projectsWords = (n: number, t: StorageT) => (n === 1 ? t('msProjectOne') : t('msProjectCount', { n }))

/**
 * The card's kept room: up to three projects, each with its packed bytes and
 * a bar for its share of the file (SQLite) or of the projects (PostgreSQL;
 * no bar while its bytes are unknown). A status (a transfer, in use, down)
 * takes the top and leaves one project row; no project leaves the room
 * empty with Add project.
 */
function Slot({ s, split, t, now, status, onAdd, onOpenProject }: { s: StoreRow, split: ProjectSplit | null, t: StorageT, now: number, status?: ReactNode, onAdd: () => void, onOpenProject: (key: string) => void }): ReactNode {
  const tt = t as Translate
  const rows = split?.rows ?? [...s.projects].sort().map((name) => ({ name, bytes: null, share: 0, hue: 0, at: null, counting: true, error: null }))
  return (
    <div className={css.slot}>
      {status}
      {!s.projects.length ? (
        status ? null : <p className={css.none}>{t('msNoProjects')} · <LinkButton className={css.noneAction} onClick={onAdd}>{t('addProject')}</LinkButton></p>
      ) : (
        <ul className={css.projects}>
          {rows.slice(0, status ? 1 : ROWS).map((r) => (
            <li key={r.name}>
              <button type="button" className={css.project} onClick={() => onOpenProject(`${s.name}/${r.name}`)}
                title={[t('msOpenProject', { project: r.name }), r.at ? t('msLastWrite', { when: ago(r.at, now, tt) }) : null].filter(Boolean).join(' · ')}>
                <span className={css.projectName}>{r.name}</span>
                <span className={css.projectMeta}>
                  {r.bytes === null ? (r.counting ? t('spReading') : <span title={r.error ?? t('msNotCounted')}>—</span>) : r.at === null ? t('spNoCommits') : bytes(r.bytes)}
                </span>
                {r.bytes !== null ? <span className={css.track} aria-hidden="true"><span style={{ width: `${Math.round(r.share * 1000) / 10}%` }} /></span> : <span />}
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}

/** "+N projects" in the foot when the room holds fewer than the Store has; opens the Store. */
function More({ s, shown, t, onOpenStore }: { s: StoreRow, shown: number, t: StorageT, onOpenStore: (name: string) => void }): ReactNode {
  const rest = s.projects.length - shown
  return rest > 0 ? <LinkButton className={css.more} onClick={() => onOpenStore(s.name)}>{t(rest === 1 ? 'msMoreProjectOne' : 'msMoreProjects', { n: rest })}</LinkButton> : null
}

/** The backups badge: provider marks, the count and the worst state's dot; "No backup" when there is none. */
function BackupsBadge({ s, view, t, onBackups, onBackup }: { s: StoreRow, view: StorageView, t: StorageT, onBackups: () => void, onBackup: () => void }): ReactNode {
  const backups = storeBackups(s, view.buckets)
  if (!backups.length) return <button type="button" className={css.badge} data-none onClick={onBackup} title={t('noBackup')}>{t('msNoBackup')}</button>
  const n = backupCounts(backups)
  const worst = n.problem ? 'problem' : n.stale ? 'stale' : n.unknown ? 'unknown' : 'current'
  const providers = backupProviders(backups)
  return (
    <button type="button" className={css.badge} onClick={onBackups} aria-label={`${t('msBackupsOpen', { store: s.name, n: backups.length })}. ${t('bkHealth', n)}`} title={t('bkHealth', n)}>
      <span className={css.marks}>{providers.slice(0, 3).map((p) => <span key={p ?? '?'} className={css.mark}><BucketMark provider={p} size={13} /></span>)}</span>
      {backups.length === 1 ? t('msBackupOne') : t('msBackupCount', { n: backups.length })}
      <span className={css.dot} data-health={worst} />
    </button>
  )
}

interface Act { label: string, run: () => void, disabled?: boolean }

/** The foot's actions: one button, the rest in a ⋯ menu. */
function FootActions({ main, rest, label }: { main: Act, rest: Act[], label: string }): ReactNode {
  return (
    <span className={css.footActions}>
      <Button size="sm" variant="ghost" disabled={main.disabled} onClick={main.run}>{main.label}</Button>
      {rest.length ? (
        <MoreMenu label={label}>
          {(close) => rest.map((a) => <MenuItemButton key={a.label} disabled={a.disabled} onSelect={() => { close(); a.run() }}>{a.label}</MenuItemButton>)}
        </MoreMenu>
      ) : null}
    </span>
  )
}

/** A sqlite Store's size: as read now, else the last size read (its machine did not answer), else "Size unknown". */
function SizeFigure({ s, t, now, last }: { s: StoreRow, t: StorageT, now: number, last: string | null }): ReactNode {
  const size = s.size
  const seen = size?.stale && size.seen ? ago(size.seen, now, t as Translate) : null
  const words = [seen ? t('msSizeSeen', { when: seen }) : null, last ? t('msLastWrite', { when: last }) : null].filter(Boolean).join(' · ')
  const meta = words ? <span className={css.metaText}>{words}</span> : null
  if (!size) return <Figure big={t('msSizeUnknown')} tone="none" meta={meta} />
  const parts = bytes(size.bytes)
  return <Figure big={bytes(size.bytes)} tone={size.stale ? 'stale' : undefined} meta={meta}
    // a stopped machine is stopped, not "does not answer" (R2-2)
    title={size.stale ? t(stoppedBlock(s) ? 'msSizeStaleStoppedTitle' : 'msSizeStaleTitle', { parts, when: seen ?? '?' }) : undefined} />
}

/** A Store's state in words on its card (LayerFS's sentence as the tip) and its fix as the one action. */
/** The card's status: dot and word, LayerFS's sentence (or its fix) as the tip, and the one fix (R2-2: never a bare sentence). */
function StoreStatus({ s, t, words, onStart }: { s: StoreRow, t: StorageT, words: string, onStart?: (() => void) | undefined }): ReactNode {
  const tip = useStoreTip(s, t) ?? s.fix ?? undefined
  return (
    <div className={css.statusRow} data-tone={s.state === 'broken' ? 'danger' : 'warning'}>
      <StateDot state={s.state === 'broken' ? 'error' : 'idle'} size={8} />
      <span className={css.statusText} title={tip}>{words}</span>
      <StoreAction store={s} t={t} onStart={onStart} />
    </div>
  )
}

function SqliteCard({ s, api, machine, t, view, jobs, dismissed, now, actionsFor, onOpenStore, onOpenProject, onBackups }: CardProps & { s: StoreRow }): ReactNode {
  const on = actionsFor(s)
  const job = cardJob(jobs, s.name, dismissed, now)
  const split = useSplit(api, s, s.size?.bytes ?? null)
  const busy = s.state !== 'ready' || !!job && !job.ended
  const running = !!job && !job.ended
  const last = split?.lastWrite ? ago(split.lastWrite, now, t as Translate) : null
  // Shown only when something is happening: ready says nothing. A state word with its reason; the fix line is the tip (the one action).
  const notice = storeNotice(s, null, t)
  const status = job ? <JobPanel job={job} t={t} onRetry={() => on.retry(job)} onDismiss={() => on.dismiss(job)} />
    : s.op ? <OpPanel store={s} t={t} />
      : notice ? <StoreStatus s={s} t={t} words={notice} onStart={on.start} />
        : null
  return (
    <Card className={css.card} data-store={s.name} data-kind="sqlite">
      <Head s={s} on={on} t={t} onOpenStore={onOpenStore} />
      <SizeFigure s={s} t={t} now={now} last={last} />
      {machine === null ? <MachineLine s={s} view={view} t={t} on={on} /> : null}
      <Slot s={s} split={split} t={t} now={now} status={status} onAdd={on.addProject} onOpenProject={onOpenProject} />
      <div className={css.foot}>
        <BackupsBadge s={s} view={view} t={t} onBackups={() => onBackups(s.name)} onBackup={on.backup} />
        <More s={s} shown={status ? 1 : ROWS} t={t} onOpenStore={onOpenStore} />
        {/* the head's ⋯ holds Move, Back up and Remove */}
        {running ? null : s.machine === 'native'
          ? <FootActions label={t('msMoreActions', { name: s.name })} main={{ label: t('pushTo'), run: on.push, disabled: s.state !== 'ready' }} rest={[]} />
          : <FootActions label={t('msMoreActions', { name: s.name })} main={{ label: t('msPull'), run: on.pull, disabled: busy }} rest={[]} />}
      </div>
    </Card>
  )
}

/** A group's state as a dot. */
const healthOf = (state: string) => (state === 'ready' ? 'current' : state === 'broken' ? 'problem' : 'unknown')

/** A login group's tip: the servers its Stores are on, when it was saved, and its Stores. */
function loginTip(g: PgLoginGroup, t: StorageT, now: number): string {
  return [
    g.servers.length ? g.servers.join(', ') : null,
    g.saved ? t('pgSavedAgo', { when: ago(g.saved, now, t as Translate) }) : null,
    g.stores.length ? t('pgUsedBy', { stores: g.stores.map((s) => s.name).join(', ') }) : t('pgLoginUnused'),
    g.fix ?? null,
  ].filter(Boolean).join(' · ')
}

/**
 * PostgreSQL (spec v23 §5.1): the cards under one header per saved login
 * (V23-1), a login no Store names too, then "No login" for Stores whose
 * login is missing. The header is the login, its user, its Store count and
 * one state word (its Stores' worst) with a dot, Rotate password… as its one
 * action and Remove login in its ⋯ (only while no Store names it). A
 * machine page shows only the logins of the Stores connected from it.
 */
function PgGroups(props: CardProps & { stores: StoreRow[] }): ReactNode {
  const { t, view, loginActions, now, machine } = props
  const groups = loginGroups(props.stores, view.auths ?? []).filter((g) => machine === null || g.stores.length > 0)
  return (
    <div className={css.pgGroups}>
      {groups.map((g) => {
        const count = g.stores.length === 1 ? t('msStoreOne', { n: 1 }) : t('msStores', { n: g.stores.length })
        // a ready login whose Stores' last reach failed (a wrong password after a rotation) is not "Ready": the header follows its chips (M FAIL 10)
        const failed = g.state === 'ready' ? g.stores.flatMap((st) => (st.connectedFrom ?? []).filter((c) => c.reached === 'failed')) : []
        const state = failed.length ? 'broken' : g.state
        const causes = [...new Set(failed.map((c) => c.reach?.error).filter((e): e is string => !!e))]
        return (
          <section key={g.auth ?? '(no login)'} className={css.pgGroup} data-pg-login={g.auth ?? ''} aria-label={g.auth ?? t('pgNoLogin')}>
            <header className={css.pgServer} title={[loginTip(g, t, now), ...causes].join('\n')}>
              <StoreGlyph kind="postgresql" size={14} />
              <span className={css.pgServerName}>{g.auth ?? t('pgNoLogin')}</span>
              <span className={css.pgServerFacts}>{[g.user, count].filter(Boolean).join(' · ')}</span>
              <span className={css.check} data-state={state === 'broken' ? 'failed' : undefined}>
                <span className={css.dot} data-health={healthOf(state)} />{failed.length ? t('pgLoginNotReached') : storeStateWords(g.state, t)}
              </span>
              {g.auth && loginActions ? (
                <span className={css.pgServerAdd}>
                  <Button size="sm" variant="ghost" onClick={() => loginActions.rotate(g.auth!)}>{t('pgRotate')}</Button>
                  <MoreMenu label={t('pgLoginActions', { auth: g.auth })}>
                    {(close) => (
                      <MenuItemButton danger disabled={g.stores.length > 0} disabledReason={g.stores.length ? t('pgLoginInUse', { stores: g.stores.map((s) => s.name).join(', ') }) : undefined}
                        onSelect={() => { close(); loginActions.remove(g.auth!) }}>{t('pgRemoveLogin')}</MenuItemButton>
                    )}
                  </MoreMenu>
                </span>
              ) : null}
            </header>
            {g.stores.length
              ? <div className={css.grid}>{g.stores.map((s) => <PgCard key={s.name} s={s} {...props} />)}</div>
              : <p className={css.empty}>{t('pgLoginUnused')}</p>}
          </section>
        )
      })}
    </div>
  )
}

function PgCard({ s, api, machine, t, view, jobs, dismissed, now, actionsFor, sandboxes, onOpenStore, onOpenProject }: CardProps & { s: StoreRow }): ReactNode {
  const on = actionsFor(s)
  // A compaction on this database shows its steps and result here, as on a sqlite card.
  const job = cardJob(jobs, s.name, dismissed, now)
  // a broken database: its words, LayerFS's sentence as the tip, its fix as the one action (Save a login…, Update)
  const notice = storeNotice(s, null, t)
  const status = job ? <JobPanel job={job} t={t} onRetry={() => on.retry(job)} onDismiss={() => on.dismiss(job)} />
    : s.op ? <OpPanel store={s} t={t} />
      : notice ? <StoreStatus s={s} t={t} words={notice} /> : null
  // The database's size as LayerFS measures it (store get --with stats), read again when its projects change or a job ends.
  const stats = useQuery(() => api.call<StoreStats>('GET', `/api/stores/${enc(s.name)}/stats`), [api, s.name],
    { refresh: [s.projects.length, job?.ended], cache: `store-stats:${s.name}` })
  const size = pgStoreBytes(stats.data)
  const split = useSplit(api, s, size)
  const pg = s.postgresql
  // Until the size is read: the projects' Commit counters, which leave out what Init saved.
  const counted = split?.rows.filter((r) => r.bytes !== null) ?? []
  const total = size ?? (counted.length ? counted.reduce((n, r) => n + (r.bytes ?? 0), 0) : null)
  const more = s.projects.length > ROWS
  return (
    <Card className={css.card} data-store={s.name} data-kind="postgresql" tone={s.state === 'broken' ? 'danger' : undefined}>
      <Head s={s} on={on} t={t} machine={machine ?? undefined} onOpenStore={onOpenStore} />
      {/* refused stats (a broken database): its size is unknown, "—", never the 0 B of its uncounted projects */}
      <Figure big={stats.error && size === null ? '—' : total !== null ? bytes(total) : projectsWords(s.projects.length, t)}
        title={[pg?.server, pg?.binding ? t('pgDatabaseId', { id: pg.binding.slice(0, 8) }) : null].filter(Boolean).join(' · ') || undefined}
        meta={pg?.database ? <span className={css.metaText}>{pg.database}</span> : null} />
      <ConnectedFrom s={s} view={view} t={t} on={on} now={now} sandboxes={sandboxes} />
      <Slot s={s} split={split} t={t} now={now} status={status} onAdd={on.addProject} onOpenProject={onOpenProject} />
      {more || status ? (
        <div className={css.foot}>
          <More s={s} shown={status ? 1 : ROWS} t={t} onOpenStore={onOpenStore} />
        </div>
      ) : null}
    </Card>
  )
}
