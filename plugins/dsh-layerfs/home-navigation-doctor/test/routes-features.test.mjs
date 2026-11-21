// Route behaviour that depends on the pinned CLI's call table (features, the
// Stage-0 sandbox images and create guard, docs/specs/sandbox-images.md), the refusal status map, Sandbox read invalidation after
// writes, Branch paging with main first, and a first Commit's changed paths.
//   node --test test/routes-features.test.mjs
import { fileURLToPath } from 'node:url'
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { execFileSync } from 'node:child_process'
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { loadRoutes, testHost } from './routes-helper.mjs'

const R = await loadRoutes()

class Refusal extends Error {
  constructor(code, message, detail = null) { super(message); Object.assign(this, { code, detail, transport: 'refusal' }) }
}

const entry = (family, call, options = [], kind = 'text') => ({ family, call, args: [], options: options.map((name) => ({ name, kind: name === 'on_progress' ? 'function' : kind })) })
/** The pinned 2.0 table: no machine.images/load/prune, no force, `image` a text option (digests only). */
const PINNED = [entry('machine', 'list'), entry('machine', 'stop'), entry('sandbox', 'create', ['image'])]
/** Stage 1's table: images, load, prune, `image` of kind `image`, progress (plus the identity's `sandbox-runtime`). */
const STAGE1 = [entry('machine', 'list'), entry('machine', 'images'), entry('machine', 'load_image'), entry('machine', 'prune'), entry('machine', 'stop', ['force']), entry('sandbox', 'create', ['image', 'on_progress'], 'image')]

function fixture(lfs, calls = PINNED, features = []) {
  const root = mkdtempSync(join(tmpdir(), 'dsh-routes-features-'))
  const t = testHost({ lfs, root, home: join(root, 'home'), discovery: async () => ({ identity: { version: '2.0.0-test', features }, calls }) })
  const groups = R.createGroups(t.host, async () => ({}))
  return { groups, host: t.host, events: t.events, logs: t.logs, http: (method, path, body) => R.handle(groups, { method, path, body }), close: () => rmSync(root, { recursive: true, force: true }) }
}

// the 2.0 machine family: rows keyed `machine`; platforms from home.get --with platforms
const machineBase = {
  list: async () => ({ items: [{ machine: 'native', platform: 'native', state: 'ready', supports: ['sqlite'], stores: [], sandboxes: [] }] }),
}
const homeBase = { get: async () => ({ state: 'ready', platforms: { host: 'macos', items: [] } }) }
const viewLfs = (extra = {}, sandboxes = []) => ({
  machine: { ...machineBase, ...extra },
  home: homeBase,
  store: { list: async () => ({ items: [] }) },
  // 2.0 `sandbox list [--machine M] [--detail]`: `machine` and `image` are long-only
  sandbox: { list: async (o = {}) => ({ items: sandboxes.filter((b) => !o.machine || b.machine === o.machine).map((b) => (o.detail ? { sandbox: b.name, project: 'p', ...b } : { sandbox: b.name, project: 'p' })), next_cursor: null }) },
})

const NO_INSPECT = { machineLog: false, sandboxLog: false, files: false, machineFiles: false, machineStats: false }
/** The features read from one call or option each (layerfs-project#164, #180): absent from this table's CLIs. */
const NO_OBSERVE = { machineSpace: false }
const NONE = { forceStop: false, images: false, imageLoad: false, imagePrune: false, imageRefs: false, createProgress: false, sandboxRuntime: false, imageStage: 0, inspect: NO_INSPECT, ...NO_OBSERVE }

test('features: the machines view says what the pinned CLI has, Stage 1 only with every piece', async () => {
  const pinned = fixture(viewLfs())
  const almost = fixture(viewLfs(), STAGE1)
  const full = fixture(viewLfs(), STAGE1, ['sandbox-runtime'])
  try {
    assert.deepEqual((await pinned.http('GET', '/api/machines')).body.features, NONE)
    // no `sandbox-runtime` identity feature: everything else is there, still Stage 0 (D11)
    assert.deepEqual((await almost.http('GET', '/api/machines')).body.features, { forceStop: true, images: true, imageLoad: true, imagePrune: true, imageRefs: true, createProgress: true, sandboxRuntime: false, imageStage: 0, inspect: NO_INSPECT, ...NO_OBSERVE })
    assert.deepEqual((await full.http('GET', '/api/machines')).body.features, { forceStop: true, images: true, imageLoad: true, imagePrune: true, imageRefs: true, createProgress: true, sandboxRuntime: true, imageStage: 1, inspect: NO_INSPECT, ...NO_OBSERVE })
  } finally { pinned.close(); almost.close(); full.close() }
})

test('features: featuresOf reads the call table and identity; `image` of kind text is no reference support', async () => {
  const { featuresOf } = R.util
  assert.deepEqual(featuresOf(null), NONE)
  assert.deepEqual(featuresOf({ calls: PINNED, identity: { features: ['sandbox-runtime'] } }), { ...NONE, sandboxRuntime: true })
  const json = [entry('machine', 'images'), entry('machine', 'load_image'), entry('machine', 'prune'), entry('sandbox', 'create', ['image'], 'json')]
  assert.equal(featuresOf({ calls: json, identity: { features: ['sandbox-runtime'] } }).imageStage, 0, 'the feature branch\'s json `image` is no reference support')
  assert.equal(featuresOf({ calls: STAGE1, identity: { features: ['sandbox-runtime'] } }).imageStage, 1)
})

const hex = (c) => c.repeat(64)
/** A docker-archive tar from `files` (JSON bodies, or Buffers), at `<dir>/<name>`. */
function tarOf(dir, files, name = 'sandbox-image.tar') {
  mkdirSync(dir, { recursive: true })
  for (const [path, body] of Object.entries(files)) { mkdirSync(join(dir, path, '..'), { recursive: true }); writeFileSync(join(dir, path), Buffer.isBuffer(body) ? body : JSON.stringify(body)) }
  execFileSync('tar', ['-cf', join(dir, name), '-C', dir, ...Object.keys(files)])
  return join(dir, name)
}
const arch = process.arch === 'x64' ? 'amd64' : process.arch
const other = arch === 'arm64' ? 'amd64' : 'arm64'
/** A layer tar holding the glibc loader of this host's architecture (a runnable image's). */
function glibcLayer(dir) {
  const loader = arch === 'amd64' ? 'lib64/ld-linux-x86-64.so.2' : 'lib/ld-linux-aarch64.so.1'
  const src = join(dir, 'glibc-src')
  mkdirSync(join(src, loader, '..'), { recursive: true })
  writeFileSync(join(src, loader), '')
  execFileSync('tar', ['-cf', join(dir, 'glibc.tar'), '-C', src, loader])
  return readFileSync(join(dir, 'glibc.tar'))
}

