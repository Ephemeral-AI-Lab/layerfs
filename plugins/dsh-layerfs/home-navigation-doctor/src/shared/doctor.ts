/**
 * The one doctor answer of LayerFS 2.0 and the fix lines a client may run,
 * shared by the host route (`host/routes/doctor.ts`) and the Doctor page
 * (`client/doctor/`). Shapes follow the frozen call surface (`calls.json`:
 * `doctor`, `home.get` `with.tree`, `state_companions`). Pure: no I/O.
 */

/** What a doctor can name. A Workspace has no doctor of its own: its Sandbox checks it. */
export type ThingKind = 'home' | 'machine' | 'store' | 'project' | 'sandbox' | 'workspace' | 'bucket'
export const THING_KINDS: readonly ThingKind[] = ['home', 'machine', 'store', 'project', 'sandbox', 'workspace', 'bucket']

/** The kinds a doctor call takes, and the kinds that have a Full check. */
export const DOCTOR_KINDS: readonly ThingKind[] = ['home', 'machine', 'store', 'project', 'sandbox', 'bucket']
export const FULL_KINDS: readonly ThingKind[] = ['project', 'bucket']

/** The lists a node may carry, by the kind of what they hold. */
export const CHILD_LISTS: Readonly<Record<string, ThingKind>> = {
  machines: 'machine', stores: 'store', projects: 'project', sandboxes: 'sandbox', workspaces: 'workspace', buckets: 'bucket',
}

export type Outcome = 'pass' | 'notice' | 'warning' | 'error' | 'not_checked'
export type Status = 'ok' | 'notice' | 'warning' | 'error' | 'not_checked'
export type Severity = 'error' | 'warning' | 'notice'
/** Why a check that was tried gave no verdict (R3). */
export type CouldNotReason = 'timeout' | 'busy' | 'lost'

export interface DoctorCheck { check: string, outcome: Outcome, from: string | null }
/** `{ <machine|store|sandbox>: name, reason }`: the thing at the root of a block. */
export type BlockedBy = Record<string, string>

/** One node of a report's `tree`: `{ <thing>: name, status, coverage, checks, blocked_by, reason, <child lists> }`. */
export interface DoctorNode {
  status: Status
  coverage: 'none' | 'shallow' | 'full'
  checks: DoctorCheck[]
  blocked_by: BlockedBy | null
  reason: CouldNotReason | null
  [key: string]: unknown
}

export interface DoctorFinding {
  check: string
  severity: Severity
  message: string
  fix: string | null
  from: string | null
  evidence?: unknown
  [thing: string]: unknown
}

export interface NotChecked {
  check: string | null
  blocked_by: BlockedBy | null
  reason: CouldNotReason | null
  [thing: string]: unknown
}

/** The long form of every doctor (`--detail`). */
export interface DoctorReport {
  ok: boolean
  findings: DoctorFinding[]
  not_checked: NotChecked[]
  status: Status
  counts: Record<Status, number>
  observed_at: string
  tree: DoctorNode
}

/** `--progress`: one event per finished thing; `node` is that thing's node without child lists. */
export interface DoctorProgress { phase: string, done: number, total: number, node: DoctorNode }

/** The kind and handle an object names by its own word (`{ "machine": "dev", … }`), or null. */
export function thingOf(value: unknown): { kind: ThingKind, name: string } | null {
  if (!value || typeof value !== 'object') return null
  for (const kind of THING_KINDS) {
    const name = (value as Record<string, unknown>)[kind]
    if (typeof name === 'string' && name) return { kind, name }
  }
  return null
}

/** `kind:name`, the key the page and the host file a thing under. */
export const thingKey = (kind: string, name: string): string => `${kind}:${name}`

/** A report the page can read: a tree that names a thing, with its lists. */
export function isReport(value: any): value is DoctorReport {
  return !!value && typeof value === 'object' && Array.isArray(value.findings) && Array.isArray(value.not_checked)
    && typeof value.status === 'string' && !!thingOf(value.tree) && Array.isArray(value.tree.checks)
}

