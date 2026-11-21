// The Stores boards and Connect a database, as data
// (src/client/storage/machine-stores.ts): the two kinds on a machine, the kind
// the tab opens on, PostgreSQL Stores under their server, connection URLs and
// which step a failed check stopped at; a card's projects split and a Store's
// backups by provider (src/client/storage/store-backups.ts).
//   node --test test/machine-stores.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtempSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

const root = fileURLToPath(new URL('..', import.meta.url))
const out = mkdtempSync(join(root, 'lib', '.test-machine-stores-'))
await build({ absWorkingDir: root, stdin: { contents: "export * from './src/client/storage/machine-stores.ts'; export * from './src/client/storage/store-backups.ts'; export { pgUsable } from './src/client/machines/model.ts'", resolveDir: root, loader: 'ts' }, outfile: join(out, 'ms.mjs'), bundle: true, format: 'esm', platform: 'node', target: 'node24', external: ['@deepseek-ai/*'], logLevel: 'warning' })
const M = await import(pathToFileURL(join(out, 'ms.mjs')).href)
test.after(() => rmSync(out, { recursive: true, force: true }))

const t = (key, params) => (params ? `${key} ${JSON.stringify(params)}` : key)
// the host's 2.0 StoreRow: the six-state `state` with its companions; a PostgreSQL Store's saved login is `postgresql.auth`
const sqlite = (name, machine, bytes) => ({ name, kind: 'sqlite', machine, projects: [name], projectsCount: 1, size: bytes == null ? null : { bytes }, state: 'ready', links: [], job: null })
// `from`: the machine its database is connected from (spec v23 `connections`; this computer is the row with no machine)
const pg = (name, target, from, more = {}) => ({
  name, kind: 'postgresql', machine: null, projects: ['p'], projectsCount: 1, size: null, state: 'ready', links: [], job: null,
  postgresql: { auth: 'layerfs', url: `postgresql://${target}`, target, server: null, database: target.split('/').at(-1), synchronousCommit: null },
  connectedFrom: from ? [{ machine: from === 'native' ? null : from, reached: 'ok' }] : [], ownConnections: [], ...more,
})
// 2.0 `machine list --detail`: `supports` lists what the machine takes
const machine = (name, platform, supports = ['sqlite', 'postgresql_sandbox'], state = 'ready') => ({ name, platform, state, supports })
const view = (stores, machines = [machine('native', 'native'), machine('docker', 'docker')]) => ({ stores, machines, buckets: [], jobs: [], at: 0 })

test('a machine\'s Stores: its SQLite files with their total size, and the PostgreSQL Stores connected from it, by name', () => {
  const v = view([sqlite('b', 'docker', 300), sqlite('a', 'docker', null), sqlite('h', 'native', 5), pg('z', 'u@localhost:5432/z', 'docker'), pg('y', 'u@localhost:5432/y', 'native'), pg('x', 'u@localhost:5432/x', 'native')])
  const s = M.machineStores(v, 'docker', t)
  assert.deepEqual(s.sqlite.map((x) => x.name), ['a', 'b'])
  assert.equal(s.sqliteBytes, 300, 'a Store without a size is left out of the total, never counted as 0')
  assert.deepEqual(s.postgresql.map((x) => x.name), ['z'], 'only the Store connected from docker')
  assert.deepEqual(M.machineStores(v, 'docker', t, new Set(['x'])).postgresql.map((x) => x.name), ['x', 'z'], 'and one with a Sandbox on docker now')
  assert.deepEqual(M.machineStores(v, 'native', t).postgresql.map((x) => x.name), ['x', 'y'])
  assert.equal(s.noSqlite, null)
  assert.equal(M.machineStores(view([sqlite('a', 'docker', null)]), 'docker', t).sqliteBytes, null, 'no size known is null')
  const unseen = { ...pg('w', 'u@db:5432/w', null) }
  assert.deepEqual(M.machineStores(view([unseen]), 'native', t).postgresql.map((x) => x.name), [], 'a Store no side connected from yet is on no machine page')
})

