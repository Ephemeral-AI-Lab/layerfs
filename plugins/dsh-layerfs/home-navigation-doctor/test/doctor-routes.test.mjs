// The Doctor page's host routes (`src/host/routes/doctor.ts`) against the
// stand-in CLI (`test/fixtures/doctor-cli.mjs`): the tree, doctor calls with
// their progress, Stop, could-not-check reasons, the fix allow list, and one
// attempt per call (counted from the stand-in's spawn log).
//   node --test --test-timeout=60000 test/doctor-routes.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'
import { testHost } from './routes-helper.mjs'
import { freshState, thingKeys } from './fixtures/doctor-scenarios.mjs'

const root = fileURLToPath(new URL('..', import.meta.url))
const out = mkdtempSync(join(tmpdir(), 'dsh-doctor-routes-'))
await build({
  absWorkingDir: root,
  stdin: { contents: "export * from './src/host/routes/doctor.ts'\nexport * from './src/host/routes/router.ts'\nexport * as cli from './src/host/layerfs/cli.ts'\nexport { hasTreeRead } from './src/shared/doctor.ts'\n", resolveDir: root, loader: 'ts' },
  outfile: join(out, 'routes.mjs'), bundle: true, format: 'esm', platform: 'node', target: 'node24', logLevel: 'warning',
})
const R = await import(pathToFileURL(join(out, 'routes.mjs')).href)
const STANDIN = join(root, 'test/fixtures/doctor-cli.mjs')
const dirs = []
test.after(() => { for (const d of [out, ...dirs]) rmSync(d, { recursive: true, force: true }) })

const sleep = (ms) => new Promise((r) => setTimeout(r, ms))
/** Waits for `read()` to be truthy, for at most `ms`: a bounded wait, never forever. */
async function until(read, what, ms = 15_000) {
  const end = Date.now() + ms
  for (;;) {
    const value = read()
    if (value) return value
    if (Date.now() > end) assert.fail(`timed out waiting for ${what}`)
    await sleep(10)
  }
}

/** A 2.0 call table as far as these routes read it: `home get` lists `tree` among its `--with` values. */
const TABLE_2_0 = async () => ({ identity: {}, calls: [{ family: 'home', call: 'get', args: [], options: [], form: { with: { platforms: {}, tree: {} } } }] })
let n = 0
const id = () => `run-${String(++n).padStart(6, '0')}`

/** A host over the stand-in CLI in one scenario, with its own folder (the stand-in's state and spawn log). */
function fixture(scenario, { timing = {}, state = null, options = {} } = {}) {
  const dir = mkdtempSync(join(tmpdir(), 'dsh-doctor-standin-'))
  dirs.push(dir)
  writeFileSync(join(dir, 'control.json'), JSON.stringify({ scenario, epoch: 1, timing }))
  if (state) writeFileSync(join(dir, 'state.json'), JSON.stringify({ ...freshState(scenario), epoch: 1, ...state }))
  // every call of this host carries its own stand-in folder, so tests can run side by side
  const run = (spec, argv, o = {}) => R.cli.execute(spec, argv, { ...o, env: { ...o.env, LFS_DOCTOR_DIR: dir } })
  const spec = { cli: STANDIN, home: join(dir, 'home') }
  // the host's CLI client asks the CLI only about a home that is set up
  mkdirSync(spec.home); writeFileSync(join(spec.home, 'layerfs.json'), '{}')
  const call = (...argv) => run(spec, argv, {})
  const lfs = { spec }
  const actions = {
    machineStop: (name) => call('machine', 'stop', '--', name),
    machineStart: (name) => call('machine', 'start', '--', name),
    projectDelete: (project) => call('project', 'delete', '--', project),
    backupCreate: (store, bucket) => call('backup', 'create', '--', store, bucket),
  }
  const t = testHost({ lfs, root: dir, home: spec.home, discovery: TABLE_2_0 })
  const checks = new R.DoctorChecks(t.host, run, { budgetMs: 20_000, treeKeepMs: 0, actions, ...options })
  const spawns = (...words) => (existsSync(join(dir, 'spawns.log')) ? readFileSync(join(dir, 'spawns.log'), 'utf8').trim().split('\n').filter(Boolean).map((l) => JSON.parse(l.slice(l.indexOf(' ') + 1))) : [])
    .filter((argv) => words.every((w, i) => argv[i] === w))
  const events = (type, runId) => t.events.filter((e) => e.type === type && (!runId || e.data.id === runId)).map((e) => e.data)
  const ended = (runId) => until(() => events('doctor-run', runId).find((e) => ['done', 'failed', 'stopped'].includes(e.state)), `run ${runId} to end`)
  const fixEnded = (fixId) => until(() => events('doctor-fix', fixId).find((e) => e.state !== 'running'), `fix ${fixId} to end`)
  const post = (path, body) => checks.route('POST', path.split('/'), body)
  const start = async (kind, name, more = {}) => { const runId = id(); const answer = await post('runs', { runs: [{ id: runId, kind, ...(name ? { name } : {}), ...more }] }); return { runId, answer } }
  return { dir, checks, host: t.host, spawns, events, ended, fixEnded, post, start, close: () => checks.close() }
}
const refusal = async (promise) => { try { await promise } catch (error) { return error } assert.fail('expected a refusal') }