// ---------------------------------------------------------------- the doctor calls

/** One doctor call: the thing's kind, its handle (none for the home), and whether it is the Full check. */
export interface DoctorCall { kind: ThingKind, name?: string | undefined, full?: boolean | undefined }

/** The family whose `doctor` call checks a kind (a bucket's is `backup doctor`). */
const FAMILY: Partial<Record<ThingKind, string>> = { home: 'home', machine: 'machine', store: 'store', project: 'project', sandbox: 'sandbox', bucket: 'backup' }
/** The doctors that report progress (calls.json `progress: true`); `backup doctor` checks one thing and reports none. */
const PROGRESS: readonly ThingKind[] = ['home', 'machine', 'store', 'project', 'sandbox']

/**
 * Whether a call table (`help --json`) has the records tree, `home get --with
 * tree`. The 2.0 table lists a call's `--with` values under `form.with`, keyed
 * by value (`{ platforms: {...}, tree: {...} }`). A 1.x table has no form.
 */
export function hasTreeRead(calls: readonly unknown[] | null | undefined): boolean {
  const get = (calls ?? []).find((c): c is Record<string, unknown> => !!c && typeof c === 'object' && (c as { family?: unknown }).family === 'home' && (c as { call?: unknown }).call === 'get')
  const values = (get?.form as { with?: unknown } | undefined)?.with
  return !!values && typeof values === 'object' && !Array.isArray(values) && 'tree' in values
}

/**
 * The CLI words of a doctor call, always in the long form (`--detail`), with
 * `--progress` where the call reports it and `--full` on a Full check.
 * @returns null for a kind without a doctor, a missing handle, or `full` on a kind that has none.
 */
export function doctorArgv(call: DoctorCall): string[] | null {
  const family = FAMILY[call.kind]
  if (!family) return null
  if (call.kind === 'home' ? call.name !== undefined : !call.name) return null
  if (call.full && !FULL_KINDS.includes(call.kind)) return null
  return [family, 'doctor', '--detail', ...(PROGRESS.includes(call.kind) ? ['--progress'] : []), ...(call.full ? ['--full'] : []), ...(call.kind === 'home' ? [] : ['--', call.name!])]
}

/** The call as a person would type it (the page's Call line and tips). */
export function doctorCommand(call: DoctorCall): string {
  const family = FAMILY[call.kind] ?? call.kind
  return ['layerfs', family, 'doctor', ...(call.kind === 'home' || !call.name ? [] : [call.name]), ...(call.full ? ['--full'] : [])].join(' ')
}

// ---------------------------------------------------------------- fixes

/** One LayerFS call of a fix line. */
export interface FixStep { family: string, call: string, args: string[] }

/**
 * A fix the page may run for the person, behind one button:
 *   restart  `layerfs machine stop M && layerfs machine start M` (an outdated worker, R2)
 *   start    `layerfs machine start M`
 *   delete   `layerfs project delete S/P`
 *   backup   `layerfs backup create S B` (a Store whose backup is behind)
 * Anything else stays text to copy: a command with an option is never run, so
 * the page never runs a `--force` (an outdated worker with Sandboxes on it).
 */
export interface Fix { kind: 'restart' | 'start' | 'delete' | 'backup', steps: FixStep[] }

const WORD = /^[A-Za-z0-9][\w.:/-]*$/

/**
 * One command of a fix line as a LayerFS call, or null when it is not one
 * (`open -a Docker`), takes an option (`--force`), or is not plain words.
 */
function stepOf(command: string): FixStep | null {
  const words = command.trim().split(/\s+/)
  if (words.length < 4 || words[0] !== 'layerfs') return null
  const [, family, call, ...args] = words
  if (!/^[a-z]+$/.test(family!) || !/^[a-z][a-z-]*$/.test(call!)) return null
  if (!args.every((a) => WORD.test(a))) return null
  return { family: family!, call: call!.replaceAll('-', '_'), args }
}

