// The `http` operation's router: dispatch, Connect's status mapping and
// bodies over a fake lfs, and a live pass over the real pinned CLI against a
// private temporary LAYERFS_HOME.
//   node --test test/routes.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync, existsSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { loadRoutes, pinned, testHost } from './routes-helper.mjs'

const R = await loadRoutes()

class Refusal extends Error {
  constructor(code, message, detail = null, transport = 'refusal') { super(message); Object.assign(this, { code, detail, transport }) }
}

/** A fake SDK: `calls` records every `family.call(args)`; `impl[family][call]` answers. */
function fakeLfs(impl) {
  const calls = []
  const lfs = new Proxy({ spec: { cli: '/fake/layerfs', home: '/fake/home' } }, {
    get(target, family) {
      if (family in target) return target[family]
      return new Proxy({}, {
        get(_t, call) {
          return async (...args) => {
            calls.push(`${family}.${call}`)
            const fn = impl[family]?.[call]
            if (!fn) throw new Refusal('Unsupported', `fake has no ${family}.${call}`)
            return fn(...args)
          }
        },
      })
    },
  })
  return { lfs, calls }
}

const discovery = async () => ({ identity: { version: '2.0.0-test', commit: 'a'.repeat(40), dirty: false, features: [] }, calls: [], commands: [], switches: [] })

function fakeGroups(impl, discover = discovery, secrets = undefined) {
  const root = mkdtempSync(join(tmpdir(), 'dsh-routes-root-'))
  const { lfs, calls } = fakeLfs(impl)
  const t = testHost({ lfs, root, home: join(root, 'home'), discovery: discover, ...(secrets ? { secrets: secrets(root) } : {}) })
  const groups = R.createGroups(t.host, async () => ({ ok: true, version: '2.0.0-test' }))
  return { groups, calls, events: t.events, root, http: (method, path, body) => R.handle(groups, { method, path, body }) }
}

const base = {
  machine: {
    list: async () => ({ machines: [{ name: 'native', platform: 'native', state: 'ready', stores: ['s1'] }, { name: 'lima1', platform: 'lima', state: 'stopped' }] }),
    platforms: async () => ({ host: 'macos', platforms: [{ platform: 'lima', ready: true, options: [{ name: 'cpus', kind: 'int' }, { name: 'nested', kind: 'bool' }] }, { platform: 'ssh', ready: true }] }),
    status: async (name) => ({ name, state: 'stopped', free_disk: 10 }),
  },
  store: {
    list: async () => ({ items: [{ name: 's1', machine: 'native', projects: ['p'], state: 'idle' }] }),
    status: async () => ({ state: 'idle', file: { exists: true, bytes: 100, wal_bytes: 5 }, holders: [] }),
    create: async () => ({}),
  },
  sandbox: { list: async () => ({ items: [], next_page_pos: null }) },
}

test('dispatch: machines view, platforms filter, unknown route 404', async () => {
  const { http, root } = fakeGroups(base)
  try {
    const m = await http('GET', '/api/machines')
    assert.equal(m.status, 200)
    assert.deepEqual(m.body.machines.map((x) => x.name), ['native', 'lima1'])
    assert.equal(m.body.layerfs, '2.0.0-test')
    assert.equal(m.body.status.lima1.freeDisk, 10)
    assert.deepEqual(m.body.stores.map((s) => s.name), ['s1'])
    const p = await http('GET', '/api/platforms')
    assert.deepEqual(p.body.platforms.map((x) => x.platform), ['lima'])
    assert.deepEqual(p.body.platforms[0].options.map((o) => o.name), ['cpus'])
    const nf = await http('GET', '/api/demos')
    assert.equal(nf.status, 404)
    assert.equal(nf.body.code, 'not-found')
  } finally { rmSync(root, { recursive: true, force: true }) }
})