test('the tree is one records read: no doctor runs, concurrent readers share it', async () => {
  const f = fixture('stopped', { options: { treeKeepMs: 5000 } })
  const [a, b] = await Promise.all([f.checks.route('GET', ['tree']), f.checks.route('GET', ['tree'])])
  assert.equal(a, b)
  assert.equal(a.home.state, 'ready')
  assert.equal(a.home.version, '2.0.0')
  assert.deepEqual(a.tree.machines.map((m) => [m.machine, m.state]), [['native', 'ready'], ['demo-build', 'ready'], ['docker', 'stopped'], ['dev', 'ready']])
  assert.deepEqual(a.tree.machines[0].sandboxes[0].workspaces, [{ workspace: 'tests:agent', branch: 'agent' }, { workspace: 'tests:main', branch: 'main' }])
  assert.deepEqual(a.tree.buckets.map((x) => x.bucket), ['r2-main', 'minio-local', 'r2-old'])
  assert.deepEqual(f.spawns(), [['home', 'get', '--with', 'tree', '--detail']], 'opening the page spawns the records read only')
  assert.deepEqual(f.checks.results(), { last: [], running: [], fixes: [] })
})

test('a runtime without the tree is Unsupported, not an empty page', async () => {
  // today's call table (`home get` with no `--with`): the call is never sent
  let sent = 0
  const today = testHost({ lfs: { spec: { cli: '/nonexistent', home: '/nonexistent' } }, root: out, home: out, discovery: async () => ({ identity: {}, calls: [{ family: 'home', call: 'get', args: [], options: [] }] }) }).host
  const gated = new R.DoctorChecks(today, async () => { sent += 1; return {} })
  const g = await refusal(gated.route('GET', ['tree']))
  assert.deepEqual([g.status, g.code, sent], [501, 'Unsupported', 0])
  // the 2.0 table lists the value under `form.with`, keyed by value; nothing else counts
  assert.equal(R.hasTreeRead([{ family: 'home', call: 'get', form: { with: { platforms: {}, tree: {} } } }]), true)
  assert.equal(R.hasTreeRead([{ family: 'home', call: 'get', form: { with: { platforms: {} } } }]), false)
  assert.equal(R.hasTreeRead([{ family: 'home', call: 'get', with: { tree: {} } }]), false)
  assert.equal(R.hasTreeRead([{ family: 'home', call: 'get', options: [{ name: 'with', kind: 'list' }] }]), false)
  assert.equal(R.hasTreeRead([{ family: 'home', call: 'get', with: { platforms: {} }, options: [] }]), false)
  assert.equal(R.hasTreeRead([{ family: 'machine', call: 'get', with: { tree: {} } }]), false)
  assert.equal(R.hasTreeRead(null), false)
  // a table that has it over a CLI that refuses the words, or answers without a tree
  const host = testHost({ lfs: { spec: { cli: '/nonexistent', home: '/nonexistent' } }, root: out, home: out, discovery: TABLE_2_0 }).host
  const old = new R.DoctorChecks(host, async () => { throw Object.assign(new Error('usage'), { code: 'CliIncompatible', transport: 'usage-incompatible' }) })
  const a = await refusal(old.route('GET', ['tree']))
  assert.equal(a.status, 501); assert.equal(a.code, 'Unsupported')
  const bare = new R.DoctorChecks(host, async () => ({ home: '/h', state: 'ready' }))
  assert.equal((await refusal(bare.route('GET', ['tree']))).status, 501)
})

