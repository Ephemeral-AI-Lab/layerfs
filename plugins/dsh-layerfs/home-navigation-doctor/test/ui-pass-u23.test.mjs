// Spec v23 on the plugin, package U23: no Store-wide "Runs on" (each session
// picks its machine, never a broken or stopped one), the PostgreSQL tab by
// saved login, who connects to a database (Connected from), the Connection
// tab, a new login named after its server, URL login parameters refused, and
// the v23 fix lines (Save a login…, Update).
//   node --test test/ui-pass-u23.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

const root = fileURLToPath(new URL('..', import.meta.url))
mkdirSync(join(root, 'lib'), { recursive: true })
const out = mkdtempSync(join(root, 'lib', '.test-ui-pass-u23-'))

// React, the DSH primitives and clsx are stubbed: the test reads the modules' pure exports, it renders nothing.
// Each importer gets a stub exporting exactly the names it imports from that package.
const stub = {
  name: 'stub-ui',
  setup(b) {
    b.onResolve({ filter: /^(react|react-dom|clsx|@deepseek-ai\/.*)(\/.*)?$/ }, (a) => ({ path: `${a.path}|${a.importer}`, namespace: 'stub' }))
    b.onLoad({ filter: /.*/, namespace: 'stub' }, (a) => {
      const [spec, importer] = a.path.split('|')
      const text = readFileSync(importer, 'utf8')
      // the automatic JSX runtime's own names, imported by esbuild rather than the source
      const names = new Set(spec.endsWith('jsx-runtime') ? ['jsx', 'jsxs', 'Fragment'] : [])
      const quoted = spec.replace(/[.*+?^${}()|[\]\\/]/g, '\\$&')
      for (const m of text.matchAll(new RegExp(`import\\s+(type\\s+)?([^'";]*?)\\s+from\\s+['"]${quoted}['"]`, 'g'))) {
        if (m[1]) continue
        const braces = /\{([^}]*)\}/.exec(m[2])
        for (const part of braces ? braces[1].split(',') : []) {
          const w = part.trim()
          if (!w || w.startsWith('type ')) continue
          names.add(w.split(/\s+as\s+/)[0].trim())
        }
      }
      const lines = ['const f = function Stub() { return null }', 'export default f', ...[...names].map((n) => `export const ${n} = f`)]
      return { contents: lines.join('\n'), loader: 'js' }
    })
  },
}
await build({
  absWorkingDir: root,
  stdin: {
    contents: [
      "export * as sm from './src/client/storage/model.ts'",
      "export * as ms from './src/client/storage/machine-stores.ts'",
      "export * as doc from './src/shared/doctor.ts'",
      "export { fixCall } from './src/client/storage/fix-call.ts'",
      "export { serverLogin } from './src/client/storage/ConnectDatabase.tsx'",
      "export { PG_URL_LOGIN } from './src/client/storage/pg-fields.tsx'",
      "export * as setup from './src/client/projects/setup/model.ts'",
      "export * as place from './src/host/placement.ts'",
      "export { connectionRows, loginParameter, pgLoginName, storeRow } from './src/host/routes/storage.ts'",
      "export { pgStoreOf } from './src/host/routes/machines.ts'",
      "export { locales as storageLocales } from './src/client/storage/locale.ts'",
      "export { locales as setupLocales } from './src/client/projects/setup/locale.ts'",
    ].join('\n'),
    resolveDir: root, loader: 'ts',
  },
  outfile: join(out, 'u23.mjs'), bundle: true, format: 'esm', platform: 'node', target: 'node24', plugins: [stub], external: ['node:*'], logLevel: 'warning',
})
const U = await import(pathToFileURL(join(out, 'u23.mjs')).href)
test.after(() => rmSync(out, { recursive: true, force: true }))
const src = (path) => readFileSync(join(root, path), 'utf8')