/**
 * A finding's `fix` (one runnable shell line; two commands joined with `&&`)
 * as a fix on the allow list, or null. The whole line must be on the list:
 * a line with one more command, an option or another call is never run in part.
 */
export function parseFix(line: string | null | undefined): Fix | null {
  if (typeof line !== 'string' || !line.trim() || line.length > 512) return null
  const steps = line.split('&&').map(stepOf)
  if (steps.some((s) => !s)) return null
  const [a, b] = steps as FixStep[]
  const is = (s: FixStep | undefined, family: string, call: string, n: number) => !!s && s.family === family && s.call === call && s.args.length === n
  if (steps.length === 2 && is(a, 'machine', 'stop', 1) && is(b, 'machine', 'start', 1) && a!.args[0] === b!.args[0]) return { kind: 'restart', steps: [a!, b!] }
  if (steps.length !== 1) return null
  if (is(a, 'machine', 'start', 1)) return { kind: 'start', steps: [a!] }
  if (is(a, 'project', 'delete', 1) && /^[^/]+\/[^/]+$/.test(a!.args[0]!)) return { kind: 'delete', steps: [a!] }
  if (is(a, 'backup', 'create', 2)) return { kind: 'backup', steps: [a!] }
  return null
}

/**
 * A refusal's `fix` line the plugin can run behind one button (UI rule: the fix
 * line becomes the one action), or null when it stays a tip to copy. Exactly
 * these lines, nothing in part:
 *   start / stop / restart  `layerfs machine start M`, `… stop M`, `… stop M && … start M`
 *   prune     `layerfs machine prune M --unused`
 *   compact   `layerfs store compact S`
 *   delete    `layerfs project delete S/P`
 *   backup    `layerfs backup create S B`
 *   homeInit  `layerfs home init`
 *   doctor    `layerfs machine doctor M` (the built-in docker whose record has no vm_name): runs that doctor
 *   removeMachine  `layerfs machine remove M` (a record with no vm_name: only its record goes; a built-in comes back fresh)
 *   removeStore    `layerfs store remove S` (a Store record LayerFS cannot place, no `kind`: records only)
 *   removeSandbox  `layerfs sandbox remove B` (a broken Sandbox, an `lfs-` one included: LayerFS's remove)
 *   saveAuth  `layerfs store save-auth <auth> --user <user> --password-stdin && layerfs store update S --auth <auth>`
 *             (a PostgreSQL Store naming no saved login), or the same with the login's name and
 *             user filled in (spec v23: a Store naming a login the home lacks, `missing`): opens
 *             the save-login dialog for S; the page never fills a password in for the person
 *   updateAuth  `layerfs store update S --auth A` (spec v23: a record holding fields 2.0 no
 *             longer writes, `outdated`): store update names its own login again, which
 *             rewrites the record
 *   connectBucket  `layerfs backup connect <name> <url> --access-key <key> --key-stdin` (a bucket
 *             whose key was refused or whose record has none, V21-4): opens the Connect dialog
 *             with the name and URL; connecting again with the same URL rotates the key
 *             (spec v21 `rotated`), the person types the key there
 * Any other option (`--force`, `--mount …`) or command keeps it a tip.
 */
export type RunnableFix =
  | { kind: 'start' | 'stop' | 'restart' | 'prune' | 'doctor' | 'removeMachine', machine: string }
  | { kind: 'compact' | 'removeStore', store: string }
  | { kind: 'removeSandbox', sandbox: string }
  | { kind: 'delete', project: string }
  | { kind: 'backup', store: string, bucket: string }
  | { kind: 'homeInit' }
  | { kind: 'saveAuth', store: string }
  | { kind: 'updateAuth', store: string, auth: string }
  | { kind: 'connectBucket', bucket: string, url: string }
  | { kind: 'chain', steps: ChainStep[] }

