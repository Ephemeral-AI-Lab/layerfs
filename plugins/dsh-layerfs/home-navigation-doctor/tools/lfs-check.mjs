#!/usr/bin/env node
// LayerFS state before/after a test (docs/specs/backend-e2e-tests.md §3.4): a
// read-only snapshot of a dev server's PRIVATE LayerFS home and plugin call log,
// and the difference between two snapshots.
//
//   node tools/lfs-check.mjs [<dev dir>] [--save <file>] [--diff <before.json>] [--json]
//
// <dev dir> defaults to $DSH_LAYERFS_DEV_DIR, else .dev. The home is
// <dev dir>/layerfs-home (never ~/.layerfs); the pinned CLI is $DSH_LAYERFS_CLI,
// else the newest runtime installed in ~/.layerfs (scripts/runtime-pin.mjs, read only).
// Every CLI call made here is a read (`list`, `get`, `doctor`); none is logged in the
// plugin's call log (that log is written by the dev server's host only).
//
// snapshot(devDir) → { at, home, machines, stores, projects, sandboxes, workspaces, branches,
//   mounts, doctor, daemonLogs, calls, errors }
// diff(before, after) → { failures: [{ kind, message }], notes: [{ kind, message }] }
import { execFileSync } from 'node:child_process'
import { closeSync, existsSync, openSync, readFileSync, readSync, readdirSync, realpathSync, statSync, writeFileSync } from 'node:fs'
import { homedir } from 'node:os'
import { join, resolve } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { runtimePin } from '../scripts/runtime-pin.mjs'
import { isWrite } from '../src/shared/logs.ts'

/** A call-log line is a write unless the Logs page counts it as a read (one rule for both). */
export const isWriteCall = (argv) => isWrite((argv ?? []).map(String))

/** Daemon log lines that are problems. */
const LOG_ERROR = /\bpanic(?:ked)?\b|\bERROR\b|\bFATAL\b|\bAbort(?:ed)?\b/
/**
 * Daemon log lines recorded but not failures. ConnectionFailure: the native daemon logs one
 * for every client connection that closes after its reply (cause `CloseFailed … UnexpectedEof`,
 * request `end-session`), i.e. on every ordinary CLI read; reported as a note with the count.
 */
