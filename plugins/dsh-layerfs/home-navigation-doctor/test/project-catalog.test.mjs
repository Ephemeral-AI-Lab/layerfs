import { test } from 'node:test'
import assert from 'node:assert/strict'
import { build } from 'esbuild'
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
const root = fileURLToPath(new URL('..', import.meta.url))
mkdirSync(join(root, 'lib'), { recursive: true })
const out = mkdtempSync(join(root, 'lib', '.test-catalog-'))
await build({ entryPoints: [join(root, 'src/host/sessions/catalog.ts')], outfile: join(out, 'catalog.mjs'), bundle: true, format: 'esm', platform: 'node', target: 'node24' })
const { ProjectCatalog, uuidOfBranches, identityBranch } = await import(pathToFileURL(join(out, 'catalog.mjs')))
test.after(() => rmSync(out, { recursive: true, force: true }))

const A = '00000000-0000-4000-8000-00000000000a'
const B = '00000000-0000-4000-8000-00000000000b'
const fail = (code) => Object.assign(new Error(code), { code })
// a 2.0 `project get --detail` / list row
const info = (key, created = 100, machine = 'native') => ({ project: key, store: key.split('/')[0], engine: 'sqlite', machine, created_at: new Date(created).toISOString() })

/** A fake LayerFS 2.0: projects by address, each with its branch names; refusals by 2.0 code. */
function fixture() {
  const dir = mkdtempSync(join(root, '.dev-test-catalog-'))
  const state = { projects: new Map([['store/app', { info: info('store/app'), branches: ['main'] }]]), offline: null, forks: 0, beforeFork: null }
  const project = (key) => {
    if (state.offline) throw fail('Unavailable')
    const p = state.projects.get(key)
    if (!p) throw fail('NotFound')
    return p
  }
  const lfs = {
    project: {
      list: async () => { if (state.offline === 'global') throw fail('Unavailable'); return { items: state.offline ? [] : [...state.projects.values()].map((p) => p.info) } },
      get: async (key) => project(key).info,
    },
    branch: {
      list: async (key, o = {}) => ({ items: project(key).branches.filter((name) => !o.prefix || name.startsWith(o.prefix)).map((branch) => ({ branch })) }),
      fork: async (key, name, from) => {
        const p = project(key)
        state.forks++
        state.beforeFork?.(p)
        if (!p.branches.includes(from)) throw fail('NotFound')
        if (p.branches.includes(name)) throw fail('Exists')
        p.branches.push(name)
        return { commit: `32${'a'.repeat(64)}` }
      },
    },
    store: { get: async () => (state.offline ? { state: 'blocked', blocked_by: { machine: 'native', reason: 'stopped' } } : { state: 'ready' }) },
  }
  return { state, dir, catalog: new ProjectCatalog(() => lfs, dir), close: () => rmSync(dir, { recursive: true, force: true }) }
}

test('the identity is the smallest UUID among dsh.project.* branches', () => {
  assert.equal(uuidOfBranches(['main', identityBranch(B), identityBranch(A), 'dsh.project.not-a-uuid']), A)
  assert.equal(uuidOfBranches(['main', 'dsh.session.x']), null)
})

test('listing never claims; attaching claims once and the UUID survives machine changes', async () => {
  const f = fixture()
  try {
    const listed = (await f.catalog.list())[0]
    assert.equal(listed.projectId, 'unclaimed:store/app')
    assert.equal(f.state.forks, 0, 'a listing writes nothing to the project')
    const claimed = await f.catalog.resolve('store/app')
    assert.match(claimed.projectId, /^[0-9a-f-]{36}$/)
    assert.deepEqual(f.state.projects.get('store/app').branches, ['main', identityBranch(claimed.projectId)])
    f.state.projects.get('store/app').info.machine = 'vm'
    const again = await f.catalog.resolve('store/app')
    assert.equal(again.projectId, claimed.projectId)
    assert.equal(f.state.forks, 1)
    const row = (await f.catalog.list())[0]
    assert.equal(row.projectId, claimed.projectId)
    assert.equal(row.machine, 'vm')
  } finally { f.close() }
})

test('two installations claiming at once agree on the smallest UUID', async () => {
  const f = fixture()
  try {
    f.state.beforeFork = (p) => { f.state.beforeFork = null; p.branches.push(identityBranch(A)) } // the other side wins first
    const claimed = await f.catalog.resolve('store/app')
    assert.equal(claimed.projectId, A)
  } finally { f.close() }
})