test('the Machines board lists a PostgreSQL Store on each machine it is connected from, and on any machine holding one of its Sandboxes', () => {
  const fleet = {
    postgresql: [{ name: 'team', projects: 1, connectedFrom: [{ machine: 'native', reached: null }, { machine: 'docker', reached: 'ok' }] }, { name: 'lab', projects: 1, connectedFrom: [{ machine: 'native', reached: 'ok' }] }, { name: 'idle', projects: 0, connectedFrom: [{ machine: 'native', reached: null }] }],
    sandboxes: [{ name: 'b1', machine: 'vm1', project: 'lab/app', workspaces: 1 }, { name: 'b2', machine: 'vm1', project: null, workspaces: 0 }, { name: 'b3', machine: 'docker', project: 'file/app', workspaces: 0 }],
  }
  const on = (name) => M.pgUsable(fleet, { name }).map((s) => s.name)
  assert.deepEqual(on('docker'), ['team'])
  assert.deepEqual(on('native'), ['team', 'lab', 'idle'], 'this computer\'s row: every Store')
  assert.deepEqual(on('vm1'), ['lab'], 'a Sandbox of its project runs here now')
  assert.deepEqual(on('other'), [], 'no machine lists every Store: a PostgreSQL Store is attached to none')
})

test('a machine that takes neither kind says why; the tab opens on the kind asked for when the machine takes it, else the kind with Stores', () => {
  const v = view([pg('z', 'u@db:5432/z', 'native')], [machine('lima', 'lima', ['sqlite'])])
  const s = M.machineStores(v, 'lima', t)
  assert.equal(s.noPostgresql, 'msNoPostgresql')
  assert.equal(M.startKind(s, 'postgresql'), 'sqlite', 'a kind the machine cannot run, with nothing running there, is not opened')
  const both = M.machineStores(view([sqlite('a', 'docker', 1), pg('z', 'u@db:5432/z', 'docker')]), 'docker', t)
  assert.equal(M.startKind(both, 'postgresql'), 'postgresql')
  assert.equal(M.startKind(both, null), 'sqlite')
  const pgOnly = M.machineStores(view([pg('z', 'u@db:5432/z', 'docker')]), 'docker', t)
  assert.equal(M.startKind(pgOnly, null), 'postgresql', 'with nothing asked, the kind that has Stores here')
})

test('PostgreSQL Stores sit under their server: its address, saved login and state, said once for all its databases', () => {
  const groups = M.serverGroups([
    sqlite('file', 'native', 1),
    pg('demo-services', 'localhost:5432/demo_services', 'docker'),
    pg('demo-experiments', 'localhost:5432/demo_experiments', 'native', { state: 'broken', reason: 'no_database', fix: 'layerfs store connect demo-experiments …' }),
    pg('analytics', 'warehouse.lan:5432/analytics', 'native', { state: 'broken', reason: 'unreachable', postgresql: { auth: 'ro_user', url: null, target: 'warehouse.lan:5432/analytics', server: null, database: 'analytics', synchronousCommit: null } }),
    pg('later', 'new.lan:5432/later', 'native', { state: 'unknown', postgresql: { auth: 'app', url: null, target: 'new.lan:5432/later', server: null, database: 'later', synchronousCommit: null } }),
  ])
  assert.deepEqual(groups.map((g) => [g.server, g.user, g.state, g.stores.map((s) => s.name)]), [
    ['localhost:5432', 'layerfs', 'ok', ['demo-experiments', 'demo-services']],
    ['new.lan:5432', 'app', 'unknown', ['later']],
    ['warehouse.lan:5432', 'ro_user', 'failed', ['analytics']],
  ], 'servers by address, Stores by name, SQLite Stores left out')
  assert.deepEqual([groups[0].version, groups[0].latencyMs, groups[0].error], [null, null, null], 'a server one Store reached is reachable; a 2.0 list row carries no version or latency')
  assert.equal(groups[2].error, 'unreachable')
  // a card says its own status only when it is not its server's
  assert.equal(M.storeOffServer(groups[0].stores[0], groups[0]), 'no_database', 'this database fails while its server answers')
  assert.equal(M.storeOffServer(groups[0].stores[1], groups[0]), null, 'a ready Store says nothing')
  assert.equal(M.storeOffServer(groups[2].stores[0], groups[2]), null, 'an unreachable server is said once, in its header')
  assert.equal(M.serverKey({ name: 's', postgresql: { server: 'db:6000', target: 'u@other:1/x' } }), 'db:6000', 'the host\'s own reading of the URL first')
  assert.equal(M.serverKey({ name: 's', postgresql: null }), 's')
})

