// The two homes (docs/specs/homes.md): LAYERFS_HOME holds LayerFS's records,
// its runtime (runtime/<version>-<sha>/) and secrets (secret/); DSH_HOME holds
// dsh-layerfs's own state.
//   node --test test/homes.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, symlinkSync, utimesSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

const root = fileURLToPath(new URL('..', import.meta.url))
// Inside the checkout, so the external DSH packages resolve from node_modules.
mkdirSync(join(root, 'lib'), { recursive: true })
const out = mkdtempSync(join(root, 'lib', '.test-homes-'))
await build({
  stdin: {
    contents: "export * from './src/host/layerfs/runtime-home.ts'\nexport { LayerfsService } from './src/host/service.ts'\nexport { Context } from '@deepseek-ai/cordis'\n",
    resolveDir: root,
    loader: 'ts',
  },
  bundle: true,
  platform: 'node',
  format: 'esm',
  external: ['@deepseek-ai/*'],
  outfile: join(out, 'homes.mjs'),
  absWorkingDir: root,
  target: 'node24',
  logLevel: 'warning',
})
const H = await import(pathToFileURL(join(out, 'homes.mjs')).href)

/** A scratch folder with a fake CLI, daemon and bundle. */
function fixture() {
  const dir = mkdtempSync(join(tmpdir(), 'dsh-homes-'))
  const src = join(dir, 'src')
  mkdirSync(join(src, 'bundle', 'linux-aarch64'), { recursive: true })
  writeFileSync(join(src, 'layerfs'), '#!/bin/sh\necho cli-1\n', { mode: 0o755 })
  writeFileSync(join(src, 'layerfs-daemon'), 'daemon-1', { mode: 0o755 })
  writeFileSync(join(src, 'bundle', 'linux-aarch64', 'bundle.json'), JSON.stringify({ hash: 'h1' }))
  writeFileSync(join(src, 'bundle', 'linux-aarch64', 'worker'), 'w')
  return { dir, src, home: join(dir, 'home'), done: () => rmSync(dir, { recursive: true, force: true }) }
}

test('runtime install: copies into <home>/runtime/<version>-<sha>, owner-only home, reused when identical', () => {
  const f = fixture()
  try {
    const given = { cli: join(f.src, 'layerfs'), daemon: join(f.src, 'layerfs-daemon'), bundle: join(f.src, 'bundle', 'linux-aarch64') }
    const a = H.installRuntime(f.home, '2.0.0', given)
    assert.match(a.id, /^2\.0\.0-[0-9a-f]{12}$/)
    const dir = join(f.home, 'runtime', a.id)
    assert.deepEqual([a.cli, a.daemon, a.bundle], [join(dir, 'bin', 'layerfs'), join(dir, 'bin', 'layerfs-daemon'), join(dir, 'bundle', 'linux-aarch64')])
    assert.equal(readFileSync(a.cli, 'utf8'), '#!/bin/sh\necho cli-1\n')
    assert.equal(statSync(a.cli).mode & 0o111, 0o111)
    assert.equal(readFileSync(join(a.bundle, 'worker'), 'utf8'), 'w')
    assert.equal(statSync(f.home).mode & 0o777, 0o700)
    // again, and from the installed copies themselves: the same folder
    assert.deepEqual(H.installRuntime(f.home, '2.0.0', given), a)
    assert.deepEqual(H.installRuntime(f.home, '2.0.0', { cli: a.cli, daemon: null, bundle: null }), a)
    const listed = H.installedRuntimes(f.home)
    assert.deepEqual(listed.map((r) => [r.id, r.cli, r.daemon, r.bundle]), [[a.id, a.cli, a.daemon, a.bundle]])
  } finally { f.done() }
})

