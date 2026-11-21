/**
 * Sandbox conversation view: where the session's work runs. The LayerFS mark
 * sits in a hub at the centre; branches are pills on an arc around it, agents
 * (whales, one hue each) wire into the hub from the left and sandboxes with
 * their workspaces sit on the right. Every call travels once: agent → hub →
 * branch (→ workspace), or hub → sandbox for a call without a branch; it lands
 * with a ripple, holds while it runs, and its result comes back once in green
 * or red. Positions are measured per layout; everything that moves is a pure
 * function of the replay clock (docs/specs/conversation-views.md §5).
 */
import { Fragment, memo, useCallback, useEffect, useId, useLayoutEffect, useMemo, useRef, useState, type ReactNode, type SVGProps } from 'react'
import clsx from 'clsx'
import {
  Button, Checkbox, IconBranchOutlineRegular, IconCheckOutlineRegular, IconChevronRightOutlineRegular, IconCloseOutlineRegular,
  IconPauseOutlineRegular, IconPlayOutlineRegular, IconRefreshOutlineRegular, IconUsersOutlineRegular, IconWarningOutlineRegular,
  SegmentedControl,
} from '@deepseek-ai/dsh-client-ui-primitives'
import { LOGS_EVENT, shellCommand, type LabeledLine, type LogsListAnswer, type LogsOutputAnswer } from '../../shared/logs.ts'
import { MachineGlyph, SandboxGlyph } from '../machines/icons.tsx'
import type { MachinesView } from '../machines/model.ts'
import { Gate, useNoComposer, type ViewProps } from './FileViews.tsx'
import { useRootState } from './hooks.ts'
import type { Translate } from './locale.ts'
import { useLocaleRevision } from '../ui/locale-sync.ts'
import { LayerfsMark, Whale } from './marks.tsx'
import { WorkspaceTerminal } from './WorkspaceTerminal.tsx'
import {
  RESULT_S, RIPPLE_S, TAIL_S, activeAt, agentTree, buildSandboxMap, stateWords, recentBranches, onlyBranches, wallAt, countEnded, easeOut, indexEvents, isQuietCall, lastEnded, mergeCalls, mountedAt, phaseOf, requestS, rippleAt, stamp, startedBy, windowAt,
  type LiveSandboxes, type MapAgent, type MapBinding, type MapEvent, type MapWorkspace, type SandboxMapModel,
} from './model.ts'
import css from './sandbox.module.css'
import { dateTime } from '../ui/locale-sync.ts'

/** Agent hues (`--lane-0` … in sandbox.module.css). */
const LANES = 6
const OK = 'var(--dsw-alias-state-success-primary)'
const BAD = 'var(--dsw-alias-state-error-primary)'
/** Calls that save a version on their branch. */

const ACTIVITY_KEY = 'dsh-layerfs:sandbox-activity'
const readFlag = (key: string) => { try { return window.localStorage.getItem(key) } catch { return null } }
const writeFlag = (key: string, value: string) => { try { window.localStorage.setItem(key, value) } catch { /* storage unavailable */ } }
/** Map lines an agent's first read gathers (state polls not counted). */
const CALL_LIMIT = 500
/** Pages of 1000 log lines read per agent at most while older calls fill in. */
const CALL_PAGES = 3
/** Pause between background reads of older pages. */
const BACKFILL_MS = 1200
/** Map lines kept per agent as refreshes add to them. */
const CALL_KEEP = 2000
/** How far before its last read an agent's refresh looks: a bash call runs up to 600 s and its line is dated at its start. */
const LATE_CALL_MS = 660_000
/** Wait after the log grows before reading it again: the log can grow every second during a team run. */
const REFRESH_MS = 4000
/** Agents whose newest page is read at the same time (the host's log reads are cheap since its tail cache, F15). */
const READERS = 3
const EVENT_ROWS = 50
/** Workspaces a Sandbox can hold mounted at once. */
const WS_SLOTS = 16
type Kind = 'agent' | 'project' | 'branch' | 'ws' | 'sandbox'
const node = (kind: Kind, id: string) => `${kind}:${id}`
const clamp = (v: number, lo: number, hi: number) => Math.max(lo, Math.min(hi, v))
const reducedMotion = () => typeof window !== 'undefined' && !!window.matchMedia?.('(prefers-reduced-motion: reduce)').matches

function Glyph({ size = 13, children, ...rest }: { size?: number, children: ReactNode } & SVGProps<SVGSVGElement>): ReactNode {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth={1.2} strokeLinecap="round" strokeLinejoin="round" aria-hidden {...rest}>
      {children}
    </svg>
  )
}
/** A command: a prompt chevron and a cursor. */
const PromptGlyph = ({ size }: { size?: number }) => <Glyph size={size}><path d="m3 4.5 3.5 3.5L3 11.5" /><path d="M8.5 11.5H13" /></Glyph>
/** A save on a branch. */
const CommitGlyph = ({ size }: { size?: number }) => <Glyph size={size}><circle cx="8" cy="8" r="2.6" /><path d="M1.5 8h3.9M10.6 8h3.9" /></Glyph>
/** A sandbox's machine: the platform's brand mark (Docker, Apple, Lima…) on a small tile. */
function MachineMark({ kind, label }: { kind: MachineKind, label: string }): ReactNode {
  return (
    <span className={css.machineMark} title={label} aria-label={label} role="img">
      {kind.platform === 'unknown' ? <SandboxGlyph size={13} /> : <MachineGlyph platform={kind.platform} os={kind.os} size={13} />}
    </span>
  )
}
/** A workspace: a window. */
const WorkspaceGlyph = ({ size }: { size?: number }) => <Glyph size={size}><rect x="2" y="3" width="12" height="10" rx="2" /><path d="M2 6.2h12" /></Glyph>

/** One page of an agent's map lines (state polls left out), newest first, and the cursor to older lines. */
async function readPage(api: ViewProps['api'], id: string, args: { since?: number, cursor?: string }): Promise<{ lines: LabeledLine[], older?: string }> {
  const answer = await api.op<LogsListAnswer>('logs.list', { tag: `dsh:${id}:`, limit: 1000, ...args })
  return { lines: answer.lines.filter((line) => !isQuietCall(line)), ...(answer.has_more && answer.next ? { older: answer.next } : {}) }
}

interface AgentCalls {
  lines: LabeledLine[]
  /** When the newest lines were read (ms), for the next refresh's `since`. */
  read: number
  /** Cursor to older lines not read yet, while there is more and the agent is under `CALL_LIMIT`. */
  older?: string
  pages: number
}

/**
 * Each agent's call log lines, loaded progressively: the newest page of every
 * agent first (the map shows as each lands), then older pages one at a time in
 * the background (`BACKFILL_MS` apart) until an agent has `CALL_LIMIT` map lines
 * or `CALL_PAGES` pages, since a conversation's own host polls can fill the
 * newest pages of its tag (F12). The log can grow every second during a team
 * run, so one read runs at a time and always lands: refreshes wait
 * `REFRESH_MS`, and those during a read become one more read after it. A known
 * agent's refresh asks for one page of lines since its last read (less
 * `LATE_CALL_MS`, as a line is written when its call ends but dated when it
 * started). A hidden page reads nothing until shown.
 */
function useAgentCalls(api: ViewProps['api'], agents: readonly MapAgent[]) {
  const ids = agents.map((a) => a.id).join('|')
  const [calls, setCalls] = useState<Record<string, LabeledLine[]>>({})
  const [loaded, setLoaded] = useState(false)
  const [backfilling, setBackfilling] = useState(false)
  const wanted = useRef(ids)
  wanted.current = ids
  const kick = useRef<() => void>(() => {})
  useEffect(() => {
    const store = new Map<string, AgentCalls>()
    let running = false, again = false, stopped = false, missed = false
    let timer: ReturnType<typeof setTimeout> | undefined
    const later = (ms = REFRESH_MS) => { if (!timer) timer = setTimeout(() => { timer = undefined; void read() }, ms) }
    const pending = (list: readonly string[]) => list.find((id) => { const a = store.get(id); return !!a?.older && a.pages < CALL_PAGES && a.lines.length < CALL_LIMIT })
    const read = async () => {
      if (stopped) return
      // A hidden page reads nothing (each read costs the host a scan of the log); it catches up when shown.
      if (document.visibilityState === 'hidden') { missed = true; return }
      if (running) { again = true; return }
      running = true
      again = false
      const started = Date.now()
      const list = wanted.current ? wanted.current.split('|') : []
      let changed = false
      const show = () => { changed = false; setCalls(Object.fromEntries(list.map((id) => [id, store.get(id)?.lines ?? []]))) }
      // Newest lines first, `READERS` agents at a time.
      const queue = [...list]
      const reader = async () => { for (let id = queue.shift(); id !== undefined && !stopped; id = queue.shift()) {
        const had = store.get(id)
        try {
          if (had) {
            const page = await readPage(api, id, { since: had.read - LATE_CALL_MS })
            const lines = mergeCalls(had.lines, page.lines, CALL_KEEP)
            if (lines !== had.lines) changed = true
            store.set(id, { ...had, lines, read: started })
          } else {
            const page = await readPage(api, id, {})
            store.set(id, { lines: page.lines, read: started, pages: 1, ...(page.older ? { older: page.older } : {}) })
            if (!stopped) show()   // a first page shows at once
          }
        } catch {
          // Kept as it was; an agent never read is read next time.
        }
      } }
      await Promise.all(Array.from({ length: READERS }, reader))
      // Then one older page of one agent per read, until none is left.
      const back = stopped ? undefined : pending(list)
      if (back) {
        const had = store.get(back)!
        try {
          const page = await readPage(api, back, { cursor: had.older })
          const lines = mergeCalls(had.lines, page.lines, CALL_KEEP)
          if (lines !== had.lines) changed = true
          store.set(back, { ...had, lines, pages: had.pages + 1, older: page.older })
        } catch {
          store.set(back, { ...had, older: undefined })   // the log rotated under the cursor: keep what we have
        }
      }
      running = false
      if (stopped) return
      for (const id of [...store.keys()]) if (!list.includes(id)) { store.delete(id); changed = true }
      // A read that added no map lines draws nothing again (the map rebuilds from every agent's lines).
      if (changed) show()
      setLoaded(true)
      const more = !!pending(list)
      setBackfilling(more)
      if (again) later()
      else if (more) later(BACKFILL_MS)
    }
    kick.current = () => { void read() }
    const shown = () => { if (document.visibilityState === 'visible' && missed) { missed = false; void read() } }
    document.addEventListener('visibilitychange', shown)
    const offs = [api.onEvent(LOGS_EVENT, () => later()), api.onEvent('layerfs-checkpoint', () => later()), api.onEvent('connection', () => later())]
    return () => { stopped = true; kick.current = () => {}; document.removeEventListener('visibilitychange', shown); offs.forEach((off) => off()); if (timer) clearTimeout(timer) }
  }, [api])
  useEffect(() => { kick.current() }, [api, ids])
  return { calls, loaded, backfilling }
}

