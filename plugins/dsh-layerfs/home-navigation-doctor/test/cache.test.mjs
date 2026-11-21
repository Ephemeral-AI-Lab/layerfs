// Host read caches, their invalidation and TTL bounds, and "no writes on reads"
// (docs/specs/backend-e2e-tests.md §4.1, findings C1 and H6), in process over a
// fake `lfs` that records every `family.call`. Time is t.mock.timers' Date.
//   node --test test/cache.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { build } from 'esbuild'
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { loadRoutes, testHost } from './routes-helper.mjs'

const root = fileURLToPath(new URL('..', import.meta.url))
const R = await loadRoutes()
// the real project catalog (assertProjectIdentity) and presets manager, bundled like the routes
const extraDir = mkdtempSync(join(tmpdir(), 'dsh-cache-test-'))
await build({
  absWorkingDir: root,
  stdin: { contents: "export { ProjectCatalog } from './src/host/sessions/catalog.ts'\nexport { PresetManager } from './src/host/presets/manager.ts'\nexport { presetStateFor } from './src/host/presets/state-dir.ts'\n", resolveDir: root, loader: 'ts' },
  outfile: join(extraDir, 'extra.mjs'), bundle: true, format: 'esm', platform: 'node', target: 'node24', logLevel: 'warning',
})
const { ProjectCatalog, PresetManager, presetStateFor } = await import(pathToFileURL(join(extraDir, 'extra.mjs')).href)
test.after(() => rmSync(extraDir, { recursive: true, force: true }))

class Refusal extends Error {
  constructor(code, message, detail = null, transport = 'refusal') { super(message); Object.assign(this, { code, detail, transport }) }
}

/** Calls that change LayerFS state: none may run while answering a GET. */
const MUTATING = new Set([
  'branch.fork', 'branch.merge', 'branch.publish', 'branch.remove',
  'file.write', 'file.edit', 'file.rm', 'file.mkdir', 'file.mv', 'file.cp', 'file.apply',
  'store.create', 'store.remove', 'store.connect', 'store.update', 'store.move', 'store.disconnect', 'store.save_auth', 'store.remove_auth', 'store.compact',
  'sandbox.create', 'sandbox.remove',
  'workspace.mount', 'workspace.unmount', 'workspace.commit', 'workspace.exec', 'workspace.cancel',
  'project.init', 'project.delete', 'project.export', 'project.hide_path',
  'machine.start', 'machine.stop', 'machine.create', 'machine.remove', 'machine.update', 'machine.load_image', 'machine.prune',
  'backup.create', 'backup.restore', 'backup.connect', 'backup.disconnect',
  'home.clear_log', 'home.init',
])
/** The read calls (cli.ts READS plus workspace.status and home.log): anything else a GET makes is suspect too. */
const READS = new Set(['list', 'get', 'status', 'log', 'images', 'stats', 'read', 'stat', 'glob', 'grep', 'blame', 'diff', 'history', 'auths', 'doctor'])
/** Monitoring reads, named with their family (cli.ts INSPECT_CALLS). */
const INSPECT = new Set(['machine.log', 'sandbox.log', 'sandbox.files', 'machine.files', 'machine.stats'])
const isRead = (call) => (READS.has(call.split('.')[1]) || INSPECT.has(call)) && !MUTATING.has(call)

const L = `32${'a'.repeat(64)}`
const C1 = `c1${'1'.repeat(64)}`
const C2 = `c2${'2'.repeat(64)}`
// 2.0 `commit list --detail` rows
const commit1 = { commit: C1, message: 'one', parent: null, base: L, created_at: '1970-01-01T00:00:01Z', size: 3, new_canonical_bytes: 1, new_stored_bytes: 1, new_metadata_bytes: 1 }
const commit2 = { commit: C2, message: 'two', parent: C1, created_at: '1970-01-01T00:00:02Z', size: 4, new_canonical_bytes: 2, new_stored_bytes: 2, new_metadata_bytes: 1 }

/**
 * A fake LayerFS 2.0 home (machines native + lima1, Store s1 with projects p
 * and q, Branch feat with two Commits, Sandbox box1 with Workspace box1:w)
 * behind a Proxy that records `family.call` with its args, answering the long
 * forms. `gate` holds sandbox.get until released; `sandboxBusy` makes it
 * refuse Busy.
 */
