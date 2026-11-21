// The `http` operation's router: dispatch, Connect's status mapping and
// bodies over a fake lfs, and a live pass over the real pinned CLI against a
// private temporary LAYERFS_HOME.
//   node --test test/routes.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync, existsSync } from 'node:fs'
import { execFileSync } from 'node:child_process'
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
  // the 2.0 machine family (long forms): records from machine.list, the live probe from machine.get --with live
  machine: {
    list: async () => ({ items: [{ machine: 'native', platform: 'native', state: 'ready', stores: ['s1'], sandboxes: [], supports: ['sqlite'] }, { machine: 'lima1', platform: 'lima', state: 'stopped', stores: [], sandboxes: [], supports: ['sqlite'] }] }),
    get: async (name) => ({ machine: name, platform: name === 'native' ? 'native' : 'lima', state: name === 'native' ? 'ready' : 'stopped', live: { disk_free_bytes: 10, tool_version: null, worker: null } }),
  },
  home: {
    get: async () => ({ state: 'ready', version: '2.0.0', platforms: { host: 'macos', items: [{ platform: 'lima', how: 'created', ready: true, options: [{ option: 'cpus', kind: 'int' }, { option: 'nested', kind: 'bool' }] }, { platform: 'ssh', how: 'created', ready: true, options: [] }] } }),
  },
  // the 2.0 store family: `store list --detail [--with stats]` rows keyed `store` (a ready row may leave `state` out)
  store: {
    list: async () => ({ items: [{ store: 's1', machine: 'native', engine: 'sqlite', projects: ['p'], state: 'ready', size: 105 }] }),
    get: async (name) => ({ store: name, machine: 'native', engine: 'sqlite', projects: ['p'], state: 'ready' }),
    create: async () => ({ ok: true }),
  },
  sandbox: { list: async () => ({ items: [] }) },
}

test('dispatch: machines view, platforms filter, unknown route 404', async () => {
  const { http, root } = fakeGroups(base)
  try {
    const m = await http('GET', '/api/machines')
    assert.equal(m.status, 200)
    assert.deepEqual(m.body.machines.map((x) => x.name), ['native', 'lima1'])
    assert.equal(m.body.layerfs, '2.0.0-test')
    assert.equal(m.body.live.lima1.freeDisk, 10)
    assert.deepEqual(m.body.stores.map((s) => s.name), ['s1'])
    const p = await http('GET', '/api/platforms')
    assert.deepEqual(p.body.platforms.map((x) => x.platform), ['lima'])
    assert.deepEqual(p.body.platforms[0].options.map((o) => o.option), ['cpus'])
    const nf = await http('GET', '/api/demos')
    assert.equal(nf.status, 404)
    assert.equal(nf.body.code, 'NotFound')
  } finally { rmSync(root, { recursive: true, force: true }) }
})

test('status mapping: refusal codes, transport, detail, conversion', async () => {
  const { http, root } = fakeGroups({
    ...base,
    project: {
      get: async (key) => { throw new Refusal('NotFound', `no project ${key}`, { entity: key }) },
      delete: async () => { throw new Refusal('InUse', 'converting', { related: [{ kind: 'sandbox', name: 'b', why: 'close it' }] }) },
    },
    store: { ...base.store, remove: async () => { throw new Refusal('Unknown', 'lost', null, 'transport-unknown') } },
    machine: { ...base.machine, stop: async () => { throw new Refusal('Busy', 'held', { op: { name: 'machine.start' } }) } },
  })
  try {
    const a = await http('GET', `/api/projects/${encodeURIComponent('s1/p')}`)
    assert.equal(a.status, 404)
    assert.deepEqual(a.body, { code: 'NotFound', transport: 'refusal', message: 'no project s1/p', detail: { entity: 's1/p' } })
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
    // 2.0 `store get --with stats`: the section under `stats`; no Store identity (no observation journal)
    store: { ...base.store, get: async (name) => ({ store: name, engine: 'postgresql', stats: { canonical_bytes: 2n ** 70n, size: 105 } }) },
  })
  try {
    const r = await http('POST', '/api/stores', { name: 'new1' })
    assert.equal(r.status, 200)
    assert.ok(calls.includes('store.create'))
    assert.equal(r.body.stores[0].size.bytes, 105, 'the size from `store list --with stats`')
    assert.deepEqual(events.map((e) => e.type), ['storage'])
    assert.deepEqual(events[0].data, { type: 'storage' })
    const bad = await http('POST', '/api/stores', { name: 'Bad Name' })
    assert.equal(bad.status, 400)
    const s = await http('GET', '/api/stores/s1/stats')
    assert.deepEqual(s.body.canonical_bytes, { $layerfs: 'i64-v1', decimal: String(2n ** 70n) })
    assert.match(s.body.observationError, /Store identity/, 'no identity: no observation is kept, and the answer says why')
    const o = await http('GET', '/api/stores/s1/observations')
    assert.equal(o.body.series.length, 0)
  } finally { rmSync(root, { recursive: true, force: true }) }
})

test('storage view: a Store with more than 20 projects lists every name by project list --store (its `projects` is cut at 20)', async () => {
  const names = Array.from({ length: 25 }, (_, i) => `p${String(i).padStart(2, '0')}`)
  const asked = []
  const { http, root } = fakeGroups({
    ...base,
    store: { ...base.store, list: async () => ({ items: [{ store: 's1', machine: 'native', engine: 'sqlite', projects: names.slice(0, 20), projects_count: 25, state: 'ready', size: 1 }, { store: 's2', machine: 'native', engine: 'sqlite', projects: ['a'], state: 'ready', size: 1 }] }) },
    project: {
      list: async (o) => {
        asked.push(o.store)
        const from = o.cursor ? Number(o.cursor) : 0
        return { items: names.slice(from, from + 10).map((n) => ({ project: `s1/${n}` })), next_cursor: from + 10 < names.length ? String(from + 10) : null }
      },
    },
  })
  try {
    const view = await http('GET', '/api/storage')
    assert.equal(view.status, 200, JSON.stringify(view.body))
    const s1 = view.body.stores.find((s) => s.name === 's1'), s2 = view.body.stores.find((s) => s.name === 's2')
    assert.deepEqual(s1.projects, names)
    assert.equal(s1.projectsCount, 25)
    assert.deepEqual(s2.projects, ['a'], 'a whole list is not read again')
    assert.deepEqual(asked, ['s1', 's1', 's1'], 'only the cut Store, three pages')
  } finally { rmSync(root, { recursive: true, force: true }) }
})

test('storage view distinguishes a refused bucket inventory from confirmed removal', async () => {
  let buckets = [{ bucket: 'unlinked', provider: 'minio' }]
  let offline = false
  const { http, root } = fakeGroups({
    ...base,
    home: { get: async () => {
      if (offline) throw new Refusal('Busy', 'home tree is busy')
      return { tree: { buckets } }
    } },
  })
  try {
    const initial = await http('GET', '/api/storage')
    assert.equal(initial.body.bucketInventoryComplete, true)
    assert.deepEqual(initial.body.buckets.map((bucket) => bucket.name), ['unlinked'])

    offline = true
    const refused = await http('GET', '/api/storage')
    assert.equal(refused.status, 200)
    assert.equal(refused.body.bucketInventoryComplete, false)
    assert.deepEqual(refused.body.buckets, [])

    offline = false
    buckets = []
    const removed = await http('GET', '/api/storage')
    assert.equal(removed.body.bucketInventoryComplete, true)
    assert.deepEqual(removed.body.buckets, [])
  } finally { rmSync(root, { recursive: true, force: true }) }
})

