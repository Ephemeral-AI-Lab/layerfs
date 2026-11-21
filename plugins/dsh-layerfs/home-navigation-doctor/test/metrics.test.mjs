// Machine and Sandbox metrics (layerfs-project#182 D1–D3): the host's sampler
// (src/host/routes/metrics.ts), GET /api/metrics and the caches beside it, the
// page's poller and rate rules (src/client/machines/metrics.ts). Fake
// LayerFS answers and a hand-moved clock; no CLI is run.
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'
import { loadRoutes, testHost } from './routes-helper.mjs'

const root = fileURLToPath(new URL('..', import.meta.url))
const out = mkdtempSync(join(root, 'lib', '.test-metrics-'))
const bundle = async (name, contents) => {
  await build({
    absWorkingDir: root, stdin: { contents, resolveDir: root, loader: 'tsx' }, outfile: join(out, `${name}.mjs`), bundle: true, format: 'esm', platform: 'node', target: 'node24',
    jsx: 'automatic', loader: { '.module.css': 'local-css', '.css': 'css' }, external: ['@deepseek-ai/*', 'react', 'react/*', 'react-dom', 'react-dom/*'], logLevel: 'warning',
  })
  return import(pathToFileURL(join(out, `${name}.mjs`)).href)
}
const H = await bundle('sampler', "export * from './src/host/routes/metrics.ts'")
const C = await bundle('client-metrics', "export * from './src/client/machines/metrics.ts'")
const R = await loadRoutes()
test.after(() => rmSync(out, { recursive: true, force: true }))

class Refusal extends Error {
  constructor(code, message = code) { super(message); Object.assign(this, { code, transport: 'refusal' }) }
}

/** A clock a test moves by hand. */
function fakeClock(start = 1_000_000) {
  let now = start, id = 0
  const timers = new Map()
  return {
    now: () => now,
    setTimeout: (run, ms) => { timers.set(++id, { at: now + ms, run }); return id },
    clearTimeout: (timer) => { timers.delete(timer) },
    /** Move time on, running the timers that come due, oldest first. */
    async advance(ms) {
      const end = now + ms
      for (;;) {
        await settle()
        const due = [...timers.entries()].filter(([, t]) => t.at <= end).sort((a, b) => a[1].at - b[1].at)[0]
        if (!due) break
        timers.delete(due[0]); now = Math.max(now, due[1].at); due[1].run()
      }
      now = end
      await settle()
    },
  }
}
const settle = async () => { for (let i = 0; i < 20; i++) await Promise.resolve() }

// `machine stats --detail` (2.0): flat *_bytes and *_ms keys, `sandboxes` as rows keyed `sandbox`, times as ISO
const iso = (ms) => new Date(ms).toISOString()
const usage = (cpuMs, mem, more = {}) => ({ cpu_ms: cpuMs, memory_bytes: mem, memory_peak_bytes: mem * 2, memory_file_bytes: Math.round(mem / 4), pid_count: 3, oom_kill_count: 0, ...more })
const statsOf = (name, n, items, more = {}) => ({
  cpus: 4, memory_used_bytes: 2e9 + n, memory_total_bytes: 8e9, disk_free_bytes: 6e10, disk_total_bytes: 1e11, incomplete: false, disk_used_bytes: 4e10, reasons: [],
  machine: name, platform: 'lima', observed_at: iso(1_800_000_000_000 + n * 15_000), scope: 'vm', cpu_busy_ms: n * 3_000, cpu_total_ms: n * 60_000, worker: null,
  data_disk: { path: '/layerfs/data', fs: 'ext4', scope: 'volume', volume: 'v', inodes_free: 100 }, stores: [],
  ...(items ? { sandboxes: items } : {}), ...more,
})
const row = (name, n, more = {}) => ({ sandbox: name, live: true, live_reason: null, started_at: iso(1_799_000_000_000), usage: usage(n * 1_500, 1e8 + n), usage_reason: null, ...more })