test('a doctor call: queued, one progress event per finished thing, then its report; kept for a page that opens later', async () => {
  const f = fixture('mixed')
  const { runId, answer } = await f.start('machine', 'dev')
  assert.equal(answer.status, 202)
  assert.deepEqual(answer.body.already, [])
  assert.equal(answer.body.runs[0].state, 'queued')
  const end = await f.ended(runId)
  assert.equal(end.state, 'done')
  const seen = f.events('doctor-run', runId)
  assert.equal(seen[0].state, 'queued')
  const progress = seen.filter((e) => e.progress)
  // dev, its Store shop, and its three Sandboxes: one event each, each naming its thing with its checks
  assert.deepEqual(progress.map((e) => [e.progress.done, e.progress.total]), [[1, 5], [2, 5], [3, 5], [4, 5], [5, 5]])
  assert.deepEqual(progress.map((e) => Object.keys(e.progress.node)[0] + ':' + Object.values(e.progress.node)[0]), ['machine:dev', 'store:shop', 'sandbox:sdk-1', 'sandbox:exp-1', 'sandbox:funnels'])
  assert.ok(progress.every((e) => Array.isArray(e.progress.node.checks) && !('stores' in e.progress.node)))
  assert.equal(end.report.status, 'error')
  assert.deepEqual(end.report.findings.map((x) => x.check).sort(), ['sandbox.daemon_connect', 'store.backups'])
  assert.deepEqual(f.spawns('machine', 'doctor'), [['machine', 'doctor', '--detail', '--progress', '--', 'dev']])
  const held = f.checks.results()
  assert.equal(held.running.length, 0)
  assert.equal(held.last.length, 1)
  assert.equal(held.last[0].report.tree.machine, 'dev')
  assert.equal(f.checks.busy(), false)
})

test('Full check takes --full on a project and a bucket; a bucket reports no progress; other kinds have none', async () => {
  const f = fixture('healthy')
  const a = await f.start('project', 'shop/api', { full: true })
  const b = await f.start('bucket', 'r2-main', { full: true })
  const [ea, eb] = [await f.ended(a.runId), await f.ended(b.runId)]
  assert.equal(ea.report.tree.coverage, 'full')
  assert.deepEqual(ea.report.tree.checks.map((c) => c.check), ['project.record', 'project.listed', 'project.mount_holders', 'project.data'])
  assert.equal(eb.report.tree.coverage, 'full')
  assert.deepEqual(f.spawns('project', 'doctor'), [['project', 'doctor', '--detail', '--progress', '--full', '--', 'shop/api']])
  assert.deepEqual(f.spawns('backup', 'doctor'), [['backup', 'doctor', '--detail', '--full', '--', 'r2-main']])
  for (const [kind, name] of [['machine', 'dev'], ['store', 'shop'], ['sandbox', 'tests'], ['home', undefined]]) {
    const e = await refusal(f.post('runs', { runs: [{ id: id(), kind, ...(name ? { name } : {}), full: true }] }))
    assert.equal(e.status, 400, `${kind} has no Full check`)
  }
})

test('what is asked for is checked before anything runs', async () => {
  const f = fixture('healthy')
  const bad = [
    { runs: [] },
    { runs: [{ id: 'short', kind: 'machine', name: 'dev' }] },
    { runs: [{ id: id(), kind: 'workspace', name: 'tests:agent' }] },
    { runs: [{ id: id(), kind: 'machine' }] },
    { runs: [{ id: id(), kind: 'home', name: 'x' }] },
    { runs: [{ id: id(), kind: 'machine', name: '--force' }] },
    { runs: [{ id: id(), kind: 'machine', name: 'dev; rm -rf /' }] },
    { runs: [{ id: 'same-id-twice', kind: 'machine', name: 'dev' }, { id: 'same-id-twice', kind: 'machine', name: 'native' }] },
    { runs: [{ id: id(), kind: 'machine', name: 'dev' }], group: 'no' },
  ]
  for (const body of bad) assert.equal((await refusal(f.post('runs', body))).status, 400, JSON.stringify(body))
  assert.deepEqual(f.spawns(), [], 'nothing ran')
  assert.equal(await f.checks.route('GET', ['nothing']), undefined)
})

test('a thing already being checked is not checked twice', async () => {
  const f = fixture('healthy', { timing: { self: 300 } })
  const first = id(), second = id()
  await f.post('runs', { runs: [{ id: first, kind: 'machine', name: 'dev' }] })
  const again = await f.post('runs', { runs: [{ id: second, kind: 'machine', name: 'dev' }] })
  assert.deepEqual(again.body, { runs: [], already: [second] })
  assert.equal(f.checks.busy(), true)
  assert.deepEqual(f.checks.results().running.map((r) => r.id), [first])
  await f.ended(first)
  assert.equal(f.spawns('machine', 'doctor').length, 1)
})

