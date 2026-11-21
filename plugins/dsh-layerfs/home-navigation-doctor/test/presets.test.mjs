// Presets (src/host/presets): admission, request-ID reconciliation, restart
// recovery, the router's exclusive guard, script paging, and a live reset of a
// private temporary LAYERFS_HOME over the real pinned CLI.
//   node --test test/presets.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { execFileSync } from 'node:child_process'
import { chmodSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, statSync, writeFileSync, existsSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'
import { loadRoutes, pinned, testHost } from './routes-helper.mjs'
import { MachineStop } from '../src/host/presets/machine-stop.js'
import { EnvironmentReset } from '../src/host/presets/environment-reset.js'
import { CONTENT_VERSION } from '../src/host/presets/recipe/presets.js'

const root = fileURLToPath(new URL('..', import.meta.url))
const out = mkdtempSync(join(tmpdir(), 'dsh-presets-'))
await build({
  absWorkingDir: root, entryPoints: { presets: 'src/host/presets/manager.ts', cli: 'src/host/layerfs/cli.ts', driver: 'src/host/presets/driver.ts', stateDir: 'src/host/presets/state-dir.ts' }, outdir: out,
  bundle: true, format: 'esm', platform: 'node', target: 'node24', logLevel: 'warning', outExtension: { '.js': '.mjs' },
})
const { PresetManager } = await import(pathToFileURL(join(out, 'presets.mjs')).href)
const { REQUIRED_CALLS } = await import(pathToFileURL(join(out, 'cli.mjs')).href)
const { presetStateFor, legacyPresetState } = await import(pathToFileURL(join(out, 'stateDir.mjs')).href)
const R = await loadRoutes()

function fakeHost({ cli = '/nonexistent/layerfs', home, state, sessions = [], projects = [], busy = [], conversations = [] } = {}) {
  const events = [], forgotten = [], stopped = [], deleted = [], order = []
  const of = (stores) => conversations.filter((c) => stores.some((s) => s.name === c.project.split('/')[0]))
  return {
    events, forgotten, stopped, deleted, order,
    host: {
      spec: () => ({ cli, home, bundle: null, daemon: null, state }),
      busy: () => busy,
      boundSessions: () => sessions,
      dshProjects: () => projects.filter((p) => !forgotten.includes(p.project)),
      forgetProject: async (project) => { forgotten.push(project) },
      machinesStopped: async (machines) => { stopped.push(...machines); order.push(...machines.map((m) => `stop:${m}`)) },
      homeStores: async (also) => [...new Set(['store', ...also])].map((name) => ({ name })),
      projectConversations: (stores) => of(stores),
      deleteProjectConversations: async (stores) => {
        const ids = of(stores).map((c) => c.id).filter((id) => !deleted.includes(id))
        deleted.push(...ids); order.push(...ids.map((id) => `delete:${id}`))
        return ids
      },
      emit: (type, data) => events.push({ type, data }),
    },
  }
}

function dirs() {
  const base = mkdtempSync(join(tmpdir(), 'dsh-presets-t-'))
  return { base, state: join(base, 'presets'), home: join(base, 'home') }
}

test('list: the Starter and Studio libraries over an uninitialized home', () => {
  const d = dirs()
  const m = new PresetManager(fakeHost(d).host, d.state, d.home)
  const view = m.list()
  assert.equal(view.busy, false)
  assert.deepEqual(view.items.map((p) => p.id), ['quickstart', 'studio'])
  const starter = view.items[0]
  assert.deepEqual([starter.machines, starter.stores, starter.projects], [['docker'], 1, 1])
  assert.deepEqual(starter.requirements, ['docker'])
  assert.equal(starter.projectGroups[0].kind, 'sqlite')
  assert.ok(starter.projectDefinitions[0].history.commits >= 500 && starter.projectDefinitions[0].history.branches >= 15)
  const studio = view.items[1]
  assert.equal(studio.id, 'studio')
  assert.deepEqual([studio.machines.length, studio.stores, studio.projects], [4, 4, 8])
  assert.equal(studio.initialized, false)
  assert.equal(studio.built, false)
  assert.equal(studio.contentReady, false)
  assert.equal(studio.lastBuild, null)
  assert.ok(studio.projectDefinitions.every((p) => p.history.commits > 0 && p.branches.length > 0))
  assert.deepEqual(studio.requirements, ['docker', 'lima'])
  assert.equal(m.definition('studio', 'mixed').requirements.includes('daemon'), true)
  rmSync(d.base, { recursive: true, force: true })
})

test('list: only the preset the manifest names is built', () => {
  const d = dirs()
  mkdirSync(d.state, { recursive: true })
  writeFileSync(join(d.state, 'demo-fixtures.json'), JSON.stringify({ tool: 'dsh-layerfs-presets', version: 1, preset: 'quickstart', homeId: 'h1', contentVersion: CONTENT_VERSION }))
  const m = new PresetManager(fakeHost(d).host, d.state, d.home)
  const built = Object.fromEntries(m.list().items.map((p) => [p.id, p.built]))
  assert.deepEqual(built, { quickstart: true, studio: false })
  rmSync(d.base, { recursive: true, force: true })
})

test('admission: stop-only reset allows sessions but retains the storage/import/doctor busy gate', async () => {
  const d = dirs()
  const sessions = [{ id: 'session-abc', project: 's/p' }]
  const m = new PresetManager(fakeHost({ ...d, sessions, busy: ['a storage job'] }).host, d.state, d.home)
  const body = { requestId: 'r1', expectedPlan: 'x', confirm: true, rebuild: false, sandboxes: false }
  await assert.rejects(m.resetEnvironment('studio', body), (e) => {
    assert.equal(e.code, 'ConnectBusy'); assert.equal(e.status, 409)
    assert.deepEqual(e.detail.sessions, [])
    assert.deepEqual(e.detail.work, ['a storage job'])
    assert.doesNotMatch(e.message, /open Sandbox/)
    return true
  })
  await assert.rejects(m.resetEnvironment('studio', { ...body, confirm: false }), (e) => e.code === 'InvalidInput')
  // force skips the session check, never the other work
  await assert.rejects(m.resetEnvironment('studio', { ...body, requestId: 'r2', force: true }), (e) => {
    assert.equal(e.code, 'ConnectBusy')
    assert.deepEqual(e.detail.sessions, [])
    assert.deepEqual(e.detail.work, ['a storage job'])
    assert.doesNotMatch(e.message, /open Sandbox/)
    return true
  })
  rmSync(d.base, { recursive: true, force: true })
})

test('admission: confirmed reset is admitted with bound sessions', async () => {
  const d = dirs()
  const m = new PresetManager(fakeHost({ ...d, sessions: [{ id: 'session-abc', project: 's/p' }] }).host, d.state, d.home)
  const body = { requestId: 'r1', expectedPlan: 'x', confirm: true, rebuild: false, sandboxes: false }
  // admitted, then fails inside the run: this home was never initialized
  const job = await m.resetEnvironment('studio', body)
  assert.equal(job.state, 'queued')
  assert.equal(m.exclusive(), true)
  await m.settle()
  assert.equal(job.state, 'failed')
  assert.notEqual(job.error.code, 'ConnectBusy')
  rmSync(d.base, { recursive: true, force: true })
})

test('reset returns a durable queued job before a stalled CLI responds; duplicate requests share it', async () => {
  const d = dirs()
  const cli = join(d.base, 'slow-cli')
  writeFileSync(cli, `#!/usr/bin/env node\nsetTimeout(() => process.exit(1), 1000)\n`)
  chmodSync(cli, 0o700)
  const m = new PresetManager(fakeHost({ ...d, cli }).host, d.state, d.home)
  const body = { requestId: 'slow1', expectedPlan: 'sig', confirm: true, rebuild: false, sandboxes: false }
  try {
    const start = performance.now()
    const job = await m.resetEnvironment('studio', body)
    assert.ok(performance.now() - start < 500, 'admission must not wait for the CLI')
    assert.equal(job.state, 'queued')
    assert.equal(JSON.parse(readFileSync(join(d.state, 'demo-preset-jobs.json'), 'utf8'))[0].id, job.id)
    assert.equal((await m.resetEnvironment('studio', body)).id, job.id)
    assert.equal(m.exclusive(), true)
    await m.settle()
    assert.equal(job.state, 'failed')
    assert.equal(m.exclusive(), false)
  } finally { await m.settle(); rmSync(d.base, { recursive: true, force: true }) }
})

test('restart: running jobs become interrupted, a dead lock is cleared; request IDs reconcile', async () => {
  const d = dirs()
  mkdirSync(d.state, { recursive: true })
  const job = { id: 'j1', requestId: 'r1', fingerprint: JSON.stringify(['studio', 'environment-reset', 'sig', false, false, 'filesystem', false]), preset: 'studio', command: 'environment-reset', executionMode: 'filesystem', rebuild: false, actionId: 'a', state: 'running', phase: 'x', created: 1, log: [] }
  writeFileSync(join(d.state, 'demo-preset-jobs.json'), JSON.stringify([job]))
  writeFileSync(join(d.state, 'demo-fixtures.lock'), JSON.stringify({ pid: 2 ** 22 + 12345, actionId: 'a' }))
  const m = new PresetManager(fakeHost(d).host, d.state, d.home)
  assert.equal(existsSync(join(d.state, 'demo-fixtures.lock')), false)
  const seen = await m.route('GET', ['requests', 'r1'], undefined, new URLSearchParams())
  assert.equal(seen.state, 'interrupted')
  assert.equal(m.exclusive(), false)
  // the same request answers the same job; other settings under its ID are refused
  const same = await m.resetEnvironment('studio', { requestId: 'r1', expectedPlan: 'sig', confirm: true, rebuild: false, sandboxes: false })
  assert.equal(same.id, 'j1')
  await assert.rejects(m.resetEnvironment('studio', { requestId: 'r1', expectedPlan: 'other', confirm: true, rebuild: false, sandboxes: false }), (e) => e.code === 'Conflict')
  await assert.rejects(m.route('GET', ['requests', 'nope'], undefined, new URLSearchParams()), (e) => e.status === 404)
  assert.equal(await m.route('GET', ['studio', 'unknown-op'], undefined, new URLSearchParams()), undefined)
  rmSync(d.base, { recursive: true, force: true })
})

test('script: paged snapshot, scoped to a project, without touching the home', async () => {
  const d = dirs()
  const m = new PresetManager(fakeHost(d).host, d.state, d.home)
  const first = await m.route('GET', ['studio', 'script-no-sandboxes'], undefined, new URLSearchParams({ offset: '0', limit: '100' }))
  assert.equal(first.commands.length, 100)
  assert.ok(first.total > 1000)
  assert.equal(first.next, 100)
  assert.equal(first.dynamicReset, true)
  assert.equal(first.configuration.home, d.home)
  const second = await m.route('GET', ['studio', 'script-no-sandboxes'], undefined, new URLSearchParams({ offset: '100', limit: '100', snapshot: first.snapshot }))
  assert.equal(second.offset, 100)
  assert.equal(second.snapshot, first.snapshot)
  const scoped = await m.route('GET', ['studio', 'script-no-sandboxes'], undefined, new URLSearchParams({ offset: '0', limit: '1000', kind: 'project', name: 'demo-studio/website' }))
  assert.ok(scoped.commands.length > 0)
  assert.ok(scoped.commands.every((c) => c.phase !== 'reset' && c.argv.join(' ').includes('demo-studio/website')))
  await assert.rejects(m.route('GET', ['studio', 'script'], undefined, new URLSearchParams({ offset: '0', snapshot: first.snapshot })), (e) => e.code === 'Conflict')
  assert.equal(existsSync(d.home), false)
  rmSync(d.base, { recursive: true, force: true })
})

test('router: while a preset holds the home, changes are refused and reads pass', async () => {
  const base = mkdtempSync(join(tmpdir(), 'dsh-presets-r-'))
  const lfs = new Proxy({ spec: { cli: '/fake', home: base } }, { get: (t, f) => f in t ? t[f] : new Proxy({}, { get: () => async () => ({ items: [], machines: [] }) }) })
  const t = testHost({ lfs, root: base, home: join(base, 'home'), discovery: async () => ({ identity: {}, calls: [], commands: [], switches: [] }) })
  const groups = R.createGroups(t.host, async () => ({ ok: true }))
  let held = true
  groups.presets = { exclusive: () => held, route: async (method, parts) => ({ method, parts }) }
  const refused = await R.handle(groups, { method: 'POST', path: '/api/stores', body: { name: 's' } })
  assert.equal(refused.status, 409)
  assert.equal(refused.body.code, 'ConnectBusy')
  const preset = await R.handle(groups, { method: 'POST', path: '/api/demo-presets/studio/environment-reset', body: {} })
  assert.deepEqual(preset, { status: 200, body: { method: 'POST', parts: ['studio', 'environment-reset'] } })
  const read = await R.handle(groups, { method: 'GET', path: '/api/demo-presets' })
  assert.equal(read.status, 200)
  held = false
  const allowed = await R.handle(groups, { method: 'POST', path: '/api/stores', body: { name: 's' } })
  assert.notEqual(allowed.body?.code, 'ConnectBusy')
  groups.storage.close(); await groups.projects.close()
  rmSync(base, { recursive: true, force: true })
})

// ---------------------------------------------------------------- live

const pin = pinned()

test('live: Build cleanup still removes data from a private home', { skip: !pin && 'no pinned LayerFS CLI', timeout: 300_000 }, async () => {
  const base = mkdtempSync('/tmp/lfsp-')
  const home = join(base, 'h'), state = join(base, 'presets')
  const env = { ...process.env, LAYERFS_HOME: home }
  const cli = (...argv) => JSON.parse(execFileSync(pin.cli, argv, { env, encoding: 'utf8' }))
  try {
    // a 2.0 CLI makes no home on a read: `home init` first
    cli('home', 'init'); cli('store', 'create', '--', 'ps')
    const src = join(base, 'src'); mkdirSync(src); writeFileSync(join(src, 'a.txt'), 'a\n')
    cli('project', 'init', '--', 'ps/p1', src)
    const { CliDriver } = await import(pathToFileURL(join(out, 'driver.mjs')).href)
    const reset = new EnvironmentReset({ spec: { cli: pin.cli, home, state }, driver: new CliDriver({ cli: pin.cli, home, state }) })
    const plan = await reset.plan()
    assert.deepEqual(plan.remove.stores, ['ps'])
    assert.deepEqual(plan.projectKeys, ['ps/p1'])
    const receipt = await reset.run({ expectedPlan: plan.signature, assertIdle: async () => {} })
    assert.equal(receipt.verified, true)
    assert.deepEqual(cli('store', 'list').items, [])
  } finally {
    try { execFileSync(pin.cli, ['machine', 'stop', 'native'], { env, stdio: 'ignore' }) } catch {}
    rmSync(base, { recursive: true, force: true })
  }
})

function stoppingFixture({ fail = null, forceSupport = true } = {}) {
  const directory = join(root, '.dev-preset-stop-tests')
  mkdirSync(directory, { recursive: true })
  const base = mkdtempSync(join(directory, 'case-'))
  const home = join(base, 'home'), state = join(base, 'presets')
  mkdirSync(home); mkdirSync(state)
  writeFileSync(join(home, 'layerfs.json'), JSON.stringify({ id: 'stop-test-home' }))
  // `machine list --detail` rows (2.0): keyed `machine`, six states
  const machines = [{ machine: 'native', state: 'ready', created_at: '2026-11-01T00:00:00Z' }, { machine: 'docker', state: 'ready', created_at: '2026-11-01T00:00:00Z' }, { machine: 'vm', state: 'stopped', created_at: '2026-11-02T00:00:00Z' }]
  const inventory = join(base, 'machines.json'), commands = join(base, 'commands.jsonl'), cli = join(base, 'layerfs.mjs')
  writeFileSync(inventory, JSON.stringify(machines))
  writeFileSync(cli, `#!/usr/bin/env node
import fs from 'node:fs'
const args = process.argv.slice(2)
const inventory = ${JSON.stringify(inventory)}, commands = ${JSON.stringify(commands)}, fail = ${JSON.stringify(fail)}
if (args[0] === '--version') console.log(JSON.stringify({commit:'a'.repeat(40)}))
else if (args[0] === 'help') console.log(JSON.stringify({calls:${JSON.stringify([...REQUIRED_CALLS.map(name => { const [family, call] = name.split('.'); return { family, call, ...(name === 'home.get' ? { form: { with: { platforms: {}, tree: {} } } } : {}) } }), { family: 'machine', call: 'stop', options: forceSupport ? [{ name: 'force', kind: 'bool' }] : [] }])}}))
else {
  fs.appendFileSync(commands, JSON.stringify(args)+'\\n')
  const machines = JSON.parse(fs.readFileSync(inventory))
  if (args.join(' ') === 'machine list --detail') console.log(JSON.stringify({items: machines}))
  else if (args[0] === 'machine' && args[1] === 'stop' && args[3] === '--force' && args[4] === '--detail') {
    if (args[2] === fail) { console.error(JSON.stringify({error:'Failed',message:'stop failed'})); process.exit(125) }
    const machine = machines.find(row => row.machine === args[2]); machine.state = machine.machine === 'native' ? 'ready' : 'stopped'
    fs.writeFileSync(inventory, JSON.stringify(machines)); console.log(JSON.stringify({ok:true,unchanged:false,stopped_sandboxes:[],stopped_sandboxes_count:0,unknown_workspaces:[],unknown_workspaces_count:0,outcome:'stopped',machine:machine.machine,forced:true}))
  } else { console.error(JSON.stringify({error:'InvalidInput',message:args.join(' ')})); process.exit(125) }
}
`)
  chmodSync(cli, 0o700)
  return { base, home, state, cli, commands, inventory }
}

const recordedCommands = fixture => readFileSync(fixture.commands, 'utf8').trim().split('\n').map(JSON.parse)

test('Reset only force-stops registered machines, preserves content/manifest/journal and invalidates stopped bindings', async () => {
  const fixture = stoppingFixture()
  try {
    const manifest = { tool: 'kept-tool', homeId: 'stop-test-home', contentVersion: CONTENT_VERSION, executionMode: 'filesystem', projects: { 'store/project': { retained: true } }, resources: { store: { retained: true } } }
    const manifestText = JSON.stringify(manifest), journalText = 'existing history receipt\n'
    writeFileSync(join(fixture.state, 'demo-fixtures.json'), manifestText)
    writeFileSync(join(fixture.state, 'demo-steps.ndjson'), journalText)
    const buildJob = { id: 'build', requestId: 'build-request', preset: 'studio', rebuild: true, state: 'completed', presetVerified: true, executionMode: 'filesystem', created: 1, ended: 2, log: [] }
    writeFileSync(join(fixture.state, 'demo-preset-jobs.json'), JSON.stringify([buildJob]))
    const conversations = [{ id: 'session-active', project: 'store/project' }, { id: 'session-old', project: 'store/project' }, { id: 'session-elsewhere', project: 'other-home-store/p' }]
    const fake = fakeHost({ ...fixture, sessions: [{ id: 'session-active', project: 'store/project' }], projects: [{ project: 'store/project', sessions: 1 }], conversations })
    const manager = new PresetManager(fake.host, fixture.state, fixture.home)
    const plan = await manager.environmentPlan('studio')
    assert.equal(plan.scope, 'machines')
    assert.deepEqual(plan.stopMachines, ['docker', 'native', 'vm'])
    assert.deepEqual(plan.dsh.workspaces, [])
    assert.deepEqual(plan.dsh.conversations.map((c) => c.id), ['session-active', 'session-old'], 'the plan names this home\'s project sessions only')
    const job = await manager.resetEnvironment('studio', { requestId: 'stop', expectedPlan: plan.signature, confirm: true, rebuild: false, sandboxes: false })
    await manager.settle()
    assert.equal(job.state, 'completed', job.error?.message)
    assert.equal(job.command, 'machine-stop')
    assert.equal(job.cleanup.verified, true)
    assert.deepEqual(job.cleanup.completed, ['docker', 'native', 'vm'].map(name => ['machine', 'stop', name, '--force', '--detail']))
    assert.deepEqual(fake.stopped, ['docker', 'native', 'vm'])
    assert.deepEqual(fake.forgotten, [])
    assert.deepEqual(job.deletedSessions, ['session-active', 'session-old'])
    assert.deepEqual(fake.order, ['stop:docker', 'stop:native', 'stop:vm', 'delete:session-active', 'delete:session-old'], 'sessions are deleted after every machine stopped')
    assert.equal(readFileSync(join(fixture.state, 'demo-fixtures.json'), 'utf8'), manifestText)
    assert.equal(readFileSync(join(fixture.state, 'demo-steps.ndjson'), 'utf8'), journalText)
    assert.equal(manager.definition('studio').contentReady, true, 'stopping machines does not erase build verification')
    assert.deepEqual(manager.definition('studio').lastBuild, { ended: 2, executionMode: 'filesystem', verified: true }, 'the status line keeps the last Build, not the later stop')
    assert.ok(recordedCommands(fixture).every(argv => argv[0] === 'machine' && ['list', 'stop'].includes(argv[1])), 'no worker inspection, boot, unmount or resource deletion')
  } finally { rmSync(fixture.base, { recursive: true, force: true }) }
})

test('a partial stop failure still invalidates successful machines and attempts remaining targets', async () => {
  const fixture = stoppingFixture({ fail: 'native' })
  try {
    const fake = fakeHost({ ...fixture, conversations: [{ id: 's1', project: 'store/project' }] })
    const manager = new PresetManager(fake.host, fixture.state, fixture.home)
    const plan = await manager.environmentPlan('studio')
    const job = await manager.resetEnvironment('studio', { requestId: 'partial', expectedPlan: plan.signature, confirm: true, rebuild: false, sandboxes: false })
    await manager.settle()
    assert.equal(job.state, 'failed')
    assert.deepEqual(fake.stopped, ['docker', 'vm'])
    assert.deepEqual(fake.deleted, [], 'a failed stop deletes no session')
    assert.equal(job.cleanup.verified, false)
    assert.deepEqual(job.cleanup.completed, ['docker', 'vm'].map(name => ['machine', 'stop', name, '--force', '--detail']))
    assert.deepEqual(JSON.parse(readFileSync(fixture.inventory)).map(row => row.machine).sort(), ['docker', 'native', 'vm'])
  } finally { rmSync(fixture.base, { recursive: true, force: true }) }
})

test('a changed stop-plan signature refuses mutation before stopping any machine', async () => {
  const fixture = stoppingFixture()
  try {
    const fake = fakeHost(fixture), manager = new PresetManager(fake.host, fixture.state, fixture.home)
    const job = await manager.resetEnvironment('studio', { requestId: 'stale-stop', expectedPlan: 'stale', confirm: true, rebuild: false, sandboxes: false })
    await manager.settle()
    assert.equal(job.error?.code, 'Conflict')
    assert.deepEqual(fake.stopped, [])
    assert.ok(recordedCommands(fixture).every(argv => argv[1] === 'list'))
  } finally { rmSync(fixture.base, { recursive: true, force: true }) }
})


test('Reset rejects a CLI without force-stop support before confirmation or any machine operation', async () => {
  const fixture = stoppingFixture({ forceSupport: false })
  try {
    const fake = fakeHost(fixture), manager = new PresetManager(fake.host, fixture.state, fixture.home)
    await assert.rejects(manager.environmentPlan('studio'), error => {
      assert.equal(error.code, 'CliIncompatible')
      assert.equal(error.status, 409)
      assert.match(error.message, /machine.stop.*force/)
      assert.match(error.message, /Center → Settings → Change…/)
      return true
    })
    assert.equal(existsSync(fixture.commands), false, 'capability discovery must not execute inventory or stop commands')
    const job = await manager.resetEnvironment('studio', { requestId: 'unsupported-stop', expectedPlan: 'old-plan', confirm: true, rebuild: false, sandboxes: false })
    await manager.settle()
    assert.equal(job.state, 'failed')
    assert.equal(job.error.code, 'CliIncompatible')
    assert.equal(existsSync(fixture.commands), false, 'a direct request cannot bypass the capability guard')
    assert.equal(existsSync(join(fixture.state, 'machine-stop.json')), false, 'no stop receipt is manufactured')
    assert.equal(existsSync(join(fixture.state, 'demo-fixtures.lock')), false, 'admission is released')
    assert.deepEqual(fake.stopped, [])
  } finally { rmSync(fixture.base, { recursive: true, force: true }) }
})

test('live: the selected legacy CLI is rejected before Reset reaches its private home', { skip: !pin && 'no pinned LayerFS CLI' }, async t => {
  const fixture = stoppingFixture({ forceSupport: false })
  try {
    const { CliDriver } = await import(pathToFileURL(join(out, 'driver.mjs')).href)
    const spec = { ...fixture, cli: pin.cli }, driver = new CliDriver(spec)
    const discovery = await driver.discover()
    const force = discovery.calls.find(call => call.family === 'machine' && call.call === 'stop')?.options?.some(option => option.name === 'force' && option.kind === 'bool')
    if (force) return t.skip('the selected CLI already advertises force-stop; supported execution is covered above')
    const original = readFileSync(join(fixture.home, 'layerfs.json'), 'utf8')
    const reset = new MachineStop({ spec, driver })
    await assert.rejects(reset.plan(), error => error.code === 'CliIncompatible')
    await assert.rejects(reset.run({ expectedPlan: 'untrusted-old-plan' }), error => error.code === 'CliIncompatible')
    assert.equal(readFileSync(join(fixture.home, 'layerfs.json'), 'utf8'), original)
    assert.equal(existsSync(join(fixture.state, 'machine-stop.json')), false)
    assert.equal(existsSync(join(fixture.state, 'demo-fixtures.lock')), false)
  } finally { rmSync(fixture.base, { recursive: true, force: true }) }
})

test('presets state: each home keeps it in <home>/preset; an earlier <root>/presets/homes/<digest> moves in with its recorded paths rewritten', () => {
  const base = mkdtempSync(join(tmpdir(), 'dsh-preset-state-'))
  try {
    const pluginRoot = join(base, 'root'), oldHome = join(base, 'old-home'), newHome = join(base, 'new-home')
    for (const [dir, id] of [[oldHome, 'a'.repeat(32)], [newHome, 'b'.repeat(32)]]) { mkdirSync(dir, { recursive: true }); writeFileSync(join(dir, 'layerfs.json'), JSON.stringify({ id })) }
    // a fresh home: its own folder inside it, named for no client
    assert.equal(presetStateFor(pluginRoot, newHome), join(newHome, 'preset'))
    // an earlier build's per-home folder moves into the home
    const legacy = legacyPresetState(pluginRoot, oldHome)
    mkdirSync(join(legacy, 'demo-sources', 'st', 'p'), { recursive: true })
    const real = realpathSync(legacy)
    writeFileSync(join(legacy, 'demo-fixtures.json'), JSON.stringify({ homeId: 'a'.repeat(32), state: real }))
    writeFileSync(join(legacy, 'demo-steps.ndjson'), JSON.stringify({ argv: ['init', `${real}/demo-sources/st/p`] }) + '\n')
    const target = presetStateFor(pluginRoot, oldHome)
    assert.equal(target, join(oldHome, 'preset'))
    assert.ok(!existsSync(legacy))
    assert.equal(statSync(target).mode & 0o777, 0o700)
    const to = realpathSync(target)
    assert.equal(JSON.parse(readFileSync(join(target, 'demo-fixtures.json'), 'utf8')).state, to)
    assert.equal(JSON.parse(readFileSync(join(target, 'demo-steps.ndjson'), 'utf8')).argv[1], `${to}/demo-sources/st/p`)
    assert.ok(existsSync(join(target, 'demo-sources', 'st', 'p')))
    // stable afterwards; the flat <root>/presets files are never consulted
    mkdirSync(join(pluginRoot, 'presets'), { recursive: true })
    writeFileSync(join(pluginRoot, 'presets', 'demo-fixtures.json'), JSON.stringify({ homeId: 'a'.repeat(32) }))
    assert.equal(presetStateFor(pluginRoot, oldHome), target)
  } finally { rmSync(base, { recursive: true, force: true }) }
})