function fakeLayerfs() {
  const state = {
    calls: [],
    stores: [{ store: 's1', machine: 'native', engine: 'sqlite', projects: ['p', 'q'], state: 'ready' }],
    sandboxBusy: false,
    gate: null,
  }
  const project = (key) => ({ project: key, store: key.split('/')[0], engine: 'sqlite', machine: 'native', state: 'ready', commit: L, created_at: '1970-01-01T00:00:00.100Z' })
  const projects = () => state.stores.flatMap((s) => s.projects.map((p) => project(`${s.store}/${p}`)))
  const branches = { main: { branch: 'main', commit: L }, feat: { branch: 'feat', commit: C2, base: L, forked_from: L } }
  const chain = [commit2, commit1]
  const impl = {
    project: {
      list: async () => ({ items: projects() }),
      get: async (key) => { const p = projects().find((x) => x.project === key); if (!p) throw new Refusal('NotFound', `no project ${key}`); return p },
      stats: async () => ({ file_count: 1, dir_count: 1, symlink_count: 0, size: 3, partial: false }),
      init: async (key) => project(key),
      delete: async () => ({ ok: true }),
    },
    branch: {
      list: async () => ({ items: Object.values(branches) }),
      get: async (_p, name) => { if (!branches[name]) throw new Refusal('NotFound', `no branch ${name}`); return branches[name] },
      fork: async () => ({ commit: L }),
    },
    commit: {
      list: async (_p, _b, opts = {}) => {
        const from = opts.at ? chain.findIndex((c) => c.commit === opts.at) : 0
        const rows = from < 0 ? [] : chain.slice(from, from + (opts.limit ?? 20))
        return { items: rows, ...(from >= 0 && from + rows.length < chain.length ? { next_cursor: 'more' } : {}) }
      },
      get: async (_p, id) => chain.find((c) => c.commit === id) ?? commit2,
      diff: async () => ({ items: [{ path: '/a.txt', change: 'modified', type: 'file' }] }),
    },
    file: {
      list: async () => ({ items: [{ path: '/a.txt', type: 'file' }] }),
      stat: async () => ({ type: 'file', size: 3, mode: 420 }),
      read: async () => ({ text: 'abc', next_line: null, line_truncated: false, binary: false, size: 3, start_line: 1, end_line: 1 }),
      blame: async () => ({ ranges: [{ start: 1, end: 1, commit: C2 }], commits: [{ commit: C2, message: 'two' }], next_line: null, partial: false }),
      history: async () => ({ items: [] }),
    },
    store: {
      list: async () => ({ items: state.stores.map((s) => ({ ...s, projects_count: s.projects.length, size: 105 })) }),
      get: async (name) => ({ ...(state.stores.find((s) => s.store === name) ?? { store: name, state: 'ready' }), stats: { canonical_bytes: 10, stored_bytes: 10, metadata_bytes: 1, history_logical_bytes: 10, size: 105 } }),
      create: async (name) => { state.stores.push({ store: name, machine: 'native', engine: 'sqlite', projects: [], state: 'ready' }); return { ok: true } },
      auths: async () => ({ items: [] }),
    },
    // the 2.0 machine family (long forms); platforms come from home.get --with platforms
    machine: {
      list: async () => ({ items: [{ machine: 'native', platform: 'native', state: 'ready', supports: ['sqlite'], sandboxes: [], stores: [] }, { machine: 'lima1', platform: 'lima', state: 'stopped', supports: ['sqlite'], sandboxes: [], stores: [] }] }),
      get: async (name) => ({ machine: name, platform: name === 'native' ? 'native' : 'lima', state: name === 'native' ? 'ready' : 'stopped', daemon: '/fake/daemon', supports: ['sqlite'], sandboxes: [], stores: [] }),
      images: async (name) => ({ machine: name, mode: 'cached', items: [], disk_free_bytes: null }),
      files: async () => ({ type: 'dir', items: [], next_cursor: null }),
      log: async () => ({ items: [{ text: 'worker up' }], next_cursor: null }),
    },
    home: {
      get: async () => ({ home: '/fake/home', state: 'ready', version: '2.0.0', generation: 2, platforms: { host: 'macos', items: [{ platform: 'lima', ready: true, options: [] }] } }),
    },
    sandbox: {
      files: async () => ({ type: 'dir', items: [], next_cursor: null }),
      list: async () => ({ items: [{ sandbox: 'box1', project: 's1/p', state: 'ready', machine: 'native' }] }),
      get: async (name) => {
        if (state.gate) await state.gate.promise
        if (state.sandboxBusy) throw new Refusal('Busy', `${name} is busy`, { op: { name: 'workspace.mount' } })
        return { sandbox: name, project: 's1/p', state: 'ready', machine: 'native', workspaces: [{ workspace: `${name}:w`, branch: 'feat', path: '/tmp/w' }] }
      },
      remove: async () => ({ ok: true, outcome: 'removed', unmounted: [] }),
      log: async () => ({ items: [{ text: 'daemon up' }], next_cursor: null }),
    },
    workspace: {
      status: async () => ({ dirty: false }),
      mount: async (sandbox) => ({ workspace: `${sandbox}:w2` }),
      unmount: async () => ({ ok: true }),
      commit: async () => ({ commit: C2 }),
    },
    backup: { list: async () => ({ items: [] }) },
  }
  const lfs = new Proxy({ spec: { cli: '/fake/layerfs', home: '/fake/home' } }, {
    get(target, family) {
      if (family in target) return target[family]
      if (typeof family !== 'string' || family === 'then') return undefined
      return new Proxy({}, {
        get(_t, call) {
          if (typeof call !== 'string' || call === 'then') return undefined
          return async (...args) => {
            state.calls.push({ call: `${family}.${call}`, args })
            const fn = impl[family]?.[call]
            if (!fn) throw new Refusal('Unsupported', `fake has no ${family}.${call}`)
            return fn(...args)
          }
        },
      })
    },
  })
  return { lfs, state, impl, count: (call) => state.calls.filter((c) => c.call === call).length }
}