/** A sampler over fake machines; `answers[name]` is called per read (may return a Promise). */
function sampler(machines, answers, clock = fakeClock()) {
  const log = { lists: 0, reads: [], ends: [] }
  const s = new H.MetricsSampler({
    clock,
    offered: async () => true,
    list: async () => { log.lists++; return machines() },
    stats: async (name) => { log.reads.push([name, clock.now()]); try { return await answers[name](log.reads.length) } finally { log.ends.push([name, clock.now()]) } },
  })
  return { s, log, clock }
}

// ---------------------------------------------------------------- D1: the sampler

test('sampler: one read at a time across machines, in turn; each machine 15 s after its last read ended', async () => {
  const gates = []
  const open = (name) => (n) => new Promise((resolve) => gates.push(() => resolve(statsOf(name, n, []))))
  const { s, log, clock } = sampler(() => [{ name: 'a', state: 'ready' }, { name: 'b', state: 'ready' }, { name: 'off', state: 'stopped' }], { a: open('a'), b: open('b') })
  s.start()
  await clock.advance(0)
  assert.deepEqual(log.reads.map((r) => r[0]), ['a'], 'b waits for a: one read in flight')
  assert.equal(s.reading, true)
  await clock.advance(4000)
  assert.deepEqual(log.reads.map((r) => r[0]), ['a'], 'nothing is queued beside a slow read')
  gates.shift()(); await clock.advance(0)
  assert.deepEqual(log.reads.map((r) => r[0]), ['a', 'b'])
  await clock.advance(1000)
  gates.shift()(); await clock.advance(0)
  const aEnd = log.ends[0][1], bEnd = log.ends[1][1]
  // a is due 15 s after its read ended (not after it began)
  await clock.advance(aEnd + H.TICK_MS - clock.now() - 1)
  assert.equal(log.reads.length, 2)
  await clock.advance(1)
  assert.deepEqual(log.reads.at(-1), ['a', aEnd + H.TICK_MS])
  gates.shift()(); await clock.advance(0)
  await clock.advance(bEnd + H.TICK_MS - clock.now())
  assert.deepEqual(log.reads.at(-1), ['b', bEnd + H.TICK_MS])
  assert.ok(!log.reads.some((r) => r[0] === 'off'), 'a stopped machine is never asked')
  assert.ok(log.lists <= Math.ceil((clock.now() - 1_000_000) / H.TICK_MS) + 1, 'the machine list at most once per tick')
  s.stop()
})

test('sampler: a refused read is a gap with its code, never retried; a timed-out machine skips its next turn', async () => {
  let mode = 'ok'
  const { s, log, clock } = sampler(() => [{ name: 'a', state: 'ready' }], {
    a: async (n) => { if (mode === 'ok') return statsOf('a', n, [row('x', n)]); throw new Refusal(mode, `${mode} from LayerFS`) },
  })
  s.start(); await clock.advance(0)
  mode = 'Busy'
  await clock.advance(H.TICK_MS)
  assert.equal(log.reads.length, 2)
  let v = s.view(new URLSearchParams())
  assert.deepEqual(v.machines.a.points.at(-1), { seq: 2, t: clock.now(), values: null, code: 'Busy', message: 'Busy from LayerFS' })
  assert.equal(v.sandboxes.x.points.at(-1).code, 'Busy', 'its Sandboxes get the same gap')
  assert.deepEqual(s.liveOf('x'), { live: null, liveReason: 'busy' })
  await clock.advance(H.TICK_MS - 1)
  assert.equal(log.reads.length, 2, 'no retry before its next turn')
  mode = 'Timeout'
  await clock.advance(1)
  assert.equal(log.reads.length, 3)
  const timedOut = clock.now()
  mode = 'ok'
  await clock.advance(H.TICK_MS)
  assert.equal(log.reads.length, 3, 'the turn after a timeout is skipped')
  await clock.advance(H.TICK_MS)
  assert.equal(log.reads.length, 4); assert.equal(log.reads.at(-1)[1], timedOut + 2 * H.TICK_MS)
  v = s.view(new URLSearchParams())
  assert.deepEqual(v.machines.a.points.map((p) => p.code ?? 'ok'), ['ok', 'Busy', 'Timeout', 'ok'], 'a skipped turn adds no point')
  s.stop()
})