test('project catalog Refresh rereads the pinned CLI', async () => {
  const requested = []
  const { http, root } = fakeGroups({
    ...base,
    project: { list: async (options) => { requested.push(options); return { items: [] } } },
  })
  try {
    assert.equal((await http('GET', '/api/projects')).status, 200)
    assert.equal((await http('GET', '/api/projects?fresh=1')).status, 200)
    assert.deepEqual(requested.map((options) => options.timeout_ms), [undefined, R.cli.READ_TIMEOUT_MS])
  } finally { rmSync(root, { recursive: true, force: true }) }
})

test('the Doctor page\'s backup fix runs as a Storage job (job row, storage-job events, one job per Store) and waits for its end (review UR M7)', async () => {
  let finish
  const { groups, events, root } = fakeGroups({
    ...base,
    backup: { create: (store, bucket) => new Promise((resolve, reject) => { finish = { resolve, reject } }) },
  })
  try {
    // the Doctor's fix action as createGroups wires it
    const run = groups.checks.actions.backupCreate('s1', 'r2')
    let settled = false
    void run.then(() => { settled = true }, () => { settled = true })
    await new Promise((r) => setTimeout(r, 10))
    assert.equal(settled, false, 'it waits for the backup to end')
    const jobs = events.filter((e) => e.type === 'storage-job').map((e) => e.data.job)
    assert.ok(jobs.some((j) => j.kind === 'backup' && j.store === 's1' && j.target === 'r2' && !j.ended), 'a running backup job on the Storage page')
    await assert.rejects(groups.checks.actions.backupCreate('s1', 'r2'), (e) => e.code === 'ConnectBusy', 'one job per Store')
    finish.resolve({ store: 's1', bucket: 'r2' })
    assert.deepEqual(await run, { store: 's1', bucket: 'r2' })
    await new Promise((r) => setTimeout(r, 10))
    assert.ok(events.some((e) => e.type === 'storage-job' && e.data.job.kind === 'backup' && e.data.job.outcome === 'succeeded'))
  } finally { rmSync(root, { recursive: true, force: true }) }
})

test('store projects: every project with its ledger, newest Commit and main files; one tree walk per main head', async () => {
  let head = 'h1'
  const { http, calls, groups, root } = fakeGroups({
    ...base,
    // 2.0 long forms: `project list --store` by cursor (every project, not `store get`'s first 20), `project stats` file_count/partial, `branch get` commit
    store: { ...base.store, get: async () => ({ store: 's1', projects: ['web'], projects_count: 2 }) },
    project: {
      list: async (o) => {
        assert.equal(o.store, 's1')
        const all = [{ project: 's1/web', store: 's1', created_at: new Date(1000).toISOString() }, { project: 's1/docs', store: 's1', created_at: new Date(1000).toISOString() }]
        return o.cursor ? { items: [all[1]], next_cursor: null } : { items: [all[0]], next_cursor: 'c1' }
      },
      stats: async (key) => ({ file_count: key === 's1/web' ? 40 : 10001, partial: key !== 's1/web' }),
    },
    branch: {
      get: async () => ({ commit: head }),
      list: async () => ({ items: [] }),
    },
    commit: { list: async () => ({ items: [] }) },
  })
  try {
    const a = await http('GET', '/api/stores/s1/projects')
    assert.equal(a.status, 200)
    assert.equal(a.body.store, 's1')
    assert.deepEqual(a.body.projects.map((p) => p.name), ['docs', 'web'], 'both pages, sorted')
    assert.deepEqual(a.body.projects.map((p) => p.files), [{ files: 10001, complete: false }, { files: 40, complete: true }])
    assert.equal(a.body.projects[0].created, 1000)
    await http('GET', '/api/stores/s1/projects')
    assert.equal(calls.filter((c) => c === 'project.stats').length, 2, 'an unchanged head walks no tree again')
    head = 'h2'
    await http('GET', '/api/stores/s1/projects')
    assert.equal(calls.filter((c) => c === 'project.stats').length, 4, 'a new head is walked once per project')
    await groups.projects.close()
  } finally { rmSync(root, { recursive: true, force: true }) }
})

const pgMachines = async () => ({ items: [
  { machine: 'native', platform: 'native', state: 'ready', supports: ['sqlite', 'postgresql_sandbox'] },
  { machine: 'docker', platform: 'docker', state: 'ready', supports: ['sqlite', 'postgresql_sandbox'] },
  { machine: 'vm1', platform: 'lima', state: 'stopped', supports: ['sqlite', 'postgresql_sandbox'] },
  { machine: 'nopg', platform: 'x', state: 'ready', supports: ['sqlite'] },
] })

