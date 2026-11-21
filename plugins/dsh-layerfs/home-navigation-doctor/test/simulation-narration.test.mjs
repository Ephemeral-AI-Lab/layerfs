import { test, after } from 'node:test'
import assert from 'node:assert/strict'
import { build } from 'esbuild'
import { spawnSync } from 'node:child_process'
import { chmodSync, mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { BlockAssembler } from '@deepseek-ai/dsh-llm'
const root = fileURLToPath(new URL('..', import.meta.url))
mkdirSync(join(root, 'lib'), { recursive: true })
const out = mkdtempSync(join(root, 'lib/.test-simulation-narration-'))
await build({ absWorkingDir: root, stdin: { contents: "export * from './src/host/simulations/model.ts'; export * from './src/host/simulations/runtime.ts'; export * from './src/host/simulations/narration.ts'; export { renderSimulationCommand, SIMULATION_CLI } from './src/host/simulations/compiler.ts'", resolveDir: root, loader: 'ts' }, outfile: join(out, 'bundle.mjs'), bundle: true, format: 'esm', platform: 'node', target: 'node24', external: ['@deepseek-ai/*'], logLevel: 'warning' })
const { SimulationAdapter, SimulationRuntime, DEFAULT_PACING, pacingFromEnvironment, textDeltas, argumentDeltas, renderSimulationCommand, SIMULATION_CLI, describeCommand } = await import(pathToFileURL(join(out, 'bundle.mjs')))
const scratch = mkdtempSync(join(tmpdir(), 'sim-narration-'))
after(() => { rmSync(out, { recursive: true, force: true }); rmSync(scratch, { recursive: true, force: true }) })

const fast = { word: [0, 1], decision: [0, 1], argument: [0, 1] }
const scripts = () => [{ scriptId: 'script', name: 'Release work' }]
const assemble = chunks => { const assembler = new BlockAssembler(); for (const chunk of chunks) assembler.push(chunk); return assembler.blocks() }
const words = text => text.trim().split(/\s+/).filter(Boolean).length

test('narration and a tool call stream as one assistant message: text block 0, tool block 1', async () => {
  const call = { id: 'call-1', name: 'bash', arguments: { description: 'Fork feat-x from main', command: '"$DSH_LAYERFS_CLI" branch fork store/proj feat-x main' } }
  const adapter = new SimulationAdapter({ scripts, next: async () => ({ text: 'Forking `feat-x` from `main`.', calls: [call] }) }, { pacing: false })
  const chunks = await Array.fromAsync(adapter.stream({ provider: 'simulation', model: 'script', messages: [] }))
  assert.deepEqual(chunks.filter(chunk => chunk.type === 'block-start').map(chunk => [chunk.index, chunk.blockType]), [[0, 'text'], [1, 'tool-call']])
  const blocks = assemble(chunks)
  assert.equal(blocks.length, 2)
  assert.deepEqual(blocks[0], { type: 'text', text: 'Forking `feat-x` from `main`.' })
  assert.deepEqual(blocks[1], { type: 'tool-call', id: 'call-1', name: 'bash', arguments: JSON.stringify(call.arguments) })
  assert.equal(chunks.at(-1).reason.kind, 'tool-calls')
  assert.equal(chunks.some(chunk => chunk.type === 'usage'), false, 'no invented token usage')
  // Pacing off: one delta each.
  assert.equal(chunks.filter(chunk => chunk.type === 'text-delta').length, 1)
  assert.equal(chunks.filter(chunk => chunk.type === 'tool-call-delta').length, 1)
})

test('paced streaming sends two-to-four-word text deltas and two or three argument deltas, description first', async () => {
  const text = 'Merging `feature-a` and `feature-b` into `release-1`, and publish `release-1` to main once both are in.'
  const call = { id: 'call-2', name: 'bash', arguments: { description: 'Merge feature-a and feature-b into release-1', command: '"$DSH_LAYERFS_CLI" branch merge store/proj release-1 feature-a feature-b --message \'Integrate A and B\'' } }
  const adapter = new SimulationAdapter({ scripts, next: async () => ({ text, calls: [call] }) }, { pacing: fast })
  const chunks = await Array.fromAsync(adapter.stream({ provider: 'simulation', model: 'script', messages: [] }))
  const deltas = chunks.filter(chunk => chunk.type === 'text-delta').map(chunk => chunk.text)
  assert.ok(deltas.length > 3)
  assert.equal(deltas.join(''), text)
  for (const delta of deltas.slice(0, -1)) assert.ok(words(delta) >= 2 && words(delta) <= 4, delta)
  const args = chunks.filter(chunk => chunk.type === 'tool-call-delta')
  assert.ok(args.length >= 2 && args.length <= 3)
  assert.equal(args.map(chunk => chunk.argumentsDelta).join(''), JSON.stringify(call.arguments))
  assert.match(args[0].argumentsDelta, /^\{"description":"Merge feature-a and feature-b into release-1",?$/)
  assert.equal(args[0].name, 'bash')
  assert.deepEqual(assemble(chunks)[1].arguments, JSON.stringify(call.arguments))
  for (const sample of ['{"a":1}', JSON.stringify({ description: 'x', command: 'y'.repeat(200) })]) assert.equal(argumentDeltas(sample).join(''), sample)
  assert.equal(textDeltas('one two three four five six seven', () => 0).length, 4)
})

test('pacing is on by default, off with DSH_LAYERFS_SIMULATION_PACING=0, and delays each tool call', async () => {
  assert.equal(pacingFromEnvironment(undefined), DEFAULT_PACING)
  for (const value of ['0', 'off', 'false']) assert.equal(pacingFromEnvironment(value), null)
  assert.equal(pacingFromEnvironment('1'), DEFAULT_PACING)
  const reply = { calls: [{ id: 'a', name: 'bash', arguments: { description: 'd', command: 'true' } }] }
  const paced = new SimulationAdapter({ scripts, next: async () => reply }, { pacing: { ...fast, decision: [60, 60] } })
  const started = Date.now()
  await Array.fromAsync(paced.stream({ provider: 'simulation', model: 'script', messages: [] }))
  assert.ok(Date.now() - started >= 55)
  const controller = new AbortController()
  const slow = new SimulationAdapter({ scripts, next: async () => reply }, { pacing: { ...fast, decision: [5000, 5000] } })
  setTimeout(() => controller.abort(new Error('stopped')), 20)
  await assert.rejects(Array.fromAsync(slow.stream({ provider: 'simulation', model: 'script', messages: [], signal: controller.signal })), /stopped/)
  // Auxiliary requests stay instant and never consume script actions.
  let advanced = 0
  const auxiliary = new SimulationAdapter({ scripts, next: async () => { advanced++; return reply } }, { pacing: { ...fast, word: [5000, 5000] } })
  const title = await Array.fromAsync(auxiliary.stream({ provider: 'simulation', model: 'script', messages: [], purpose: 'title' }))
  assert.equal(advanced, 0)
  assert.equal(title.filter(chunk => chunk.type === 'text-delta').length, 1)
})

test('text said during a wait streams before the reply settles', async () => {
  let release
  const gate = new Promise(resolve => { release = resolve })
  const adapter = new SimulationAdapter({ scripts, next: async (options, stream) => { await stream.say('Waiting for the tokens revisions (a01-worker) to finish first.'); await gate; return { calls: [{ id: 'c', name: 'bash', arguments: { description: 'd', command: 'true' } }] } } }, { pacing: false })
  const iterator = adapter.stream({ provider: 'simulation', model: 'script', messages: [] })[Symbol.asyncIterator]()
  assert.deepEqual((await iterator.next()).value, { type: 'block-start', index: 0, blockType: 'text' })
  assert.match((await iterator.next()).value.text, /^Waiting for the tokens revisions/)
  release()
  const rest = []
  for (let item = await iterator.next(); !item.done; item = await iterator.next()) rest.push(item.value)
  assert.equal(rest[0].type, 'block-end')
  assert.equal(rest.find(chunk => chunk.type === 'block-start').index, 1)
})

test('commands read as typed with minimal quoting and stay byte-exact', () => {
  assert.equal(renderSimulationCommand({ key: 'fork' }, ['branch', 'fork', 'sim-store/proj', 'feat-x', 'main'], '/home/u/.dsh/layerfs/agent-bin/layerfs'), '/home/u/.dsh/layerfs/agent-bin/layerfs branch fork sim-store/proj feat-x main')
  assert.equal(renderSimulationCommand({ key: 'fork' }, ['branch', 'fork', 'p/q', 'x', 'main'], '/My Home/agent-bin/layerfs'), "'/My Home/agent-bin/layerfs' branch fork p/q x main")
  const fork = renderSimulationCommand({ key: 'fork' }, ['branch', 'fork', 'sim-store/proj', 'feat-x', 'main'])
  assert.equal(fork, '"$DSH_LAYERFS_CLI" branch fork sim-store/proj feat-x main')
  assert.equal(SIMULATION_CLI, '"$DSH_LAYERFS_CLI"')
  const merge = renderSimulationCommand({ key: 'merge' }, ['branch', 'merge', 'p/q', 'r', 'a', 'b', '--message', "Integrate A and B's work"])
  assert.equal(merge, `"$DSH_LAYERFS_CLI" branch merge p/q r a b --message 'Integrate A and B'\\''s work'`)
  // File content: here-document when exact, printf otherwise; run both through bash.
  const cli = join(scratch, 'cli')
  writeFileSync(cli, '#!/bin/sh\nprintf "%s\\n" "$@" > "$OUT.argv"\ncat > "$OUT.stdin"\n')
  chmodSync(cli, 0o755)
  const cases = [
    'Line one\nEOF\n$HOME `x` \\n\n',
    'No trailing newline $(echo nope)',
    'with \r carriage return\n',
  ]
  for (const [index, input] of cases.entries()) {
    const step = { key: `write-${index}`, input }
    const command = renderSimulationCommand(step, ['file', 'write', 'p/q', 'feat-x', '/docs/a b.md', '--message', 'Add docs'])
    if (input.endsWith('\n') && !input.includes('\r')) {
      assert.match(command, /<<'EOF2'\n/)
      assert.ok(command.endsWith('\nEOF2'))
    } else assert.match(command, /^printf '%s' /)
    const outPrefix = join(scratch, `out-${index}`)
    const result = spawnSync('bash', ['-c', command], { env: { ...process.env, DSH_LAYERFS_CLI: cli, OUT: outPrefix }, encoding: 'utf8' })
    assert.equal(result.status, 0, result.stderr)
    assert.equal(readFileSync(`${outPrefix}.stdin`, 'utf8'), input)
    assert.deepEqual(readFileSync(`${outPrefix}.argv`, 'utf8').split('\n').slice(0, -1), ['file', 'write', 'p/q', 'feat-x', '/docs/a b.md', '--message', 'Add docs'])
  }
})

test('bash descriptions come from argv', () => {
  const ws = 'sim-1:a-001-001'
  assert.equal(describeCommand(['branch', 'fork', 'p/q', 'feat-x', 'main']), 'Fork feat-x from main')
  assert.equal(describeCommand(['file', 'write', 'p/q', 'feat-x', '/docs/spec.md', '--message', 'm']), 'Write docs/spec.md on feat-x')
  // 2.0 words: `branch publish <p> <branch> --at <commit>`, `branch merge <p> <branch> <from> --into NEW`, `sandbox create <p> <sandbox>`
  assert.equal(describeCommand(['branch', 'publish', 'p/q', 'release-3', '--at', '02' + 'a'.repeat(64)]), 'Publish release-3 to main')
  assert.equal(describeCommand(['branch', 'merge', 'p/q', 'a', 'b', '--into', 'release-3', '--message', 'm']), 'Merge a and b into release-3')
  assert.equal(describeCommand(['sandbox', 'create', 'p/q', 'sim-1']), 'Create sandbox sim-1')
  assert.equal(describeCommand(['workspace', 'mount', 'sim-1', 'tokens-001', '--name', 'a-001-001']), 'Mount tokens-001 in sim-1 as a-001-001')
  assert.equal(describeCommand(['workspace', 'exec', ws, "printf '%s' 'x' > 'docs/revision.json'"]), 'Write docs/revision.json in workspace a-001-001')
  assert.equal(describeCommand(['workspace', 'commit', ws, '--message', 'm']), 'Commit workspace a-001-001')
  assert.equal(describeCommand(['workspace', 'unmount', ws]), 'Unmount workspace a-001-001')
})

const layer = '32' + 'b'.repeat(64), commit = '02' + 'c'.repeat(64)
function fixture({ receipts = {}, actions, tasks, agents, agentId = null, waitTasks = async () => {} } = {}) {
  const steps = [
    { key: 'fork', argv: ['branch', 'fork', 'p/q', 'release-1', 'main'], dependsOn: [] },
    { key: 'write', argv: ['file', 'write', 'p/q', 'release-1', '/docs/spec.md', '--message', 'Spec'], input: 'x\n', dependsOn: ['fork'] },
    { key: 'publish', argv: ['branch', 'publish', 'p/q', 'release-1', '--at', '<commit:write>'], commitFrom: 'write', dependsOn: ['write'] },
  ]
  const script = { scriptId: 'main-script', agentKey: 'c-main', conversationId: 'c', agentId, turns: [{ id: 'turn-01', taskIds: (tasks ?? []).map(task => task.id), actions }] }
  const plan = { steps, tasks: tasks ?? [], agents: [script, ...(agents ?? [])] }
  const calls = []
  const run = { plan, directory: scratch, bindings: new Map(), journal: { receipts, records: [] }, execution: { waitTasks, waitSteps: async () => {}, validateCommand: () => {}, execute: async (key, context, executor, signal) => { calls.push(key); return executor(steps.find(step => step.key === key).argv, steps.find(step => step.key === key), signal) } } }
  const runtime = new SimulationRuntime({ layerfs: { agentCli: () => '/plugin/agent-bin/layerfs' } })
  const binding = { script, run, sessionId: 'session', rootSessionId: 'session', state: 'running', turn: 0, controller: new AbortController() }
  runtime.bindings.set('session', binding)
  return { runtime, binding, run, calls, options: { sessionId: 'session', model: 'main-script', messages: [] } }
}
const releaseTask = { id: 'release', title: 'Publish release 1 with typography maintenance', cycle: 1, conversationId: 'c', agentId: null, stepKeys: ['fork', 'write', 'publish'], dependsOn: [] }

test('a task narrates its intent in the message that issues its first tool, and the call is exact-match authorized', async () => {
  const f = fixture({ tasks: [releaseTask], actions: [{ type: 'assignment', taskId: 'release' }, { type: 'wait', tasks: [] }, { type: 'tool', stepKey: 'fork', taskId: 'release' }, { type: 'tool', stepKey: 'write', taskId: 'release' }, { type: 'report', taskId: 'release' }] })
  const reply = await f.runtime.next(f.options)
  assert.equal(reply.text, "Forking `release-1` from `main`. I'll revise `docs/spec.md` on `release-1`. Then I'll publish `release-1` to main.")
  assert.equal(reply.calls.length, 1)
  assert.deepEqual(reply.calls[0].arguments, { description: 'Fork release-1 from main', command: '/plugin/agent-bin/layerfs branch fork p/q release-1 main' })
  const id = reply.calls[0].id
  const ok = { isError: false, value: { kind: 'foreground', exitCode: 0, aborted: false, timedOut: false, stdout: { text: '{"kind":"Forked"}\n', truncated: false } } }
  const agent = { id: 'session' }
  await assert.rejects(f.runtime.executeTool({ agent, callId: id, name: 'bash', arguments: { command: "'/plugin/agent-bin/layerfs' 'branch' 'fork' 'p/q' 'release-1' 'main'" }, signal: new AbortController().signal }, async () => ok), /differs from the authorized/)
  assert.equal(await f.runtime.executeTool({ agent, callId: id, name: 'bash', arguments: reply.calls[0].arguments, signal: new AbortController().signal }, async () => ok), ok)
  assert.deepEqual(f.calls, ['fork'])
  // The next request issues the write without repeating the narration.
  f.run.journal.receipts.fork = { kind: 'Forked' }
  const second = await f.runtime.next({ ...f.options, messages: [{ role: 'tool', toolCallId: id, content: [{ type: 'text', text: '{}' }] }] })
  assert.equal(second.text, undefined)
  assert.equal(second.calls[0].arguments.description, 'Write docs/spec.md on release-1')
  assert.match(second.calls[0].arguments.command, /<<'EOF'\nx\nEOF$/)
})

test('the final reply is built from confirmed receipts, not a fixed sentence', async () => {
  // 2.0 answers: every saving call answers `commit`
  const receipts = { fork: { commit: layer }, write: { commit }, publish: { commit: layer } }
  const f = fixture({ receipts, tasks: [releaseTask], actions: [{ type: 'assignment', taskId: 'release' }, { type: 'wait', tasks: [] }, ...['fork', 'write', 'publish'].map(stepKey => ({ type: 'tool', stepKey, taskId: 'release' })), { type: 'report', taskId: 'release' }] })
  f.binding.script.turns.push({ id: 'turn-02', taskIds: [], actions: [] }) // not the last turn: no final verification
  const reply = await f.runtime.next(f.options)
  assert.equal(reply.calls, undefined)
  assert.match(reply.text, /^Release 1 is published\./)
  assert.match(reply.text, /- Forked `release-1`\./)
  assert.match(reply.text, /- Committed `docs\/spec\.md` on `release-1` \(latest commit `02cccccccccc`\)\./)
  assert.match(reply.text, new RegExp(`- Published \`release-1\` to main as Layer \`${layer.slice(0, 12)}\``))
  assert.match(reply.text, /no application build or tests were run/)
  assert.doesNotMatch(reply.text, /requested project changes are saved|commands|steps/)
  assert.equal(f.binding.complete, true)
})

test('independent delegations leave together with a child-title description and a delegation line', async () => {
  const tokens = { id: 'tokens', title: 'Revise tokens for release 1', cycle: 1, conversationId: 'c', agentId: 'a01-worker', stepKeys: [], dependsOn: [] }
  const forms = { id: 'forms', title: 'Revise forms for release 1', cycle: 1, conversationId: 'c', agentId: 'a02-worker', stepKeys: [], dependsOn: [] }
  const child = (agentId, task) => ({ agentKey: `c-${agentId}`, agentId, conversationId: 'c', scriptId: `${agentId}-script`, name: `Kit: ${agentId}`, turns: [{ request: `Do ${task.id}`, taskIds: [task.id] }] })
  const f = fixture({ tasks: [tokens, forms], agents: [child('a01-worker', tokens), child('a02-worker', forms)], actions: [{ type: 'delegate', taskId: 'tokens', agentId: 'a01-worker' }, { type: 'delegate', taskId: 'forms', agentId: 'a02-worker' }] })
  f.run.bindings.set('c-a02-worker', { sessionId: 'existing-child' })
  const reply = await f.runtime.next(f.options)
  assert.equal(reply.text, 'Delegating the tokens revisions to a01-worker and the forms revisions to a02-worker.')
  assert.deepEqual(reply.calls.map(call => call.name), ['subagent', 'send_message'])
  assert.equal(reply.calls[0].arguments.description, 'Revise tokens for release 1')
  assert.equal(reply.calls[0].arguments.model, 'a01-worker-script')
  assert.deepEqual(reply.calls[1].arguments, { agent_id: 'existing-child', message: 'Do forms' })
})

test('a dependency wait says what it waits for before blocking', async () => {
  let release
  const tokens = { id: 'tokens', title: 'Revise tokens for release 1', cycle: 1, conversationId: 'c', agentId: 'a01-worker', stepKeys: ['fork'], dependsOn: [] }
  const integrate = { ...releaseTask, id: 'integrate', title: 'Integrate tokens and forms for release 1', stepKeys: ['write'] }
  const f = fixture({ tasks: [tokens, integrate], actions: [{ type: 'assignment', taskId: 'integrate' }, { type: 'wait', tasks: ['tokens'] }, { type: 'tool', stepKey: 'write', taskId: 'integrate' }], waitTasks: () => new Promise(resolve => { release = resolve }) })
  const said = []
  const pending = f.runtime.next(f.options, { say: async text => { said.push(text) } })
  await new Promise(resolve => setImmediate(resolve))
  assert.deepEqual(said, ["I'll revise `docs/spec.md` on `release-1`. Waiting for the tokens revisions (a01-worker) to finish first."])
  f.run.journal.receipts.fork = { kind: 'Forked' }
  release()
  const reply = await pending
  assert.equal(reply.text, undefined, 'the intent was already said')
  assert.equal(reply.calls[0].arguments.description, 'Write docs/spec.md on release-1')
})

test('a failed tool is narrated with its output tail, then the run settles through the ordinary failure path', async () => {
  const f = fixture({ tasks: [releaseTask], actions: [{ type: 'tool', stepKey: 'fork', taskId: 'release' }] })
  let idle
  f.binding.agent = { id: 'session', whenIdle: () => new Promise(resolve => { idle = resolve }) }
  const failures = []
  f.runtime.fail = (agent, error) => failures.push([agent.id, error.message])
  const id = 'sim-main-script-turn-01-0'
  const reply = await f.runtime.next({ ...f.options, messages: [{ role: 'tool', toolCallId: id, isError: true, content: [{ type: 'text', text: 'starting\nerror: branch release-1 already exists' }] }] })
  assert.match(reply.text, /^The last command \(Fork release-1 from main\) failed\. Its output ended with:/)
  assert.match(reply.text, /error: branch release-1 already exists/)
  assert.equal(reply.calls, undefined)
  assert.deepEqual(failures, [])
  idle()
  await new Promise(resolve => setImmediate(resolve))
  assert.deepEqual(failures, [['session', 'A project tool call failed. Inspect its native result before continuing.']])
  await assert.rejects(f.runtime.next(f.options), /tool call failed/, 'a later request does not narrate again')
})