test('sampler: a stopped machine gets machine_stopped gaps and is read at its next turn once it runs', async () => {
  let state = 'stopped'
  const { s, log, clock } = sampler(() => [{ name: 'vm', state }], { vm: async (n) => statsOf('vm', n, [row('x', n)]) })
  s.start(); await clock.advance(0)
  await clock.advance(3 * H.TICK_MS)
  assert.equal(log.reads.length, 0)
  const v = s.view(new URLSearchParams())
  assert.deepEqual(v.machines.vm.points.map((p) => [p.values, p.code]), [[null, 'machine_stopped'], [null, 'machine_stopped'], [null, 'machine_stopped'], [null, 'machine_stopped']])
  assert.equal(v.machines.vm.latest, null)
  state = 'ready'; s.relist()
  await clock.advance(H.TICK_MS)
  assert.equal(log.reads.length, 1)
  state = 'stopped'; s.relist()
  await clock.advance(H.TICK_MS)
  assert.equal(log.reads.length, 1)
  assert.equal(s.view(new URLSearchParams()).sandboxes.x.points.at(-1).code, 'machine_stopped')
  assert.deepEqual(s.liveOf('x'), { live: null, liveReason: 'machine_stopped' })
  s.stop()
})

test('sampler: 240 points per machine and per Sandbox; a closed Sandbox is kept 10 minutes, then dropped', async () => {
  let rows = (n) => [row('keep', n), row('goes', n)]
  const { s, clock } = sampler(() => [{ name: 'a', state: 'ready' }], { a: async (n) => statsOf('a', n, rows(n)) })
  s.start(); await clock.advance(0)
  await clock.advance(300 * H.TICK_MS)
  let v = s.view(new URLSearchParams())
  assert.equal(v.machines.a.points.length, H.RING); assert.equal(H.RING, 240)
  assert.equal(v.sandboxes.keep.points.length, 240); assert.equal(v.sandboxes.goes.points.length, 240)
  rows = (n) => [row('keep', n)]
  await clock.advance(H.TICK_MS)
  v = s.view(new URLSearchParams())
  const closed = v.sandboxes.goes.closedAt
  assert.equal(typeof closed, 'number', 'a Sandbox no longer in the rows is closed, its history kept')
  await clock.advance(H.CLOSED_KEEP_MS - H.TICK_MS)
  assert.ok(s.view(new URLSearchParams()).sandboxes.goes, 'still there before 10 minutes')
  await clock.advance(2 * H.TICK_MS)
  assert.equal(s.view(new URLSearchParams()).sandboxes.goes, undefined, 'dropped after 10 minutes')
  assert.equal(s.view(new URLSearchParams()).sandboxes.keep.closedAt, null)
  s.stop()
})

test('sampler: an answer without its Sandbox rows gives no Sandbox points; the view filters by machine and since', async () => {
  const { s, clock } = sampler(() => [{ name: 'old', state: 'ready' }, { name: 'new', state: 'ready' }], {
    old: async (n) => statsOf('old', n, undefined), new: async (n) => statsOf('new', n, [row('sb', n)]),
  })
  s.start(); await clock.advance(0); await clock.advance(0)
  let v = s.view(new URLSearchParams())
  assert.deepEqual(v.machines.old.latest.stats.sandboxes, { recorded: null, live: null, items: [] }, 'not read: no count, no rows')
  assert.deepEqual(v.machines.new.latest.stats.sandboxes.items.map((r) => r.name), ['sb'])
  assert.deepEqual(Object.keys(v.sandboxes), ['sb'])
  assert.deepEqual(s.liveOf('nothing'), {}, 'no point: no live')
  assert.deepEqual(s.liveOf('sb'), { live: true })
  const seq = v.seq
  await clock.advance(H.TICK_MS)
  v = s.view(new URLSearchParams(`machines=new&since=${seq}`))
  assert.deepEqual(Object.keys(v.machines), ['new'])
  assert.ok(v.machines.new.points.every((p) => p.seq > seq))
  assert.equal(v.since, seq)
  // a since from another host's numbering (dsh restarted) gets everything kept
  assert.equal(s.view(new URLSearchParams('since=999999')).since, 0)
  assert.equal(v.tick, 15000); assert.equal(v.ring, 240); assert.equal(v.epoch, 1_000_000)
  s.stop()
})