/** A machine's kind for its mark: from the Machines list when it is readable, else guessed from the name. */
export interface MachineKind { platform: string, os?: 'mac' | 'linux' | 'pc' }
const PLATFORM_LABEL: Record<string, string> = { native: 'platformNative', docker: 'platformDocker', lima: 'platformLima', 'apple-container': 'platformAppleContainer', microsandbox: 'platformMicrosandbox' }
function guessKind(name: string): MachineKind {
  if (name === 'native') return { platform: 'native' }
  const n = name.toLowerCase()
  for (const platform of ['docker', 'lima', 'apple-container', 'microsandbox']) if (n.includes(platform.split('-')[0])) return { platform }
  return { platform: 'unknown' }
}
/** The kinds of the machines the map names, read once per set of machines. */
function useMachineKinds(api: ViewProps['api'], names: readonly string[]) {
  const key = [...new Set(names)].sort().join('|')
  const [known, setKnown] = useState<Record<string, MachineKind>>({})
  useEffect(() => {
    if (!key) return
    let live = true
    Promise.resolve().then(() => api.call<MachinesView>('GET', '/api/machines')).then((view) => {
      if (!live || !view || !('machines' in view) || !Array.isArray(view.machines)) return
      setKnown(Object.fromEntries(view.machines.map((m) => [m.name, {
        platform: m.platform, os: m.backend === 'namespaces' ? 'linux' : m.backend == null && m.platform === 'native' ? 'pc' : undefined,
      } satisfies MachineKind])))
    }, () => {})
    return () => { live = false }
  }, [api, key])
  return useCallback((name: string | undefined): MachineKind => name ? known[name] ?? guessKind(name) : { platform: 'unknown' }, [known])
}

/** Replay clock renders per second at most. */
const CLOCK_FPS = 30
/** Wall-clock now, re-read every `everyMs` while `on`. */
function useWallClock(on: boolean, everyMs: number): number {
  const [now, setNow] = useState(() => Date.now())
  useEffect(() => {
    if (!on) return
    setNow(Date.now())
    const timer = setInterval(() => setNow(Date.now()), everyMs)
    return () => clearInterval(timer)
  }, [on, everyMs])
  return now
}

/** Replay speeds offered beside Replay (replay seconds per real second). */
const REPLAY_SPEEDS = ['1', '10', '30', '100'] as const
type ReplaySpeed = typeof REPLAY_SPEEDS[number]

/** Seconds Live may trail the end while it plays through new calls. */
const LIVE_LAG_S = 4

/**
 * Replay clock: play/pause, scrub, Replay, Live. Live follows the end: when
 * new calls are appended the playhead plays through them at real speed.
 */
function useClock(duration: number, startLive: boolean) {
  const [t, setT] = useState(duration)
  // Replay seconds per real second while replaying; Live always follows the end at 1×.
  const [speed, setSpeed] = useState<ReplaySpeed>('1')
  const [playing, setPlaying] = useState(startLive && !reducedMotion())
  const [live, setLive] = useState(startLive)
  const durationRef = useRef(duration)
  durationRef.current = duration
  const started = useRef(false)
  // Untouched (no pause, scrub or replay yet), a paused playhead stays at the end as older calls fill in.
  const pinned = useRef(true)
  // Open at the end; once the user moves it, keep a paused playhead where it is.
  useEffect(() => { if ((!started.current || pinned.current) && duration > 0) { started.current = true; setT(duration) } }, [duration])
  const atEnd = t >= duration
  useEffect(() => {
    // Live at the end has nothing to draw until the log grows (`duration` changes).
    if (!playing || (live && atEnd)) return
    let raf = 0, last = 0
    const frame = (now: number) => {
      // At most `CLOCK_FPS` renders a second: the whole map redraws on each. Real elapsed time, so a slow
      // frame does not play the replay slow.
      if (last && now - last < 1000 / CLOCK_FPS - 2) { raf = requestAnimationFrame(frame); return }
      const dt = last ? Math.min(1, (now - last) / 1000) : 0
      last = now
      setT((value) => {
        const end = durationRef.current
        // Live never trails the end by more than `LIVE_LAG_S`, however fast the log grows.
        const next = Math.min(end, live ? Math.max(value + dt, end - LIVE_LAG_S) : value + dt * Number(speed))
        if (next >= end && !live) setPlaying(false)
        return next
      })
      raf = requestAnimationFrame(frame)
    }
    raf = requestAnimationFrame(frame)
    return () => cancelAnimationFrame(raf)
  }, [playing, live, atEnd, speed])
  return {
    t, playing, live, speed,
    setSpeed,
    toggle: () => {
      pinned.current = false
      if (playing) { setPlaying(false); setLive(false); return }
      if (t >= durationRef.current) setT(0)
      setPlaying(true)
    },
    scrub: (value: number) => { pinned.current = false; setLive(false); setPlaying(false); setT(value) },
    goLive: () => { setLive(true); setT(durationRef.current); setPlaying(!reducedMotion()) },
    replay: () => { pinned.current = false; setLive(false); setT(0); setPlaying(true) },
  }
}

type Focus = { kind: Kind | 'event', id: string } | null

export function SandboxView(props: ViewProps): ReactNode {
  const { api, sessionId, sessions } = props
  // B2 #6: the translator is a stable function, so the memoized rows (Activity's agent and turn words) take a new one on a locale switch
  const revision = useLocaleRevision()
  const t = useMemo<Translate>(() => (key, params) => props.t(key, params), [props.t, revision])
  useNoComposer(props.hideComposer)
  const state = useRootState(api, sessionId, sessions)
  return (
    <Gate state={state} t={t}>
      {(view, project) => <ChatSandboxMap {...props} t={t} project={project} binding={view.binding} root={state.root} byId={state.byId} title={state.title} label={state.label} />}
    </Gate>
  )
}

function ChatSandboxMap({ api, t, sessionId, project, binding, root, byId, title, label }: ViewProps & {
  project: string
  binding: MapBinding
  root: string
  byId: ReturnType<typeof useRootState>['byId']
  title: (id: string) => string | undefined
  label: (id: string) => string | undefined
}): ReactNode {
  const agents = useMemo(() => agentTree(root, byId), [root, byId])
  const { calls, loaded, backfilling } = useAgentCalls(api, agents)
  const name = useCallback((agent: MapAgent) => agent.depth === 0 ? t('mainAgent')
    : label(agent.id) ?? title(agent.id) ?? t('subagent', { n: agents.indexOf(agent) }), [label, title, t, agents])
  const groupTitle = useCallback((head: MapAgent) => title(head.id) ?? t('lead'), [title, t])
  return (
    <SandboxMap api={api} t={t} project={project} agents={agents} calls={calls} loaded={loaded} binding={binding}
      sessionId={sessionId} startLive={agents.some((a) => a.running)} name={name} groupTitle={groupTitle}
      note={backfilling ? t('olderLoading') : undefined} />
  )
}

/** What the map draws, from whichever scope (one conversation, or one project). */
export interface SandboxMapProps {
  api: ViewProps['api']
  t: Translate
  project: string
  /** Agent trees in order: each depth-0 agent heads a group of the Agents column. */
  agents: MapAgent[]
  calls: Readonly<Record<string, readonly LabeledLine[]>>
  loaded: boolean
  /** A conversation's binding (its branch and workspace stay on the map). */
  binding: MapBinding | null
  /** The Sandboxes open now, when known. */
  live?: LiveSandboxes
  /** The conversation this map is opened from (tags "This chat", offers the filter). */
  sessionId?: string
  startLive: boolean
  name: (agent: MapAgent) => string
  /** The title of a group, from its head agent. */
  groupTitle: (head: MapAgent) => string
  /** A Sandbox's state when the page knows it better than the calls. */
  boxState?: (name: string) => { text: string, live: boolean, bad?: boolean } | undefined
  /** Controls beside a Sandbox (Close). */
  boxActions?: (name: string) => ReactNode
  /** A line under the toolbar (what the calls shown cover). */
  note?: ReactNode
  /** The inspector's copy while there are no calls. */
  emptyCopy?: { title: string, body: string }
}