test('postgresql: logins are saved in LayerFS (password on stdin, never a file or a URL); store.update edits the Store; a machine\'s own address is URL-only', async () => {
  // 2.0 `store get --detail`: `auth` (the saved login's name), `connections` (the Store's address, and machines' own)
  const record = { store: 'pg', engine: 'postgresql', auth: 'pg', connections: [{ url: 'postgresql://db.lan:5432/team' }], synchronous_commit: 'local', projects: ['app'], state: 'ready' }
  const seen = []
  const { http, root } = fakeGroups({
    ...base,
    machine: { ...base.machine, list: pgMachines },
    store: {
      ...base.store,
      list: async () => ({ items: [record] }),
      get: async () => record,
      save_auth: async (auth, options) => { seen.push(['save_auth', auth, options]); return { ok: true } },
      auths: async () => ({ items: [{ auth: 'pg', user: 'saved-user', stores: ['pg'] }] }),
      connect: async (name, url, options) => {
        seen.push(['connect', name, url, options])
        if (url.includes('bad')) throw new Refusal('Denied', 'password authentication failed for user "lfs"')
        if (options.machine) record.connections = [...record.connections.filter((c) => c.machine !== options.machine), { machine: options.machine, url, own: true }]
        return { ok: true }
      },
      update: async (name, options) => { seen.push(['update', options]); if (options.synchronous_commit) record.synchronous_commit = options.synchronous_commit; return { ok: true } },
      disconnect: async (name, options) => { seen.push(['disconnect', options ?? {}]); record.connections = record.connections.filter((c) => c.machine !== options?.machine); return { ok: true } },
    },
  })
  const nothingWritten = () => !existsSync(join(root, 'secrets')) || readdirSync(join(root, 'secrets')).length === 0
  try {
    // connect with a typed login: saved first (`store save-auth`, the password goes on stdin), then connect --auth; a refusal is LayerFS's code
    const bad = await http('POST', '/api/stores/postgresql', { name: 'pg2', url: 'postgresql://bad/x', user: 'lfs', passwordValue: 's3cret' })
    assert.equal(bad.status, 403)
    // a typed login is named after its server (spec v23 V23-12), never the Store
    assert.deepEqual(seen.slice(0, 2), [['save_auth', 'pg-bad-5432', { user: 'lfs', password: 's3cret' }], ['connect', 'pg2', 'postgresql://bad/x', { auth: 'pg-bad-5432' }]])
    assert.ok(!JSON.stringify(bad.body).includes('s3cret'))
    seen.length = 0
    const ok = await http('POST', '/api/stores/postgresql', { name: 'pg2', url: 'postgresql://db/x', auth: 'pg', synchronousCommit: 'off' })
    assert.equal(ok.status, 200)
    assert.deepEqual(seen, [['connect', 'pg2', 'postgresql://db/x', { auth: 'pg', synchronous_commit: 'off' }]], 'a saved login is named, nothing saved again')
    assert.ok(nothingWritten(), 'no password file on this host')
    // the view: the saved login's name, the address without a user, no machines of its own
    const row = ok.body.stores[0]
    assert.deepEqual([row.postgresql.auth, row.postgresql.target, row.postgresql.server, row.postgresql.database, row.postgresql.synchronousCommit], ['pg', 'db.lan:5432/team', 'db.lan:5432', 'team', 'local'])
    assert.deepEqual(row.ownConnections, [])
    // a URL holding a user or password, or no login at all: refused before LayerFS
    seen.length = 0
    assert.equal((await http('POST', '/api/stores/postgresql', { name: 'pg3', url: 'postgresql://a:b@h/d', auth: 'pg' })).status, 400)
    assert.equal((await http('POST', '/api/stores/postgresql', { name: 'pg3', url: 'postgresql://h/d' })).status, 400)
    assert.equal((await http('POST', '/api/stores/postgresql', { name: 'pg3', url: 'postgresql://h/d', passwordValue: 'x' })).status, 400, 'a password needs its user')
    assert.deepEqual(seen, [])
    // the Store's own connection: synchronous_commit and another login go to store.update
    const edited = await http('POST', '/api/stores/pg/connection', { synchronousCommit: 'remote_write', auth: 'other' })
    assert.equal(edited.status, 200)
    assert.deepEqual(seen.at(-1), ['update', { auth: 'other', synchronous_commit: 'remote_write' }])
    // a typed password rotates the Store's own login (save-auth under its name), no store.update
    seen.length = 0
    assert.equal((await http('POST', '/api/stores/pg/connection', { user: 'lfs', passwordValue: 'newpw' })).status, 200)
    assert.deepEqual(seen, [['save_auth', 'pg', { user: 'lfs', password: 'newpw' }]])
    // a password alone: `store save-auth` always gets the user (required in 2.0): the saved login's own
    seen.length = 0
    assert.equal((await http('POST', '/api/stores/pg/connection', { passwordValue: 'newpw2' })).status, 200)
    assert.deepEqual(seen, [['save_auth', 'pg', { user: 'saved-user', password: 'newpw2' }]])
    // a machine's own address: URL only, proven from it (store connect --machine)
    const vm = await http('POST', '/api/stores/pg/connection', { machine: 'vm1', url: 'postgresql://10.0.0.5/team' })
    assert.equal(vm.status, 200)
    assert.deepEqual(seen.at(-1), ['connect', 'pg', 'postgresql://10.0.0.5/team', { machine: 'vm1' }])
    assert.deepEqual(vm.body.stores[0].ownConnections, [{ machine: 'vm1', url: 'postgresql://10.0.0.5/team', target: '10.0.0.5/team' }])
    // nothing to change, or clear without a machine: refused before LayerFS
    const before = seen.length
    assert.equal((await http('POST', '/api/stores/pg/connection', { machine: 'x1' })).status, 400)
    assert.equal((await http('POST', '/api/stores/pg/connection', { machine: 'x1', url: 'postgresql://u@h/d' })).status, 400)
    assert.equal((await http('POST', '/api/stores/pg/connection', { clear: true })).status, 400, 'clear names a machine')
    assert.equal(seen.length, before)
    // clear removes that machine's own address (store disconnect --machine), never the Store
    const cleared = await http('POST', '/api/stores/pg/connection', { machine: 'vm1', clear: true })
    assert.equal(cleared.status, 200)
    assert.deepEqual(seen.at(-1), ['disconnect', { machine: 'vm1' }])
    assert.deepEqual(cleared.body.stores[0].ownConnections, [])
    // the old routes are gone
    assert.equal((await http('POST', '/api/stores/pg/attach', { machine: 'docker' })).status, 404)
    assert.equal((await http('POST', '/api/stores/pg/detach', { machine: 'docker' })).status, 404)
  } finally { rmSync(root, { recursive: true, force: true }) }
})

// CUT (LayerFS 2.0, 2026-11-19): a PostgreSQL Store recorded before saved logins (no
// `auth`) is `broken` `outdated` with the save-auth fix, and its calls are refused. The
// page's one action saves the login the person names (never an existing one: that would
// change another Store's password), then `store update --auth` names it; nothing is guessed.
test('postgresql: a Store naming no saved login is broken/outdated; its fix saves the named login, then names it', async () => {
  const fix = 'layerfs store save-auth <auth> --user <user> --password-stdin && layerfs store update old --auth <auth>'
  const record = { store: 'old', engine: 'postgresql', auth: null, connections: [{ url: 'postgresql://lfs@db.lan:5432/team' }], projects: [], state: 'broken', reason: 'outdated', fix }
  const seen = []
  const { http, root } = fakeGroups({
    ...base,
    machine: { ...base.machine, list: pgMachines },
    store: {
      ...base.store,
      list: async () => ({ items: [record] }),
      get: async () => record,
      auths: async () => ({ items: [{ auth: 'taken', user: 'x', stores: ['other'] }] }),
      save_auth: async (auth, options) => { seen.push(['save_auth', auth, options]); return { ok: true } },
      update: async (name, options) => { seen.push(['update', name, options]); return { ok: true } },
    },
  })
  try {
    const view = await http('GET', '/api/storage')
    assert.equal(view.status, 200)
    const row = view.body.stores.find((s) => s.name === 'old')
    assert.deepEqual([row.state, row.reason, row.fix, row.postgresql.auth], ['broken', 'outdated', fix, null])
    // a typed user + password without a login name, or under a saved login's name: refused before LayerFS
    for (const body of [{ user: 'lfs', passwordValue: 'pw' }, { authName: 'taken', user: 'lfs', passwordValue: 'pw' }, { authName: 'old', passwordValue: 'pw' }]) {
      assert.equal((await http('POST', '/api/stores/old/connection', body)).status, 400, JSON.stringify(body))
    }
    assert.deepEqual(seen, [])
    // the named login: saved (password on stdin), then made the Store's
    assert.equal((await http('POST', '/api/stores/old/connection', { authName: 'old', user: 'lfs', passwordValue: 'pw' })).status, 200)
    assert.deepEqual(seen, [['save_auth', 'old', { user: 'lfs', password: 'pw' }], ['update', 'old', { auth: 'old' }]])
    // a saved login picked instead: store update --auth only
    seen.length = 0
    assert.equal((await http('POST', '/api/stores/old/connection', { auth: 'taken' })).status, 200)
    assert.deepEqual(seen, [['update', 'old', { auth: 'taken' }]])
  } finally { rmSync(root, { recursive: true, force: true }) }
})