test('sandbox-images: Stage 0 ready rows from the archive a Sandbox runs from (bundle, machine record, native)', async () => {
  const OWN = `sha256:${hex('2')}`
  const records = {}
  const f = fixture(viewLfs({ get: async (name) => records[name] }, [
    { name: 'b1', machine: 'docker', image: `sha256:${hex('1')}` }, { name: 'b2', machine: 'docker', image: `sha256:${hex('1')}` },
    { name: 'b3', machine: 'vm', image: `sha256:${hex('4')}` }, { name: 'b4', machine: 'other', image: `sha256:${hex('1')}` },
  ]))
  const bundle = join(f.host.root, 'bundle')
  const glibc = glibcLayer(f.host.root)
  tarOf(bundle, { [`blobs/sha256/${hex('1')}`]: { os: 'linux', architecture: arch }, 'layer.tar': glibc, 'manifest.json': [{ Config: `blobs/sha256/${hex('1')}`, Layers: ['layer.tar'] }] })
  const many = tarOf(join(f.host.root, 'many'), {
    [`blobs/sha256/${hex('3')}`]: { os: 'linux', architecture: other }, [`blobs/sha256/${hex('4')}`]: { os: 'linux', architecture: arch }, [`blobs/sha256/${hex('5')}`]: { os: 'linux', architecture: arch }, [`blobs/sha256/${hex('7')}`]: { os: 'linux', architecture: arch },
    'layer.tar': glibc, 'musl.tar': Buffer.alloc(1024),
    'manifest.json': [
      { Config: `blobs/sha256/${hex('3')}`, RepoTags: ['web:1'], Layers: ['layer.tar'] },
      { Config: `blobs/sha256/${hex('4')}`, RepoTags: ['api:1', 'api:latest'], Layers: ['layer.tar'] },
      { Config: `blobs/sha256/${hex('7')}`, RepoTags: ['alpine:1'], Layers: ['musl.tar'] },
      { Config: `blobs/sha256/${hex('5')}`, RepoTags: ['db:1'], Layers: ['layer.tar'] },
    ],
  })
  Object.assign(records, {
    docker: { name: 'docker', platform: 'docker', state: 'running', image: 'debian:13' },
    pinnedvm: { name: 'pinnedvm', platform: 'lima', state: 'stopped', image: OWN },
    vm: { name: 'vm', platform: 'lima', state: 'stopped', image: many },
    native: { name: 'native', platform: 'native', state: 'ready' },
  })
  f.host.bundle = () => bundle
  const get = async (name) => (await f.http('GET', `/api/machines/${name}/sandbox-images`)).body
  try {
    // docker's record image is its worker container's, never a Sandbox image: the bundle's archive
    const docker = await get('docker')
    assert.deepEqual({ ...docker, observedAt: 0 }, {
      machine: 'docker', platform: 'docker', state: 'running', stage: 0, free: null, docker: null, observedAt: 0,
      mode: 'images', architecture: arch, default: `sha256:${hex('1')}`, stale: false,
      ready: [{ id: `sha256:${hex('1')}`, name: 'LayerFS default', refs: [], size: glibc.length, source: 'bundle', sandboxes: 2, lastUsed: null, usable: true }],
    })
    // a stopped VM answers too (the archive is read on this host); its record digest is not a row: the bundle's only image
    const pinned = await get('pinnedvm')
    assert.deepEqual([pinned.state, pinned.default, pinned.ready.map((r) => r.id)], ['stopped', `sha256:${hex('1')}`, [`sha256:${hex('1')}`]])
    // a VM's own archive of several: rows in manifest order, a wrong architecture unusable, the first usable row preselected
    const vm = await get('vm')
    assert.deepEqual(vm.ready.map((r) => [r.name, r.refs, r.source, r.usable, r.reason ?? null, r.sandboxes]), [
      ['web:1', ['web:1'], 'machine', false, { kind: 'arch', value: other }, 0],
      ['api:latest', ['api:1', 'api:latest'], 'machine', true, null, 1],
      ['alpine:1', ['alpine:1'], 'machine', false, { kind: 'no-glibc' }, 0],
      ['db:1', ['db:1'], 'machine', true, null, 0],
    ])
    assert.equal(vm.ready[1].size, glibc.length)
    assert.equal(vm.default, `sha256:${hex('4')}`)
    // the record's digest wins when it is a usable row
    records.vm = { ...records.vm, image_archive: many, image: `sha256:${hex('5')}` }
    assert.equal((await get('vm')).default, `sha256:${hex('5')}`)
    const native = await get('native')
    assert.equal(native.mode, process.platform === 'linux' ? 'images' : 'host')
    if (process.platform !== 'linux') assert.deepEqual([native.ready, native.default, native.docker], [[], null, null])
  } finally { f.close() }
})

test('sandbox-images: an unreadable archive answers the kept rows marked stale, else none', async () => {
  const records = { vm: { name: 'vm', platform: 'lima', state: 'running' } }
  const f = fixture(viewLfs({ get: async (name) => records[name] }))
  const archive = tarOf(join(f.host.root, 'a'), { [`blobs/sha256/${hex('6')}`]: { os: 'linux', architecture: arch }, 'layer.tar': glibcLayer(f.host.root), 'manifest.json': [{ Config: `blobs/sha256/${hex('6')}`, RepoTags: ['x:1'], Layers: ['layer.tar'] }] })
  const get = async (name) => (await f.http('GET', `/api/machines/${name}/sandbox-images`)).body
  try {
    records.vm.image = join(f.host.root, 'missing.tar')
    const none = await get('vm')
    assert.deepEqual([none.ready, none.default, none.stale.code], [[], null, 'ENOENT'])
    records.vm.image = archive
    const good = await get('vm')
    assert.equal(good.stale, false)
    rmSync(archive)
    const stale = await get('vm')
    assert.deepEqual(stale.ready, good.ready)
    assert.equal(stale.default, `sha256:${hex('6')}`)
    assert.equal(stale.stale.at, good.observedAt)
    // no pinned bundle and no archive of its own: no rows, not stale
    records.vm.image = null
    assert.deepEqual([(await get('vm')).ready, (await get('vm')).stale], [[], false])
  } finally { f.close() }
})

test('sandbox-images: the default rule (record digest if usable, else the only usable row, else the first usable)', () => {
  const { defaultOf } = R.machinesModule
  const row = (id, usable = true) => ({ id, usable })
  assert.equal(defaultOf([], 'sha256:a'), null)
  assert.equal(defaultOf([row('a'), row('b')], 'sha256:z'), 'a')
  assert.equal(defaultOf([row('a', false), row('b')], 'b'), 'b')
  assert.equal(defaultOf([row('a', false), row('b')], 'a'), 'b', 'an unusable record digest is not preselected')
  assert.equal(defaultOf([row('a', false)], null), null)
})

test('prune-images: 501 Unsupported until the CLI has machine.prune; never logged as a fault', async () => {
  const f = fixture(viewLfs())
  try {
    const answer = await f.http('POST', '/api/machines/vm/prune-images', { unused: true })
    assert.equal(answer.status, 501)
    assert.equal(answer.body.code, 'Unsupported')
    assert.equal(answer.body.detail.need, 'image-prune')
    assert.deepEqual(f.logs, [])
    for (const path of ['/api/docker-images', '/api/machines/vm/images']) assert.equal((await f.http('GET', path)).status, 404, `${path} is gone`)
    assert.equal((await f.http('POST', '/api/machines/vm/default-image', { digest: `sha256:${hex('1')}` })).status, 404)
  } finally { f.close() }
})

test('create: Stage 0 takes a full digest only (501 image-refs before any call); Stage 1 passes a reference through', async () => {
  const created = []
  // 2.0: a sqlite project's Sandbox runs on its Store's machine (LayerFS's default): no machine is named
  const lfs = {
    project: { get: async (key) => ({ project: key, store: 's1', machine: 'native', engine: 'sqlite' }) },
    store: { get: async () => ({ store: 's1', engine: 'sqlite', machine: 'native' }) },
    sandbox: { create: async (...args) => { created.push(args); return { sandbox: args[1] } } },
  }
  const pinned = fixture(lfs)
  const stage1 = fixture(lfs, STAGE1, ['sandbox-runtime'])
  const key = encodeURIComponent('s1/p')
  const DIG = `sha256:${hex('a')}`
  try {
    for (const image of ['postgres:17', 'sha256:abc', `sha256:${hex('A')}`]) {
      const refused = await pinned.http('POST', `/api/projects/${key}/sandboxes`, { name: 'b1', image })
      assert.equal(refused.status, 501, image)
      assert.deepEqual([refused.body.code, refused.body.detail], ['Unsupported', { need: 'image-refs' }])
    }
    // the object form is gone
    assert.equal((await pinned.http('POST', `/api/projects/${key}/sandboxes`, { name: 'b1', image: { archive: '/tmp/x.tar' } })).status, 400)
    assert.equal(created.length, 0)
    assert.deepEqual(pinned.logs, [], 'expected refusals log no stack')
    assert.equal((await pinned.http('POST', `/api/projects/${key}/sandboxes`, { name: 'b1', image: DIG })).status, 200)
    assert.deepEqual(created.at(-1), ['s1/p', 'b1', { image: DIG }])
    assert.equal((await pinned.http('POST', `/api/projects/${key}/sandboxes`, { name: 'b2' })).status, 200)
    assert.deepEqual(created.at(-1), ['s1/p', 'b2', {}])
    assert.equal((await stage1.http('POST', `/api/projects/${key}/sandboxes`, { name: 'b3', image: 'postgres:17' })).status, 200)
    const [, , options] = created.at(-1)
    assert.deepEqual([created.at(-1).slice(0, 2), options.image, typeof options.on_progress, 'machine' in options], [['s1/p', 'b3'], 'postgres:17', 'function', false], 'the reference as it was picked, with progress')
  } finally { pinned.close(); stage1.close() }
})