test('runtime install: a different daemon or bundle for the same CLI is refused, never replaced', () => {
  const f = fixture()
  try {
    const given = { cli: join(f.src, 'layerfs'), daemon: join(f.src, 'layerfs-daemon'), bundle: join(f.src, 'bundle', 'linux-aarch64') }
    const a = H.installRuntime(f.home, '2.0.0', given)
    writeFileSync(join(f.src, 'layerfs-daemon'), 'daemon-2')
    assert.throws(() => H.installRuntime(f.home, '2.0.0', given), (e) => e.code === 'InvalidInput' && /different daemon/.test(e.message))
    assert.equal(readFileSync(a.daemon, 'utf8'), 'daemon-1')
    writeFileSync(join(f.src, 'layerfs-daemon'), 'daemon-1')
    writeFileSync(join(given.bundle, 'bundle.json'), JSON.stringify({ hash: 'h2' }))
    assert.throws(() => H.installRuntime(f.home, '2.0.0', given), (e) => e.code === 'InvalidInput' && /different Linux bundle/.test(e.message))
    // another CLI build gets its own folder
    writeFileSync(join(f.src, 'layerfs'), '#!/bin/sh\necho cli-2\n')
    const b = H.installRuntime(f.home, '2.0.0', given)
    assert.notEqual(b.id, a.id)
    assert.equal(H.installedRuntimes(f.home).length, 2)
    assert.ok(!H.installedRuntimes(f.home).some((r) => r.id.includes('.tmp-')))
  } finally { f.done() }
})

test('service defaults: the LayerFS home is ~/.layerfs-style and apart from DSH_HOME; secrets in <home>/secret; logs under <root>', () => {
  const f = fixture()
  const saved = { DSH_HOME: process.env.DSH_HOME, DSH_LAYERFS_HOME: process.env.DSH_LAYERFS_HOME, LAYERFS_HOME: process.env.LAYERFS_HOME, DSH_LAYERFS_CLI: process.env.DSH_LAYERFS_CLI, DSH_LAYERFS_SESSION_HOME: process.env.DSH_LAYERFS_SESSION_HOME, DSH_LAYERFS_SECRETS: process.env.DSH_LAYERFS_SECRETS }
  try {
    for (const k of Object.keys(saved)) delete process.env[k]
    process.env.DSH_HOME = join(f.dir, 'dsh')
    process.env.LAYERFS_HOME = f.home
    const s = new H.LayerfsService(new H.Context(), {})
    assert.equal(s.home, f.home)
    assert.equal(s.root, join(f.dir, 'dsh', 'layerfs'))
    assert.equal(s.secrets, join(f.home, 'secret'))
    assert.equal(s.projectSessionHome, join(f.dir, 'dsh', 'layerfs', 'project-sessions'))
    assert.equal(s.cli, null)
    // nothing pinned: the newest runtime installed in the home
    const a = H.installRuntime(f.home, '2.0.0', { cli: join(f.src, 'layerfs'), daemon: join(f.src, 'layerfs-daemon'), bundle: join(f.src, 'bundle', 'linux-aarch64') })
    const t = new H.LayerfsService(new H.Context(), {})
    assert.deepEqual([t.cli, t.daemon, t.bundle], [a.cli, a.daemon, a.bundle])
    // an explicit pin still wins
    const u = new H.LayerfsService(new H.Context(), { cli: join(f.src, 'layerfs') })
    assert.equal(u.cli, join(f.src, 'layerfs'))
    // without LAYERFS_HOME the default is ~/.layerfs, never under DSH_HOME
    delete process.env.LAYERFS_HOME
    const v = new H.LayerfsService(new H.Context(), {})
    assert.match(v.home, /\/\.layerfs$/)
    assert.ok(!v.home.startsWith(process.env.DSH_HOME))
  } finally {
    for (const [k, v] of Object.entries(saved)) { if (v === undefined) delete process.env[k]; else process.env[k] = v }
    f.done()
  }
})