test('status mapping: refusal codes, transport, detail, conversion', async () => {
  const { http, root } = fakeGroups({
    ...base,
    project: {
      get: async (key) => { throw new Refusal('NoSuchProject', `no project ${key}`, { entity: key }) },
      remove: async () => { throw new Refusal('InUse', 'converting', { related: [{ kind: 'sandbox', name: 'b', why: 'close it' }] }) },
    },
    store: { ...base.store, remove: async () => { throw new Refusal('Unknown', 'lost', null, 'transport-unknown') } },
    machine: { ...base.machine, stop: async () => { throw new Refusal('Busy', 'held', { op: { name: 'machine.start' } }) } },
  })
  try {
    const a = await http('GET', `/api/projects/${encodeURIComponent('s1/p')}`)
    assert.equal(a.status, 404)
    assert.deepEqual(a.body, { code: 'NoSuchProject', transport: 'refusal', message: 'no project s1/p', detail: { entity: 's1/p' } })
    const b = await http('DELETE', `/api/projects/${encodeURIComponent('s1/p')}`)
    assert.equal(b.status, 409)
    assert.equal(b.body.conversion, true)
    const c = await http('DELETE', '/api/stores/s1?confirm=s1')
    assert.equal(c.status, 500)
    assert.equal(c.body.transport, 'transport-unknown')
    const d = await http('POST', '/api/machines/lima1/stop')
    assert.equal(d.status, 409)
    assert.equal(d.body.code, 'Busy')
    // the host's own guards keep their explicit status
    const e = await http('DELETE', '/api/stores/s1?confirm=nope')
    assert.equal(e.status, 400)
    assert.equal(e.body.code, 'InvalidInput')
    const f = await http('GET', '/api/projects/bare')
    assert.equal(f.status, 400)
    const g = await http('POST', '/api/machines', { name: 'x', platform: 'ssh' })
    assert.equal(g.status, 501)
    const h = await http('POST', '/api/machines', { name: 'x', platform: 'lima', disk: 9 })
    assert.equal(h.status, 400)
    assert.match(h.body.message, /takes no disk/)
  } finally { rmSync(root, { recursive: true, force: true }) }
})

test('stores: create emits storage, answers the view; bigints are tagged i64-v1', async () => {
  const { http, events, calls, root } = fakeGroups({
    ...base,
    store: { ...base.store, stats: async () => ({ kind: 'postgresql', canonical: 2n ** 70n, store_id: 'id1', store_generation: 1 }), get: async () => ({ name: 's1' }) },
  })
  try {
    const r = await http('POST', '/api/stores', { name: 'new1' })
    assert.equal(r.status, 200)
    assert.ok(calls.includes('store.create'))
    assert.deepEqual(r.body.stores[0].size, { bytes: 105, file: 100, wal: 5 })
    assert.deepEqual(events.map((e) => e.type), ['storage'])
    assert.deepEqual(events[0].data, { type: 'storage' })
    const bad = await http('POST', '/api/stores', { name: 'Bad Name' })
    assert.equal(bad.status, 400)
    const s = await http('GET', '/api/stores/s1/stats')
    assert.deepEqual(s.body.canonical, { $layerfs: 'i64-v1', decimal: String(2n ** 70n) })
    const o = await http('GET', '/api/stores/s1/observations')
    assert.equal(o.body.series.length, 1)
  } finally { rmSync(root, { recursive: true, force: true }) }
})