test('Stop ends a Full check: its process goes, nothing is reported, the target keeps its last outcome', async () => {
  const f = fixture('slowfull', { timing: { slow: 60_000 } })
  const shallow = await f.start('project', 'shop/api')
  await f.ended(shallow.runId)
  const { runId } = await f.start('project', 'shop/api', { full: true })
  await until(() => f.spawns('project', 'doctor').length === 2, 'the Full check to start')
  await until(() => f.events('doctor-run', runId).some((e) => e.state === 'running'), 'the running event')
  assert.deepEqual(await f.post(`runs/${runId}/stop`), { stopped: true })
  const end = await f.ended(runId)
  assert.equal(end.state, 'stopped')
  assert.equal(end.report, undefined); assert.equal(end.failure, undefined)
  const held = f.checks.results()
  assert.deepEqual(held.last.map((r) => [r.id, r.full, r.state]), [[shallow.runId, false, 'done']])
  assert.equal((await refusal(f.post(`runs/${runId}/stop`))).status, 404)
  assert.equal(f.spawns('project', 'doctor').length, 2, 'never started again')
})

test('could not check: timeout, busy and lost, each from one attempt', async () => {
  // a call that never answers is ended at the host's budget
  const slow = fixture('healthy', { state: { fail: { 'machine:dev': 'hang' } }, options: { budgetMs: 400 } })
  const t = await slow.start('machine', 'dev')
  const timed = await slow.ended(t.runId)
  assert.equal(timed.state, 'failed')
  assert.equal(timed.failure.reason, 'timeout')
  assert.equal(slow.spawns('machine', 'doctor').length, 1)

  const f = fixture('timeout')
  const busy = await f.start('sandbox', 'sdk-1')
  const lost = await f.start('bucket', 'minio-local')
  const own = await f.start('store', 'qv2-store')
  assert.deepEqual((await f.ended(busy.runId)).failure, { reason: 'busy', code: 'Busy' })
  assert.equal((await f.ended(lost.runId)).failure.reason, 'lost')
  // LayerFS's own budget ran out: a report whose node says why
  const report = (await f.ended(own.runId)).report
  assert.equal(report.tree.status, 'not_checked'); assert.equal(report.tree.reason, 'timeout')
  assert.deepEqual(report.not_checked, [{ store: 'qv2-store', check: null, blocked_by: null, reason: 'timeout' }])
  for (const [family, name] of [['sandbox', 'sdk-1'], ['backup', 'minio-local'], ['store', 'qv2-store']]) assert.equal(f.spawns(family, 'doctor').length, 1, `${name}: one attempt`)
  assert.deepEqual(f.checks.results().last.map((r) => r.state).sort(), ['done', 'failed', 'failed'])
  // the failure is one-time in the stand-in: the next press is a new call, and it works
  const next = await f.start('sandbox', 'sdk-1')
  assert.equal((await f.ended(next.runId)).state, 'done')
})

test('failureOf reads the three reasons', () => {
  assert.deepEqual(R.failureOf({ code: 'Unknown', transport: 'transport-unknown', detail: { timeout: true } }), { reason: 'timeout', code: 'Unknown' })
  assert.deepEqual(R.failureOf({ code: 'Timeout' }), { reason: 'timeout', code: 'Timeout' })
  assert.deepEqual(R.failureOf({ code: 'Busy' }), { reason: 'busy', code: 'Busy' })
  assert.deepEqual(R.failureOf({ code: 'Busy', transport: 'transport-unknown' }), { reason: 'lost', code: 'Busy' })
  assert.deepEqual(R.failureOf({ code: 'Unavailable' }), { reason: 'lost', code: 'Unavailable' })
  assert.deepEqual(R.failureOf(new Error('socket hang up')), { reason: 'lost', code: null })
})

