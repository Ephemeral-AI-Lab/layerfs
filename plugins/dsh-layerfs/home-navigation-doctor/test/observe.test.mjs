// Observed values (layerfs-project#164), host and page side, over a fake `lfs`
// (machine.stats, machine.log, sandbox.list --live, sandbox.get daemon and usage):
//   host    featuresOf, the inspect lane, GET /api/machines/:id/stats (single
//           flight, one answer for 5 s, stale answers, 501 without the call),
//           the machines view's `live`, sandbox.get's fields, the machine log route
//   page    the stats poller (src/client/machines/stats.ts), what a stopped
//           machine and a null value read as, live counts, usage words
//           (leftovers are on no page: LayerFS removes them by itself)
//   node --test test/observe.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'
import { loadRoutes, testHost } from './routes-helper.mjs'

const root = fileURLToPath(new URL('..', import.meta.url))
const out = mkdtempSync(join(root, 'lib', '.test-observe-'))
const bundle = async (name, contents) => {
  await build({ absWorkingDir: root, stdin: { contents, resolveDir: root, loader: 'ts' }, outfile: join(out, `${name}.mjs`), bundle: true, format: 'esm', platform: 'node', target: 'node24', external: ['@deepseek-ai/*', 'react', 'react/*'], logLevel: 'warning' })
  return import(pathToFileURL(join(out, `${name}.mjs`)).href)
}
const S = await bundle('stats', "export * from './src/client/machines/stats.ts'")
const R = await loadRoutes()
test.after(() => rmSync(out, { recursive: true, force: true }))

class Refusal extends Error {
  constructor(code, message = code, detail = null) { super(message); Object.assign(this, { code, detail, transport: 'refusal' }) }
}
const entry = (family, call, options = [], kind = 'text') => ({ family, call, args: [], options: options.map((name) => ({ name, kind })) })
const BASE = [entry('machine', 'list'), entry('sandbox', 'list', ['page_size', 'page_pos'])]
/** The merged build's table, as far as these routes read it. */
const OBSERVE = [entry('machine', 'list'), entry('machine', 'stats'), entry('sandbox', 'list', ['page_size', 'page_pos', 'live'], 'bool')]

function fixture(lfs, calls = OBSERVE) {
  const dir = mkdtempSync(join(tmpdir(), 'dsh-observe-'))
  const t = testHost({ lfs, root: dir, home: join(dir, 'home'), discovery: async () => ({ identity: { version: '2.0.0-test', features: [] }, calls }) })
  const groups = R.createGroups(t.host, async () => ({}))
  return { groups, logs: t.logs, http: (method, path, body) => R.handle(groups, { method, path, body }), close: () => rmSync(dir, { recursive: true, force: true }) }
}
const viewLfs = (machine = {}, sandbox = {}) => ({
  machine: { list: async () => ({ items: [{ machine: 'native', platform: 'native', state: 'ready', supports: ['sqlite'], stores: [], sandboxes: [] }] }), ...machine },
  home: { get: async () => ({ state: 'ready', platforms: { host: 'macos', items: [] } }) },
  store: { list: async () => ({ items: [] }) },
  sandbox: { list: async () => ({ items: [], next_page_pos: null }), ...sandbox },
})

// The answers as the merged build gives them (docs/architecture/api/sdk/machine.md, sandbox.md).
const NATIVE = { machine: 'native', platform: 'native', observed_at: 1794776707857, worker: null, resources: { cpus: 14, memory_total: 38654705664, memory_used: 31340068864, disk_total: 994662584320, disk_used: 641705943040, disk_free: 352956641280, scope: 'host' }, data_disk: { path: '/Users/me/.layerfs', fs: 'apfs', scope: 'host_fs', volume: null, inodes_free: 3446842200 }, stores: [{ name: 'shop', bytes: 217088, allocated: 217088 }], sandboxes: { recorded: 12, live: 10 }, complete: true }

// ---------------------------------------------------------------- host: features and the lane