// ---------------------------------------------------------------- D2: routes and caches

const entry = (family, call, options = [], kind = 'text') => ({ family, call, args: [], options: options.map((name) => ({ name, kind })) })
const CALLS = [entry('machine', 'list'), entry('machine', 'stats'), entry('sandbox', 'list', ['page_size', 'page_pos', 'live'], 'bool'), entry('sandbox', 'get', ['page_size', 'page_pos']), entry('workspace', 'mount'), entry('machine', 'update')]

/** Route groups over a fake lfs that counts every call. */
function fixture(answers) {
  const calls = []
  const lfs = new Proxy({}, { get: (_, family) => new Proxy({}, { get: (__, call) => async (...args) => { calls.push(`${String(family)}.${String(call)}`); const f = answers[`${String(family)}.${String(call)}`]; return f ? f(...args) : {} } }) })
  const dir = mkdtempSync(join(tmpdir(), 'dsh-metrics-'))
  const t = testHost({ lfs, root: dir, home: join(dir, 'home'), discovery: async () => ({ identity: { version: '2.0.0-test', features: [] }, calls: CALLS }) })
  const groups = R.createGroups(t.host, async () => ({}))
  return { groups, calls, answers, http: (method, path, body) => R.handle(groups, { method, path, body }), close: () => { groups.machines.metrics.stop(); rmSync(dir, { recursive: true, force: true }) } }
}
const until = async (check, ms = 2000) => { const end = Date.now() + ms; while (!check()) { if (Date.now() > end) throw new Error('timed out'); await new Promise((r) => setTimeout(r, 5)) } }

test('sampler (U10 add-on 7): a Sandbox just made drops its earlier points; a read begun before it was made adds none, the next read does', async () => {
  let live = false
  // the read is held open while the create ends: it began before the Sandbox was made
  let release = null
  const { s, clock } = sampler(() => [{ name: 'm', state: 'ready' }], {
    m: (n) => (release === 'hold' ? new Promise((done) => { release = () => done(statsOf('m', n, [row('sb', n, { live })])) }) : statsOf('m', n, [row('sb', n, { live })])),
  })
  s.start(); await clock.advance(0); await clock.advance(0)
  assert.deepEqual(s.liveOf('sb'), { live: false }, 'during its create its daemon was not running')
  release = 'hold'
  await clock.advance(H.TICK_MS)
  assert.equal(typeof release, 'function', 'a read is under way')
  live = true
  s.created('sb')
  assert.deepEqual(s.liveOf('sb'), {}, 'made: no live until a read after it, so the card shows its record (never "Daemon not running")')
  live = false
  release(); await clock.advance(0)
  assert.deepEqual(s.liveOf('sb'), {}, 'the read begun before it was made adds nothing')
  release = null; live = true
  await clock.advance(H.TICK_MS)
  assert.deepEqual(s.liveOf('sb'), { live: true }, 'the next read counts')
  s.stop()
})