test('Check all on a large home loses no result (more calls than the old 40-job list held)', async () => {
  const host = testHost({ lfs: { spec: { cli: '/nonexistent', home: '/nonexistent' } }, root: out, home: out })
  const fake = async (_spec, argv, o) => {
    const name = argv.at(-1)
    o.onSpawn?.({ pid: 1 })
    o.onProgress?.({ phase: 'self', done: 1, total: 1, node: { store: name, status: 'ok', coverage: 'shallow', checks: [], blocked_by: null, reason: null } })
    return { ok: true, findings: [], not_checked: [], status: 'ok', counts: {}, observed_at: new Date().toISOString(), tree: { store: name, status: 'ok', coverage: 'shallow', checks: [], blocked_by: null, reason: null } }
  }
  const checks = new R.DoctorChecks(host.host, fake)
  const runs = Array.from({ length: 150 }, (_, i) => ({ id: id(), kind: 'store', name: `store-${i}` }))
  const answer = await checks.route('POST', ['runs'], { runs, group: 'all-00000001' })
  assert.equal(answer.body.runs.length, 150)
  await until(() => checks.results().last.length === 150, 'all 150 outcomes')
  assert.equal(new Set(checks.results().last.map((r) => r.name)).size, 150)
  assert.ok(checks.results().last.every((r) => r.group === 'all-00000001' && r.report))
  assert.equal(host.events.filter((e) => e.data.state === 'done').length, 150)
})

test('a fix runs only from a line this host produced, and only when it is on the allow list', async () => {
  const f = fixture('mixed')
  const fix = (kind, name, line) => f.post('fixes', { id: id(), kind, name, fix: line })
  // nothing read yet: no line is on offer
  assert.equal((await refusal(fix('machine', 'docker', 'layerfs machine start docker'))).status, 409)
  await f.checks.route('GET', ['tree'])
  // the records offer Start for the stopped machine, and nothing else
  assert.equal((await refusal(fix('machine', 'dev', 'layerfs machine start dev'))).status, 409)
  assert.equal((await refusal(fix('machine', 'docker', 'layerfs machine remove docker'))).status, 409)
  assert.equal((await refusal(fix('machine', 'docker', 'layerfs machine start docker; rm -rf ~'))).status, 409)
  // a report's lines: the page never runs --force, nor a call off the list, though LayerFS offered both
  const home = await f.start('home')
  const sandbox = await f.start('sandbox', 'exp-1')
  const store = await f.start('store', 'shop')
  await Promise.all([f.ended(home.runId), f.ended(sandbox.runId), f.ended(store.runId)])
  const forced = await refusal(fix('machine', 'demo-build', 'layerfs machine stop demo-build --force && layerfs machine start demo-build'))
  assert.equal(forced.status, 400); assert.equal(forced.code, 'InvalidInput')
  assert.equal((await refusal(fix('sandbox', 'exp-1', 'layerfs sandbox remove exp-1'))).status, 400)
  // the plain restart was not offered for a machine with Sandboxes on it
  assert.equal((await refusal(fix('machine', 'demo-build', 'layerfs machine stop demo-build && layerfs machine start demo-build'))).status, 409)
  // a line offered for one thing is not a fix for another
  assert.equal((await refusal(fix('store', 'demo-studio', 'layerfs backup create shop r2-old'))).status, 409)
  assert.deepEqual(f.spawns('machine', 'stop'), [])
  assert.deepEqual(f.spawns('machine', 'start'), [])

  // on the list and offered: Start, then the Store's backup
  const started = await fix('machine', 'docker', 'layerfs machine start docker')
  assert.equal(started.status, 202)
  assert.deepEqual(started.body.fix.steps, ['start'])
  assert.equal((await f.fixEnded(started.body.fix.id)).state, 'done')
  const backup = await fix('store', 'shop', 'layerfs backup create shop r2-old')
  assert.equal((await f.fixEnded(backup.body.fix.id)).state, 'done')
  assert.deepEqual(f.spawns('machine', 'start'), [['machine', 'start', '--', 'docker']])
  assert.deepEqual(f.spawns('backup', 'create'), [['backup', 'create', '--', 'shop', 'r2-old']])
  // done: the line is no longer on offer until a doctor names it again
  assert.equal((await refusal(fix('store', 'shop', 'layerfs backup create shop r2-old'))).status, 409)
  const tree = await f.checks.route('GET', ['tree'])
  assert.equal(tree.tree.machines.find((m) => m.machine === 'docker').state, 'ready')
})

