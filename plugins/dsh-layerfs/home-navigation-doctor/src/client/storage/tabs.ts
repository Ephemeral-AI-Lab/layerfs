import type { StoreKind } from './machine-stores.ts'

/**
 * The pages the Storage page can show, as ids: the overview, the machines
 * board, a machine, a Sandbox (`sandbox:<machine>/<name>`), a Store, a project
 * (`project:<store>/<name>`), a bucket and a backup file (`backup:<bucket>/<file>`).
 * Links from other pages (`openStorage`) name one of these.
 */
export type TabId = 'overview' | 'machines' | 'health' | 'logs' | `machine:${string}` | `sandbox:${string}` | `store:${string}` | `project:${string}` | `bucket:${string}` | `backup:${string}`

/** Whether `id` names a page the Storage page can show. */
export function isTabId(id: string): id is TabId {
  return id === 'overview' || id === 'machines' || id === 'health' || id === 'logs'
    || /^(machine|store|bucket):.+$/.test(id)
    || /^(sandbox|project|backup):[^/]+\/.+$/.test(id)
}

export interface PageHistory { stack: TabId[], at: number }

export function visitPage(history: PageHistory, id: TabId): PageHistory {
  if (history.stack[history.at] === id) return history
  const stack = [...history.stack.slice(0, history.at + 1), id].slice(-50)
  return { stack, at: stack.length - 1 }
}

/** Prune both directions without leaving duplicate stops on either side of a removal. */
export function forgetPages(history: PageHistory, gone: (id: TabId) => boolean): PageHistory {
  const stack: TabId[] = []
  let at = 0
  for (const [index, id] of history.stack.entries()) {
    if (!gone(id) && stack.at(-1) !== id) stack.push(id)
    if (index === history.at) at = Math.max(0, stack.length - 1)
  }
  if (stack.length === history.stack.length) return history
  return { stack: stack.length ? stack : ['overview'], at }
}

export function underPage(id: TabId, parent: TabId): boolean {
  if (id === parent) return true
  const [kind] = parent.split(':')
  const children: Record<string, string> = { store: 'project', bucket: 'backup', machine: 'sandbox' }
  const child = children[kind ?? '']
  return !!child && id.startsWith(`${child}:${parent.slice(parent.indexOf(':') + 1)}/`)
}

export type OverviewTab = 'projects' | 'stores' | 'machines'
export const isOverviewTab = (v: unknown): v is OverviewTab => v === 'projects' || v === 'stores' || v === 'machines'

// ---------------------------------------------------------------- a Stores board's kind, and Home's second view

/** The board scope of Home › Stores (a machine page's board is scoped by its machine). */
export const HOME_SCOPE = '(home)'

const KIND_KEY = 'layerfs.machineStoreKind'
const ALT_KEY = 'dsh-layerfs:home-stores-alt'
const kindWatchers = new Set<() => void>()

function storedMap(key: string): Record<string, unknown> {
  try { const v = JSON.parse(localStorage.getItem(key) ?? '{}'); return v && typeof v === 'object' && !Array.isArray(v) ? v : {} } catch { return {} }
}

/** The kind this viewer had open last on a board (per machine; `HOME_SCOPE` on Home). */
export function readStoreKind(scope: string): StoreKind | null {
  const v = storedMap(KIND_KEY)[scope]
  return v === 'sqlite' || v === 'postgresql' ? v : null
}

export function writeStoreKind(scope: string, kind: StoreKind): void {
  try { localStorage.setItem(KIND_KEY, JSON.stringify({ ...storedMap(KIND_KEY), [scope]: kind })) } catch { /* the tile still switches */ }
}

/** Whether Home › Stores shows SQLite's second view (Buckets) instead of its Store cards. PostgreSQL has one view. */
export function readStoresAlt(): boolean {
  return storedMap(ALT_KEY).sqlite === true
}

export function writeStoresAlt(buckets: boolean): void {
  try { localStorage.setItem(ALT_KEY, JSON.stringify({ sqlite: buckets })) } catch { /* the view still switches */ }
}

/** Point Home › Stores at `kind` (SQLite: its Store cards, or `buckets`) and tell a board already showing it. */
export function openHomeStores(kind: StoreKind, buckets: boolean): void {
  writeStoreKind(HOME_SCOPE, kind)
  if (kind === 'sqlite') writeStoresAlt(buckets)
  for (const watch of [...kindWatchers]) watch()
}

/** Hear `openHomeStores`. Returns the release. */
export function watchHomeStores(listener: () => void): () => void {
  kindWatchers.add(listener)
  return () => { kindWatchers.delete(listener) }
}

// ---------------------------------------------------------------- a machine's or Sandbox's own tabs

/** The tabs inside a machine page (Overview, Files, Worker log), a Sandbox page (Terminals, Files, Daemon log) and a Store page (Overview, Projects, Backups or Connections). */
export type DetailTab = 'overview' | 'terminals' | 'files' | 'log' | 'sandboxes' | 'stores' | 'images' | 'projects' | 'backups' | 'connections'
export type DetailKind = 'machine' | 'sandbox' | 'store'

const DETAIL_KEY = 'dsh-layerfs:detail-tabs'
/** How many machines and Sandboxes keep their last tab. */
const DETAIL_KEEP = 100
const DETAIL_TABS: readonly unknown[] = ['overview', 'terminals', 'files', 'log', 'sandboxes', 'stores', 'images', 'projects', 'backups', 'connections']
const isDetailTab = (v: unknown): v is DetailTab => DETAIL_TABS.includes(v)