test('create: start runs machine.start first (machine-progress), then sandbox.create', async () => {
  const order = []
  const lfs = {
    project: { get: async (key) => ({ project: key, store: 's1', machine: 'lima1', engine: 'sqlite' }) },
    store: { get: async () => ({ store: 's1', engine: 'sqlite', machine: 'lima1' }) },
    machine: { start: async (name, { on_progress }) => { order.push(`start ${name}`); on_progress({ stage: 'boot' }); return { ok: true } } },
    sandbox: { create: async (project, name) => { order.push(`create ${name}`); return { sandbox: name } } },
  }
  const f = fixture(lfs)
  try {
    assert.equal((await f.http('POST', `/api/projects/${encodeURIComponent('s1/p')}/sandboxes`, { name: 'b1', start: true })).status, 200)
    assert.deepEqual(order, ['start lima1', 'create b1'])
    assert.ok(f.events.some((e) => e.type === 'machine-progress' && e.data.name === 'lima1'))
    await f.http('POST', `/api/projects/${encodeURIComponent('s1/p')}/sandboxes`, { name: 'b2' })
    assert.deepEqual(order.slice(2), ['create b2'], 'no start without start: true')
  } finally { f.close() }
})

test('status map: LayerFS and host refusals that are not server faults', async () => {
  const cases = { NotMounted: 409, StaleSandbox: 409, Cancelled: 409, NoStore: 404, NoSuchBucket: 404, Timeout: 504, ProjectIdentityChanged: 409 }
  for (const [code, status] of Object.entries(cases)) {
    const f = fixture({ sandbox: { get: async () => { throw new Refusal(code, code) } } })
    try {
      const answer = await f.http('GET', '/api/sandboxes/b1')
      assert.equal(answer.status, status, code)
      assert.equal(answer.body.code, code)
    } finally { f.close() }
  }
  const f = fixture({})
  try {
    assert.equal((await f.http('POST', '/api/stores', '{')).body.code, 'InvalidInput')
    assert.equal((await f.http('GET', '/api/nothing')).body.code, 'NotFound')
  } finally { f.close() }
})

test('errors: Capacity is 507 with its detail; Unavailable, Capacity and Unsupported log no stack, Unknown and Timeout do', async () => {
  assert.deepEqual(R.errorAnswer(new Refusal('Capacity', 'need more', { needed_bytes: 10, free_bytes: 1, path: '/d' })), { status: 507, body: { code: 'Capacity', transport: 'refusal', message: 'need more', detail: { needed_bytes: 10, free_bytes: 1, path: '/d' } } })
  // a refusal's fix line reaches the client beside its code
  assert.deepEqual(R.errorAnswer(Object.assign(new Refusal('Exists', 'taken'), { fix: 'layerfs sandbox create p s2' })).body, { code: 'Exists', transport: 'refusal', message: 'taken', fix: 'layerfs sandbox create p s2' })
  const answered = await (async () => { const f = fixture({ sandbox: { get: async () => { throw Object.assign(new Refusal('Unavailable', 'stopped'), { fix: 'layerfs machine start dev' }) } } }); try { return await f.http('GET', '/api/sandboxes/b1') } finally { f.close() } })()
  assert.equal(answered.body.fix, 'layerfs machine start dev')
  for (const [code, status, logged] of [['Unavailable', 502, false], ['Capacity', 507, false], ['Unsupported', 501, false], ['MountsActive', 409, false], ['Unknown', 500, true], ['Timeout', 504, true]]) {
    const f = fixture({ sandbox: { get: async () => { throw new Refusal(code, code) } } })
    try {
      assert.equal((await f.http('GET', '/api/sandboxes/b1')).status, status, code)
      assert.equal(f.logs.length > 0, logged, code)
    } finally { f.close() }
  }
})

test('Sandbox reads: a cached Busy refusal is dropped by mount, unmount, commit and close', async () => {
  let busy = true
  let reads = 0
  const lfs = {
    sandbox: {
      get: async () => { reads++; if (busy) throw new Refusal('Busy', 'held'); return { sandbox: 'b1', project: 's1/p', workspaces: [] } },
      remove: async () => ({ ok: true, outcome: 'removed', unmounted: [] }),
    },
    workspace: { mount: async () => ({ workspace: 'b1:w' }), unmount: async () => ({ ok: true }), commit: async () => ({ commit: `12${'c'.repeat(64)}` }) },
  }
  const f = fixture(lfs)
  // a good answer serves every tab for SANDBOX_KEEP_MS: each round starts once it has run out
  const realNow = Date.now
  let skew = 0
  Date.now = () => realNow() + skew
  const busyAgain = async () => {
    busy = true
    skew += R.util.SANDBOX_KEEP_MS + 1
    assert.equal((await f.http('GET', '/api/sandboxes/b1')).status, 409)
    const before = reads
    assert.equal((await f.http('GET', '/api/sandboxes/b1')).status, 409)
    assert.equal(reads, before, 'Busy answers from the cache')
    busy = false
  }
  try {
    for (const write of [
      () => f.http('POST', '/api/sandboxes/b1/workspaces', { branch: 'side' }),
      () => f.http('DELETE', `/api/workspaces/${encodeURIComponent('b1:w')}?force=1`),
      () => f.http('POST', `/api/workspaces/${encodeURIComponent('b1:w')}/commit`, {}),
      () => f.http('DELETE', '/api/sandboxes/b1?force=1'),
    ]) {
      await busyAgain()
      assert.equal((await write()).status, 200)
      const before = reads
      assert.equal((await f.http('GET', '/api/sandboxes/b1')).status, 200, 'the write cleared the Busy answer')
      assert.equal(reads, before + 1)
    }
  } finally { Date.now = realNow; f.close() }
})

test('Branches: main first on page 0 and never again, however many pages', async () => {
  const head = 'c'.repeat(66)
  // 2.0 `branch list --detail` by cursor: rows `{ branch, commit }`; main read on its own (`branch get`)
  const listed = [
    { items: [{ branch: 'dsh.session.a', commit: head }, { branch: 'dsh.session.b', commit: head }], next_cursor: 'k2' },
    { items: [{ branch: 'main', commit: head }, { branch: 'side', commit: head }] },
  ]
  const lfs = { branch: { list: async (_p, { cursor }) => listed[cursor === 'k2' ? 1 : 0], get: async () => ({ commit: head }) } }
  const f = fixture(lfs)
  const key = encodeURIComponent('s1/p')
  try {
    const h0 = await f.http('GET', `/api/projects/${key}/history`)
    const h1 = await f.http('GET', `/api/projects/${key}/history?pos=k2`)
    assert.deepEqual([...h0.body.branches, ...h1.body.branches].map((b) => b.name), ['main', 'dsh.session.a', 'dsh.session.b', 'side'])
    assert.equal(h0.body.next, 'k2')
    const b0 = await f.http('GET', `/api/projects/${key}/branches`)
    const b1 = await f.http('GET', `/api/projects/${key}/branches?pos=k2`)
    assert.deepEqual([...b0.body.items, ...b1.body.items].map((b) => b.name), ['main', 'dsh.session.a', 'dsh.session.b', 'side'])
    assert.equal(b0.body.items[0].head, head)
  } finally { f.close() }
})

