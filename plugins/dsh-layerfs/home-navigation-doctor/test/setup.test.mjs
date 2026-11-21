// The setup check's host pieces (docs/specs/layerfs-setup.md §3): build
// verdicts from stamps, pin leases, bundle replacement while no worker runs,
// and the home gate over a 2.0 CLI's `home get` (ready, or broken with reason
// missing / outdated) and `home init`.
//   node --test test/setup.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

const root = fileURLToPath(new URL('..', import.meta.url))
mkdirSync(join(root, 'lib'), { recursive: true })
const out = mkdtempSync(join(root, 'lib', '.test-setup-'))
await build({
  stdin: {
    contents: "export * from './src/host/layerfs/runtime-home.ts'\nexport { LayerfsService } from './src/host/service.ts'\nexport { client } from './src/host/layerfs/cli.ts'\nexport { Context } from '@deepseek-ai/cordis'\n",
    resolveDir: root,
    loader: 'ts',
  },
  bundle: true, platform: 'node', format: 'esm', external: ['@deepseek-ai/*'],
  outfile: join(out, 'setup.mjs'), absWorkingDir: root, target: 'node24', logLevel: 'warning',
})
const H = await import(pathToFileURL(join(out, 'setup.mjs')).href)
process.on('exit', () => rmSync(out, { recursive: true, force: true }))

const A = 'a'.repeat(40), B = 'b'.repeat(40)

test('verdicts follow LayerFS `matches`: other commit, same and both clean, else unknown; no piece is missing', () => {
  assert.equal(H.verdictOf({ commit: A, dirty: false }, { commit: B, dirty: false }), 'other')
  assert.equal(H.verdictOf({ commit: A, dirty: false }, { commit: A, dirty: false }), 'matches')
  assert.equal(H.verdictOf({ commit: A, dirty: true }, { commit: A, dirty: false }), 'unknown')
  assert.equal(H.verdictOf({ commit: A, dirty: false }, { commit: A, dirty: null }), 'unknown')
  assert.equal(H.verdictOf({ commit: A, dirty: false }, { commit: null, dirty: null }), 'unknown')
  assert.equal(H.verdictOf({ commit: A, dirty: false }, null), 'missing')
})

test('stamps: bundle.json source.head/dirty, the daemon build identity read from the file, none for older pieces', () => {
  const dir = mkdtempSync(join(tmpdir(), 'dsh-setup-'))
  try {
    mkdirSync(join(dir, 'bundle'))
    writeFileSync(join(dir, 'bundle', 'bundle.json'), JSON.stringify({ source: { head: A, dirty: false } }))
    assert.deepEqual(H.bundleStamp(join(dir, 'bundle')), { commit: A, dirty: false })
    writeFileSync(join(dir, 'bundle', 'bundle.json'), JSON.stringify({ source: { head: A } }))
    assert.deepEqual(H.bundleStamp(join(dir, 'bundle')), { commit: A, dirty: null })
    assert.equal(H.bundleStamp(join(dir, 'nope')), null)
    writeFileSync(join(dir, 'daemon'), Buffer.concat([Buffer.alloc(4096, 1), Buffer.from(`layerfs-build-identity:${B}:dirty\0`), Buffer.alloc(64)]))
    assert.deepEqual(H.daemonStamp(join(dir, 'daemon')), { commit: B, dirty: true })
    writeFileSync(join(dir, 'old-daemon'), Buffer.alloc(1024, 2))
    assert.deepEqual(H.daemonStamp(join(dir, 'old-daemon')), { commit: null, dirty: null })
    assert.equal(H.daemonStamp(join(dir, 'missing')), null)
  } finally { rmSync(dir, { recursive: true, force: true }) }
})

function runtimeFixture() {
  const dir = mkdtempSync(join(tmpdir(), 'dsh-setup-'))
  const src = join(dir, 'src')
  mkdirSync(join(src, 'bundle', 'linux-aarch64'), { recursive: true })
  writeFileSync(join(src, 'layerfs'), '#!/bin/sh\necho cli\n', { mode: 0o755 })
  writeFileSync(join(src, 'bundle', 'linux-aarch64', 'bundle.json'), JSON.stringify({ hash: 'h1' }))
  return { dir, src, home: join(dir, 'home'), done: () => rmSync(dir, { recursive: true, force: true }) }
}