/** dsh has no URLs: the tab a link asks for is kept here, in the app, and mirrored to localStorage as the last one per entity. */
const chosen = new Map<string, DetailTab>()
const watchers = new Set<(key: string, tab: DetailTab) => void>()
const detailKey = (kind: DetailKind, name: string): string => `${kind}:${name}`

function storedTabs(): Record<string, unknown> {
  try { const v = JSON.parse(localStorage.getItem(DETAIL_KEY) ?? '{}'); return v && typeof v === 'object' && !Array.isArray(v) ? v : {} } catch { return {} }
}

/** The tab this viewer had open last on a machine or Sandbox, if any (storage may be blocked). */
export function readDetailTab(kind: DetailKind, name: string): DetailTab | null {
  const key = detailKey(kind, name)
  const own = chosen.get(key)
  if (own) return own
  const v = storedTabs()[key]
  return isDetailTab(v) ? v : null
}

/** Remember the tab, and tell a page already showing that machine or Sandbox to switch to it. */
export function writeDetailTab(kind: DetailKind, name: string, tab: DetailTab): void {
  const key = detailKey(kind, name)
  chosen.delete(key)
  chosen.set(key, tab)
  if (chosen.size > DETAIL_KEEP) chosen.delete(chosen.keys().next().value!)
  try {
    const all = storedTabs()
    delete all[key]
    all[key] = tab
    const keys = Object.keys(all)
    for (const old of keys.slice(0, Math.max(0, keys.length - DETAIL_KEEP))) delete all[old]
    localStorage.setItem(DETAIL_KEY, JSON.stringify(all))
  } catch { /* the tab still switches */ }
  for (const watch of [...watchers]) watch(key, tab)
}

/** Hear a tab chosen for `kind:name` (a link elsewhere on the page). Returns the release. */
export function watchDetailTab(kind: DetailKind, name: string, listener: (tab: DetailTab) => void): () => void {
  const key = detailKey(kind, name)
  const watch = (k: string, tab: DetailTab) => { if (k === key) listener(tab) }
  watchers.add(watch)
  return () => { watchers.delete(watch) }
}

// ---------------------------------------------------------------- a machine's or Sandbox's Files view

/**
 * The Files views: the roots LayerFS ranks by role (its own folders on a
 * machine; a Sandbox's Workspaces on a Sandbox), a Sandbox's other folders
 * (`sandbox`: home, daemon log), and everything from `/`.
 */
export type FilesViewId = 'layerfs' | 'sandbox' | 'whole'

/** Where a Files tab is: its view, and the path open in it (null: the view's root). */
export interface FilesPlace { view: FilesViewId, path: string | null }

const FILES_KEY = 'dsh-layerfs:inspect-files'
const isFilesView = (v: unknown): v is FilesViewId => v === 'layerfs' || v === 'sandbox' || v === 'whole'
const placeOf = (v: unknown): FilesPlace | null => {
  const o = v as { view?: unknown, path?: unknown } | null
  if (!o || typeof o !== 'object' || !isFilesView(o.view)) return null
  return { view: o.view, path: typeof o.path === 'string' && o.path.startsWith('/') && o.path.length <= 4096 ? o.path : null }
}

/** As the detail tabs: the place a link asks for is kept in the app, and the last one per entity mirrored to localStorage. */
const places = new Map<string, FilesPlace>()
const placeWatchers = new Set<(key: string, place: FilesPlace) => void>()

function storedPlaces(): Record<string, unknown> {
  try { const v = JSON.parse(localStorage.getItem(FILES_KEY) ?? '{}'); return v && typeof v === 'object' && !Array.isArray(v) ? v : {} } catch { return {} }
}

/** The view and path this viewer had last in a machine's or Sandbox's Files tab, if any (storage may be blocked). */
export function readFilesPlace(kind: DetailKind, name: string): FilesPlace | null {
  const key = detailKey(kind, name)
  return places.get(key) ?? placeOf(storedPlaces()[key])
}

/**
 * Remember where a Files tab is.
 * @param tell - a link from elsewhere: a Files tab already showing that machine or Sandbox goes there.
 */
export function writeFilesPlace(kind: DetailKind, name: string, place: FilesPlace, tell = false): void {
  const key = detailKey(kind, name)
  places.delete(key)
  places.set(key, place)
  if (places.size > DETAIL_KEEP) places.delete(places.keys().next().value!)
  try {
    const all = storedPlaces()
    delete all[key]
    all[key] = place
    const keys = Object.keys(all)
    for (const old of keys.slice(0, Math.max(0, keys.length - DETAIL_KEEP))) delete all[old]
    localStorage.setItem(FILES_KEY, JSON.stringify(all))
  } catch { /* the view still goes there */ }
  if (tell) for (const watch of [...placeWatchers]) watch(key, place)
}

/** Hear a place a link asked for on `kind:name`. Returns the release. */
export function watchFilesPlace(kind: DetailKind, name: string, listener: (place: FilesPlace) => void): () => void {
  const key = detailKey(kind, name)
  const watch = (k: string, place: FilesPlace) => { if (k === key) listener(place) }
  placeWatchers.add(watch)
  return () => { placeWatchers.delete(watch) }
}