// U9: the project dialog's new PostgreSQL Store names a saved login, or saves a new one under
// the name the person gives (store save-auth, the password on stdin) and then connects with it.
// A name a saved login has is refused (saving would change that login's password), a password
// needs its user, and a connect LayerFS refuses removes the new login again (a refusal proves no
// connect happened); an unknown outcome keeps it.
test('postgresql: a new connection\'s new login: saved under its name then connected; an existing name or no user is refused; a refused connect removes it', async () => {
  const saved = [{ auth: 'team', user: 'app', stores: ['pg'] }]
  const seen = []
  const records = []
  const { http, root } = fakeGroups({
    ...base,
    machine: { ...base.machine, list: pgMachines },
    store: {
      ...base.store,
      list: async () => ({ items: records }),
      get: async (name) => records.find((r) => r.store === name) ?? (() => { throw new Refusal('NotFound', `no Store ${name}`) })(),
      auths: async () => ({ items: saved }),
      save_auth: async (auth, options) => { seen.push(['save_auth', auth, options]); return { ok: true } },
      remove_auth: async (auth) => { seen.push(['remove_auth', auth]); return { ok: true } },
      connect: async (name, url, options) => {
        seen.push(['connect', name, url, options])
        if (url.includes('denied')) throw new Refusal('Denied', 'password authentication failed for user "app"')
        if (url.includes('lost')) throw new Refusal('Unknown', 'LayerFS ended without a complete result', null, 'transport-unknown')
        records.push({ store: name, engine: 'postgresql', auth: options.auth, connections: [{ url }], projects: [], state: 'ready' })
        return { ok: true }
      },
    },
  })
  try {
    // success: the new login is saved first, under the name given, then named on connect
    const ok = await http('POST', '/api/stores/postgresql', { name: 'shop', url: 'postgresql://db.lan:5432/shop', authName: 'shop-login', user: 'app', passwordValue: 's3cret' })
    assert.equal(ok.status, 200)
    assert.deepEqual(seen, [['save_auth', 'shop-login', { user: 'app', password: 's3cret' }], ['connect', 'shop', 'postgresql://db.lan:5432/shop', { auth: 'shop-login' }]])
    assert.ok(!JSON.stringify(ok.body).includes('s3cret'), 'the password is never answered')
    assert.equal(ok.body.stores.find((s) => s.name === 'shop').postgresql.auth, 'shop-login')
    // a saved login picked instead: connect --auth only, nothing saved
    seen.length = 0
    assert.equal((await http('POST', '/api/stores/postgresql', { name: 'shop2', url: 'postgresql://db.lan:5432/shop2', auth: 'team' })).status, 200)
    assert.deepEqual(seen, [['connect', 'shop2', 'postgresql://db.lan:5432/shop2', { auth: 'team' }]])
    // an existing login name, given or defaulted from the Store's name, and a password without its user: refused before LayerFS
    seen.length = 0
    const taken = await http('POST', '/api/stores/postgresql', { name: 'shop3', url: 'postgresql://db.lan/shop3', authName: 'team', user: 'app', passwordValue: 'pw' })
    assert.equal(taken.status, 400)
    assert.match(taken.body.message, /saved login named team exists/)
    saved.push({ auth: 'pg-team-lan-5432', user: 'app', stores: [] })
    assert.equal((await http('POST', '/api/stores/postgresql', { name: 'team2', url: 'postgresql://team.lan/team2', user: 'app', passwordValue: 'pw' })).status, 400, 'the default name is the server\'s (spec v23 V23-12): taken too')
    const noUser = await http('POST', '/api/stores/postgresql', { name: 'shop3', url: 'postgresql://db.lan/shop3', authName: 'shop3', user: '  ', passwordValue: 'pw' })
    assert.equal(noUser.status, 400)
    assert.match(noUser.body.message, /user/)
    assert.deepEqual(seen, [], 'nothing saved, nothing connected')
    // LayerFS refuses the connect: its code comes back, and the login saved for it is removed again
    const denied = await http('POST', '/api/stores/postgresql', { name: 'shop4', url: 'postgresql://denied/shop4', authName: 'shop4', user: 'app', passwordValue: 'wrong' })
    assert.equal(denied.status, 403)
    assert.equal(denied.body.code, 'Denied')
    assert.deepEqual(seen, [['save_auth', 'shop4', { user: 'app', password: 'wrong' }], ['connect', 'shop4', 'postgresql://denied/shop4', { auth: 'shop4' }], ['remove_auth', 'shop4']])
    // an unknown outcome proves nothing: the login stays
    seen.length = 0
    const lost = await http('POST', '/api/stores/postgresql', { name: 'shop5', url: 'postgresql://lost/shop5', authName: 'shop5', user: 'app', passwordValue: 'pw' })
    assert.notEqual(lost.status, 200)
    assert.deepEqual(seen.map((c) => c[0]), ['save_auth', 'connect'])
    // a picked login is never removed when its connect is refused
    seen.length = 0
    assert.equal((await http('POST', '/api/stores/postgresql', { name: 'shop6', url: 'postgresql://denied/shop6', auth: 'team' })).status, 403)
    assert.deepEqual(seen.map((c) => c[0]), ['connect'])
  } finally { rmSync(root, { recursive: true, force: true }) }
})

test('postgresql: a Sandbox is made on the machine asked for, else its project\'s last machine, else one its database was reached from, else native; never a stopped one (spec v23)', async () => {
  const made = []
  let last = null
  let reached = []
  const { http, root } = fakeGroups({
    ...base,
    machine: { ...base.machine, list: pgMachines },
    project: { get: async (key) => ({ project: key, store: 'pg', engine: 'postgresql' }) },
    store: { ...base.store, list: async () => ({ items: [{ store: 'pg', engine: 'postgresql', auth: 'pg', connections: [{ url: 'postgresql://h/d' }], state: 'ready' }] }),
      get: async () => ({ store: 'pg', engine: 'postgresql', auth: 'pg', connections: [{ machine: null, url: 'postgresql://h/d', reached: null }, ...reached.map((m) => ({ machine: m, url: 'postgresql://h/d', reached: 'ok' }))] }) },
    // 2.0 `sandbox create <project> <name> --machine M`: the machine is an option
    sandbox: { ...base.sandbox, list: async () => ({ items: last ? [{ sandbox: 'old', machine: last, created_at: '2026-11-19T00:00:00Z' }] : [] }), create: async (project, name, options) => { made.push([name, options.machine]); return { ok: true } } },
  })
  const create = (body) => http('POST', `/api/projects/${encodeURIComponent('pg/app')}/sandboxes`, body)
  try {
    assert.equal((await create({ name: 'b1' })).status, 200)
    assert.equal((await create({ name: 'b2', machine: 'vm1' })).status, 200, 'a machine it was never "attached" to')
    reached = ['docker']
    assert.equal((await create({ name: 'b3' })).status, 200)
    // the project's last machine wins over a reached one; a stopped last machine (vm1 here) is skipped
    last = 'native'
    assert.equal((await create({ name: 'b4' })).status, 200)
    last = 'vm1'
    assert.equal((await create({ name: 'b5' })).status, 200)
    assert.deepEqual(made, [['b1', 'native'], ['b2', 'vm1'], ['b3', 'docker'], ['b4', 'native'], ['b5', 'docker']])
    assert.equal(existsSync(join(root, 'store-machines.json')), false, 'nothing Store-wide is written')
  } finally { rmSync(root, { recursive: true, force: true }) }
})