test('features: machine.stats and its --with space are each read from the call table (2.0 has no sandbox list live)', () => {
  const { featuresOf } = R.util
  const none = featuresOf({ calls: BASE })
  assert.equal(none.inspect.machineStats, false); assert.equal(none.machineSpace, false)
  const all = featuresOf({ calls: OBSERVE })
  assert.equal(all.inspect.machineStats, true); assert.equal(all.machineSpace, false); assert.equal(all.sandboxLive, undefined)
  // `--with space` is a value of machine.stats' 2.0 `form.with`
  assert.equal(featuresOf({ calls: [{ ...entry('machine', 'stats'), form: { with: { space: {} } } }] }).machineSpace, true)
  // an option of another call, or the call without the option, is not the feature
  assert.equal(featuresOf(null).inspect.machineStats, false)
  assert.ok(!R.cli.REQUIRED_CALLS.includes('machine.stats'), 'machine.stats is optional')
})

test('machine.stats is a monitoring read: the inspect lane, whatever lane the caller names', () => {
  assert.ok(R.cli.INSPECT_CALLS.has('machine.stats'))
  assert.deepEqual(R.cli.laneOf(['machine', 'stats', '--', 'docker']), R.cli.INSPECT_LANE)
  // store.stats and machine.get stay where they were
  assert.equal(R.cli.laneOf(['store', 'stats', '--', 's']).name, 'calls')
  assert.equal(R.cli.laneOf(['machine', 'get', '--', 'docker']).name, 'probes')
  assert.ok(!R.cli.KEPT_READS.has('machine.stats'), 'the SDK keeps no monitoring answer: the route does, per machine')
})

// ---------------------------------------------------------------- host: GET /api/machines/:id/stats

test('stats route: 501 without machine.stats, and no call', async () => {
  let calls = 0
  const f = fixture({ ...viewLfs({ stats: async () => { calls++; return NATIVE } }) }, BASE)
  try {
    const r = await f.http('GET', '/api/machines/native/stats')
    assert.equal(r.status, 501); assert.equal(r.body.code, 'Unsupported'); assert.equal(r.body.detail.need, 'machine.stats')
    assert.equal(calls, 0)
    assert.deepEqual(f.logs, [], 'an expected refusal logs no stack')
    assert.equal((await f.http('POST', '/api/machines/native/stats', {})).status, 404, 'only GET')
  } finally { f.close() }
})

// ---------------------------------------------------------------- host: the machines view, sandbox.get, doctor

test('sandbox.get: daemon, usage and usage_reason by presence', async () => {
  // 2.0 `sandbox get --detail --with stats`: the process facts in `stats` (null when nothing was observed); usage in cpu_ms, bytes, pid_count
  const record = (stats) => ({ sandbox: 'sb', project: 's/p', machine: 'native', workspaces: [], ...(stats === undefined ? {} : { stats }) })
  const iso = (ms) => new Date(ms).toISOString()
  const get = async (answer) => {
    const f = fixture({ ...viewLfs(), sandbox: { get: async () => answer } })
    try { return (await f.http('GET', '/api/sandboxes/sb')).body } finally { f.close() }
  }
  const old = await get(record(undefined))
  for (const key of ['daemon', 'usage', 'usageReason']) assert.ok(!(key in old), `${key} is absent without the stats section`)
  const host = await get(record({ daemon: { pid: 4242, started_at: iso(1794776000000) }, usage: null, usage_reason: 'host_process' }))
  assert.deepEqual(host.daemon, { pid: 4242, startedAt: 1794776000000 }); assert.equal(host.usage, null); assert.equal(host.usageReason, 'host_process')
  const ns = await get(record({ daemon: { pid: 77, started_at: null }, usage: { cpu_ms: 1234, memory_bytes: 34000000, memory_peak_bytes: null, pid_count: 3 } }))
  assert.deepEqual(ns.daemon, { pid: 77, startedAt: null }, 'a process that has ended has no start time')
  assert.deepEqual(ns.usage, { cpuUsec: 1234000, memoryCurrent: 34000000, memoryPeak: null, pids: 3 }); assert.ok(!('usageReason' in ns))
  // a usage whose memory and process counts are null comes with the reason beside it
  const bare = await get(record({ daemon: { pid: 7, started_at: iso(1794776000000) }, usage: { cpu_ms: 205, memory_bytes: null, memory_peak_bytes: null, pid_count: null }, usage_reason: 'no_controller' }))
  assert.deepEqual([bare.usage, bare.usageReason], [{ cpuUsec: 205000, memoryCurrent: null, memoryPeak: null, pids: null }, 'no_controller'])
  const unseen = await get(record({ daemon: null, usage: null, usage_reason: 'not_observed' }))
  assert.equal(unseen.daemon, null); assert.equal(unseen.usageReason, 'not_observed')
  const notReady = await get(record(null))
  assert.deepEqual([notReady.daemon, notReady.usage], [null, null], 'a null section: nothing observed')
})