test('Commits: a Branch\'s first Commit diffs against its base Layer', async () => {
  const layer = `32${'a'.repeat(64)}`
  const diffs = []
  const lfs = {
    branch: { get: async () => ({ forked_from: null }) },
    // 2.0 `commit list --detail` rows and `commit diff <ref> --from`
    commit: {
      list: async () => ({ items: [{ commit: 'c2', parent: 'c1', base: layer }, { commit: 'c1', parent: null, base: layer }] }),
      diff: async (_p, ref, { from }) => { diffs.push([from, ref]); return { items: [{ path: `/${ref}.txt`, change: 'added' }] } },
    },
  }
  const f = fixture(lfs)
  try {
    const answer = await f.http('GET', `/api/projects/${encodeURIComponent('s1/p')}/commits?branch=side`)
    assert.equal(answer.status, 200)
    assert.deepEqual(diffs.sort(), [[layer, 'c1'], ['c1', 'c2']])
    assert.deepEqual(answer.body.map((c) => c.changes), [[{ path: '/c2.txt', kind: 'added' }], [{ path: '/c1.txt', kind: 'added' }]])
  } finally { f.close() }
})

test('file compare: a modified file keeps its before side, an added one has none, a non-text one has sizes', async () => {
  const pin = (c) => c.repeat(66)
  const answers = {
    '/a.txt': { path: '/a.txt', change: 'modified', type: 'file', content: true, hunks: [{ op: ' ', text: 'one' }, { op: '-', text: 'two' }, { op: '+', text: 'three' }] },
    '/new.txt': { path: '/new.txt', change: 'added', type: 'file', content: true, hunks: [{ op: '+', text: 'hi' }] },
    '/gone.txt': { path: '/gone.txt', change: 'removed', type: 'file', content: true, hunks: [{ op: '-', text: 'bye' }] },
    '/img.png': { path: '/img.png', change: 'modified', type: 'file', content: true },
  }
  const stats = []
  const lfs = {
    branch: { get: async () => ({ head: pin('b') }) },
    commit: { diff: async (_p, _ref, { path }) => ({ items: [answers[path]] }) },
    file: { stat: async (_p, _b, path, { at }) => { stats.push([path, at]); return { size: at === pin('a') ? 10 : 20 } } },
  }
  const f = fixture(lfs)
  const query = (path) => f.http('GET', `/api/projects/${encodeURIComponent('s1/p')}/file-query?branch=main&path=${encodeURIComponent(path)}&pin=${pin('b')}&left=${pin('a')}&kind=compare`)
  try {
    const modified = (await query('/a.txt')).body
    assert.deepEqual(modified.rows.map((r) => [r.op, r.left_line ?? null, r.right_line ?? null]), [[' ', 1, 1], ['-', 2, null], ['+', null, 2]])
    assert.deepEqual([modified.before, modified.after], [{}, {}])
    const added = (await query('/new.txt')).body
    assert.deepEqual([added.before, added.after], [null, {}])
    const removed = (await query('/gone.txt')).body
    assert.deepEqual([removed.before, removed.after], [{}, null])
    assert.deepEqual(stats, [], 'text files are not stat-ed')
    const binary = (await query('/img.png')).body
    assert.deepEqual([binary.rows, binary.before, binary.after], [[], { size: 10 }, { size: 20 }])
  } finally { f.close() }
})

test('file history and blame name the Branch each Commit was made on, asking LayerFS once per Commit', async () => {
  const pin = (c) => c.repeat(66)
  const gets = []
  const lfs = {
    branch: { get: async () => ({ head: pin('f') }) },
    // provenance: two of feat's own Commits and one made on main before the fork
    file: {
      history: async (_p, _b, _path, { traversal }) => (traversal === 'branch'
        ? { items: [{ commit: 'c3' }, { commit: 'c2' }] }
        : { items: [{ commit: 'c3', parent: 'c2', change: 'modified' }, { commit: 'c2', parent: null, change: 'modified' }, { commit: 'm1', parent: 'm0', change: 'added' }] }),
      read: async () => ({ text: 'a\nb\n' }),
      blame: async () => ({ ranges: [{ start: 1, end: 1, commit: 'm1' }, { start: 2, end: 2, commit: 'c3' }], commits: [] }),
    },
    commit: { get: async (_p, id) => { gets.push(id); return { m1: { branch: 'main' }, c3: { branch: 'feat' } }[id] ?? {} } },
  }
  const f = fixture(lfs)
  const query = (kind) => f.http('GET', `/api/projects/${encodeURIComponent('s1/p')}/file-query?branch=feat&path=%2Fa.txt&kind=${kind}`)
  try {
    const revisions = (await query('revisions')).body.items
    assert.deepEqual(revisions.map((r) => [r.ref.id, r.ref.branch, r.compared_to?.branch ?? null]), [['c3', 'feat', 'feat'], ['c2', 'feat', null], ['m1', 'main', 'main']])
    assert.deepEqual(gets, ['m1'], 'only the Commit not on the Branch\'s own line is asked')
    const blame = (await query('blame')).body.lines
    assert.deepEqual(blame.map((l) => l.owner.ref.branch), ['main', 'feat'])
    assert.deepEqual(gets, ['m1', 'c3'], 'm1 was kept from the history read')
    await query('blame')
    assert.deepEqual(gets, ['m1', 'c3'], 'a Commit\'s Branch is asked once')
  } finally { f.close() }
})

test('file history preserves an unknown provenance branch and retries its lookup', async (t) => {
  const { build } = await import('esbuild')
  const bundle = await build({ entryPoints: [fileURLToPath(new URL('../src/client/files/api.ts', import.meta.url))], bundle: true, write: false, format: 'esm', platform: 'node' })
  const { filesApi } = await import(`data:text/javascript;base64,${Buffer.from(bundle.outputFiles[0].contents).toString('base64')}`)
  const commit = 'a'.repeat(66), parent = 'b'.repeat(66)
  let readable = false
  const f = fixture({
    file: { history: async (_project, _branch, _path, { traversal }) => ({
      items: traversal === 'branch' ? [] : [{ commit, parent, change: 'modified' }],
    }) },
    commit: {
      get: async () => {
        if (!readable) throw new Error('commit temporarily unavailable')
        return { branch: 'main' }
      },
      diff: async (_project, at, { from, path }) => {
        assert.deepEqual([at, from, path], [commit, parent, '/report.txt'])
        return { items: [{ path, change: 'modified', hunks: [{ op: '+', text: 'updated' }] }] }
      },
    },
  })
  t.after(f.close)
  const api = { call: async (...args) => {
    const reply = await f.http(...args)
    assert.equal(reply.status, 200)
    return reply.body
  } }
  const read = () => filesApi.revisions(api, 's1/p', 'feat', '/report.txt', commit, 'provenance')
  const [unknown] = (await read()).items
  assert.deepEqual([unknown.ref.id, unknown.ref.branch, unknown.compared_to.id, unknown.compared_to.branch], [commit, null, parent, null])
  const comparison = await filesApi.fileDiff(api, 's1/p', unknown.ref.branch ?? 'feat', '/report.txt', unknown.compared_to.id, unknown.ref.id, unknown.compared_to.branch ?? 'feat')
  assert.deepEqual(comparison.rows, [{ op: '+', text: 'updated', right_line: 1 }])
  readable = true
  const [known] = (await read()).items
  assert.deepEqual([known.ref.branch, known.compared_to.branch], ['main', 'main'])
})

test('client api: a Remote call lost after it was sent is tagged transport-unknown', async (t) => {
  const { build } = await import('esbuild')
  const { pathToFileURL } = await import('node:url')
  const root = fileURLToPath(new URL('..', import.meta.url))
  const out = mkdtempSync(join(tmpdir(), 'dsh-api-'))
  t.after(() => rmSync(out, { recursive: true, force: true }))
  await build({ absWorkingDir: root, stdin: { contents: "export { createApi, HostError } from './src/client/api.ts'", resolveDir: root, loader: 'ts' }, outfile: join(out, 'api.mjs'), bundle: true, format: 'esm', platform: 'node', target: 'node24', logLevel: 'warning' })
  const { createApi } = await import(pathToFileURL(join(out, 'api.mjs')).href)
  const events = { [Symbol.asyncIterator]: () => ({ next: () => new Promise(() => {}) }), dispose() {} }
  const api = createApi(() => ({ call: async () => { throw new Error('connection reset') }, events: () => events }), { openPanel() {}, openWorkspace: async () => {}, openSession() {} }, () => undefined)
  try {
    await assert.rejects(api.call('POST', '/api/machines/a/stop'), (e) => e.code === 'no-host' && e.status === 0 && e.transport === 'transport-unknown')
  } finally { api.dispose() }
})