test('PostgreSQL Stores sit under their saved login (spec v23 V23-1): every login, one no Store uses too, then No login; each with its worst state and servers', () => {
  const at = (server) => ({ server })
  const groups = M.loginGroups([
    sqlite('file', 'native', 1),
    pg('b', 'h:1/b', 'native', { postgresql: { auth: 'demo-pg', ...at('h:5432') } }),
    pg('a', 'h:1/a', 'native', { state: 'broken', reason: 'outdated', postgresql: { auth: 'demo-pg', ...at('h:5432') } }),
    pg('far', 'w:1/far', 'native', { postgresql: { auth: 'demo-pg', ...at('w:6000') } }),
    pg('lost', 'h:1/lost', 'native', { state: 'broken', reason: 'missing', postgresql: { auth: 'gone', ...at('h:5432') } }),
    pg('none', 'h:1/none', 'native', { state: 'broken', reason: 'outdated', postgresql: { auth: null, ...at('h:5432') } }),
  ], [{ auth: 'unused', user: 'u', stores: [] }, { auth: 'demo-pg', user: 'postgres', stores: ['a', 'b', 'far'], updated: 7 }, { auth: 'bad', user: null, stores: [], state: 'broken', reason: 'failed', fix: 'layerfs store save-auth bad --user <user> --password-stdin' }])
  assert.deepEqual(groups.map((g) => [g.auth, g.user, g.state, g.stores.map((s) => s.name), g.servers]), [
    ['bad', null, 'broken', [], []],
    ['demo-pg', 'postgres', 'broken', ['a', 'b', 'far'], ['h:5432', 'w:6000']],
    ['unused', 'u', 'ready', [], []],
    [null, null, 'broken', ['lost', 'none'], ['h:5432']],
  ], 'logins by name; a login serves Stores on several servers (V21-3); a missing login and none go under No login')
  assert.equal(groups[1].saved, 7)
  assert.equal(groups[0].fix, 'layerfs store save-auth bad --user <user> --password-stdin')
  assert.deepEqual(M.loginGroups([], []), [])
})

test('connection URLs: parsed from a URL or a bare target, built back without a password; a loopback server becomes the machine\'s alias', () => {
  assert.deepEqual(M.parsePgUrl('postgresql://app@db.internal:5432/orders'), { user: 'app', host: 'db.internal:5432', database: 'orders' })
  assert.deepEqual(M.parsePgUrl('ro_user@warehouse.lan:5432/analytics'), { user: 'ro_user', host: 'warehouse.lan:5432', database: 'analytics' })
  assert.equal(M.parsePgUrl(''), null)
  assert.equal(M.pgUrl({ user: 'app', host: 'db:5432', database: 'orders' }), 'postgresql://app@db:5432/orders')
  assert.equal(M.routedServer('localhost:5432', 'host.docker.internal'), 'host.docker.internal:5432')
  assert.equal(M.routedServer('127.0.0.1:5432', '192.168.5.2'), '192.168.5.2:5432')
  assert.equal(M.routedServer('db.internal:5432', 'host.docker.internal'), 'db.internal:5432', 'a remote server is dialled as it is')
  assert.equal(M.routedServer('localhost:5432', null), 'localhost:5432', 'native dials the host itself')
})

test('the step a failed check stopped at, from LayerFS\'s documented refusals; any other refusal names no step', () => {
  assert.equal(M.failedStep('Unavailable', 'machine docker cannot reach PostgreSQL Store x\'s server at db:5432: connection refused'), 'server')
  assert.equal(M.failedStep('Unavailable', 'machine docker, PostgreSQL Store x at db:5432: the PostgreSQL server holds another Store than x'), 'store')
  assert.equal(M.failedStep('Denied', 'password authentication failed for user "app"'), 'server', 'a login refused')
  assert.equal(M.failedStep('Denied', 'role app lacks USAGE on schema layerfs (3 privilege(s) missing in all)'), 'store')
  assert.equal(M.failedStep('Denied', 'role app lacks INSERT on table layerfs.objects (1 privilege(s) missing in all)'), 'write')
  assert.equal(M.failedStep('Conflict', 'Store x changed while machine docker was checked'), null)
  assert.equal(M.failedStep(undefined, undefined), null)
})

const proj = (name, stored, time, more = {}) => ({ name, created: 1, totals: stored == null ? null : { saves: 1, merges: 0, branches: 1, stored, metadata: 0, commitLogical: 0, canonical: 0 }, last: time == null ? null : { id: 'c', time, branch: 'main', message: '' }, files: null, partial: false, building: null, error: null, ...more })