test('/api/metrics answers from memory with no LayerFS call; the stats route answers the sampler\'s values', async () => {
  const f = fixture({
    'machine.list': async () => ({ items: [{ machine: 'vm', platform: 'lima', state: 'ready' }] }),
    'machine.stats': async () => statsOf('vm', 1, [row('sb', 1)]),
  })
  try {
    f.groups.machines.metrics.start()
    await until(() => f.groups.machines.metrics.reads === 1 && !f.groups.machines.metrics.reading)
    f.groups.machines.metrics.stop()
    const before = f.calls.length
    for (let i = 0; i < 5; i++) {
      const r = await f.http('GET', '/api/metrics?machines=vm&since=0')
      assert.equal(r.status, 200)
      assert.equal(r.body.machines.vm.points.length, 1); assert.equal(r.body.sandboxes.sb.machine, 'vm')
      assert.equal(r.body.machines.vm.latest.stats.resources.cpuBusyUsec, 3_000_000)
    }
    const stats = (await f.http('GET', '/api/machines/vm/stats')).body
    assert.equal(stats.stale, false); assert.equal(stats.stats.resources.memoryTotal, 8e9)
    assert.deepEqual(stats.stats.sandboxes.items[0].usage, { cpuUsec: 1_500_000, memoryCurrent: 1e8 + 1, memoryPeak: 2e8 + 2, memoryFile: 25000000, pids: 3, oomKills: 0 })
    assert.deepEqual(f.calls.slice(before), [], 'neither route calls LayerFS')
  } finally { f.close() }
})

test('machines view: one answer serves every tab for 5 s, a write reads anew; sandbox.list is never asked with live, live comes from the sampler', async () => {
  const asked = []
  const f = fixture({
    'machine.list': async () => ({ items: [{ machine: 'vm', platform: 'lima', state: 'ready' }] }),
    'machine.stats': async () => statsOf('vm', 1, [row('up', 1), row('dead', 1, { live: false, usage: null, usage_reason: 'not_observed' })]),
    'sandbox.list': async (options) => { asked.push(options); return { items: [{ name: 'up', machine: 'vm', state: 'ready' }, { name: 'dead', machine: 'vm', state: 'ready' }, { name: 'new', machine: 'vm', state: 'ready' }], next_page_pos: null } },
  })
  const realNow = Date.now
  try {
    f.groups.machines.metrics.start()
    await until(() => f.groups.machines.metrics.reads === 1 && !f.groups.machines.metrics.reading)
    f.groups.machines.metrics.stop()
    const a = (await f.http('GET', '/api/machines')).body
    assert.equal(asked.length, 1); assert.ok(!('live' in asked[0]), 'never --live')
    assert.deepEqual(a.sandboxes.map((b) => [b.name, b.live, b.liveReason]), [['up', true, undefined], ['dead', false, undefined], ['new', undefined, undefined]])
    await f.http('GET', '/api/machines'); await f.http('GET', '/api/machines')
    assert.equal(asked.length, 1, 'other tabs within 5 s: the same answer')
    const t0 = realNow()
    Date.now = () => t0 + 5001
    await f.http('GET', '/api/machines')
    assert.equal(asked.length, 2, 'read again after 5 s')
    await f.http('POST', '/api/machines/vm/update', {})
    await f.http('GET', '/api/machines')
    assert.equal(asked.length, 3, 'a write reads anew at once')
    assert.equal(f.calls.filter((c) => c === 'sandbox.list').length, 3)
  } finally { Date.now = realNow; f.close() }
})