const discoveryOf = (calls = []) => async () => ({ identity: { version: '2.0.0-test', commit: 'a'.repeat(40), dirty: false, features: [] }, calls, commands: [], switches: [] })

/** Route groups over a fresh fake home. */
function setup(t, { discovery = discoveryOf() } = {}) {
  const dir = mkdtempSync(join(tmpdir(), 'dsh-cache-'))
  t.after(() => rmSync(dir, { recursive: true, force: true }))
  const fake = fakeLayerfs()
  const host = testHost({ lfs: fake.lfs, root: join(dir, 'root'), home: join(dir, 'home'), discovery })
  const groups = R.createGroups(host.host, async () => ({ ok: true, version: '2.0.0-test' }))
  t.after(() => groups.projects.close())
  const http = (method, path, body) => R.handle(groups, { method, path, body })
  return { ...fake, dir, host, groups, http }
}

const P = encodeURIComponent('s1/p')
const flush = () => new Promise((resolve) => setImmediate(resolve))

// ---------------------------------------------------------------- hits

// §4.1 Hits: the project list joins the Storage view (store.list …) at most once per 15 s (JOIN_MS).
test('two project lists within 15 s read the Store join once', async (t) => {
  t.mock.timers.enable({ apis: ['Date'], now: 1_000_000 })
  const f = setup(t)
  const a = await f.http('GET', '/api/projects')
  assert.equal(a.status, 200)
  assert.deepEqual(a.body.map((p) => [p.key, p.storeState]), [['s1/p', 'ready'], ['s1/q', 'ready']])
  t.mock.timers.tick(10_000)
  await f.http('GET', '/api/projects')
  assert.equal(f.count('project.list'), 2, 'the project records themselves are read each time')
  assert.equal(f.count('store.list'), 1, 'the joined Storage view is reused within 15 s')
})

// §4.1 Hits: the platforms (home.get --with platforms) are answered from a 30 s cache.
test('two platform reads within 30 s make one home.get --with platforms', async (t) => {
  t.mock.timers.enable({ apis: ['Date'], now: 1_000_000 })
  const f = setup(t)
  assert.equal((await f.http('GET', '/api/platforms')).status, 200)
  t.mock.timers.tick(20_000)
  const again = await f.http('GET', '/api/platforms')
  assert.deepEqual(again.body.platforms.map((p) => p.platform), ['lima'])
  // the machines view reads platforms through the same cache
  await f.http('GET', '/api/machines')
  assert.equal(f.state.calls.filter((c) => c.call === 'home.get' && c.args[0]?.with?.includes('platforms')).length, 1)
})

// §4.1 Hits: parallel reads of one Sandbox share the sandbox.get in flight.
test('two parallel Sandbox reads make one sandbox.get', async (t) => {
  const f = setup(t)
  f.state.gate = Promise.withResolvers()
  const both = Promise.all([f.http('GET', '/api/sandboxes/box1'), f.http('GET', '/api/sandboxes/box1')])
  for (let i = 0; i < 5; i++) await flush()
  f.state.gate.resolve()
  f.state.gate = null
  const [a, b] = await both
  assert.equal(a.status, 200)
  assert.deepEqual(b.body, a.body)
  assert.deepEqual(a.body.workspaces.map((w) => w.ws), ['box1:w'])
  assert.equal(f.count('sandbox.get'), 1)
})

// §4.1 Hits: concurrent reads that need the Storage view (the project list's join and the
// Storage page) share one store.list; a Store write in between makes the next read anew.
test('parallel project list and Storage reads make one store.list; a write reads anew', async (t) => {
  const f = setup(t)
  const [a, b, c] = await Promise.all([f.http('GET', '/api/projects'), f.http('GET', '/api/storage'), f.http('GET', '/api/storage')])
  assert.deepEqual([a.status, b.status, c.status], [200, 200, 200])
  assert.equal(f.count('store.list'), 1, 'one Storage view for all three')
  const created = await f.http('POST', '/api/stores', { name: 's2' })
  assert.equal(created.status, 200)
  assert.equal(f.count('store.list'), 2, 'the write reads the view anew')
  assert.ok(created.body.stores.some((s) => s.name === 's2'))
})

// §4.1 Hits: concurrent machines views (the sidebar, the machines store, a dialog) share one read,
// and one answer serves every tab for 5 s (layerfs-project#182 D2); a GET after a write reads anew.
test('parallel machines views make one machine.list; a write reads anew', async (t) => {
  const f = setup(t)
  const [a, b] = await Promise.all([f.http('GET', '/api/machines'), f.http('GET', '/api/machines')])
  assert.equal(a.status, 200)
  assert.deepEqual(b.body, a.body)
  assert.equal(f.count('machine.list'), 1)
  const first = f.http('GET', '/api/machines')
  await f.http('POST', '/api/machines/lima1/start')
  const second = f.http('GET', '/api/machines')
  await Promise.all([first, second])
  assert.equal(f.count('machine.list'), 2, 'the GET before the start was the kept answer; the GET after it read anew')
})