test('postgresql: the check from a machine (or this computer) is store doctor, answered with the reach LayerFS recorded (spec v23)', async () => {
  const seen = []
  // 2.0 `store doctor <store> [--machine M] --detail`; LayerFS records each reach and `store get` reads it back as a `connections` row
  let report = { findings: [], not_checked: [], observed_at: '2026-11-19T10:00:00Z' }
  let rows = []
  const { http, root } = fakeGroups({
    ...base,
    store: {
      ...base.store,
      list: async () => ({ items: [{ store: 'pg', engine: 'postgresql', auth: 'pg', connections: [{ url: 'postgresql://localhost/d' }], state: 'ready' }] }),
      get: async () => ({ store: 'pg', engine: 'postgresql', auth: 'pg', connections: rows }),
      doctor: async (name, options) => {
        seen.push(options)
        if (options.machine === 'ghost') throw new Refusal('NotFound', 'no machine named ghost')
        return report
      },
    },
  })
  const row = (machine, reach) => ({ machine, url: 'postgresql://localhost/d', own: false, server: { host: 'localhost', port: 5432, database: 'd' }, reached: reach ? (reach.ok ? 'ok' : 'failed') : null, reach })
  try {
    rows = [row(null, null), row('docker', { at: '2026-11-19T10:00:00Z', ok: true, latency_ms: 2, server_version: '17.6', error: null })]
    const ok = await http('GET', '/api/stores/pg/check?machine=docker')
    assert.equal(ok.status, 200)
    assert.deepEqual(ok.body, { machine: 'docker', host: 'localhost', ok: true, at: Date.parse('2026-11-19T10:00:00Z'), version: '17.6', latencyMs: 2 })
    assert.equal(seen[0].machine, 'docker')
    assert.ok(seen[0].timeout_ms > 0, 'its own budget, so it always asks LayerFS')
    // this computer: no --machine, its row is the one without a machine
    rows = [row(null, { at: '2026-11-19T10:01:00Z', ok: true, latency_ms: 0, server_version: '17.6', error: null })]
    const here = await http('GET', '/api/stores/pg/check')
    assert.deepEqual([here.status, here.body.machine, here.body.ok, here.body.latencyMs], [200, 'native', true, 0])
    assert.equal(seen[1].machine, undefined)
    // a check that failed is an answer, not a refusal
    rows = [row(null, null), row('docker', { at: '2026-11-19T10:02:00Z', ok: false, error: 'machine docker cannot reach PostgreSQL Store pg\'s server: connection refused' })]
    const failed = await http('GET', '/api/stores/pg/check?machine=docker')
    assert.deepEqual([failed.status, failed.body.ok, failed.body.error.code], [200, false, 'Unavailable'])
    assert.match(failed.body.error.message, /connection refused/)
    // a check LayerFS could not run from there (the machine stopped) is an answer too
    rows = [row(null, null)]
    report = { findings: [], not_checked: [{ check: 'store.connection', from: 'docker', reason: 'stopped', message: 'machine docker is stopped' }] }
    const skipped = await http('GET', '/api/stores/pg/check?machine=docker')
    assert.deepEqual([skipped.status, skipped.body.ok, skipped.body.error.code], [200, false, 'stopped'])
    // a bad input is LayerFS's refusal
    assert.equal((await http('GET', '/api/stores/pg/check?machine=ghost')).status, 404)
  } finally { rmSync(root, { recursive: true, force: true }) }
})

test('postgresql: a Store row names its server; a new Store reuses a Store\'s saved login on the same server only', async () => {
  const records = [
    { store: 'pg', engine: 'postgresql', auth: 'pg', connections: [{ url: 'postgresql://DB.lan/team?sslmode=require' }], projects: [], state: 'ready' },
  ]
  const seen = []
  const { http, root } = fakeGroups({
    ...base,
    store: {
      ...base.store,
      list: async () => ({ items: records }),
      get: async (name) => records.find((r) => r.store === name) ?? (() => { throw new Refusal('NotFound', `no Store ${name}`) })(),
      connect: async (name, url, options) => { seen.push([name, url, options.auth]); records.push({ store: name, engine: 'postgresql', auth: options.auth, connections: [{ url }], projects: [], state: 'ready' }); return { ok: true } },
    },
  })
  try {
    const view = await http('GET', '/api/storage')
    const pg = view.body.stores.find((s) => s.name === 'pg').postgresql
    assert.deepEqual([pg.server, pg.database, pg.auth], ['db.lan:5432', 'team', 'pg'])
    // the same server: the new Store names the same saved login, nothing is saved again
    assert.equal((await http('POST', '/api/stores/postgresql', { name: 'pg2', url: 'postgresql://db.lan:5432/two', passwordFrom: 'pg' })).status, 200)
    assert.deepEqual(seen.at(-1), ['pg2', 'postgresql://db.lan:5432/two', 'pg'])
    assert.ok(!existsSync(join(root, 'secrets')), 'no password file on this host')
    // another server, or no such Store: refused before connect
    const before = seen.length
    assert.equal((await http('POST', '/api/stores/postgresql', { name: 'pg5', url: 'postgresql://other/five', passwordFrom: 'pg' })).status, 400)
    assert.notEqual((await http('POST', '/api/stores/postgresql', { name: 'pg6', url: 'postgresql://db.lan/six', passwordFrom: 'nope' })).status, 200)
    assert.equal(seen.length, before)
  } finally { rmSync(root, { recursive: true, force: true }) }
})

test('buckets: the keys go to LayerFS (the secret on stdin), never to a file here; the home\'s buckets come from home get --with tree', async () => {
  const seen = []
  const buckets = []
  const { http, root } = fakeGroups({
    ...base,
    home: { get: async (o = {}) => ({ ...(await base.home.get()), ...(o.with?.includes('tree') ? { tree: { machines: [], stores: [], buckets: buckets.map((b) => ({ ...b })) } } : {}) }) },
    backup: {
      // the long form (spec v21): `rotated` when the name is connected at that URL already (the key replaced, links kept)
      connect: async (name, url, options) => {
        seen.push(['connect', name, url, options])
        const had = buckets.some((b) => b.bucket === name)
        if (!had) buckets.push({ bucket: name, provider: url.startsWith('r2://') ? 'r2' : 'minio' })
        return options?.detail ? { bucket: name, outcome: had ? 'rotated' : 'connected' } : { ok: true }
      },
      disconnect: async (name) => { seen.push(['disconnect', name]); buckets.splice(buckets.findIndex((b) => b.bucket === name), 1); return { ok: true, unlinked: ['s1'] } },
      list: async () => ({ items: [] }),
    },
  })
  const home = join(root, 'home')
  try {
    const ok = await http('POST', '/api/buckets', { name: 'r2b', provider: 'r2', account: 'a'.repeat(32), bucket: 'bk', accessKey: 'AK', secretKey: 'SK' })
    assert.equal(ok.status, 200)
    // `secret_key` is a content option: the seam writes it on stdin (`--key-stdin`), never on argv or disk
    assert.deepEqual(seen[0], ['connect', 'r2b', `r2://${'a'.repeat(32)}/bk`, { access_key: 'AK', secret_key: 'SK', detail: true }])
    assert.equal(ok.body.outcome, 'connected', 'the long form\'s outcome comes back with the view')
    assert.ok(!existsSync(join(home, 'secret')) && !existsSync(join(root, 'secrets')), 'no key file on this host')
    assert.deepEqual(ok.body.buckets.map((b) => [b.name, b.provider]), [['r2b', 'r2']])
    assert.ok(!JSON.stringify(ok.body).includes('SK'))
    // both keys, without spaces, or nothing is sent
    const before = seen.length
    assert.equal((await http('POST', '/api/buckets', { name: 'm', provider: 'minio', endpoint: 'http://minio:9000', bucket: 'bk', accessKey: 'AK' })).status, 400)
    assert.equal((await http('POST', '/api/buckets', { name: 'm', url: 'http://u:p@minio:9000/bk', accessKey: 'AK', secretKey: 'SK' })).status, 400)
    assert.equal(seen.length, before)
    // a bucket another client connected shows up: the home's records are LayerFS's
    buckets.push({ bucket: 'shared', provider: 'minio' })
    assert.equal((await http('POST', '/api/stores', { name: 'bump' })).status, 200)
    assert.deepEqual((await http('GET', '/api/storage')).body.buckets.map((b) => [b.name, b.provider]), [['r2b', 'r2'], ['shared', 'minio']])
    // connecting again at the same URL rotates the key (U10 add-on 3): the answer says so, the page shows "Key replaced; links kept"
    const again = await http('POST', '/api/buckets', { name: 'r2b', provider: 'r2', account: 'a'.repeat(32), bucket: 'bk', accessKey: 'AK2', secretKey: 'SK2' })
    assert.equal(again.status, 200)
    assert.equal(again.body.outcome, 'rotated')
    assert.ok(!JSON.stringify(again.body).includes('SK2'))
    // the old key routes are gone: a key changes by connecting again
    assert.equal((await http('POST', '/api/buckets/r2b/keys', { accessKey: 'AK2', secretKey: 'SK2' })).status, 404)
    const gone = await http('DELETE', '/api/buckets/r2b')
    assert.equal(gone.status, 200)
    assert.deepEqual(gone.body.unlinked, ['s1'])
    assert.deepEqual(gone.body.view.buckets.map((b) => b.name), ['shared'])
  } finally { rmSync(root, { recursive: true, force: true }) }
})