test('sandbox.get: one answer serves every tab for 10 s; a write on the Sandbox or a command of this host drops it', async () => {
  let gets = 0
  const f = fixture({
    'sandbox.get': async () => { gets++; return { name: 'sb', state: 'running', project: 's/p', machine: 'native', workspaces: [], next_page_pos: null } },
    'workspace.mount': async () => ({ ws: 'sb:w' }),
  })
  const realNow = Date.now
  try {
    await Promise.all([f.http('GET', '/api/sandboxes/sb'), f.http('GET', '/api/sandboxes/sb')])
    await f.http('GET', '/api/sandboxes/sb')
    assert.equal(gets, 1, 'two tabs on one Sandbox cost what one does')
    const t0 = realNow()
    Date.now = () => t0 + 9_000
    await f.http('GET', '/api/sandboxes/sb')
    assert.equal(gets, 1)
    Date.now = () => t0 + 10_001
    await f.http('GET', '/api/sandboxes/sb')
    assert.equal(gets, 2, 'asked again after 10 s')
    // the age counts from when the read began: a slow read does not stretch it (a page polling
    // every 5 s while a command runs gets a new answer every 10 s, not every 15 s)
    let release
    const slow = new Promise((r) => { release = r })
    f.answers['sandbox.get'] = async () => { gets++; await slow; return { name: 'sb', state: 'running', project: 's/p', machine: 'native', workspaces: [], next_page_pos: null } }
    Date.now = () => t0 + 20_002
    const pending = f.http('GET', '/api/sandboxes/sb')
    await new Promise((r) => setTimeout(r, 20))
    Date.now = () => t0 + 24_000
    release(); await pending
    assert.equal(gets, 3)
    Date.now = () => t0 + 30_003
    await f.http('GET', '/api/sandboxes/sb')
    assert.equal(gets, 4, '10 s after the read began, though it ended 6 s ago')
    await f.http('POST', '/api/sandboxes/sb/workspaces', { branch: 'side' })
    await f.http('GET', '/api/sandboxes/sb')
    assert.equal(gets, 5, 'a mount drops the kept answer')
    assert.equal(R.util.SANDBOX_KEEP_MS, 10_000)
  } finally { Date.now = realNow; f.close() }
})

// ---------------------------------------------------------------- D2: the page's poller

test('page poller: one GET per 15 s while visible, only newer points; a new machine reads everything now; a restarted host starts over', async () => {
  const clock = fakeClock()
  const asked = []
  let visible = true, epoch = 1, seq = 0
  const answer = (query) => {
    asked.push([query, clock.now()])
    seq++
    const p = { seq, t: clock.now(), values: { observedAt: clock.now(), cpus: 4, cpuBusyUsec: seq, cpuTotalUsec: seq * 10, memoryUsed: 1, memoryTotal: 2, diskUsed: 1, diskTotal: 2, reason: null } }
    return Promise.resolve({ epoch, seq, since: Number(new URLSearchParams(query).get('since')), now: clock.now(), tick: 15000, ring: 240, offered: true, machines: { a: { state: 'running', latest: null, points: [p] } }, sandboxes: {} })
  }
  const poller = new C.MetricsPoller(answer, () => visible, clock)
  const off = poller.watch('a'); await clock.advance(0)
  assert.deepEqual(asked.map((a) => a[0]), ['?machines=a&since=0'])
  await clock.advance(C.METRICS_POLL_MS - 1)
  assert.equal(asked.length, 1)
  await clock.advance(1)
  assert.deepEqual(asked.map((a) => a[0]), ['?machines=a&since=0', '?machines=a&since=1'])
  assert.equal(poller.getSnapshot().machines.a.points.length, 2)
  visible = false
  await clock.advance(5 * C.METRICS_POLL_MS)
  assert.equal(asked.length, 2, 'paused while hidden')
  visible = true; poller.wake(); await clock.advance(0)
  assert.equal(asked.length, 3)
  poller.watch('b'); await clock.advance(0)
  assert.deepEqual(asked.at(-1)[0], '?machines=a,b&since=0', 'a new machine: everything kept, at once')
  epoch = 2; seq = 0
  await clock.advance(C.METRICS_POLL_MS)
  assert.equal(poller.getSnapshot().epoch, 2)
  assert.equal(poller.getSnapshot().machines.a.points.length, 1, 'a restarted host: what was held is replaced')
  off(); poller.dispose()
})

// ---------------------------------------------------------------- D2: rates

const TICK = 15_000
const mp = (seq, t, busy, total, more = {}) => ({ seq, t, values: { observedAt: t, cpus: 4, cpuBusyUsec: busy, cpuTotalUsec: total, memoryUsed: 100 * seq, memoryTotal: 1000, diskUsed: 1, diskTotal: 2, reason: null, ...more } })
const gap = (seq, t, code) => ({ seq, t, values: null, code })