test('a different bundle for the same CLI replaces the old one only when no worker runs', () => {
  const f = runtimeFixture()
  try {
    const given = { cli: join(f.src, 'layerfs'), daemon: null, bundle: join(f.src, 'bundle', 'linux-aarch64') }
    const a = H.installRuntime(f.home, '2.0.0', given)
    writeFileSync(join(f.src, 'bundle', 'linux-aarch64', 'bundle.json'), JSON.stringify({ hash: 'h2' }))
    assert.throws(() => H.installRuntime(f.home, '2.0.0', given), /different Linux bundle/)
    const b = H.installRuntime(f.home, '2.0.0', given, { replaceBundle: true })
    assert.equal(b.bundle, a.bundle)
    assert.equal(JSON.parse(readFileSync(join(b.bundle, 'bundle.json'), 'utf8')).hash, 'h2')
    assert.deepEqual(readdirSync(join(b.bundle, '..')), ['linux-aarch64'])
  } finally { f.done() }
})

test('a pin lease keeps a runtime from another host\'s cleanup and moves with the pin', () => {
  const f = runtimeFixture()
  try {
    const one = H.installRuntime(f.home, '1.0.0', { cli: join(f.src, 'layerfs'), daemon: null, bundle: null })
    writeFileSync(join(f.src, 'layerfs'), '#!/bin/sh\necho cli-2\n', { mode: 0o755 })
    const two = H.installRuntime(f.home, '2.0.0', { cli: join(f.src, 'layerfs'), daemon: null, bundle: null })
    H.leasePin(f.home, f.dir, one.cli)
    const id = H.hostId(f.dir)
    assert.ok(existsSync(join(f.home, 'runtime', one.id, 'pins', id)))
    const plan = H.planRuntimeCleanup(H.installedRuntimes(f.home), { pinned: [], machines: { items: [] }, processes: [], newest: 0 })
    assert.equal(plan.keep.get(one.id), 'lease')
    assert.deepEqual(plan.remove.map((r) => r.id), [two.id])
    H.leasePin(f.home, f.dir, two.cli)
    assert.ok(!existsSync(join(f.home, 'runtime', one.id, 'pins', id)))
    assert.ok(existsSync(join(f.home, 'runtime', two.id, 'pins', id)))
  } finally { f.done() }
})

/**
 * A fake 2.0 LayerFS CLI (node script) logging its argv: `home get` answers the 2.0 shape,
 * `home init` makes a missing folder. An older LayerFS's folder (layout 2) is `broken`
 * `outdated` and never converted (CUT): `home init` and every other call refuse it
 * `Unsupported` with the fix that moves it aside.
 */