test('RV4 NOTE 6: a connect that succeeds clears the bucket\'s remembered "key refused" without List again', async () => {
  let refused = true
  const { http, root } = fakeGroups({
    ...base,
    home: { get: async (o = {}) => ({ ...(await base.home.get()), ...(o.with?.includes('tree') ? { tree: { machines: [], stores: [], buckets: [{ bucket: 'bk', provider: 'minio', url: 'http://minio:9000/bk' }] } } : {}) }) },
    backup: {
      connect: async (name, url, options) => { refused = false; return options?.detail ? { bucket: name, outcome: 'rotated' } : { ok: true } },
      list: async () => { if (refused) throw new Refusal('refused', 'the access key was refused'); return { items: [] } },
    },
  })
  try {
    assert.equal((await http('GET', '/api/buckets/bk/files')).body.error.code, 'refused')
    assert.equal((await http('GET', '/api/storage')).body.buckets.find((b) => b.name === 'bk').error.code, 'refused', 'remembered')
    const again = await http('POST', '/api/buckets', { name: 'bk', url: 'http://minio:9000/bk', accessKey: 'AK2', secretKey: 'SK2' })
    assert.equal(again.status, 200)
    assert.equal(again.body.buckets.find((b) => b.name === 'bk').error, null, 'the connect answers the refusal at once')
    assert.equal((await http('GET', '/api/storage')).body.buckets.find((b) => b.name === 'bk').error, null)
  } finally { rmSync(root, { recursive: true, force: true }) }
})

test('RV4 NOTE 5: Inspect reads the record now; a broken listing fixed outside the page does not read broken again', async () => {
  let fixed = false
  const broken = { machine: 'lima1', platform: 'lima', state: 'broken', reason: 'outdated', fix: 'layerfs machine update lima1', message: 'the worker is outdated', stores: [], sandboxes: [], supports: ['sqlite'] }
  const reads = []
  const { http, root } = fakeGroups({
    ...base,
    machine: {
      list: async () => ({ items: [base.machine.list ? (await base.machine.list()).items[0] : null, broken].filter(Boolean) }),
      get: async (name, o = {}) => {
        reads.push(o.with?.includes('live') ? 'live' : 'record')
        if (name !== 'lima1') return base.machine.get(name)
        // LayerFS's record after `machine update` on the CLI: ready; before: broken (the live read alone says unknown)
        if (!o.with?.includes('live')) return fixed ? { ...broken, state: 'ready', reason: null, fix: null, message: null } : broken
        return { machine: 'lima1', platform: 'lima', state: fixed ? 'ready' : 'unknown', live: { disk_free_bytes: 7, worker: null, tool_version: null } }
      },
    },
  })
  try {
    assert.equal((await http('GET', '/api/machines')).body.machines.find((m) => m.name === 'lima1').state, 'broken')
    const before = await http('GET', '/api/machines/lima1/inspect')
    assert.deepEqual([before.body.state, before.body.reason], ['broken', 'outdated'], 'a record broken now keeps its state over the live read')
    fixed = true   // fixed on the CLI; the page has not listed since
    reads.length = 0
    const after = await http('GET', '/api/machines/lima1/inspect')
    assert.equal(after.status, 200)
    assert.equal(after.body.state, 'ready', 'never the stale listing\'s broken')
    assert.equal(after.body.fix ?? null, null)
    assert.deepEqual(reads.sort(), ['live', 'record'], 'one record read beside the live one')
  } finally { rmSync(root, { recursive: true, force: true }) }
})

test('storage state lives in the LayerFS home: jobs are kept in client-neutral <home>/job and <home>/stat', async () => {
  const { http, root } = fakeGroups(base)
  const home = join(root, 'home')
  try {
    const v = await http('GET', '/api/storage')
    assert.equal(v.status, 200)
    assert.equal(statSync(join(home, 'job')).mode & 0o777, 0o700)
    assert.equal(statSync(join(home, 'stat')).mode & 0o777, 0o700)
    assert.ok(!existsSync(join(home, 'dsh-layerfs')))
    assert.ok(!existsSync(join(root, 'buckets.json')) && !existsSync(join(root, 'storage-jobs.json')) && !existsSync(join(root, 'doctor-jobs.json')))
  } finally { rmSync(root, { recursive: true, force: true }) }
})

test('storage state: an earlier <home>/dsh-layerfs is moved into job/ and stat/ when the routes start; what is there wins, the old folder goes', () => {
  const root = mkdtempSync(join(tmpdir(), 'dsh-routes-root-'))
  const home = join(root, 'home'), legacy = join(home, 'dsh-layerfs')
  try {
    mkdirSync(join(legacy, 'project-storage'), { recursive: true })
    writeFileSync(join(legacy, 'doctor-jobs.json'), '[]')
    writeFileSync(join(legacy, 'project-storage', 'a.json'), '{}')
    mkdirSync(join(home, 'job'), { recursive: true })
    writeFileSync(join(home, 'job', 'storage-jobs.json'), '[]')
    writeFileSync(join(legacy, 'storage-jobs.json'), '[{"old":1}]')
    const { lfs } = fakeLfs(base)
    R.createGroups(testHost({ lfs, root, home, discovery }).host, async () => ({ ok: true }))
    assert.equal(readFileSync(join(home, 'job', 'doctor-jobs.json'), 'utf8'), '[]')
    assert.equal(readFileSync(join(home, 'stat', 'project-storage', 'a.json'), 'utf8'), '{}')
    assert.equal(readFileSync(join(home, 'job', 'storage-jobs.json'), 'utf8'), '[]')
    for (const d of ['job', 'stat', 'stat/project-storage', 'stat/storage-observations']) assert.equal(statSync(join(home, d)).mode & 0o777, 0o700, d)
    assert.ok(!existsSync(legacy))
  } finally { rmSync(root, { recursive: true, force: true }) }
})

