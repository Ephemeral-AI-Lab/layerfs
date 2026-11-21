// Self-sufficient receipts (docs/specs/layerfs-logs.md §4, §5a): both writers
// fill `at`, `ms`, redacted portable `argv`, `tag`, outcome code, kind, exec id
// and exit, and the simulation step; a store import merges receipts by id;
// `logs.list` renders an imported session's receipts when this machine's log
// has no line for them; simulation host calls carry `dsh-sim:` tags.
//   node --test test/receipt-logs.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { build } from 'esbuild'
import { mkdtempSync, writeFileSync } from 'node:fs'
import { homedir, tmpdir } from 'node:os'
import { join } from 'node:path'
import { spawn } from 'node:child_process'
import { fileURLToPath, pathToFileURL } from 'node:url'

const root = mkdtempSync(join(tmpdir(), 'dsh-receipt-logs-'))
await build({
  stdin: { contents: "export * from './src/host/layerfs/call-output.ts'; export * from './src/host/layerfs/call-log.ts'; export * from './src/host/layerfs/tracing.ts'; export { execute } from './src/host/layerfs/cli.ts'; export * from './src/shared/logs.ts'; export { apply as applyLogs, receiptLine, sessionOfTag } from './src/host/logs/index.ts'", resolveDir: process.cwd(), loader: 'ts' },
  outfile: join(root, 'out.mjs'), bundle: true, platform: 'node', format: 'esm', target: 'node24', external: ['@deepseek-ai/*'],
})
const L = await import(pathToFileURL(join(root, 'out.mjs')))
const id = '12' + 'a'.repeat(64)
let n = 0
const place = () => {
  const dir = mkdtempSync(join(root, `case${n++}-`))
  return { dir, callLog: join(dir, 'trace', 'calls.jsonl'), outputs: join(dir, 'trace', 'outputs') }
}
function fakeCli(dir) {
  const cli = join(dir, 'fake.mjs')
  writeFileSync(cli, `#!/usr/bin/env node
const argv = process.argv.slice(2)
let input = ''; process.stdin.on('data', c => input += c); process.stdin.on('end', () => {
  if (argv.includes('fail')) { process.stderr.write(JSON.stringify({ error: 'Conflict', message: 'moved' }) + '\\n'); process.exitCode = 125; return }
  // 2.0: workspace exec prints its one result JSON and exits with the command's own code
  if (argv[0] === 'workspace' && argv[1] === 'exec') { console.log(JSON.stringify({ exit_code: 3, stdout: 'hi\\n', stderr: '', exec: 'exec-7' })); process.exitCode = 3; return }
  console.log(JSON.stringify({ commit: '${id}' }))
})
`, { mode: 0o700 })
  return cli
}
function run(command, argv, env, input = '') {
  return new Promise((resolve, reject) => {
    const child = spawn(command, argv, { env: { ...process.env, ...env }, stdio: 'pipe' })
    let stdout = ''
    child.stdout.on('data', d => { stdout += d }); child.stderr.on('data', () => {})
    child.on('error', reject); child.on('close', code => resolve({ stdout, code })); child.stdin.end(input)
  })
}
const step = { runId: 'run-1', stepKey: 'cycle.1.write' }

