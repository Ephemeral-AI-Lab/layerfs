// The plugin's stand-in call log (docs/specs/layerfs-logs.md §3, LayerFS #659 §4–§7):
// line format, rendering and redaction, rotation, reset, list filters and paging,
// and the tag a line shares with its receipt.
//   node --test test/call-log.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { build } from 'esbuild'
import { appendFileSync, existsSync, mkdtempSync, readFileSync, statSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { spawn } from 'node:child_process'
import { pathToFileURL } from 'node:url'

const root = mkdtempSync(join(tmpdir(), 'dsh-call-log-'))
await build({ stdin: { contents: "export * from './src/host/layerfs/call-log.ts'; export * from './src/host/layerfs/tracing.ts'; export { execute } from './src/host/layerfs/cli.ts'; export * from './src/shared/logs.ts'", resolveDir: process.cwd(), loader: 'ts' }, outfile: join(root, 'log.mjs'), bundle: true, platform: 'node', format: 'esm', target: 'node24' })
const L = await import(pathToFileURL(join(root, 'log.mjs')))
const id = '12' + 'a'.repeat(64), base = '32' + 'b'.repeat(64)
let n = 0
const fresh = () => join(mkdtempSync(join(root, `log${n++}-`)), 'trace', 'calls.jsonl')
const line = (at, argv, extra = {}) => ({ v: 1, at, ms: 1, client: 'cli', argv, outcome: 'ok', ...extra })

test('render: canonical order, front-end switches and `--` dropped, Bool flags bare', () => {
  assert.deepEqual(L.renderCall(['file', 'write', '--message', 'save it', '--mode', '420', '--', 'demo/app', 'main', 'src/a.ts'], 5321),
    ['file', 'write', 'demo/app', 'main', 'src/a.ts', '<5321 bytes>', '--message', '<7 bytes>', '--mode', '420'])
  assert.deepEqual(L.renderCall(['workspace', 'exec', '--events', '--exec-id', 'e1', '--', 'ws1', 'npm test']), ['workspace', 'exec', 'ws1', '<8 bytes>', '--exec-id', 'e1'])
  assert.deepEqual(L.renderCall(['workspace', 'exec', 'ws1', 'ls', '--stream']), ['workspace', 'exec', 'ws1', '<2 bytes>', '--stream'])
  assert.deepEqual(L.renderCall(['workspace', 'commit', '--force=true', '--json', '--', 'ws']), ['workspace', 'commit', 'ws', '--force'])
  assert.deepEqual(L.renderCall(['commit', 'diff', '--paths-only=false', '--', 'p', 'a', 'b']), ['commit', 'diff', 'p', 'a', 'b'])
  assert.deepEqual(L.renderCall(['workspace', 'exec_cancel', 'ws', 'e1', '--progress']), ['workspace', 'exec-cancel', 'ws', 'e1'])
  assert.deepEqual(L.renderCall(['file', 'rm', '-m', 'bye', 'p', 'b', '/x']), ['file', 'rm', 'p', 'b', '/x', '--message', '<3 bytes>'])
})
test('redaction: free text and credentials as byte counts, other values cut to 200 characters', () => {
  assert.deepEqual(L.renderCall(['file', 'edit', '--old', 'aé', '--new=b', '--', 'p', 'b', '/x']), ['file', 'edit', 'p', 'b', '/x', '--old', '<3 bytes>', '--new', '<1 bytes>'])
  assert.deepEqual(L.renderCall(['store', 'connect', '--password', 'env:PW', '--', 'pg', 'postgres://u:secret@h/db']), ['store', 'connect', 'pg', '<24 bytes>', '--password', '<6 bytes>'])
  assert.deepEqual(L.renderCall(['backup', 'connect', 'b', 's3://k@bucket', '--url', 'x']), ['backup', 'connect', 'b', '<13 bytes>', '--url', '<1 bytes>'])
  assert.deepEqual(L.renderCall(['backup', 'connect', '--key', 'env:R2_KEY', '--', 'b', 's3://bucket']), ['backup', 'connect', 'b', '<11 bytes>', '--key', '<10 bytes>'])
  // a text value starting with `--` is still a value: redacted names always, any option with the table's Bools known
  assert.deepEqual(L.renderCall(['file', 'edit', '--old', '--secret-flag', '--new', '--x', '--message', '--m', '--', 'p', 'b', '/x']),
    ['file', 'edit', 'p', 'b', '/x', '--old', '<13 bytes>', '--new', '<3 bytes>', '--message', '<3 bytes>'])
  assert.deepEqual(L.renderCall(['branch', 'diff', '--path-prefix', '--odd', '--paths-only', '--', 'p', 'a', 'b'], null, ['paths_only']),
    ['branch', 'diff', 'p', 'a', 'b', '--path-prefix', '--odd', '--paths-only'])
  const long = '😀'.repeat(250)
  const [, , , , path] = L.renderCall(['file', 'rm', 'p', 'b', long])
  assert.equal(Array.from(path).length, 201); assert.ok(path.endsWith('…'))
  const rendered = JSON.stringify(L.callLine({ at: 1, ms: 1, argv: ['workspace', 'exec', '--', 'ws', 'echo file-content-secret'], input: 'stdin-secret', outcome: 'ok' }))
  assert.ok(!rendered.includes('secret'))
})
test('saved fields follow #659 §4.2 and only name 66-hex ids', () => {
  const at = (argv, result) => L.savedFields(argv, result)
  // 2.0: every saving call answers `commit` (66 hex), with `unchanged` when nothing was made; the kind is this log's own word
  assert.deepEqual(at(['workspace', 'commit', 'ws'], { commit: id }), { kind: 'Committed', commit: id })
  assert.deepEqual(at(['file', 'write', 'p', 'b', '/x'], { commit: base, unchanged: true }), { kind: 'UpToDate', commit: base })
  assert.deepEqual(at(['branch', 'fork', 'p', 'n'], { commit: id }), { kind: 'Forked', commit: id })
  assert.deepEqual(at(['branch', 'merge', 'p', 'n', 'a', 'b'], { commit: id, fast_forward: false }), { kind: 'Merged', commit: id })
  assert.deepEqual(at(['branch', 'publish', 'p', 'b'], { commit: base }), { kind: 'Added', layer: base })
  assert.deepEqual(at(['branch', 'publish', 'p', 'b'], { commit: base, unchanged: true }), { kind: 'UpToDate', layer: base })
  assert.deepEqual(at(['workspace', 'mount', 'p', 'b'], { workspace: 'p:w' }), {})
  assert.deepEqual(at(['workspace', 'commit', 'ws'], { commit: 'abc' }), {}, 'only a 66-hex id is named')
  const ok = L.callLine({ at: 5, ms: 2.4, argv: ['workspace', 'commit', '--', 'ws'], outcome: 'ok', result: { commit: id }, tag: 'dsh:s:r' })
  assert.deepEqual(ok, { v: 1, at: 5, ms: 2, client: 'cli', argv: ['workspace', 'commit', 'ws'], outcome: 'ok', kind: 'Committed', commit: id, tag: 'dsh:s:r' })
  assert.deepEqual(L.callLine({ at: 5, ms: 1, argv: ['branch', 'merge', 'p', 'n', 'a', 'b'], outcome: 'Conflict' }), { v: 1, at: 5, ms: 1, client: 'cli', argv: ['branch', 'merge', 'p', 'n', 'a', 'b'], outcome: 'Conflict' })
})
test('receipt ids keep tags within 128 bytes', () => {
  const a = L.makeReceipt('session-1', 'abcd1234', 'f'.repeat(32))
  assert.equal(a.id, `abcd1234.${'f'.repeat(32)}`); assert.equal(a.tag, `dsh:session-1:${a.id}`)
  assert.deepEqual(L.parseTag(a.tag), { sessionId: 'session-1', receiptId: a.id })
  const long = L.makeReceipt('s'.repeat(100), undefined, 'f'.repeat(32))
  assert.ok(Buffer.byteLength(long.tag) <= 128); assert.ok(long.id.length >= 8)
  assert.equal(L.makeReceipt('s'.repeat(130), undefined, 'f'.repeat(32)).tag, undefined)
})
test('append: owner-only file in an owner-only directory, one JSON line per call', () => {
  const path = fresh()
  L.appendCall(path, line(1, ['store', 'list']))
  L.appendCall(path, line(2, ['project', 'list']))
  assert.equal(statSync(path).mode & 0o777, 0o600)
  assert.equal(statSync(join(path, '..')).mode & 0o777, 0o700)
  assert.deepEqual(readFileSync(path, 'utf8').trim().split('\n').map(text => JSON.parse(text).at), [1, 2])
})
test('rotation past the threshold, list across both files newest first, reset of both', () => {
  const path = fresh(), rotated = path.replace('calls.jsonl', 'calls.1.jsonl')
  for (let at = 1; at <= 5; at++) L.appendCall(path, line(at, ['store', 'list']), 300)
  assert.ok(existsSync(rotated)); assert.ok(!existsSync(path.replace('calls.jsonl', 'calls.lock')))
  const ats = L.listCalls(path, {}).lines.map(l => l.at)
  assert.equal(ats[0], 5); assert.deepEqual(ats, [...ats].sort((x, y) => y - x))
  const before = L.listCalls(path, {}).lines.length
  assert.ok(before >= 3)
  assert.deepEqual(L.resetCalls(path), { removed: before })
  assert.equal(readFileSync(path, 'utf8'), ''); assert.ok(!existsSync(rotated))
  assert.deepEqual(L.listCalls(path, {}), { lines: [], has_more: false, skipped: 0 })
  // A real 16 MiB file rotates on the next append.
  writeFileSync(path, `${JSON.stringify(line(1, ['store', 'list']))}\n`.padEnd(16 * 1024 * 1024 + 10, ' '))
  L.appendCall(path, line(2, ['store', 'list']))
  assert.ok(existsSync(rotated)); assert.ok(!existsSync(path))
})
test('list filters: op, project substring, client, tag, outcome, since, writes', () => {
  const path = fresh()
  L.appendCall(path, line(1, ['store', 'list']))
  L.appendCall(path, line(2, ['file', 'write', 'Demo/App', 'main', '/x', '<3 bytes>'], { tag: 'dsh:s1:r1', kind: 'Committed', commit: id }))
  L.appendCall(path, line(3, ['file', 'read', 'demo/app', 'main', '/x'], { tag: 'dsh:s1:r2' }))
  L.appendCall(path, line(4, ['branch', 'merge', 'other/app', 'n', 'a', 'b'], { outcome: 'Conflict', client: 'node', tag: 'dsh:s2:r3' }))
  const ats = args => L.listCalls(path, args).lines.map(l => l.at)
  assert.deepEqual(ats({}), [4, 3, 2, 1])
  assert.deepEqual(ats({ op: 'file' }), [3, 2])
  assert.deepEqual(ats({ op: 'file.write' }), [2])
  assert.deepEqual(ats({ project: 'demo/APP' }), [3, 2])
  assert.deepEqual(ats({ project: 'app' }), [4, 3, 2])
  assert.deepEqual(ats({ client: 'node' }), [4])
  assert.deepEqual(ats({ tag: 'dsh:s1:' }), [3, 2])
  assert.deepEqual(ats({ tag: 'dsh:s1:r2' }), [3])
  assert.deepEqual(ats({ tag: 'dsh:s1' }), [])
  assert.deepEqual(ats({ outcome: 'Conflict' }), [4])
  assert.deepEqual(ats({ since: 3 }), [4, 3])
  assert.deepEqual(ats({ writes: true }), [4, 2])
})
test('paging: opaque cursor, has_more, stale cursor after reset or rotation, torn lines skipped', () => {
  const path = fresh()
  for (let at = 1; at <= 5; at++) L.appendCall(path, line(at, ['store', 'list']))
  appendFileSync(path, 'not json\n')
  L.appendCall(path, line(6, ['store', 'list']))
  appendFileSync(path, '{"v":1,"at":7,"ms"') // torn tail
  const first = L.listCalls(path, { limit: 2 })
  assert.deepEqual(first.lines.map(l => l.at), [6, 5]); assert.equal(first.has_more, true); assert.equal(first.skipped, 2, 'the torn tail and the bad line above 5')
  assert.equal(typeof first.next, 'string')
  L.appendCall(path, line(8, ['store', 'list'])) // appends do not disturb a cursor
  const second = L.listCalls(path, { limit: 2, cursor: first.next })
  assert.deepEqual(second.lines.map(l => l.at), [4, 3]); assert.equal(second.skipped, 0)
  const third = L.listCalls(path, { limit: 2, cursor: second.next })
  assert.deepEqual(third.lines.map(l => l.at), [2, 1]); assert.equal(third.has_more, false); assert.equal(third.next, undefined)
  const all = L.listCalls(path, { limit: 1000 })
  assert.equal(all.skipped, 2); assert.equal(all.has_more, false); assert.deepEqual(all.lines.map(l => l.at), [6, 5, 4, 3, 2, 1])
  assert.throws(() => L.listCalls(path, { cursor: 'garbage' }), (e) => e.code === 'InvalidInput')
  const stale = L.listCalls(path, { limit: 1 }).next
  L.resetCalls(path)
  for (let at = 1; at <= 3; at++) L.appendCall(path, line(at, ['store', 'list']))
  assert.throws(() => L.listCalls(path, { cursor: stale }), (e) => e.code === 'InvalidInput')
  const beforeRotation = L.listCalls(path, { limit: 1 }).next
  L.appendCall(path, line(9, ['store', 'list']), 10)
  assert.throws(() => L.listCalls(path, { cursor: beforeRotation }), (e) => e.code === 'InvalidInput')
})

function run(command, argv, env, input = '') {
  return new Promise((resolve, reject) => {
    const child = spawn(command, argv, { env: { ...process.env, ...env }, stdio: 'pipe' })
    let stdout = ''
    child.stdout.on('data', data => { stdout += data })
    child.on('error', reject); child.on('close', (code) => resolve({ stdout, code })); child.stdin.end(input)
  })
}
test('the wrapper and execute() each write one line carrying the same tag as their receipt', async () => {
  const dir = mkdtempSync(join(root, 'tags-')), callLog = join(dir, 'trace', 'calls.jsonl')
  const cli = join(dir, 'fake.mjs')
  writeFileSync(cli, `#!/usr/bin/env node\nlet input='';process.stdin.on('data',c=>input+=c);process.stdin.on('end',()=>{if(process.argv.includes('bad-usage')){process.exitCode=2;return}console.log(JSON.stringify({commit:'${id}'}))});`, { mode: 0o700 })
  const spec = { cli, home: join(dir, 'home'), callLog, nativeLogs: false }
  const receipts = []
  const identity = { origin: 'tool', sessionId: 'sess', project: 'demo/app', turn: 1, toolCallId: 'call-1', workspace: 'sb:main', scope: L.toolScope('sess', 'call-1') }
  await L.withTraceContext({ ...identity, record: async row => receipts.push(row) }, async () => {
    await L.execute(spec, ['file', 'write', '--message', 'm', '--', 'demo/app', 'main', '/x'], { input: 'abc' })
    await L.execute({ ...spec }, ['help', '--json'], { discovery: true })
    await assert.rejects(L.execute(spec, ['file', 'write', '--', 'demo/app', 'main', '/x', 'bad-usage'], { input: '' }))
  })
  let { lines } = L.listCalls(callLog, {})
  assert.equal(lines.length, 1, 'discovery and usage errors write no line')
  assert.deepEqual(lines[0].argv, ['file', 'write', 'demo/app', 'main', '/x', '<3 bytes>', '--message', '<1 bytes>'])
  assert.equal(lines[0].commit, id)
  assert.equal(lines[0].tag, L.sessionTag('sess', receipts[0].id))
  const command = L.managedCli({ root: dir, cli, home: join(dir, 'home'), callLog })
  await run(command, ['workspace', 'commit', 'sb:main'], { DSH_LAYERFS_TRACE_CONTEXT: L.traceEnvironment(identity) })
  ;({ lines } = L.listCalls(callLog, {}))
  assert.equal(lines.length, 2)
  const tag = L.parseTag(lines[0].tag)
  assert.equal(tag.sessionId, 'sess'); assert.ok(tag.receiptId.startsWith(`${identity.scope}.`))
  const receipt = L.receiptFromLine(lines[0], identity, tag.receiptId)
  assert.equal(L.sessionTag(receipt.sessionId, receipt.id), lines[0].tag)
  assert.deepEqual(receipt.versions, [{ kind: 'Commit', id, effect: 'created' }])
  // With LayerFS's own log the plugin writes no stand-in line.
  await L.execute({ ...spec, nativeLogs: true }, ['file', 'write', '--', 'demo/app', 'main', '/x'], { input: '' })
  assert.equal(L.listCalls(callLog, {}).lines.length, 2)
})

test('loggable: table calls write a line; help, the logs family and the quiet monitoring reads do not', () => {
  for (const argv of [['machine', 'list'], ['sandbox', 'get', '--', 'sb'], ['machine', 'get', '--', 'vm'], ['file', 'read', '--', 'p/a', 'main', '/x'], ['workspace', 'exec-cancel', '--', 'sb:ws'], ['sandbox', 'list'], ['machine', 'images', '--', 'vm']]) {
    assert.equal(L.loggable(argv), true, argv.join(' '))
  }
  // not table calls
  for (const argv of [['--version', '--json'], ['help', '--json'], ['doctor'], []]) assert.equal(L.loggable(argv), false, argv.join(' '))
  // the logs family (#659): the call log's own reads, and the worker and daemon logs (layerfs-project#178)
  for (const argv of [['logs', 'list'], ['logs', 'reset'], ['logs', 'machine', '--', 'vm'], ['logs', 'sandbox', '--cursor', 'c', '--', 'sb']]) assert.equal(L.loggable(argv), false, argv.join(' '))
  // the files reads are quiet in LayerFS's log; the stand-in skips them too
  for (const argv of [['sandbox', 'files', '--', 'sb'], ['sandbox', 'files', '--depth', '1', '--', 'sb', '/home'], ['machine', 'files', '--', 'vm', '/etc']]) assert.equal(L.loggable(argv), false, argv.join(' '))
})