test('without claim an unclaimed project is refused, never minted', async () => {
  const f = fixture()
  try {
    await assert.rejects(f.catalog.resolve('store/app', { claim: false }), { code: 'ProjectIdentityChanged' })
    assert.equal(f.state.forks, 0)
  } finally { f.close() }
})

test('an unclaimed id from a listing claims on attach; a claimed id must match', async () => {
  const f = fixture()
  try {
    const row = await f.catalog.assert('store/app', 'unclaimed:store/app', { claim: true })
    assert.match(row.projectId, /^[0-9a-f-]{36}$/)
    assert.equal((await f.catalog.assert('store/app', row.projectId)).projectId, row.projectId)
    await assert.rejects(f.catalog.assert('store/app', B), { code: 'ProjectIdentityChanged' })
  } finally { f.close() }
})

test('reads with an unclaimed id never claim; they accept the address as it is', async () => {
  const f = fixture()
  try {
    const read = await f.catalog.assert('store/app', 'unclaimed:store/app')
    assert.equal(read.projectId, 'unclaimed:store/app')
    assert.equal(read.key, 'store/app')
    assert.equal(f.state.forks, 0, 'a read forks no identity branch')
    assert.deepEqual(f.state.projects.get('store/app').branches, ['main'])
    assert.equal(f.catalog.retained().length, 0, 'an unclaimed read is not remembered')
    await assert.rejects(f.catalog.assert('store/app', 'unclaimed:store/other'), { code: 'ProjectIdentityChanged' })
    await assert.rejects(f.catalog.assert('store/gone', 'unclaimed:store/gone'), { code: 'NotFound' })
    // Claimed since the listing (a conversation was placed): reads answer the new UUID.
    f.state.projects.get('store/app').branches.push(identityBranch(A))
    assert.equal((await f.catalog.assert('store/app', 'unclaimed:store/app')).projectId, A)
    assert.equal(f.state.forks, 0)
  } finally { f.close() }
})

test('unavailable and deleted projects remain in the durable catalog across restarts', async () => {
  const f = fixture()
  try {
    const original = await f.catalog.resolve('store/app')
    f.state.offline = 'global'
    assert.equal((await f.catalog.list())[0].catalogState, 'unavailable')
    assert.equal(new ProjectCatalog(() => ({}), f.dir).retained()[0].projectId, original.projectId)
    f.state.offline = null; f.state.projects.clear()
    const removed = (await f.catalog.list())[0]
    assert.equal(removed.catalogState, 'removed')
    assert.equal(removed.projectId, original.projectId)
  } finally { f.close() }
})

test('a removed project is asked about once per GONE_MS, not on every listing', async (t) => {
  const f = fixture()
  try {
    await f.catalog.resolve('store/app')
    f.state.projects.clear()
    let gets = 0
    const lfs = f.catalog['lfs']()
    const get = lfs.project.get
    lfs.project.get = async (key) => { gets++; return get(key) }
    t.mock.timers.enable({ apis: ['Date'], now: Date.now() })
    for (let i = 0; i < 3; i++) assert.equal((await f.catalog.list())[0].catalogState, 'removed')
    assert.equal(gets, 1, 'one project.get for three listings')
    t.mock.timers.tick(10 * 60_000)
    assert.equal((await f.catalog.list())[0].catalogState, 'removed')
    assert.equal(gets, 2, 'asked again after GONE_MS')
  } finally { f.close() }
})

test('a failed observation never proves removal and a returning Store reconnects the same identity', async () => {
  const f = fixture()
  try {
    const original = await f.catalog.resolve('store/app')
    f.state.offline = 'machine'
    assert.equal((await f.catalog.list())[0].catalogState, 'unavailable')
    f.state.offline = null
    const ready = (await f.catalog.list())[0]
    assert.equal(ready.projectId, original.projectId)
    assert.equal(ready.catalogState, 'available')
  } finally { f.close() }
})

test('a project recreated under the same name is a different project', async () => {
  const f = fixture()
  try {
    const original = await f.catalog.resolve('store/app')
    f.state.projects.set('store/app', { info: info('store/app', 200), branches: ['main'] })
    const rows = await f.catalog.list()
    assert.equal(rows.length, 2)
    assert.equal(rows.find((row) => row.projectId === original.projectId).catalogState, 'identity-mismatch')
    assert.equal(rows.find((row) => row.projectId === 'unclaimed:store/app').catalogState, 'available')
    await assert.rejects(f.catalog.assert('store/app', original.projectId), { code: 'ProjectIdentityChanged' })
  } finally { f.close() }
})