test('SDK receipts carry time, duration, redacted portable argv, tag, outcome code, kind, exec id and exit', async () => {
  const { dir, callLog } = place(), cli = fakeCli(dir)
  const spec = { cli, home: join(dir, 'home'), callLog, nativeLogs: false }
  const receipts = []
  const before = Date.now()
  await L.withTraceContext({ origin: 'tool', sessionId: 's', project: 'demo/app', turn: 1, toolCallId: 'c1', scope: L.toolScope('s', 'c1'), step, record: async row => receipts.push(row) }, async () => {
    await L.execute(spec, ['file', 'write', '--message', 'secret words', '--from', join(homedir(), 'notes.txt'), '--', 'demo/app', 'main', '/x'])
    assert.equal((await L.execute(spec, ['workspace', 'exec', '--detail', '--', 'sb:main', 'echo hi'])).exit_code, 3, 'a command\'s own exit is its answer, not a failure')
    await assert.rejects(L.execute(spec, ['file', 'write', '--', 'demo/app', 'main', '/x', 'fail'], { input: 'abc' }))
  })
  const { lines } = L.listCalls(callLog, {})
  const [write, exec, refused] = receipts
  assert.ok(write.at >= before && write.at <= Date.now()); assert.ok(Number.isInteger(write.ms) && write.ms >= 0)
  assert.deepEqual(write.argv, ['file', 'write', 'demo/app', 'main', '/x', '--message', '<12 bytes>', '--from', '<local path>'])
  assert.ok(!JSON.stringify(receipts).includes(homedir()), 'no machine-local path in a receipt')
  assert.equal(write.kind, 'Committed'); assert.equal(write.code, undefined)
  assert.deepEqual(write.step, step, 'the scripted step travels with the receipt')
  assert.equal(write.tag, lines.at(-1).tag, 'the receipt names its line')
  assert.equal(exec.execId, 'exec-7', 'LayerFS\'s exec id from the answer'); assert.equal(exec.exit, 3); assert.deepEqual(exec.argv, ['workspace', 'exec', 'sb:main', '<7 bytes>', '--detail'])
  assert.equal(refused.outcome, 'refusal'); assert.equal(refused.code, 'Conflict'); assert.equal(refused.kind, undefined)
  assert.equal(refused.argv.at(-1), '<3 bytes>')
  // The receipt renders as the line it stands for.
  const rendered = L.receiptLine(write), line = lines.at(-1)
  assert.deepEqual(rendered, { ...line, argv: rendered.argv }, 'same at, ms, outcome, kind, version and tag')
  assert.equal(L.receiptLine(refused).outcome, 'Conflict')
  assert.equal(L.receiptLine({ id: 'old', sessionId: 's', operation: 'file.write', outcome: 'success', versions: [] }), undefined, 'older receipts without time give no line')
})

test('receipts from wrapper lines carry the same fields; exit comes from the stored output', async () => {
  const { dir, callLog, outputs } = place(), cli = fakeCli(dir)
  const command = L.managedCli({ root: dir, cli, home: join(dir, 'home'), callLog })
  const identity = { origin: 'tool', sessionId: 's', project: 'demo/app', turn: 2, toolCallId: 'bash-1', scope: L.toolScope('s', 'bash-1'), step }
  const env = { DSH_LAYERFS_TRACE_CONTEXT: L.traceEnvironment(identity) }
  await run(command, ['workspace', 'exec', 'sb:main', 'echo hi'], env)
  await run(command, ['project', 'init', 'demo/app', join(tmpdir(), 'src-folder')], env)
  const [init, exec] = L.listCalls(callLog, {}).lines
  const execReceipt = L.receiptFromLine(exec, identity, L.parseTag(exec.tag).receiptId, L.exitOfOutput(L.readOutput(outputs, exec.tag)))
  assert.equal(execReceipt.at, exec.at); assert.equal(execReceipt.ms, exec.ms); assert.equal(execReceipt.tag, exec.tag)
  // a wrapper line names no exec id (only the answer does); the exit comes from the stored result
  assert.equal(execReceipt.execId, undefined); assert.equal(execReceipt.exit, 3); assert.equal(execReceipt.kind, 'exit:3')
  assert.deepEqual(execReceipt.step, step); assert.equal(execReceipt.turn, 2)
  const initReceipt = L.receiptFromLine(init, identity, L.parseTag(init.tag).receiptId)
  assert.deepEqual(initReceipt.argv, ['project', 'init', 'demo/app', '<local path>'], 'a project source folder is local')
  assert.equal(initReceipt.execId, undefined); assert.equal(initReceipt.exit, undefined)
})

test('receipt argv stays bounded', () => {
  const long = ['file', 'write', ...Array.from({ length: 40 }, (_, i) => `word-${i}-${'x'.repeat(60)}`)]
  const argv = L.portableArgv(long)
  assert.equal(argv.length, L.RECEIPT_ARGV_WORDS + 1); assert.equal(argv.at(-1), '<10 more>')
  assert.ok(argv.slice(2, -1).every(w => Array.from(w).length <= 41))
  assert.ok(Buffer.byteLength(JSON.stringify(argv)) < 2000)
  assert.deepEqual(L.portableArgv(['file', 'read', 'demo/app', 'main', '/src/a.ts']), ['file', 'read', 'demo/app', 'main', '/src/a.ts'], 'project paths stay')
})

test('store import merges receipts by id', () => {
  const local = [{ id: 'a', operation: 'file.write', at: 1 }, { id: 'b', operation: 'file.read' }]
  const incoming = [{ id: 'b', operation: 'file.read', at: 5, argv: ['file', 'read'] }, { id: 'c', operation: 'workspace.exec', at: 9 }, null, { nope: 1 }]
  assert.deepEqual(L.mergeInvocations(local, incoming), [
    { id: 'a', operation: 'file.write', at: 1 },
    { id: 'b', operation: 'file.read', at: 5, argv: ['file', 'read'] },
    { id: 'c', operation: 'workspace.exec', at: 9, imported: true },
  ])
  assert.deepEqual(L.mergeInvocations(undefined, [{ id: 'x' }]), [{ id: 'x', imported: true }])
  assert.deepEqual(L.mergeInvocations([{ id: 'x' }], undefined), [{ id: 'x' }])
})