test('machine processor %: Δbusy/Δtotal between two good samples at most two ticks apart; each gap case', () => {
  const s = C.machineCpu([
    mp(1, 0, 0, 0),
    mp(2, 15_000, 3e6, 12e6), // 25 %
    gap(3, 30_000, 'Busy'),
    mp(4, 45_000, 6e6, 24e6), // previous is a gap
    mp(5, 60_000, 9e6, 36e6), // 25 %
    mp(6, 100_000, 12e6, 48e6), // 40 s apart: too far
    mp(7, 115_000, 1e6, 2e6), // busy went down: rebooted
    mp(8, 130_000, null, null, { reason: 'cpu_unsupported' }),
  ], TICK)
  assert.deepEqual(s.map((x) => [x.v, x.reason]), [
    [null, 'first_sample'], [0.25, undefined], [null, 'Busy'], [null, 'first_sample'], [0.25, undefined], [null, 'first_sample'], [null, 'restarted'], [null, 'cpu_unsupported'],
  ])
  // a build without the counters: no processor samples at all (the figure is hidden)
  const old = C.machineCpu([{ seq: 1, t: 0, values: { observedAt: 0, cpus: 4, memoryUsed: 1, memoryTotal: 2, diskUsed: 1, diskTotal: 2, reason: null } }], TICK)
  assert.deepEqual(old, [])
  assert.deepEqual(C.machineMemory([mp(1, 0, 0, 0), gap(2, 15_000, 'machine_stopped'), mp(3, 30_000, 0, 0, { memoryUsed: null, reason: 'memory_not_read' })]).map((x) => [x.v, x.reason]), [[100, undefined], [null, 'machine_stopped'], [null, 'memory_not_read']])
})

const sp = (seq, t, cpu, more = {}, usageMore = {}) => ({ seq, t, values: { observedAt: t, live: true, startedAt: 1, usage: { cpuUsec: cpu, memoryCurrent: 1000 + seq, memoryPeak: 5000, memoryFile: 10, pids: 2, oomKills: 0, ...usageMore }, ...more } })

test('Sandbox processor in processors\' worth: Δcpu_usec/Δobserved_at; restart, negative delta, distance and no usage are gaps; kills are marked', () => {
  const points = [
    sp(1, 0, 0),
    sp(2, 15_000, 19_500_000), // 1.3 processors
    sp(3, 30_000, 20_000_000, { startedAt: 60_000 }), // restarted (started_at moved by a minute)
    sp(4, 45_000, 10_000_000), // started_at back to 1 but counter went down
    sp(5, 60_000, 25_000_000, {}, { oomKills: 1 }),
    sp(6, 120_000, 26_000_000, {}, { oomKills: 3 }), // 60 s apart
    { seq: 7, t: 135_000, values: { observedAt: 135_000, live: true, startedAt: 1, usage: null, usageReason: 'host_process' } },
    gap(8, 150_000, 'Timeout'),
  ]
  assert.deepEqual(C.sandboxCpu(points, TICK).map((x) => [x.v, x.reason]), [
    [null, 'first_sample'], [1.3, undefined], [null, 'restarted'], [null, 'restarted'], [1, undefined], [null, 'first_sample'], [null, 'host_process'], [null, 'Timeout'],
  ])
  assert.deepEqual(C.sandboxMemory(points).map((x) => x.v ?? x.reason), [1001, 1002, 1003, 1004, 1005, 1006, 'host_process', 'Timeout'])
  assert.deepEqual(C.oomMarks(points), [{ t: 60_000, kills: 1 }, { t: 120_000, kills: 2 }])
  assert.deepEqual(C.oomMarks([sp(1, 0, 0, {}, { oomKills: 5 }), sp(2, 15_000, 1, { startedAt: 9_000_000 }, { oomKills: 0 })]), [], 'a restarted Sandbox counts from its new start')
  // a Linux start read through a corrected VM clock moves by a second or two: the same run (Lima, real runs)
  const drift = [sp(1, 0, 0, { startedAt: 100_000 }), sp(2, 15_000, 15_000_000, { startedAt: 99_000 }), sp(3, 30_000, 30_000_000, { startedAt: 101_000 }), sp(4, 45_000, 45_000_000, { startedAt: 104_000 })]
  assert.deepEqual(C.sandboxCpu(drift, TICK).map((x) => [x.v, x.reason]), [[null, 'first_sample'], [1, undefined], [1, undefined], [null, 'restarted']])
  assert.ok(C.sameRun(1, 2_001) && !C.sameRun(1, 2_002) && C.sameRun(null, null) && !C.sameRun(null, 5))
  assert.equal(C.processors(1.3), '1.3'); assert.equal(C.processors(0.05), '0.05'); assert.equal(C.percent(0.25), '25 %')
  assert.equal(C.upTime(0, 3_725_000), '1 h 2 min'); assert.equal(C.upTime(null, 1), '—')
})