/**
 * One step of a fix chain: a single-command fix that is a host call (no dialog).
 * A chain is LayerFS's multi-command fix (`layerfs store remove S && layerfs
 * machine remove M`, a record with no vm_name and a Store on it): the page runs
 * it step by step behind one confirm press, one attempt per step, and stops at
 * the first refusal (UI pass 3 R FAIL 1).
 */
export type ChainStep = Extract<RunnableFix, { kind: 'start' | 'stop' | 'restart' | 'prune' | 'removeMachine' | 'compact' | 'removeStore' | 'removeSandbox' | 'delete' | 'backup' }>
const CHAIN_KINDS = new Set(['start', 'stop', 'prune', 'removeMachine', 'compact', 'removeStore', 'removeSandbox', 'delete', 'backup'])
/** The most steps a chain the page runs may have; a longer line stays a tip to copy. */
const CHAIN_MOST = 4

/** A fix line of two or more commands, each a step the page runs, as a chain; else null. */
export function chainFix(line: string | null | undefined): Extract<RunnableFix, { kind: 'chain' }> | null {
  if (typeof line !== 'string' || line.length > 512 || !line.includes('&&')) return null
  const parts = line.split('&&').map((c) => c.trim())
  if (parts.length < 2 || parts.length > CHAIN_MOST || parts.some((c) => !c)) return null
  const steps: ChainStep[] = []
  for (const part of parts) {
    const step = runnableFix(part)
    if (!step || !CHAIN_KINDS.has(step.kind)) return null
    steps.push(step as ChainStep)
  }
  return { kind: 'chain', steps }
}

/** Whether a fix removes something (its press asks a second, confirming press). */
export const removesSomething = (fix: RunnableFix): boolean =>
  fix.kind === 'chain' ? fix.steps.some(removesSomething)
    : fix.kind === 'delete' || fix.kind === 'prune' || fix.kind === 'removeMachine' || fix.kind === 'removeStore' || fix.kind === 'removeSandbox'

/**
 * The name a chain's confirm asks to be typed (review FAIL-1): the target of its
 * first step that removes data (a Store with its projects, or a project), as the
 * standalone Store Remove asks; null when no step removes data (one confirm press).
 */
export const typedNameOf = (fix: RunnableFix | null | undefined): string | null => {
  if (fix?.kind !== 'chain') return null
  const step = fix.steps.find((s) => s.kind === 'removeStore' || s.kind === 'delete')
  return step?.kind === 'removeStore' ? step.store : step?.kind === 'delete' ? step.project : null
}

/** The machine a fix's last step removes (`… && layerfs machine remove M`), or null. */
export const removesMachine = (fix: RunnableFix | null | undefined): string | null => {
  const last = fix?.kind === 'chain' ? fix.steps[fix.steps.length - 1] : fix
  return last?.kind === 'removeMachine' ? last.machine : null
}

/** LayerFS's fix for a PostgreSQL Store that names no saved login (its placeholders are for the person). */
const SAVE_AUTH = /^layerfs store save-auth <auth> --user (?:<user>|[A-Za-z0-9][\w.@-]*) --password-stdin && layerfs store update (\S+) --auth <auth>$/
/** The same fix for a login the home lacks (spec v23 `missing`): the login's name in both places, a user or `<user>`. */
const SAVE_NAMED = /^layerfs store save-auth (\S+) --user \S+ --password-stdin && layerfs store update (\S+) --auth (\S+)$/

/**
 * A shell line's words, with '…' and "…" quoting and backslash escapes undone
 * (LayerFS shell-quotes a fix's names and URLs); null when a quote is left open.
 */