test('client api: New project session creates distinct drafts in a shared scratch workspace', async (t) => {
  const { build } = await import('esbuild')
  const { outputFiles } = await build({ entryPoints: [fileURLToPath(new URL('../src/client/api.ts', import.meta.url))], bundle: true, write: false, format: 'esm', platform: 'node' })
  const { createApi } = await import(`data:text/javascript;base64,${Buffer.from(outputFiles[0].contents).toString('base64')}`)
  const bindings = new Map()
  const opened = []
  const workspace = { connectWorkspace: async () => 'existing-draft' }
  const events = { [Symbol.asyncIterator]: () => ({ next: () => new Promise(() => {}) }), dispose() {} }
  const remote = { events: () => events, async call({ op, args }) {
    if (op === 'sessions.prepareScratch') return { ok: true, value: { workspaceId: 'scratch' } }
    assert.equal(op, 'sessions.choose')
    bindings.set(args.sessionId, args.project)
    return { ok: true, value: {} }
  } }
  const api = createApi(() => remote, {
    createSession: async workspaceId => { assert.equal(workspaceId, 'scratch'); return `session-${opened.length + 1}` },
    openSession: id => opened.push(id),
  }, () => workspace)
  t.after(() => api.dispose())
  await api.openProjectSession('store/first')
  await api.openProjectSession('store/second')
  assert.deepEqual([...bindings], [['session-1', 'store/first'], ['session-2', 'store/second']])
  assert.deepEqual(opened, ['session-1', 'session-2'])
})

/** The client API over a fake Remote whose event stream the test drops and reopens, and whose calls it answers by hand. */
async function droppableApi(t) {
  const { build } = await import('esbuild')
  const { pathToFileURL } = await import('node:url')
  const root = fileURLToPath(new URL('..', import.meta.url))
  const out = mkdtempSync(join(tmpdir(), 'dsh-api-'))
  t.after(() => rmSync(out, { recursive: true, force: true }))
  await build({ absWorkingDir: root, stdin: { contents: "export { createApi, HostError, GET_TIMEOUT_MS } from './src/client/api.ts'", resolveDir: root, loader: 'ts' }, outfile: join(out, 'api.mjs'), bundle: true, format: 'esm', platform: 'node', target: 'node24', logLevel: 'warning' })
  const mod = await import(pathToFileURL(join(out, 'api.mjs')).href)
  // Each events() call is one connection: it says `ready`, then ends when the test drops it.
  let end = () => {}
  const stream = () => {
    const queue = [{ type: 'ready' }]
    let wake = null
    end = () => { queue.push(null); wake?.() }
    return {
      [Symbol.asyncIterator]: () => ({
        next: async () => {
          while (!queue.length) await new Promise((r) => { wake = r })
          const item = queue.shift()
          return item === null ? { done: true, value: undefined } : { done: false, value: item }
        },
      }),
      dispose() {},
    }
  }
  const sent = []
  const remote = { call: (req) => new Promise((resolve) => sent.push({ req, answer: (body) => resolve({ ok: true, value: { status: 200, body } }) })), events: stream }
  const api = mod.createApi(() => remote, { openPanel() {}, openWorkspace: async () => {}, openSession() {} }, () => undefined)
  t.after(() => api.dispose())
  const connected = () => new Promise((resolve) => { if (api.connected()) resolve(); else { const off = api.onEvent('connection', (e) => { if (e.data?.connected) { off(); resolve() } }) } })
  await connected()
  return { api, mod, sent, drop: () => end(), connected, tick: () => new Promise((r) => setTimeout(r, 0)) }
}

test('client api: a GET whose connection drops is sent again once the host is back, and answers', async (t) => {
  const { api, sent, drop } = await droppableApi(t)
  const reply = api.call('GET', '/api/storage')
  await new Promise((r) => setTimeout(r, 10))
  assert.equal(sent.length, 1)
  drop()
  // The stream reconnects after its back-off (500 ms); the read goes out again then.
  for (let i = 0; i < 300 && sent.length < 2; i++) await new Promise((r) => setTimeout(r, 10))
  assert.equal(sent.length, 2, 'the read was re-sent on the new connection')
  assert.equal(api.connected(), true)
  assert.deepEqual(sent[1].req, sent[0].req)
  sent[1].answer({ stores: [] })
  assert.deepEqual(await reply, { stores: [] })
})

test('client api: a write whose connection drops fails as transport-unknown and is never re-sent', async (t) => {
  const { api, sent, drop, connected } = await droppableApi(t)
  const reply = api.call('POST', '/api/machines/a/stop')
  await new Promise((r) => setTimeout(r, 10))
  drop()
  await assert.rejects(reply, (e) => e.code === 'no-host' && e.status === 0 && e.transport === 'transport-unknown')
  await connected()
  await new Promise((r) => setTimeout(r, 10))
  assert.equal(sent.length, 1)
})

test('client api: a named operation whose connection drops fails instead of waiting forever', async (t) => {
  const { api, drop } = await droppableApi(t)
  const reply = api.op('session.state', { sessionId: 's' })
  await new Promise((r) => setTimeout(r, 10))
  drop()
  await assert.rejects(reply, (e) => e.code === 'no-host' && e.transport === 'transport-unknown')
})

test('client api: a GET that never answers fails after its budget', async (t) => {
  const { api, mod, sent } = await droppableApi(t)
  t.mock.timers.enable({ apis: ['setTimeout'] })
  const reply = api.call('GET', '/api/storage')
  await Promise.resolve(); await Promise.resolve(); await Promise.resolve()
  assert.equal(sent.length, 1)
  t.mock.timers.tick(mod.GET_TIMEOUT_MS + 1)
  await assert.rejects(reply, (e) => e.code === 'timeout' && e.transport === 'transport-unknown')
})

// ---------------------------------------------------------------- Stage 1 (docs/specs/sandbox-images.md §4.1)

const PG = `sha256:${hex('d')}`, BUNDLE = `sha256:${hex('b')}`, UB = `sha256:${hex('c')}`
/** `machine images --detail --with docker` (2.0): `items` keyed `image` with `default`, `disk_free_bytes`, and the `docker` section LayerFS lists, dedupes and sizes. */
const dockerImages = () => [
  { image: PG, refs: ['postgres:17-bookworm'], size: 412e6, os: 'linux', architecture: arch, usable: true, unusable: null, needed_bytes: 865_200_000, fits: true },
  { image: `sha256:${hex('e')}`, refs: ['mssql:2022'], size: 1.5e9, os: 'linux', architecture: other, usable: false, unusable: { kind: 'arch', value: other }, needed_bytes: 3_150_000_000, fits: true },
]
const imagesAnswer = (extra = {}) => ({
  items: [
    { image: BUNDLE, refs: [`layerfs-cache/sandbox:${hex('b')}`], size: 310e6, default: true, architecture: arch, source: 'bundle', sandboxes: ['b0'], sandboxes_count: 1, last_used_at: null },
    { image: UB, refs: ['ubuntu:24.04'], size: 780e6, default: false, architecture: arch, source: 'imported', sandboxes: ['b1', 'b2'], sandboxes_count: 2, last_used_at: '2026-11-13T10:00:00Z' },
  ],
  machine: 'lima1', mode: 'images', architecture: arch, disk_free_bytes: 5e9,
  docker: { images: dockerImages(), reason: null, message: null },
  ...extra,
})
function stage1Fixture({ images = async () => imagesAnswer(), prune, create } = {}) {
  const calls = []
  const lfs = {
    project: { get: async (key) => ({ project: key, store: 's1', machine: 'lima1', engine: 'sqlite' }) },
    machine: {
      ...machineBase,
      get: async (name) => ({ machine: name, platform: name === 'docker' ? 'docker' : 'lima', state: 'ready' }),
      images: async (...args) => { calls.push(['images', ...args]); return images(...args) },
      prune: async (...args) => { calls.push(['prune', ...args]); return prune(...args) },
    },
    store: { list: async () => ({ items: [] }), get: async () => ({ store: 's1', engine: 'sqlite', machine: 'lima1' }) },
    sandbox: { list: async () => ({ items: [] }), create: async (...args) => { calls.push(['create', ...args]); return create ? create(...args) : { sandbox: args[1] } } },
  }
  return { ...fixture(lfs, STAGE1, ['sandbox-runtime']), calls }
}