test('the Sandbox list by memory; a gap or unknown memory goes last; figures read the newest row', () => {
  const snap = {
    loaded: true, offered: true, epoch: 1, now: 30_000, tick: TICK, error: null,
    machines: { vm: { state: 'running', latest: { at: 1, stats: { resources: { cpus: 4 }, sandboxes: { items: [] } } }, points: [gap(9, 30_000, 'Busy')] } },
    sandboxes: {
      small: { machine: 'vm', closedAt: null, points: [sp(1, 0, 0, {}, { memoryCurrent: 10 }), sp(2, 15_000, 15_000_000, {}, { memoryCurrent: 20 })] },
      big: { machine: 'vm', closedAt: null, points: [sp(1, 0, 0, {}, { memoryCurrent: 900 }), sp(2, 15_000, 0, {}, { memoryCurrent: 999 })] },
      unread: { machine: 'vm', closedAt: null, points: [{ seq: 1, t: 0, values: { observedAt: 0, live: true, startedAt: 1, usage: { cpuUsec: 1, memoryCurrent: null, memoryPeak: null, pids: null }, usageReason: 'no_controller' } }] },
      other: { machine: 'elsewhere', closedAt: null, points: [sp(1, 0, 0)] },
    },
  }
  assert.deepEqual(C.sandboxTable(snap, 'vm').map((r) => r.name), ['big', 'small', 'unread'])
  assert.equal(C.sandboxTable(snap, 'vm')[1].cpu.v, 1)
  const f = C.sandboxFigures(snap, 'small')
  assert.equal(f.cpus, 4); assert.equal(f.row.usage.memoryCurrent, 20); assert.equal(f.gap, null)
  const read = C.statsReadOf(snap, 'vm')
  assert.equal(read.stale.code, 'Busy', 'the newest point is a gap: the last values, marked stale')
  assert.equal(C.statsReadOf({ ...snap, loaded: false }, 'vm'), undefined)
})

test('page poller: a machine watched while a poll runs is read as soon as it ends', async () => {
  const clock = fakeClock()
  const asked = []
  let release
  const poller = new C.MetricsPoller((query) => { asked.push(query); return new Promise((resolve) => { release = () => resolve({ epoch: 1, seq: 0, since: 0, now: clock.now(), tick: 15000, ring: 240, offered: true, machines: {}, sandboxes: {} }) }) }, () => true, clock)
  poller.watch('a'); await clock.advance(0)
  poller.watch('b'); await clock.advance(0)
  assert.equal(asked.length, 1, 'one poll in flight')
  release(); await clock.advance(0)
  assert.deepEqual(asked, ['?machines=a&since=0', '?machines=a,b&since=0'])
  release(); await clock.advance(C.METRICS_POLL_MS - 1)
  assert.equal(asked.length, 2, 'then back to one per 15 s')
  poller.dispose()
})
