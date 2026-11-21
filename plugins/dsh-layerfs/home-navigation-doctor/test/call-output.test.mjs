// Per-call outputs (docs/specs/layerfs-logs.md "Outputs"): every plugin call is
// tagged (session receipt or host tag), its stdout/stderr kept bounded in
// `trace/outputs/`, refusals parsed, the wrapper tees without changing a byte,
// the store is pruned and cleared by Reset, and `logs.output` / `logs.list` serve it.
//   node --test test/call-output.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { build } from 'esbuild'
import { existsSync, mkdtempSync, readdirSync, readFileSync, statSync, utimesSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { spawn } from 'node:child_process'
import { createHash } from 'node:crypto'
import { pathToFileURL } from 'node:url'

const root = mkdtempSync(join(tmpdir(), 'dsh-call-output-'))
await build({
  stdin: { contents: "export * from './src/host/layerfs/call-output.ts'; export * from './src/host/layerfs/call-log.ts'; export * from './src/host/layerfs/tracing.ts'; export { execute } from './src/host/layerfs/cli.ts'; export * from './src/shared/logs.ts'; export { resetCalls as resetSource } from './src/host/logs/source.ts'; export { apply as applyLogs } from './src/host/logs/index.ts'", resolveDir: process.cwd(), loader: 'ts' },
  outfile: join(root, 'out.mjs'), bundle: true, platform: 'node', format: 'esm', target: 'node24', external: ['@deepseek-ai/*'],
})
const L = await import(pathToFileURL(join(root, 'out.mjs')))
const id = '12' + 'a'.repeat(64)
let n = 0
const place = () => {
  const dir = mkdtempSync(join(root, `case${n++}-`)), callLog = join(dir, 'trace', 'calls.jsonl')
  return { dir, callLog, outputs: join(dir, 'trace', 'outputs') }
}

// A fake CLI: `fail` refuses (exit 125, refusal on stderr), `--events` streams exec frames, `big` prints 200 KB.
function fakeCli(dir) {
  const cli = join(dir, 'fake.mjs')
  writeFileSync(cli, `#!/usr/bin/env node
const argv = process.argv.slice(2), tag = process.env.LAYERFS_LOGS_TAG ?? null
let input = ''; process.stdin.on('data', c => input += c); process.stdin.on('end', () => {
  process.stderr.write(JSON.stringify({ phase: 'start', done: 0, total: 1 }) + '\\n')
  if (argv.includes('fail')) { process.stderr.write(JSON.stringify({ error: 'Conflict', message: 'branch moved', fix: 'layerfs branch get p b', detail: { head: 'x' } }) + '\\n'); process.exitCode = 125; return }
  if (argv.includes('--events')) {
    console.log(JSON.stringify({ stream: 'stdout', bytes: 'hello ' }))
    console.log(JSON.stringify({ stream: 'stdout', bytes: { base64: Buffer.from('world\\n').toString('base64') } }))
    console.log(JSON.stringify({ stream: 'stderr', bytes: 'warn\\n' }))
    console.log(JSON.stringify({ result: { kind: 'Exited', exit_code: 0, tag } }))
    return
  }
  if (argv.includes('big')) { process.stdout.write(Buffer.alloc(200 * 1024, 0x61)); process.stdout.write('\\n'); return }
  console.log(JSON.stringify({ kind: 'Committed', commit: { id: '${id}' }, tag }))
})
`, { mode: 0o700 })
  return cli
}
const files = (dir) => existsSync(dir) ? readdirSync(dir).filter(f => f.endsWith('.json')) : []

test('records: stdout keeps its head, stderr its tail, within the cap on character boundaries', () => {
  const out = L.outputSink(10, 'head'), err = L.outputSink(10, 'tail')
  out.push(Buffer.from('abcdefg')); out.push(Buffer.from('é€xyz')) // 'é' is 2 bytes, '€' 3: the head's cut lands inside '€'
  err.push('0123é'); err.push(Buffer.from('abcdefghi')) // the tail's cut lands inside 'é'
  const r = L.outputRecord({ tag: 'dsh:-:t', exit: 0, stdout: out, stderr: err, max: 10 })
  assert.equal(r.stdout, 'abcdefgé'); assert.equal(r.stderr, 'abcdefghi')
  assert.deepEqual(r.bytes, { stdout: 15, stderr: 15 }); assert.deepEqual(r.truncated, { stdout: true, stderr: true })
  assert.equal(r.refusal, undefined)
  const small = L.outputRecord({ tag: 'dsh:-:t', exit: 0, stdout: L.outputSink(10, 'head'), stderr: L.outputSink(10, 'tail'), max: 10 })
  assert.deepEqual(small, { tag: 'dsh:-:t', exit: 0, bytes: { stdout: 0, stderr: 0 } })
  // A refusal is kept only for a refused call.
  const refusal = { error: 'Conflict', message: 'm', detail: { a: 1 } }
  assert.deepEqual(L.outputRecord({ tag: 'dsh:-:t', exit: 125, stdout: L.outputSink(1, 'head'), stderr: L.outputSink(1, 'tail'), refusal, max: 1 }).refusal, { code: 'Conflict', message: 'm', detail: { a: 1 } })
  assert.equal(L.outputRecord({ tag: 'dsh:-:t', exit: 0, stdout: L.outputSink(1, 'head'), stderr: L.outputSink(1, 'tail'), refusal, max: 1 }).refusal, undefined)
})

test('execute(): host tags outside sessions, session tags inside, outputs stored for success and refusal', async () => {
  const { dir, callLog, outputs } = place(), cli = fakeCli(dir)
  const spec = { cli, home: join(dir, 'home'), callLog, nativeLogs: false }
  const ok = await L.execute(spec, ['store', 'create', '--', 'demo'])
  assert.match(ok.tag, /^dsh:-:[a-f0-9]{32}$/); assert.equal(L.parseTag(ok.tag), undefined); assert.ok(L.isDshTag(ok.tag))
  let [line] = L.listCalls(callLog, {}).lines
  assert.equal(line.tag, ok.tag)
  const stored = L.readOutput(outputs, ok.tag)
  assert.equal(stored.exit, 0); assert.equal(JSON.parse(stored.stdout).tag, ok.tag)
  assert.match(stored.stderr, /"phase"/); assert.equal(stored.truncated, undefined); assert.equal(stored.bytes.stdout, Buffer.byteLength(stored.stdout))
  assert.equal(statSync(outputs).mode & 0o777, 0o700)
  assert.equal(statSync(join(outputs, files(outputs)[0])).mode & 0o777, 0o600)
  // Refused: the line carries the code, the output the parsed refusal; no stdin content is stored.
  await assert.rejects(L.execute(spec, ['file', 'write', '--', 'demo/app', 'main', '/x', 'fail'], { input: 'secret-content' }), e => e.code === 'Conflict')
  ;[line] = L.listCalls(callLog, {}).lines
  assert.equal(line.outcome, 'Conflict')
  const refused = L.readOutput(outputs, line.tag)
  assert.equal(refused.exit, 125); assert.deepEqual(refused.refusal, { code: 'Conflict', message: 'branch moved', fix: 'layerfs branch get p b', detail: { head: 'x' } })
  assert.ok(!JSON.stringify(refused).includes('secret-content'))
  // Archive writes (no trace context) and session calls.
  const receipts = []
  await L.withTraceContext({ origin: 'host', sessionId: 'sess', project: 'demo/app', record: async r => receipts.push(r) }, async () => {
    const inSession = await L.execute(spec, ['workspace', 'commit', '--', 'ws'])
    assert.equal(inSession.tag, L.sessionTag('sess', receipts[0].id))
    const archive = await L.withoutTraceContext(() => L.execute(spec, ['file', 'apply', '--', 'st/app', 'b'], { input: '[]' }))
    assert.match(archive.tag, /^dsh:-:/)
  })
  assert.equal(receipts.length, 1, 'host-tagged archive writes record no receipt')
  assert.ok(L.hasOutput(outputs, L.sessionTag('sess', receipts[0].id)))
  // Session tag-prefix filters never see host lines.
  assert.equal(L.listCalls(callLog, { tag: 'dsh:sess:' }).lines.length, 1)
  assert.equal(L.listCalls(callLog, { tag: 'dsh:-:' }).lines.length, 3)
  // Discovery is neither tagged nor stored; a spec without a call log neither.
  const before = files(outputs).length
  await L.execute({ cli, home: '' }, ['help', '--json'], { discovery: true })
  assert.equal((await L.execute({ cli, home: join(dir, 'home') }, ['store', 'create', '--', 'x'])).tag, null)
  assert.equal(files(outputs).length, before)
  // The native call log: no stand-in line, but the call is tagged and its output kept.
  const native = await L.execute({ ...spec, nativeLogs: true }, ['store', 'create', '--', 'n'])
  assert.match(native.tag, /^dsh:-:/); assert.ok(L.hasOutput(outputs, native.tag))
  assert.equal(L.listCalls(callLog, {}).lines.length, 4)
})

function run(command, argv, env, input = '') {
  return new Promise((resolve, reject) => {
    const child = spawn(command, argv, { env: { ...process.env, ...env }, stdio: 'pipe' })
    const out = [], err = []
    child.stdout.on('data', d => out.push(d)); child.stderr.on('data', d => err.push(d))
    child.on('error', reject); child.on('close', (code, signal) => resolve({ stdout: Buffer.concat(out), stderr: Buffer.concat(err).toString(), code, signal })); child.stdin.end(input)
  })
}

test('wrapper: tees stdout byte-identical, stores the bounded output for its tag, in both modes', async () => {
  const { dir, callLog, outputs } = place(), cli = fakeCli(dir)
  const identity = { origin: 'tool', sessionId: 'w', project: 'p', toolCallId: 'c', scope: L.toolScope('w', 'c') }
  const env = { DSH_LAYERFS_TRACE_CONTEXT: L.traceEnvironment(identity) }
  const direct = await run(cli, ['file', 'read', 'big'], {})
  const command = L.managedCli({ root: dir, cli, home: join(dir, 'home'), callLog })
  const big = await run(command, ['file', 'read', 'big'], env)
  assert.equal(big.code, 0); assert.ok(big.stdout.equals(direct.stdout), 'stdout passes through unchanged'); assert.equal(big.stdout.length, 200 * 1024 + 1)
  const [line] = L.listCalls(callLog, {}).lines
  const stored = L.readOutput(outputs, line.tag)
  assert.equal(Buffer.byteLength(stored.stdout), L.OUTPUT_MAX); assert.equal(stored.bytes.stdout, 200 * 1024 + 1); assert.equal(stored.truncated.stdout, true)
  const fail = await run(command, ['file', 'write', 'p', 'b', '/x', 'fail'], env, 'secret-content')
  assert.equal(fail.code, 125)
  const refused = L.readOutput(outputs, L.listCalls(callLog, {}).lines[0].tag)
  assert.equal(refused.exit, 125); assert.equal(refused.refusal.code, 'Conflict'); assert.ok(!JSON.stringify(refused).includes('secret-content'))
  assert.equal(refused.stderr, fail.stderr)
  // Without a trace context: untagged, nothing stored.
  const count = files(outputs).length
  await run(command, ['store', 'create', 'x'], {})
  assert.equal(files(outputs).length, count)
  // Native mode still spawns and pipes to capture, and writes no stand-in line.
  const n2 = place(), nativeCmd = L.managedCli({ root: n2.dir, cli, home: join(n2.dir, 'home'), callLog: n2.callLog, native: true })
  const nat = await run(nativeCmd, ['store', 'create', 'x'], env)
  const tag = JSON.parse(nat.stdout).tag
  assert.match(tag, /^dsh:w:/); assert.equal(existsSync(n2.callLog), false)
  const kept = L.readOutput(n2.outputs, tag)
  assert.equal(kept.stdout, nat.stdout.toString()); assert.equal(kept.exit, 0)
})

test('pruning keeps the store within its file and byte bounds, oldest first', () => {
  const { outputs } = place()
  const record = (i, size = 10) => ({ tag: `dsh:-:${i}`, bytes: { stdout: size, stderr: 0 }, stdout: 'x'.repeat(size) })
  for (let i = 0; i < 6; i++) {
    assert.ok(L.saveOutput(outputs, record(i), { files: 1000, bytes: 1e9, every: 1e9 }))
    const file = join(outputs, L.outputName(createHash, `dsh:-:${i}`))
    utimesSync(file, new Date(1000 + i * 1000), new Date(1000 + i * 1000))
  }
  L.saveOutput(outputs, record(6), { files: 3, bytes: 1e9, every: 1 })
  assert.equal(files(outputs).length, 3)
  assert.deepEqual([4, 5, 6].map(i => L.hasOutput(outputs, `dsh:-:${i}`)), [true, true, true])
  L.saveOutput(outputs, record(7, 3000), { files: 100, bytes: 3100, every: 1 })
  assert.deepEqual(files(outputs), [L.outputName(createHash, 'dsh:-:7')])
})

test('ops: logs.output validates tags and answers missing; logs.list marks stored outputs; reset clears them', async () => {
  const { dir, callLog, outputs } = place(), cli = fakeCli(dir)
  const spec = { cli, home: join(dir, 'home'), callLog, nativeLogs: false }
  const layerfs = { callLog, nativeLogs: async () => false, emit() {} }
  let ops
  L.applyLogs({ effect() {}, get() {}, layerfs, layerfsRemote: { operate(_ctx, table) { ops = table } } })
  const ok = await L.execute(spec, ['store', 'create', '--', 'demo'])
  L.appendCall(callLog, { v: 1, at: Date.now(), ms: 1, client: 'node', argv: ['store', 'list'], outcome: 'ok' })
  L.appendCall(callLog, { v: 1, at: Date.now(), ms: 1, client: 'cli', argv: ['store', 'list'], outcome: 'ok', tag: 'dsh:-:gone' })
  const page = await ops['logs.list']({})
  assert.deepEqual(page.lines.map(l => l.output ?? false), [false, false, true])
  assert.equal(page.lines[2].dsh, undefined, 'a host tag is no session label')
  assert.equal((await ops['logs.output']({ tag: ok.tag })).exit, 0)
  assert.deepEqual(await ops['logs.output']({ tag: 'dsh:-:gone' }), { missing: true })
  for (const tag of [undefined, '', 'other:tag', 42, `dsh:${'x'.repeat(130)}`, 'dsh:-:a\nb']) {
    await assert.rejects(ops['logs.output']({ tag }), e => e.code === 'InvalidInput', String(tag))
  }
  // A file whose record names another tag (a hash collision or tampering) is not served.
  writeFileSync(join(outputs, L.outputName(createHash, 'dsh:-:other')), JSON.stringify({ tag: 'dsh:-:x', bytes: { stdout: 0, stderr: 0 } }))
  assert.deepEqual(L.readOutput(outputs, 'dsh:-:other'), { missing: true })
  const answer = await L.resetSource(layerfs)
  assert.equal(answer.removed, 3); assert.equal(files(outputs).length, 0)
  assert.deepEqual(await ops['logs.output']({ tag: ok.tag }), { missing: true })
  assert.equal(readFileSync(callLog, 'utf8'), '')
})