/** A logs ctx over one stand-in log and fake sidecars / session state. */
function logsCtx(callLog, sidecars, sessions = {}) {
  let ops
  const services = {
    sessionPersistence: { readSidecar: async (sid) => sidecars[sid] },
    layerfsSessions: { checkpoints: () => [], state: { read: () => ({ sessions, checkpoints: {} }) } },
  }
  L.applyLogs({ effect() {}, get: (name) => services[name], layerfs: { callLog, nativeLogs: async () => false, emit() {} }, layerfsRemote: { operate(_ctx, table) { ops = table } } })
  return ops
}

test('logs.list falls back to an imported session\'s receipts, pages them and skips ones this log holds', async () => {
  const { callLog } = place()
  const receipt = (rid, at, extra = {}) => ({ id: rid, origin: 'tool', sessionId: 'imp', project: 'demo/app', turn: 1, toolCallId: `t-${rid}`, operation: 'file.write', outcome: 'success',
    versions: [{ kind: 'Commit', id, effect: 'created' }], at, ms: 4, argv: ['file', 'write', 'demo/app', 'main', '/x', '<3 bytes>'], tag: `dsh:imp:${rid}`, kind: 'Committed', imported: true, ...extra })
  const sidecars = {
    imp: { title: 'Imported', invocations: [receipt('r1', 1000, { step }), receipt('r2', 2000), receipt('r3', 3000, { operation: 'file.read', argv: ['file', 'read', 'demo/app', 'main', '/x'], versions: [], kind: undefined })] },
    own: { invocations: [{ ...receipt('o1', 1000), sessionId: 'own', tag: 'dsh:own:o1', imported: undefined }] },
  }
  const ops = logsCtx(callLog, sidecars)
  let page = await ops['logs.list']({ tag: 'dsh:imp:' })
  assert.deepEqual(page.lines.map(l => l.tag), ['dsh:imp:r3', 'dsh:imp:r2', 'dsh:imp:r1'])
  assert.ok(page.lines.every(l => l.source === 'receipt' && l.client === 'cli' && !l.output))
  assert.equal(page.lines[2].commit, id); assert.equal(page.lines[2].dsh.toolCallId, 't-r1'); assert.deepEqual(page.lines[2].dsh.step, step)
  assert.equal(page.lines[2].dsh.title, 'Imported')
  assert.equal((await ops['logs.list']({ tag: 'dsh:imp:', writes: true })).lines.length, 2, 'filters apply to receipt lines')
  assert.deepEqual((await ops['logs.list']({ tag: 'dsh:imp:r2' })).lines.map(l => l.tag), ['dsh:imp:r2'])
  // Paging continues through a receipt cursor.
  page = await ops['logs.list']({ tag: 'dsh:imp:', limit: 2 })
  assert.equal(page.lines.length, 2); assert.equal(page.has_more, true); assert.equal(page.next, 'receipt:2')
  page = await ops['logs.list']({ tag: 'dsh:imp:', limit: 2, cursor: page.next })
  assert.deepEqual(page.lines.map(l => l.tag), ['dsh:imp:r1']); assert.equal(page.has_more, false)
  await assert.rejects(ops['logs.list']({ cursor: 'receipt:0' }), e => e.code === 'InvalidInput')
  // This machine's lines come first; a receipt whose line is here is not repeated.
  L.appendCall(callLog, { v: 1, at: 5000, ms: 1, client: 'cli', argv: ['file', 'write', 'demo/app', 'main', '/y'], outcome: 'ok', tag: 'dsh:imp:local' })
  L.appendCall(callLog, { v: 1, at: 3000, ms: 1, client: 'cli', argv: ['file', 'read', 'demo/app', 'main', '/x'], outcome: 'ok', tag: 'dsh:imp:r3' })
  page = await ops['logs.list']({ tag: 'dsh:imp:' })
  assert.deepEqual(page.lines.map(l => [l.tag, l.source ?? 'log']), [['dsh:imp:r3', 'log'], ['dsh:imp:local', 'log'], ['dsh:imp:r2', 'receipt'], ['dsh:imp:r1', 'receipt']])
  // Receipts recorded on this machine are never shown in place of a reset log.
  assert.equal((await ops['logs.list']({ tag: 'dsh:own:' })).lines.length, 0)
  // Without a session tag there is no fallback.
  assert.ok((await ops['logs.list']({})).lines.every(l => !l.source))
  assert.equal(L.sessionOfTag('dsh:-:'), undefined); assert.equal(L.sessionOfTag('dsh:imp:'), 'imp'); assert.equal(L.sessionOfTag('dsh:imp:r1'), 'imp')
})