test('postgresql: an entered password goes to an owner-only file; attach and detach; refusals kept, never echoed', async () => {
  const record = { name: 'pg', kind: 'postgresql', url: 'postgresql://lfs@db.lan:5432/team', password: 'env:PGPW', projects: ['app'], state: 'idle', machines: { native: { checked: { at: 1, server_version: '17.1', latency_ms: 2 } } } }
  const seen = []
  const withAttach = async () => ({ ...(await discovery()), calls: [{ family: 'store', call: 'attach', options: [] }] })
  const { http, root } = fakeGroups({
    ...base,
    store: {
      ...base.store,
      list: async () => ({ items: [record] }),
      status: async () => ({ state: 'idle', server: { reachable: true, server_version: '17.1', latency_ms: 2 } }),
      connect: async (name, url, options) => { seen.push(['connect', options.password]); if (url.includes('bad')) throw new Refusal('Denied', 'password authentication failed') },
      attach: async (name, machine, options) => {
        seen.push(['attach', machine, options])
        if (machine === 'dev') throw new Refusal('Unavailable', 'machine dev cannot reach PostgreSQL Store pg\'s server at db.lan: refused')
        record.machines[machine] = { ...(options.url ? { url: options.url } : {}), ...(options.password ? { password: options.password } : {}), checked: { at: 2, server_version: '17.1', latency_ms: 4 } }
        return record
      },
      detach: async (name, machine) => { seen.push(['detach', machine]); delete record.machines[machine]; return record },
      disconnect: async () => ({ disconnected: 'pg' }),
    },
  }, withAttach)
  const secrets = join(root, 'secrets')
  const files = () => (existsSync(secrets) ? readdirSync(secrets).sort() : [])
  try {
    // connect with an entered password: a 0600 file, passed as file:, removed on refusal
    const bad = await http('POST', '/api/stores/postgresql', { name: 'pg2', url: 'postgresql://lfs@bad/x', passwordValue: 's3cret' })
    assert.equal(bad.status, 403)
    assert.ok(seen[0][1].startsWith(`file:${secrets}/pg-pg2--`))
    assert.deepEqual(files(), [])
    const ok = await http('POST', '/api/stores/postgresql', { name: 'pg2', url: 'postgresql://lfs@db/x', passwordValue: 's3cret' })
    assert.equal(ok.status, 200)
    assert.equal(files().length, 1)
    assert.equal(statSync(join(secrets, files()[0])).mode & 0o777, 0o600)
    assert.equal(readFileSync(join(secrets, files()[0]), 'utf8'), 's3cret')
    assert.ok(!JSON.stringify(ok.body).includes('s3cret'))
    // the view: attachments and the feature
    assert.equal(ok.body.features.attach, true)
    assert.deepEqual(ok.body.stores[0].attachments, [{ machine: 'native', own: false, url: null, target: 'lfs@db.lan:5432/team', password: null, saved: false, checked: { at: 1, version: '17.1', latencyMs: 2 }, lastError: null }])
    // attach with the Store's connection
    const docker = await http('POST', '/api/stores/pg/attach', { machine: 'docker' })
    assert.equal(docker.status, 200)
    assert.deepEqual(seen.at(-1), ['attach', 'docker', {}])
    assert.deepEqual(docker.body.stores[0].attachments.map((a) => a.machine), ['native', 'docker'])
    // an own connection with an entered password: its own file, the record shows it as saved
    const vm = await http('POST', '/api/stores/pg/attach', { machine: 'vm1', url: 'postgresql://vm@10.0.0.5/team', passwordValue: 'vmpw' })
    assert.equal(vm.status, 200)
    const vmFile = files().find((f) => f.startsWith('pg-pg--vm1--'))
    assert.ok(vmFile)
    const row = vm.body.stores[0].attachments.find((a) => a.machine === 'vm1')
    assert.deepEqual([row.own, row.target, row.saved, row.password], [true, 'vm@10.0.0.5/team', true, `file:${join(secrets, vmFile)}`])
    // a second attach of the same machine replaces its file
    await http('POST', '/api/stores/pg/attach', { machine: 'vm1', url: 'postgresql://vm@10.0.0.5/team', passwordValue: 'vmpw2' })
    assert.equal(files().filter((f) => f.startsWith('pg-pg--vm1--')).length, 1)
    assert.notEqual(files().find((f) => f.startsWith('pg-pg--vm1--')), vmFile)
    // a refusal: LayerFS's code, kept for the view, its file removed
    const refused = await http('POST', '/api/stores/pg/attach', { machine: 'dev', passwordValue: 'devpw' })
    assert.equal(refused.status, 502)
    assert.equal(refused.body.code ?? refused.body.error?.code, 'Unavailable')
    assert.ok(!files().some((f) => f.startsWith('pg-pg--dev--')))
    const view = await http('GET', '/api/storage')
    assert.ok(!JSON.stringify(view.body).includes('devpw'))
    assert.deepEqual(view.body.stores[0].attachRefusals.map((e) => [e.machine, e.code]), [['dev', 'Unavailable']])
    // a password in the URL, a bad reference: refused before LayerFS
    assert.equal((await http('POST', '/api/stores/pg/attach', { machine: 'x1', url: 'postgresql://a:b@h/d' })).status, 400)
    assert.equal((await http('POST', '/api/stores/pg/attach', { machine: 'x1', password: 'plain' })).status, 400)
    // detach removes that machine's files
    const detached = await http('POST', '/api/stores/pg/detach', { machine: 'vm1' })
    assert.equal(detached.status, 200)
    assert.ok(!files().some((f) => f.startsWith('pg-pg--vm1--')))
    assert.deepEqual(seen.at(-1), ['detach', 'vm1'])
  } finally { rmSync(root, { recursive: true, force: true }) }
})