test('storage jobs: a move runs as a job with storage-job events; one job per Store', async () => {
  let finish
  const { http, events, root } = fakeGroups({
    ...base,
    // 2.0 `store move` replaces push and pull
    store: { ...base.store, move: (store, machine, { on_progress }) => new Promise((resolve) => { on_progress({ phase: 'send', done: 1, total: 2 }); finish = resolve }) },
  })
  try {
    const j = await http('POST', '/api/stores/s1/push', { machine: 'lima1' })
    assert.equal(j.status, 200)
    assert.equal(j.body.kind, 'push')
    const again = await http('POST', '/api/stores/s1/push', { machine: 'lima1' })
    assert.equal(again.status, 409)
    assert.equal(again.body.code, 'ConnectBusy')
    await new Promise((r) => setTimeout(r, 10))
    finish({ ok: true, outcome: 'moved' })
    await new Promise((r) => setTimeout(r, 10))
    const done = await http('GET', `/api/storage-jobs/${j.body.id}`)
    assert.equal(done.body.outcome, 'succeeded')
    const jobEvents = events.filter((e) => e.type === 'storage-job')
    assert.ok(jobEvents.length >= 2)
    assert.equal(jobEvents.at(-1).data.type, 'storage-job')
    assert.ok(jobEvents.at(-1).data.job.ended)
    assert.ok(existsSync(join(root, 'home', 'job', 'storage-jobs.json')))
    const view = await http('GET', '/api/storage')
    assert.equal(view.body.jobs[0].id, j.body.id)
  } finally { rmSync(root, { recursive: true, force: true }) }
})

test('project removal sends project.delete (2.0 has no project.remove)', async () => {
  const sent = []
  const project = { delete: async (key) => { sent.push(['delete', key]); return { ok: true } } }
  const { http, root } = fakeGroups({ ...base, project })
  try {
    const r = await http('DELETE', '/api/projects/s1%2Fp')
    assert.equal(r.status, 200)
    assert.deepEqual(r.body, { removed: true })
    assert.deepEqual(sent, [['delete', 's1/p']])
  } finally { rmSync(root, { recursive: true, force: true }) }
})

test('storage jobs: compact runs as a job with its steps, phases and report (2.0 store compact always reports progress)', async () => {
  const compactCall = (options) => async () => ({ ...(await discovery()), calls: [{ family: 'store', call: 'compact', args: [{ name: 'name', kind: 'name' }], options }] })
  let finish, given
  const impl = (kind) => ({
    ...base,
    store: {
      ...base.store,
      get: async () => ({ store: 's1', engine: kind, machine: kind === 'sqlite' ? 'native' : null }),
      compact: (store, options) => new Promise((resolve) => { given = options; options.on_progress?.({ phase: 'mark', done: null, total: null }); options.on_progress?.({ phase: 'build', done: null, total: null }); finish = resolve }),
    },
  })
  // a runtime whose store.compact declares on_progress
  const { http, root } = fakeGroups(impl('sqlite'), compactCall([{ name: 'on_progress', kind: 'function' }]))
  try {
    assert.equal((await http('GET', '/api/storage')).body.compact, true)
    const j = await http('POST', '/api/stores/s1/compact')
    assert.equal(j.status, 200)
    assert.equal(j.body.kind, 'compact')
    assert.deepEqual(j.body.steps, ['mark', 'build', 'verify', 'install'])
    await new Promise((r) => setTimeout(r, 10))
    assert.equal(typeof given.on_progress, 'function')
    const running = await http('GET', `/api/storage-jobs/${j.body.id}`)
    assert.equal(running.body.phase, 'build')
    assert.deepEqual(running.body.phases.map((p) => p.phase), ['mark', 'build'])
    assert.equal((await http('POST', '/api/stores/s1/compact')).body.code, 'ConnectBusy')
    // the 2.0 long form: `outcome` and `freed_bytes`
    finish({ outcome: 'compacted', freed_bytes: 4096 })
    await new Promise((r) => setTimeout(r, 10))
    const done = await http('GET', `/api/storage-jobs/${j.body.id}`)
    assert.equal(done.body.outcome, 'succeeded')
    assert.deepEqual(done.body.result, { outcome: 'compacted', freed_bytes: 4096 })
  } finally { rmSync(root, { recursive: true, force: true }) }
  // a PostgreSQL Store names its own steps; a refusal in `install` keeps the SDK's own code (not a transfer's unknown)
  const pg = fakeGroups({ ...impl('postgresql'), store: { ...impl('postgresql').store, compact: async () => { throw new Refusal('Capacity', 'no space', { phase: 'install' }) } } }, compactCall([{ name: 'on_progress', kind: 'function' }]))
  try {
    const j = await pg.http('POST', '/api/stores/s1/compact')
    assert.deepEqual(j.body.steps, ['mark', 'build', 'swap', 'cleanup'])
    await new Promise((r) => setTimeout(r, 10))
    const failed = await pg.http('GET', `/api/storage-jobs/${j.body.id}`)
    assert.equal(failed.body.outcome, 'failed')
    assert.equal(failed.body.error.code, 'Capacity')
  } finally { rmSync(pg.root, { recursive: true, force: true }) }
})