function fakeCli(dir, { homeGetFails = false, slowGetMs = 0, recorded = null }) {
  const log = join(dir, 'calls.log')
  // the host's REQUIRED_CALLS (2.0 names) and what this test drives
  const required = ['project.init', 'project.list', 'project.get', 'project.export', 'branch.fork', 'branch.list', 'commit.list', 'commit.diff',
    'file.read', 'file.list', 'file.apply', 'sandbox.create', 'sandbox.list', 'sandbox.get', 'sandbox.remove',
    'workspace.mount', 'workspace.commit', 'workspace.status', 'workspace.unmount', 'workspace.exec', 'workspace.cancel',
    'machine.list', 'store.list', 'store.get', 'store.create', 'machine.get', 'machine.update', 'home.get', 'home.init']
  const calls = required.map((n) => ({ family: n.split('.')[0], call: n.split('.')[1], args: n === 'machine.get' || n === 'machine.update' ? [{ name: 'name', kind: 'text' }] : [], options: [], ...(n === 'home.get' ? { form: { with: { platforms: {}, tree: {} } } } : {}) }))
  const cli = join(dir, 'layerfs')
  writeFileSync(cli, `#!/usr/bin/env node
const fs = require('fs'), path = require('path')
const argv = process.argv.slice(2), home = process.env.LAYERFS_HOME
fs.appendFileSync(${JSON.stringify(log)}, argv.filter((a) => a !== '--').join(' ') + '\\n')
const say = (v) => { process.stdout.write(JSON.stringify(v) + '\\n'); process.exit(0) }
if (argv[0] === '--version') say({ version: '9.9.9', commit: '${A}', dirty: false, features: [] })
if (argv[0] === 'help') say({ calls: ${JSON.stringify(calls)}, commands: [], switches: [] })
const file = path.join(home, 'layerfs.json')
const st = fs.existsSync(file) ? (JSON.parse(fs.readFileSync(file, 'utf8')).layout === 2 ? 'outdated' : 'current') : 'missing'
const older = { error: 'Unsupported', message: home + ' is the home of an older LayerFS; this build does not convert it: move it aside and make a new home', fix: 'mv ' + home + ' ' + home + '.old && layerfs home init' }
const create = () => { fs.mkdirSync(home, { recursive: true }); fs.writeFileSync(file, JSON.stringify({ layout: 3 })) }
if (argv[0] === 'home' && argv[1] === 'get' && ${homeGetFails}) { process.stderr.write('boom\\n'); process.exit(1) }
if (argv[0] === 'home' && argv[1] === 'get' && ${slowGetMs}) { const until = Date.now() + ${slowGetMs}; while (Date.now() < until) {} }
if (argv[0] === 'home' && argv[1] === 'get') say({ home, state: st === 'current' ? 'ready' : 'broken', version: '2.0.0', generation: 1, reason: st === 'current' ? null : st === 'missing' ? 'missing' : 'outdated', fix: st === 'current' ? null : st === 'missing' ? 'layerfs home init' : older.fix, message: st === 'outdated' ? older.message : null, layout: st === 'missing' ? null : st === 'current' ? 3 : 2, mode: '0700', bundle: null, daemon: ${JSON.stringify(recorded)}, cache: { path: '/c' } })
if (st === 'outdated') { process.stderr.write(JSON.stringify(older) + '\\n'); process.exit(125) }
if (argv[0] === 'home' && argv[1] === 'init') { const was = st; create(); say({ ok: true, outcome: was === 'missing' ? 'created' : 'unchanged', home, layout: 3 }) }
// every other call opens a missing home as LayerFS does (creates it)
create()
if (argv[0] === 'machine' && argv[1] === 'list') say({ items: [] })
say({})
`, { mode: 0o755 })
  return { cli, calls: () => (existsSync(log) ? readFileSync(log, 'utf8').trim().split('\n').filter(Boolean) : []) }
}

async function withService(opts, run) {
  const dir = mkdtempSync(join(tmpdir(), 'dsh-setup-gate-'))
  const keys = ['DSH_HOME', 'DSH_LAYERFS_HOME', 'LAYERFS_HOME', 'DSH_LAYERFS_CLI', 'DSH_LAYERFS_DAEMON', 'DSH_LAYERFS_BUNDLE', 'LAYERFS_BUNDLE']
  const saved = Object.fromEntries(keys.map((k) => [k, process.env[k]]))
  try {
    for (const k of keys) delete process.env[k]
    process.env.DSH_HOME = join(dir, 'dsh')
    const home = join(dir, 'home')
    if (opts.layout2) { mkdirSync(home, { recursive: true, mode: 0o700 }); writeFileSync(join(home, 'layerfs.json'), JSON.stringify({ layout: 2 })) }
    const fake = fakeCli(dir, opts)
    const s = new H.LayerfsService(new H.Context(), { cli: fake.cli, home, ...(opts.daemon ? { daemon: opts.daemon } : {}) })
    await run({ s, home, fake })
  } finally {
    for (const [k, v] of Object.entries(saved)) { if (v === undefined) delete process.env[k]; else process.env[k] = v }
    rmSync(dir, { recursive: true, force: true })
  }
}