test('an outdated worker with no Sandbox on its machine: stop, then start, one call each', async () => {
  const f = fixture('outdated0', { timing: { fix: 60 } })
  const m = await f.start('machine', 'demo-build')
  const line = (await f.ended(m.runId)).report.findings[0].fix
  assert.equal(line, 'layerfs machine stop demo-build && layerfs machine start demo-build')
  const a = await f.post('fixes', { id: id(), kind: 'machine', name: 'demo-build', fix: line })
  assert.deepEqual(a.body.fix, { id: a.body.fix.id, kind: 'machine', name: 'demo-build', fix: 'restart', step: 0, steps: ['stop', 'start'], state: 'running' })
  assert.equal((await refusal(f.post('fixes', { id: id(), kind: 'machine', name: 'demo-build', fix: line }))).code, 'Busy')
  assert.equal(f.checks.results().fixes.length, 1)
  assert.equal((await f.fixEnded(a.body.fix.id)).state, 'done')
  assert.deepEqual(f.events('doctor-fix', a.body.fix.id).map((e) => [e.state, e.step]), [['running', 0], ['running', 1], ['done', 1]])
  assert.deepEqual(f.spawns('machine').filter((argv) => argv[1] !== 'doctor'), [['machine', 'stop', '--', 'demo-build'], ['machine', 'start', '--', 'demo-build']])
  const again = await f.start('machine', 'demo-build')
  assert.equal((await f.ended(again.runId)).report.status, 'ok')
})

test('a fix LayerFS refuses is reported with its code and sentence, and is not tried again', async () => {
  const f = fixture('refused')
  await f.checks.route('GET', ['tree'])
  const a = await f.post('fixes', { id: id(), kind: 'machine', name: 'docker', fix: 'layerfs machine start docker' })
  const end = await f.fixEnded(a.body.fix.id)
  assert.equal(end.state, 'refused')
  assert.deepEqual(end.refusal, { code: 'Unavailable', message: 'Docker Engine is not running' })
  assert.equal(f.spawns('machine', 'start').length, 1)
  assert.equal(f.checks.busy(), false)
  // the person presses again: a new call
  await f.checks.route('GET', ['tree'])
  const b = await f.post('fixes', { id: id(), kind: 'machine', name: 'docker', fix: 'layerfs machine start docker' })
  assert.equal((await f.fixEnded(b.body.fix.id)).state, 'done')
  assert.equal(f.spawns('machine', 'start').length, 2)
})

test('a fix whose reply is lost is reported lost, never run again', async () => {
  const host = testHost({ lfs: { spec: { cli: '/nonexistent', home: '/nonexistent' } }, root: out, home: out, discovery: TABLE_2_0 })
  let starts = 0
  const fake = async () => ({ home: '/h', state: 'ready', tree: { machines: [{ machine: 'docker', platform: 'docker', state: 'stopped' }], stores: [], buckets: [] } })
  const checks = new R.DoctorChecks(host.host, fake, { actions: { machineStart: async () => { starts += 1; throw Object.assign(new Error('socket closed'), { code: 'Unknown', transport: 'transport-unknown' }) } } })
  await checks.route('GET', ['tree'])
  await checks.route('POST', ['fixes'], { id: id(), kind: 'machine', name: 'docker', fix: 'layerfs machine start docker' })
  const end = await until(() => host.events.find((e) => e.type === 'doctor-fix' && e.data.state !== 'running'), 'the fix to end')
  assert.equal(end.data.state, 'lost')
  assert.equal(end.data.refusal, undefined)
  assert.equal(starts, 1)
})

test('the router sends /api/doctor/… to these routes and leaves the Overview read where it was', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'dsh-doctor-router-'))
  dirs.push(dir)
  const { host } = testHost({ lfs: { spec: { cli: '/nonexistent', home: dir } }, root: dir, home: join(dir, 'home'), discovery: async () => ({ identity: {}, calls: [] }) })
  const groups = R.createGroups(host, async () => ({}))
  try {
    assert.ok(groups.checks instanceof R.DoctorChecks)
    const results = await R.handle(groups, { method: 'GET', path: '/api/doctor/results' })
    assert.equal(results.status, 200)
    assert.deepEqual(results.body, { last: [], running: [], fixes: [] })
    assert.equal((await R.handle(groups, { method: 'POST', path: '/api/doctor/runs', body: { runs: [] } })).status, 400)
    assert.equal((await R.handle(groups, { method: 'GET', path: '/api/doctor/nothing' })).status, 404)
    assert.equal((await R.handle(groups, { method: 'POST', path: '/api/doctor/runs/abcdefgh/stop' })).status, 404)
  } finally { groups.checks.close() }
})

test('the stand-in world is the one the scenarios describe', () => {
  assert.equal(thingKeys(freshState('healthy')).length, 1 + 4 + 9 + 14 + 7 + 7 + 3)
})