test('sandbox-images Stage 1: ready rows, free, default and the Docker group come from machine.images as LayerFS answers them', async () => {
  const f = stage1Fixture()
  try {
    const answer = (await f.http('GET', '/api/machines/lima1/sandbox-images')).body
    assert.deepEqual({ ...answer, observedAt: 0 }, {
      machine: 'lima1', platform: 'lima', state: 'ready', stage: 1, mode: 'images', architecture: arch, free: 5e9,
      // D7: the newest last_used wins over LayerFS's default
      default: UB, observedAt: 0, stale: false,
      ready: [
        { id: BUNDLE, name: 'LayerFS default', refs: [], size: 310e6, source: 'bundle', sandboxes: 1, lastUsed: null, usable: true },
        { id: UB, name: 'ubuntu:24.04', refs: ['ubuntu:24.04'], size: 780e6, source: 'imported', sandboxes: 2, lastUsed: '2026-11-13T10:00:00Z', usable: true },
      ],
      docker: { available: true, images: [
        { id: PG, name: 'postgres:17-bookworm', refs: ['postgres:17-bookworm'], size: 412e6, architecture: arch, os: 'linux', need: 865_200_000, fits: true, usable: true },
        { id: `sha256:${hex('e')}`, name: 'mssql:2022', refs: ['mssql:2022'], size: 1.5e9, architecture: other, os: 'linux', need: 3_150_000_000, fits: true, usable: false, reason: { kind: 'arch', value: other } },
      ] },
    })
    assert.deepEqual(f.calls.filter(([c]) => c === 'images'), [['images', 'lima1', { detail: true, with: ['docker'] }]], 'one machine.images --with docker read; the plugin never reads Docker')
  } finally { f.close() }
})

test('sandbox-images Stage 1: Docker off, LayerFS default without last_used, macOS native host mode', async () => {
  let answer = imagesAnswer({ docker: { images: [], reason: 'not_running', message: 'Cannot connect to the Docker daemon' } })
  answer.items[1].last_used_at = null
  const f = stage1Fixture({ images: async () => answer })
  try {
    const off = (await f.http('GET', '/api/machines/lima1/sandbox-images')).body
    assert.deepEqual(off.docker, { available: false, reason: 'not-running', message: 'Cannot connect to the Docker daemon' })
    assert.equal(off.default, BUNDLE, 'never used: LayerFS\'s own default')
    answer = imagesAnswer({ docker: { images: [], reason: 'weird', message: null } })
    const odd = (await f.http('GET', '/api/machines/lima1/sandbox-images')).body
    assert.deepEqual(odd.docker, { available: false, reason: 'error' })
    answer = { items: [], machine: 'lima1', mode: 'host', architecture: null, disk_free_bytes: null, docker: null }
    const host = (await f.http('GET', '/api/machines/lima1/sandbox-images')).body
    assert.deepEqual([host.mode, host.stage, host.ready, host.docker, host.default], ['host', 1, [], null, null])
    answer = imagesAnswer({ docker: null })
    assert.equal((await f.http('GET', '/api/machines/lima1/sandbox-images')).body.docker, null, 'no Docker part: no group')
  } finally { f.close() }
})

test('sandbox-images Stage 1: a stopped machine (Unavailable) answers the kept answer marked stale, else none', async () => {
  let refuse = false
  const f = stage1Fixture({ images: async () => { if (refuse) throw new Refusal('Unavailable', 'machine lima1 is stopped', { what: 'machine' }); return imagesAnswer() } })
  try {
    refuse = true
    const none = (await f.http('GET', '/api/machines/lima1/sandbox-images')).body
    assert.deepEqual([none.ready, none.docker, none.free, none.default, none.stale.code], [[], null, null, null, 'Unavailable'])
    refuse = false
    const good = (await f.http('GET', '/api/machines/lima1/sandbox-images')).body
    refuse = true
    const kept = (await f.http('GET', '/api/machines/lima1/sandbox-images')).body
    assert.deepEqual([kept.ready, kept.docker, kept.free, kept.default], [good.ready, good.docker, good.free, good.default])
    assert.deepEqual(kept.stale, { at: good.observedAt, code: 'Unavailable', message: 'machine lima1 is stopped' })
    assert.deepEqual(f.logs, [])
  } finally { f.close() }
})

test('sandbox-images Stage 1: a worker of another LayerFS build is WorkerOutdated (message kept), not a stopped machine', async () => {
  const message = 'machine docker is of another build (2.0.0+abc); stop it and start it again to replace its worker'
  let refuse = false
  const f = stage1Fixture({ images: async () => { if (refuse) throw new Refusal('Unavailable', message, { what: 'machine' }); return imagesAnswer() } })
  try {
    refuse = true
    const none = (await f.http('GET', '/api/machines/lima1/sandbox-images')).body
    assert.equal(none.state, 'ready')
    assert.deepEqual([none.ready, none.stale.code, none.stale.message], [[], 'WorkerOutdated', message])
    refuse = false
    const good = (await f.http('GET', '/api/machines/lima1/sandbox-images')).body
    refuse = true
    const kept = (await f.http('GET', '/api/machines/lima1/sandbox-images')).body
    assert.deepEqual([kept.ready, kept.docker], [good.ready, good.docker], 'the kept answer, Docker group included')
    assert.equal(kept.stale.code, 'WorkerOutdated')
    assert.deepEqual(R.machinesModule.staleOf(new Refusal('Unavailable', 'machine lima1 is stopped'), 1), { at: 1, code: 'Unavailable', message: 'machine lima1 is stopped' })
    assert.equal(R.machinesModule.staleOf(new Refusal('Busy', 'replace its worker later'), 1).code, 'Busy', 'only Unavailable')
    assert.deepEqual(f.logs, [])
  } finally { f.close() }
})

test('sandbox-images Stage 1: an image is named by its :latest ref, else its first; every ref listed', async () => {
  const answer = imagesAnswer()
  answer.items[1].refs = ['alpine:3', 'alpine:latest']
  answer.docker.images[0].refs = ['postgres:17', 'postgres:latest', 'postgres:17-bookworm']
  answer.docker.images[1].refs = ['mssql:2022', 'mssql:2022-cu1']
  const f = stage1Fixture({ images: async () => answer })
  try {
    const got = (await f.http('GET', '/api/machines/lima1/sandbox-images')).body
    assert.deepEqual([got.ready[1].name, got.ready[1].refs], ['alpine:latest', ['alpine:3', 'alpine:latest']])
    assert.deepEqual(got.docker.images.map((r) => r.name), ['postgres:latest', 'mssql:2022'])
    assert.equal(got.docker.images[0].refs.length, 3)
  } finally { f.close() }
})

