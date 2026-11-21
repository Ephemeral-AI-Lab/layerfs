/**
 * The Files tab of a machine page and of a Sandbox page (layerfs-project#178
 * "Files view"), read through `GET /api/machines/:id/files` or
 * `/api/sandboxes/:name/files`: one shared tree.
 *
 * - Views. A machine: LayerFS folders (the roots LayerFS ranks by role)
 *   and, when the roots answer offers `/`, Whole filesystem (/). A Sandbox:
 *   Workspaces, each a folder of its saved files (its Branch at its base
 *   Commit, read through the project: LayerFS lists no Workspace mount),
 *   Sandbox folders (home, daemon log), and Whole filesystem (/) when offered.
 *   On a machine page a Workspace mount is listed, never entered.
 *   Each view keeps its own folder, opened folders and file. The page never
 *   tests a platform or backend name.
 * - A path bar: the path shown, editable; "Go to path" takes a path typed or
 *   pasted from a terminal. A folder becomes the tree's top (a breadcrumb
 *   leads back); a file opens with its folder. A path that cannot be opened
 *   falls back to the view's root, and a line says so.
 * - A filter over the entries read so far. No "expand all".
 * - Folders are read as they open, one page at a time ("More entries"; a
 *   page LayerFS ended at its time limit says so). A read shows its seconds
 *   after 1 s, with Cancel. Refresh reads the open folders again; nothing polls.
 * - Entries LayerFS lists but never opens (restricted, unreadable, system,
 *   mounts, links, other kinds) are rows that cannot be activated.
 * - Below 600 px the tree drills in: one folder a screen.
 *
 * Reads: at most two of this tab at a time (inspect-files.ts), each dropped
 * when its folder collapses or the view is left. A refusal stays inside the
 * folder or file it is about.
 */
import { useCallback, useEffect, useMemo, useRef, useState, type FormEvent, type MutableRefObject, type ReactNode } from 'react'
import {
  Button, FileTypeIcon, IconChevronLeftOutlineRegular, IconCloseOutlineRegular, IconFolderCloseRegular, IconRefreshOutlineRegular, IconSearchOutlineRegular, Input, SegmentedControl,
} from '@deepseek-ai/dsh-client-ui-primitives'
import { FileTree, type TreeDir, type TreeMore, type TreeRow } from '../files/FileTree.tsx'
import { bytes, shortId, when } from '../files/format.ts'
import { EmptyState } from '../files/States.tsx'
import type { Translate as FilesTranslate } from '../files/types.ts'
import { WindowedText, type TextWindow } from '../files/WindowedText.tsx'
import { HostError } from '../api.ts'
import { readFilesPlace, watchFilesPlace, writeFilesPlace, type DetailKind, type FilesPlace, type FilesViewId } from '../storage/tabs.ts'
import { LinkButton, Notice } from '../ui/index.tsx'
import { errorCode, errorText } from '../ui/use-query.ts'
import { useMachinesContext } from './context.tsx'
import { LockGlyph, MountGlyph, SandboxGlyph, SystemGlyph } from './icons.tsx'
import {
  ROOTS, ReadQueue, SB_ROOTS, WS_ROOTS, actOf, baseName, crumbsOf, filesRoute, groupEntries, isDropped, isLinkedRole, isUnder, isWorkerLog, isWorkspaceEntry, parentOf, pastedPath, readFiles, readKey,
  readWorkspace, refusalError, waitedSeconds, wholeOf, type FileEntry, type FilesAsk, type FilesView, type InspectEntry, type Stale, type WorkspaceSource,
} from './inspect-files.ts'
import type { MachinesKey } from './locale.ts'
import { workspaceChanges, workspaceName, type WorkspaceInfo } from './model.ts'
import css from './inspect-files.module.css'
import { clockTime } from '../ui/locale-sync.ts'

/** The width under which the tree drills in instead of expanding in place. */
const NARROW_PX = 600

/**
 * A label per role LayerFS answers (machine.files: `data` … `image`; sandbox.files: `home`).
 * `workspace` and `log` have rows of their own; `machine` and `root` are the
 * Whole filesystem view, never a row. A role this page does not know shows its path.
 */
const ROLE: Record<string, MachinesKey> = {
  data: 'roleData', stores: 'roleStores', sandbox_records: 'roleSandboxRecords', run_folders: 'roleRunFolders', images: 'roleImages', staging: 'roleStaging',
  logs: 'roleLogs', state: 'roleState', bundle: 'roleBundle', image: 'roleImage', home: 'roleHome',
}

export interface InspectFilesProps {
  kind: DetailKind
  name: string
  /** The tree's accessible name ("Files of <name>"). */
  label: string
  /** A Sandbox's Workspaces (sandbox.get), for its Workspace rows and where their files are read; null while that read has not answered. */
  workspaces?: readonly WorkspaceInfo[] | null | undefined
  /** A Sandbox's project: its Workspaces' saved files are read from it. */
  project?: string | null | undefined
  /** Opens the page's log tab (the daemon or worker log's own row); absent when the page has no such tab. */
  onOpenLog?: (() => void) | undefined
  /**
   * On a machine page: what opens the Daemon log tab of the Sandbox whose run
   * folder is named `folder` (a `daemon.log` met in the machine's run folders);
   * null when no Sandbox of this machine has that run folder.
   */
  onOpenSandboxLog?: ((folder: string) => (() => void) | null) | undefined
  /** The name of that tab, for a log row's tooltip. */
  logTab?: string | undefined
  /** What stands beside an `Unavailable` refusal (a stopped machine's Start). */
  unavailableAction?: ReactNode
}