test('buckets: entered keys go to a configured secrets folder; replace and disconnect use the recorded file', async () => {
  const seen = []
  const { http, root } = fakeGroups({
    ...base,
    backup: {
      connect: async (name, url, options) => { seen.push(['connect', name, url, options.key]); return { url, region: 'auto' } },
      list: async () => ({ items: [] }),
      disconnect: async () => ({ unlinked: [] }),
    },
  }, discovery, (root) => join(root, 'keys'))
  const keys = join(root, 'keys')
  try {
    const ok = await http('POST', '/api/buckets', { name: 'r2b', provider: 'r2', account: 'a'.repeat(32), bucket: 'bk', accessKey: 'AK', secretKey: 'SK' })
    assert.equal(ok.status, 200)
    const file = join(keys, 'bucket-r2b.key')
    assert.deepEqual(seen[0], ['connect', 'r2b', `r2://${'a'.repeat(32)}/bk`, `file:${file}`])
    assert.equal(statSync(keys).mode & 0o777, 0o700)
    assert.equal(statSync(file).mode & 0o777, 0o600)
    assert.equal(readFileSync(file, 'utf8'), 'access_key=AK\nsecret_key=SK\n')
    assert.ok(!existsSync(join(root, 'secrets')) && !existsSync(join(root, 'home')))
    // the keys never reach the records or the view
    assert.ok(!readFileSync(join(root, 'buckets.json'), 'utf8').includes('SK'))
    assert.ok(!JSON.stringify(ok.body).includes('SK'))
    const replaced = await http('POST', '/api/buckets/r2b/keys', { accessKey: 'AK2', secretKey: 'SK2' })
    assert.equal(replaced.status, 200)
    assert.equal(readFileSync(file, 'utf8'), 'access_key=AK2\nsecret_key=SK2\n')
    const gone = await http('DELETE', '/api/buckets/r2b')
    assert.equal(gone.status, 200)
    assert.ok(!existsSync(file))
  } finally { rmSync(root, { recursive: true, force: true }) }
})

test('buckets: by default entered keys go to <home>/secret; a home made for them is owner-only', async () => {
  const { http, root } = fakeGroups({
    ...base,
    backup: { connect: async (name, url) => ({ url, region: 'auto' }) },
  }, discovery, (root) => join(root, 'home', 'secret'))
  try {
    const ok = await http('POST', '/api/buckets', { name: 'b', provider: 'minio', endpoint: 'http://minio:9000', bucket: 'bk', accessKey: 'AK', secretKey: 'SK' })
    assert.equal(ok.status, 200)
    assert.equal(statSync(join(root, 'home')).mode & 0o777, 0o700)
    assert.equal(statSync(join(root, 'home', 'secret', 'bucket-b.key')).mode & 0o777, 0o600)
  } finally { rmSync(root, { recursive: true, force: true }) }
})

test('postgresql: without store.attach in the pinned CLI attach is refused and the view says so', async () => {
  const { http, root } = fakeGroups({ ...base, store: { ...base.store, list: async () => ({ items: [{ name: 'pg', kind: 'postgresql', url: 'postgresql://l@h/d', state: 'idle' }] }) } })
  try {
    const v = await http('GET', '/api/storage')
    assert.equal(v.body.features.attach, false)
    assert.equal(v.body.stores[0].attachments, null)
    const r = await http('POST', '/api/stores/pg/attach', { machine: 'docker' })
    assert.equal(r.status, 400)
  } finally { rmSync(root, { recursive: true, force: true }) }
})