test('setup check: a missing folder is created with home init; a read waits for it', async () => {
  await withService({}, async ({ s, home, fake }) => {
    await s.lfs.store.list()
    assert.ok(existsSync(join(home, 'layerfs.json')))
    assert.ok(fake.calls().includes('home init --detail'), fake.calls().join('\n'))
    const status = await s.setup()
    assert.equal(status.folder.state, 'current')
    assert.equal(status.homeCalls, true)
    assert.deepEqual(status.needs, [])
  })
})

// CUT (LayerFS 2.0, 2026-11-19): an older LayerFS's folder is not converted. Every call is
// refused Uninitialized, the setup status carries LayerFS's sentence and its fix (move it
// aside, then `home init`) for the page to show, and nothing writes the folder.
test('setup check: an older LayerFS\'s folder refuses every call Uninitialized; its fix is shown, never run', async () => {
  await withService({ layout2: true }, async ({ s, home, fake }) => {
    await assert.rejects(() => s.lfs.store.list(), (e) => e.code === 'Uninitialized' && /does not convert/.test(e.message))
    await assert.rejects(() => s.lfs.machine.update('native', {}), (e) => e.code === 'Uninitialized')
    const status = await s.setup()
    assert.equal(status.folder.state, 'outdated')
    assert.deepEqual(status.needs, ['folder'])
    assert.equal(status.folder.fix, `mv ${home} ${home}.old && layerfs home init`)
    assert.match(status.folder.message, /older LayerFS/)
    assert.equal(status.folder.did, null)
    // `home init` (a refusal's fix on a missing folder) is LayerFS's to refuse here; the folder stays as it was
    await assert.rejects(() => s.initHome(), (e) => e.code === 'Unsupported')
    assert.equal(JSON.parse(readFileSync(join(home, 'layerfs.json'), 'utf8')).layout, 2, 'nothing converted it')
    assert.ok(!fake.calls().some((c) => c.startsWith('store ') || c.startsWith('machine ')), fake.calls().join('\n'))
  })
})

test('setup check: a failing home get is unknown, never current, and the gate converts nothing', async () => {
  await withService({ homeGetFails: true, layout2: true }, async ({ s, home }) => {
    const status = await s.setup()
    assert.equal(status.folder.state, 'unknown')
    assert.deepEqual(status.needs, ['folder'])
    await assert.rejects(() => s.lfs.store.list(), (e) => e.code === 'Uninitialized')
    assert.equal(JSON.parse(readFileSync(join(home, 'layerfs.json'), 'utf8')).layout, 2)
  })
})

test('kept reads: home get stays kept between writes; a read in flight across a write is neither kept nor shared', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'dsh-setup-kept-'))
  try {
    const home = join(dir, 'home')
    const fake = fakeCli(dir, { slowGetMs: 400 })
    const lfs = H.client({ cli: fake.cli, home })
    const gets = () => fake.calls().filter((c) => c === 'home get --detail').length
    // a read starts, then a write runs to completion while it is still running
    const early = lfs.home.get({ detail: true })
    const until = Date.now() + 10_000
    while (gets() < 1 && Date.now() < until) await new Promise((r) => setTimeout(r, 10))
    await lfs.home.init({ detail: true })
    const late = lfs.home.get({ detail: true })
    assert.equal((await early).reason, 'missing', 'the early read saw no folder')
    assert.equal((await late).state, 'ready', 'a read asked after the write is not the early one')
    assert.equal(gets(), 2)
    // between writes home get stays kept: the late answer serves the next read
    assert.equal((await lfs.home.get({ detail: true })).state, 'ready', 'the early answer was not kept')
    assert.equal(gets(), 2, 'a read inside the keep window after the late one is its kept answer')
  } finally { rmSync(dir, { recursive: true, force: true }) }
})