export function shellWords(line: string): string[] | null {
  const out: string[] = []
  let word: string | null = null
  let quote: '"' | "'" | null = null
  for (let i = 0; i < line.length; i++) {
    const c = line[i]!
    if (quote === "'") { if (c === "'") quote = null; else word = (word ?? '') + c; continue }
    if (quote === '"') {
      if (c === '"') quote = null
      else if (c === '\\' && i + 1 < line.length && '"\\$`'.includes(line[i + 1]!)) word = (word ?? '') + line[++i]
      else word = (word ?? '') + c
      continue
    }
    if (/\s/.test(c)) { if (word !== null) out.push(word); word = null; continue }
    if (word === null) word = ''
    if (c === "'" || c === '"') quote = c
    else if (c === '\\' && i + 1 < line.length) word = (word ?? '') + line[++i]
    else word = (word ?? '') + c
  }
  if (quote) return null
  if (word !== null) out.push(word)
  return out
}

/** LayerFS's credentials fix for a bucket (V21-4): `layerfs backup connect <name> <url> --access-key <key> --key-stdin`, its bucket and URL. */
export function connectBucketFix(line: string | null | undefined): { bucket: string, url: string } | null {
  if (typeof line !== 'string' || line.length > 512 || line.includes('&&')) return null
  const w = shellWords(line.trim())
  if (!w || w.length !== 8 || w[0] !== 'layerfs' || w[1] !== 'backup' || w[2] !== 'connect') return null
  if (w[5] !== '--access-key' || w[7] !== '--key-stdin' || !WORD.test(w[3]!) || !/^(https?:\/\/|r2:\/\/)\S+$/.test(w[4]!)) return null
  return { bucket: w[3]!, url: w[4]! }
}

export function runnableFix(line: string | null | undefined): RunnableFix | null {
  if (typeof line !== 'string' || !line.trim() || line.length > 512) return null
  const connect = connectBucketFix(line)
  if (connect) return { kind: 'connectBucket', ...connect }
  const fix = parseFix(line)
  if (fix) {
    const [a] = fix.steps as [FixStep]
    if (fix.kind === 'delete') return { kind: 'delete', project: a.args[0]! }
    if (fix.kind === 'backup') return { kind: 'backup', store: a.args[0]!, bucket: a.args[1]! }
    return { kind: fix.kind, machine: a.args[0]! }
  }
  const words = line.trim().split(/\s+/)
  const plain = (n: number) => words.length === n && words.slice(3).every((w) => WORD.test(w) && !w.startsWith('-'))
  if (words.join(' ') === 'layerfs home init') return { kind: 'homeInit' }
  const login = SAVE_AUTH.exec(words.join(' '))
  if (login && WORD.test(login[1]!)) return { kind: 'saveAuth', store: login[1]! }
  const named = SAVE_NAMED.exec(words.join(' '))
  if (named && named[1] === named[3] && WORD.test(named[1]!) && WORD.test(named[2]!)) return { kind: 'saveAuth', store: named[2]! }
  if (words[1] === 'store' && words[2] === 'update' && words.length === 6 && words[4] === '--auth' && WORD.test(words[3]!) && WORD.test(words[5]!)) return { kind: 'updateAuth', store: words[3]!, auth: words[5]! }
  if (words[0] !== 'layerfs') return null
  if (words[1] === 'machine' && words[2] === 'doctor' && plain(4)) return { kind: 'doctor', machine: words[3]! }
  if (words[1] === 'machine' && words[2] === 'remove' && plain(4)) return { kind: 'removeMachine', machine: words[3]! }
  if (words[1] === 'machine' && words[2] === 'stop' && plain(4)) return { kind: 'stop', machine: words[3]! }
  if (words[1] === 'store' && words[2] === 'compact' && plain(4)) return { kind: 'compact', store: words[3]! }
  if (words[1] === 'store' && words[2] === 'remove' && plain(4)) return { kind: 'removeStore', store: words[3]! }
  if (words[1] === 'sandbox' && words[2] === 'remove' && plain(4)) return { kind: 'removeSandbox', sandbox: words[3]! }
  if (words[1] === 'machine' && words[2] === 'prune' && words.length === 5 && words[4] === '--unused' && WORD.test(words[3]!)) return { kind: 'prune', machine: words[3]! }
  return chainFix(line)
}