test('storage jobs: push runs as a job with storage-job events; one job per Store', async () => {
  let finish
  const { http, events, root } = fakeGroups({
    ...base,
    store: { ...base.store, push: (store, machine, { on_progress }) => new Promise((resolve) => { on_progress({ phase: 'send', done: 1, total: 2 }); finish = resolve }) },
  })
  try {
    const j = await http('POST', '/api/stores/s1/push', { machine: 'lima1' })
    assert.equal(j.status, 200)
    assert.equal(j.body.kind, 'push')
    const again = await http('POST', '/api/stores/s1/push', { machine: 'lima1' })
    assert.equal(again.status, 409)
    assert.equal(again.body.code, 'ConnectBusy')
    await new Promise((r) => setTimeout(r, 10))
    finish({ ok: true })
    await new Promise((r) => setTimeout(r, 10))
    const done = await http('GET', `/api/storage-jobs/${j.body.id}`)
    assert.equal(done.body.outcome, 'succeeded')
    const jobEvents = events.filter((e) => e.type === 'storage-job')
    assert.ok(jobEvents.length >= 2)
    assert.equal(jobEvents.at(-1).data.type, 'storage-job')
    assert.ok(jobEvents.at(-1).data.job.ended)
    assert.ok(existsSync(join(root, 'storage-jobs.json')))
    const view = await http('GET', '/api/storage')
    assert.equal(view.body.jobs[0].id, j.body.id)
  } finally { rmSync(root, { recursive: true, force: true }) }
})

test('project init job and saved reads route to their calls', async () => {
  const head = 'c'.repeat(66)
  const src = mkdtempSync(join(tmpdir(), 'dsh-routes-src-'))
  const { http, root } = fakeGroups({
    ...base,
    project: { init: async (key) => ({ project: key, name: 'p', store: 's1', machine: 'native' }), list: async () => ({ items: [{ project: 's1/p', name: 'p', store: 's1' }], next_page_pos: null }) },
    branch: { get: async () => ({ name: 'main', base: head }), list: async () => ({ items: [{ name: 'main', base: head }], next_page_pos: null }) },
    file: {
      list: async (project, branch, path, at) => ({ items: [{ name: 'a.txt', path: '/a.txt', type: 'file', size: 3 }], has_more: false, at: { id: at } }),
      stat: async () => ({ size: 3, mode: 0o644 }),
      read: async () => ({ text: 'hi\n', next_line: null }),
    },
  })
  try {
    const job = await http('POST', '/api/projects', { store: 's1', name: 'p', source: src })
    assert.equal(job.status, 200)
    assert.equal(job.body.name, 's1/p')
    await new Promise((r) => setTimeout(r, 10))
    const ended = await http('GET', `/api/project-inits/${job.body.id}`)
    assert.equal(ended.body.step, 'done')
    assert.equal(ended.body.project.key, 's1/p')
    const list = await http('GET', '/api/projects')
    assert.deepEqual(list.body.map((p) => [p.key, p.storeState]), [['s1/p', 'idle']])
    const key = encodeURIComponent('s1/p')
    const fs = await http('GET', `/api/projects/${key}/fs?branch=main&path=/`)
    assert.deepEqual(fs.body, { items: [{ name: 'a.txt', path: '/a.txt', kind: 'file', size: 3, mode: null }], next: null, at: { id: head } })
    const file = await http('GET', `/api/projects/${key}/file?branch=main&path=/a.txt`)
    assert.equal(file.body.text, 'hi\n')
    assert.equal(file.body.binary, false)
    const bad = await http('GET', `/api/projects/${key}/fs?branch=main&path=../x`)
    assert.equal(bad.status, 400)
    const br = await http('GET', `/api/projects/${key}/branches`)
    assert.deepEqual(br.body.items[0], { name: 'main', head, parent: null, forkedFrom: null, state: 'open', commits: null })
  } finally { rmSync(root, { recursive: true, force: true }); rmSync(src, { recursive: true, force: true }) }
})

// ---------------------------------------------------------------- live

const pin = pinned()