test('a moved project keeps its identity at the new address', async () => {
  const f = fixture()
  try {
    const original = await f.catalog.resolve('store/app')
    const moved = f.state.projects.get('store/app')
    f.state.projects.clear()
    f.state.projects.set('other/app', { info: info('other/app', 300), branches: moved.branches })
    const rows = await f.catalog.list()
    assert.equal(rows.length, 1)
    assert.equal(rows[0].key, 'other/app')
    assert.equal(rows[0].projectId, original.projectId)
    assert.equal(rows[0].catalogState, 'available')
  } finally { f.close() }
})

test('a restored copy beside its original does not take over the identity', async () => {
  const f = fixture()
  try {
    const original = await f.catalog.resolve('store/app')
    f.state.projects.set('store/copy', { info: info('store/copy', 400), branches: [...f.state.projects.get('store/app').branches] })
    const rows = await f.catalog.list()
    assert.equal(rows.find((row) => row.key === 'store/app').catalogState, 'available')
    const copy = rows.find((row) => row.key === 'store/copy')
    assert.equal(copy.projectId, original.projectId)
    assert.equal(copy.catalogState, 'identity-mismatch')
    assert.match(copy.catalogError, /store\/app/)
  } finally { f.close() }
})

test('a damaged durable catalog is reported instead of silently replaced', async () => {
  const f = fixture()
  try {
    writeFileSync(join(f.dir, 'project-catalog.json'), '{broken')
    assert.throws(() => f.catalog.retained(), SyntaxError)
  } finally { f.close() }
})

test('parallel identity lookups of one project share one read; a refusal is one attempt', async () => {
  const f = fixture()
  try {
    const lfs = f.catalog['lfs']()
    const list = lfs.branch.list
    let reads = 0, refuse = 1
    lfs.branch.list = async (key, paging) => {
      reads++
      await new Promise((r) => setTimeout(r, 20))
      if (refuse-- > 0) throw Object.assign(new Error('Os { code: 2, kind: NotFound, message: "No such file or directory" }'), { code: 'Failed' })
      return list(key, paging)
    }
    f.state.projects.get('store/app').branches.push(identityBranch(A))
    const refused = await Promise.allSettled([1, 2, 3].map(() => f.catalog.uuidOf('store/app')))
    assert.deepEqual(refused.map((r) => r.status), ['rejected', 'rejected', 'rejected'], 'the shared read was refused: never run again')
    assert.equal(reads, 1, 'three callers, one read')
    const ids = await Promise.all([1, 2, 3].map(() => f.catalog.uuidOf('store/app')))
    assert.deepEqual(ids, [A, A, A])
    assert.equal(reads, 2, 'the next lookup is its own read, shared again')
  } finally { f.close() }
})

test('a listing reads an unclaimed project\'s Branches once per UNCLAIMED_MS; claims and placement reads stay live', async (t) => {
  const f = fixture()
  try {
    let lists = 0
    const lfs = f.catalog['lfs']()
    const list = lfs.branch.list
    lfs.branch.list = async (key, paging) => { lists++; return list(key, paging) }
    t.mock.timers.enable({ apis: ['Date'], now: Date.now() })
    for (let i = 0; i < 3; i++) assert.equal((await f.catalog.list())[0].projectId, 'unclaimed:store/app')
    assert.equal(lists, 1, 'one branch list for three listings')
    // Another installation claims: placement reads see it at once, listings within a minute.
    f.state.projects.get('store/app').branches.push(identityBranch(A))
    assert.equal((await f.catalog.assert('store/app', 'unclaimed:store/app')).projectId, A)
    t.mock.timers.tick(60_000)
    assert.equal((await f.catalog.list())[0].projectId, A)
    // A project recreated under the same name (new `created`) is read again at once.
    f.state.projects.set('store/new', { info: info('store/new', 200), branches: ['main'] })
    await f.catalog.list()
    f.state.projects.set('store/new', { info: info('store/new', 300), branches: ['main'] })
    const before = lists
    await f.catalog.list()
    assert.equal(lists, before + 1, 'a new `created` is a new project')
  } finally { f.close() }
})

test('a listing never trusts an unclaimed read that failed', async () => {
  const f = fixture()
  try {
    let lists = 0, failing = true
    const lfs = f.catalog['lfs']()
    const list = lfs.branch.list
    lfs.branch.list = async (key, paging) => { lists++; if (failing) throw fail('Unavailable'); return list(key, paging) }
    assert.equal((await f.catalog.list())[0].catalogState, 'unavailable')
    failing = false
    assert.equal((await f.catalog.list())[0].catalogState, 'available')
    assert.equal(lists, 2)
  } finally { f.close() }
})