test('prune-images Stage 1: per id, a refused image listed (InUse) without failing the rest; unused; nothing named is refused', async () => {
  const f = stage1Fixture({
    prune: async (name, image) => {
      // the 2.0 long form of machine.prune
      if (typeof image === 'object') return { freed_bytes: 900, outcome: 'removed', machine: name, removed: [PG, UB], removed_count: 2 }
      if (image === UB) throw new Refusal('InUse', 'ubuntu:24.04 is used by 2 Sandboxes')
      return { freed_bytes: 400, outcome: 'removed', machine: name, removed: [image], removed_count: 1 }
    },
  })
  try {
    const answer = await f.http('POST', '/api/machines/lima1/prune-images', { images: [PG, UB, PG] })
    assert.equal(answer.status, 200)
    assert.deepEqual(answer.body, { removed: [PG], freed: 400, refused: [{ id: UB, code: 'InUse', message: 'ubuntu:24.04 is used by 2 Sandboxes' }] })
    assert.deepEqual(f.calls.filter(([c]) => c === 'prune'), [['prune', 'lima1', PG, { detail: true }], ['prune', 'lima1', UB, { detail: true }]], 'once per id, in the long form')
    assert.deepEqual((await f.http('POST', '/api/machines/lima1/prune-images', { unused: true })).body, { removed: [PG, UB], freed: 900, refused: [] })
    assert.deepEqual(f.calls.at(-1), ['prune', 'lima1', { unused: true, detail: true }])
    assert.equal((await f.http('POST', '/api/machines/lima1/prune-images', {})).status, 400)
  } finally { f.close() }
  const g = stage1Fixture({ prune: async () => { throw new Refusal('Unavailable', 'machine lima1 is stopped') } })
  try {
    const answer = await g.http('POST', '/api/machines/lima1/prune-images', { images: [PG] })
    assert.deepEqual([answer.status, answer.body.code], [502, 'Unavailable'], 'a machine-wide refusal fails the request')
  } finally { g.close() }
})

test('create Stage 1: Capacity precheck (507, before any create or job) only for a Docker image that does not fit', async () => {
  let answer = imagesAnswer({ disk_free_bytes: 5e8 })
  const f = stage1Fixture({ images: async () => answer })
  const key = encodeURIComponent('s1/p')
  try {
    for (const image of [PG, 'postgres:17-bookworm']) {
      for (const background of [false, true]) {
        const refused = await f.http('POST', `/api/projects/${key}/sandboxes`, { name: 'b1', image, background })
        assert.equal(refused.status, 507, `${image} ${background}`)
        assert.deepEqual([refused.body.code, refused.body.detail], ['Capacity', { needed_bytes: 865_200_000, free_bytes: 5e8, path: null }], 'in LayerFS\'s own Capacity shape')
      }
    }
    assert.equal(f.calls.filter(([c]) => c === 'create').length, 0)
    assert.deepEqual(f.events.filter((e) => e.type === 'sandbox-progress'), [], 'no job admitted')
    assert.deepEqual(f.logs, [], 'an expected refusal logs no stack')
    // a ready image needs no copy; an unknown name is LayerFS's to judge
    for (const image of [UB, 'ubuntu:24.04', 'nothing:here']) assert.equal((await f.http('POST', `/api/projects/${key}/sandboxes`, { name: `r${image.length}`, image })).status, 200, image)
    answer = imagesAnswer({ disk_free_bytes: 5e9 })
    assert.equal((await f.http('POST', `/api/projects/${key}/sandboxes`, { name: 'fits', image: PG })).status, 200, 'fits')
    answer = imagesAnswer({ disk_free_bytes: null })
    assert.equal((await f.http('POST', `/api/projects/${key}/sandboxes`, { name: 'unknown', image: PG })).status, 200, 'free unknown: no precheck')
  } finally { f.close() }
})

test('create Stage 1: the CLI\'s phases become the job\'s (bytes coalesced), unknown events ignored, refusals kept with detail', async () => {
  const f = stage1Fixture({
    create: async (project, name, { on_progress }) => {
      for (const p of [{ phase: 'inspect' }, { phase: 'export' }, { phase: 'send', done: 0, total: 412 }, { phase: 'send', done: 100, total: 412 }, { stage: 'other' }, { phase: 'unpack' }, { phase: 'start' }]) on_progress(p)
      if (name === 'bad') throw new Refusal('Unsupported', 'arm64 machine, amd64 image', { architecture: 'amd64' })
      if (name === 'full') throw Object.assign(new Refusal('Capacity', 'no room', { needed_bytes: 10, free_bytes: 1, path: '/d' }), { fix: 'layerfs machine prune dev --unused' })
      return { sandbox: name }
    },
  })
  const key = encodeURIComponent('s1/p')
  const settle = () => new Promise((r) => setTimeout(r, 20))
  try {
    const job = (await f.http('POST', `/api/projects/${key}/sandboxes`, { name: 'b1', image: PG, background: true })).body.creation
    await settle()
    const seen = f.events.filter((e) => e.type === 'sandbox-progress' && e.data.creation.id === job.id).map((e) => [e.data.creation.phase, e.data.creation.done ?? null])
    assert.deepEqual(seen, [['queued', null], ['inspect', null], ['export', null], ['send', 0], ['unpack', null], ['start', null], ['done', null]], 'phase changes at once; the second send held (500 ms) and dropped by unpack')
    const bad = (await f.http('POST', `/api/projects/${key}/sandboxes`, { name: 'bad', image: PG, background: true })).body.creation
    await settle()
    const failed = (await f.http('GET', '/api/sandbox-creates')).body.creations.find((c) => c.id === bad.id)
    assert.deepEqual(failed.error, { code: 'Unsupported', message: 'arm64 machine, amd64 image', detail: { architecture: 'amd64' } })
    // the refusal's fix line stays on the job's error record (the page offers it as the one action)
    const full = (await f.http('POST', `/api/projects/${key}/sandboxes`, { name: 'full', image: PG, background: true })).body.creation
    await settle()
    const kept = (await f.http('GET', '/api/sandbox-creates')).body.creations.find((c) => c.id === full.id)
    assert.equal(kept.error.fix, 'layerfs machine prune dev --unused')
  } finally { f.close() }
})

test('create: no on_progress option when the CLI declares none (Stage 0 sends exactly what it did)', async () => {
  const created = []
  // 2.0: a sqlite project's Sandbox runs on its Store's machine (LayerFS's default): no machine is named
  const lfs = {
    project: { get: async (key) => ({ project: key, store: 's1', machine: 'native', engine: 'sqlite' }) },
    store: { get: async () => ({ store: 's1', engine: 'sqlite', machine: 'native' }) },
    sandbox: { create: async (...args) => { created.push(args); return { sandbox: args[1] } } },
  }
  const f = fixture(lfs)
  try {
    await f.http('POST', `/api/projects/${encodeURIComponent('s1/p')}/sandboxes`, { name: 'b1', image: `sha256:${hex('a')}` })
    assert.deepEqual(created, [['s1/p', 'b1', { image: `sha256:${hex('a')}` }]])
  } finally { f.close() }
})

// ---------------------------------------------------------------- monitoring reads (layerfs-project#178 "Host side")

// machine.log and sandbox.log (2.0: both page backwards, `--cursor` is a page's next_cursor)
const LOGS = [...PINNED, entry('machine', 'log', ['limit', 'cursor']), entry('sandbox', 'log', ['limit', 'cursor'])]
const refusal = (code, message = code) => new Refusal(code, message)
const tick = () => new Promise((r) => setTimeout(r, 10))

test('features: each monitoring call is its own feature, read from the call table; none is required', async () => {
  const { featuresOf } = R.util
  assert.deepEqual(featuresOf({ calls: LOGS }).inspect, { machineLog: true, sandboxLog: true, files: false, machineFiles: false, machineStats: false })
  assert.deepEqual(featuresOf({ calls: [entry('logs', 'list'), entry('sandbox', 'log'), entry('sandbox', 'files')] }).inspect, { machineLog: false, sandboxLog: true, files: true, machineFiles: false, machineStats: false })
  assert.deepEqual(featuresOf({ calls: [entry('machine', 'files'), entry('machine', 'log')] }).inspect, { machineLog: true, sandboxLog: false, files: false, machineFiles: true, machineStats: false })
  for (const name of ['machine.log', 'sandbox.log', 'sandbox.files', 'machine.files']) assert.ok(!R.cli.REQUIRED_CALLS.includes(name), `${name} is optional`)
  // the machines view carries them, and a machine's `supports` list says whether it has a log (2.0: `log` in the list)
  const f = fixture(viewLfs({ list: async () => ({ items: [
    { machine: 'native', platform: 'native', state: 'ready', supports: ['sqlite', 'postgresql_sandbox'] },
    { machine: 'docker', platform: 'docker', state: 'ready', supports: ['sqlite', 'postgresql_sandbox', 'log'] },
  ] }), get: async (name) => ({ machine: name, platform: name, state: 'ready', live: {} }) }), LOGS)
  try {
    const view = (await f.http('GET', '/api/machines')).body
    assert.deepEqual(view.features.inspect, { machineLog: true, sandboxLog: true, files: false, machineFiles: false, machineStats: false })
    assert.deepEqual(view.machines.map((m) => [m.name, m.supports.includes('log')]), [['native', false], ['docker', true]])
  } finally { f.close() }
})