test('log route: machine.log pages backwards only (2.0): the tail, then the lines before its next_cursor; a forward cursor is refused', async () => {
  const LOGS = [...OBSERVE, entry('machine', 'log', ['limit', 'cursor'])]
  const asked = []
  const f = fixture({ ...viewLfs({ log: async (_name, options) => {
    asked.push({ cursor: options.cursor ?? null, detail: options.detail === true })
    if (!options.cursor) return { items: [{ text: 'worker started', continued: false }, { text: 'ready', continued: false }], next_cursor: 'c1.0.2a.9f.40' }
    return { items: [{ text: 'boot' }], next_cursor: null }
  } }) }, LOGS)
  try {
    const tail = (await f.http('GET', '/api/machines/docker/log')).body
    assert.deepEqual([tail.lines.map((l) => l.text), tail.cursor, tail.before, tail.hasMore, tail.stale], [['worker started', 'ready'], null, 'c1.0.2a.9f.40', true, false])
    const earlier = (await f.http('GET', '/api/machines/docker/log?before=c1.0.2a.9f.40')).body
    assert.deepEqual([earlier.lines.map((l) => l.text), earlier.before, earlier.hasMore], [['boot'], null, false])
    assert.deepEqual(asked, [{ cursor: null, detail: true }, { cursor: 'c1.0.2a.9f.40', detail: true }])
    // the kept tail is the tail read
    assert.deepEqual(f.groups.machines.reads.last('log:machine:docker').value.lines.map((l) => l.text), ['worker started', 'ready'])
    // today's forward cursor is gone: refused, and nothing is asked
    const forward = await f.http('GET', '/api/machines/docker/log?cursor=c1.0.2a.9f.40')
    assert.equal(forward.status, 400)
    assert.equal(asked.length, 2)
  } finally { f.close() }
})

const stats = (n) => ({ platform: 'docker', observedAt: n, worker: null, resources: { cpus: n, memoryTotal: 1, memoryUsed: 1, diskTotal: 10, diskUsed: n, diskFree: 1, scope: 'vm', reason: null }, dataDisk: { path: '/', fs: 'ext4', scope: 'volume', volume: null, inodesFree: 1 }, stores: [], sandboxes: { recorded: 0, live: 0 }, complete: true, reason: null })

// ---------------------------------------------------------------- page: words

test('a stopped machine is never asked and reads "not observed (machine stopped)"; scopes and reasons in plain words', () => {
  const stopped = { name: 'dev', platform: 'lima', state: 'stopped' }
  assert.equal(S.statsAskable(stopped), false)
  assert.equal(S.statsAskable({ platform: 'native', state: 'ready' }), true, 'native answers in the caller\'s process')
  assert.equal(S.statsAskable({ platform: 'docker', state: 'ready' }), true)
  assert.equal(S.statsAskable({ platform: 'docker', state: 'broken' }), false)
  assert.deepEqual(S.notObserved(stopped, undefined), { key: 'notObservedStopped' })
  // even with values kept from before it stopped
  assert.deepEqual(S.notObserved(stopped, { stats: stats(1), at: 1, stale: null, reading: false, loaded: true }), { key: 'notObservedStopped' })
  assert.deepEqual(S.notObserved({ platform: 'lima', state: 'busy' }, undefined), { key: 'notObservedState', values: { state: 'busy' } })
  const running = { platform: 'docker', state: 'ready' }
  assert.deepEqual(S.notObserved(running, undefined), { key: 'notObservedYet' })
  assert.equal(S.notObserved(running, { stats: stats(1), at: 1, stale: { at: 1, code: 'Busy', message: 'm' }, reading: false, loaded: true }), null, 'values with a refusal are still values')
  assert.deepEqual(S.notObserved(running, { stats: null, at: null, stale: { at: null, code: 'Unavailable', message: 'm' }, reading: false, loaded: true }), { key: 'notObservedUnavailable' })
  assert.deepEqual(S.notObserved(running, { stats: null, at: null, stale: { at: null, code: 'Busy', message: 'two reads run' }, reading: false, loaded: true }), { key: 'notObservedRefused', values: { code: 'Busy', message: 'two reads run' } })
  assert.deepEqual([S.diskScopeWords('volume').key, S.diskScopeWords('vm_disk').key, S.diskScopeWords('host_fs').key], ['scopeVolume', 'scopeVmDisk', 'scopeHostFs'])
  assert.deepEqual([S.resourceScopeWords('host').key, S.resourceScopeWords('engine_host').key, S.resourceScopeWords('vm').key], ['scopeHost', 'scopeEngineHost', 'scopeVm'])
  assert.deepEqual(S.diskScopeWords('lun'), { key: 'raw', values: { text: 'lun' } }, 'a scope this page does not know is shown as it came')
  assert.equal(S.diskScopeWords(null), null)
  assert.deepEqual(S.reasonList('memory_not_read, disk_not_read').map((w) => w.key), ['reasonMemoryNotRead', 'reasonDiskNotRead'])
  assert.deepEqual(S.reasonList(null), [])
  assert.deepEqual(S.reasonWords('host_process'), { key: 'reasonHostProcess' })
  assert.deepEqual(S.reasonWords('something_new'), { key: 'raw', values: { text: 'something new' } })
  assert.deepEqual([S.staleWords({ at: 1, code: 'Timeout', message: '' }).key, S.staleWords({ at: 1, code: 'Busy', message: '' }).key, S.staleWords({ at: 1, code: 'Unknown', message: 'x' }).key], ['statsStaleTimeout', 'statsStaleBusy', 'statsStaleRefused'])
})