// a `store list --detail` PostgreSQL item as LayerFS v23 answers it
const reach = (ok, more = {}) => ({ at: '2026-11-19T12:00:00Z', ok, latency_ms: ok ? 1 : null, server_version: ok ? '17.6' : null, error: ok ? null : 'machine x cannot reach the server', ...more })
const item = (connections, more = {}) => ({ store: 'pg', engine: 'postgresql', state: 'ready', auth: 'demo-pg', binding: 'f64587b8a1c2d3e4', connections, projects: ['abc'], backups: [], ...more })
const row = (machine, reached, own = false, url = 'postgresql://127.0.0.1:60506/demo') => ({ machine, url, own, server: { host: '127.0.0.1', port: 60506, database: 'demo' }, reached, reach: reached ? reach(reached === 'ok') : null })

test('Runs on is gone: no store-machines module, no runs-on route, no runs-on step or dialog (V23-11)', () => {
  assert.equal(existsSync(join(root, 'src/host/store-machines.ts')), false)
  assert.doesNotMatch(src('src/host/routes/router.ts'), /runs-on/)
  assert.doesNotMatch(src('src/client/projects/setup/run.ts'), /runs-on/)
  assert.doesNotMatch(src('src/client/storage/dialogs.tsx'), /RunsOnDialog|pg-runs-on/)
  for (const loc of Object.values(U.storageLocales)) assert.equal(loc.pgRunsOn, undefined)
  const steps = U.setup.createSteps({ name: 'app', kind: 'postgresql', store: { name: 'pg', kind: 'postgresql', projects: [] }, machines: ['vm'] })
  assert.deepEqual(steps.map((s) => s.id), ['init', 'open'])
  assert.equal(steps[1].machine, 'vm', 'the picked machine is the first session\'s own')
})

test('placement: the last machine, a reached one, native; never broken or stopped, the host and the page alike (5.4)', () => {
  const machines = [{ name: 'native', state: 'ready' }, { name: 'docker', state: 'stopped' }, { name: 'lima', state: 'broken' }, { name: 'vm', state: 'ready' }]
  const s = U.storeRow(item([row(null, null), row('docker', 'ok'), row('lima', 'ok'), row('vm', 'ok')]), [])
  assert.equal(U.sm.pgDefaultMachine(s, machines), 'vm')
  assert.equal(U.sm.pgDefaultMachine(s, machines, 'docker'), 'vm', 'a stopped last machine is skipped')
  assert.equal(U.place.pgPlacement(machines, { last: 'lima', reached: U.place.reachedFrom(item([row('docker', 'ok'), row('vm', 'ok')])) }), 'vm')
  assert.equal(U.sm.pgDefaultMachine(U.storeRow(item([row(null, null)]), []), machines), 'native')
})

test('a Store row lists who connects, with the reach words and binding; the Machines view carries them too (V23-3, V23-5)', () => {
  const s = U.storeRow(item([row(null, 'ok'), row('demo-research', 'ok'), row('docker', 'failed', true, 'postgresql://10.0.0.5:6432/demo'), row('native2', null)]), [])
  assert.deepEqual(s.connectedFrom.map((c) => [c.machine, c.reached, c.own]), [[null, 'ok', false], ['demo-research', 'ok', false], ['docker', 'failed', true], ['native2', null, false]])
  assert.equal(s.connectedFrom[0].reach.latencyMs, 1)
  assert.equal(s.postgresql.binding, 'f64587b8a1c2d3e4')
  assert.deepEqual(s.ownConnections.map((c) => c.machine), ['docker'])
  assert.deepEqual(U.sm.connectedMachines(s), ['native', 'demo-research', 'docker', 'native2'])
  assert.deepEqual(U.pgStoreOf(item([row(null, 'ok'), row('vm', null)])).connectedFrom, [{ machine: 'native', reached: 'ok' }, { machine: 'vm', reached: null }])
  assert.equal(JSON.stringify(s).includes('password'), false)
})

test('the PostgreSQL tab groups by saved login, unused logins and No login included (V23-1)', () => {
  const st = (name, auth, state = 'ready', reason) => ({ ...U.storeRow(item([row(null, 'ok')], { store: name, auth, state, ...(reason ? { reason } : {}) }), []) })
  const groups = U.ms.loginGroups([st('a', 'demo-pg'), st('b', 'demo-pg', 'broken', 'outdated'), st('c', 'gone', 'broken', 'missing')], [{ auth: 'demo-pg', user: 'postgres', stores: ['a', 'b'] }, { auth: 'old', user: 'u', stores: [] }])
  assert.deepEqual(groups.map((g) => [g.auth, g.state, g.stores.map((s) => s.name)]), [['demo-pg', 'broken', ['a', 'b']], ['old', 'ready', []], [null, 'broken', ['c']]])
  assert.match(src('src/client/storage/MachineStores.tsx'), /loginActions\.rotate/)
  assert.match(src('src/client/storage/MachineStores.tsx'), /disabled=\{g\.stores\.length > 0\}/, 'Remove login only while no Store names it')
})