// §4.1 Hits (finding 1): History reads a Branch at its head with its fork point from the Branch
// list: no branch.get, and a page read at a named Commit is kept (it never changes).
test('history pages read at a head skip branch.get and are kept', async (t) => {
  const f = setup(t)
  const path = `/api/projects/${P}/history?branch=feat&at=${C2}&fork=${L}`
  const a = await f.http('GET', path)
  assert.equal(a.status, 200)
  assert.deepEqual(a.body.items.map((c) => c.id), [C2, C1])
  assert.equal(f.count('branch.get'), 0, 'the fork point came from the caller')
  const b = await f.http('GET', path)
  assert.deepEqual(b.body, a.body)
  assert.equal(f.count('commit.list'), 1, 'the page at a named head is kept')
  // without a head the Branch may have moved: read again
  await f.http('GET', `/api/projects/${P}/history?branch=feat&fork=${L}`)
  await f.http('GET', `/api/projects/${P}/history?branch=feat&fork=${L}`)
  assert.equal(f.count('commit.list'), 3)
})

// History's whole count: wide pages back to the fork, kept per head; a moved head reads only back
// to the last counted head; main has Layers (no count); concurrent asks share one count.
test('history counts a Branch once and then only what is new', async (t) => {
  const f = setup(t)
  const ids = Array.from({ length: 130 }, (_, i) => `12${String(i + 1).padStart(4, '0')}`)
  const chain = new Map(ids.map((id, i) => [id, { commit: id, parent: i ? ids[i - 1] : L, base: L, message: null, created_at: null }]))
  const pages = []
  // `commit list --at C --limit N`: from C back, newest first; a cursor when there is more
  f.impl.commit.list = async (_p, _b, opts) => {
    pages.push(opts)
    const rows = []
    for (let c = chain.get(opts.at); c && rows.length < opts.limit; c = chain.get(c.parent)) rows.push(c)
    return { items: rows, ...(chain.get(rows.at(-1)?.parent) ? { next_cursor: 'more' } : {}) }
  }
  const count = (head) => f.http('GET', `/api/projects/${P}/history?branch=feat&at=${head}&fork=${L}&count=1`)
  const [a, b] = await Promise.all([count(ids[119]), count(ids[119])])
  assert.equal(a.status, 200)
  assert.deepEqual(a.body, { head: ids[119], count: 120 })
  assert.deepEqual(b.body, a.body)
  const read = () => pages
  assert.equal(read().length, 2, 'one count of 120 in pages of 100 (one attempt each, no shrinking), shared by both asks')
  assert.ok(read().every((o) => o.limit === 100))
  assert.deepEqual((await count(ids[119])).body.count, 120)
  assert.equal(read().length, 2, 'kept at that head')
  assert.deepEqual((await count(ids[129])).body, { head: ids[129], count: 130 })
  assert.equal(read().length, 3, 'ten new Commits: one page, stopping at the counted head')
  assert.deepEqual((await f.http('GET', `/api/projects/${P}/history?branch=main&at=${L}&count=1`)).body.count, null)
  assert.deepEqual((await f.http('GET', `/api/projects/${P}/history?branch=feat&at=${L}&fork=${L}&count=1`)).body.count, 0)
  assert.equal(read().length, 3)
})

// Load all reads wide pages, kept apart from the narrow first pages.
test('history wide pages ask for a wide page and are kept by width', async (t) => {
  const f = setup(t)
  const seen = []
  const commits = f.impl.commit.list
  f.impl.commit.list = async (...args) => { seen.push(args[2].limit); return commits(...args) }
  await f.http('GET', `/api/projects/${P}/history?branch=feat&at=${C2}&fork=${L}`)
  await f.http('GET', `/api/projects/${P}/history?branch=feat&at=${C2}&fork=${L}&wide=1`)
  await f.http('GET', `/api/projects/${P}/history?branch=feat&at=${C2}&fork=${L}&wide=1`)
  assert.deepEqual(seen, [20, 100])
})

// §4.1 Hits: a Commit's changed paths are kept (a Commit never changes): repeated branch-commits make no new commit.diff.
test('repeated branch-commits reuse each Commit\'s changed paths', async (t) => {
  const f = setup(t)
  const a = await f.http('GET', `/api/projects/${P}/branch-commits?branch=feat&branch=main`)
  assert.equal(a.status, 200)
  assert.deepEqual(a.body.main, [])
  assert.deepEqual(a.body.feat.map((c) => [c.id, c.changes]), [[C2, [{ path: '/a.txt', kind: 'modified' }]], [C1, [{ path: '/a.txt', kind: 'modified' }]]])
  assert.equal(f.count('commit.diff'), 2)
  const b = await f.http('GET', `/api/projects/${P}/branch-commits?branch=feat`)
  assert.deepEqual(b.body.feat, a.body.feat)
  await f.http('GET', `/api/projects/${P}/commits?branch=feat`)
  assert.equal(f.count('commit.diff'), 2, 'no commit.diff for a Commit already compared')
})