/**
 * The map's frame from its width and branch count: the side columns, the hub
 * radius, the arc the branch pills sit on and how wide a pill may grow. A
 * pure function, so the render and the measuring pass agree.
 */
function frameOf(W: number, n: number, wide = false) {
  const pad = 16, top = 40, bottom = 34
  const narrow = W < 1000
  const leftW = clamp(Math.round(W * 0.25), 220, 300)
  // A workspace terminal unfolded widens the Sandboxes column for its output.
  const rightW = wide ? clamp(Math.round(W * 0.34), 260, 440) : clamp(Math.round(W * 0.25), 220, 320)
  const cz0 = pad + leftW, cz1 = W - pad - rightW, czW = cz1 - cz0
  const rh = czW < 470 ? 36 : 46
  const step = n <= 1 ? 0 : n <= 6 ? 46 : n <= 9 ? 36 : narrow ? 28 : 30
  const half = (n - 1) / 2 * step
  const leftRoom = narrow ? 48 : 58, rightRoom = narrow ? 36 : 46
  // The pills sit on a circle around the hub; with many branches it flattens
  // into an ellipse (as tall, narrower), so the pills keep room for their names.
  const Ry = Math.max(rh + (narrow ? 72 : 84), half / Math.sin((narrow ? 72 : 66) * Math.PI / 180))
  const Rx = Math.min(Ry, Math.max(rh + 60, czW - leftRoom - rightRoom - rh - 130))
  const pillCap = clamp(czW - leftRoom - rightRoom - rh - Rx, 84, 172)
  return { pad, top, bottom, narrow, leftW, rightW, cz0, cz1, czW, rh, step, half, Rx, Ry, leftRoom, rightRoom, pillCap }
}

interface Rect { x: number, y: number, w: number, h: number }
/** Where everything sits, measured once per geometry (never from the clock). */
interface Layout {
  W: number, H: number, hx: number, hy: number, rh: number, Rx: number, Ry: number
  agentsTop: number, boxesTop: number, hubLabelLeft: number
  branch: Record<string, { x: number, y: number, left: number, right: number, ang: number, w: number }>
  /** An agent's wire start (row edge), its port on the hub, and its whale's centre. */
  agent: Record<string, { x: number, y: number, phi: number, px: number, py: number, cx: number, cy: number }>
  ws: Record<string, { x: number, y: number, r: Rect }>
  box: Record<string, { x: number, y: number, r: Rect }>
  /** Heights between pills where a branchless call leaves the hub. */
  gaps: number[]
  maxRight: number
}

const f1 = (v: number) => Math.round(v * 10) / 10
const curve = (x1: number, y1: number, x2: number, y2: number, k = 0.5) => {
  const d = Math.max(18, (x2 - x1) * k)
  return `C ${f1(x1 + d)} ${f1(y1)}, ${f1(x2 - d)} ${f1(y2)}, ${f1(x2)} ${f1(y2)}`
}
function agentWire(L: Layout, id: string): string | null {
  const A = L.agent[id]
  if (!A) return null
  const k = Math.max(26, (A.px - A.x) * 0.55)
  return `M ${f1(A.x)} ${f1(A.y)} C ${f1(A.x + k)} ${f1(A.y)}, ${f1(A.px + Math.cos(A.phi) * k * 0.7)} ${f1(A.py + Math.sin(A.phi) * k * 0.7)}, ${f1(A.px)} ${f1(A.py)}`
}
const hubEdge = (L: Layout, ang: number) => [L.hx + L.rh * Math.cos(ang), L.hy + L.rh * Math.sin(ang)] as const
/**
 * One continuous path per call: agent → into the hub → out along the branch
 * spoke (→ workspace), or out through the gap between pills to a sandbox for
 * a call without a branch.
 */
function chainOf(L: Layout, e: MapEvent): { d: string, target: string } | null {
  let d = agentWire(L, e.agent)
  if (!d) return null
  if (e.branch && L.branch[e.branch]) {
    const B = L.branch[e.branch]
    const [sx, sy] = hubEdge(L, B.ang)
    d += ` Q ${f1(L.hx)} ${f1(L.hy)}, ${f1(sx)} ${f1(sy)} L ${f1(B.left)} ${f1(B.y)}`
    const W = e.ws ? L.ws[e.ws] : undefined
    if (!W) return { d, target: node('branch', e.branch) }
    return { d: `${d} L ${f1(B.right)} ${f1(B.y)} ${curve(B.right, B.y, W.x, W.y)}`, target: node('ws', e.ws!) }
  }
  const box = e.sandbox ? L.box[e.sandbox] : undefined
  if (!box) return null
  const gy = [...L.gaps].filter((g) => Math.abs(g - L.hy) < L.rh - 6).sort((a, b) => Math.abs(a - box.y) - Math.abs(b - box.y))[0] ?? L.hy
  const gx = L.hx + Math.sqrt(Math.max(0, L.rh ** 2 - (gy - L.hy) ** 2))
  const run = L.maxRight + 14
  return { d: `${d} Q ${f1(L.hx)} ${f1(L.hy)}, ${f1(gx)} ${f1(gy)} L ${f1(run)} ${f1(gy)} ${curve(run, gy, box.x, box.y)}`, target: node('sandbox', e.sandbox!) }
}
const SVG_NS = 'http://www.w3.org/2000/svg'