test('live: machines, store create, project init, list/get, branches, fs, file', { skip: !pin && 'no pinned LayerFS CLI', timeout: 600_000 }, async () => {
  // a short path: the native daemon's sockets live in the home
  const root = mkdtempSync('/tmp/dshrt-')
  const home = join(root, 'home')
  mkdirSync(home, { mode: 0o700 })
  const src = join(root, 'src')
  mkdirSync(join(src, 'lib'), { recursive: true })
  writeFileSync(join(src, 'README.md'), '# hello\nworld\n')
  writeFileSync(join(src, 'lib', 'a.js'), 'export const a = 1\n')
  const lfs = R.cli.client({ cli: pin.cli, home })
  let recorded
  const t = testHost({
    lfs, root, home, cli: pin.cli, daemon: pin.daemon ?? null,
    discovery: () => R.cli.discover({ cli: pin.cli }),
    ensureNative: () => recorded ??= (process.platform === 'darwin' && pin.daemon ? lfs.machine.update('native', { daemon: pin.daemon }) : Promise.resolve()),
  })
  const groups = R.createGroups(t.host, async () => ({ ok: true }))
  const http = async (method, path, body) => {
    const r = await R.handle(groups, { method, path, body })
    return r
  }
  const ok = async (method, path, body) => {
    const r = await http(method, path, body)
    assert.equal(r.status, 200, `${method} ${path}: ${JSON.stringify(r.body)}`)
    return r.body
  }
  const key = encodeURIComponent('rt/demo')
  try {
    const machines = await ok('GET', '/api/machines')
    assert.ok(machines.machines.some((m) => m.name === 'native'))
    const status = await ok('GET', '/api/status')
    assert.equal(status.settings.home, home)
    const view = await ok('POST', '/api/stores', { name: 'rt' })
    assert.ok(view.stores.some((s) => s.name === 'rt' && s.machine === 'native'))
    assert.ok(t.events.some((e) => e.type === 'storage'))
    const job = await ok('POST', '/api/projects', { store: 'rt', name: 'demo', source: src })
    let init = job
    for (let i = 0; i < 600 && !init.ended; i++) {
      await new Promise((r) => setTimeout(r, 200))
      init = await ok('GET', `/api/project-inits/${job.id}`)
    }
    assert.equal(init.step, 'done', JSON.stringify(init.error))
    const list = await ok('GET', '/api/projects')
    assert.ok(list.some((p) => p.key === 'rt/demo' && p.storeState))
    const state = await ok('GET', `/api/projects/${key}`)
    assert.equal(state.project.key, 'rt/demo')
    assert.ok(state.branches.some((b) => b.name === 'main'))
    const branches = await ok('GET', `/api/projects/${key}/branches`)
    assert.ok(branches.items.some((b) => b.name === 'main'))
    const fs = await ok('GET', `/api/projects/${key}/fs?branch=main&path=/`)
    assert.deepEqual(fs.items.map((i) => i.name).sort(), ['README.md', 'lib'])
    assert.equal(fs.items.find((i) => i.name === 'lib').kind, 'dir')
    const sub = await ok('GET', `/api/projects/${key}/fs?branch=main&path=/lib`)
    assert.deepEqual(sub.items.map((i) => i.path), ['/lib/a.js'])
    const file = await ok('GET', `/api/projects/${key}/file?branch=main&path=/README.md`)
    assert.equal(file.binary, false)
    assert.match(file.text, /hello/)
    const missing = await http('GET', `/api/projects/${key}/file?branch=main&path=/nope.txt`)
    assert.equal(missing.status, 404, JSON.stringify(missing.body))
    const sandboxes = await ok('GET', `/api/projects/${key}/sandboxes`)
    assert.ok(Array.isArray(sandboxes))
    const storage = await ok('GET', '/api/storage')
    assert.ok(storage.stores.some((s) => s.name === 'rt' && s.projects.includes('demo')))
    // a Sandbox, a forked side Branch mounted in it, its status, unmount and close
    await ok('POST', `/api/projects/${key}/sandboxes`, { name: 'rtsb' })
    const ws = await ok('POST', '/api/sandboxes/rtsb/workspaces', { branch: 'side', fork: true, name: 'w' })
    assert.equal(ws.branch ?? 'side', 'side')
    const sb = await ok('GET', '/api/sandboxes/rtsb')
    assert.equal(sb.workspaces.length, 1)
    const handle = sb.workspaces[0].ws
    await ok('GET', `/api/workspaces/${encodeURIComponent(handle)}/status`)
    const main = await http('POST', '/api/sandboxes/rtsb/workspaces', { branch: 'main' })
    assert.equal(main.status, 400)
    await ok('DELETE', `/api/workspaces/${encodeURIComponent(handle)}?discard=1`)
    await ok('DELETE', '/api/sandboxes/rtsb')
    const gone = await http('GET', '/api/sandboxes/rtsb')
    assert.equal(gone.status, 404, JSON.stringify(gone.body))
  } finally {
    await http('POST', `/api/projects/${key}/close-sandboxes`, { force: true }).catch(() => {})
    await groups.projects.close()
    rmSync(root, { recursive: true, force: true })
  }
})