test('a Sandbox whose daemon is dead is not running; one that was not observed is not confirmed', () => {
  // 2.0: a Sandbox recorded `ready` is judged by its daemon (`live`, the sampler's)
  const look = (b) => ({ tone: b.state === 'ready' ? 'success' : 'neutral', label: `state:${b.state}` })
  const tw = (words) => `${words.key}${words.values ? `(${Object.values(words.values).join(',')})` : ''}`
  const up = { state: 'ready', live: true }, dead = { state: 'ready', live: false }, unseen = { state: 'ready', live: null, liveReason: 'machine_stopped' }, old = { state: 'ready' }, closing = { state: 'busy', live: false }
  assert.deepEqual([up, dead, unseen, old, closing].map(S.sandboxRuns), [true, false, true, true, false])
  assert.deepEqual(S.sandboxLook(up, look), { tone: 'success', label: 'state:ready' })
  assert.deepEqual(S.sandboxLook(old, look), { tone: 'success', label: 'state:ready' }, 'without `live` the record\'s state shows, as before')
  assert.deepEqual(S.sandboxLook(dead, look), { tone: 'danger', label: 'stateDaemonDown', attention: true })
  assert.deepEqual(S.sandboxLook(unseen, look), { tone: 'neutral', label: 'state:ready', dashed: true })
  assert.deepEqual(S.sandboxLook(closing, look), { tone: 'neutral', label: 'state:busy' }, 'only a Sandbox recorded ready is judged by its daemon')
  assert.equal(S.liveNote(up, tw), null); assert.equal(S.liveNote(old, tw), null)
  assert.equal(S.liveNote(dead, tw), 'liveDown')
  assert.equal(S.liveNote(unseen, tw), 'liveUnknownBecause(reasonMachineStopped)')
  assert.equal(S.liveNote({ state: 'ready', live: null }, tw), 'liveUnknown')
  assert.deepEqual(S.liveCounts([up, up, dead, unseen, closing]), { total: 5, running: 2, down: 1, unknown: 1, observed: true })
  assert.deepEqual(S.liveCounts([old, old, { state: 'broken', reason: 'lost' }]), { total: 3, running: 2, down: 0, unknown: 0, observed: false }, 'an older CLI: the records\' count, as before')
  assert.deepEqual(S.liveCounts([]), { total: 0, running: 0, down: 0, unknown: 0, observed: false })
})

test('CPU time in words', () => {
  assert.deepEqual([0, 250_000, 1_234_567, 42_000_000, 184_000_000, 7_500_000_000].map(S.cpuTime), ['0.00 s', '0.25 s', '1.2 s', '42 s', '3 min 4 s', '2 h 5 min'])
  assert.equal(S.cpuTime(null), '—')
})