export function SandboxMap({ api, t, project, agents, calls, loaded, binding, live, sessionId, startLive, name, groupTitle, boxState, boxActions, note, emptyCopy }: SandboxMapProps): ReactNode {
  const full = useMemo(() => buildSandboxMap(agents, binding, calls, live), [agents, binding, calls, live])
  const clock = useClock(full.duration, startLive)
  // Only the Branches active in the 5 minutes before the playhead (Live: before
  // now), with a Workspace mounted, or the session Branch. The set is keyed by
  // name so a playing replay rebuilds the map only when a Branch comes or goes.
  const wall = useWallClock(clock.live, 15_000)
  const atMs = clock.live ? wall : wallAt(full, clock.t)
  const untilS = clock.live ? Infinity : clock.t
  const sessionBranch = binding?.branch
  const branchKey = useMemo(() => [...recentBranches(full, atMs, sessionBranch ? [sessionBranch] : [], undefined, untilS)].sort().join('\n'), [full, atMs, untilS, sessionBranch])
  const model = useMemo(() => onlyBranches(full, new Set(branchKey ? branchKey.split('\n') : [])), [full, branchKey])
  const hiddenBranches = full.branches.length - model.branches.length
  const speedId = useId()
  const machineLabel = (kind: MachineKind, name: string | undefined) => [PLATFORM_LABEL[kind.platform] ? t(PLATFORM_LABEL[kind.platform]) : null, name].filter(Boolean).join(' · ')
  const kindOf = useMachineKinds(api, model.sandboxes.map((b) => b.machine ?? (binding?.sandbox === b.name ? binding.machine : undefined) ?? ''))
  const now = clock.t
  const [onlyChat, setOnlyChat] = useState(false)
  const [showQuiet, setShowQuiet] = useState(false)
  const [hover, setHover] = useState<Focus>(null)
  const [selected, setSelected] = useState<Focus>(null)
  const focus = hover ?? selected
  // The Activity list folds away so the map has the page; the choice is remembered per browser.
  const [activityOpen, setActivityOpen] = useState(() => readFlag(ACTIVITY_KEY) !== 'folded')
  const toggleActivity = () => setActivityOpen((open) => { writeFlag(ACTIVITY_KEY, open ? 'folded' : 'open'); return !open })
  // Workspaces unfolded to their terminal.
  const [openWs, setOpenWs] = useState<ReadonlySet<string>>(new Set())
  const toggleWs = (id: string) => setOpenWs((had) => { const next = new Set(had); if (!next.delete(id)) next.add(id); return next })
  const reduced = reducedMotion()
  const motion = !reduced

  const shownAgents = onlyChat && sessionId ? agents.filter((a) => a.id === sessionId || a.parent === sessionId) : agents
  const shownKey = shownAgents.map((a) => a.id).join('|')
  const events = useMemo(() => {
    const shown = new Set(shownKey.split('|'))
    return model.events.filter((e) => shown.has(e.agent))
  }, [model.events, shownKey])
  // Lookups built once per set of calls; a frame reads only the calls near the playhead.
  const index = useMemo(() => indexEvents(events), [events])
  const win = windowAt(index, now, index.maxSpan + TAIL_S + RIPPLE_S + 0.05)
  const active = activeAt(win, now)
  const done = (e: MapEvent) => e.end <= now
  // Each agent keeps one hue; its calls travel in it.
  const laneIndex = useMemo(() => new Map(agents.map((a, i) => [a.id, i % LANES])), [agents])
  const laneOf = useCallback((agent: string) => `var(--lane-${laneIndex.get(agent) ?? 0})`, [laneIndex])

  // What the focused node or call touches, for dimming the rest.
  const related = useMemo(() => {
    if (!focus) return null
    const set = new Set<string>([node('project', '')])
    const touch = (e: MapEvent) => {
      set.add(e.id); set.add(node('agent', e.agent))
      if (e.branch) set.add(node('branch', e.branch))
      if (e.ws) set.add(node('ws', e.ws))
      if (e.sandbox) set.add(node('sandbox', e.sandbox))
    }
    if (focus.kind === 'event') {
      const e = events.find((x) => x.id === focus.id)
      if (e) touch(e)
      return set
    }
    set.add(node(focus.kind, focus.id))
    for (const e of events) {
      if ((focus.kind === 'agent' && e.agent === focus.id) || (focus.kind === 'branch' && e.branch === focus.id) || (focus.kind === 'project')
        || (focus.kind === 'ws' && e.ws === focus.id) || (focus.kind === 'sandbox' && e.sandbox === focus.id)) touch(e)
    }
    for (const box of model.sandboxes) for (const ws of box.workspaces) {
      const mine = (focus.kind === 'ws' && focus.id === ws.id) || (focus.kind === 'sandbox' && focus.id === box.name) || (focus.kind === 'branch' && focus.id === ws.branch)
      if (mine) { set.add(node('ws', ws.id)); set.add(node('sandbox', box.name)); if (ws.branch) set.add(node('branch', ws.branch)) }
    }
    return set
  }, [focus, events, model.sandboxes])
  const dim = (key: string) => related !== null && !related.has(key)

  // A sandbox retires when its close ends; its workspaces leave the map with it.
  const retired = (box: SandboxMapModel['sandboxes'][number]) => {
    const closure = index.closeBySandbox.get(box.name)
    return box.closed && (!closure || done(closure))
  }
  const openBoxes = model.sandboxes.filter((box) => !retired(box))
  // A Workspace lives from its mount to its unmount (usually one bash call), so a Sandbox lists only the ones
  // mounted at the playhead (and any whose terminal is open), each on the Branch of its current mount.
  const mountsOf = (id: string) => index.mountsByWs.get(id) ?? []
  const branchAt = (ws: MapWorkspace) => {
    let branch = ws.branch
    for (const e of mountsOf(ws.id)) { if (e.start > now) break; if (e.method === 'workspace.mount' && e.branch) branch = e.branch }
    return branch
  }
  const liveWs = new Map(openBoxes.map((box) => [box.name, box.workspaces
    .filter((ws) => openWs.has(ws.id) || mountedAt(mountsOf(ws.id), ws.id, now))
    .map((ws) => ({ ...ws, branch: branchAt(ws) }))]))
  const wsOrder = openBoxes.flatMap((box) => liveWs.get(box.name) ?? [])
  // Every Workspace seen, for the arc: Branch places stay put while Workspaces come and go.
  const allWs = openBoxes.flatMap((box) => box.workspaces)
  // Branches mounted in a workspace take arc slots in workspace order, so branch → workspace wires rarely cross.
  const arc = useMemo(() => {
    const at = (b: string) => allWs.findIndex((w) => w.branch === b)
    const mounted = model.branches.filter((b) => at(b) >= 0).sort((a, b) => at(a) - at(b))
    let k = 0
    return model.branches.map((b) => at(b) >= 0 ? mounted[k++] : b)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [model.branches, allWs.map((w) => `${w.id}>${w.branch}`).join('|')])
  const born = useMemo(() => {
    const out = new Map<string, number>()
    for (const b of model.branches) {
      const first = model.events.find((e) => e.branch === b)
      out.set(node('branch', b), first?.method === 'branch.fork' ? first.start : -1)
    }
    for (const box of model.sandboxes) {
      const create = model.events.find((e) => e.method === 'sandbox.create' && e.sandbox === box.name)
      out.set(node('sandbox', box.name), create ? create.start : -1)
    }
    return out
  }, [model])
  const unborn = (key: string) => { const at = born.get(key) ?? -1; return at >= 0 && now < at }

  // Subagents with no LayerFS calls (theirs or their subagents') fold into one row.
  const quiet = useMemo(() => {
    const busy = new Set(model.events.map((e) => e.agent))
    const parentOf = new Map(agents.map((a) => [a.id, a.parent]))
    for (const id of [...busy]) for (let up = parentOf.get(id); up && !busy.has(up); up = parentOf.get(up)) busy.add(up)
    return new Set(agents.filter((a) => a.depth > 0 && !a.running && !busy.has(a.id)).map((a) => a.id))
  }, [agents, model.events])
  // One group per tree. Its card is the tree's: the title stays the root's when filtered to one subagent.
  const groups = useMemo(() => {
    const out: { head: MapAgent, agents: MapAgent[] }[] = []
    let head: MapAgent | undefined
    const shown = new Set(shownKey.split('|'))
    for (const agent of agents) {
      if (agent.depth === 0 && !(agent.joins && head?.id === agent.joins)) head = agent
      if (!head || !shown.has(agent.id)) continue
      if (out.at(-1)?.head !== head) out.push({ head, agents: [] })
      out.at(-1)!.agents.push(agent)
    }
    return out
  }, [agents, shownKey])
  const listedRows = groups.reduce((n, g) => n + (showQuiet ? g.agents.length : g.agents.filter((a) => !quiet.has(a.id)).length), 0)
  const compactAgents = listedRows > 6
  const compactWs = wsOrder.length > 5

  // Layout: measured from the DOM when the set of nodes or the width changes, never from the clock.
  const mapRef = useRef<HTMLDivElement>(null)
  const [width, setWidth] = useState(0)
  const [L, setL] = useState<Layout | null>(null)
  const frame = frameOf(width || 1200, arc.length, openWs.size > 0)
  const busyAgents = useMemo(() => {
    const busy = new Set(events.map((e) => e.agent))
    return shownAgents.filter((a) => busy.has(a.id)).map((a) => a.id)
  }, [events, shownAgents])
  const geometry = [arc.join(','), busyAgents.join(','), shownKey, showQuiet, openBoxes.map((b) => `${b.name}:${(liveWs.get(b.name) ?? []).map((w) => w.id).join('+')}`).join(','), model.sandboxes.length, compactAgents, compactWs, [...openWs].join('+')].join('#')
  const measure = useCallback(() => {
    const el = mapRef.current
    if (!el || !el.clientWidth) return
    const W = el.clientWidth
    setWidth(W)
    const f = frameOf(W, arc.length, openWs.size > 0)
    const nodes = new Map([...el.querySelectorAll<HTMLElement>('[data-flow-node]')].map((n) => [n.dataset.flowNode!, n]))
    const agentsCol = el.querySelector<HTMLElement>('[data-col="agents"]'), boxesCol = el.querySelector<HTMLElement>('[data-col="boxes"]')
    if (!agentsCol || !boxesCol) return
    const leftH = agentsCol.offsetHeight, rightH = boxesCol.offsetHeight
    const pw = arc.map((b) => nodes.get(node('branch', b))?.offsetWidth ?? 80)
    const maxPill = Math.max(0, ...pw)
    const hx = f.cz0 + f.leftRoom + f.rh + Math.max(0, (f.czW - f.leftRoom - f.rightRoom - (f.rh + f.Rx + maxPill)) / 2)
    const contentH = Math.max(leftH, rightH, arc.length ? 2 * f.half + 40 : 0, 2 * f.rh + (arc.length ? 150 : 134))
    const H = f.top + contentH + f.bottom
    const hy = f.top + contentH / 2
    const agentsTop = f.top + Math.max(0, (contentH - leftH) / 2), boxesTop = f.top + Math.max(0, (contentH - rightH) / 2)
    const n = arc.length
    const branch: Layout['branch'] = {}
    arc.forEach((b, i) => {
      const y = hy + (i - (n - 1) / 2) * f.step
      const x = hx + f.Rx * Math.sqrt(Math.max(0, 1 - ((y - hy) / f.Ry) ** 2))
      branch[b] = { x, y, left: x - 6, right: x - 6 + pw[i], ang: Math.atan2(y - hy, x - hx), w: pw[i] }
    })
    const mapBox = el.getBoundingClientRect()
    const colBox = { agents: agentsCol.getBoundingClientRect(), boxes: boxesCol.getBoundingClientRect() }
    // Rows sit where their column will: the column's new top plus the row's place in it.
    const place = (n: HTMLElement, col: 'agents' | 'boxes'): Rect => {
      const r = n.getBoundingClientRect()
      return { x: r.left - mapBox.left, y: (col === 'agents' ? agentsTop : boxesTop) + r.top - colBox[col].top, w: r.width, h: r.height }
    }
    const agent: Layout['agent'] = {}
    const ports = busyAgents.filter((id) => nodes.has(node('agent', id)))
    const spread = Math.min(84, 16 * Math.max(1, ports.length - 1)) * Math.PI / 180
    ports.forEach((id, i) => {
      const row = nodes.get(node('agent', id))!
      const r = place(row, 'agents')
      const av = row.querySelector<HTMLElement>('[data-avatar]')
      const a = av ? place(av, 'agents') : r
      const phi = Math.PI + spread / 2 - spread * (ports.length === 1 ? 0.5 : i / (ports.length - 1))
      agent[id] = { x: r.x + r.w, y: r.y + r.h / 2, phi, px: hx + f.rh * Math.cos(phi), py: hy + f.rh * Math.sin(phi), cx: a.x + a.w / 2, cy: a.y + a.h / 2 }
    })
    const ws: Layout['ws'] = {}
    for (const w of wsOrder) { const n = nodes.get(node('ws', w.id)); if (n) { const r = place(n, 'boxes'); ws[w.id] = { x: r.x, y: r.y + Math.min(r.h / 2, 16), r } } }
    const box: Layout['box'] = {}
    for (const b of openBoxes) {
      const head = nodes.get(node('sandbox', b.name))
      const card = head?.closest<HTMLElement>('[data-box]')
      if (head && card) { const r = place(head, 'boxes'); box[b.name] = { x: place(card, 'boxes').x, y: r.y + r.h / 2, r } }
    }
    const ys = arc.map((b) => branch[b].y)
    const gaps = n === 0 ? [hy] : n === 1 ? [hy + 22, hy - 22] : ys.slice(1).map((y, i) => (y + ys[i]) / 2)
    const zone = el.querySelector<HTMLElement>('[data-zone="hub"]')
    const zw = zone?.offsetWidth ?? 0
    const next: Layout = {
      W, H, hx, hy, rh: f.rh, Rx: f.Rx, Ry: f.Ry, agentsTop, boxesTop, hubLabelLeft: clamp(hx - zw / 2, f.cz0 + 8, Math.max(f.cz0 + 8, f.cz1 - zw - 8)),
      branch, agent, ws, box, gaps, maxRight: Math.max(hx + f.rh + 20, ...Object.values(branch).map((p) => p.right)),
    }
    setL((prev) => prev && JSON.stringify(prev) === JSON.stringify(next) ? prev : next)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [geometry])
  useLayoutEffect(() => {
    const el = mapRef.current
    if (!el) return
    const observer = new ResizeObserver(() => measure())
    observer.observe(el)
    el.querySelectorAll<HTMLElement>('[data-col]').forEach((c) => observer.observe(c))
    measure()
    return () => observer.disconnect()
  }, [measure])

  // Path lengths come from hidden SVG paths, one per chain, kept per layout.
  const meter = useRef<SVGGElement>(null)
  const meters = useMemo(() => new Map<string, { el: SVGPathElement, len: number }>(), [L])
  useEffect(() => () => { for (const m of meters.values()) m.el.remove() }, [meters])
  const measured = (d: string) => {
    let m = meters.get(d)
    if (!m && meter.current) {
      const el = document.createElementNS(SVG_NS, 'path')
      el.setAttribute('d', d)
      meter.current.appendChild(el)
      m = { el, len: el.getTotalLength() }
      meters.set(d, m)
    }
    return m
  }
  const chains = useMemo(() => new Map(L ? events.map((e) => [e.id, chainOf(L, e)] as const) : []), [L, events])

  const agentStatus = (agent: MapAgent) => {
    const running = active.find((e) => e.agent === agent.id)
    const last = lastEnded(index.endedByAgent.get(agent.id), now)
    if (running) return { live: true, text: verb(running, t), running, last }
    if (last && last.outcome !== 'ok') return { live: false, warn: true, text: t('needsAttention'), last }
    if (agent.running) return { live: true, text: t('working'), last }
    return { live: false, text: last ? t('done') : t('idle'), last }
  }
  /** The call of `ended` that ended last, and how much of its green/red glow is left (1 → 0). */
  const flashOf = (ended: Parameters<typeof lastEnded>[0]) => {
    const found = lastEnded(ended, now)
    if (!found) return null
    const k = (now - found.end) / TAIL_S
    return k < 1 ? { e: found, f: reduced ? 1 : 1 - easeOut(k) } : null
  }
  const arrived = (e: MapEvent) => now >= e.start + requestS(e, reduced) * 0.9

  // Calls in flight: the lit trail, the packet and its tail, arrival ripples.
  const lit: ReactNode[] = [], packets: ReactNode[] = [], ripples: ReactNode[] = []
  const litAgents = new Set<string>()
  const ripple = (key: string, target: string, k: number, color: string, muted: boolean) => {
    if (!L) return
    const op = 0.55 * (1 - k), grow = easeOut(k) * 10
    const props = { fill: 'none', stroke: color, strokeWidth: 1.5, strokeOpacity: op, className: clsx(muted && css.faded) }
    if (target.startsWith('agent:')) {
      const A = L.agent[target.slice(6)]
      if (A) ripples.push(<circle key={key} cx={A.cx} cy={A.cy} r={15 + grow} {...props} />)
      return
    }
    let r: Rect & { rx: number } | undefined
    if (target.startsWith('branch:')) { const B = L.branch[target.slice(7)]; if (B) r = { x: B.left, y: B.y - 12, w: B.w, h: 24, rx: 12 } }
    else if (target.startsWith('ws:')) { const W = L.ws[target.slice(3)]; if (W) r = { ...W.r, rx: 9 } }
    else if (target.startsWith('sandbox:')) { const b = L.box[target.slice(8)]; if (b) r = { x: b.r.x + 4, y: b.r.y, w: b.r.w - 8, h: b.r.h, rx: 9 } }
    if (r) ripples.push(<rect key={key} x={r.x - grow} y={r.y - grow} width={r.w + 2 * grow} height={r.h + 2 * grow} rx={r.rx + grow} {...props} />)
  }
  for (const e of win) {
    if (now < e.start || now > e.end + TAIL_S + RIPPLE_S) continue
    const chain = chains.get(e.id)
    if (!chain) continue
    const ph = phaseOf(e, now, reduced)
    const lane = laneOf(e.agent), result = e.outcome === 'ok' ? OK : BAD
    const muted = related !== null && !related.has(e.id)
    if (motion) {
      const k1 = rippleAt(now, e.start + requestS(e)); if (k1 !== null) ripple(`${e.id}:in`, chain.target, k1, lane, muted)
      const k2 = rippleAt(now, e.end + RESULT_S); if (k2 !== null) ripple(`${e.id}:home`, node('agent', e.agent), k2, result, muted)
    }
    if (!ph) continue
    if (ph.kind !== 'flash') litAgents.add(e.agent)
    const m = measured(chain.d)
    if (!m) continue
    const len = m.len
    const base = { d: chain.d, fill: 'none', strokeLinecap: 'round' as const, strokeLinejoin: 'round' as const, className: clsx(muted && css.faded) }
    if (ph.kind === 'request') lit.push(<path key={e.id} {...base} stroke={lane} strokeWidth={1.6} strokeOpacity={0.75} strokeDasharray={`${f1(ph.u * len)} ${f1(len + 10)}`} />)
    else if (ph.kind === 'held') lit.push(<path key={e.id} {...base} stroke={lane} strokeWidth={1.6} strokeOpacity={0.75} />)
    else if (ph.kind === 'result') {
      const s = ph.u * len
      lit.push(<path key={e.id} {...base} stroke={lane} strokeWidth={1.6} strokeOpacity={0.75} strokeDasharray={`${f1(s)} ${f1(len + 10)}`} />)
      lit.push(<path key={`${e.id}:r`} {...base} stroke={result} strokeWidth={1.8} strokeOpacity={0.85} strokeDasharray={`${f1(len)} ${f1(len + 10)}`} strokeDashoffset={f1(-s)} />)
    } else lit.push(<path key={e.id} {...base} stroke={result} strokeWidth={1.6} strokeOpacity={0.8 * ph.f * ph.f} />)
    if (motion && (ph.kind === 'request' || ph.kind === 'result')) {
      const s = ph.u * len
      const color = ph.kind === 'request' ? lane : result
      const tail = 26
      const from = ph.kind === 'request' ? Math.max(0, s - tail) : s, to = ph.kind === 'request' ? s : Math.min(len, s + tail)
      const pt = m.el.getPointAtLength(clamp(s, 0, len))
      packets.push(<path key={`${e.id}:tail`} {...base} stroke={color} strokeWidth={3.6} strokeOpacity={0.32} strokeDasharray={`${f1(to - from)} ${f1(len + 40)}`} strokeDashoffset={f1(-from)} />)
      packets.push(
        <g key={`${e.id}:p`} transform={`translate(${f1(pt.x)} ${f1(pt.y)})`} className={clsx(muted && css.faded)}>
          <circle r={7} fill={color} fillOpacity={0.14} />
          <circle r={4.2} fill={color} className={css.packet} />
          <circle r={4.2} fill="url(#sandbox-map-bubble)" />
        </g>,
      )
    }
  }

  const selectedEvent = useMemo(() => selected?.kind === 'event' ? events.find((e) => e.id === selected.id) : undefined, [selected, events])
  const listFocus = focus && focus.kind !== 'event' ? focus : null
  const focusCalls = !listFocus || listFocus.kind === 'project' ? events
    : (listFocus.kind === 'agent' ? index.byAgent : listFocus.kind === 'branch' ? index.byBranch : listFocus.kind === 'ws' ? index.byWs : index.bySandbox).get(listFocus.id) ?? []
  const listedCount = startedBy(focusCalls, now)
  const recentCalls = focusCalls.slice(Math.max(0, listedCount - EVENT_ROWS), listedCount)
  const agentName = useCallback((id: string) => { const a = agents.find((x) => x.id === id); return a ? name(a) : id }, [agents, name])
  const focusName = (f: Focus) => !f || f.kind === 'event' ? null : f.kind === 'agent' ? agentName(f.id) : f.kind === 'project' ? project.split('/').at(-1) ?? project : f.id.split(':').at(-1) ?? f.id
  const choose = useCallback((next: NonNullable<Focus>) => setSelected((s) => s && s.kind === next.kind && s.id === next.id ? null : next), [])
  const hoverProps = (f: NonNullable<Focus>) => ({ onMouseEnter: () => setHover(f), onMouseLeave: () => setHover(null), onFocus: () => setHover(f), onBlur: () => setHover(null), onClick: () => choose(f) })
  const isSelected = (f: NonNullable<Focus>) => selected?.kind === f.kind && selected.id === f.id
  const empty = loaded && model.events.length === 0
  const disabled = model.duration === 0
  const playState = clock.live && clock.playing ? t('live') : clock.playing ? t('playing') : t('paused')
  const following = focusName(focus)
  const hubBusy = win.some((e) => e.start <= now && now < e.end + RESULT_S)
  const projectName = project.split('/').at(-1) ?? project
  // One SVG in replay seconds: a longer log only changes its viewBox, and new calls add marks.
  const strip = useMemo(() => {
    const rows = agents.filter((a) => index.byAgent.has(a.id))
    return (
      <svg className={css.strip} viewBox={`0 0 ${f1(Math.max(0.1, model.duration))} ${Math.max(1, rows.length)}`} preserveAspectRatio="none" aria-hidden>
        {rows.map((a, r) => <StripRow key={a.id} row={r} lane={laneIndex.get(a.id) ?? 0} events={index.byAgent.get(a.id)!} />)}
      </svg>
    )
  }, [agents, index, model.duration, laneIndex])

  return (
    <div className={css.page} data-playing={clock.playing || undefined}>
      <div className={css.toolbar}>
        <span className={css.facts}><IconUsersOutlineRegular size={13} />{[count(t, shownAgents.length, 'factAgentOne', 'factAgents'), count(t, model.branches.length, 'factBranchOne', 'factBranches'), count(t, model.sandboxes.length, 'factSandboxOne', 'factSandboxes')].join(' · ')}
          {hiddenBranches > 0 ? <span className={css.muted} title={t('recentBranchesHint')}> · {t('recentBranches', { n: full.branches.length })}</span> : null}</span>
        {sessionId && agents.length > 1 ? <Checkbox className={css.filter} checked={onlyChat} onChange={setOnlyChat} label={t('thisChat')} /> : null}
        <span className={css.spacer} />
        <Button variant="outline" size="sm" disabled={disabled} onClick={clock.toggle}
          icon={clock.playing ? <IconPauseOutlineRegular size={14} /> : <IconPlayOutlineRegular size={14} />}>
          {clock.playing ? t('pause') : now >= model.duration && !disabled ? t('playAgain') : t('play')}
        </Button>
        <Button variant="ghost" size="sm" disabled={disabled} onClick={clock.replay} aria-label={t('replay')} title={t('replay')} icon={<IconRefreshOutlineRegular size={14} />} />
        <SegmentedControl<ReplaySpeed> id={`${speedId}-speed`} className={css.speed} value={clock.speed} label={t('replaySpeed')}
          disabled={disabled} onChange={clock.setSpeed} options={REPLAY_SPEEDS.map((v) => ({ value: v, label: `${v}×` }))} />
        <Button variant={clock.live ? 'outline' : 'ghost'} size="sm" disabled={disabled} onClick={clock.goLive} aria-pressed={clock.live}>
          <span className={clsx(css.liveDot, clock.live && clock.playing && css.liveOn)} aria-hidden />{t('live')}
        </Button>
      </div>
      {note ? <p className={css.note}>{note}</p> : null}

      <div className={css.scroller}>
      <div className={clsx(css.map, !L && css.measuring)} ref={mapRef} onMouseLeave={() => setHover(null)}
        style={{ height: L?.H, ['--hx' as string]: L ? `${L.hx}px` : '50%', ['--hy' as string]: L ? `${L.hy}px` : '50%' }}>
        <svg className={css.under} aria-hidden>
          <defs>
            <radialGradient id="sandbox-map-bubble" cx="35%" cy="32%" r="70%">
              <stop offset="0" stopColor="var(--dsw-static-neutral-00, #fff)" stopOpacity={0.95} />
              <stop offset="0.35" stopColor="var(--dsw-static-neutral-00, #fff)" stopOpacity={0} />
            </radialGradient>
          </defs>
          <g ref={meter} visibility="hidden" />
          {L ? (
            <>
              <g className={css.rings}>
                <circle cx={L.hx} cy={L.hy} r={L.rh + 16} strokeOpacity={0.55} />
                <circle cx={L.hx} cy={L.hy} r={L.rh + 38} strokeOpacity={0.32} />
                <ellipse cx={L.hx} cy={L.hy} rx={L.Rx} ry={L.Ry} strokeOpacity={0.5} strokeDasharray="1.5 5" strokeLinecap="round" />
              </g>
              <g className={css.wires}>
                {busyAgents.map((id) => {
                  const d = agentWire(L, id)
                  return d ? <path key={id} d={d} className={clsx(dim(node('agent', id)) && css.wireDim)}
                    style={litAgents.has(id) ? { stroke: `color-mix(in srgb, ${laneOf(id)} 30%, transparent)` } : undefined} /> : null
                })}
                {arc.map((b) => {
                  const B = L.branch[b]
                  if (!B) return null
                  const [sx, sy] = hubEdge(L, B.ang)
                  return <path key={b} d={`M ${f1(sx)} ${f1(sy)} L ${f1(B.left)} ${f1(B.y)}`} strokeDasharray={unborn(node('branch', b)) ? '2 4' : undefined} className={clsx(dim(node('branch', b)) && css.wireDim)} />
                })}
                {wsOrder.map((w) => {
                  const B = w.branch ? L.branch[w.branch] : undefined, W = L.ws[w.id]
                  if (!B || !W) return null
                  return <path key={w.id} d={`M ${f1(B.right)} ${f1(B.y)} ${curve(B.right, B.y, W.x, W.y)}`} strokeDasharray={!mountedAt(index.mountsByWs.get(w.id) ?? [], w.id, now) ? '2 4' : undefined} className={clsx(dim(node('ws', w.id)) && css.wireDim)} />
                })}
              </g>
              <g>{lit}</g>
              <g>{ripples}</g>
              <g>{packets}</g>
            </>
          ) : null}
        </svg>

        <div className={css.zone} style={{ left: frame.pad + 4 }}><IconUsersOutlineRegular size={13} /><b>{t('agents')}</b><span className={css.count}>{shownAgents.length}</span></div>
        <div className={css.zone} data-zone="hub" style={{ left: L?.hubLabelLeft ?? frame.cz0 + 8, maxWidth: frame.czW - 16 }}>
          <b>{t('layerfs')}</b><span className={css.project} title={project}>{projectName}</span><span className={css.count}>{count(t, model.branches.length, 'factBranchOne', 'factBranches')}</span>
        </div>
        <div className={css.zone} style={{ left: frame.cz1 + 4 }}><SandboxGlyph size={13} /><b>{t('sandboxes')}</b><span className={css.count}>{model.sandboxes.length}</span></div>

        <section className={clsx(css.col, compactAgents && css.compact)} data-col="agents" style={{ left: frame.pad, width: frame.leftW, top: L?.agentsTop ?? frame.top }}>
          {groups.map((group) => {
            const quietShown = group.agents.filter((a) => quiet.has(a.id)).length
            const listedAgents = showQuiet ? group.agents : group.agents.filter((a) => !quiet.has(a.id))
            return (
              <div key={group.head.id} className={css.pod}>
                <div className={css.podHead}>
                  <span className={css.podTitle} title={groupTitle(group.head)}>{groupTitle(group.head)}</span>
                  {group.head.id === sessionId ? <span className={css.tag}>{t('thisChat')}</span> : null}
                </div>
                {listedAgents.map((agent) => {
                  const s = agentStatus(agent)
                  const f = { kind: 'agent', id: agent.id } as const
                  const run = s.running
                  const since = run ? now - run.start : 0
                  const depth = Math.min(agent.depth, 3)
                  return (
                    <button key={agent.id} type="button" data-flow-node={node('agent', agent.id)} {...hoverProps(f)}
                      className={clsx(css.agent, run && css.working, s.warn && css.warn, dim(node('agent', agent.id)) && css.dim, isSelected(f) && css.selected)}
                      style={{ paddingLeft: 8 + depth * 14, ['--lane' as string]: laneOf(agent.id) }}>
                      {depth > 0 ? <span className={css.guide} style={{ left: depth * 14 - 4 }} aria-hidden /> : null}
                      <span className={css.avatar} data-avatar>
                        <Whale size={compactAgents ? 24 : 28}
                          spout={run ? (motion ? easeOut(clamp(since / 0.35, 0, 1)) : 1) : 0}
                          bob={run && motion ? -Math.sin(since * Math.PI * 2 / 1.9) * 1.1 : 0} />
                      </span>
                      <span className={css.rowHead}>
                        <span className={css.rowName} title={name(agent)}>{name(agent)}</span>
                        {agent.depth > 0 && agent.id === sessionId ? <span className={css.tag}>{t('thisChat')}</span> : null}
                        <span className={css.status}><i className={clsx(css.dot, run && css.dotLane, !run && s.live && css.dotLive, s.warn && css.dotBad)} />{s.text}</span>
                      </span>
                      <span className={css.callLine}>
                        {run ? <><PromptGlyph size={12} /><span className={css.mono}>{run.method}</span><span className={css.right}>{t('dur_s', { n: since.toFixed(1) })}</span></>
                          : s.last ? <>{s.last.outcome === 'ok' ? <IconCheckOutlineRegular size={12} /> : <IconWarningOutlineRegular size={12} className={css.errorText} />}<span className={css.mono}>{s.last.method}</span><span className={css.right}>{s.last.turn ? t('turn', { n: s.last.turn }) : stamp(s.last.end)}</span></>
                          : <><IconBranchOutlineRegular size={12} /><span>{binding?.branch ?? t('noCalls')}</span></>}
                      </span>
                    </button>
                  )
                })}
                {quietShown > 0 ? (
                  <button type="button" className={css.fold} aria-expanded={showQuiet} onClick={() => setShowQuiet((v) => !v)}>
                    <IconChevronRightOutlineRegular size={12} className={clsx(css.foldIcon, showQuiet && css.foldOpen)} />
                    {showQuiet ? t('quietHide', { n: quietShown }) : t('quietShow', { n: quietShown })}
                  </button>
                ) : null}
              </div>
            )
          })}
        </section>

        <button type="button" className={clsx(css.hub, hubBusy && css.hubBusy, dim(node('project', '')) && css.dim, isSelected({ kind: 'project', id: '' }) && css.selected)}
          data-flow-node={node('project', '')} aria-label={`${t('layerfs')} ${projectName}`} title={project} {...hoverProps({ kind: 'project', id: '' })}
          style={L ? { left: L.hx - L.rh, top: L.hy - L.rh, width: 2 * L.rh, height: 2 * L.rh } : { width: 2 * frame.rh, height: 2 * frame.rh }}>
          <LayerfsMark size="56%" />
        </button>

        {arc.map((branch) => {
          const f = { kind: 'branch', id: branch } as const
          const key = node('branch', branch)
          const use = win.filter((e) => e.branch === branch && arrived(e) && now < e.end)
          const flash = use.length ? null : flashOf(index.endedOnBranch.get(branch))
          const saves = countEnded(index.savesByBranch.get(branch), now)
          const B = L?.branch[branch]
          return (
            <button key={branch} type="button" data-flow-node={key} {...hoverProps(f)}
              className={clsx(css.pill, unborn(key) && css.ghost, use.length > 0 && css.use, flash && (flash.e.outcome === 'ok' ? css.okFlash : css.badFlash), dim(key) && css.dim, isSelected(f) && css.selected)}
              style={{ left: B ? B.left : 0, top: B ? B.y - 12 : 0, maxWidth: frame.pillCap, ['--lane' as string]: use.length ? laneOf(use.at(-1)!.agent) : undefined, ['--f' as string]: flash ? flash.f.toFixed(3) : undefined }}>
              <span className={css.knob}><i /></span>
              <span className={css.pillName} title={branch}>{branch}</span>
              {binding && branch === binding.branch ? <span className={css.tag}>{t('thisChat')}</span> : null}
              {saves > 0 ? <span className={css.saves} title={saves === 1 ? t('saveOne') : t('saves', { n: saves })}><CommitGlyph size={11} /><span key={saves} className={css.bump}>{saves}</span></span> : null}
            </button>
          )
        })}

        <section className={css.col} data-col="boxes" style={{ left: frame.cz1, width: frame.rightW, top: L?.boxesTop ?? frame.top }}>
          {model.sandboxes.length === 0 ? (
            <div className={css.boxNone}><SandboxGlyph size={15} /><div><p className={css.emptyTitle}>{t('noSandboxes')}</p><p className={css.muted}>{t('noSandboxesBody')}</p></div></div>
          ) : null}
          {model.sandboxes.map((box) => {
            const f = { kind: 'sandbox', id: box.name } as const
            const key = node('sandbox', box.name)
            if (retired(box)) return <div key={box.name} className={css.retired}><IconCheckOutlineRegular size={13} />{t('closed', { name: box.name })}<span className={css.muted}>{t('closedNote')}</span></div>
            const creation = index.createBySandbox.get(box.name)
            const creating = !!creation && creation.start <= now && now < creation.end
            const ghost = unborn(key)
            const bound = !!binding && binding.sandbox === box.name
            const known = boxState?.(box.name)
            const state = ghost ? t('notCreated') : creating ? t('creating') : known ? known.text : bound ? stateWords(binding.executionState ?? (binding.state === 'bound' ? 'active' : binding.state), t) : t('active')
            const stateLive = !ghost && (creating || (known ? known.live : bound && binding.state === 'bound'))
            const use = win.find((e) => !e.branch && e.sandbox === box.name && arrived(e) && now < e.end)
            const machine = box.machine ?? (bound ? binding.machine : undefined)
            const shownWs = liveWs.get(box.name) ?? []
            const mounted = shownWs.filter((ws) => mountedAt(mountsOf(ws.id), ws.id, now))
            const used = mounted.length + (box.reserved ?? 0)   // dsh's own Workspace holds a slot though it is not drawn
            return (
              <div key={box.name} data-box className={clsx(css.box, ghost && css.ghost, dim(key) && css.dim)}>
                <button type="button" className={clsx(css.boxHead, use && css.use, isSelected(f) && css.selected)} data-flow-node={key} {...hoverProps(f)}
                  title={[box.name, machineLabel(kindOf(machine), machine ?? t('machineUnknown')), t('workspacesMounted', { n: used, max: WS_SLOTS })].join('\n')}
                  style={{ ['--lane' as string]: use ? laneOf(use.agent) : creation ? laneOf(creation.agent) : undefined }}>
                  <MachineMark kind={kindOf(machine)} label={machineLabel(kindOf(machine), machine)} /><span className={css.rowName}>{box.name}</span>
                  {!ghost ? <span className={clsx(css.count, used >= WS_SLOTS - 1 && css.errorText)} title={t('workspacesMounted', { n: used, max: WS_SLOTS })}>{used}/{WS_SLOTS}</span> : null}
                  <span className={css.status}><i className={clsx(css.dot, creating ? css.dotLane : known?.bad ? css.dotBad : stateLive && css.dotLive)} />{state}</span>
                </button>
                {boxActions ? <div className={css.boxActions}>{boxActions(box.name)}</div> : null}
                {/* dsh's own session Workspace: listed (it holds a slot), off the flow; nobody runs commands in it */}
                {!ghost && box.own?.map((ws) => (
                  <p key={ws.id} className={css.boxOwn} data-own-ws={ws.id} title={[ws.id, ws.branch ? `${t('branch')}: ${ws.branch}` : null].filter(Boolean).join('\n')}>
                    <span className={css.rowName}>{ws.name}</span><span className={css.muted}>{t('sessionWorkspace')}</span>
                  </p>
                ))}
                {shownWs.length === 0 && !(box.own?.length && !ghost) ? <p className={css.boxEmpty}>{creating ? t('starting') : t('noWorkspaces')}</p> : shownWs.map((ws) => {
                  const wf = { kind: 'ws', id: ws.id } as const
                  const wkey = node('ws', ws.id)
                  const wsGhost = ghost || !mountedAt(mountsOf(ws.id), ws.id, now)
                  const run = active.find((e) => e.ws === ws.id)
                  const here = !!run && arrived(run)
                  const last = lastEnded(index.endedByWs.get(ws.id), now)
                  const bad = !run && !!last && last.outcome !== 'ok'
                  const flash = run ? null : flashOf(index.endedByWs.get(ws.id))
                  const users = (index.usersByWs.get(ws.id) ?? []).filter((u) => u.first <= now).map((u) => u.agent)
                  const open = openWs.has(ws.id)
                  // One line: the name, and a state only when something is going on. The rest is in the tooltip and the terminal.
                  const state = run ? (here ? verb(run, t) : t('incoming')) : bad ? last!.outcome : wsGhost ? t('notMounted') : null
                  const tip = [ws.id, ws.branch ? `${t('branch')}: ${ws.branch}` : null, users.length ? users.map(agentName).join(', ') : null, open ? t('wsTerminalHide') : t('wsTerminalShow')].filter(Boolean).join('\n')
                  return (
                    <Fragment key={ws.id}>
                    <button type="button" data-flow-node={wkey} {...hoverProps(wf)} onClick={() => toggleWs(ws.id)} aria-expanded={open} title={tip}
                      className={clsx(css.ws, open && css.wsOpen, wsGhost && css.ghost, here && css.use, flash && (flash.e.outcome === 'ok' ? css.okFlash : css.badFlash), bad && css.bad, dim(wkey) && css.dim, isSelected(wf) && css.selected)}
                      style={{ ['--lane' as string]: run ? laneOf(run.agent) : undefined, ['--f' as string]: flash ? flash.f.toFixed(3) : undefined }}>
                      <span className={css.wsName}><IconChevronRightOutlineRegular size={11} className={clsx(css.foldIcon, css.wsChevron, open && css.foldOpen)} /><span className={css.rowName}>{ws.name}</span></span>
                      {state ? <span className={css.status}>{run ? <i className={clsx(css.dot, css.dotLane)} /> : null}{state}{run ? <span className={css.elapsed}>{t('dur_s', { n: (now - run.start).toFixed(1) })}</span> : null}</span> : null}
                    </button>
                    {open ? <WorkspaceTerminal api={api} t={t} events={(index.byWs.get(ws.id) ?? []).slice(0, startedBy(index.byWs.get(ws.id) ?? [], now))} wsName={ws.name} now={now} agentName={agentName} laneOf={laneOf} reduced={reduced} /> : null}
                    </Fragment>
                  )
                })}
              </div>
            )
          })}
        </section>

        <div className={css.legend} style={{ left: L?.hx ?? frame.cz0 + frame.czW / 2 }}>
          <span><i className={css.legendLane} />{t('legendRequest')}</span>
          <span><i className={css.legendOk} />{t('legendResult')}</span>
          <span><i className={css.legendError} />{t('legendRefused')}</span>
        </div>
      </div>
      </div>

      <div className={css.playback}>
        <span className={css.playState}><i className={clsx(css.dot, clock.playing && css.dotLive)} />{playState}</span>
        <div className={css.track}>
          {strip}
          <input className={css.scrub} type="range" min={0} max={Math.max(0.1, model.duration)} step={0.05} value={Math.min(now, model.duration)}
            style={{ ['--fill' as string]: `${model.duration ? Math.min(100, (now / model.duration) * 100) : 0}%` }}
            aria-label={t('scrub')} disabled={disabled} onChange={(e) => clock.scrub(Number(e.target.value))} />
        </div>
        <span className={css.time}>{stamp(now)} / {stamp(model.duration)}</span>
      </div>

      <section className={css.inspector}>
        <header className={css.inspectorHead}>
          <button type="button" className={css.activityToggle} aria-expanded={activityOpen} onClick={() => toggleActivity()}
            title={activityOpen ? t('activityHide') : t('activityShow')}>
            <IconChevronRightOutlineRegular size={12} className={clsx(css.foldIcon, activityOpen && css.foldOpen)} />
            <strong>{selectedEvent ? selectedEvent.method : following ? t('following', { name: following }) : t('activity')}</strong>
          </button>
          {!activityOpen ? <span className={css.muted}>{count(t, listedCount, 'callsOne', 'callsN')}</span>
            : selected ? (
              <Button variant="ghost" size="sm" aria-label={t('clear')} title={t('clear')} icon={<IconCloseOutlineRegular size={13} />} onClick={() => { setSelected(null); setHover(null) }} />
            ) : <span className={css.muted}>{listedCount > EVENT_ROWS ? t('callsShown', { n: EVENT_ROWS }) : t('follow')}</span>}
        </header>
        {!activityOpen ? null : selectedEvent ? <EventDetail api={api} e={selectedEvent} t={t} now={now} owner={agentName(selectedEvent.agent)} lane={laneOf(selectedEvent.agent)} />
          : empty ? (
            <div className={css.emptyCalls}><p className={css.emptyTitle}>{emptyCopy?.title ?? t('noActivity')}</p><p className={css.muted}>{emptyCopy?.body ?? t('noActivityBody')}</p></div>
          ) : (
            <ActivityRows calls={focusCalls} count={listedCount} ended={recentCalls.filter(done).length} now={now} t={t}
              choose={choose} setHover={setHover} laneOf={laneOf} agentName={agentName} />
          )}
      </section>
    </div>
  )
}


interface ActivityRowsProps {
  /** The calls listed (sorted by start) and how many of them have started. */
  calls: readonly MapEvent[]
  count: number
  /** How many of the rows shown have ended: with `calls` and `count`, all that changes the rows. */
  ended: number
  now: number
  t: Translate
  choose: (f: NonNullable<Focus>) => void
  setHover: (f: Focus) => void
  laneOf: (agent: string) => string
  agentName: (id: string) => string
}

/** The newest `EVENT_ROWS` calls, newest first; redrawn only when a row is added or ends, not every frame. */
const ActivityRows = memo(function ActivityRows({ calls, count, now, t, choose, setHover, laneOf, agentName }: ActivityRowsProps): ReactNode {
  const rows = calls.slice(Math.max(0, count - EVENT_ROWS), count).reverse()
  return (
    <ol className={css.events}>
      {rows.map((e) => (
        <li key={e.id}>
          <button type="button" className={css.event} onClick={() => choose({ kind: 'event', id: e.id })}
            onMouseEnter={() => setHover({ kind: 'event', id: e.id })} onMouseLeave={() => setHover(null)} style={{ ['--lane' as string]: laneOf(e.agent) }}>
            <EventIcon e={e} now={now} />
            <time className={css.eventTime} dateTime={new Date(e.at).toISOString()} title={dateTime(e.at)}>{stamp(e.start)}</time>
            <span className={css.eventMethod}>{e.method}</span>
            <span className={css.eventTarget}>{e.ws ?? e.branch ?? e.sandbox ?? ''}</span>
            {e.outcome !== 'ok' && e.end <= now ? <span className={css.errorText}>{e.outcome}</span> : null}
            <span className={css.eventOwner}><Whale size={16} />{agentName(e.agent)}{e.turn ? ` · ${t('turn', { n: e.turn })}` : ''}</span>
            <IconChevronRightOutlineRegular size={12} className={css.muted} />
          </button>
        </li>
      ))}
    </ol>
  )
}, (a, b) => a.calls === b.calls && a.count === b.count && a.ended === b.ended && a.t === b.t && a.choose === b.choose
  && a.setHover === b.setHover && a.laneOf === b.laneOf && a.agentName === b.agentName)

/** One agent's calls on the timeline strip; a mark keeps at least a pixel at any zoom. */
const StripRow = memo(function StripRow({ row, lane, events }: { row: number, lane: number, events: readonly MapEvent[] }): ReactNode {
  return (
    <g style={{ color: `var(--lane-${lane})` }}>
      {events.map((e) => (
        <rect key={e.id} x={e.start} y={row + 0.12} width={Math.max(0.001, e.end - e.start)} height={0.76} fill="currentColor"
          stroke={e.outcome !== 'ok' ? BAD : 'currentColor'} strokeWidth={1} vectorEffect="non-scaling-stroke" />
      ))}
    </g>
  )
})

/** `1 agent` or `{n} agents`: a count with its singular. */
function count(t: Translate, n: number, one: string, many: string): string {
  return n === 1 ? t(one) : t(many, { n })
}

const VERBS: Record<string, string> = {
  'workspace.exec': 'executing', 'workspace.commit': 'saving', 'commit.create': 'saving', 'workspace.mount': 'mounting',
  'sandbox.create': 'creating', 'branch.fork': 'forking', 'branch.merge': 'merging', 'branch.publish': 'publishing', 'file.write': 'writing',
}
function verb(e: MapEvent, t: Translate): string {
  return t(VERBS[e.method] ?? 'calling')
}

function EventIcon({ e, now }: { e: MapEvent, now: number }): ReactNode {
  const running = e.end > now
  return (
    <span className={clsx(css.eventIcon, !running && (e.outcome === 'ok' ? css.okText : css.errorText))}>
      {running ? <i className={clsx(css.dot, css.dotLive)} /> : e.outcome === 'ok' ? <IconCheckOutlineRegular size={13} /> : <IconWarningOutlineRegular size={13} />}
    </span>
  )
}

/** One call: the command, its timing and outcome, and what it printed when this plugin kept it. */
function EventDetail({ api, e, t, now, owner, lane }: { api: ViewProps['api'], e: MapEvent, t: Translate, now: number, owner: string, lane: string }): ReactNode {
  const [output, setOutput] = useState<LogsOutputAnswer | null>(null)
  useEffect(() => {
    setOutput(null)
    if (!e.tag || !e.output) return
    let live = true
    api.op<LogsOutputAnswer>('logs.output', { tag: e.tag }).then((answer) => { if (live) setOutput(answer) }, () => { if (live) setOutput({ missing: true }) })
    return () => { live = false }
  }, [api, e.tag, e.output])
  const inFlight = now < e.end
  const text = output && !('missing' in output) ? [output.stdout, output.stderr].filter(Boolean).join('\n').trim() : ''
  return (
    <div className={css.detail}>
      <div className={css.detailFacts}>
        <EventIcon e={e} now={now} />
        <span className={css.detailOwner} style={{ ['--lane' as string]: lane }}><Whale size={16} />{owner}{e.turn ? ` · ${t('turn', { n: e.turn })}` : ''}</span>
        <span>{stamp(e.start)} → {stamp(e.end)}</span>
        <span>{e.ms < 1000 ? t('dur_ms', { n: e.ms }) : t('dur_s', { n: (e.ms / 1000).toFixed(1) })}</span>
        {!inFlight ? <span className={e.outcome === 'ok' ? css.okText : css.errorText}>{e.outcome === 'ok' ? t('outcomeOk') : e.outcome}</span> : null}
      </div>
      <code className={css.detailCommand}>{shellCommand(e.argv)}</code>
      <pre className={clsx(css.detailOutput, !inFlight && e.outcome !== 'ok' && css.errorText)}>
        {now < e.start ? t('notYet') : inFlight ? t('inFlight') : text ? text.slice(0, 4000) : e.output && !output ? t('loading') : t('noOutput')}
      </pre>
    </div>
  )
}

export type { SandboxMapModel }
