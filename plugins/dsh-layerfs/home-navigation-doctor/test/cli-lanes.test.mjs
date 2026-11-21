// The pinned-CLI SDK under load and failure (src/host/layerfs/cli.ts): admission lanes keep short
// reads clear of long work and terminals, reads have a default time budget, a timed-out call ends
// its whole process group and settles, every call is one attempt (a Store-lease refusal is not run again),
// argument shapes that would shift or invent CLI words are refused, the native daemon record is
// retried after a failure (service.ts), and preset long work gets a long budget (presets/driver.ts).
//   node --test test/cli-lanes.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { build } from 'esbuild'
import { appendFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { pathToFileURL } from 'node:url'

const root = mkdtempSync(join(tmpdir(), 'dsh-cli-lanes-'))
process.on('exit', () => rmSync(root, { recursive: true, force: true }))
await build({
  stdin: {
    contents: "export * from './src/host/layerfs/cli.ts'\nexport { presetTimeout, PRESET_TIMEOUT_MS, PRESET_LONG_TIMEOUT_MS } from './src/host/presets/driver.ts'\nexport { LayerfsService } from './src/host/service.ts'\nexport { Context } from '@deepseek-ai/cordis'\n",
    resolveDir: process.cwd(), loader: 'ts',
  },
  outfile: join(root, 'cli.mjs'), bundle: true, platform: 'node', format: 'esm', target: 'node24',
})
const C = await import(pathToFileURL(join(root, 'cli.mjs')))

const a = (name, kind, optional) => ({ name, kind, ...(optional ? { optional } : {}) })
const o = (name, kind) => ({ name, kind })
const CALLS = {
  'machine.list': [[], []],
  'machine.get': [[a('name', 'name')], []],
  'machine.update': [[a('name', 'name')], [o('daemon', 'text'), o('cpus', 'int')]],
  'workspace.exec': [[a('workspace', 'text'), a('command', 'text')], [o('cwd', 'text'), o('stream', 'bool'), o('stdout', 'function'), o('stderr', 'function'), o('on_progress', 'function')]],
  'store.move': [[a('store', 'name'), a('machine', 'name')], [o('on_progress', 'function')]],
  'file.list': [[a('project', 'project'), a('branch', 'name'), a('path', 'text')], [o('at', 'name'), o('limit', 'int'), o('cursor', 'text')]],
  'file.write': [[a('project', 'project'), a('branch', 'name'), a('path', 'text'), a('content', 'content')], [o('mode', 'text'), o('expected', 'name'), o('message', 'text')]],
  // a made-up shape with two optional positionals: the seam's shift check (no 2.0 call has two)
  'project.stats': [[a('project', 'project'), a('branch', 'name', true), a('commit', 'name', true)], []],
  'commit.diff': [[a('project', 'project'), a('ref', 'name')], [o('from', 'name'), o('path', 'text'), o('limit', 'int'), o('cursor', 'text')]],
  // Stage 1 (docs/specs/sandbox-images.md §3): images, prune, and create taking an image reference with progress
  'machine.images': [[a('name', 'name')], []],
  'machine.prune': [[a('name', 'name'), a('image', 'text', true)], [o('unused', 'bool')]],
  'sandbox.create': [[a('project', 'project'), a('name', 'name')], [o('machine', 'name'), o('image', 'image'), o('on_progress', 'function')]],
  // monitoring reads (layerfs-project#178)
  'machine.log': [[a('machine', 'name')], [o('limit', 'int'), o('cursor', 'text')]],
  'machine.stats': [[a('machine', 'name')], []],
  'sandbox.log': [[a('sandbox', 'name')], [o('limit', 'int'), o('cursor', 'text')]],
  'sandbox.files': [[a('sandbox', 'name'), a('path', 'text', true)], [o('depth', 'int'), o('limit', 'int'), o('cursor', 'text'), o('byte_offset', 'int'), o('byte_length', 'int')]],
  'machine.files': [[a('machine', 'name'), a('path', 'text', true)], [o('depth', 'int'), o('cursor', 'text'), o('limit', 'int'), o('byte_offset', 'int'), o('byte_length', 'int')]],
}
/** The 2.0 `--with` values a row lists (`form.with`, keyed by value). */
const FORMS = { 'home.get': { with: { platforms: {}, tree: {} } }, 'machine.get': { with: { live: {} } }, 'machine.stats': { with: { space: {} } } }
const calls = [...new Set([...C.REQUIRED_CALLS, ...Object.keys(CALLS)])].map((name) => {
  const [family, call] = name.split('.'), [args, options] = CALLS[name] ?? [[], []]
  return { family, call, args, options, ...(FORMS[name] ? { form: FORMS[name] } : {}) }
})

/** A fake pinned CLI: discovery, a call log, held calls released by a file, and scripted refusals. */
function fakeCli(dir) {
  mkdirSync(join(dir, 'home'), { recursive: true })
  writeFileSync(join(dir, 'home', 'layerfs.json'), '{}')
  writeFileSync(join(dir, 'help.json'), JSON.stringify({ calls, commands: [], switches: [] }))
  const cli = join(dir, 'layerfs')
  writeFileSync(cli, `#!/usr/bin/env node
import { appendFileSync, existsSync, readFileSync, writeFileSync, writeSync } from 'node:fs'
const dir = ${JSON.stringify(dir)}, a = process.argv.slice(2), op = a[0] + '.' + a[1]
if (a[0] === '--version') { console.log(JSON.stringify({ version: '2.0.0', commit: 'a'.repeat(40), dirty: false, features: [] })); process.exit(0) }
if (a[0] === 'help') { process.stdout.write(readFileSync(dir + '/help.json')); process.exit(0) }
appendFileSync(dir + '/calls.log', JSON.stringify(a) + '\\n')
const answer = (value) => { console.log(JSON.stringify(value)); process.exit(0) }
const failing = (counter, env) => {
  const n = existsSync(dir + '/' + counter) ? Number(readFileSync(dir + '/' + counter, 'utf8')) : 0
  writeFileSync(dir + '/' + counter, String(n + 1))
  return n < Number(process.env[env] ?? 0)
}
const inspect = ['machine.log', 'sandbox.log', 'sandbox.files', 'machine.files'].includes(op)
if (op === 'workspace.exec' && a.at(-1) === 'own 125') { console.log(JSON.stringify({ exit_code: 125, stdout: '', stderr: 'x', exec: 'exec-1' })); process.exit(125) }
if (op === 'workspace.exec' && a.at(-1) === 'refused 125') { process.stderr.write(JSON.stringify({ error: 'Busy', message: 'a command runs', fix: 'layerfs workspace cancel ws' }) + '\\n'); process.exit(125) }
if (op === 'workspace.exec' && a.at(-1) === 'lost 125') process.exit(125)
if (op === 'workspace.exec' || op === 'store.move') setInterval(() => { if (existsSync(dir + '/release')) answer({ held: op }) }, 10)
else if (inspect && process.env.FAKE_INSPECT_RACE) { process.stderr.write(JSON.stringify({ error: 'Failed', message: 'Os { code: 2, kind: NotFound, message: "No such file or directory" }' }) + '\\n'); process.exit(125) }
else if (inspect && existsSync(dir + '/hold-inspect')) { appendFileSync(dir + '/held.pids', process.pid + '\\n'); setInterval(() => { if (existsSync(dir + '/release')) answer({ held: op, argv: a }) }, 10) }
else if ((op === 'file.list' || op === 'file.write') && failing('race.count', 'FAKE_RACE_FAILS')) {
  process.stderr.write(JSON.stringify({ error: 'Failed', message: 'Os { code: 2, kind: NotFound, message: "No such file or directory" }' }) + '\\n'); process.exit(125)
} else if (op === 'machine.update' && failing('update.count', 'FAKE_UPDATE_FAILS')) {
  process.stderr.write(JSON.stringify({ error: 'Failed', message: 'daemon not reachable' }) + '\\n'); process.exit(125)
} else if (op === 'machine.get' && process.env.FAKE_SLOW_MS) setTimeout(() => answer({ slow: true }), Number(process.env.FAKE_SLOW_MS))
else if (op === 'sandbox.create' && a.includes('--progress')) { writeSync(2, JSON.stringify({ phase: 'send', done: 1, total: 2 }) + '\\n'); answer({ argv: a }) }
else if (op === 'machine.list' && process.env.FAKE_SLOW_LIST_MS) setTimeout(() => answer({ items: [] }), Number(process.env.FAKE_SLOW_LIST_MS))
else answer({ argv: a })
`, { mode: 0o700 })
  return { cli, home: join(dir, 'home'), log: () => existsSync(join(dir, 'calls.log')) ? readFileSync(join(dir, 'calls.log'), 'utf8').trim().split('\n').map(JSON.parse) : [] }
}
let n = 0
const fresh = () => { const dir = join(root, `t${n++}`); mkdirSync(dir); return { dir, ...fakeCli(dir) } }

test('laneOf: short calls, long jobs, Workspace commands and cancel take separate lanes', () => {
  // Machine probes poll an unreachable VM for their whole budget: they never take a page read's slot.
  for (const argv of [['machine', 'list'], ['machine', 'get', '--', 'vm'], ['machine', 'images', '--', 'vm']]) assert.equal(C.laneOf(argv).name, 'probes', argv.join(' '))
  assert.equal(C.laneOf(['store', 'list']).name, 'calls')
  assert.equal(C.laneOf(['file', 'write', '--', 'p/a', 'main', '/x']).name, 'calls')
  for (const argv of [['store', 'move'], ['store', 'compact'], ['backup', 'create'], ['backup', 'restore'], ['project', 'init'], ['project', 'export'], ['machine', 'create'], ['machine', 'start'], ['machine', 'stop'], ['machine', 'update'], ['sandbox', 'create'], ['doctor', 'store', 'integrity']]) {
    assert.equal(C.laneOf(argv).name, 'jobs', argv.join(' '))
  }
  assert.equal(C.laneOf(['workspace', 'exec']).name, 'exec')
  assert.equal(C.laneOf(['workspace', 'cancel']).name, 'cancel')
})

test('short reads answer while Workspace commands and long jobs fill their lanes', { timeout: 30000 }, async (t) => {
  const f = fresh()
  t.after(() => writeFileSync(join(f.dir, 'release'), ''))
  const lfs = C.client({ cli: f.cli, home: f.home })
  await lfs.machine.list()
  const held = []
  for (let i = 0; i < 8; i++) held.push(lfs.workspace.exec(`sb${i}:ws`, 'sleep 600', { stdout: () => {} }))
  for (let i = 0; i < 4; i++) held.push(lfs.store.move(`s${i}`, 'vm'))
  // all 12 are running at once: none queued behind another lane
  for (let i = 0; i < 300 && f.log().length < 13; i++) await new Promise((r) => setTimeout(r, 20))
  assert.equal(f.log().filter((argv) => argv[0] !== 'machine').length, 12, 'eight Workspace commands and four jobs run together')
  const started = Date.now()
  assert.deepEqual((await lfs.machine.list()).argv.slice(0, 2), ['machine', 'list'])
  assert.ok(Date.now() - started < 10000, `machine.list answered (the held calls never end on their own) in ${Date.now() - started} ms`)
  writeFileSync(join(f.dir, 'release'), '')
  assert.equal((await Promise.all(held)).length, 12)
})

test('reads get a time budget (timeout_ms overrides it); a timed-out call is transport-unknown', { timeout: 20000 }, async (t) => {
  const f = fresh()
  const lfs = C.client({ cli: f.cli, home: f.home })
  assert.equal(C.READ_TIMEOUT_MS, 120_000)
  t.after(() => { delete process.env.FAKE_SLOW_MS })
  // discovery (two spawns, slow under a parallel test run) first, so the clock below times only the call
  process.env.FAKE_SLOW_MS = '0'
  await lfs.machine.get('vm')
  process.env.FAKE_SLOW_MS = '5000'
  const started = Date.now()
  await assert.rejects(lfs.machine.get('vm', { timeout_ms: 300 }), (error) => error.code === 'Unknown' && error.transport === 'transport-unknown' && error.detail.timeout === 300)
  // well under the fake's 5 s
  assert.ok(Date.now() - started < 4000, `settled in ${Date.now() - started} ms`)
  process.env.FAKE_SLOW_MS = '200'
  assert.deepEqual(await lfs.machine.get('vm', { timeout_ms: 0 }), { slow: true }, '0: no budget')
  await assert.rejects(lfs.machine.get('vm', { timeout_ms: -1 }), (error) => error.code === 'InvalidInput')
  assert.ok(!f.log().some((argv) => argv.some((word) => word.includes('timeout'))), 'timeout_ms never reaches the CLI')
})

test('machine probes default to the short budget: a dead VM answers unreachable in seconds, not minutes', { timeout: 40000 }, async (t) => {
  const f = fresh()
  const lfs = C.client({ cli: f.cli, home: f.home })
  assert.ok(C.PROBE_TIMEOUT_MS < C.READ_TIMEOUT_MS)
  assert.deepEqual([...C.PROBES].sort(), ['machine.get', 'machine.images', 'machine.list'])
  // sandbox-images.md §4.1: machine.load_image and machine.prune run as long as their copy, in the jobs lane
  for (const call of ['load_image', 'prune']) assert.equal(C.laneOf(['machine', call]).name, 'jobs', call)
  t.after(() => { delete process.env.FAKE_SLOW_LIST_MS })
  await lfs.machine.get('vm') // discovery first, so the clock times only the probe
  process.env.FAKE_SLOW_LIST_MS = String(C.PROBE_TIMEOUT_MS + 20000)
  const started = Date.now()
  await assert.rejects(lfs.machine.list(), (error) => error.code === 'Unknown' && error.transport === 'transport-unknown' && error.detail.timeout === C.PROBE_TIMEOUT_MS)
  const took = Date.now() - started
  assert.ok(took >= C.PROBE_TIMEOUT_MS - 200 && took < C.PROBE_TIMEOUT_MS + 2 * C.KILL_GRACE_MS + 3000, `settled after ${took} ms`)
})

test('a timeout ends the CLI\'s process group, escalating to SIGKILL, and settles', { timeout: 40000 }, async () => {
  const dir = join(root, 'hang'); mkdirSync(join(dir, 'home'), { recursive: true })
  const cli = join(dir, 'layerfs')
  // ignores SIGTERM, and so does its helper, which holds stdout/stderr open
  const ignore = "process.on('SIGTERM', () => {}); setInterval(() => {}, 1000)"
  writeFileSync(cli, `#!/usr/bin/env node
import { spawn } from 'node:child_process'
import { writeFileSync } from 'node:fs'
${ignore}
const helper = spawn(process.execPath, ['-e', ${JSON.stringify(ignore)}], { stdio: 'inherit' })
writeFileSync(${JSON.stringify(join(dir, 'helper'))}, String(helper.pid))
`, { mode: 0o700 })
  const started = Date.now()
  let pid
  // a budget past Node's start (slow under a loaded test run), so the CLI's SIGTERM handler is in place
  const budget = 5000
  await assert.rejects(C.execute({ cli, home: join(dir, 'home') }, ['machine', 'list'], { timeout: budget, onSpawn: (child) => { pid = child.pid } }),
    (error) => error.code === 'Unknown' && error.transport === 'transport-unknown')
  const took = Date.now() - started
  assert.ok(existsSync(join(dir, 'helper')), 'the CLI was up when its budget ran out')
  assert.ok(took >= budget + C.KILL_GRACE_MS - 200 && took < budget + 2 * C.KILL_GRACE_MS + 3000, `settled after ${took} ms (SIGTERM ignored, SIGKILL after the grace)`)
  const helper = Number(readFileSync(join(dir, 'helper'), 'utf8'))
  await new Promise((r) => setTimeout(r, 100))
  assert.throws(() => process.kill(helper, 0), /ESRCH/, 'the helper is gone')
  assert.throws(() => process.kill(pid, 0), /ESRCH/, 'the CLI is gone')
  assert.equal(C.children.size, 0)
})

test('a CLI that exits while a helper holds its stdout still answers; child.kill signals the whole group', { timeout: 40000 }, async (t) => {
  const dir = join(root, 'linger'); mkdirSync(join(dir, 'home'), { recursive: true })
  const cli = join(dir, 'layerfs')
  writeFileSync(cli, `#!/bin/sh\nif [ "$1" = hold ]; then sleep 30 & echo $! > "$(dirname "$0")/helper"; wait; exit 0; fi\nsleep 30 &\necho '{"ok":true}'\nexit 0\n`, { mode: 0o700 })
  let group
  t.after(() => { try { process.kill(-group, 'SIGKILL') } catch { /* gone */ } })
  const started = Date.now()
  assert.deepEqual(await C.execute({ cli, home: join(dir, 'home') }, ['machine', 'list'], { onSpawn: (child) => { group = child.pid } }), { ok: true })
  assert.ok(Date.now() - started < 20000, `answered after ${Date.now() - started} ms, not when the 30 s helper ended`)
  assert.equal(C.children.size, 0)

  // shutdown (index.ts) and simulation cancel call child.kill: it reaches the helper too
  let spawned
  const running = C.execute({ cli, home: join(dir, 'home') }, ['hold', 'x'], { onSpawn: (child) => { spawned = child } })
  for (let i = 0; i < 200 && !existsSync(join(dir, 'helper')); i++) await new Promise((r) => setTimeout(r, 10))
  assert.equal(C.children.size, 1)
  for (const child of C.children) child.kill('SIGTERM')
  await assert.rejects(running, (error) => error.transport === 'transport-unknown')
  const helper = Number(readFileSync(join(dir, 'helper'), 'utf8'))
  await new Promise((r) => setTimeout(r, 100))
  assert.throws(() => process.kill(helper, 0), /ESRCH/)
  assert.ok(spawned.pid)
})

test('a read refused by another process\'s Store lease (Failed/NotFound) is one attempt, as every call is; writes too', { timeout: 20000 }, async (t) => {
  const f = fresh()
  const lfs = C.client({ cli: f.cli, home: f.home })
  t.after(() => { delete process.env.FAKE_RACE_FAILS })
  const lists = () => f.log().filter((argv) => argv[1] === 'list' && argv[0] === 'file').length
  process.env.FAKE_RACE_FAILS = '1'
  await assert.rejects(lfs.file.list('demo/app', 'main', '/src'), (error) => error.code === 'Failed' && /NotFound/.test(error.message) && C.storeRace(error))
  assert.equal(lists(), 1, 'one attempt: never run again')
  assert.deepEqual((await lfs.file.list('demo/app', 'main', '/')).argv.slice(0, 2), ['file', 'list'], 'the caller\'s next read is its own call')
  rmSync(join(f.dir, 'race.count'))
  process.env.FAKE_RACE_FAILS = '1'
  await assert.rejects(lfs.file.write('demo/app', 'main', '/x', 'body'), (error) => error.code === 'Failed')
  assert.equal(f.log().filter((argv) => argv[1] === 'write').length, 1, 'a write is never replayed')
  assert.equal(C.storeRace(new C.CliError('Failed', 'Busy')), false)
})

test('null options are left out; an omitted optional positional followed by a given one is refused', { timeout: 20000 }, async () => {
  const f = fresh()
  const lfs = C.client({ cli: f.cli, home: f.home })
  const { argv } = await lfs.commit.diff('demo/app', 'b', { from: null, path: null })
  assert.deepEqual(argv, ['commit', 'diff', '--', 'demo/app', 'b'])
  await assert.rejects(lfs.project.stats('demo/app', undefined, 'c1'), (error) => error.code === 'InvalidInput' && /commit/.test(error.message))
  await assert.rejects(lfs.project.stats('demo/app', null, 'c1'), (error) => error.code === 'InvalidInput')
  assert.deepEqual((await lfs.project.stats('demo/app', 'main', undefined)).argv, ['project', 'stats', '--', 'demo/app', 'main'])
  assert.deepEqual((await lfs.file.write('demo/app', 'main', '/x', 'body', { message: null })).argv, ['file', 'write', '--', 'demo/app', 'main', '/x'])
  assert.ok(!f.log().some((argv) => argv[1] === 'stats' && argv.includes('c1')), 'the shifted call never ran')
})

test('ensureNative retries after a failed record and caches a success', { skip: process.platform !== 'darwin' && 'macOS only', timeout: 20000 }, async (t) => {
  const f = fresh()
  rmSync(join(f.home, 'layerfs.json'))
  const daemon = join(f.dir, 'layerfs-daemon'); writeFileSync(daemon, '')
  process.env.FAKE_UPDATE_FAILS = '1'
  t.after(() => { delete process.env.FAKE_UPDATE_FAILS })
  const service = new C.LayerfsService(new C.Context(), { cli: f.cli, daemon, home: f.home, root: join(f.dir, 'root') })
  const updates = () => f.log().filter((argv) => argv[0] === 'machine' && argv[1] === 'update').length
  await service.ensureNative()
  assert.equal(updates(), 1)
  await service.ensureNative()
  assert.equal(updates(), 2, 'the failed attempt is not kept')
  await service.ensureNative()
  assert.equal(updates(), 2, 'a recorded daemon is kept')
})

test('preset calls: reads and short writes keep 2 minutes, long work gets 30 minutes', () => {
  assert.equal(C.presetTimeout(['machine', 'list'], true), C.PRESET_TIMEOUT_MS)
  assert.equal(C.presetTimeout(['file', 'write', 'p/a', 'main', '/x'], false), C.PRESET_TIMEOUT_MS)
  for (const argv of [['machine', 'create', 'vm', 'lima'], ['machine', 'start', 'vm'], ['store', 'move', 's', 'vm'], ['project', 'init', 'p/a', '/src'], ['sandbox', 'create', 'p/a', 'sb']]) {
    assert.equal(C.presetTimeout(argv, false), C.PRESET_LONG_TIMEOUT_MS, argv.join(' '))
  }
  assert.equal(C.PRESET_LONG_TIMEOUT_MS, 30 * 60_000)
})

test('Stage 1 calls: machine.images is kept 5 s and cleared by machine.prune; sandbox.create sends the reference and --progress', { timeout: 20000 }, async () => {
  const f = fresh()
  const lfs = C.client({ cli: f.cli, home: f.home })
  assert.equal(C.KEPT_READS.get('machine.images'), 5000)
  const images = () => f.log().filter((argv) => argv[0] === 'machine' && argv[1] === 'images').length
  await lfs.machine.images('vm')
  await lfs.machine.images('vm')
  assert.equal(images(), 1, 'the second read is the kept answer')
  await lfs.machine.prune('vm', { unused: true })
  assert.deepEqual(f.log().find((argv) => argv[1] === 'prune'), ['machine', 'prune', '--unused=true', '--', 'vm'])
  await lfs.machine.prune('vm', 'sha256:' + 'a'.repeat(64))
  assert.deepEqual(f.log().filter((argv) => argv[1] === 'prune')[1], ['machine', 'prune', '--', 'vm', 'sha256:' + 'a'.repeat(64)])
  await lfs.machine.images('vm')
  assert.equal(images(), 2, 'a prune cleared the kept answer')
  const seen = []
  await lfs.sandbox.create('s/p', 'b1', { machine: 'vm', image: 'postgres:17-bookworm', on_progress: (p) => seen.push(p) })
  const create = f.log().find((argv) => argv[0] === 'sandbox' && argv[1] === 'create')
  assert.deepEqual(create, ['sandbox', 'create', '--machine', 'vm', '--image', 'postgres:17-bookworm', '--progress', '--', 's/p', 'b1'])
  assert.deepEqual(seen, [{ phase: 'send', done: 1, total: 2 }])
})

// ---------------------------------------------------------------- monitoring reads (layerfs-project#179 D)

const inspectSpawns = (f) => f.log().filter((argv) => C.INSPECT_CALLS.has(`${argv[0]}.${argv[1]}`))
const until = async (check, ms = 6000) => { for (let i = 0; i < ms / 20 && !check(); i++) await new Promise((r) => setTimeout(r, 20)); return check() }

test('monitoring reads are named with their family and take the inspect lane', () => {
  assert.deepEqual([...C.INSPECT_CALLS].sort(), ['machine.files', 'machine.log', 'machine.stats', 'sandbox.files', 'sandbox.log'])
  assert.deepEqual(C.INSPECT_LANE, { name: 'inspect', limit: 2, queue: 4 })
  assert.ok(C.INSPECT_TIMEOUT_MS <= 15_000)
  for (const argv of [['machine', 'log', '--', 'vm'], ['sandbox', 'log', '--', 'sb'], ['sandbox', 'files', '--', 'sb'], ['machine', 'files', '--', 'vm']]) assert.equal(C.laneOf(argv).name, 'inspect', argv.join(' '))
  // the neighbours stay where they were: the call log's own reads, sandbox.get; machine.get is a probe (`--with live`)
  for (const argv of [['home', 'log'], ['home', 'clear-log'], ['sandbox', 'get', '--', 'sb'], ['sandbox', 'list']]) assert.equal(C.laneOf(argv).name, 'calls', argv.join(' '))
  assert.equal(C.laneOf(['machine', 'get', '--', 'vm']).name, 'probes')
})

test('the inspect lane runs 2 and queues 4; a 7th read is Busy at once; a dropped queued read never starts a process; a dropped running one is ended', { timeout: 40000 }, async (t) => {
  const f = fresh()
  t.after(() => writeFileSync(join(f.dir, 'release'), ''))
  const lfs = C.client({ cli: f.cli, home: f.home })
  await lfs.machine.list()
  writeFileSync(join(f.dir, 'hold-inspect'), '')
  const signals = Array.from({ length: 6 }, () => new AbortController())
  const settled = []
  const reads = signals.map((c, i) => {
    const read = (i % 2 ? lfs.machine.log(`vm${i}`, { signal: c.signal }) : lfs.sandbox.files(`sb${i}`, { signal: c.signal }))
    return read.then((value) => { settled[i] = { value }; return value }, (error) => { settled[i] = { error } })
  })
  assert.ok(await until(() => inspectSpawns(f).length === 2), 'two reads run')
  await new Promise((r) => setTimeout(r, 300))
  assert.equal(inspectSpawns(f).length, 2, 'the other four wait: no third process')
  // the 7th: refused now, by this host, without a process
  const started = Date.now()
  await assert.rejects(lfs.sandbox.log('sb7'), (error) => error.code === 'Busy' && error.transport === 'refusal' && error.detail.lane === 'inspect' && error.detail.running === 2 && error.detail.queued === 4)
  assert.ok(Date.now() - started < 1000, `Busy after ${Date.now() - started} ms`)
  assert.equal(inspectSpawns(f).length, 2)
  // the calls and probes lanes are untouched: a page read answers while both inspect slots are held
  assert.deepEqual((await lfs.machine.get('vm')).argv.slice(0, 2), ['machine', 'get'])
  assert.deepEqual((await lfs.machine.list()).argv.slice(0, 2), ['machine', 'list'])
  // a queued read is dropped: refused Cancelled, and it never starts
  signals[5].abort()
  await reads[5]
  assert.equal(settled[5].error?.code, 'Cancelled')
  // room for one more waiter again
  const extra = new AbortController()
  const seventh = lfs.machine.files('vm7', { signal: extra.signal }).then((value) => ({ value }), (error) => ({ error }))
  await new Promise((r) => setTimeout(r, 100))
  assert.equal(inspectSpawns(f).length, 2)
  // a running read is dropped: its process is ended, it is refused Cancelled, and a waiter takes the slot
  const pids = readFileSync(join(f.dir, 'held.pids'), 'utf8').trim().split('\n').map(Number)
  assert.equal(pids.length, 2)
  signals[0].abort()
  await reads[0]
  assert.equal(settled[0].error?.code, 'Cancelled')
  assert.ok(await until(() => inspectSpawns(f).length === 3), 'the next waiter starts')
  assert.ok(await until(() => pids.filter((pid) => { try { process.kill(pid, 0); return true } catch { return false } }).length === 1), 'the dropped read\'s process is gone')
  writeFileSync(join(f.dir, 'release'), '')
  await Promise.all(reads)
  assert.ok((await seventh).value, 'the later waiter answers')
  for (const i of [1, 2, 3, 4]) assert.ok(settled[i].value?.held, `read ${i} answered`)
  // six processes in all: reads 0-4 and the later waiter; the dropped queued read (vm5) never ran
  assert.equal(inspectSpawns(f).length, 6)
  assert.ok(!inspectSpawns(f).some((argv) => argv.includes('vm5')), 'the dropped queued read never started a process')
  assert.ok(!f.log().some((argv) => argv.some((word) => /signal|abort/i.test(word))), 'the signal never reaches the CLI')
  assert.equal(C.children.size, 0)
})

test('monitoring reads run once, keep nothing, clear nothing and are not shared', { timeout: 20000 }, async (t) => {
  const f = fresh()
  const lfs = C.client({ cli: f.cli, home: f.home })
  t.after(() => { delete process.env.FAKE_INSPECT_RACE })
  const count = (family, call) => f.log().filter((argv) => argv[0] === family && argv[1] === call).length
  // a kept read, then monitoring reads, then the kept read again: still the kept answer
  await lfs.machine.images('vm')
  await lfs.machine.list()
  assert.deepEqual((await lfs.machine.log('vm', { cursor: 'c1', limit: 50, detail: true })).argv, ['machine', 'log', '--cursor', 'c1', '--limit', '50', '--detail', '--', 'vm'])
  assert.deepEqual((await lfs.sandbox.log('sb', { cursor: 'b1' })).argv, ['sandbox', 'log', '--cursor', 'b1', '--', 'sb'])
  assert.deepEqual((await lfs.sandbox.files('sb', '/home', { depth: 1 })).argv, ['sandbox', 'files', '--depth', '1', '--', 'sb', '/home'])
  assert.deepEqual((await lfs.machine.files('vm')).argv, ['machine', 'files', '--', 'vm'])
  // the files routes' form: no path (the roots) with options, and a file window
  assert.deepEqual((await lfs.machine.files('vm', undefined, { limit: 5, signal: new AbortController().signal })).argv, ['machine', 'files', '--limit', '5', '--', 'vm'])
  assert.deepEqual((await lfs.sandbox.files('sb', '/-odd name', { byte_offset: 262144, byte_length: 1048576 })).argv, ['sandbox', 'files', '--byte-offset', '262144', '--byte-length', '1048576', '--', 'sb', '/-odd name'])
  await lfs.machine.images('vm')
  await lfs.machine.list()
  assert.equal(count('machine', 'images'), 1, 'a monitoring read cleared no kept answer')
  assert.equal(count('machine', 'list'), 1)
  // the same read twice is two processes (routes share reads, not the SDK), and nothing is kept
  await Promise.all([lfs.machine.log('vm'), lfs.machine.log('vm')])
  assert.equal(count('machine', 'log'), 3)
  // LayerFS's Store-race refusal: one process each, as every call
  process.env.FAKE_INSPECT_RACE = '1'
  const before = inspectSpawns(f).length
  await assert.rejects(lfs.sandbox.log('sb'), (error) => error.code === 'Failed' && C.storeRace(error))
  await assert.rejects(lfs.machine.files('vm', '/etc'), (error) => error.code === 'Failed')
  assert.equal(inspectSpawns(f).length, before + 2, 'one process each')
  // option checks: a signal must be one, and only monitoring reads take it
  await assert.rejects(lfs.machine.log('vm', { signal: {} }), (error) => error.code === 'InvalidInput' && /AbortSignal/.test(error.message))
  await assert.rejects(lfs.machine.get('vm', { signal: new AbortController().signal }), (error) => error.code === 'InvalidInput' && /unknown option signal/.test(error.message))
})

test('monitoring reads write no call log line and keep no output, in stand-in mode too', { timeout: 20000 }, async () => {
  const f = fresh()
  const callLog = join(f.dir, 'trace', 'calls.jsonl')
  const lfs = C.client({ cli: f.cli, home: f.home, callLog, nativeLogs: false })
  await lfs.machine.log('vm')
  await lfs.sandbox.log('sb', { cursor: 'c' })
  await lfs.sandbox.files('sb')
  await lfs.machine.files('vm', '/layerfs')
  assert.equal(existsSync(callLog), false, 'no call log line')
  assert.equal(existsSync(join(f.dir, 'trace', 'outputs')), false, 'no output kept')
  // the same client does log an ordinary call
  await lfs.machine.get('vm')
  const lines = readFileSync(callLog, 'utf8').trim().split('\n').map(JSON.parse)
  assert.deepEqual(lines.map((l) => l.argv.slice(0, 2)), [['machine', 'get']])
  assert.equal(existsSync(join(f.dir, 'trace', 'outputs')), true)
})

test('a monitoring read has at most 15 s: a longer budget is cut to it, and running out is Timeout, not Unknown', { timeout: 40000 }, async (t) => {
  const f = fresh()
  t.after(() => writeFileSync(join(f.dir, 'release'), ''))
  const lfs = C.client({ cli: f.cli, home: f.home })
  await lfs.machine.list()
  writeFileSync(join(f.dir, 'hold-inspect'), '')
  let started = Date.now()
  await assert.rejects(lfs.machine.log('vm', { timeout_ms: 300 }), (error) => error.code === 'Timeout' && error.transport === 'transport-unknown' && error.detail.timeout === 300)
  assert.ok(Date.now() - started < 300 + 2 * C.KILL_GRACE_MS + 2000, `settled in ${Date.now() - started} ms`)
  // asked for a minute: stopped at 15 s
  started = Date.now()
  await assert.rejects(lfs.sandbox.files('sb', { timeout_ms: 60_000 }), (error) => error.code === 'Timeout' && error.detail.timeout === C.INSPECT_TIMEOUT_MS)
  const took = Date.now() - started
  assert.ok(took >= C.INSPECT_TIMEOUT_MS - 200 && took < C.INSPECT_TIMEOUT_MS + 2 * C.KILL_GRACE_MS + 3000, `settled after ${took} ms`)
  assert.equal(C.children.size, 0)
})

test('a caller-named lane never moves a monitoring read out of the inspect lane', { timeout: 20000 }, async (t) => {
  const f = fresh()
  t.after(() => writeFileSync(join(f.dir, 'release'), ''))
  const spec = { cli: f.cli, home: f.home }
  writeFileSync(join(f.dir, 'hold-inspect'), '')
  // a bulk caller's wide lane (presets, simulations): three reads would all run in it
  const bulk = { name: 'bulk', limit: 16 }
  const reads = [1, 2, 3].map((i) => C.execute(spec, ['machine', 'files', '--', `vm${i}`], { lane: bulk, read: true }))
  assert.ok(await until(() => inspectSpawns(f).length === 2), 'two run')
  await new Promise((r) => setTimeout(r, 300))
  assert.equal(inspectSpawns(f).length, 2, 'the third waits in the inspect lane')
  writeFileSync(join(f.dir, 'release'), '')
  assert.equal((await Promise.all(reads)).length, 3)
})

test('workspace exec: exit 125 with a result and no refusal is the command\'s own; with a refusal it is LayerFS\'s; with neither it is Unknown (review UR bug 3)', async () => {
  const f = fresh()
  const lfs = C.client({ cli: f.cli, home: f.home })
  assert.deepEqual(await lfs.workspace.exec('ws', 'own 125'), { exit_code: 125, stdout: '', stderr: 'x', exec: 'exec-1' })
  await assert.rejects(lfs.workspace.exec('ws', 'refused 125'), (e) => e.code === 'Busy' && e.transport === 'refusal' && e.fix === 'layerfs workspace cancel ws')
  await assert.rejects(lfs.workspace.exec('ws', 'lost 125'), (e) => e.code === 'Unknown' && e.transport === 'transport-unknown')
})