test('simulation host calls carry dsh-sim tags, keep outputs and are labelled with their run', async () => {
  const { dir, callLog, outputs } = place(), cli = fakeCli(dir)
  const spec = { cli, home: join(dir, 'home'), callLog, nativeLogs: false }
  assert.equal(L.simTag('run-1', 'setup.store'), 'dsh-sim:run-1:setup.store')
  assert.equal(L.simTag('a:b', 'x'), undefined); assert.equal(L.simTag('r', 'x'.repeat(200)), undefined)
  assert.deepEqual(L.parseSimTag('dsh-sim:run-1:verify'), { runId: 'run-1', stepKey: 'verify' })
  assert.equal(L.parseTag('dsh-sim:run-1:verify'), undefined, 'never a session tag')
  await L.execute(spec, ['store', 'create', '--', 'demo'], { tag: 'dsh-sim:run-1:setup.store' })
  await L.execute(spec, ['store', 'list'], { tag: 'dsh-sim:run-1:verify' })
  await L.execute(spec, ['store', 'list'], { tag: 'forged' })
  // A session receipt's tag wins over a caller's.
  const receipts = []
  await L.withTraceContext({ origin: 'host', sessionId: 's', project: 'p', record: async row => receipts.push(row) }, () => L.execute(spec, ['store', 'list'], { tag: 'dsh-sim:run-1:verify' }))
  const lines = L.listCalls(callLog, {}).lines
  assert.deepEqual(lines.map(l => l.tag.split(':').slice(0, 2).join(':')), ['dsh:s', 'dsh:-', 'dsh-sim:run-1', 'dsh-sim:run-1'])
  assert.equal(lines[0].tag, receipts[0].tag)
  assert.ok(L.hasOutput(outputs, 'dsh-sim:run-1:setup.store'), 'a simulation call keeps its output')
  const ops = logsCtx(callLog, {}, { root: { simulation: { runId: 'run-1', agentKey: 'a' } }, child: { simulation: { runId: 'run-1', agentKey: 'b', rootSessionId: 'root' } } })
  const page = await ops['logs.list']({ tag: 'dsh-sim:run-1:' })
  assert.deepEqual(page.lines.map(l => l.sim), [{ runId: 'run-1', stepKey: 'verify', sessionId: 'root' }, { runId: 'run-1', stepKey: 'setup.store', sessionId: 'root' }])
  assert.ok(page.lines.every(l => l.output && !l.dsh))
  assert.equal((await ops['logs.output']({ tag: 'dsh-sim:run-1:setup.store' })).exit, 0)
})

test('stepOfCall joins a scripted tool call to its run and step, only for its own session', async () => {
  const repo = fileURLToPath(new URL('..', import.meta.url))
  const { mkdirSync, rmSync } = await import('node:fs')
  mkdirSync(join(repo, 'lib'), { recursive: true })
  const out = mkdtempSync(join(repo, 'lib/.test-receipt-step-'))
  try {
    await build({ absWorkingDir: repo, stdin: { contents: "export { SimulationRuntime } from './src/host/simulations/runtime.ts'", resolveDir: repo, loader: 'ts' }, outfile: join(out, 'runtime.mjs'), bundle: true, format: 'esm', platform: 'node', target: 'node24', external: ['@deepseek-ai/*'], logLevel: 'warning' })
    const { SimulationRuntime } = await import(pathToFileURL(join(out, 'runtime.mjs')))
    const runtime = new SimulationRuntime({})
    const binding = { sessionId: 'sim-1', run: { manifest: { runId: 'run-1' } } }
    runtime.calls.set('call-a', { binding, action: { type: 'tool', stepKey: 'cycle.1.write' }, argv: ['file', 'write'] })
    runtime.calls.set('call-b', { binding, action: { type: 'delegate', agentId: 'helper' } })
    assert.deepEqual(runtime.stepOfCall('sim-1', 'call-a'), { runId: 'run-1', stepKey: 'cycle.1.write' })
    assert.equal(runtime.stepOfCall('other', 'call-a'), undefined)
    assert.equal(runtime.stepOfCall('sim-1', 'call-b'), undefined)
    assert.equal(runtime.stepOfCall('sim-1', 'missing'), undefined)
  } finally { rmSync(out, { recursive: true, force: true }) }
})