/**
 * The machine a fix line would stop by force and start again (`layerfs machine
 * stop M --force && layerfs machine start M`: an outdated worker with Sandboxes
 * on it), or null. The page shows its button off and the line to copy; nothing runs it.
 */
export function forcedRestart(line: string | null | undefined): string | null {
  const m = /^\s*layerfs\s+machine\s+stop\s+(\S+)\s+--force\s*&&\s*layerfs\s+machine\s+start\s+(\S+)\s*$/.exec(line ?? '')
  return m && m[1] === m[2] ? m[1]! : null
}

/** The fix line that starts a machine stopped on purpose (the page offers it from records, before any doctor ran). */
export const startLine = (machine: string): string => `layerfs machine start ${machine}`

/**
 * A refusal's fix line against the machines' records (UI pass 2 F1): a
 * start, stop or restart of a machine whose own record is broken stands for
 * that machine's own fix (null when it has none), never `machine start`.
 * Anything else is the line as it is.
 */
export function fleetFix(line: string, fleet: readonly { name: string, state?: string | null, fix?: string | null }[] | null | undefined): { line: string, fix: RunnableFix | null } | null {
  const fix = runnableFix(line)
  if (!fix || (fix.kind !== 'start' && fix.kind !== 'stop' && fix.kind !== 'restart')) return { line, fix }
  const m = fleet?.find((x) => x.name === fix.machine)
  if (!m || m.state !== 'broken' || m.fix === line) return { line, fix }
  return m.fix ? { line: m.fix, fix: runnableFix(m.fix) } : null
}

/** A Docker image id cut to `sha256:` and its first twelve hex digits (`sha256:0123456789ab…`); anything else as it is. */
export const shortDigest = (ref: string | null | undefined): string =>
  !ref ? '' : ref.replace(/^(sha256:)?([0-9a-f]{12})[0-9a-f]{52}$/, (_, p: string | undefined, h: string) => `${p ?? ''}${h}…`)

/** A fix line's commands, one per line (tips and the detail's Fix line). */
export const fixCommands = (line: string): string[] => line.split('&&').map((c) => c.trim()).filter(Boolean)

// ---------------------------------------------------------------- the records tree

/** A Sandbox node's Workspace: its handle and Branch (an older tree names the handle alone). */
export type TreeWorkspace = string | { workspace: string, branch?: string | null }

/**
 * `home get --with tree --detail`: one node of `tree`, from records only. A
 * machine has `machine, platform, cpus, memory_bytes`; a Store `store, engine,
 * projects` (bare names; a project's handle is `<store>/<project>`), a
 * PostgreSQL Store also `connections`; a Sandbox `sandbox, project, image,
 * workspaces`; a bucket `bucket, provider`. `state` and its companion (`op`,
 * `blocked_by`, `reason` with `fix`, `observed_at`) are there when it is not
 * ready. A state's `reason` is its own list of codes, not a check's
 * `timeout | busy | lost`.
 */
export interface TreeNode {
  state?: string | null
  op?: { name: string, phase: string | null, started_at: string } | null
  blocked_by?: BlockedBy | null
  reason?: string | null
  fix?: string | null
  observed_at?: string | null
  platform?: string | null
  cpus?: number | null
  memory_bytes?: number | null
  engine?: string | null
  /** A PostgreSQL Store: its connections, `{ machine, url }` each (`machine` null is the home's own); null on a sqlite Store. */
  connections?: unknown[] | null
  provider?: string | null
  project?: string | null
  image?: string | null
  stores?: TreeNode[] | null
  sandboxes?: TreeNode[] | null
  projects?: string[] | null
  workspaces?: TreeWorkspace[] | null
  workspaces_not_listed?: boolean | null
  [key: string]: unknown
}

export interface HomeTree { machines?: TreeNode[], stores?: TreeNode[], buckets?: TreeNode[] }