// §4.1 Hits: discovery runs once per CLI path (cli.ts `discover`), not per call.
test('discovery runs once per CLI path', async (t) => {
  const dir = mkdtempSync(join(tmpdir(), 'dsh-cache-disc-'))
  t.after(() => rmSync(dir, { recursive: true, force: true }))
  const { cli, log } = fakeCli(dir)
  const [a, b] = await Promise.all([R.cli.discover({ cli }), R.cli.discover({ cli })])
  await R.cli.discover({ cli })
  assert.equal(a, b)
  assert.deepEqual(readLog(log).map((argv) => argv.join(' ')), ['--version --json', 'help --json'])
})

// ---------------------------------------------------------------- invalidation

// §4.1 Invalidation: a Store write refreshes the joined view, so the next project list shows it without waiting 15 s.
test('creating a Store refreshes the project list\'s Store join', async (t) => {
  t.mock.timers.enable({ apis: ['Date'], now: 1_000_000 })
  const f = setup(t)
  await f.http('GET', '/api/projects')
  // store.list's item carries the state the view shows
  f.state.stores[0].state = 'busy'
  t.mock.timers.tick(1_000)
  assert.equal((await f.http('POST', '/api/stores', { name: 'new1' })).status, 200)
  const after = await f.http('GET', '/api/projects')
  assert.deepEqual(after.body.map((p) => p.storeState), ['busy', 'busy'], 'the write\'s fresh view is joined, not the cached one')
  assert.equal(f.count('store.list'), 2, 'one read before, one by the write; the list after it reuses that')
  const storage = await f.http('GET', '/api/storage')
  assert.ok(storage.body.stores.some((s) => s.name === 'new1'))
})

// H6: every Sandbox write (mount, unmount, commit, close) clears the cached Busy refusal, so the next read asks again.
for (const [label, write] of [
  ['mount', (http) => http('POST', '/api/sandboxes/box1/workspaces', { branch: 'feat' })],
  ['unmount', (http) => http('DELETE', '/api/workspaces/box1:w')],
  ['commit', (http) => http('POST', '/api/workspaces/box1:w/commit', { message: 'save' })],
  ['close', (http) => http('DELETE', '/api/sandboxes/box1')],
]) {
  test(`a Sandbox ${label} clears its cached Busy`, async (t) => {
    t.mock.timers.enable({ apis: ['Date'], now: 1_000_000 })
    const f = setup(t)
    f.state.sandboxBusy = true
    assert.equal((await f.http('GET', '/api/sandboxes/box1')).status, 409)
    t.mock.timers.tick(1_000)
    const cached = await f.http('GET', '/api/sandboxes/box1')
    assert.equal(cached.status, 409)
    assert.equal(cached.body.code, 'Busy')
    assert.equal(f.count('sandbox.get'), 1, 'a Busy refusal answers again without a call')
    f.state.sandboxBusy = false
    const w = await write(f.http)
    assert.equal(w.status, 200, JSON.stringify(w.body))
    const fresh = await f.http('GET', '/api/sandboxes/box1')
    assert.equal(fresh.status, 200)
    assert.equal(f.count('sandbox.get'), 2, `the ${label} made the next read ask LayerFS again`)
  })
}

// H6: a write while a read is in flight drops that read, so a read after the write is not answered by the stale one.
test('a mount during a Sandbox read makes the next read a new sandbox.get', async (t) => {
  const f = setup(t)
  f.state.gate = Promise.withResolvers()
  const first = f.http('GET', '/api/sandboxes/box1')
  for (let i = 0; i < 5; i++) await flush()
  assert.equal(f.count('sandbox.get'), 1)
  assert.equal((await f.http('POST', '/api/sandboxes/box1/workspaces', { branch: 'feat' })).status, 200)
  const second = f.http('GET', '/api/sandboxes/box1')
  for (let i = 0; i < 5; i++) await flush()
  f.state.gate.resolve()
  f.state.gate = null
  await Promise.all([first, second])
  assert.equal(f.count('sandbox.get'), 2)
})

// ---------------------------------------------------------------- TTL bounds

// §4.1 TTL bounds: Store join (15 s), platforms (30 s) and a cached Busy (BUSY_MS 30 s): cached at TTL − 1 ms, asked at TTL.
test('TTL bounds: cached at TTL − 1 ms, read again at TTL', async (t) => {
  t.mock.timers.enable({ apis: ['Date'], now: 1_000_000 })
  const f = setup(t)
  const bound = async (read, call, ttl) => {
    const start = f.count(call)
    await read()
    assert.equal(f.count(call), start + 1, `${call}: first read`)
    t.mock.timers.tick(ttl - 1)
    await read()
    assert.equal(f.count(call), start + 1, `${call}: cached at ${ttl - 1} ms`)
    t.mock.timers.tick(1)
    await read()
    assert.equal(f.count(call), start + 2, `${call}: read again at ${ttl} ms`)
  }
  await bound(() => f.http('GET', '/api/projects'), 'store.list', 15_000)
  await bound(() => f.http('GET', '/api/platforms'), 'home.get', 30_000)
  f.state.sandboxBusy = true
  await bound(() => f.http('GET', '/api/sandboxes/box1'), 'sandbox.get', 30_000)
})

// ---------------------------------------------------------------- no writes on reads