test('project storage: a ledger of each save\'s counters, read once, then only new saves', async () => {
  const id = (c, n) => `12${String(n).padStart(2, '0')}${c.repeat(62)}`
  const layer = (n) => `32${String(n).padStart(2, '0')}${'f'.repeat(62)}`
  const commit = (branch, n, extra = {}) => ({ id: id(branch[0], n), ref: { id: id(branch[0], n), kind: 'Commit', branch }, time: 1000 + n, via: 'fs', logical: 100, ...extra })
  const history = {
    a: [commit('a', 3), commit('a', 2, { via: 'merge' }), commit('a', 1)],
    b: [commit('b', 2), commit('b', 1)],
    main: [{ id: layer(2), ref: { id: layer(2), kind: 'Layer', branch: 'main' }, logical: 40 }, { id: layer(1), ref: { id: layer(1), kind: 'Layer', branch: 'main' }, logical: 30 }],
  }
  const read = []
  const impl = {
    ...base,
    project: { get: async (key) => ({ project: key, name: 'p', store: 's1', machine: 'native', created: 7 }), stats: async () => ({ logical: 100, files: 3, directories: 1, complete: true }) },
    store: { ...base.store, stats: async () => ({ name: 's1', history_logical: 1000, canonical: 400, stored: 100, metadata: 50, file: 4096 }), get: async () => ({ name: 's1', projects: ['p', 'q'] }) },
    branch: {
      list: async () => ({ items: [{ name: 'main', base: history.main[0].id }, { name: 'a', head: history.a[0].id }, { name: 'b', head: history.b[0].id }], next_page_pos: null }),
      commits: async (_project, branch, options) => {
        const all = history[branch], from = all.findIndex((c) => c.id === options.at_commit)
        const rows = all.slice(from < 0 ? 0 : from), start = (options.page - 1) * options.page_size
        return { items: rows.slice(start, start + options.page_size), has_more: start + options.page_size < rows.length }
      },
      get_commit: async (_project, branch, commitId) => {
        read.push(commitId)
        const c = history[branch].find((x) => x.id === commitId)
        return { ...c, message: `save ${commitId.slice(2, 4)}`, new_canonical: 20, new_stored: 10, new_metadata: 4 }
      },
    },
  }
  const { http, groups, root } = fakeGroups(impl)
  const path = `/api/projects/${encodeURIComponent('s1/p')}/storage`
  try {
    const first = await http('GET', path)
    assert.equal(first.status, 200, JSON.stringify(first.body))
    assert.equal(first.body.totals, null, 'nothing counted before the first read finishes')
    assert.ok(first.body.building, 'the first read runs in the background')
    await groups.projects.storageLedgers.settle('s1/p', 7)
    const done = await http('GET', path)
    assert.equal(done.body.building, null)
    assert.deepEqual(done.body.totals, { saves: 5, merges: 1, branches: 2, publications: 2, commitLogical: 500, publicationLogical: 70, canonical: 100, stored: 50, metadata: 20 })
    assert.equal(done.body.recent.length, 5)
    assert.equal(done.body.recent.at(-1).id, history.a[0].id, 'newest save last')
    assert.equal(done.body.growth.at(-1), 50)
    assert.deepEqual({ name: done.body.store.name, projects: done.body.store.projects, stored: done.body.store.stored, file: done.body.store.file }, { name: 's1', projects: 2, stored: 100, file: 4096 })
    assert.deepEqual(done.body.main, { logical: 100, files: 3, directories: 1, complete: true })
    assert.equal(read.length, 5)
    // a new save on b: a recount reads only it
    history.b.unshift(commit('b', 3))
    read.length = 0
    await http('GET', `${path}?refresh=1`)
    await groups.projects.storageLedgers.settle('s1/p', 7)
    const again = await http('GET', path)
    assert.deepEqual(read, [history.b[0].id])
    assert.equal(again.body.totals.saves, 6)
    assert.equal(again.body.totals.stored, 60)
  } finally {
    await groups.projects.close()
    rmSync(root, { recursive: true, force: true })
  }
})
