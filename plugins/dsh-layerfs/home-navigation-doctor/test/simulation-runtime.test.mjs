import { test, after } from 'node:test'
import assert from 'node:assert/strict'
import { build } from 'esbuild'
import { existsSync, mkdtempSync, mkdirSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { apply as installNativeSubagentControl } from '@deepseek-ai/dsh-tool-subagent-control'
const root = fileURLToPath(new URL('..', import.meta.url))
mkdirSync(join(root, 'lib'), { recursive: true })
const out = mkdtempSync(join(root, 'lib/.test-simulation-runtime-'))
await build({ absWorkingDir: root, stdin: { contents: "export * from './src/host/simulations/model.ts'; export * from './src/host/simulations/runtime.ts'; export { simulationArguments } from './src/host/simulations/index.ts'; export { SimulationExecution } from './src/host/simulations/gate.ts'; export { admitStore } from './src/host/layerfs/cli.ts'", resolveDir: root, loader: 'ts' }, outfile: join(out, 'runtime.mjs'), bundle: true, format: 'esm', platform: 'node', target: 'node24', external: ['@deepseek-ai/*'], logLevel: 'warning' })
const { SimulationAdapter, SimulationRuntime, SimulationExecution, bashReceipt, simulationArguments, admitStore } = await import(pathToFileURL(join(out, 'runtime.mjs')))
after(() => rmSync(out, { recursive: true, force: true }))

test('model adapter emits native tool blocks without fake usage, auxiliary calls cannot advance actions', async () => {
  let advanced = 0
  const adapter = new SimulationAdapter({ scripts: () => [{ scriptId: 'billing', name: 'Billing release' }], next: async () => { advanced++; return { id: 'call-1', name: 'bash', arguments: { command: 'true' } } } }, { pacing: false })
  const auxiliary = await Array.fromAsync(adapter.stream({ provider: 'simulation', model: 'billing', messages: [], purpose: 'title' }))
  assert.equal(advanced, 0)
  assert.equal(auxiliary.find(chunk => chunk.type === 'text-delta').text, 'Billing release')
  const action = await Array.fromAsync(adapter.stream({ provider: 'simulation', model: 'billing', messages: [] }))
  assert.equal(advanced, 1)
  assert.deepEqual(action.find(chunk => chunk.type === 'block-end').block, { type: 'tool-call', id: 'call-1', name: 'bash', arguments: '{"command":"true"}' })
  assert.equal(action.some(chunk => chunk.type === 'usage'), false)
  assert.equal(action.at(-1).reason.kind, 'tool-calls')
})

test('canonical bash parsing refuses promotion, truncation and uncertain exits', () => {
  const good = { isError: false, value: { kind: 'foreground', exitCode: 0, aborted: false, timedOut: false, stdout: { text: '{"kind":"Committed"}\n', truncated: false } } }
  assert.deepEqual(bashReceipt(good), { kind: 'Committed' })
  for (const changes of [{ exitCode: 1 }, { aborted: true }, { timedOut: true }, { stdout: { text: '{}', truncated: true } }, { kind: 'promoted', exitCode: undefined }, { stdout: { text: '{}\n{}', truncated: false } }]) assert.throws(() => bashReceipt({ ...good, value: { ...good.value, ...changes } }))
})

test('root metadata does not make a root its own shared child', () => {
  const runtime = new SimulationRuntime({})
  const root = { run: { manifest: { runId: 'run' } }, script: { agentKey: 'c-main', scriptId: 'root', name: 'Release', hash: 'h', agentId: null }, rootSessionId: 'parent', state: 'running', ordinaryModel: { provider: 'real', model: 'model', reasoningEffort: 'high' } }
  assert.equal(runtime.metadata(root).rootSessionId, undefined)
  assert.equal(runtime.metadata({ ...root, script: { ...root.script, agentId: 'worker' } }).rootSessionId, 'parent')
  assert.equal(runtime.metadata(root).ordinaryModel.reasoningEffort, 'high')
})

test('each child gets its own route and repeated assignments reuse its native identity', async () => {
  const runtime = new SimulationRuntime({})
  const child = { agentKey: 'c-worker', agentId: 'worker', conversationId: 'c', scriptId: 'worker-script', name: 'Review', turns: [{ request: 'Review release', taskIds: ['child-task'] }] }
  const script = { agentKey: 'c-main', conversationId: 'c', scriptId: 'main-script', turns: [{ id: 'turn-01', taskIds: ['child-task'], actions: [{ type: 'delegate', taskId: 'child-task', agentId: 'worker' }] }] }
  const run = { plan: { agents: [script, child] }, bindings: new Map() }
  const binding = { script, run, sessionId: 'parent', state: 'running', turn: 0, controller: new AbortController() }
  runtime.bindings.set('parent', binding)
  const options = { sessionId: 'parent', model: 'main-script', messages: [] }
  const first = (await runtime.next(options)).calls[0]
  assert.equal(first.name, 'subagent')
  assert.equal(first.arguments.provider, 'simulation')
  assert.equal(first.arguments.model, 'worker-script')
  assert.equal(first.arguments.prompt, 'Review release')
  run.bindings.set('c-worker', { sessionId: 'native-child-id' })
  const reused = (await runtime.next(options)).calls[0]
  assert.equal(reused.name, 'send_message')
  assert.deepEqual(reused.arguments, { agent_id: 'native-child-id', message: 'Review release' })
  const definitions = new Map(), delivered = []
  installNativeSubagentControl({ tools: { register: definition => definitions.set(definition.name, definition) }, agents: {}, subagents: { sendMessage: async (...args) => { delivered.push(args); return 'native-message-id' } } })
  const native = definitions.get(reused.name)
  assert.equal(native.parameters.required.includes('agent_id'), true, 'installed tool requires the concrete native child ID')
  assert.equal(native.parameters.properties.agent, undefined)
  const sender = { id: 'parent' }, signal = new AbortController().signal
  await native.execute(reused.arguments, { agent: sender, signal })
  assert.equal(delivered[0][0], sender)
  assert.equal(delivered[0][1], 'native-child-id')
  assert.deepEqual(delivered[0][2], [{ type: 'text', text: 'Review release' }])
})

test('a failed native tool result cannot become a successful summary', async () => {
  const runtime = new SimulationRuntime({})
  const script = { scriptId: 'main', turns: [{ id: 'turn-01', actions: [{ type: 'tool', stepKey: 'edit' }] }] }
  runtime.bindings.set('session', { script, turn: 0, state: 'running', controller: new AbortController(), run: {} })
  await assert.rejects(runtime.next({ sessionId: 'session', model: 'main', messages: [{ role: 'tool', toolCallId: 'sim-main-turn-01-0', isError: true }] }), /tool call failed/)
})

test('continuation reads a typed confirmed publication, never a bare current head', () => {
  const runtime = new SimulationRuntime({})
  const id = 'ab'.repeat(33)
  const binding = { script: { conversationId: 'c' }, run: { plan: { agents: [], continuationTargets: { c: { receipt: 'publish', type: 'Layer', branch: 'main' } } }, journal: { records: [], receipts: { publish: { commit: id } } } } }
  assert.deepEqual(runtime.target(binding), { kind: 'Layer', id, branch: 'main' })
  binding.run.journal.receipts.publish = { head: id }
  assert.equal(runtime.target(binding), undefined)
})

test('stop closes admission before cancellation and restores only native session selection after idle', async () => {
  const calls = []
  const runtime = new SimulationRuntime({ layerfsSessions: { completeSimulation: async (...args) => calls.push(['persist', ...args]) } })
  const id = 'ab'.repeat(33), ordinaryModel = { provider: 'real', model: 'model', reasoningEffort: 'high' }
  const script = { agentKey: 'c-main', conversationId: 'c', agentId: null, turns: [{ request: 'Prepared request' }] }
  const run = { plan: { agents: [script], continuationTargets: { c: { receipt: 'publish', type: 'Layer', branch: 'main' } } }, journal: { records: [], receipts: { publish: { commit: id } }, append: async record => calls.push(['journal', record]) }, bindings: new Map(), execution: { stop: async () => calls.push(['gate-stopped']) } }
  const binding = { run, script, sessionId: 'parent', rootSessionId: 'parent', ordinaryModel, scriptRequests: new Set(['script']), state: 'running', controller: new AbortController() }
  const followup = { id: 'human', content: [{ type: 'text', text: 'Human followup' }] }
  const scripted = { id: 'script', content: [{ type: 'text', text: 'Prepared request' }] }
  binding.agent = { inbox: { nextTurn: [scripted, followup], nextStep: [], remove: id => calls.push(['remove', id]) }, cancel: () => { assert.equal(binding.controller.signal.aborted, true); calls.push(['cancel']) }, whenIdle: async () => calls.push(['idle']), model: { select: selection => calls.push(['model', selection]) } }
  runtime.bindings.set('parent', binding); run.bindings.set('c-main', binding)
  await runtime.stop('parent')
  assert.equal(binding.state, 'stopped')
  assert.deepEqual(calls.filter(call => call[0] === 'remove'), [['remove', 'script']])
  assert.ok(calls.findIndex(call => call[0] === 'model') > calls.findIndex(call => call[0] === 'idle'))
  assert.deepEqual(calls.find(call => call[0] === 'model')[1], ordinaryModel)
  await assert.rejects(runtime.next({ sessionId: 'parent', model: 'anything' }), /no longer active/)
})

test('slash arguments have one unambiguous host command form', () => {
  assert.deepEqual(simulationArguments('--project startup --machine native --mixed'), { project: 'startup', machine: 'native', mixed: true })
  for (const input of ['', '--project startup', '--project startup --machine native --machine native', '--project startup --machine native --cycles 1']) assert.throws(() => simulationArguments(input))
})

test('queued human messages stay out of the script and return through the native inbox after handoff', async () => {
  const removed = [], sent = [], records = []
  const runtime = new SimulationRuntime({})
  const script = { agentKey: 'root', turns: [{ request: 'Prepared work' }] }
  const human = { id: 'human-message', source: { kind: 'user' }, content: [{ type: 'text', text: 'Read the previous result and continue' }] }
  const binding = { sessionId: 'parent', state: 'running', script, run: { journal: { append: async record => records.push(record) } }, agent: { inbox: { remove: id => removed.push(id) }, send: (...args) => sent.push(args) } }
  runtime.bindings.set('parent', binding)
  runtime.inserted({ id: 'parent' }, human)
  await Promise.resolve()
  assert.deepEqual(removed, ['human-message'])
  assert.equal(sent.length, 0)
  assert.deepEqual(records[0], { type: 'human-deferred', agentKey: 'root', sessionId: 'parent', messageId: 'human-message' })
  assert.equal('content' in records[0], false, 'private journal contains only a native message reference')
  binding.state = 'completed'
  await runtime.releaseHumans(binding, true)
  assert.deepEqual(sent, [[human, 'next-turn', true]])
  assert.equal(binding.parkedHuman.size, 0)
})

test('next script turns precede queued humans without changing the ordinary model default', async () => {
  const inserted = [], modelSelections = []
  const runtime = new SimulationRuntime({})
  const binding = { script: { agentKey: 'root', agentId: null, turns: [{ request: 'First' }, { request: 'Second' }] }, state: 'running', complete: true, turn: 0, sessionId: 'parent', run: { journal: { append: async () => {} } } }
  const agent = { id: 'parent', inbox: { prepend: (...args) => inserted.push(args) }, model: { select: value => modelSelections.push(value) } }
  binding.agent = agent
  runtime.bindings.set('parent', binding)
  await runtime.turnStopping(agent)
  assert.equal(inserted[0][0], 'next-turn')
  assert.equal(inserted[0][1].content[0].text, 'Second')
  assert.equal(modelSelections.length, 0)
  assert.equal(binding.turn, 1)
})

test('recovered human queue references are restored from native logs without executing a script', async () => {
  const sent = [], records = []
  const runtime = new SimulationRuntime({})
  const message = { id: 'human-message', source: { kind: 'user' }, content: [{ type: 'text', text: 'Follow up' }] }
  runtime.recoveredDeferred.set('parent', { journal: { append: async record => records.push(record) }, ids: new Set(['human-message']) })
  const agent = { id: 'parent', inbox: { nextTurn: [], nextStep: [] }, session: { snapshotEvents: () => [{ type: 'agent/inbox/spliced', data: { inserted: [message] } }] }, send: (...args) => sent.push(args) }
  await runtime.restoreDeferred(agent)
  assert.deepEqual(sent, [[message, 'next-turn', false]])
  assert.deepEqual(records, [{ type: 'human-released', sessionId: 'parent', messageId: 'human-message' }])
})

test('parking flushes a recovery reference before removal and pre-step filters a racing human claim', async () => {
  const removed = []
  let durable
  const flushed = new Promise(resolve => { durable = resolve })
  const runtime = new SimulationRuntime({ logger: () => ({ warn() {} }) })
  const message = { id: 'human', source: { kind: 'user' }, content: [{ type: 'text', text: 'The same prepared request' }] }
  const binding = { sessionId: 'parent', state: 'running', controller: new AbortController(), scriptRequests: new Set(['generated-script-message']), script: { scriptId: 'script', agentKey: 'root', turns: [{ request: 'The same prepared request' }] }, run: { journal: { append: () => flushed } }, agent: { id: 'parent', inbox: { remove: id => removed.push(id) } } }
  runtime.bindings.set('parent', binding)
  runtime.inserted(binding.agent, message)
  assert.deepEqual(removed, [], 'native pending input remains until journal flush succeeds')
  const decision = runtime.preStep({ agent: binding.agent, signal: new AbortController().signal, modelSelection: { provider: 'simulation', model: 'script' }, messages: [message] }, async () => ({ kind: 'enter', messages: [message] }))
  durable()
  const filtered = await decision
  assert.deepEqual(removed, ['human'])
  assert.deepEqual(filtered.messages, [], 'claimed human input cannot become script instructions')
  assert.equal(binding.parkedHuman.get('human'), message, 'identical text does not grant a generated request identity')
})

test('a script route cannot claim a child outside its native parent lineage', async () => {
  let prepared = 0
  const runtime = new SimulationRuntime({ layerfsSessions: { prepareSimulation: async () => { prepared++ } } })
  const script = { scriptId: 'worker-script', agentKey: 'c-worker', agentId: 'worker', conversationId: 'c' }
  const run = { plan: { agents: [script] }, bindings: new Map([['c-main', { sessionId: 'real-parent', state: 'running' }]]) }
  runtime.runs.set('run', run)
  for (const header of [{ origin: undefined, parentSession: undefined }, { origin: 'subagent', parentSession: 'unrelated-parent' }]) {
    const agent = { id: 'intruder', model: { current: () => ({ provider: 'simulation', model: 'worker-script' }) }, session: { header } }
    await assert.rejects(runtime.created(agent), /own native parent/)
  }
  assert.equal(prepared, 0, 'no project ownership is changed on unauthorized child creation')
})

test('a final child restores its real route before native retirement', async () => {
  const selections = []
  const runtime = new SimulationRuntime({})
  const ordinaryModel = { provider: 'real', model: 'model', reasoningEffort: 'high' }
  const binding = { sessionId: 'child', state: 'running', complete: true, turn: 0, script: { agentKey: 'c-worker', agentId: 'worker', turns: [{ request: 'Work' }] }, ordinaryModel, run: { journal: { append: async () => {} } } }
  const agent = { id: 'child', model: { select: selection => selections.push(selection) } }
  runtime.bindings.set('child', binding)
  await runtime.turnStopping(agent)
  assert.deepEqual(selections, [ordinaryModel])
  assert.equal(binding.ordinaryRestored, true)
})

test('native child initial request retains appended delivery guidance and is never deferred as human input', async () => {
  const journal = [], removed = []
  const runtime = new SimulationRuntime({})
  const objective = 'Complete the release changes and return confirmed results.'
  const message = { id: 'native-initial', source: { kind: 'user' }, content: [{ type: 'text', text: objective }, { type: 'text', text: 'Your parent agent id is "parent". Before you finish, send your result to that agent with send_message({ agent_id: "parent", message: "<self-contained result>" }).' }] }
  const binding = { sessionId: 'child', state: 'running', script: { scriptId: 'worker-script', agentKey: 'worker', agentId: 'worker', turns: [{ request: objective }] }, run: { journal: { append: async record => journal.push(record) } }, agent: { id: 'child', inbox: { remove: id => removed.push(id) } } }
  runtime.bindings.set('child', binding)
  runtime.inserted(binding.agent, message)
  const decision = await runtime.preStep({ agent: binding.agent, modelSelection: { provider: 'simulation', model: 'worker-script' }, signal: new AbortController().signal, messages: [message] }, async () => ({ kind: 'enter', messages: [message] }))
  assert.deepEqual(journal, [], 'native generated prompt never enters human-deferred recovery')
  assert.deepEqual(removed, [])
  assert.equal(decision.messages[0], message, 'all native content blocks reach standard history intact')
  assert.equal(binding.scriptRequests.has('native-initial'), true)
})

test('Stop closes every descendant gate before waiting for parent human-input durability', async () => {
  let flush
  const durable = new Promise(resolve => { flush = resolve })
  const id = 'ab'.repeat(33)
  const runtime = new SimulationRuntime({ layerfsSessions: { completeSimulation: async () => {} } })
  const ordinaryModel = { provider: 'real', model: 'model' }
  const rootScript = { agentKey: 'c-main', conversationId: 'c', turns: [] }
  const childScript = { agentKey: 'c-child', conversationId: 'c', agentId: 'worker', turns: [] }
  const journal = { records: [], receipts: { publish: { commit: id } }, append: async event => { if (event.type === 'human-deferred') await durable } }
  const run = { plan: { agents: [rootScript, childScript], continuationTargets: { c: { receipt: 'publish', branch: 'main', type: 'Layer' } } }, bindings: new Map(), journal, execution: { stop: async () => {} } }
  const human = { id: 'human', source: { kind: 'user' }, content: [{ type: 'text', text: 'Continue afterward' }] }
  const makeAgent = (id, pending) => ({ id, inbox: { nextTurn: pending, nextStep: [], remove: () => {} }, cancel: () => {}, whenIdle: async () => {}, model: { select: () => {} }, send: () => {} })
  const rootBinding = { run, script: rootScript, sessionId: 'parent', rootSessionId: 'parent', ordinaryModel, state: 'running', controller: new AbortController(), agent: makeAgent('parent', [human]) }
  const childBinding = { run, script: childScript, sessionId: 'child', rootSessionId: 'parent', ordinaryModel, ordinaryRestored: true, state: 'running', controller: new AbortController(), agent: makeAgent('child', []) }
  for (const binding of [rootBinding, childBinding]) { runtime.bindings.set(binding.sessionId, binding); run.bindings.set(binding.script.agentKey, binding) }
  const stopping = runtime.stop('parent')
  assert.equal(rootBinding.controller.signal.aborted, true)
  assert.equal(childBinding.controller.signal.aborted, true, 'child admission closes before any parent journal fsync can yield')
  flush()
  await stopping
})

test('handoff writer failure settles the whole family as failed with unready pins instead of leaving Stop pending', async () => {
  const sidecars = new Map(), records = []
  const runtime = new SimulationRuntime({ layerfsSessions: { completeSimulation: async (id, target, state, error) => sidecars.set(id, { target, state, error }) }, logger: () => ({ warn() {} }) })
  const version = 'ab'.repeat(33), ordinaryModel = { provider: 'real', model: 'model' }
  const rootScript = { agentKey: 'c-main', conversationId: 'c', turns: [] }, childScript = { agentKey: 'c-worker', conversationId: 'c', agentId: 'worker', turns: [] }
  const run = { plan: { agents: [rootScript, childScript], continuationTargets: { c: { receipt: 'publish', type: 'Layer', branch: 'main' } } }, journal: { records, receipts: { publish: { commit: version } }, append: async event => records.push(event) }, bindings: new Map(), execution: { stop: async () => {} } }
  const make = (script, sessionId, throws) => ({ run, script, sessionId, rootSessionId: 'parent', ordinaryModel, ordinaryRestored: !!script.agentId, state: 'running', controller: new AbortController(), agent: { id: sessionId, inbox: { nextTurn: [], nextStep: [], remove() {} }, cancel() {}, whenIdle: async () => {}, model: { select() { if (throws) throw new Error('native writer unavailable') } } } })
  const rootBinding = make(rootScript, 'parent', true), childBinding = make(childScript, 'child', false)
  for (const binding of [rootBinding, childBinding]) { runtime.bindings.set(binding.sessionId, binding); run.bindings.set(binding.script.agentKey, binding) }
  const result = await runtime.stop('parent')
  assert.equal(result.state, 'failed')
  assert.match(result.error, /native writer unavailable/)
  assert.equal(rootBinding.state, 'failed')
  assert.equal(childBinding.state, 'failed')
  for (const id of ['parent', 'child']) {
    assert.equal(sidecars.get(id).target, undefined, 'a partially completed family cannot retain a ready pin')
    assert.equal(sidecars.get(id).state, 'failed')
  }
  assert.equal(rootBinding.ordinaryRestored, false, 'failed model publication never claims restoration succeeded')
  assert.equal(records.filter(record => record.type === 'driver-handoff-failed').length, 2)
})

test('imported script inspection rejects a run path outside the private package namespace', async () => {
  for (const runId of ['../outside', '/tmp/outside', '', 'a/b', '..', null]) {
    const runtime = new SimulationRuntime({ layerfs: { root: '/private-plugin' }, layerfsSessions: { simulationOf: () => ({ runId, hash: 'ab'.repeat(32), state: 'interrupted' }) } })
    await assert.rejects(runtime.inspect('imported-session'), /saved script identity is invalid/)
  }
})

function compactedSessionRuntime(action, nativeEvents, receipts = {}) {
  const runtime = new SimulationRuntime({})
  const script = { scriptId: 'active-script', agentKey: 'main', conversationId: 'c', agentId: 'worker', turns: [{ id: 'turn-01', taskIds: ['task'], actions: [action] }] }
  const wait = []
  const run = { journal: { receipts, records: [] }, execution: { waitTasks: async tasks => wait.push(tasks) }, bindings: new Map(), plan: {} }
  const binding = { script, run, sessionId: 'session', state: 'running', turn: 0, controller: new AbortController(), agent: { session: { snapshotEvents: from => nativeEvents.filter(event => event.seq >= from) } } }
  runtime.bindings.set('session', binding)
  return { runtime, binding, run, wait, options: { sessionId: 'session', model: 'active-script', messages: [{ role: 'user', source: { kind: 'compaction' }, content: [{ type: 'text', text: 'Earlier work was compacted.' }] }] } }
}
const compactedCallId = 'sim-active-script-turn-01-0'
const actualNativeActionEvents = (name, failed = false) => [
  { type: 'assistant/message', seq: 0, data: { turn: 1, step: 1, message: { role: 'assistant', content: [{ type: 'tool-call', id: compactedCallId, name, arguments: '{}' }] }, stream: [] } },
  { type: 'tool/call', seq: 1, data: { turn: 1, step: 1, callId: compactedCallId, name, arguments: '{}' } },
  { type: 'tool/result', seq: 2, data: { turn: 1, step: 1, message: { role: 'tool', source: { kind: 'tool', callId: compactedCallId }, toolCallId: compactedCallId, isError: failed, content: [{ type: 'text', text: failed ? 'policy denied' : 'actual result' }] } } },
]

test('surface compaction cannot reemit a write already confirmed in the native log and receipt journal', async () => {
  const f = compactedSessionRuntime({ type: 'tool', stepKey: 'saved-write', tool: 'bash' }, actualNativeActionEvents('bash'), { 'saved-write': { commit: '02' + 'a'.repeat(64) } })
  const reply = await f.runtime.next(f.options)
  assert.equal('text' in reply, true)
  assert.equal(f.runtime.calls.size, 0, 'no new tool call or attempt is created for a compacted result')
  assert.deepEqual(f.wait, [['task']])
  assert.deepEqual(f.run.journal.records, [], 'native action facts need no duplicate private transcript')
})

test('compacted native subagent start and repeated-child delivery results are never redelivered', async () => {
  for (const name of ['subagent', 'send_message']) {
    const f = compactedSessionRuntime({ type: 'delegate', taskId: 'child-task', agentId: 'worker' }, actualNativeActionEvents(name))
    const reply = await f.runtime.next(f.options)
    assert.equal('text' in reply, true)
    assert.equal(f.runtime.calls.size, 0, `${name} delivery is completed in the sole native log`)
  }
})

test('compaction cannot conceal native tool errors or uncertain issued delegations', async () => {
  const failed = compactedSessionRuntime({ type: 'tool', stepKey: 'write' }, actualNativeActionEvents('bash', true))
  await assert.rejects(failed.runtime.next(failed.options), /failed.*native result/)
  const uncertain = compactedSessionRuntime({ type: 'delegate', taskId: 'child-task', agentId: 'worker' }, actualNativeActionEvents('subagent').slice(0, 2))
  await assert.rejects(uncertain.runtime.next(uncertain.options), /no confirmed native result.*cannot replay/)
  assert.equal(uncertain.runtime.calls.size, 0)
})

test('compaction auxiliary returns a bounded faithful checkpoint without advancing any actions', async () => {
  let actions = 0
  const pin = '32' + 'a'.repeat(64)
  const adapter = new SimulationAdapter({ scripts: () => [{ scriptId: 'active-script', name: 'Billing release' }], next: async () => { actions++; throw new Error('must not execute') }, compactionContext: () => `Project: store/billing\nLatest confirmed saved-version references: Layer ${pin}` }, { pacing: false })
  const chunks = await Array.fromAsync(adapter.stream({ provider: 'simulation', model: 'active-script', purpose: 'compaction', maxTokens: 2000, messages: [
    { role: 'system', content: [{ type: 'text', text: 'Preserve the selected scratch folder and existing project ownership.' }] },
    { role: 'user', source: { kind: 'user' }, content: [{ type: 'text', text: 'Update billing rules and preserve boundary cases.' }] },
    { role: 'tool', source: { kind: 'tool', callId: 'actual-tool' }, toolCallId: 'actual-tool', content: [{ type: 'text', text: 'Confirmed branch publication. Additional exact output: ' + 'x'.repeat(20000) }] },
    { role: 'user', content: [{ type: 'text', text: 'Summarize the preceding conversation.' }] },
  ] }))
  const text = chunks.find(chunk => chunk.type === 'block-end').block.text
  assert.match(text, /Update billing rules and preserve boundary cases/)
  assert.match(text, /Preserve the selected scratch folder/)
  assert.match(text, /Confirmed branch publication/)
  assert.ok(text.includes(pin))
  assert.match(text, /excerpt truncated/)
  assert.ok(text.length <= 6000)
  assert.equal(actions, 0)
  assert.equal(chunks.some(chunk => chunk.blockType === 'tool-call' || chunk.type === 'usage'), false)
})

// ---- terminal states: final for the script, free for real models ----

const realModel = { provider: 'real', model: 'model', reasoningEffort: 'high' }
const layer = 'ab'.repeat(33)
const modelAgent = (id, route) => {
  const selections = []
  let current = route
  return { id, selections, inbox: { nextTurn: [], nextStep: [], remove() {} }, cancel() {}, whenIdle: async () => {}, send() {}, session: { snapshotEvents: () => [] }, model: { current: () => current, select: value => { selections.push(value); current = value } } }
}

test('after a restart a reopened script session switches back to its remembered real model', async () => {
  const sidecars = { parent: { runId: 'run', state: 'interrupted', ordinaryModel: realModel }, child: { runId: 'run', state: 'interrupted' } }
  const runtime = new SimulationRuntime({ layerfsSessions: { simulationOf: id => sidecars[id] }, agentDefaultModel: { currentSelection: () => ({ provider: 'default', model: 'fallback' }) } })
  const parent = modelAgent('parent', { provider: 'simulation', model: 'run-c01-main' })
  await runtime.created(parent)
  assert.deepEqual(parent.selections, [realModel], 'session-local selection of the remembered route')
  const child = modelAgent('child', { provider: 'simulation', model: 'run-c01-a01-worker' })
  await runtime.created(child)
  assert.deepEqual(child.selections, [{ provider: 'default', model: 'fallback' }], 'missing remembered route falls back to the default selection')
  const real = modelAgent('parent', realModel)
  await runtime.created(real)
  assert.deepEqual(real.selections, [], 'an already real route is left alone')
  // Pre-step backstop for an agent created before the record existed.
  const late = modelAgent('parent', { provider: 'simulation', model: 'gone' })
  let entered = false
  await runtime.preStep({ agent: late, modelSelection: late.model.current(), messages: [], signal: new AbortController().signal }, async () => { entered = true; return { kind: 'enter', messages: [] } })
  assert.equal(entered, true)
  assert.deepEqual(late.selections, [realModel])
})

test('stop mid-command ends without a pin, restores the real model and releases parked humans', async () => {
  const persisted = []
  const runtime = new SimulationRuntime({ layerfsSessions: { completeSimulation: async (...args) => persisted.push(args) } })
  const script = { agentKey: 'c-main', conversationId: 'c', agentId: null, turns: [{ request: 'Work' }] }
  const records = [{ type: 'intent', stepKey: 'write', agentKey: 'c-main' }]
  const run = { plan: { agents: [script], steps: [], continuationTargets: { c: { receipt: 'publish', branch: 'main', fallbacks: [{ receipt: 'publish', branch: 'main' }] } } }, journal: { records, receipts: { publish: { commit: layer } }, append: async record => records.push(record) }, bindings: new Map(), execution: { stop: async () => {} } }
  const agent = modelAgent('parent', { provider: 'simulation', model: 'script' })
  const sent = []
  agent.send = (...args) => sent.push(args)
  const human = { id: 'human', source: { kind: 'user' }, content: [{ type: 'text', text: 'Carry on' }] }
  const binding = { run, script, sessionId: 'parent', rootSessionId: 'parent', ordinaryModel: realModel, state: 'running', controller: new AbortController(), agent, parkedHuman: new Map([['human', human]]) }
  runtime.bindings.set('parent', binding); run.bindings.set('c-main', binding)
  const result = await runtime.stop('parent')
  assert.equal(result.state, 'failed', 'an unconfirmed mutation leaves no continuation pin')
  assert.equal(persisted[0][1], undefined)
  assert.doesNotMatch(persisted[0][3], /Reconcile/)
  assert.deepEqual(agent.selections, [realModel], 'the real model is selected even without a pin')
  assert.deepEqual(sent, [[human, 'next-turn', true]], 'parked human input is delivered to the real model')
  assert.equal(runtime.isActive('parent'), false)
  await assert.rejects(runtime.next({ sessionId: 'parent', model: 'script' }), /no longer active/)
})

function twoConversationRun(runtime) {
  const steps = [
    { key: 'a1', phase: 'conversation', conversationId: 'a', agentId: null, argv: ['file', 'write'], dependsOn: [] },
    { key: 'b1', phase: 'conversation', conversationId: 'b', agentId: null, argv: ['file', 'write'], dependsOn: [] },
    { key: 'c1', phase: 'conversation', conversationId: 'c', agentId: null, argv: ['branch', 'fork'], dependsOn: ['a1'] },
  ]
  const tasks = [{ id: 'ta', conversationId: 'a', stepKeys: ['a1'], dependsOn: [] }, { id: 'tb', conversationId: 'b', stepKeys: ['b1'], dependsOn: [] }, { id: 'tc', conversationId: 'c', stepKeys: ['c1'], dependsOn: ['ta'] }]
  const agents = ['a', 'b', 'c'].map(id => ({ agentKey: `${id}-main`, scriptId: `run-${id}-main`, conversationId: id, agentId: null, turns: [{ request: 'Work' }] }))
  const target = { receipt: 'setup.project', branch: 'main', initial: true }
  const journal = { records: [], receipts: { 'setup.project': { commit: layer }, b1: { commit: '02' + 'f'.repeat(64) } }, append: async record => journal.records.push(record) }
  const plan = { steps, tasks, agents, continuationTargets: Object.fromEntries(['a', 'b', 'c'].map(id => [id, { ...target, fallbacks: [target] }])) }
  const run = { manifest: { runId: 'run' }, plan, journal, bindings: new Map(), execution: new SimulationExecution(plan, journal, { directory: '/unused' }) }
  runtime.runs.set('run', run)
  for (const script of agents) {
    const id = `session-${script.conversationId}`
    const binding = { run, script, sessionId: id, rootSessionId: id, ordinaryModel: realModel, turn: 0, state: 'running', controller: new AbortController(), agent: modelAgent(id, { provider: 'simulation', model: script.scriptId }) }
    runtime.bindings.set(id, binding); run.bindings.set(script.agentKey, binding)
  }
  return run
}

test('stopping one root conversation leaves independent ones running and ends dependents as stopped', async () => {
  const persisted = new Map()
  const runtime = new SimulationRuntime({ layerfsSessions: { completeSimulation: async (id, target, state, error) => persisted.set(id, { target, state, error }) }, logger: () => ({ warn() {} }) })
  const run = twoConversationRun(runtime)
  await runtime.stop('session-a')
  assert.equal(runtime.bindings.get('session-a').state, 'stopped')
  assert.equal(runtime.bindings.get('session-b').state, 'running', 'an independent conversation is untouched')
  // B's last turn waits only for its own conversation; no whole-run verification while A is unfinished.
  await runtime.settleConversation(runtime.bindings.get('session-b'), new AbortController().signal)
  assert.equal(run.verification, undefined)
  // C waited on A's stopped work: it ends as stopped with an explanation, not as a dependency failure.
  await assert.rejects(run.execution.waitTasks(['ta'], new AbortController().signal), /did not complete/)
  assert.equal(run.execution.blockedByStop('c'), true)
  assert.equal(run.execution.blockedByStop('b'), false)
  runtime.fail(runtime.bindings.get('session-c').agent, new Error('Dependency a1 did not complete; execution cannot continue'))
  await runtime.bindings.get('session-c').stopping
  assert.equal(persisted.get('session-c').state, 'stopped')
  assert.match(persisted.get('session-c').error, /Another conversation of this run was stopped/)
  assert.deepEqual(runtime.bindings.get('session-c').agent.selections.at(-1), realModel)
  assert.equal(runtime.bindings.get('session-b').state, 'running')
})

test('stopping a child ends its root family as stopped', async () => {
  const persisted = new Map()
  const runtime = new SimulationRuntime({ layerfsSessions: { completeSimulation: async (id, target, state) => persisted.set(id, state) } })
  const rootScript = { agentKey: 'c-main', conversationId: 'c', agentId: null, turns: [] }, childScript = { agentKey: 'c-worker', conversationId: 'c', agentId: 'worker', turns: [] }
  const target = { receipt: 'setup.project', branch: 'main', initial: true }
  const run = { plan: { agents: [rootScript, childScript], steps: [], continuationTargets: { c: { ...target, fallbacks: [target] } } }, journal: { records: [], receipts: { 'setup.project': { commit: layer } }, append: async () => {} }, bindings: new Map(), execution: { stop: async () => {} } }
  const make = (script, sessionId) => ({ run, script, sessionId, rootSessionId: 'parent', ordinaryModel: realModel, state: 'running', controller: new AbortController(), agent: modelAgent(sessionId, { provider: 'simulation', model: sessionId }) })
  const root = make(rootScript, 'parent'), child = make(childScript, 'child')
  for (const binding of [root, child]) { runtime.bindings.set(binding.sessionId, binding); run.bindings.set(binding.script.agentKey, binding) }
  const result = await runtime.stop('child')
  assert.equal(result.state, 'stopped')
  assert.deepEqual(Object.fromEntries(persisted), { child: 'stopped', parent: 'stopped' })
  assert.deepEqual(child.agent.selections, [realModel])
  assert.deepEqual(root.agent.selections, [realModel])
})

test('launch refuses before creating any Store, project or package when no real model is usable', async () => {
  const dir = mkdtempSync(join(root, 'lib/.test-simulation-launch-'))
  try {
    let native = 0
    for (const [requested, fallback, resolve] of [[undefined, undefined], [{ provider: 'simulation', model: 'old-script' }, { provider: 'simulation', model: 'x' }], [{ provider: 'gone', model: 'm' }, undefined, async () => { throw new Error('unknown provider') }]]) {
      const runtime = new SimulationRuntime({ layerfs: { root: dir, cli: '/bin/false', ensureNative: async () => { native++ } }, agentDefaultModel: { currentSelection: () => fallback }, llm: resolve ? { resolveModelInfo: resolve } : undefined, logger: () => ({ warn() {} }) })
      await assert.rejects(runtime.launch({ project: 'design', machine: 'native', ordinaryModel: requested }), /usable real model.*Nothing was created/)
      assert.equal(runtime.runs.size, 0)
    }
    assert.equal(native, 0)
    assert.equal(existsSync(join(dir, 'simulations')), false)
  } finally { rmSync(dir, { recursive: true, force: true }) }
})

test('the model list offers only scripts that can still run and announces changes', () => {
  const emitted = []
  const runtime = new SimulationRuntime({ emit: (...args) => emitted.push(args) })
  const rootScript = { agentKey: 'c-main', scriptId: 'root', conversationId: 'c', agentId: null }
  const bound = { agentKey: 'c-a01', scriptId: 'child-bound', conversationId: 'c', agentId: 'a01' }
  const pending = { agentKey: 'c-a02', scriptId: 'child-pending', conversationId: 'c', agentId: 'a02' }
  const run = { plan: { agents: [rootScript, bound, pending] }, bindings: new Map() }
  const rootBinding = { script: rootScript, state: 'running' }
  run.bindings.set('c-main', rootBinding).set('c-a01', { script: bound, state: 'running' })
  runtime.runs.set('run', run)
  const ids = () => runtime.scripts().map(script => script.scriptId).sort()
  assert.deepEqual(ids(), ['child-bound', 'child-pending', 'root'])
  runtime.refreshModels()
  runtime.refreshModels()
  assert.equal(emitted.length, 1, 'unchanged lists are not re-announced')
  run.bindings.get('c-a01').ordinaryRestored = true
  assert.deepEqual(ids(), ['child-pending', 'root'], 'a child that finished its turns is no longer offered')
  rootBinding.state = 'stopped'
  run.bindings.get('c-a01').state = 'stopped'
  assert.deepEqual(ids(), [], 'a terminal family offers nothing, including children it never started')
  runtime.refreshModels()
  assert.equal(runtime.runs.size, 0, 'finished runs are dropped')
  assert.deepEqual(emitted.at(-1), ['llm/models-updated', 'simulation'])
  assert.equal(emitted.length, 2)
})

const sleep = ms => new Promise(resolve => setTimeout(resolve, ms))
const inboxAgent = (id) => {
  const agent = modelAgent(id, { provider: 'simulation', model: id })
  agent.injected = []
  agent.inbox = { nextTurn: [], nextStep: [], remove(messageId) { for (const list of [this.nextTurn, this.nextStep]) { const at = list.findIndex(item => item.id === messageId); if (at >= 0) list.splice(at, 1) } } }
  agent.inject = message => agent.injected.push(message)
  return agent
}
const settled = (child) => ({ id: `settled-${child}`, source: { kind: 'subagent-settled', form: 'notice', senderSessionId: child }, content: [{ type: 'text', text: `Background subagent ${child} was stopped before it finished.` }] })

test('a stopping root stays busy until its children settle; their notices wait for the real model without a wake', async () => {
  const live = new Set(['child'])
  const runtime = new SimulationRuntime({ agents: { get: id => live.has(id) ? {} : undefined } })
  const rootScript = { agentKey: 'c-main', conversationId: 'c', agentId: null }, childScript = { agentKey: 'c-worker', conversationId: 'c', agentId: 'worker' }
  const run = { bindings: new Map() }
  const root = { run, script: rootScript, sessionId: 'parent', rootSessionId: 'parent', state: 'running', agent: inboxAgent('parent') }
  const child = { run, script: childScript, sessionId: 'child', rootSessionId: 'parent', state: 'running', agent: inboxAgent('child') }
  run.bindings.set('c-main', root).set('c-worker', child)
  runtime.bindings.set('parent', root).set('child', child)
  // A running script receives notices as ordinary input.
  const early = settled('other')
  assert.equal(runtime.parkNotice(root, early, true), false)
  root.state = 'stopping'
  let held = true
  const hold = runtime.holdForChildren(root).then(() => { held = false })
  await sleep(80)
  assert.equal(held, true, 'the stopping root waits for its live child')
  const notice = settled('child')
  root.agent.inbox.nextStep.push(notice)
  runtime.inserted(root.agent, notice)
  assert.deepEqual(root.agent.inbox.nextStep, [], 'taken out of the inbox at once, so no wake finds pending input')
  await hold
  assert.equal(held, false)
  // Claimed into a step while stopping, a notice is kept too (the step is rejected).
  const claimed = settled('late')
  const decision = await runtime.preStep({ agent: root.agent, messages: [claimed], signal: new AbortController().signal, modelSelection: { provider: 'simulation', model: 'x' } }, async () => assert.fail('a stopping script never enters a step'))
  assert.deepEqual(decision, { kind: 'reject' })
  root.state = 'stopped'
  await runtime.releaseHumans(root, true)
  assert.deepEqual(root.agent.injected.map(message => message.id), ['settled-child', 'settled-late'], 'context for the next real turn, never a wake')
  assert.equal(root.parkedNotices.size, 0)
})

test('the root hold is bounded: a child that left without a notice releases it shortly after', async () => {
  const live = new Set(['child'])
  const runtime = new SimulationRuntime({ agents: { get: id => live.has(id) ? {} : undefined } })
  const run = { bindings: new Map() }
  const root = { run, script: { agentKey: 'c-main', agentId: null }, sessionId: 'parent', rootSessionId: 'parent', state: 'stopping' }
  run.bindings.set('c-main', root).set('c-worker', { run, script: { agentKey: 'c-worker', agentId: 'worker' }, sessionId: 'child', rootSessionId: 'parent', state: 'stopping' })
  const started = Date.now()
  setTimeout(() => live.delete('child'), 50)
  await runtime.holdForChildren(root)
  assert.ok(Date.now() - started < 1000)
  // Children are never held, nor is a root that is not stopping.
  await runtime.holdForChildren({ ...root, state: 'running' })
  const capped = Date.now()
  live.add('child')
  await runtime.holdForChildren(root, 100)
  assert.ok(Date.now() - capped < 500, 'never longer than its limit')
})

test('a finished run releases its calls and bindings, except a dormant child still holding parked input', async () => {
  const runtime = new SimulationRuntime({ layerfsSessions: { completeSimulation: async () => {} } })
  const rootScript = { agentKey: 'c-main', scriptId: 'root', conversationId: 'c', agentId: null, turns: [] }, childScript = { agentKey: 'c-worker', scriptId: 'worker', conversationId: 'c', agentId: 'worker', turns: [] }
  const target = { receipt: 'setup.project', branch: 'main', initial: true }
  const run = { manifest: { runId: 'run' }, plan: { agents: [rootScript, childScript], steps: [], continuationTargets: { c: { ...target, fallbacks: [target] } } }, journal: { records: [], receipts: { 'setup.project': { commit: layer } }, append: async () => {} }, bindings: new Map(), execution: { stop: async () => {} } }
  const make = (script, sessionId) => ({ run, script, sessionId, rootSessionId: 'parent', ordinaryModel: realModel, state: 'running', controller: new AbortController(), agent: modelAgent(sessionId, { provider: 'simulation', model: sessionId }) })
  const root = make(rootScript, 'parent'), child = make(childScript, 'child')
  for (const binding of [root, child]) { runtime.bindings.set(binding.sessionId, binding); run.bindings.set(binding.script.agentKey, binding) }
  runtime.runs.set('run', run)
  runtime.calls.set('call-1', { binding: root, action: { type: 'tool', stepKey: 'x' } })
  await runtime.stop('parent')
  assert.equal(runtime.runs.size, 0)
  assert.equal(runtime.calls.size, 0)
  assert.equal(runtime.bindings.size, 0)
  assert.equal(runtime.isActive('parent'), false)
  // A dormant child whose parked human input awaits its continuation owner is kept.
  const second = new SimulationRuntime({ layerfsSessions: { completeSimulation: async () => {} } })
  const kept = { ...child, state: 'failed', parkedHuman: new Map([['h', {}]]) }
  const done = { ...root, state: 'stopped' }
  const run2 = { ...run, bindings: new Map([['c-main', done], ['c-worker', kept]]) }
  done.run = kept.run = run2
  second.runs.set('run', run2); second.bindings.set('parent', done).set('child', kept)
  second.refreshModels()
  assert.equal(second.runs.size, 0)
  assert.deepEqual([...second.bindings.keys()], ['child'])
})

test('a scripted command takes the Store lane only when it opens the Store; waiting ends on abort', async () => {
  const hooks = []
  const runtime = new SimulationRuntime({ on: (name, hook) => hooks.push([name, hook]), layerfs: { home: '/lane-home' } })
  runtime.ensureStoreLane()
  const [[name, hook]] = hooks
  assert.equal(name, 'tools/execute')
  const binding = { sessionId: 's', run: { manifest: { store: 'st' } } }
  runtime.calls.set('sandbox', { binding, action: { type: 'tool' }, argv: ['sandbox', 'create', 'st/p', 'sb-1', 'native'] })
  runtime.calls.set('write', { binding, action: { type: 'tool' }, argv: ['file', 'write', 'st/p', 'main', '/a.md', '--message', 'm'] })
  let release
  const holder = admitStore('/lane-home', 'st', () => new Promise(resolve => { release = resolve }))
  await sleep(10)
  // A sandbox command goes through the daemon: it never waits for the Store lane.
  assert.equal(await hook({ callId: 'sandbox', agent: { id: 's' }, signal: new AbortController().signal }, async () => 'ran'), 'ran')
  const controller = new AbortController()
  let ran = false
  const waiting = hook({ callId: 'write', agent: { id: 's' }, signal: controller.signal }, async () => { ran = true })
  await sleep(10)
  controller.abort(new Error('stopped'))
  await assert.rejects(waiting, /stopped/)
  assert.equal(ran, false)
  release()
  await holder
  // The lane is free again, and re-entrant inside its own hold.
  assert.equal(await admitStore('/lane-home', 'st', () => admitStore('/lane-home', 'st', async () => 'nested')), 'nested')
})

test('Stop takes a settlement notice already queued for the next turn, then hands it to the real model without a wake', async () => {
  const runtime = new SimulationRuntime({ layerfsSessions: { completeSimulation: async () => {} } })
  const script = { agentKey: 'c-main', scriptId: 'root', conversationId: 'c', agentId: null, turns: [] }
  const target = { receipt: 'setup.project', branch: 'main', initial: true }
  const run = { manifest: { runId: 'run' }, plan: { agents: [script], steps: [], continuationTargets: { c: { ...target, fallbacks: [target] } } }, journal: { records: [], receipts: { 'setup.project': { commit: layer } }, append: async () => {} }, bindings: new Map(), execution: { stop: async () => {} } }
  const agent = inboxAgent('parent')
  const sent = []
  agent.send = (...args) => sent.push(args)
  const notice = settled('finished-child')
  agent.inbox.nextTurn.push(notice)
  let pendingWhenStopping
  agent.cancel = () => { pendingWhenStopping = agent.inbox.nextTurn.length + agent.inbox.nextStep.length }
  const binding = { run, script, sessionId: 'parent', rootSessionId: 'parent', ordinaryModel: realModel, state: 'running', controller: new AbortController(), agent }
  runtime.bindings.set('parent', binding); run.bindings.set('c-main', binding)
  await runtime.stop('parent')
  assert.equal(pendingWhenStopping, 0, 'nothing is left for the cancelled driver to open a turn with')
  assert.deepEqual(agent.injected, [notice])
  assert.deepEqual(sent, [], 'no wake')
})

test('an aborted script step holds a stopping session through the adapter, also during pacing', async () => {
  const held = []
  const controller = new AbortController()
  const adapter = new SimulationAdapter({ scripts: () => [{ scriptId: 's', name: 'S' }], next: async () => ({ text: 'Forking now', calls: [{ id: 'c1', name: 'bash', arguments: { command: 'true' } }] }), holdStopping: async id => { held.push(id) } }, { pacing: { word: [50, 50], decision: [200, 200], argument: [10, 10] } })
  const reading = Array.fromAsync(adapter.stream({ provider: 'simulation', model: 's', sessionId: 'root', messages: [], signal: controller.signal }))
  setTimeout(() => controller.abort(new Error('Stop')), 120)
  await assert.rejects(reading)
  assert.deepEqual(held, ['root'], 'the pause before a tool call is an abort point too')
  // An uninterrupted step never holds.
  await Array.fromAsync(adapter.stream({ provider: 'simulation', model: 's', sessionId: 'root', messages: [] }))
  assert.deepEqual(held, ['root'])
})

test('Stop holds an idle root in a DSH maintenance task until its children settle; a pruned session reports its saved state', async () => {
  const live = new Set(['child'])
  const runtime = new SimulationRuntime({ agents: { get: id => live.has(id) ? {} : undefined }, layerfsSessions: { simulationOf: id => id === 'old' ? { state: 'completed' } : undefined } })
  const run = { bindings: new Map() }
  let maintenance
  const agent = { status: 'idle', runMaintenance: task => (maintenance = task(new AbortController().signal)) }
  const root = { run, script: { agentKey: 'c-main', agentId: null }, sessionId: 'parent', rootSessionId: 'parent', state: 'stopping', agent }
  run.bindings.set('c-main', root).set('c-worker', { run, script: { agentKey: 'c-worker', agentId: 'worker' }, sessionId: 'child', rootSessionId: 'parent', state: 'stopping' })
  runtime.holdIdleRoot(root)
  let done = false
  maintenance.then(() => { done = true })
  await sleep(60)
  assert.equal(done, false)
  runtime.parkNotice(root, settled('child'))
  await maintenance
  // A busy root is not put in maintenance (the adapter and tool hooks hold it).
  let called = false
  runtime.holdIdleRoot({ ...root, agent: { status: 'running', runMaintenance: () => { called = true } } })
  assert.equal(called, false)
  assert.deepEqual(await runtime.stop('old'), { state: 'completed' })
  assert.deepEqual(await runtime.stop('never'), { state: 'idle' })
})