/** What one view shows: the folder at the tree's top (null: the view's root), the folders open under it, the file open. */
interface ViewState { base: string | null, open: ReadonlySet<string>, file: string | null }
const EMPTY_VIEW: ViewState = { base: null, open: new Set(), file: null }
const NO_OPEN: ReadonlySet<string> = new Set()

/** What a folder's last read said beyond its entries. */
interface FolderMeta {
  reason: 'deadline' | 'limit' | null
  /** The entries shown are a kept answer: reading again was refused. */
  stale: Stale | null
  /** The next page's read was refused. */
  moreError: Stale | null
}

interface Pending { since: number, more: boolean, view: FilesViewId }

/** How a tab reads one path: the files route, or a Workspace's saved files. */
type Reader = (route: string, ask: FilesAsk, signal: AbortSignal) => Promise<FilesView>

/** The folders of one Files tab: what was read, what is being read, and the reads themselves. */
function useInspectFolders(kind: DetailKind, name: string, reader: MutableRefObject<Reader>) {
  const { api, t } = useMachinesContext()
  const route = filesRoute(kind, name)
  const queue = useMemo(() => new ReadQueue(), [route])
  const [dirs, setDirs] = useState<Map<string, TreeDir<InspectEntry>>>(new Map())
  const [nexts, setNexts] = useState<Record<string, string | null>>({})
  const [meta, setMeta] = useState<Record<string, FolderMeta>>({})
  const [pending, setPending] = useState<Record<string, Pending>>({})
  /** The `/` a roots answer offers as the second view; undefined until the roots answered. */
  const [whole, setWhole] = useState<FileEntry | null | undefined>(undefined)
  const controllers = useRef(new Map<string, AbortController>())
  const read = useRef(dirs)
  read.current = dirs
  const live = useRef(true)
  useEffect(() => { live.current = true; return () => { live.current = false; for (const c of controllers.current.values()) c.abort(); controllers.current.clear() } }, [])

  const settle = useCallback((key: string, controller: AbortController): boolean => {
    if (!live.current || controllers.current.get(key) !== controller) return false
    controllers.current.delete(key)
    setPending((p) => { const { [key]: _gone, ...rest } = p; return rest })
    return true
  }, [])

  /** What a read of folder `key` answered, into the tree. */
  const take = useCallback((key: string, view: FilesView, cursor: string | null, quiet: boolean) => {
    if (view.kind === 'folder') {
      const entries = key === ROOTS ? view.entries.filter((e) => e !== wholeOf(view.entries)) : view.entries
      if (key === ROOTS) setWhole(wholeOf(view.entries) ?? null)
      const groups = groupEntries(entries, key)
      setDirs((all) => {
        const next = new Map(all)
        for (const [folder, rows] of groups) {
          const had = next.get(folder)
          next.set(folder, folder === key && cursor !== null && Array.isArray(had) ? [...had, ...rows] : rows)
        }
        return next
      })
      setNexts((m) => ({ ...m, [key]: view.next }))
      setMeta((m) => ({ ...m, [key]: { reason: view.reason, stale: view.stale || null, moreError: null } }))
      return
    }
    const stale: Stale = view.kind === 'refused' ? view.stale : { at: null, code: 'InvalidInput', message: t('filesNotFolder', { path: key }) }
    if (cursor !== null) { setMeta((m) => ({ ...m, [key]: { ...(m[key] ?? { reason: null, stale: null }), moreError: stale } })); return }
    // a reread that was refused keeps the folder it had, marked
    if (quiet && Array.isArray(read.current.get(key))) { setMeta((m) => ({ ...m, [key]: { ...(m[key] ?? { reason: null, moreError: null }), stale } })); return }
    setDirs((all) => new Map(all).set(key, refusalError(stale)))
  }, [t])

  /**
   * Read folder `key` (a path, or {@link ROOTS}): its first page, or with `cursor` the page there.
   * @param quiet - a first page that keeps what is on screen until it answers, and when it is refused.
   */
  const load = useCallback((asked: string, view: FilesViewId, cursor: string | null = null, quiet = false) => {
    const key = readKey(asked)
    controllers.current.get(key)?.abort()
    const controller = new AbortController()
    controllers.current.set(key, controller)
    setPending((p) => ({ ...p, [key]: { since: Date.now(), more: cursor !== null, view } }))
    if (cursor === null && !quiet) setDirs((all) => new Map(all).set(key, 'loading'))
    else if (cursor !== null) setMeta((m) => (m[key]?.moreError ? { ...m, [key]: { ...m[key]!, moreError: null } } : m))
    reader.current(route, { path: key === ROOTS ? null : key, cursor, depth: key === ROOTS ? undefined : 1 }, controller.signal).then(
      (answer) => { if (settle(key, controller)) take(key, answer, cursor, quiet) },
      (error: unknown) => {
        if (!settle(key, controller) || isDropped(error)) return
        take(key, { kind: 'refused', path: key === ROOTS ? null : key, observedAt: Date.now(), stale: { at: null, code: errorCode(error) ?? 'Unknown', message: errorText(error) } }, cursor, quiet)
      },
    )
  }, [route, settle, take, reader])

  /**
   * Stop waiting for folder `key`'s read: a read still queued in the page is
   * never sent; one already sent is dropped on the host too.
   * @returns whether a first page was being read (the folder has nothing to show).
   */
  const drop = useCallback((asked: string): boolean => {
    const key = readKey(asked)
    const controller = controllers.current.get(key)
    if (!controller) return false
    controllers.current.delete(key)
    controller.abort()
    setPending((p) => { const { [key]: _gone, ...rest } = p; return rest })
    const first = read.current.get(key) === 'loading'
    if (first) setDirs((all) => { const next = new Map(all); if (next.get(key) === 'loading') next.delete(key); return next })
    return first
  }, [])

  /** Put a folder answer read elsewhere (Go to path) into the tree. */
  const put = useCallback((key: string, view: FilesView) => take(key, view, null, false), [take])
  const fail = useCallback((key: string, stale: Stale) => setDirs((all) => new Map(all).set(key, refusalError(stale))), [])

  return { api, route, queue, dirs, nexts, meta, pending, whole, load, drop, put, fail, controllers }
}