test('project init job and saved reads route to their calls', async () => {
  const head = 'c'.repeat(66)
  const src = mkdtempSync(join(tmpdir(), 'dsh-routes-src-'))
  const { http, root } = fakeGroups({
    ...base,
    // 2.0 long forms: project rows, `branch get`/list `commit`, `file list` items `{ path, type }`, `file stat`, `file read --detail`
    project: {
      init: async (key) => ({ project: key, store: 's1', engine: 'sqlite', machine: 'native' }),
      list: async () => ({ items: [{ project: 's1/p', store: 's1', engine: 'sqlite' }] }),
      get: async (key) => ({ project: key, store: 's1', engine: 'sqlite', machine: 'native' }),
    },
    branch: { get: async () => ({ commit: head }), list: async () => ({ items: [{ branch: 'main', commit: head }] }) },
    file: {
      list: async () => ({ items: [{ path: '/a.txt', type: 'file' }] }),
      stat: async () => ({ type: 'file', size: 3, mode: 0o644 }),
      read: async () => ({ text: 'hi\n', next_line: null, line_truncated: false, binary: false, size: 3 }),
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
    assert.deepEqual(list.body.map((p) => [p.key, p.storeState]), [['s1/p', 'ready']])
    const key = encodeURIComponent('s1/p')
    const fs = await http('GET', `/api/projects/${key}/fs?branch=main&path=/`)
    assert.deepEqual(fs.body, { items: [{ name: 'a.txt', path: '/a.txt', kind: 'file', size: null, mode: null }], next: null, at: { id: head } }, '2.0 lists no size or mode: an opened file reads them')
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

test('live: machines, store create, project init, list/get, branches, fs, file', { skip: !pin && 'no pinned LayerFS CLI', timeout: 600_000 }, async (context) => {
  // a short path: the native daemon's sockets live in the home
  const root = mkdtempSync('/tmp/dshrt-')
  const home = join(root, 'home')
  // a 2.0 CLI makes no home on a read: `home init` first, as Set up LayerFS does
  execFileSync(pin.cli, ['home', 'init'], { env: { ...process.env, LAYERFS_HOME: home }, timeout: 30_000 })
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
  context.after(async () => {
    await http('POST', `/api/projects/${key}/close-sandboxes`, { force: true }).catch(() => {})
    await groups.projects.close()
    execFileSync(pin.cli, ['machine', 'stop', 'native'], { env: { ...process.env, LAYERFS_HOME: home }, timeout: 30_000 })
    rmSync(root, { recursive: true, force: true })
  })
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
})

test('project storage: a ledger of each save\'s counters from 2.0 commit list --detail pages (by cursor), then only new saves', async () => {
  const id = (c, n) => `12${String(n).padStart(3, '0')}${c.repeat(61)}`
  const layer = (n) => `32${String(n).padStart(3, '0')}${'f'.repeat(61)}`
  const at = (n) => new Date(1_000_000 + n * 1000).toISOString()
  // a 2.0 `commit list --detail` row: its time, how it was made, its tree's bytes and the bytes it wrote first
  const commit = (branch, n, extra = {}) => ({ commit: id(branch[0], n), branch, created_at: at(n), via: 'fs', size: 100, new_canonical_bytes: 20, new_stored_bytes: 10, new_metadata_bytes: 4, message: `save ${n}`, ...extra })
  const history = {
    a: [commit('a', 3), commit('a', 2, { via: 'merge' }), commit('a', 1)],
    b: [commit('b', 2), commit('b', 1)],
    // c is longer than one page (100 rows): its second page is asked by cursor
    c: Array.from({ length: 120 }, (_, i) => commit('c', 120 - i)),
    main: [{ commit: layer(2), source_branch: 'a', size: 40 }, { commit: layer(1), source_branch: 'b', size: 30 }],
  }
  const asked = []
  let openB = () => {}
  const gate = new Promise((resolve) => { openB = resolve })
  const impl = {
    ...base,
    project: {
      get: async (key) => ({ project: key, store: 's1', engine: 'sqlite', machine: 'native', created_at: new Date(7).toISOString() }),
      stats: async () => ({ size: 100, file_count: 3, dir_count: 1, symlink_count: 0 }),
    },
    store: { ...base.store, get: async (name, o = {}) => ({ store: 's1', engine: 'sqlite', projects: ['p', 'q'], ...(o.with?.includes('stats') ? { stats: { logical_bytes: 1000, canonical_bytes: 400, stored_bytes: 100, metadata_bytes: 50, file_bytes: 4096 } } : {}) }) },
    branch: {
      // a Branch with no Commit of its own answers the one it was forked at (`d`): nothing to walk
      list: async () => ({ items: [{ branch: 'main', commit: history.main[0].commit }, ...['a', 'b', 'c'].map((b) => ({ branch: b, commit: history[b][0].commit, forked_from: layer(1) })), { branch: 'd', commit: layer(1), forked_from: layer(1) }] }),
    },
    commit: {
      list: async (_project, branch, options) => {
        asked.push([branch, options.cursor ?? null])
        if (branch === 'b') await gate
        const all = history[branch], from = all.findIndex((c) => c.commit === options.at)
        const rows = all.slice(from < 0 ? 0 : from), start = options.cursor ? Number(options.cursor) : 0
        return { items: rows.slice(start, start + options.limit), ...(start + options.limit < rows.length ? { next_cursor: String(start + options.limit) } : {}) }
      },
    },
  }
  const { http, groups, root } = fakeGroups(impl)
  const path = `/api/projects/${encodeURIComponent('s1/p')}/storage`
  try {
    const first = await http('GET', path)
    assert.equal(first.status, 200, JSON.stringify(first.body))
    assert.equal(first.body.totals, null, 'nothing counted yet')
    assert.ok(first.body.building, 'the first read runs in the background')
    // while b's listing waits, the view already holds a's saves, marked partial
    let midway = first
    for (let i = 0; i < 100 && !(midway.body.totals?.saves >= 3); i++) {
      await new Promise((resolve) => setTimeout(resolve, 5))
      midway = await http('GET', path)
    }
    assert.equal(midway.body.totals?.saves, 3, JSON.stringify(midway.body))
    assert.equal(midway.body.partial, true)
    assert.ok(midway.body.building)
    assert.equal(midway.body.totals.stored, 30)
    openB()
    await groups.projects.storageLedgers.settle('s1/p', 7)
    const done = await http('GET', path)
    assert.equal(done.body.building, null)
    assert.equal(done.body.partial, false)
    assert.deepEqual(done.body.totals, { saves: 125, merges: 1, branches: 3, publications: 2, commitLogical: 12500, publicationLogical: 70, canonical: 2500, stored: 1250, metadata: 500 })
    assert.equal(done.body.recent.length, 30)
    assert.equal(done.body.growth.at(-1), 1250)
    assert.deepEqual({ name: done.body.store.name, projects: done.body.store.projects, stored: done.body.store.stored, file: done.body.store.file }, { name: 's1', projects: 2, stored: 100, file: 4096 })
    assert.deepEqual(done.body.main, { logical: 100, files: 3, directories: 1, complete: true })
    assert.deepEqual(asked.filter(([b]) => b === 'c'), [['c', null], ['c', '100']], 'c\'s second page by the cursor its first answered')
    assert.ok(!asked.some(([b]) => b === 'd'))
    // a new save on b: a recount lists only b, and stops at the save it knows
    history.b.unshift(commit('b', 200))
    asked.length = 0
    await http('GET', `${path}?refresh=1`)
    await groups.projects.storageLedgers.settle('s1/p', 7)
    const again = await http('GET', path)
    assert.deepEqual(asked, [['b', null]], 'one listing page of b, nothing of a, c or main')
    assert.equal(again.body.totals.saves, 126)
    assert.equal(again.body.totals.stored, 1260)
  } finally {
    openB()
    await groups.projects.close()
    rmSync(root, { recursive: true, force: true })
  }
})

test('project size: counting until the first count ends, then packed bytes and main\'s size; the Store\'s bytes when alone', async () => {
  const id = (n) => `12${String(n).padStart(3, '0')}${'a'.repeat(61)}`
  const history = [2, 1].map((n) => ({ commit: id(n), created_at: new Date(1_000_000 + n).toISOString(), via: 'fs', size: 100, new_canonical_bytes: 20, new_stored_bytes: 10, new_metadata_bytes: 4 }))
  let open = () => {}
  const gate = new Promise((resolve) => { open = resolve })
  const impl = {
    ...base,
    project: { get: async (key) => ({ project: key, store: 's1', engine: 'sqlite', machine: 'native', created_at: new Date(7).toISOString() }), stats: async () => ({ size: 900, file_count: 3, dir_count: 1, symlink_count: 0 }) },
    store: { ...base.store, get: async (name, o = {}) => ({ store: 's1', engine: 'sqlite', projects: ['p'], ...(o.with?.includes('stats') ? { stats: { stored_bytes: 8423, metadata_bytes: 222021 } } : {}) }) },
    branch: { list: async () => ({ items: [{ branch: 'a', commit: history[0].commit, forked_from: null }] }) },
    commit: {
      list: async (_project, _branch, options) => {
        await gate
        const start = options.cursor ? Number(options.cursor) : 0
        return { items: history.slice(start, start + options.limit) }
      },
    },
  }
  const { http, groups, root } = fakeGroups(impl)
  const path = `/api/projects/${encodeURIComponent('s1/p')}/size`
  try {
    const first = await http('GET', path)
    assert.equal(first.status, 200, JSON.stringify(first.body))
    assert.deepEqual(first.body, { project: 's1/p', stored: null, counting: true, files: 900 })
    open()
    await groups.projects.storageLedgers.settle('s1/p', 7)
    const done = await http('GET', path)
    assert.deepEqual(done.body, { project: 's1/p', stored: 20, counting: false, files: 900 })
    // alone in its Store: the Store's packed bytes, publications to main included
    const alone = await http('GET', `${path}?alone=1`)
    assert.deepEqual(alone.body, { project: 's1/p', stored: 8423, counting: false, files: 900 })
  } finally {
    open()
    await groups.projects.close()
    rmSync(root, { recursive: true, force: true })
  }
})