/** A recording fake CLI for the paths that spawn it directly (doctor, presets): argv lines in `log`. */
function fakeCli(dir) {
  const cli = join(dir, 'fake-layerfs.cjs'), log = join(dir, 'fake-layerfs.log')
  const required = ['project.init', 'project.list', 'project.get', 'project.export', 'branch.fork', 'branch.list', 'commit.list', 'commit.diff',
    'file.read', 'file.list', 'file.apply', 'sandbox.create', 'sandbox.list', 'sandbox.get', 'sandbox.remove', 'workspace.mount', 'workspace.commit', 'workspace.status',
    'workspace.unmount', 'workspace.exec', 'workspace.cancel', 'machine.list', 'machine.get', 'store.list', 'store.create', 'store.get', 'home.get', 'home.doctor']
  const entry = (k) => ({ family: k.split('.')[0], call: k.split('.')[1], args: [], options: [], ...(k === 'home.get' ? { form: { with: { platforms: {}, tree: {} } } } : {}) })
  const help = { calls: required.map(entry), commands: [], switches: [] }
  writeFileSync(cli, `#!/usr/bin/env node
const { appendFileSync, existsSync, writeFileSync } = require('node:fs')
const argv = process.argv.slice(2)
appendFileSync(${JSON.stringify(log)}, JSON.stringify(argv) + '\\n')
const out = (v) => process.stdout.write(JSON.stringify(v) + '\\n')
if (argv[0] === '--version') out({ version: '2.0.0-test', commit: '${'b'.repeat(40)}', dirty: false, features: ['machine'] })
else if (argv[0] === 'help') out(${JSON.stringify(help)})
else if (argv[1] === 'doctor') out({ ok: true, findings: [], not_checked: [], status: 'ok', counts: {}, observed_at: '2026-11-19T00:00:00Z', tree: { home: '/fake', status: 'ok', coverage: 'shallow', checks: [], blocked_by: null, reason: null } })
else if (argv[1] === 'list') out({ items: [], next_page_pos: null })
else if (argv[0] === 'home' && argv[1] === 'get') out({ home: '/fake', state: 'ready', version: '2.0.0', platforms: { host: 'macos', items: [] }, tree: { machines: [], stores: [], buckets: [] } })
else if (argv[1] === 'get') out({ machine: argv[argv.indexOf('--') + 1] ?? argv[2], platform: 'native', state: 'ready' })
else out({})
`, { mode: 0o700 })
  return { cli, log }
}
const readLog = (log) => existsSync(log) ? readFileSync(log, 'utf8').trim().split('\n').filter(Boolean).map((l) => JSON.parse(l)) : []

/**
 * Every GET route of router.ts's `route()`, as `group[/:id|/literal][/sub]`,
 * read from its source so a new GET route fails the coverage check below.
 */