/** `n` runtimes in `home`, r0 the oldest; each with a CLI, a daemon and (unless `bare` names it) a bundle. */
function runtimes(home, n, { bare = [] } = {}) {
  const made = []
  for (let i = 0; i < n; i++) {
    const dir = join(home, 'runtime', `2.0.0-${String(i).padStart(12, '0')}`)
    mkdirSync(join(dir, 'bin'), { recursive: true })
    writeFileSync(join(dir, 'bin', 'layerfs'), `cli-${i}`)
    writeFileSync(join(dir, 'bin', 'layerfs-daemon'), `daemon-${i}`)
    if (!bare.includes(i)) {
      mkdirSync(join(dir, 'bundle', 'linux-aarch64'), { recursive: true })
      writeFileSync(join(dir, 'bundle', 'linux-aarch64', 'bundle.json'), '{}')
    }
    // installedRuntimes orders by the CLI's mtime: one second apart
    utimesSync(join(dir, 'bin', 'layerfs'), 1_700_000_000 + i, 1_700_000_000 + i)
    made.push({ id: `2.0.0-${String(i).padStart(12, '0')}`, dir, cli: join(dir, 'bin', 'layerfs'), daemon: join(dir, 'bin', 'layerfs-daemon'), bundle: join(dir, 'bundle', 'linux-aarch64') })
  }
  return made
}
// `machine list --detail` (2.0): rows keyed `machine`, the native record's `daemon` path in the long form
const stopped = (extra = {}) => ({ items: [{ machine: 'native', platform: 'native', state: 'ready', ...extra }, { machine: 'docker', platform: 'docker', state: 'stopped' }] })
const left = (home) => readdirSync(join(home, 'runtime')).sort()
const whys = (result) => Object.fromEntries(result.kept.map((k) => [k.id.slice(-1), k.why]))

test('runtime cleanup: keeps the pinned runtime, the newest few, one a machine record names and one a process runs from; removes the rest', () => {
  const f = fixture()
  try {
    const r = runtimes(f.home, 8)
    assert.equal(H.KEEP_NEWEST, 3)
    const result = H.cleanRuntimes(f.home, {
      pinned: [r[0].cli, null, undefined],
      machines: stopped({ daemon: r[1].daemon }),
      processes: ['/sbin/launchd', `${r[2].daemon} --config /x/config`],
    })
    assert.deepEqual(whys(result), { 7: 'newest', 6: 'newest', 5: 'newest', 2: 'process', 1: 'machine', 0: 'pinned' })
    assert.deepEqual(result.removed.sort(), [r[3].id, r[4].id])
    assert.deepEqual(result.failed, [])
    assert.deepEqual(left(f.home), [r[0], r[1], r[2], r[5], r[6], r[7]].map((x) => x.id))
    assert.deepEqual(H.installedRuntimes(f.home).map((x) => x.id), [r[7], r[6], r[5], r[2], r[1], r[0]].map((x) => x.id))
    // a second run has nothing left to remove
    assert.deepEqual(H.cleanRuntimes(f.home, { pinned: [r[0].cli], machines: stopped({ daemon: r[1].daemon }), processes: ['x', `${r[2].daemon}`] }).removed, [])
  } finally { f.done() }
})

test('runtime cleanup: a worker machine that is not stopped keeps every runtime with a bundle', () => {
  const f = fixture()
  try {
    const r = runtimes(f.home, 6, { bare: [1] })
    for (const state of ['ready', 'busy', 'broken', 'blocked', 'unknown', undefined]) {
      const machines = { items: [{ machine: 'native', platform: 'native', state: 'ready' }, { machine: 'docker', platform: 'docker', state }] }
      const plan = H.planRuntimeCleanup(H.installedRuntimes(f.home), { pinned: [], machines, processes: ['x'] })
      assert.deepEqual(plan.remove.map((x) => x.id), [r[1].id], `state ${state}: only the runtime without a bundle may go`)
      assert.equal(plan.keep.get(r[0].id), 'worker')
    }
    // a machine with no readable platform counts as a worker that may run
    const odd = H.planRuntimeCleanup(H.installedRuntimes(f.home), { pinned: [], machines: { items: [null, { machine: 'x' }] }, processes: ['x'] })
    assert.deepEqual(odd.remove.map((x) => x.id), [r[1].id])
    const result = H.cleanRuntimes(f.home, { pinned: [], machines: { items: [{ machine: 'vm', platform: 'lima', state: 'ready' }] }, processes: ['x'] })
    assert.deepEqual(result.removed, [r[1].id])
    assert.deepEqual(left(f.home), [r[0], r[2], r[3], r[4], r[5]].map((x) => x.id))
    // all workers stopped: the old ones with a bundle go too
    assert.deepEqual(H.cleanRuntimes(f.home, { pinned: [], machines: stopped(), processes: ['x'] }).removed.sort(), [r[0].id, r[2].id])
  } finally { f.done() }
})