/** `GET /api/doctor/tree`. */
export interface TreeAnswer {
  /** When the host read it (ms). */
  at: number
  home: { home: string, state: string, version: string | null, generation: number | null, reason: string | null, fix: string | null }
  tree: HomeTree
}

// ---------------------------------------------------------------- runs and fixes on the wire

/** Why a doctor call gave no report: it timed out, was refused Busy, or its reply was lost (any other failure). */
export interface RunFailure { reason: CouldNotReason, code: string | null }

/** One doctor call the host runs, and the `doctor-run` event's data. */
export interface DoctorRun {
  /** The caller's id for it (the page makes it, so an event can never outrun its POST). */
  id: string
  kind: ThingKind
  name: string | null
  full: boolean
  /** Runs started together (Check all, the checks after a fix) share a group. */
  group: string | null
  state: 'queued' | 'running' | 'done' | 'failed' | 'stopped'
  started: number
  ended: number | null
  /** On a `running` event: the thing that just finished. */
  progress?: DoctorProgress
  /** On `done`. */
  report?: DoctorReport
  /** On `failed`. */
  failure?: RunFailure
}

/** One fix the host runs, and the `doctor-fix` event's data. */
export interface DoctorFixRun {
  id: string
  kind: ThingKind
  name: string
  fix: Fix['kind']
  /** The step running now (0-based), and each step's call word (`stop`, `start`, …). */
  step: number
  steps: string[]
  state: 'running' | 'done' | 'refused' | 'lost'
  /** On `refused`: LayerFS's refusal code and sentence (for the tip). */
  refusal?: { code: string, message: string }
}

/** `GET /api/doctor/results`: what the host holds for a page that opens or reconnects. */
export interface DoctorResults {
  /** The last outcome of each doctor call the host ran, one per target. */
  last: DoctorRun[]
  running: DoctorRun[]
  fixes: DoctorFixRun[]
}

// ---------------------------------------------------------------- the Overview's count

/** What the Overview's Doctor button counts: the doctors' open findings, and when the newest of them answered. */
export interface DoctorCount {
  findings: DoctorFinding[]
  /** Epoch ms of the newest report counted; null before any doctor answered. */
  at: number | null
}

/** Every thing a report's tree checked (coverage other than `none`), by key. */
function coveredBy(node: unknown, into: Set<string>): void {
  if (!node || typeof node !== 'object') return
  const thing = thingOf(node)
  if (thing && (node as { coverage?: unknown }).coverage !== 'none') into.add(thingKey(thing.kind, thing.name))
  for (const list of Object.keys(CHILD_LISTS)) {
    const children = (node as Record<string, unknown>)[list]
    if (Array.isArray(children)) for (const child of children) coveredBy(child, into)
  }
}

/**
 * The open findings across the doctors' last reports (`GET /api/doctor/results`
 * `last`): newest first, a thing's findings come from the newest report that
 * checked it, so an older report cannot bring back what a newer one cleared.
 * Notices are left out: they are not problems.
 */
export function openFindings(last: readonly DoctorRun[]): DoctorCount {
  const done = last.filter((r) => r.state === 'done' && r.report && typeof r.ended === 'number').sort((a, b) => b.ended! - a.ended!)
  const covered = new Set<string>()
  const seen = new Set<string>()
  const findings: DoctorFinding[] = []
  for (const run of done) {
    const report = run.report!
    for (const f of Array.isArray(report.findings) ? report.findings : []) {
      const thing = thingOf(f)
      const key = thing ? thingKey(thing.kind, thing.name) : ''
      if (f.severity === 'notice' || (key && covered.has(key)) || seen.has(`${key}|${f.check}`)) continue
      seen.add(`${key}|${f.check}`)
      findings.push(f)
    }
    const mine = new Set<string>()
    coveredBy(report.tree, mine)
    for (const key of mine) covered.add(key)
  }
  return { findings, at: done.length ? done[0].ended : null }
}
