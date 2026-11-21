// The stand-in CLI for the log calls (test/fixtures/cli-inspect-logs.mjs): it must
// answer as layerfs-project#178 "Design > Calls" describes, or the UI checks run
// against it check nothing. A stand-in for a CLI without logs.machine / logs.sandbox;
// never evidence about a real build.
//   node --test test/inspect-fixture.test.mjs
import { fileURLToPath } from 'node:url'
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { appendFileSync, mkdirSync, mkdtempSync, readFileSync, renameSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { pinned } from './routes-helper.mjs'

const shim = fileURLToPath(new URL('./fixtures/cli-inspect-logs.mjs', import.meta.url))
const pin = pinned()

function fixture() {
  const dir = mkdtempSync(join(tmpdir(), 'dsh-inspect-fixture-'))
  mkdirSync(join(dir, 'machine')); mkdirSync(join(dir, 'sandbox')); mkdirSync(join(dir, 'home'))
  const run = (args, { timeout = 20000 } = {}) => {
    const r = spawnSync(process.execPath, [shim, ...args], { encoding: 'utf8', timeout, maxBuffer: 1 << 26, env: { ...process.env, LFS_REAL_CLI: pin?.cli ?? '/usr/bin/false', LFS_INSPECT_DIR: dir, LAYERFS_HOME: join(dir, 'home') } })
    let out = null, err = null
    try { out = JSON.parse(r.stdout) } catch { /* not JSON */ }
    try { err = JSON.parse(r.stderr.trim().split('\n').at(-1)) } catch { /* not JSON */ }
    return { status: r.status, signal: r.signal, out, err, stdout: r.stdout, stderr: r.stderr }
  }
  const log = (kind, name, n = '') => join(dir, kind, `${name}${n}.log`)
  return { dir, run, log, control: (c) => writeFileSync(join(dir, 'control.json'), JSON.stringify(c)), done: () => rmSync(dir, { recursive: true, force: true }) }
}
const numbered = (from, to, pad = '') => Array.from({ length: to - from }, (_, i) => `line ${String(from + i).padStart(5, '0')}${pad}\n`).join('')
const texts = (answer) => answer.lines.map((l) => l.text)

test('tail: the last 200 lines by default, oldest first, a limit of 1 to 1000, an empty or missing log', () => {
  const f = fixture()
  try {
    // no file: an empty log with the cursor of a file that is not there (a real answer always carries one)
    const none = { lines: [], cursor: 'c1.0.0.0.0', has_more: false }
    assert.deepEqual(f.run(['logs', 'machine', '--', 'docker']).out, none)
    // reading after it: nothing while there is no file, then the file from its start
    assert.deepEqual(f.run(['logs', 'machine', '--cursor', 'c1.0.0.0.0', '--', 'docker']).out, none)
    writeFileSync(f.log('machine', 'docker'), 'first\nsecond\n')
    const started = f.run(['logs', 'machine', '--cursor', 'c1.0.0.0.0', '--', 'docker']).out
    assert.deepEqual([texts(started), started.has_more, 'before' in started], [['first', 'second'], false, false])
    assert.match(started.cursor, /^s1\.0\.13\./)
    assert.deepEqual(texts(f.run(['logs', 'machine', '--cursor', started.cursor, '--', 'docker']).out), [])
    writeFileSync(f.log('machine', 'docker'), '')
    const empty = f.run(['logs', 'machine', '--', 'docker']).out
    assert.deepEqual([empty.lines, empty.has_more, 'before' in empty], [[], false, false])
    assert.match(empty.cursor, /^s1\.0\.0\./)
    writeFileSync(f.log('machine', 'docker'), numbered(0, 500))
    const tail = f.run(['logs', 'machine', '--', 'docker']).out
    assert.equal(tail.lines.length, 200)
    assert.deepEqual([texts(tail)[0], texts(tail).at(-1)], ['line 00300', 'line 00499'])
    assert.equal(tail.has_more, false)
    assert.ok(tail.before, 'there are earlier lines')
    assert.deepEqual(texts(f.run(['logs', 'machine', '--limit', '3', '--', 'docker']).out), ['line 00497', 'line 00498', 'line 00499'])
    assert.equal(f.run(['logs', 'machine', '--limit=1000', '--', 'docker']).out.lines.length, 500)
    const all = f.run(['logs', 'machine', '--limit', '1000', '--', 'docker']).out
    assert.equal('before' in all, false, 'nothing before the first line of the only file')
    for (const bad of ['0', '1001', 'x', '1.5']) {
      const r = f.run(['logs', 'machine', '--limit', bad, '--', 'docker'])
      assert.deepEqual([r.status, r.err.code], [125, 'InvalidInput'], bad)
    }
    assert.equal(f.run(['logs', 'machine', '--cursor', 'a', '--before', 'b', '--', 'docker']).err.code, 'InvalidInput')
    // a Sandbox's log is its own file
    writeFileSync(f.log('sandbox', 'sb'), 'daemon up\n')
    assert.deepEqual(texts(f.run(['logs', 'sandbox', '--', 'sb']).out), ['daemon up'])
  } finally { f.done() }
})

test('cursor: lines written after it; a partial last line is held back; one 64 KiB chunk per call with has_more', () => {
  const f = fixture()
  try {
    writeFileSync(f.log('machine', 'docker'), numbered(0, 10))
    const tail = f.run(['logs', 'machine', '--', 'docker']).out
    const none = f.run(['logs', 'machine', '--cursor', tail.cursor, '--', 'docker']).out
    assert.deepEqual([none.lines, none.cursor, none.has_more], [[], tail.cursor, false])
    appendFileSync(f.log('machine', 'docker'), 'line 00010\nline 00011\npart')
    const two = f.run(['logs', 'machine', '--cursor', tail.cursor, '--', 'docker']).out
    assert.deepEqual([texts(two), two.has_more], [['line 00010', 'line 00011'], false])
    // the held-back part arrives whole once its line ends
    appendFileSync(f.log('machine', 'docker'), 'ial line\n')
    const whole = f.run(['logs', 'machine', '--cursor', two.cursor, '--', 'docker']).out
    assert.deepEqual(texts(whole), ['partial line'])
    // 5,000 lines of about 100 bytes: several chunks, each at most 64 KiB and `limit` lines, none lost or repeated
    appendFileSync(f.log('machine', 'docker'), numbered(100, 5100, ' '.repeat(88)))
    let cursor = whole.cursor, got = [], reads = 0, more = true
    while (more) {
      const page = f.run(['logs', 'machine', '--cursor', cursor, '--limit', '1000', '--', 'docker']).out
      assert.ok(page.lines.reduce((n, l) => n + l.text.length + 1, 0) <= 64 * 1024, 'one chunk')
      assert.ok(page.lines.length <= 1000)
      got.push(...texts(page)); cursor = page.cursor; more = page.has_more
      assert.ok(++reads < 40)
    }
    assert.ok(reads >= 7, `${reads} reads for 500 KB`)
    assert.equal(got.length, 5000)
    assert.deepEqual(got.map((t) => t.trim()), numbered(100, 5100).trim().split('\n'))
    // `limit` cuts a chunk short and says there is more
    appendFileSync(f.log('machine', 'docker'), numbered(6000, 6005))
    const short = f.run(['logs', 'machine', '--cursor', cursor, '--limit', '2', '--', 'docker']).out
    assert.deepEqual([texts(short), short.has_more], [['line 06000', 'line 06001'], true])
  } finally { f.done() }
})

test('before: earlier lines, back through the file and then its .1, with no line lost or repeated', () => {
  const f = fixture()
  try {
    writeFileSync(f.log('machine', 'docker', '.1'), numbered(0, 300))
    writeFileSync(f.log('machine', 'docker'), numbered(300, 1000))
    const tail = f.run(['logs', 'machine', '--limit', '100', '--', 'docker']).out
    assert.deepEqual([texts(tail)[0], texts(tail).at(-1)], ['line 00900', 'line 00999'])
    let before = tail.before, got = [...texts(tail)], reads = 0, crossed = false
    while (before) {
      const page = f.run(['logs', 'machine', '--before', before, '--limit', '250', '--', 'docker']).out
      assert.ok(page.lines.length > 0 && page.lines.length <= 250)
      assert.equal(page.has_more, !!page.before)
      if (/^s1\.1\./.test(page.before ?? '')) crossed = true
      got = [...texts(page), ...got]; before = page.before
      assert.ok(++reads < 20)
    }
    assert.ok(crossed, 'the paging crossed into the rotated file')
    assert.deepEqual(got, numbered(0, 1000).trim().split('\n'))
    // no rotated file: paging ends at the start of the only one
    rmSync(f.log('machine', 'docker', '.1'))
    const first = f.run(['logs', 'machine', '--before', tail.before, '--limit', '1000', '--', 'docker']).out
    assert.deepEqual([texts(first)[0], first.has_more, 'before' in first], ['line 00300', false, false])
  } finally { f.done() }
})

test('a line longer than a chunk comes in pieces marked continued; control characters are escaped', () => {
  const f = fixture()
  try {
    const long = 'x'.repeat(150_000)
    writeFileSync(f.log('sandbox', 'sb'), 'start\n')
    const tail = f.run(['logs', 'sandbox', '--', 'sb']).out
    appendFileSync(f.log('sandbox', 'sb'), `${long}\nafter\n`)
    let cursor = tail.cursor, pieces = [], more = true
    while (more) { const page = f.run(['logs', 'sandbox', '--cursor', cursor, '--', 'sb']).out; pieces.push(...page.lines); cursor = page.cursor; more = page.has_more }
    assert.deepEqual(pieces.map((p) => [p.text.length, p.continued === true]), [[65536, true], [65536, true], [150_000 - 2 * 65536, false], [5, false]])
    assert.equal(pieces.slice(0, 3).map((p) => p.text).join(''), long)
    assert.equal(pieces[3].text, 'after')
    // backward through the same line: the pieces before its end are continued too
    const end = f.run(['logs', 'sandbox', '--limit', '1', '--', 'sb']).out
    assert.deepEqual(texts(end), ['after'])
    let before = end.before, back = []
    while (before) { const page = f.run(['logs', 'sandbox', '--before', before, '--', 'sb']).out; back = [...page.lines, ...back]; before = page.before }
    let joined = '', lines = []
    for (const piece of back) { joined += piece.text; if (!piece.continued) { lines.push(joined); joined = '' } }
    assert.deepEqual(lines.map((l) => l.length), [5, 150_000])
    assert.equal(lines[1], long)
    writeFileSync(f.log('sandbox', 'esc'), 'tab\there \x1b[31mred\x1b[0m bell\x07 nul\x00 cr\r\n')
    assert.deepEqual(texts(f.run(['logs', 'sandbox', '--', 'esc']).out), ['tab\\there \\x1b[31mred\\x1b[0m bell\\x07 nul\\x00 cr\\r'])
  } finally { f.done() }
})

test('a rotated or replaced file makes an old cursor InvalidInput; a fresh tail then follows', () => {
  const f = fixture()
  try {
    writeFileSync(f.log('machine', 'docker'), numbered(0, 50))
    const tail = f.run(['logs', 'machine', '--', 'docker']).out
    // rotation as the launcher does it: the file moves to .1 and a new one starts
    renameSync(f.log('machine', 'docker'), f.log('machine', 'docker', '.1'))
    writeFileSync(f.log('machine', 'docker'), 'worker started\n')
    for (const flag of ['--cursor', '--before']) {
      const r = f.run(['logs', 'machine', flag, flag === '--cursor' ? tail.cursor : tail.before, '--', 'docker'])
      assert.deepEqual([r.status, r.err.code], [125, 'InvalidInput'], flag)
      assert.match(r.err.message, /rotated or replaced/)
    }
    const fresh = f.run(['logs', 'machine', '--', 'docker']).out
    assert.deepEqual(texts(fresh), ['worker started'])
    assert.match(fresh.before, /^s1\.1\./, 'the old lines are still reachable, in .1')
    appendFileSync(f.log('machine', 'docker'), 'listening\n')
    assert.deepEqual(texts(f.run(['logs', 'machine', '--cursor', fresh.cursor, '--', 'docker']).out), ['listening'])
    // truncated in place (same inode, shorter): the offset is past its end
    writeFileSync(f.log('machine', 'docker'), '')
    assert.equal(f.run(['logs', 'machine', '--cursor', fresh.cursor, '--', 'docker']).err.code, 'InvalidInput')
    assert.equal(f.run(['logs', 'machine', '--cursor', 'garbage', '--', 'docker']).err.code, 'InvalidInput')
    // a cursor given before the first line was whole still names the file
    writeFileSync(f.log('sandbox', 'new'), '')
    const empty = f.run(['logs', 'sandbox', '--', 'new']).out
    appendFileSync(f.log('sandbox', 'new'), 'first\n')
    assert.deepEqual(texts(f.run(['logs', 'sandbox', '--cursor', empty.cursor, '--', 'new']).out), ['first'])
  } finally { f.done() }
})

test('control.json: refusal codes, a delay, supports.log false; each process is counted', () => {
  const f = fixture()
  try {
    writeFileSync(f.log('machine', 'docker'), 'up\n')
    for (const code of ['Unavailable', 'Unsupported', 'Busy', 'NoSuchSandbox']) {
      f.control({ refuse: { 'machine:docker': { code, message: `${code} here` }, 'sandbox:*': { code, message: 'any Sandbox' } } })
      const r = f.run(['logs', 'machine', '--', 'docker'])
      assert.deepEqual([r.status, r.err], [125, { code, message: `${code} here`, detail: null }])
      assert.equal(f.run(['logs', 'sandbox', '--', 'whatever']).err.message, 'any Sandbox')
      assert.equal(f.run(['logs', 'machine', '--', 'other']).status, 0, 'another machine is not refused')
    }
    f.control({ supports_log: { native: false } })
    assert.equal(f.run(['logs', 'machine', '--', 'native']).err.code, 'Unsupported')
    assert.equal(f.run(['logs', 'machine', '--', 'docker']).status, 0)
    f.control({ delay_ms: { 'machine:docker': 700 } })
    const started = Date.now()
    assert.deepEqual(texts(f.run(['logs', 'machine', '--', 'docker']).out), ['up'])
    assert.ok(Date.now() - started >= 650)
    // a hang ends when the caller gives up
    f.control({ delay_ms: { 'sandbox:*': 60000 } })
    const hung = f.run(['logs', 'sandbox', '--', 'sb'], { timeout: 500 })
    assert.equal(hung.signal, 'SIGTERM')
    const spawns = readFileSync(join(f.dir, 'spawns.log'), 'utf8').trim().split('\n').map((l) => JSON.parse(l.slice(l.indexOf(' ') + 1)))
    assert.equal(spawns.length, 16)
    assert.deepEqual(spawns.at(-1), ['logs', 'sandbox', '--', 'sb'])
    // a call not listed is not answered here: it goes to the real CLI, which has none
    f.control({ calls: ['logs.sandbox'] })
    assert.notEqual(f.run(['logs', 'machine', '--', 'docker']).status, 0)
  } finally { f.done() }
})