const LOG_NOTED = [{ re: /^ConnectionFailure \{/, kind: 'ConnectionFailure' }]

const sizeOf = (path) => { try { return statSync(path).size } catch { return null } }

/** The pinned CLI the dev server uses (env, else newest installed runtime; never with the private home's LAYERFS_HOME). */
export function pinnedCli(env = process.env) {
  if (env.DSH_LAYERFS_CLI) return env.DSH_LAYERFS_CLI
  const { LAYERFS_HOME: _ignored, ...rest } = env
  const pin = runtimePin(rest)
  if (!pin) throw new Error('lfs-check: no LayerFS CLI (set DSH_LAYERFS_CLI)')
  return pin.cli
}

/** The private home of a dev dir; refuses the user's home. */
export function privateHome(devDir) {
  const home = resolve(devDir, 'layerfs-home')
  const user = resolve(homedir(), '.layerfs')
  const real = (p) => { try { return realpathSync(p) } catch { return p } }
  if (real(home) === real(user) || real(home).startsWith(`${real(user)}/`)) throw new Error(`lfs-check: refusing the user's LayerFS home ${user}`)
  return home
}

/** The plugin call log (src/host/layerfs/call-log.ts: `$DSH_HOME/layerfs/trace/calls.jsonl`; dev DSH_HOME = <dev dir>/state). */
export const callLogPath = (devDir) => join(resolve(devDir), 'state', 'layerfs', 'trace', 'calls.jsonl')

/** Files named like logs under the home, skipping Workspace mount trees. */
function daemonLogFiles(home, mounted) {
  const out = []
  const walk = (dir, depth) => {
    if (depth > 8) return
    let entries = []
    try { entries = readdirSync(dir, { withFileTypes: true }) } catch { return }
    for (const e of entries) {
      const path = join(dir, e.name)
      if (e.isDirectory()) {
        if (e.name === 'workspaces' || e.name === 'store' || mounted.has(path)) continue
        walk(path, depth + 1)
      } else if (e.isFile() && /(^|[.-])log(\.\d+)?$|\.log(\.\d+)?$|^daemon\.(out|err)$/.test(e.name)) out.push(path)
    }
  }
  walk(home, 0)
  return out.sort()
}

/** Problem lines of one log file: counts by normalized text (addresses, ports and numbers folded). */
function scanLog(path) {
  const errors = {}, noted = {}
  let text = ''
  try { text = readFileSync(path, 'utf8') } catch { return { size: null, errors, noted } }
  for (const line of text.split('\n')) {
    if (!line) continue
    const n = LOG_NOTED.find((x) => x.re.test(line))
    if (n) { noted[n.kind] = (noted[n.kind] ?? 0) + 1; continue }
    if (LOG_ERROR.test(line)) {
      const key = line.replace(/\d+/g, 'N').slice(0, 300)
      errors[key] = (errors[key] ?? 0) + 1
    }
  }
  return { size: Buffer.byteLength(text), errors, noted }
}

/** Every item of a paged CLI listing. */
function listAll(run, args) {
  const items = []
  let pos = null
  for (let i = 0; i < 100; i++) {
    const page = run([...args, ...(pos ? ['--page-pos', String(pos)] : [])])
    items.push(...(page?.items ?? []))
    pos = page?.next_page_pos ?? null
    if (!pos || page?.has_more === false) break
  }
  return items
}

/**
 * Snapshot a dev dir's private LayerFS home and call log (read only).
 * @param devDir - the dev server's `DSH_LAYERFS_DEV_DIR`.
 * @param opts.cli - the pinned CLI (default {@link pinnedCli}).
 * @param opts.sandboxDetail - `sandbox get` each Sandbox for its Workspaces (default true, at most 40).
 */
export async function snapshot(devDir, { cli = pinnedCli(), sandboxDetail = true } = {}) {
  const home = privateHome(devDir)
  if (!existsSync(home)) throw new Error(`lfs-check: no LayerFS home at ${home}`)
  const errors = []
  const env = { ...process.env, LAYERFS_HOME: home }
  delete env.LAYERFS_LOGS_TAG
  const run = (args) => {
    try {
      const out = execFileSync(cli, args, { env, encoding: 'utf8', timeout: 60_000, maxBuffer: 64 * 1024 * 1024, stdio: ['ignore', 'pipe', 'pipe'] })
      return JSON.parse(out)
    } catch (e) {
      const stderr = String(e.stderr ?? '').trim().slice(0, 400)
      const stdout = String(e.stdout ?? '').trim()
      let refusal = null
      try { refusal = JSON.parse(stdout || stderr) } catch {}
      errors.push({ argv: args.join(' '), status: e.status ?? null, code: refusal?.code ?? null, message: refusal?.message ?? (stderr || e.message) })
      return null
    }
  }
  const machines = (run(['machine', 'list'])?.machines ?? []).map((m) => ({ name: m.name, platform: m.platform ?? null, state: m.state ?? null }))
  const stores = {}
  for (const s of run(['store', 'list'])?.items ?? []) stores[s.name] = { kind: s.kind ?? null, machine: s.machine ?? null, projects: s.projects ?? [] }
  const projects = {}
  for (const p of listAll(run, ['project', 'list'])) projects[p.project ?? `${p.store}/${p.name}`] = { machine: p.machine ?? null, id: p.id ?? null }
  const sandboxes = {}, workspaces = {}
  const sbx = listAll(run, ['sandbox', 'list'])
  for (const s of sbx) sandboxes[s.name ?? s.id] = { project: s.project ?? null, machine: s.machine ?? null, state: s.state ?? null }
  if (sandboxDetail) {
    for (const s of sbx.slice(0, 40)) {
      if (s.state && s.state !== 'running') continue
      const got = run(['sandbox', 'get', s.name ?? s.id])
      for (const w of got?.workspaces ?? []) workspaces[`${s.name ?? s.id}:${w.name ?? w.namespace}`] = { branch: w.branch ?? null, mounted: !!w.mounted, dirty: (w.dirty?.entries ?? 0) + (w.dirty?.inodes ?? 0) }
    }
  }
  const branches = {}
  for (const project of Object.keys(projects)) branches[project] = listAll(run, ['branch', 'list', project]).map((b) => b.name).sort()
  let mounts = []
  try { mounts = execFileSync('mount', [], { encoding: 'utf8' }).split('\n').filter((l) => l.includes(home)).map((l) => l.trim()) } catch (e) { errors.push({ argv: 'mount', message: e.message }) }
  const mountPoints = new Set(mounts.map((l) => l.match(/ on (.+?) \(/)?.[1]).filter(Boolean))
  const doctorReport = run(['doctor'])
  const doctor = (doctorReport?.findings ?? []).map((f) => ({
    severity: f.severity ?? 'error',
    entity: f.entity ? `${f.entity.kind ?? ''}:${f.entity.name ?? ''}` : '',
    message: String(f.message ?? ''),
  }))
  const daemonLogs = {}
  for (const file of daemonLogFiles(home, mountPoints)) daemonLogs[file] = scanLog(file)
  const logPath = callLogPath(devDir)
  const calls = { path: logPath, size: sizeOf(logPath) ?? 0, rotated: sizeOf(join(logPath, '..', 'calls.1.jsonl')) }
  return { at: Date.now(), home, cli, machines, stores, projects, sandboxes, workspaces, branches, mounts, doctor, daemonLogs, calls, errors }
}

/** Call-log lines written after `before` (reads from the start when the log was reset or rotated). */
export function callsSince(before, after) {
  const path = after.calls.path
  const size = sizeOf(path) ?? 0
  const start = size >= before.calls.size && after.calls.rotated === before.calls.rotated ? before.calls.size : 0
  const end = Math.max(after.calls.size, start)
  if (end <= start) return []
  const fd = openSync(path, 'r')
  try {
    const buf = Buffer.alloc(end - start)
    readSync(fd, buf, 0, buf.length, start)
    return buf.toString('utf8').split('\n').filter(Boolean).map((l) => { try { return JSON.parse(l) } catch { return { malformed: l.slice(0, 200) } } })
  } finally { closeSync(fd) }
}

const keysAdded = (a, b) => Object.keys(b ?? {}).filter((k) => !(k in (a ?? {})))
const keysRemoved = (a, b) => Object.keys(a ?? {}).filter((k) => !(k in (b ?? {})))
/** Doctor findings compared without the numbers in their text (counts and ports change between reads). */
const findingKey = (f) => `${f.severity}|${f.entity}|${f.message.replace(/\d+(\.\d+)?/g, 'N')}`

/**
 * What changed between two snapshots.
 * @param opts.allowOutcomes - `(line) => reason|null`: non-ok call outcomes to report as notes.
 * @returns {{ failures: {kind, message}[], notes: {kind, message}[], calls: object[] }}
 */
export function diff(before, after, { allowOutcomes = () => null } = {}) {
  const failures = [], notes = []
  const fail = (kind, message) => failures.push({ kind, message })
  const note = (kind, message) => notes.push({ kind, message })
  const machinesBefore = new Map((before.machines ?? []).map((m) => [m.name, m]))
  for (const m of after.machines ?? []) {
    const was = machinesBefore.get(m.name)
    if (!was) fail('machine', `new machine ${m.name}`)
    else if (was.state !== m.state) fail('machine', `machine ${m.name} ${was.state} → ${m.state}`)
  }
  for (const s of keysAdded(before.stores, after.stores)) fail('store', `new Store ${s}`)
  for (const s of keysRemoved(before.stores, after.stores)) fail('store', `Store ${s} disappeared`)
  for (const p of keysAdded(before.projects, after.projects)) fail('project', `new project ${p}`)
  for (const p of keysRemoved(before.projects, after.projects)) fail('project', `project ${p} disappeared`)
  for (const s of keysAdded(before.sandboxes, after.sandboxes)) fail('sandbox', `new Sandbox ${s} (${after.sandboxes[s].project})`)
  for (const s of keysRemoved(before.sandboxes, after.sandboxes)) fail('sandbox', `Sandbox ${s} disappeared`)
  for (const [s, v] of Object.entries(after.sandboxes)) {
    const was = before.sandboxes[s]
    if (was && was.state !== v.state) fail('sandbox', `Sandbox ${s} ${was.state} → ${v.state}`)
  }
  for (const w of keysAdded(before.workspaces, after.workspaces)) fail('workspace', `new Workspace ${w} (branch ${after.workspaces[w].branch})`)
  for (const w of keysRemoved(before.workspaces, after.workspaces)) fail('workspace', `Workspace ${w} disappeared`)
  for (const [w, v] of Object.entries(after.workspaces)) {
    const was = before.workspaces[w]
    if (!was) continue
    if (was.mounted !== v.mounted) fail('workspace', `Workspace ${w} ${was.mounted ? 'unmounted' : 'mounted'}`)
    if (was.branch !== v.branch) fail('workspace', `Workspace ${w} branch ${was.branch} → ${v.branch}`)
    if (was.dirty !== v.dirty) fail('workspace', `Workspace ${w} dirty ${was.dirty} → ${v.dirty}`)
  }
  for (const [project, names] of Object.entries(after.branches)) {
    const was = new Set(before.branches[project] ?? [])
    const now = new Set(names)
    for (const b of names) if (!was.has(b)) fail('branch', `new branch ${project}@${b}`)
    for (const b of was) if (!now.has(b)) fail('branch', `branch ${project}@${b} disappeared`)
  }
  const mountsBefore = new Set(before.mounts)
  for (const m of after.mounts) if (!mountsBefore.has(m)) fail('mount', `new mount ${m}`)
  const mountsAfter = new Set(after.mounts)
  for (const m of before.mounts) if (!mountsAfter.has(m)) fail('mount', `mount gone ${m}`)
  const findingsBefore = new Set(before.doctor.map(findingKey))
  for (const f of after.doctor) {
    if (findingsBefore.has(findingKey(f))) continue
    if (f.severity === 'warning') note('doctor', `new doctor warning ${f.entity}: ${f.message}`)
    else fail('doctor', `new doctor ${f.severity} ${f.entity}: ${f.message}`)
  }
  for (const [file, log] of Object.entries(after.daemonLogs)) {
    const was = before.daemonLogs[file] ?? { errors: {}, noted: {} }
    for (const [line, n] of Object.entries(log.errors)) if (n > (was.errors[line] ?? 0)) fail('daemon-log', `${file}: ${n - (was.errors[line] ?? 0)}× ${line}`)
    for (const [kind, n] of Object.entries(log.noted)) if (n > (was.noted[kind] ?? 0)) note('daemon-log', `${file.replace(after.home, '<home>')}: +${n - (was.noted[kind] ?? 0)} ${kind} lines`)
  }
  const errorsBefore = new Set(before.errors.map((e) => `${e.argv}|${e.code}`))
  for (const e of after.errors) if (!errorsBefore.has(`${e.argv}|${e.code}`)) fail('lfs-check', `CLI ${e.argv}: ${e.code ?? e.status} ${e.message}`)
  const calls = callsSince(before, after)
  for (const line of calls) {
    if (line.malformed) { fail('call-log', `malformed line ${line.malformed}`); continue }
    const words = (line.argv ?? []).join(' ')
    if (line.outcome && line.outcome !== 'ok') {
      const reason = allowOutcomes(line)
      if (reason) note('call-log', `${line.outcome}: ${words} (allowed: ${reason})`)
      else fail('call-log', `${line.outcome}: ${words} [${line.tag ?? ''}]`)
    }
    if (isWriteCall(line.argv)) fail('call-log', `write call during a read-only window: ${words} → ${line.outcome} [${line.tag ?? ''}]`)
  }
  return { failures, notes, calls }
}

/** One line per difference, failures first. */
export function formatDiff({ failures, notes, calls }) {
  const out = [`lfs-check: ${failures.length} failure(s), ${notes.length} note(s), ${calls?.length ?? 0} CLI call(s) logged`]
  for (const f of failures) out.push(`  FAIL ${f.kind}: ${f.message}`)
  for (const n of notes) out.push(`  note ${n.kind}: ${n.message}`)
  return out.join('\n')
}

// CLI entry.
if (import.meta.url === pathToFileURL(process.argv[1] ?? '').href) {
  const args = process.argv.slice(2)
  const opt = (name) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : undefined }
  const positional = args.filter((a, i) => !a.startsWith('--') && !['--save', '--diff'].includes(args[i - 1]))
  const devDir = positional[0] ?? process.env.DSH_LAYERFS_DEV_DIR ?? fileURLToPath(new URL('../.dev', import.meta.url))
  const snap = await snapshot(devDir)
  if (opt('--save')) writeFileSync(opt('--save'), JSON.stringify(snap, null, 2))
  if (opt('--diff')) {
    const result = diff(JSON.parse(readFileSync(opt('--diff'), 'utf8')), snap)
    console.log(args.includes('--json') ? JSON.stringify(result, null, 2) : formatDiff(result))
    process.exit(result.failures.length ? 1 : 0)
  }
  if (args.includes('--json') || !opt('--save')) {
    if (args.includes('--json')) console.log(JSON.stringify(snap, null, 2))
    else {
      console.log(`home ${snap.home}`)
      console.log(`stores ${Object.keys(snap.stores).length} · projects ${Object.keys(snap.projects).length} · sandboxes ${Object.keys(snap.sandboxes).length} · workspaces ${Object.keys(snap.workspaces).length} · branches ${Object.values(snap.branches).reduce((n, b) => n + b.length, 0)} · mounts ${snap.mounts.length}`)
      console.log(`doctor ${snap.doctor.length} finding(s): ${snap.doctor.map((f) => f.severity).join(', ') || 'none'}`)
      console.log(`daemon logs ${Object.keys(snap.daemonLogs).length}: ${Object.values(snap.daemonLogs).reduce((n, l) => n + Object.values(l.errors).reduce((a, b) => a + b, 0), 0)} error line(s)`)
      console.log(`call log ${snap.calls.path} (${snap.calls.size} bytes)`)
      if (snap.errors.length) console.log(`CLI errors:\n${snap.errors.map((e) => `  ${e.argv}: ${e.code ?? e.status} ${e.message}`).join('\n')}`)
    }
  }
}
