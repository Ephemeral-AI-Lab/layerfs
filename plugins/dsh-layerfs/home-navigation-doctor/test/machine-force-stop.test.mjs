import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'
import { loadRoutes, testHost } from './routes-helper.mjs'

const R = await loadRoutes()
const root = fileURLToPath(new URL('..', import.meta.url))
const out = mkdtempSync(join(root, 'lib', '.test-force-stop-'))
await build({ absWorkingDir: root, stdin: { contents: "export { MachinesController } from './src/client/machines/controller.ts'; export { forceStopOf } from './src/client/machines/model.ts'", resolveDir: root, loader: 'ts' }, outfile: join(out, 'controller.mjs'), bundle: true, format: 'esm', platform: 'node', target: 'node24', logLevel: 'warning' })
const { MachinesController, forceStopOf } = await import(pathToFileURL(join(out, 'controller.mjs')).href)
test.after(() => rmSync(out, { recursive: true, force: true }))

function fixture(lfs) {
  const dir = mkdtempSync(join(tmpdir(), 'dsh-force-stop-'))
  const { host } = testHost({ lfs, root: dir, home: join(dir, 'home'), discovery: async () => ({ identity: {}, calls: [{ family: 'machine', call: 'stop', args: [], options: [{ name: 'force', kind: 'bool' }] }] }) })
  const groups = R.createGroups(host, async () => ({}))
  return { host, groups, close: () => rmSync(dir, { recursive: true, force: true }) }
}

test('machine Stop keeps normal SDK call; Force stop forwards force and invalidates only after success', async () => {
  const calls = []
  // the 2.0 long form of machine.stop; the SDK options are the long form, the progress callback and (Force stop) force
  const receipt = { ok: true, outcome: 'stopped', machine: 'machine-a', forced: false }
  const opts = (o) => Object.fromEntries(Object.entries(o).map(([k, v]) => [k, typeof v === 'function' ? 'fn' : v]))
  const f = fixture({ machine: { stop: async (name, o) => { calls.push(['stop', name, opts(o)]); return receipt } } })
  f.host.machinesStopped = async (names) => { calls.push(['invalidate', names]) }
  try {
    assert.deepEqual((await R.handle(f.groups, { method: 'POST', path: '/api/machines/machine-a/stop' })).body, receipt)
    assert.deepEqual(calls, [['stop', 'machine-a', { detail: true, on_progress: 'fn' }], ['invalidate', ['machine-a']]])
    calls.length = 0
    assert.deepEqual((await R.handle(f.groups, { method: 'POST', path: '/api/machines/machine-a/stop', body: { force: true } })).body, receipt)
    assert.deepEqual(calls, [['stop', 'machine-a', { force: true, detail: true, on_progress: 'fn' }], ['invalidate', ['machine-a']]])
    calls.length = 0
    f.host.lfs = () => ({ machine: { stop: async () => { throw Object.assign(new Error('still running'), { code: 'InUse' }) } } })
    const refused = await R.handle(f.groups, { method: 'POST', path: '/api/machines/machine-a/stop', body: { force: true } })
    assert.equal(refused.status, 409)
    assert.deepEqual(calls, [], 'a refusal must not report a confirmed lifecycle change')
  } finally { f.close() }
})

test('Force stop is refused, not sent, when the pinned CLI has no machine.stop force option', async () => {
  const calls = []
  const f = fixture({ machine: { stop: async (...args) => { calls.push(['stop', ...args]); return { kind: 'Stopped' } } } })
  f.host.discover = async () => ({ identity: {}, calls: [{ family: 'machine', call: 'stop', args: [], options: [] }] })
  try {
    const refused = await R.handle(f.groups, { method: 'POST', path: '/api/machines/machine-a/stop', body: { force: true } })
    assert.equal(refused.status, 501)
    assert.equal(refused.body.code, 'Unsupported')
    assert.deepEqual(calls, [])
    assert.equal((await R.handle(f.groups, { method: 'POST', path: '/api/machines/machine-a/stop' })).status, 200, 'a normal stop still works')
  } finally { f.close() }
})

test('forced Sandbox removal bypasses session checkpointing and preserves the actual receipt', async () => {
  const calls = []
  // the 2.0 long form of `sandbox remove --force`: `unmounted` names the Workspaces the forced removal ended
  const receipt = { ok: true, outcome: 'removed', sandbox: 'session-box', unmounted: ['session-box:with-lost-reply'] }
  const f = fixture({ sandbox: { remove: async (...args) => { calls.push(['remove', ...args]); return receipt } } })
  f.host.closeSessionSandbox = async () => { calls.push(['save-first']); return true }
  f.host.sandboxClosed = async (name) => { calls.push(['invalidate', name]) }
  try {
    const forced = await R.handle(f.groups, { method: 'DELETE', path: '/api/sandboxes/session-box?force=1' })
    assert.deepEqual(forced.body, receipt)
    assert.deepEqual(calls, [['remove', 'session-box', { detail: true, force: true }], ['invalidate', 'session-box']])
    calls.length = 0
    const normal = await R.handle(f.groups, { method: 'DELETE', path: '/api/sandboxes/session-box' })
    assert.equal(normal.body.session, true)
    assert.deepEqual(calls, [['save-first']], 'normal session close still saves before closing')
  } finally { f.close() }
})

test('machine controller sends force only for the explicit Force stop action', async () => {
  const calls = []
  const controller = new MachinesController({
    connected: () => true,
    onEvent: () => () => {},
    call: async (...args) => { calls.push(args); return {} },
  })
  try {
    await controller.face().act({ kind: 'stop', name: 'machine-a' })
    await controller.face().act({ kind: 'stop', name: 'machine-a', force: true })
    const stops = calls.filter((call) => call[0] === 'POST')
    assert.deepEqual(stops, [
      ['POST', '/api/machines/machine-a/stop', undefined],
      ['POST', '/api/machines/machine-a/stop', { force: true }],
    ])
  } finally { controller.dispose() }
})


test('Force stop remains accessible for native and stopped machines with registered Sandboxes', () => {
  const machine = (platform, state, sandboxes = []) => ({ platform, state, sandboxes })
  assert.equal(forceStopOf(machine('native', 'ready', ['local-runtime'])), true)
  assert.equal(forceStopOf(machine('lima', 'stopped', ['stale-runtime'])), true)
  assert.equal(forceStopOf(machine('docker', 'unknown'), true), true, 'global inventory may identify a registered runtime absent from machine.list')
  assert.equal(forceStopOf(machine('lima', 'ready')), true)
  assert.equal(forceStopOf(machine('lima', 'stopped')), false)
  assert.equal(forceStopOf(machine('ssh', 'ready', ['unsupported'])), false)
  assert.equal(forceStopOf(machine('lima', 'ready'), true, { forceStop: false, images: false, imageArchive: false }), false, 'a CLI without force hides it')
  assert.equal(forceStopOf(machine('lima', 'ready'), true, { forceStop: true, images: false, imageArchive: false }), true)
})