test('log routes: a CLI without the call answers 501 and makes no call; bad queries are refused before any call', async () => {
  let calls = 0
  const lfs = { ...viewLfs({ log: async () => { calls++; return { items: [{ text: 'a' }], next_cursor: null } } }) }
  lfs.sandbox.log = async () => { calls++; return { items: [{ text: 'a' }], next_cursor: null } }
  const without = fixture(lfs)
  const withLogs = fixture(lfs, LOGS)
  try {
    for (const path of ['/api/machines/docker/log', '/api/sandboxes/sb/log']) {
      const r = await without.http('GET', path)
      assert.equal(r.status, 501)
      assert.equal(r.body.code, 'Unsupported')
      assert.match(r.body.detail.need, /^(machine|sandbox)\.log$/)
    }
    assert.equal(calls, 0)
    assert.deepEqual(without.logs, [], 'an expected refusal logs no stack')
    // a machine's log pages backwards only: a forward cursor is refused too
    for (const query of ['cursor=a', 'cursor=a&before=b', 'limit=0', 'limit=1001', 'limit=abc', 'limit=1.5', `before=${'x'.repeat(1025)}`]) {
      const r = await withLogs.http('GET', `/api/machines/docker/log?${query}`)
      assert.equal(r.status, 400, query)
      assert.equal(r.body.code, 'InvalidInput', query)
    }
    assert.equal(calls, 0)
    // only GET
    assert.equal((await withLogs.http('POST', '/api/machines/docker/log', {})).status, 404)
  } finally { without.close(); withLogs.close() }
})

test('log routes: the answer as the page reads it, with the options LayerFS takes; never part of the machines view', async () => {
  const seen = []
  const lfs = { ...viewLfs({
    log: async (name, options) => { seen.push(['machine', name, { ...options, signal: options.signal instanceof AbortSignal }]); return { items: [{ text: 'a', continued: false }, { text: 'b'.repeat(8), continued: true }, { text: 'c' }, 'bare', { nope: 1 }], next_cursor: 'b0' } },
  }) }
  lfs.sandbox.log = async (name, options) => { seen.push(['sandbox', name, { ...options, signal: options.signal instanceof AbortSignal }]); return { items: [], next_cursor: null } }
  const f = fixture(lfs, LOGS)
  try {
    const m = await f.http('GET', '/api/machines/docker/log?limit=50')
    assert.equal(m.status, 200)
    assert.deepEqual(m.body.lines, [{ text: 'a' }, { text: 'bbbbbbbb', continued: true }, { text: 'c' }], 'a row that is not `{ text }` is dropped')
    assert.deepEqual([m.body.cursor, m.body.before, m.body.hasMore, m.body.stale], [null, 'b0', true, false])
    assert.equal(typeof m.body.observedAt, 'number')
    // a Sandbox's log pages backwards too: `before` is the page's next_cursor; a forward cursor is refused
    const s = await f.http('GET', '/api/sandboxes/sb/log?before=c9')
    assert.deepEqual([s.body.lines, s.body.cursor, s.body.before, s.body.hasMore, s.body.stale], [[], null, null, false, false])
    assert.equal((await f.http('GET', '/api/sandboxes/sb/log?cursor=c9')).status, 400)
    await f.http('GET', '/api/machines/docker/log?before=b0&limit=1000')
    assert.deepEqual(seen, [
      ['machine', 'docker', { limit: 50, detail: true, signal: true }],
      ['sandbox', 'sb', { cursor: 'c9', detail: true, signal: true }],
      ['machine', 'docker', { cursor: 'b0', limit: 1000, detail: true, signal: true }],
    ])
    // the machines view makes no log call, and a log GET does not make the view read again
    seen.length = 0
    let lists = 0
    lfs.machine.list = async () => { lists++; return { items: [{ machine: 'native', platform: 'native', state: 'ready' }] } }
    const first = f.http('GET', '/api/machines')
    await f.http('GET', '/api/machines/docker/log')
    await Promise.all([first, f.http('GET', '/api/machines')])
    assert.equal(seen.length, 1)
  } finally { f.close() }
})

test('log routes: the kept tail is bounded, and a cut start has no `before`', async () => {
  const { InspectReads, KEEP_LINES } = R.inspect
  const reads = new InspectReads()
  // a 2.0 log page: `items` oldest first, `next_cursor` the lines before them
  const big = { items: Array.from({ length: KEEP_LINES + 50 }, (_, i) => ({ text: `l${i}` })), next_cursor: 'start' }
  let fail = false
  const call = async () => { if (fail) throw refusal('Unavailable'); return big }
  const none = { cursor: null, before: null, limit: null }
  assert.equal((await reads.log('machine:m', none, call)).lines.length, KEEP_LINES + 50, 'the answer itself is whole')
  fail = true
  const kept = await reads.log('machine:m', none, call)
  assert.equal(kept.lines.length, KEEP_LINES)
  assert.equal(kept.lines[0].text, 'l50')
  assert.equal(kept.before, null)
  assert.equal(kept.cursor, null, 'a 2.0 log has no forward cursor')
  assert.equal(kept.stale.code, 'Unavailable')
})

test('log routes: a page request can be cancelled by its rid; the read ends only when its last caller has gone', async () => {
  const signals = []
  let release
  const lfs = viewLfs()
  lfs.sandbox.log = (_name, options) => new Promise((resolve, reject) => {
    signals.push(options.signal)
    release = () => resolve({ items: [{ text: 'line 0' }], next_cursor: null })
    options.signal.addEventListener('abort', () => reject(Object.assign(new Error('dropped'), { code: 'Cancelled' })), { once: true })
  })
  const f = fixture(lfs, LOGS)
  const reads = f.groups.machines.reads
  try {
    // two pages wait for the same read; one leaves
    const a = f.http('GET', '/api/sandboxes/sb/log?rid=a')
    const b = f.http('GET', '/api/sandboxes/sb/log?rid=b')
    await tick()
    assert.equal(signals.length, 1)
    assert.equal(reads.cancel('a'), true)
    const gone = await a
    assert.equal(gone.status, 409)
    assert.equal(gone.body.code, 'Cancelled')
    assert.equal(signals[0].aborted, false, 'another page still waits: the read goes on')
    assert.equal(reads.cancel('a'), false, 'already gone')
    release()
    assert.equal((await b).body.stale, false)
    // the only waiter leaves: the read itself is dropped (the SDK then leaves the lane or ends the process)
    const c = f.http('GET', '/api/sandboxes/sb/log?rid=c')
    await tick()
    assert.equal(signals.length, 2)
    assert.equal(reads.cancel('c'), true)
    assert.equal((await c).body.code, 'Cancelled')
    assert.equal(signals[1].aborted, true)
    assert.equal(reads.flights, 0)
    // the connection that asked going away does the same (the http operation's own signal)
    const outer = new AbortController()
    const d = R.handle(f.groups, { method: 'GET', path: '/api/sandboxes/sb/log' }, outer.signal)
    await tick()
    outer.abort()
    assert.equal((await d).body.code, 'Cancelled')
    assert.equal(signals[2].aborted, true)
    assert.equal(reads.cancel(undefined), false)
    assert.equal(reads.cancel('never'), false)
  } finally { f.close() }
})