test('a new login is named after its server; Connect a database picks the server\'s saved login (V23-12)', () => {
  assert.equal(U.pgLoginName('postgresql://127.0.0.1:60506/x'), 'pg-127-0-0-1-60506')
  assert.equal(U.pgLoginName('postgresql://DB.lan/x'), 'pg-db-lan-5432')
  assert.equal(U.sm.pgServerLogin('db.lan:5432'), 'pg-db-lan-5432')
  const view = { stores: [U.storeRow(item([row(null, 'ok')], { store: 'a', auth: 'demo-pg', connections: [row(null, 'ok')] }), [])] }
  view.stores[0].postgresql.server = '127.0.0.1:60506'
  assert.equal(U.serverLogin(view, [{ auth: 'demo-pg', user: 'postgres', stores: ['a'] }], '127.0.0.1:60506'), 'demo-pg')
  assert.equal(U.serverLogin(view, [{ auth: 'demo-pg', user: 'postgres', stores: ['a'] }], 'other:5432'), null)
  assert.equal(U.serverLogin(view, [{ auth: 'demo-pg', user: null, stores: ['a'], state: 'broken' }], '127.0.0.1:60506'), null, 'never a broken login')
})

test('a URL carrying a login is refused, a password file too; client certificates pass (V23-13)', () => {
  for (const name of ['user', 'password', 'sslpassword', 'PassFile']) assert.equal(U.loginParameter(name), true, name)
  for (const name of ['sslkey', 'sslcert', 'sslmode']) assert.equal(U.loginParameter(name), false, name)
  assert.equal(U.PG_URL_LOGIN.test('postgresql://h/d?passfile=/p'), true)
  assert.equal(U.PG_URL_LOGIN.test('postgresql://h/d?sslkey=/k'), false)
})

test('the v23 fix lines are the card\'s one action: Save a login… for a missing login, Update for old fields', async () => {
  assert.deepEqual(U.doc.runnableFix('layerfs store save-auth demo-pg --user postgres --password-stdin && layerfs store update demo-experiments --auth demo-pg'), { kind: 'saveAuth', store: 'demo-experiments' })
  assert.deepEqual(U.doc.runnableFix('layerfs store update demo-experiments --auth demo-pg'), { kind: 'updateAuth', store: 'demo-experiments', auth: 'demo-pg' })
  const calls = []
  await U.fixCall({ call: async (...a) => { calls.push(a) } }, { kind: 'updateAuth', store: 'demo-experiments', auth: 'demo-pg' })
  assert.deepEqual(calls, [['POST', '/api/stores/demo-experiments/connection', { auth: 'demo-pg' }]])
  assert.equal(U.sm.needsLogin({ kind: 'postgresql', state: 'broken', reason: 'missing', postgresql: { auth: 'gone' } }), true)
  assert.equal(U.sm.needsLogin({ kind: 'postgresql', state: 'broken', reason: 'outdated', postgresql: { auth: 'demo-pg' } }), false, 'old fields are Update, not Save a login')
})

test('every new word is in both dictionaries', () => {
  const [en, zh] = [U.storageLocales.en, U.storageLocales.zh]
  for (const key of Object.keys(en).filter((k) => /^pg(Connected|ThisComputer|Rotate|RemoveLogin|Login|Forget|Database|Machines|Check|SavedAgo|UsedBy|NoLogin|Sandbox|OwnAddress|DefaultAddress|NotChecked|Reached|ConnectMachine|ChangeLogin|DisconnectMachine|From)/.test(k))) {
    assert.ok(zh[key] && zh[key] !== en[key], `${key} has a zh word`)
  }
  assert.ok(U.setupLocales.zh.pgPlacedPerSession)
})