test('runtime cleanup: removes nothing when a fact is missing, and never a symlinked folder', () => {
  const f = fixture()
  try {
    const r = runtimes(f.home, 6)
    const before = left(f.home)
    for (const facts of [
      { pinned: [], machines: null, processes: ['x'] },
      { pinned: [], machines: stopped(), processes: null },
      { pinned: [], machines: { machines: [] }, processes: ['x'] },
      { pinned: [], machines: 'nope', processes: ['x'] },
    ]) {
      const result = H.cleanRuntimes(f.home, facts)
      assert.deepEqual(result.removed, [])
      assert.deepEqual(whys(result), { 5: 'newest', 4: 'newest', 3: 'newest', 2: 'unknown', 1: 'unknown', 0: 'unknown' })
      assert.deepEqual(left(f.home), before)
    }
    // an old runtime that is a symlink to a folder elsewhere: kept, and its target untouched
    const elsewhere = join(f.dir, 'elsewhere')
    mkdirSync(join(elsewhere, 'bin'), { recursive: true })
    writeFileSync(join(elsewhere, 'bin', 'layerfs'), 'cli-x')
    utimesSync(join(elsewhere, 'bin', 'layerfs'), 1_600_000_000, 1_600_000_000)
    symlinkSync(elsewhere, join(f.home, 'runtime', '2.0.0-linked'))
    const result = H.cleanRuntimes(f.home, { pinned: [], machines: stopped(), processes: ['x'] })
    assert.deepEqual(result.removed.sort(), [r[0].id, r[1].id, r[2].id])
    assert.ok(result.kept.some((k) => k.id === '2.0.0-linked' && k.why === 'unknown'))
    assert.ok(existsSync(join(elsewhere, 'bin', 'layerfs')))
    // no home, no runtime folder: nothing to do, no throw
    assert.deepEqual(H.cleanRuntimes(join(f.dir, 'absent'), { pinned: [], machines: stopped(), processes: ['x'] }), { kept: [], removed: [], failed: [] })
  } finally { f.done() }
})

test('runtime cleanup: the service keeps its pin and what machine.list names; an unreadable machine list removes nothing', async () => {
  const f = fixture()
  const saved = { DSH_HOME: process.env.DSH_HOME, DSH_LAYERFS_HOME: process.env.DSH_LAYERFS_HOME, LAYERFS_HOME: process.env.LAYERFS_HOME, DSH_LAYERFS_CLI: process.env.DSH_LAYERFS_CLI, DSH_LAYERFS_DAEMON: process.env.DSH_LAYERFS_DAEMON, DSH_LAYERFS_BUNDLE: process.env.DSH_LAYERFS_BUNDLE, LAYERFS_BUNDLE: process.env.LAYERFS_BUNDLE }
  try {
    for (const k of Object.keys(saved)) delete process.env[k]
    process.env.DSH_HOME = join(f.dir, 'dsh')
    process.env.LAYERFS_HOME = f.home
    const r = runtimes(f.home, 7)
    // pinned through the config: the oldest runtime
    const s = new H.LayerfsService(new H.Context(), { cli: r[0].cli, daemon: r[0].daemon, bundle: r[0].bundle })
    // the fake "CLI" is not a LayerFS CLI and the home has no layerfs.json: machine.list cannot be read
    const none = await s.cleanRuntimes()
    assert.deepEqual(none.removed, [])
    assert.equal(left(f.home).length, 7)
    // with a readable machine list (a stand-in SDK): r1 is named by the native record, r2 and r3 go
    writeFileSync(join(f.home, 'layerfs.json'), '{}')
    s.sdk = { machine: { list: async () => stopped({ daemon: r[1].daemon }) } }
    const result = await s.cleanRuntimes()
    assert.deepEqual(result.removed.sort(), [r[2].id, r[3].id])
    assert.deepEqual(whys(result), { 6: 'newest', 5: 'newest', 4: 'newest', 1: 'machine', 0: 'pinned' })
    assert.deepEqual(left(f.home), [r[0], r[1], r[4], r[5], r[6]].map((x) => x.id))
  } finally {
    for (const [k, v] of Object.entries(saved)) { if (v === undefined) delete process.env[k]; else process.env[k] = v }
    f.done()
  }
})

test.after(() => rmSync(out, { recursive: true, force: true }))