/** Re-render every second while `on`. */
function useNow(on: boolean): number {
  const [now, setNow] = useState(() => Date.now())
  useEffect(() => {
    if (!on) return
    setNow(Date.now())
    const timer = setInterval(() => setNow(Date.now()), 500)
    return () => clearInterval(timer)
  }, [on])
  return on ? now : Date.now()
}

/** Whether `element` is narrower than {@link NARROW_PX}. */
function useNarrow(): [(element: HTMLDivElement | null) => void, boolean] {
  const [narrow, setNarrow] = useState(false)
  const observer = useRef<ResizeObserver | null>(null)
  const ref = useCallback((element: HTMLDivElement | null) => {
    observer.current?.disconnect()
    observer.current = null
    if (!element) return
    setNarrow(element.clientWidth > 0 && element.clientWidth < NARROW_PX)
    if (typeof ResizeObserver === 'undefined') return
    observer.current = new ResizeObserver((entries) => { const width = entries[0]?.contentRect.width ?? 0; if (width > 0) setNarrow(width < NARROW_PX) })
    observer.current.observe(element)
  }, [])
  useEffect(() => () => observer.current?.disconnect(), [])
  return [ref, narrow]
}

export function InspectFiles({ kind, name, label, workspaces, project, onOpenLog, onOpenSandboxLog, logTab, unavailableAction }: InspectFilesProps): ReactNode {
  const { t, tw, api } = useMachinesContext()
  const ft = t as unknown as FilesTranslate
  // a path under a Workspace's mount is read from its saved files (LayerFS lists no mount); every other path from the files route
  const sourceOf = (path: string | null | undefined): WorkspaceSource | null => {
    if (!path || !project) return null
    const ws = (workspaces ?? []).find((w) => !!w.base && !!w.path && isUnder(path, w.path))
    return ws ? { root: ws.path, project, branch: ws.branch, base: ws.base! } : null
  }
  const reader = useRef<Reader>(() => Promise.reject(new Error('unready')))
  reader.current = (route, ask, signal) => {
    const source = sourceOf(ask.path)
    return source ? readWorkspace(api, queue, source, ask, signal) : readFiles(api, queue, route, ask, signal)
  }
  const folders = useInspectFolders(kind, name, reader)
  const { dirs, nexts, meta, pending, whole, load, drop, put, fail, queue, route } = folders
  const [frame, narrow] = useNarrow()

  // ---------- where the tab is: the view, and per view its folder, opened folders and file ----------
  const [place] = useState<FilesPlace>(() => readFilesPlace(kind, name) ?? { view: 'layerfs', path: null })
  const [view, setView] = useState<FilesViewId>(place.view)
  const [views, setViews] = useState<Record<FilesViewId, ViewState>>({ layerfs: EMPTY_VIEW, sandbox: EMPTY_VIEW, whole: EMPTY_VIEW })
  const [notice, setNotice] = useState<Partial<Record<FilesViewId, string>>>({})
  const [going, setGoing] = useState<{ view: FilesViewId, path: string, since: number } | null>(null)
  const [typed, setTyped] = useState<string | null>(null)
  const [pathProblem, setPathProblem] = useState<string | null>(null)
  const [filter, setFilter] = useState('')
  const [fileTick, setFileTick] = useState(0)
  // A machine: LayerFS folders, and Whole filesystem when the roots answer offers `/` (until it answers, the roots themselves).
  // A Sandbox: its Workspaces, its other folders (home, daemon log), and Whole filesystem when offered.
  const offered = (w: FileEntry | null | undefined): FilesViewId[] => ['layerfs', ...(kind === 'sandbox' ? ['sandbox' as const] : []), ...(w ? ['whole' as const] : [])]
  const shown: FilesViewId = offered(whole).includes(view) ? view : 'layerfs'
  const vs = views[shown]
  const rootKey = shown === 'whole' ? '/' : shown === 'sandbox' ? SB_ROOTS : kind === 'sandbox' ? WS_ROOTS : ROOTS
  const top = vs.base ?? rootKey
  const viewName = (id: FilesViewId): string => t(id === 'whole' ? 'filesViewWhole' : id === 'sandbox' ? 'filesViewSandbox' : kind === 'sandbox' ? 'filesViewWorkspaces' : 'filesViewLayerfs')
  // where a LayerFS folder met under `/` opens: a machine's among its LayerFS folders, a Sandbox's home among its folders
  const roleView: FilesViewId = kind === 'sandbox' ? 'sandbox' : 'layerfs'
  // a Sandbox's two views are drawn from its one roots answer
  const treeDirs = useMemo(() => {
    const r = dirs.get(ROOTS)
    if (kind !== 'sandbox' || r === undefined) return dirs
    const next = new Map(dirs)
    next.set(WS_ROOTS, Array.isArray(r) ? r.filter((e) => isWorkspaceEntry(e.entry)) : r)
    next.set(SB_ROOTS, Array.isArray(r) ? r.filter((e) => !isWorkspaceEntry(e.entry)) : r)
    return next
  }, [dirs, kind])
  const patch = useCallback((id: FilesViewId, change: Partial<ViewState>) => setViews((all) => ({ ...all, [id]: { ...all[id], ...change } })), [])

  // the file window Go to path already read, handed to the viewer once
  const probed = useRef<{ path: string, view: FilesView } | null>(null)
  const goFlight = useRef<AbortController | null>(null)
  useEffect(() => () => goFlight.current?.abort(), [])

  /**
   * Show `path` in view `id`: a folder becomes the tree's top, a file opens
   * with its folder as the top; null is the view's root. A path that cannot
   * be opened leaves the view at its root and says why.
   */
  const goTo = useCallback((id: FilesViewId, path: string | null) => {
    goFlight.current?.abort()
    setTyped(null)
    setPathProblem(null)
    if (path === null) {
      setGoing(null)
      setNotice((n) => ({ ...n, [id]: undefined }))
      patch(id, { base: null, file: null })
      return
    }
    const controller = new AbortController()
    goFlight.current = controller
    setGoing({ view: id, path, since: Date.now() })
    const done = () => { if (goFlight.current === controller) { goFlight.current = null; setGoing(null) } }
    const fallback = (code: string, message: string) => {
      setNotice((n) => ({ ...n, [id]: t('filesFallback', { path, code, message, root: id === 'whole' ? '/' : viewName(id) }) }))
      patch(id, { base: null, file: null })
    }
    reader.current(route, { path, depth: 1 }, controller.signal).then(
      (answer) => {
        if (controller.signal.aborted) return
        done()
        // a refusal, or a kept answer of a folder that can no longer be read: not there to show
        if (answer.kind === 'refused' || answer.stale) { fallback(answer.stale ? answer.stale.code : 'Unknown', answer.stale ? answer.stale.message : ''); return }
        setNotice((n) => ({ ...n, [id]: undefined }))
        if (answer.kind === 'folder') {
          put(path, answer)
          patch(id, { base: path, open: new Set(), file: null })
        } else {
          probed.current = { path, view: answer }
          patch(id, { base: parentOf(path), open: new Set(), file: path })
        }
      },
      (error: unknown) => { if (isDropped(error) || controller.signal.aborted) return; done(); fallback(errorCode(error) ?? 'Unknown', errorText(error)) },
    )
  }, [route, patch, put, t, whole])

  // the roots first: they say which views there are; then the place this tab was left at, or a link named
  useEffect(() => { load(ROOTS, 'layerfs') }, [load])
  const restored = useRef(false)
  useEffect(() => {
    if (restored.current || whole === undefined) return
    restored.current = true
    const at = readFilesPlace(kind, name) ?? place
    const id: FilesViewId = offered(whole).includes(at.view) ? at.view : 'layerfs'
    setView(id)
    if (at.path) goTo(id, at.path)
  }, [whole, kind, name, place, goTo])
  // a link from elsewhere while the tab is open ("Show in Files")
  useEffect(() => watchFilesPlace(kind, name, (to) => {
    const id: FilesViewId = offered(whole).includes(to.view) ? to.view : 'layerfs'
    setView(id)
    goTo(id, to.path)
  }), [kind, name, goTo, whole])
  // remember the view and the path shown (the file, else the folder at the top)
  useEffect(() => {
    if (!restored.current || going) return
    writeFilesPlace(kind, name, { view: shown, path: vs.file ?? vs.base })
  }, [kind, name, shown, vs.file, vs.base, going])

  // every folder the view shows is read: its top and the folders open under it (a read dropped with its view starts over here)
  useEffect(() => {
    for (const key of [top, ...(narrow ? [] : vs.open)].map(readKey)) if (dirs.get(key) === undefined && !pending[key]) load(key, shown)
  }, [shown, top, vs.open, narrow, dirs, pending, load])

  const switchView = (id: FilesViewId) => {
    if (id === shown) return
    // reads of the view being left are dropped; its folders are read again when it is shown
    for (const [key, p] of Object.entries(pending)) if (p.view === shown && key !== ROOTS) drop(key)
    goFlight.current?.abort()
    setGoing(null)
    setTyped(null)
    setPathProblem(null)
    setFilter('')
    setView(id)
  }

  const toggle = (path: string) => {
    const open = new Set(vs.open)
    if (open.has(path)) {
      open.delete(path)
      // nobody waits for its read any more: queued, it is never sent
      drop(path)
    } else open.add(path)
    patch(shown, { open })
  }
  const stopRead = (key: string) => {
    const first = drop(key)
    if (!first) return
    if (key === top) fail(key, { at: null, code: 'Cancelled', message: t('filesCancelled') })
    else { const open = new Set(vs.open); open.delete(key); patch(shown, { open }) }
  }
  const refresh = () => {
    setNotice((n) => ({ ...n, [shown]: undefined }))
    load(ROOTS, 'layerfs', null, Array.isArray(dirs.get(ROOTS)))
    for (const key of new Set([top, ...(narrow ? [] : vs.open)].map(readKey))) if (key !== ROOTS) load(key, shown, null, Array.isArray(dirs.get(key)))
    setFileTick((n) => n + 1)
  }

  // ---------- rows ----------
  const wsOf = (entry: FileEntry): WorkspaceInfo | undefined => (workspaces ?? []).find((w) => w.path === entry.path) ?? (entry.workspace ? (workspaces ?? []).find((w) => w.wsId === entry.workspace || w.ws === entry.workspace) : undefined)
  const roots = dirs.get(ROOTS)
  // a log (role `log`) is read with the logs calls, never here: a Sandbox's own daemon log and a machine's worker log
  // (worker.log and its rotated part) in this page's log tab; a Sandbox's daemon log met on its machine in that Sandbox's tab
  const ownLog = (entry: FileEntry): boolean => kind === 'sandbox' || isWorkerLog(entry.path)
  const logOpener = (entry: FileEntry): (() => void) | null => {
    if (entry.role !== 'log') return null
    if (ownLog(entry)) return onOpenLog ?? null
    const folder = parentOf(entry.path)
    return folder && onOpenSandboxLog ? onOpenSandboxLog(baseName(folder)) : null
  }
  /** A role's label with the real path as the second text; a role without a label shows its path. */
  const roleName = (entry: FileEntry): { label?: string, secondary?: string } => {
    const role = entry.role
    if (!role) return {}
    if (role === 'log') return isWorkerLog(entry.path) ? { label: t('roleWorkerLog'), secondary: entry.path } : baseName(entry.path) === 'daemon.log' ? { label: t('roleLog'), secondary: entry.path } : { label: entry.path }
    return ROLE[role] ? { label: t(ROLE[role]), secondary: entry.path } : { label: entry.path }
  }
  const tip = (entry: FileEntry, words?: string): string => [entry.path, words, entry.size != null && entry.kind !== 'folder' ? bytes(entry.size) : undefined, when(entry.mtime)].filter(Boolean).join(' · ')

  const workspaceLabel = (entry: FileEntry): string => { const ws = wsOf(entry); return t('roleWorkspace', { name: ws ? workspaceName(ws) : baseName(entry.path) }) }
  const row = (item: InspectEntry): Partial<TreeRow> => {
    const entry = item.entry
    const act = actOf(entry.kind)
    const role = entry.role
    const named = roleName(entry)
    // a Workspace on a machine page: a mount, not entered (the machine page does not know its project)
    if (isWorkspaceEntry(entry) && kind !== 'sandbox') {
      return {
        expandable: false, disabled: true, icon: <SandboxGlyph size={15} />, label: workspaceLabel(entry), secondary: entry.path,
        title: tip(entry, entry.fs ? t('kindMount', { fs: entry.fs }) : t('kindMountPlain')),
      }
    }
    // a Workspace on its Sandbox's page: a folder of its saved files (its Branch at its base Commit), opened in place
    if (isWorkspaceEntry(entry)) {
      const ws = wsOf(entry)
      const source = sourceOf(entry.path)
      const changes = ws && (ws.pendingMerge || ws.dirty != null) ? workspaceChanges(ws) : null
      const words = source
        ? t(changes?.tone === 'warning' || ws?.pendingMerge ? 'filesWsSavedDirty' : 'filesWsSaved', { branch: source.branch, id: shortId(source.base) })
        : t(workspaces == null ? 'filesWsWaiting' : 'filesWsNoSource')
      return {
        expandable: !!source && !narrow, disabled: !source, icon: <SandboxGlyph size={15} />,
        label: workspaceLabel(entry), secondary: ws?.branch ?? entry.path,
        title: tip(entry, words),
        trailing: changes ? <span className={css.chip} data-tone={changes.tone} data-files-changes>{tw(changes)}</span> : undefined,
      }
    }
    // the daemon or worker log: read in its own tab, by cursor
    if (role === 'log') {
      const opens = logOpener(entry) !== null
      const where = ownLog(entry) ? logTab : t('tabDaemonLog')
      return {
        ...named, expandable: false, disabled: !opens, icon: <FileTypeIcon path="daemon.log" size={16} />,
        title: tip(entry, ownLog(entry) ? (logTab ? t('filesLogHere', { tab: logTab }) : t('filesLogNoTab')) : t('filesLogInSandbox')),
        trailing: opens && where ? <span className={css.hint}>{where}</span> : undefined,
      }
    }
    // a LayerFS folder listed where it is not entered (met under `/`, or on a mount of its own): it opens by its own path among the LayerFS folders
    if (isLinkedRole(entry, shown === 'whole' ? 'whole' : 'layerfs')) {
      return {
        ...named, expandable: false, icon: <IconFolderCloseRegular size={16} />, title: tip(entry, t('filesOpensInLayerfs', { view: viewName(roleView) })),
        trailing: <span className={css.hint} data-files-linked>{viewName(roleView)}</span>,
      }
    }
    const lock = <span className={css.lock} aria-hidden="true"><LockGlyph size={13} /></span>
    if (act === 'restricted') return { ...named, expandable: false, disabled: true, title: tip(entry, t('kindRestricted')), trailing: lock }
    if (act === 'unreadable') return { ...named, expandable: false, disabled: true, title: tip(entry, t('kindUnreadable')), trailing: lock }
    if (act === 'system') return { ...named, expandable: false, disabled: true, icon: <SystemGlyph size={15} />, title: tip(entry, t('kindSystem')) }
    if (act === 'mount') return { ...named, expandable: false, disabled: true, icon: <MountGlyph size={15} />, title: tip(entry, entry.fs ? t('kindMount', { fs: entry.fs }) : t('kindMountPlain')), trailing: entry.fs ? <span className={css.hint}>{entry.fs}</span> : undefined }
    if (act === 'symlink') return { ...named, expandable: false, disabled: true, secondary: entry.target ? `→ ${entry.target}` : named.secondary, title: tip(entry, t('kindSymlink', { target: entry.target ?? '?' })) }
    if (act === 'other') return { ...named, expandable: false, disabled: true, title: tip(entry, t('kindOther')) }
    if (act === 'open') {
      const free = typeof entry.free_bytes === 'number' ? <span className={css.hint}>{t('filesFree', { size: bytes(entry.free_bytes) })}</span> : undefined
      // narrow: a folder is entered, not expanded in place
      return { ...named, title: tip(entry), trailing: free, ...(narrow ? { expandable: false, icon: <IconFolderCloseRegular size={16} /> } : {}) }
    }
    return { ...named, title: tip(entry) }
  }

  const openEntry = (item: InspectEntry) => {
    const entry = item.entry
    if (entry.role === 'log') { logOpener(entry)?.(); return }
    // narrow: a Workspace is entered like a folder
    if (isWorkspaceEntry(entry)) { if (sourceOf(entry.path)) { patch(shown, { base: entry.path, file: null }); setFilter('') } return }
    if (isLinkedRole(entry, shown === 'whole' ? 'whole' : 'layerfs')) { switchView(roleView); goTo(roleView, entry.path); return }
    const act = actOf(entry.kind)
    if (act === 'open') { patch(shown, { base: entry.path, file: null }); setFilter(''); return }
    if (act === 'read') { patch(shown, { file: entry.path }); setTyped(null) }
  }

  // ---------- reads in progress, refusals, pages ----------
  const now = useNow(Object.keys(pending).length > 0 || going !== null)
  const waiting = (since: number, onCancel: () => void): ReactNode => {
    const s = waitedSeconds(since, now)
    return (
      <span className={css.wait} role="status">
        {s === null ? t('files_reading') : t('filesReadingFor', { s: String(s) })}
        {s !== null ? <LinkButton onClick={onCancel}>{t('filesCancel')}</LinkButton> : null}
      </span>
    )
  }
  const refusal = (stale: { code: string, message: string }, again: () => void): ReactNode => {
    if (stale.code === 'Unavailable' && unavailableAction) {
      return <div className={css.stoppedNotice} data-files-refusal={stale.code}>
        <strong>{t('filesMachineStopped', { name })}</strong>
        <span>{t('filesStartToRead')}</span>
        <div className={css.stoppedActions}>{unavailableAction}<LinkButton onClick={again}>{t('filesReadAgain')}</LinkButton></div>
        <details><summary>{t('filesErrorDetails')}</summary><p>{stale.code} · {stale.message}</p></details>
      </div>
    }
    return (
      <span className={css.refusal} data-files-refusal={stale.code}>
        <span className={css.code}>{stale.code}</span>
        <span>{stale.message}</span>
        <LinkButton onClick={again}>{t('filesReadAgain')}</LinkButton>
      </span>
    )
  }
  const pendingNote = (key: string): ReactNode => (pending[key] ? waiting(pending[key]!.since, () => stopRead(key)) : t('files_reading'))
  const failedNote = (key: string, error: Error): ReactNode => refusal({ code: errorCode(error) ?? 'Unknown', message: error.message }, () => load(key, shown))
  const moreOf = (key: string): TreeMore => {
    const m = meta[key], p = pending[key]
    if (p?.more) return { label: t('files_reading'), busy: true, note: waitedSeconds(p.since, now) !== null ? waiting(p.since, () => stopRead(key)) : undefined }
    if (m?.moreError) return { label: t('filesReadAgain'), note: <span className={css.refusal} data-files-refusal={m.moreError.code}><span className={css.code}>{m.moreError.code}</span><span>{m.moreError.message}</span></span> }
    return { label: m?.reason === 'deadline' ? t('filesDeadline') : t('filesMore') }
  }
  const afterNote = (key: string): ReactNode => {
    const stale = meta[key]?.stale
    if (!stale) return null
    return (
      <span className={css.refusal} data-files-stale={stale.code}>
        <span>{t('filesStale', { time: stale.at ? clockTime(stale.at) : '—', code: stale.code, message: stale.message })}</span>
        <LinkButton onClick={() => load(key, shown, null, true)}>{t('filesReadAgain')}</LinkButton>
        {stale.code === 'Unavailable' && unavailableAction ? unavailableAction : null}
      </span>
    )
  }

  // ---------- the path bar ----------
  const shownPath = vs.file ?? vs.base ?? (shown === 'whole' ? '/' : '')
  const submitPath = (e: FormEvent) => {
    e.preventDefault()
    const text = typed ?? shownPath
    if (!text.trim()) { goTo(shown, null); return }
    const path = pastedPath(text)
    if (!path) { setPathProblem(t('filesPathNotAbsolute')); return }
    setFilter('')
    if (path === '/' && shown === 'whole') goTo(shown, null)
    else goTo(shown, path)
  }
  // the folders from the view's root down to the one at the tree's top
  const crumbs = useMemo(() => {
    if (vs.base === null) return []
    if (shown === 'whole') return crumbsOf(vs.base, '/')
    const under = (Array.isArray(roots) ? roots : []).filter((r) => (actOf(r.entry.kind) === 'open' || isWorkspaceEntry(r.entry)) && isUnder(vs.base!, r.path)).sort((a, b) => b.path.length - a.path.length)
    const root = under[0]
    if (!root) return crumbsOf(vs.base, null)
    const role = root.entry.role
    const rootName = isWorkspaceEntry(root.entry) ? workspaceLabel(root.entry) : role && ROLE[role] ? t(ROLE[role]) : baseName(root.path)
    return [{ name: rootName, path: root.path }, ...crumbsOf(vs.base, root.path)]
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [vs.base, shown, roots, t, workspaces])

  // ---------- the file ----------
  const file = vs.file
  // one signal per file shown: made by the first read of it (the viewer's own
  // effect runs before this component's), ended when another file is shown
  const fileAbort = useRef<{ key: string, controller: AbortController } | null>(null)
  const fileKey = `${shown}:${file ?? ''}:${fileTick}`
  const fileSignal = useCallback((key: string): AbortSignal => {
    if (fileAbort.current?.key !== key) {
      fileAbort.current?.controller.abort()
      fileAbort.current = { key, controller: new AbortController() }
    }
    return fileAbort.current.controller.signal
  }, [])
  useEffect(() => {
    if (file === null && fileAbort.current) { fileAbort.current.controller.abort(); fileAbort.current = null }
  }, [file])
  useEffect(() => () => { fileAbort.current?.controller.abort(); fileAbort.current = null }, [])
  const readWindow = useCallback((offset?: number): Promise<TextWindow<number>> => {
    const path = file!
    const kept = offset === undefined && probed.current?.path === path ? probed.current.view : null
    probed.current = null
    const answer = kept ? Promise.resolve(kept) : reader.current(route, { path, offset }, fileSignal(fileKey))
    return answer.then((got) => {
      if (got.kind === 'file') return { text: got.content, binary: got.binary, size: got.size, next: got.next }
      if (got.kind === 'refused') throw refusalError(got.stale)
      throw new HostError('InvalidInput', t('filesIsFolder', { path }), 200)
    })
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [route, file, fileKey, fileSignal])

  const topDir = treeDirs.get(top)
  const switchId = `files-view-${kind}-${name.replace(/[^\w-]/g, '_')}`
  const busy = Object.keys(pending).length > 0 || going !== null
  const showTree = !(narrow && file)
  const showFile = !narrow || !!file
  const parent = vs.base === null ? null : crumbs.length > 1 ? crumbs[crumbs.length - 2]! : { name: viewName(shown), path: null as string | null }
  return (
    <div ref={frame} className={css.files} data-files={kind} data-files-view={shown} data-files-base={vs.base ?? ''} data-files-file={file ?? ''} data-files-narrow={narrow || undefined}
      data-files-state={going ? 'going' : topDir === undefined || topDir === 'loading' ? 'reading' : topDir instanceof Error ? 'refused' : 'ready'} aria-busy={busy || undefined}>
      <div className={css.bar}>
        {offered(whole).length > 1 ? (
          <SegmentedControl id={switchId} label={t('filesView')} value={shown}
            options={offered(whole).map((id) => ({ value: id, label: viewName(id) })) as [never, ...never[]]} onChange={switchView} />
        ) : null}
        <form className={css.pathForm} onSubmit={submitPath} data-files-pathbar>
          <Input className={css.pathInput} value={typed ?? shownPath} placeholder={t('filesPathPlaceholder')} aria-label={t('filesPath')} spellCheck={false} autoComplete="off" autoCapitalize="off"
            aria-invalid={pathProblem ? true : undefined} onChange={(e) => { setTyped(e.target.value); setPathProblem(null) }}
            onKeyDown={(e) => { if (e.key === 'Escape' && typed !== null) { e.preventDefault(); setTyped(null); setPathProblem(null) } }} />
          <Button type="submit" variant="outline" size="sm" disabled={going !== null}>{t('filesGo')}</Button>
        </form>
        <Button variant="outline" size="sm" icon={<IconRefreshOutlineRegular size={13} />} onClick={refresh}>{t('refresh')}</Button>
      </div>
      {pathProblem ? <p className={css.problem} role="alert">{pathProblem}</p> : null}
      {going ? <p className={css.goingLine} data-files-going>{waiting(going.since, () => { goFlight.current?.abort(); goFlight.current = null; setGoing(null) })} <span className={css.goingPath}>{going.path}</span></p> : null}
      {notice[shown] ? (
        <Notice tone="warning" className={css.notice} role="status">
          <p data-files-notice>{notice[shown]}</p>
          <button type="button" className={css.dismiss} aria-label={t('close')} onClick={() => setNotice((n) => ({ ...n, [shown]: undefined }))}><IconCloseOutlineRegular size={12} /></button>
        </Notice>
      ) : null}

      {/* the switch is a tab list (the DSH control): the frame is the panel of the view shown */}
      <div className={css.frame} data-narrow={narrow || undefined} {...(offered(whole).length > 1 ? { role: 'tabpanel', id: `${switchId}-${shown}-panel`, 'aria-labelledby': `${switchId}-${shown}` } : {})}>
        {showTree ? (
          <div className={css.side}>
            <div className={css.sideHead}>
              {vs.base !== null ? (
                <nav className={css.crumbs} aria-label={t('filesCrumbs')} data-files-crumbs>
                  {narrow && parent ? (
                    <Button size="sm" variant="outline" icon={<IconChevronLeftOutlineRegular size={13} />} onClick={() => patch(shown, { base: parent.path, file: null })}>{t('filesBackTo', { name: parent.name })}</Button>
                  ) : (
                    <>
                      <button type="button" className={css.crumb} onClick={() => goTo(shown, null)}>{viewName(shown)}</button>
                      {crumbs.map((c, i) => (
                        <span key={c.path} className={css.crumbPart}>
                          <span className={css.crumbSep} aria-hidden="true">/</span>
                          {i === crumbs.length - 1
                            ? <span className={css.crumbHere} aria-current="location" title={c.path}>{c.name}</span>
                            : <button type="button" className={css.crumb} title={c.path} onClick={() => patch(shown, { base: c.path, file: null })}>{c.name}</button>}
                        </span>
                      ))}
                    </>
                  )}
                </nav>
              ) : null}
              {narrow && vs.base !== null ? <p className={css.here} title={vs.base}>{vs.base}</p> : null}
              <Input className={css.filter} icon={<IconSearchOutlineRegular size={14} />} placeholder={t('filesFilter')} aria-label={t('filesFilter')}
                value={filter} onChange={(e) => setFilter(e.target.value)} onKeyDown={(e) => { if (e.key === 'Escape' && filter) { e.preventDefault(); setFilter('') } }} />
            </div>
            <div className={css.treeScroll} data-files-tree>
              <FileTree<InspectEntry, string> root={top} dirs={treeDirs} open={narrow ? NO_OPEN : vs.open} nexts={nexts} selected={file ?? undefined} filter={filter}
                row={row} onToggle={toggle} onOpen={openEntry} onMore={(path, cursor) => load(path, shown, cursor)}
                pending={pendingNote} failed={failedNote} more={moreOf} after={afterNote}
                labels={{ tree: label, reading: t('files_reading'), failed: t('files_folderFailed'), empty: top === WS_ROOTS ? t('filesNoWorkspaces') : t('files_emptyFolder'), more: t('filesMore'), noMatch: t('files_noMatch') }} />
            </div>
          </div>
        ) : null}
        {showFile ? (
          <section className={css.main} aria-label={file ?? t('filesOpenHint')}>
            {file ? (
              <>
                <div className={css.fileHead}>
                  {narrow ? <Button size="sm" variant="outline" icon={<IconChevronLeftOutlineRegular size={13} />} onClick={() => patch(shown, { file: null })}>{t('filesBackTo', { name: baseName(parentOf(file) ?? '/') })}</Button> : null}
                  <span className={css.fileName} title={file}><FileTypeIcon path={baseName(file)} size={14} />{narrow ? baseName(file) : file}</span>
                  {!narrow ? <button type="button" className={css.dismiss} aria-label={t('files_close', { name: file })} onClick={() => patch(shown, { file: null })}><IconCloseOutlineRegular size={12} /></button> : null}
                </div>
                <div className={css.viewer} data-files-viewer>
                  <WindowedText key={`${shown}:${file}:${fileTick}`} read={readWindow} path={file} t={ft}
                    words={{ binary: (size) => t('filesBinary', { n: String(size) }), empty: t('filesEmptyFile'), retry: t('filesReadAgain') }} />
                </div>
              </>
            ) : <EmptyState title={t('filesOpenHint')} icon={<FileTypeIcon kind="other" size={20} />}>{t('filesOpenHintBody')}</EmptyState>}
          </section>
        ) : null}
      </div>
    </div>
  )
}