test('a card\'s projects: shares of the file, of their sum without a file size, uncounted projects kept without a share', () => {
  const sp = M.projectSplit([proj('b', 100, 5), proj('a', 300, 9), proj('c', null, null, { partial: true })], 1000)
  assert.deepEqual(sp.rows.map((r) => [r.name, r.share, r.counting]), [['a', 0.3, false], ['b', 0.1, false], ['c', 0, false]], 'a project read but not counted is not counting: it is not polled')
  assert.equal(M.projectSplit([proj('d', null, null, { partial: true, building: { branches: 0, branchesTotal: 1, saves: 3 } })], null).rows[0].counting, true)
  assert.equal(M.projectSplit([proj('e', null, null, { partial: true, error: { code: 'Unsupported', message: 'no engine' } })], null).rows[0].error, 'no engine')
  assert.equal(sp.lastWrite, 9)
  assert.deepEqual(sp.rows.map((r) => r.hue), [0, 1, 2], 'hues follow name order, as on the Store page')
  const pgSplit = M.projectSplit([proj('x', 30, 1), proj('y', 10, null)], null)
  assert.deepEqual(pgSplit.rows.map((r) => r.share), [0.75, 0.25])
  assert.deepEqual(M.projectSplit([proj('x', 30, 1)], 20).rows.map((r) => r.share), [1], 'projects counting more than the file never pass a full bar')
  assert.equal(M.projectSplit([], 50).lastWrite, null)
})

const bucket = (name, provider, files = null) => ({ name, provider, listed: files ? 1 : null, files, error: null, stores: [] })
const linked = (name, links) => ({ ...sqlite(name, 'docker', 10), links: links.map(([b, at]) => ({ bucket: b, file: name, last: at == null ? null : { op: 'backup', at, bytes: 10 } })) })
// a 2.0 `backup list --detail` file as the host answers it: `behind` / `changed` / `missing` (the client derives the state)
const STATES = { current: [false, false, false], missing: [null, null, true], store_changed: [true, false, false], both_changed: [true, true, false], bucket_changed: [false, true, false] }
const listed = (store, state, modified = 1) => { const [behind, changed, missing] = STATES[state]; return { file: store, store, bytes: 10, modified, behind, changed, missing, message: null } }

test('a Store\'s backups: most urgent first, grouped by provider with the urgent group first, and the action each state offers', () => {
  const buckets = [bucket('m1', 'minio', [listed('s', 'current', 50)]), bucket('m2', 'minio', [listed('s', 'missing')]), bucket('r1', 'r2', [listed('s', 'store_changed', 20)]), bucket('x', null)]
  const backups = M.storeBackups(linked('s', [['m1', 5], ['m2', 1], ['r1', 3], ['x', 99]]), buckets)
  assert.deepEqual(backups.map((b) => [b.link.bucket, b.health]), [['m2', 'problem'], ['r1', 'stale'], ['x', 'unknown'], ['m1', 'current']])
  assert.equal(backups.find((b) => b.link.bucket === 'm1').at, 50, 'the listing\'s time before this host\'s last run')
  assert.deepEqual(M.backupGroups(backups).map((g) => [g.provider, g.items.map((b) => b.link.bucket)]), [['minio', ['m2', 'm1']], ['r2', ['r1']], [null, ['x']]])
  assert.deepEqual(M.backupCounts(backups), { problem: 1, stale: 1, current: 1, unknown: 1 })
  assert.deepEqual(M.backupProviders(backups), ['minio', 'r2', null])
  assert.deepEqual(['missing', 'store_changed', 'both_changed', 'current', 'bucket_changed', 'unknown', null].map(M.actionOf), ['again', 'update', 'update', 'restore', 'restore', 'now', 'now'])
})

test('Back up: every bucket by provider with the backup it would update; it starts on a bucket just added, else the copy most in need', () => {
  const buckets = [bucket('r1', 'r2', [listed('s', 'current')]), bucket('m1', 'minio', [listed('s', 'missing')]), bucket('m0', 'minio')]
  const groups = M.backupTargets(linked('s', [['r1', 1], ['m1', 1]]), { buckets })
  assert.deepEqual(groups.map((g) => [g.provider, g.items.map((x) => [x.bucket.name, x.backup ? x.backup.health : null])]), [['minio', [['m0', null], ['m1', 'problem']]], ['r2', [['r1', 'current']]]])
  assert.equal(M.firstTarget(groups, undefined), 'm1')
  assert.equal(M.firstTarget(groups, ['r1', 'm1']), 'm0', 'Add a bucket returns on the new one')
  assert.equal(M.firstTarget([], undefined), '')
  assert.equal(M.freshName('s', ['s', 's-2']), 's-3', 'a missing copy goes under a name LayerFS has not seen in the bucket')
  assert.equal(M.freshName('s', []), 's-2')
  assert.equal(M.freshName('s-2', []), 's-3', 'counting on, never s-2-2, and never the missing name itself')
})