test('a stopped docker counts as idle for replacing a bundle; a ready (running) one does not', async () => {
  await withService({}, async ({ s, home }) => {
    mkdirSync(home, { recursive: true })
    writeFileSync(join(home, 'layerfs.json'), JSON.stringify({ layout: 3 }))
    // 2.0 records: native is always ready (no worker); a worker machine runs when ready or busy
    s.sdk = { machine: { list: async () => ({ items: [{ machine: 'native', platform: 'native', state: 'ready' }, { machine: 'docker', platform: 'docker', state: 'stopped' }] }) } }
    assert.equal(await s.workersIdle(), true)
    s.sdk = { machine: { list: async () => ({ items: [{ machine: 'docker', platform: 'docker', state: 'ready' }] }) } }
    assert.equal(await s.workersIdle(), false)
    s.sdk = { machine: { list: async () => ({ items: [{ machine: 'docker', platform: 'docker', state: 'busy', op: { name: 'machine.stop', phase: null, started_at: '2026-11-18T09:00:00Z' } }] }) } }
    assert.equal(await s.workersIdle(), false)
  })
})

test('a lease whose host root is gone holds nothing', () => {
  const f = runtimeFixture()
  try {
    const one = H.installRuntime(f.home, '1.0.0', { cli: join(f.src, 'layerfs'), daemon: null, bundle: null })
    mkdirSync(join(one.cli, '..', '..', 'pins'), { recursive: true })
    writeFileSync(join(one.cli, '..', '..', 'pins', 'gone'), JSON.stringify({ root: join(f.dir, 'no-such-root') }))
    assert.equal(H.leased(join(one.cli, '..', '..')), false)
    writeFileSync(join(one.cli, '..', '..', 'pins', 'live'), JSON.stringify({ root: f.dir }))
    assert.equal(H.leased(join(one.cli, '..', '..')), true)
  } finally { f.done() }
})

test('a service built in a test writes no pin lease until the plugin starts it', () => {
  const f = runtimeFixture()
  const saved = process.env.DSH_HOME, pinned = process.env.DSH_LAYERFS_CLI
  try {
    process.env.DSH_HOME = join(f.dir, 'dsh')
    // the newest runtime in the home is the pin only when no CLI is named (a test run may name the pinned one)
    delete process.env.DSH_LAYERFS_CLI
    const one = H.installRuntime(f.home, '1.0.0', { cli: join(f.src, 'layerfs'), daemon: null, bundle: null })
    const s = new H.LayerfsService(new H.Context(), { home: f.home })
    assert.equal(s.cli, one.cli)
    assert.ok(!existsSync(join(f.home, 'runtime', one.id, 'pins')), 'no lease before startSetup')
  } finally {
    if (saved === undefined) delete process.env.DSH_HOME; else process.env.DSH_HOME = saved
    if (pinned !== undefined) process.env.DSH_LAYERFS_CLI = pinned
    f.done()
  }
})

test('Use this daemon is offered only when the recorded daemon is gone or from another build, not for a same-build copy elsewhere', { skip: process.platform !== 'darwin' }, async () => {
  const dir = mkdtempSync(join(tmpdir(), 'dsh-setup-daemon-'))
  try {
    const ours = join(dir, 'layerfs-daemon')
    writeFileSync(ours, Buffer.concat([Buffer.alloc(256, 1), Buffer.from(`layerfs-build-identity:${A}:clean\0`)]), { mode: 0o755 })
    const elsewhere = { path: join(dir, 'debug', 'layerfs-daemon'), source: 'recorded', present: true }
    const cases = [
      [{ ...elsewhere, commit: A, dirty: false, matches: true }, false],
      [{ ...elsewhere, commit: B, dirty: false, matches: false }, true],
      [{ ...elsewhere, present: false, commit: null, dirty: null, matches: null }, true],
      [{ ...elsewhere, commit: A, dirty: true, matches: null }, false],
      [{ path: ours, source: 'recorded', present: true, commit: A, dirty: false, matches: true }, false],
    ]
    for (const [recorded, offered] of cases) {
      await withService({ recorded, daemon: ours }, async ({ s }) => {
        const status = await s.setup()
        assert.equal(status.daemonRecordable, offered, JSON.stringify(recorded))
      })
    }
  } finally { rmSync(dir, { recursive: true, force: true }) }
})