function routerGets() {
  const src = readFileSync(join(root, 'src/host/routes/router.ts'), 'utf8')
  const body = src.slice(src.indexOf('export async function route('), src.indexOf('export async function handle('))
  const sigs = new Set()
  let block = null, noId = false, projectGets = false
  for (const line of body.split('\n')) {
    const indent = /^ */.exec(line)[0].length
    if (indent === 2 && /^\s*if \(/.test(line)) {
      block = /^\s*if \(group === '([\w-]+)'(?: && !sub)?\) \{\s*$/.exec(line)?.[1] ?? null
      noId = false
      projectGets = false
    }
    if (/^\s*if \(!id\) \{/.test(line)) { noId = true; continue }
    if (noId && indent === 4 && /^\s*\}/.test(line)) { noId = false; continue }
    if (line.includes("method !== 'GET'")) { projectGets = true; continue }
    if (line.includes("'demo-presets'")) continue
    const projectSub = projectGets && /^\s*if \(sub === '([\w-]+)'\)/.exec(line)
    if (!line.includes("method === 'GET'") && !projectSub) continue
    const group = /group === '([\w-]+)'/.exec(line)?.[1] ?? block
    let id = /\bid === '([\w-]+)'/.exec(line)?.[1]
    if (id === undefined) id = /!id\b/.test(line) || noId ? '' : /\bid &&/.test(line) || group === 'projects' ? ':id' : ''
    const sub = /sub === '([\w-]+)'/.exec(line)?.[1] ?? ''
    sigs.add([group, id, sub].filter(Boolean).join('/'))
  }
  return sigs
}

/** One or more plausible requests per GET route (keep in sync: the test fails when router.ts gains a GET route). */
const GETS = [
  ['storage', '/api/storage'],
  ['stores/:id/stats', '/api/stores/s1/stats'],
  ['stores/:id/observations', '/api/stores/s1/observations'],
  ['stores/:id/projects', '/api/stores/s1/projects'],
  ['stores/:id/check', '/api/stores/s1/check?machine=native'],
  ['store-auths', '/api/store-auths'],
  ['buckets/:id/files', '/api/buckets/b1/files'],
  ['storage-jobs/:id', '/api/storage-jobs/job-x'],
  ['platforms', '/api/platforms'],
  ['machines', '/api/machines'],
  ['machines/:id/inspect', '/api/machines/lima1/inspect'],
  ['machines/:id/sandbox-images', '/api/machines/lima1/sandbox-images'],
  ['machines/:id/log', '/api/machines/lima1/log'],
  ['machines/:id/log', '/api/machines/lima1/log?before=c1&limit=50'],
  ['machines/:id/files', '/api/machines/lima1/files'],
  ['machines/:id/files', '/api/machines/lima1/files?path=%2Fetc&depth=1&limit=200'],
  ['machines/:id/files', '/api/machines/lima1/files?path=%2Fetc%2Fhosts&offset=0&length=4096'],
  ['machines/:id/stats', '/api/machines/lima1/stats'],
  ['machines/:id/space', '/api/machines/lima1/space'],
  ['machines/:id/space', '/api/machines/lima1/space?fresh=1'],
  ['sandbox-creates', '/api/sandbox-creates'],
  ['image-loads', '/api/image-loads'],
  ['status', '/api/status'],
  ['projects', '/api/projects'],
  ['projects/:id', `/api/projects/${P}`],
  ['projects/:id/history', `/api/projects/${P}/history`],
  ['projects/:id/history', `/api/projects/${P}/history?branch=feat`],
  ['projects/:id/history', `/api/projects/${P}/history?branch=feat&commit=${C2}`],
  ['projects/:id/storage', `/api/projects/${P}/storage`],
  ['projects/:id/storage', `/api/projects/${P}/storage?refresh=1`],
  ['projects/:id/size', `/api/projects/${P}/size`],
  ['projects/:id/size', `/api/projects/${P}/size?alone=1`],
  ['projects/:id/branches', `/api/projects/${P}/branches`],
  ['projects/:id/branches', `/api/projects/${P}/branches?branch=feat`],
  ['projects/:id/file-query', `/api/projects/${P}/file-query?branch=feat&path=/a.txt&kind=blame`],
  ['projects/:id/file-query', `/api/projects/${P}/file-query?branch=feat&path=/a.txt&kind=revisions`],
  ['projects/:id/file-query', `/api/projects/${P}/file-query?branch=feat&path=/a.txt&kind=compare`],
  ['projects/:id/file-query', `/api/projects/${P}/file-query?branch=feat&path=/a.txt&kind=attribution`],
  ['projects/:id/file-query', `/api/projects/${P}/file-query?branch=feat&path=/&kind=branch-diff&rightBranch=main`],
  ['projects/:id/stats', `/api/projects/${P}/stats`],
  ['projects/:id/commits', `/api/projects/${P}/commits?branch=feat`],
  ['projects/:id/branch-commits', `/api/projects/${P}/branch-commits?branch=feat&branch=main`],
  ['projects/:id/sandboxes', `/api/projects/${P}/sandboxes`],
  ['projects/:id/fs', `/api/projects/${P}/fs?branch=feat&path=/`],
  ['projects/:id/file', `/api/projects/${P}/file?branch=feat&path=/a.txt`],
  ['metrics', '/api/metrics?machines=lima1&since=0'],
  ['sandboxes/:id/log', '/api/sandboxes/box1/log'],
  ['sandboxes/:id/log', '/api/sandboxes/box1/log?before=b1'],
  ['sandboxes/:id/files', '/api/sandboxes/box1/files'],
  ['sandboxes/:id/files', '/api/sandboxes/box1/files?path=%2Ftmp&cursor=%2Ftmp%2Fa'],
  ['sandboxes/:id', '/api/sandboxes/box1'],
  ['sandboxes/:id/commands', '/api/sandboxes/box1/commands'],
  ['workspaces/:id/commands', '/api/workspaces/box1:w/commands'],
  ['workspaces/:id/status', '/api/workspaces/box1:w/status'],
  ['project-inits', '/api/project-inits'],
  ['project-inits/:id', '/api/project-inits/init-x'],
]
/** The presets manager's GET routes (manager.ts `route`), dispatched by router.ts under /api/demo-presets. */
const PRESET_GETS = ['', '/requests/r1', '/jobs/j1', '/studio', '/studio/environment-plan', '/studio/build-plan', '/studio/build-plan-no-sandboxes', '/studio/script', '/studio/script-no-sandboxes']
/** Routes whose 2xx needs more than the fake has: they may answer 4xx/5xx, but still must not write. */
const MAY_FAIL = new Set(['/api/storage-jobs/job-x', '/api/project-inits/init-x', '/api/stores/s1/check?machine=native'])

// Guards the GETS table: it names every GET route router.ts has.
test('the no-writes table covers every GET route in router.ts', () => {
  const parsed = routerGets()
  assert.ok(parsed.size > 30, `parsed only ${parsed.size} GET routes from router.ts: the parser needs updating`)
  assert.deepEqual([...new Set(GETS.map(([sig]) => sig))].sort(), [...parsed].sort())
})

// C1 (finding: GETs with an `unclaimed:` projectId forked dsh.project.<uuid>): no GET route makes a mutating LayerFS call.
test('no GET route makes a mutating LayerFS call, with or without a projectId', { timeout: 30_000 }, async (t) => {
  const f = setup(t, { discovery: discoveryOf([{ family: 'machine', call: 'images', args: [], options: [] }, { family: 'machine', call: 'log', args: [], options: [] }, { family: 'sandbox', call: 'log', args: [], options: [] }, { family: 'machine', call: 'files', args: [], options: [] }, { family: 'sandbox', call: 'files', args: [], options: [] }, { family: 'machine', call: 'stats', args: [], options: [], form: { with: { space: {} } } }, { family: 'home', call: 'get', args: [], options: [], form: { with: { platforms: {}, tree: {} } } }]) })
  // the real identity check, wired as routes/index.ts wires it: reads never claim
  const catalog = new ProjectCatalog(() => f.lfs, join(f.dir, 'root'))
  f.host.host.assertProjectIdentity = (project, projectId) => catalog.assert(project, projectId, { claim: false })
  // doctor and presets spawn the CLI themselves: a recording fake CLI over a home that has its layerfs.json
  const home = join(f.dir, 'home')
  mkdirSync(home, { recursive: true })
  writeFileSync(join(home, 'layerfs.json'), JSON.stringify({ id: 'home-test' }))
  const { cli, log } = fakeCli(f.dir)
  f.lfs.spec.cli = cli
  f.lfs.spec.home = home
  const presetState = presetStateFor(join(f.dir, 'root'), home)
  f.groups.presets = new PresetManager({
    spec: () => ({ cli, home, state: presetState, daemon: null }),
    busy: () => [], boundSessions: () => [], dshProjects: () => [], forgetProject: async () => {},
    homeStores: async (also) => also.map((name) => ({ name })), projectConversations: () => [], deleteProjectConversations: async () => [],
    emit: () => {},
  }, presetState, home)

  const requests = []
  for (const [sig, path] of GETS) {
    requests.push(path)
    if (sig.startsWith('projects/:id')) {
      const join = path.includes('?') ? '&' : '?'
      requests.push(`${path}${join}projectId=${encodeURIComponent('unclaimed:s1/p')}`)
      // a claimed id the project does not carry: refused ProjectIdentityChanged, still no write
      requests.push(`${path}${join}projectId=00000000-0000-4000-8000-00000000000a`)
    }
  }
  for (const p of PRESET_GETS) requests.push(`/api/demo-presets${p}`)

  const offenders = [], statuses = {}
  for (const path of requests) {
    const before = f.state.calls.length
    const answer = await f.http('GET', path)
    statuses[path] = answer.status
    for (const c of f.state.calls.slice(before)) if (!isRead(c.call)) offenders.push(`GET ${path} -> ${c.call}`)
  }
  // background work a read started (the project storage refresh) finishes too
  await f.groups.projects.close()
  for (const c of f.state.calls) if (!isRead(c.call)) offenders.push(`(any GET) -> ${c.call}`)
  for (const argv of readLog(log)) {
    const call = `${argv[0]}.${String(argv[1] ?? '').replaceAll('-', '_')}`
    if (MUTATING.has(call)) offenders.push(`(spawned CLI) -> layerfs ${argv.join(' ')}`)
  }
  assert.deepEqual([...new Set(offenders)], [])

  // the reads were real: the fake answered them (a failing route proves nothing about its writes)
  const failing = Object.entries(statuses).filter(([path, status]) => status >= 400 && !MAY_FAIL.has(path) && !path.includes('00000000-0000-4000') && !path.startsWith('/api/demo-presets/'))
  assert.deepEqual(failing, [], 'every plain GET answers 2xx over the fake home')
  for (const [path, status] of Object.entries(statuses)) if (path.includes('00000000-0000-4000')) assert.equal(status, 409, path)
  assert.equal(statuses['/api/demo-presets'], 200)
  assert.equal(f.count('branch.fork'), 0, 'an unclaimed projectId never claims (C1)')
  // the log reads made their calls, and nothing else: no sandbox.get, no machine probe
  assert.equal(f.count('machine.log'), 2)
  assert.equal(f.count('sandbox.log'), 2)
  // Stage 0: the ready images come from the archive on this host, never from a machine call
  assert.equal(statuses['/api/machines/lima1/sandbox-images'], 200)
  // GETs that write local stat journals only: Store observations and the project storage ledger, in the stat folder
  const stats = join(home, 'stat')
  // 2.0's `store get --with stats` answers no Store identity: no observation is kept (the answer says why)
  const observations = join(stats, 'storage-observations')
  assert.equal(statuses['/api/stores/s1/stats'], 200)
  assert.deepEqual(existsSync(observations) ? readdirSync(observations) : [], [], '/stores/:id/stats keeps no observation without a Store identity')
  assert.ok(readdirSync(join(stats, 'project-storage')).length > 0, '/projects/:id/storage?refresh=1 keeps its ledger locally')
})

// C1 wiring: the route identity guard passes claim: false (the catalog's own no-claim read is covered by
// test/project-catalog.test.mjs "reads with an unclaimed id never claim; they accept the address as it is").
test('routes/index.ts checks project identities without claiming', () => {
  const src = readFileSync(join(root, 'src/host/routes/index.ts'), 'utf8')
  assert.match(src, /assertProjectIdentity: async \(project, projectId\) => \{[^}]*catalog\.assert\(project, projectId, \{ claim: false \}\)/s)
})
