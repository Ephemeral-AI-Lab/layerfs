// The stand-in CLI for the observed values and the Doctor leftovers
// (test/fixtures/cli-inspect-observe.mjs): it must answer with the shapes of
// the merged build (layerfs-project#164, #180), or the UI checks run against
// it check nothing. Never evidence about a real build.
//   node --test test/observe-standin.test.mjs
import { fileURLToPath } from 'node:url'
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { chmodSync, existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

const shim = fileURLToPath(new URL('./fixtures/cli-inspect-observe.mjs', import.meta.url))

/** A fixture folder and a last CLI that answers as a CLI before #164 does, recording what it was asked. */
function fixture(config) {
  const dir = mkdtempSync(join(tmpdir(), 'dsh-observe-standin-'))
  const last = join(dir, 'last-cli.mjs')
  writeFileSync(last, `#!/usr/bin/env node
import { appendFileSync } from 'node:fs'
const a = process.argv.slice(2)
appendFileSync(${JSON.stringify(join(dir, 'last.log'))}, JSON.stringify(a) + '\\n')
const out = (v) => console.log(JSON.stringify(v))
if (a[0] === 'help') out({ calls: [{ family: 'sandbox', call: 'list', args: [], options: [{ name: 'page_size', kind: 'int' }] }, { family: 'machine', call: 'list', args: [], options: [] }] })
else if (a[0] === 'machine' && a[1] === 'list') out({ machines: [{ name: 'native', platform: 'native', state: 'ready', sandboxes: ['a', 'b'] }, { name: 'docker', platform: 'docker', state: 'stopped', sandboxes: [] }] })
else if (a[0] === 'machine' && a[1] === 'get') out({ name: a.at(-1), platform: 'docker', state: 'stopped' })
else if (a[0] === 'machine' && a[1] === 'status') out({ name: a.at(-1), platform: 'docker', state: 'stopped', free_disk: null, tool_version: '28', down_reason: 'the machine is stopped', stores: [] })
else if (a[0] === 'sandbox' && a[1] === 'list') { if (a.some((x) => x.startsWith('--live'))) { console.error('layerfs: unknown option --live'); process.exit(2) } out({ items: [{ name: 'a', machine: 'native', state: 'running' }, { name: 'b', machine: 'native', state: 'running' }, { name: 'c', machine: 'native', state: 'closing' }], next_page_pos: null }) }
else if (a[0] === 'sandbox' && a[1] === 'get') out({ name: a.at(-1), state: 'running', created: 1794781675311, workspaces: [], daemon_capacity: {} })
else if (a[0] === 'doctor' && a.length === 1) { out({ ok: true, findings: [{ severity: 'warning', entity: { kind: 'store', name: 's' }, message: 'm', fix: 'f' }] }); process.exit(1) }
else { console.error(JSON.stringify({ error: 'NotFound', message: 'no machine named ' + a.at(-1) })); process.exit(125) }
`)
  chmodSync(last, 0o755)
  const write = (c) => writeFileSync(join(dir, 'observe.json'), JSON.stringify(c))
  if (config) write(config)
  const run = (...args) => {
    const r = spawnSync(process.execPath, [shim, ...args], { encoding: 'utf8', timeout: 20000, env: { ...process.env, LFS_INSPECT_DIR: dir, LFS_REAL_CLI: last, LFS_REAL_CLI_REST: '' } })
    let out = null, err = null
    try { out = JSON.parse(r.stdout) } catch { /* not JSON */ }
    try { err = JSON.parse(r.stderr.trim().split('\n').at(-1)) } catch { /* not JSON */ }
    return { status: r.status, out, err, stderr: r.stderr }
  }
  const asked = () => { try { return readFileSync(join(dir, 'last.log'), 'utf8').trim().split('\n').map((l) => JSON.parse(l)) } catch { return [] } }
  return { dir, run, write, asked, control: (c) => writeFileSync(join(dir, 'control.json'), JSON.stringify(c)), done: () => rmSync(dir, { recursive: true, force: true }) }
}
const STATS = { platform: 'docker', worker: { version: '2.0.0', pid: 41, started_at: 1, protocol: 7, bundle: 'b' }, resources: { cpus: 8, memory_total: 8, memory_used: 3, disk_total: 58, disk_used: 19, disk_free: 36, scope: 'engine_host' }, data_disk: { path: '/layerfs/data', fs: 'ext4', scope: 'vm_disk', volume: 'layerfs-1a2b3c4d-docker-data', inodes_free: 9 }, stores: [], sandboxes: { recorded: 1, live: 1 }, complete: true }

test('help: machine.stats, doctor.fix and the option live of sandbox.list are added, each only when declared', () => {
  const f = fixture()
  try {
    const calls = f.run('help', '--json').out.calls
    const of = (family, call) => calls.find((c) => c.family === family && c.call === call)
    assert.deepEqual(of('machine', 'stats').args.map((a) => a.name), ['machine'])
    assert.ok(of('doctor', 'fix'))
    assert.deepEqual(of('sandbox', 'list').options.map((o) => [o.name, o.kind]), [['page_size', 'int'], ['live', 'bool']])
    f.write({ calls: ['doctor.fix'] })
    const fewer = f.run('help', '--json').out.calls
    assert.ok(!fewer.some((c) => c.call === 'stats')); assert.ok(fewer.some((c) => c.call === 'fix'))
    assert.deepEqual(fewer.find((c) => c.call === 'list' && c.family === 'sandbox').options.map((o) => o.name), ['page_size'])
  } finally { f.done() }
})

test('machine stats: the declared answer with its machine and time; a stopped machine Unavailable, an unknown one NotFound; delays and refusals', () => {
  const f = fixture({ machines: { off: { add: true, state: 'stopped' } }, stats: { docker: STATS } })
  try {
    const t0 = Date.now()
    const got = f.run('machine', 'stats', '--', 'docker').out
    assert.equal(got.machine, 'docker'); assert.ok(got.observed_at >= t0)
    assert.deepEqual(got.resources, STATS.resources); assert.deepEqual(got.data_disk, STATS.data_disk); assert.deepEqual(got.sandboxes, { recorded: 1, live: 1 })
    assert.deepEqual([f.run('machine', 'stats', '--', 'off').status, f.run('machine', 'stats', '--', 'off').err.error], [125, 'Unavailable'])
    assert.equal(f.run('machine', 'stats', '--', 'nobody').err.error, 'NotFound')
    f.control({ refuse: { 'stats:docker': { code: 'Busy', message: 'two reads run' } } })
    assert.deepEqual(f.run('machine', 'stats', '--', 'docker').err, { error: 'Busy', message: 'two reads run' })
    f.control({ delay_ms: { 'stats:*': 400 } })
    const t1 = Date.now()
    assert.equal(f.run('machine', 'stats', '--', 'docker').out.machine, 'docker')
    assert.ok(Date.now() - t1 >= 400)
    assert.deepEqual(f.asked(), [], 'it never asks the next CLI for stats')
    assert.equal(readFileSync(join(f.dir, 'spawns.log'), 'utf8').trim().split('\n').length, 6, 'each call is in the spawn log')
  } finally { f.done() }
})

test('machines: a declared row replaces fields of a real one, or is a machine that exists only here', () => {
  const f = fixture({ machines: { docker: { state: 'running', free_disk: 5, tool_version: '29' }, vm: { add: true, platform: 'lima', state: 'running', disk: 50 } } })
  try {
    const rows = f.run('machine', 'list').out.machines
    assert.deepEqual(rows.map((m) => [m.name, m.platform, m.state]), [['docker', 'docker', 'running'], ['native', 'native', 'ready'], ['vm', 'lima', 'running']])
    assert.ok(!('free_disk' in rows[0]) && !('add' in rows[2]), 'what is not a row field stays out of the row')
    assert.equal(f.run('machine', 'get', '--', 'docker').out.state, 'running')
    assert.deepEqual(f.run('machine', 'get', '--', 'vm').out.disk, 50)
    assert.deepEqual(f.run('machine', 'status', '--', 'docker').out, { name: 'docker', platform: 'docker', state: 'running', free_disk: 5, tool_version: '29', down_reason: null, stores: [] })
    assert.deepEqual(f.run('machine', 'status', '--', 'vm').out, { name: 'vm', platform: 'lima', state: 'running', free_disk: null, tool_version: null, down_reason: null, stores: [] })
    assert.deepEqual(f.run('machine', 'images', '--', 'vm').out.images, [])
  } finally { f.done() }
})

test('sandbox list: `live` only when asked (as the SDK writes it, --live=true), never passed on; sandbox get: daemon and usage', () => {
  const f = fixture({ sandboxes: { a: { live: false }, b: { machine: 'off', live: null, live_reason: 'machine_stopped' }, x: { daemon: null, usage: { cpu_usec: 1, memory_current: 2, memory_peak: 3, pids: 4 } } } })
  try {
    const plain = f.run('sandbox', 'list', '--page-size', '100').out.items
    assert.ok(plain.every((b) => !('live' in b)))
    assert.equal(plain[1].machine, 'off', 'a Sandbox declared on another machine is listed there')
    for (const form of ['--live=true', '--live']) {
      const items = f.run('sandbox', 'list', '--page-size', '100', form, '--').out.items
      assert.deepEqual(items.map((b) => [b.name, b.live, b.live_reason]), [['a', false, undefined], ['b', null, 'machine_stopped'], ['c', false, undefined]], form)
    }
    assert.ok(f.run('sandbox', 'list', '--live=false').out.items.every((b) => !('live' in b)))
    assert.ok(f.asked().filter((a) => a[1] === 'list').every((a) => !a.some((x) => x.startsWith('--live'))), 'the next CLI is never sent --live')
    const host = f.run('sandbox', 'get', '--', 'a').out
    assert.deepEqual([host.daemon, host.usage, host.usage_reason], [null, null, 'host_process'])
    const ns = f.run('sandbox', 'get', '--', 'x').out
    assert.deepEqual([ns.daemon, ns.usage, 'usage_reason' in ns], [null, { cpu_usec: 1, memory_current: 2, memory_peak: 3, pids: 4 }, false])
  } finally { f.done() }
})
