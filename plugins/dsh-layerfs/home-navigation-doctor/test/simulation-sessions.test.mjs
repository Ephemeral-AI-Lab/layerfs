import { test } from 'node:test'
import assert from 'node:assert/strict'
import { build } from 'esbuild'
import { mkdtempSync, mkdirSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

const root = fileURLToPath(new URL('..', import.meta.url))
mkdirSync(join(root, 'lib'), { recursive: true })
const out = mkdtempSync(join(root, 'lib', '.test-simulation-sessions-'))
await build({ absWorkingDir: root, stdin: { contents: "export * from './src/host/sessions/service.ts'; export { initialState } from './src/host/sessions/state.ts'", resolveDir: root, loader: 'ts' }, outfile: join(out, 'service.mjs'), bundle: true, format: 'esm', platform: 'node', target: 'node24', external: ['@deepseek-ai/*'] })
const { LayerfsSessions, initialState } = await import(pathToFileURL(join(out, 'service.mjs')).href)
test.after(() => rmSync(out, { recursive: true, force: true }))

function fixture() {
  const dir = mkdtempSync(join(root, '.dev-test-simulation-sessions-'))
  const state = initialState(), sidecars = new Map(), calls = [], agents = new Map(), active = new Set()
  const simulations = { isActive: (id) => active.has(id) }
  const persistence = { migrateProject: async () => {}, updateSidecar: async (id, fn) => { const v = sidecars.get(id) ?? { sessionId: id }; fn(v); sidecars.set(id, v) } }
  const lfs = {
    // 2.0 long forms: project get, NotFound for no Sandbox, `sandbox remove`, `workspace mount` answering `workspace` and `commit`
    project: { get: async () => ({ store: 'store', engine: 'sqlite', machine: 'native', state: 'ready' }) },
    branch: { fork: async (...args) => { calls.push(['fork', ...args]); return { commit: 'b'.repeat(66) } } },
    sandbox: { get: async () => { const e = new Error('absent'); e.code = 'NotFound'; throw e }, create: async (...args) => { calls.push(['sandbox', ...args]); return { ok: true } }, remove: async () => ({ ok: true }) },
    workspace: { mount: async (...args) => { calls.push(['mount', ...args]); return { path: dir, workspace: 'sandbox:main', commit: 'a'.repeat(66) } }, status: async () => { throw new Error('automatic status must not run') } },
  }
  const ctx = { layerfs: { root: dir, lfs, emit() {}, ensureNative: async () => {} },
    sessions: { get: (id) => agents.get(id)?.session }, workspaceRegistry: { list: () => [] },
    workingDirectory: { get: () => dir, set: async () => {} }, logger: () => ({ warn() {} }),
    get: (key) => key === 'sessionPersistence' ? persistence : key === 'agents' ? { get: (id) => agents.get(id) } : key === 'layerfsSimulations' ? simulations : undefined }
  const service = Object.create(LayerfsSessions.prototype)
  Object.defineProperties(service, { ctx: { value: ctx }, state: { value: { read: () => state, update: (fn) => fn(state) } }, lanes: { value: new Map() } })
  const makeAgent = (id, parent) => {
    let selected = { provider: 'simulation', model: `script-${id}` }
    const session = { id, seq: 2, header: { id, cwd: dir, ...(parent ? { origin: 'subagent', parentSession: parent } : {}) }, snapshotEvents: () => [], append() {} }
    const agent = { id, session, model: { current: () => selected, select: (value) => { selected = value } } }
    agents.set(id, agent)
    return agent
  }
  return { service, state, sidecars, calls, makeAgent, active, close: () => rmSync(dir, { recursive: true, force: true }) }
}
const meta = (key) => ({ runId: 'run-one', agentKey: key, scriptId: `script-${key}`, name: key, hash: 'a'.repeat(64), ordinaryModel: { provider: 'real', model: 'normal' } })

test('script parent and child are project-owned without automatic branches, mounts or saves', async () => {
  const f = fixture()
  try {
    f.active.add('parent'); f.active.add('child')
    await f.service.prepareSimulation('parent', 'store/project', meta('parent'))
    const parent = f.makeAgent('parent')
    await f.service.bindAgent(parent, 'create', true)
    await f.service.prepareSimulation('child', 'store/project', { ...meta('child'), rootSessionId: 'parent' })
    const child = f.makeAgent('child', 'parent')
    await f.service.bindAgent(child, 'create', true)
    await f.service.ensureBound(parent)
    await f.service.ensureBound(child)
    assert.equal(await f.service.checkpoint('parent', 'turn', 1, 'automatic'), undefined)
    assert.deepEqual(f.calls, [])
    assert.equal(f.service.binding('child').kind, 'shared')
    assert.equal(f.service.binding('child').parent, 'parent')
    assert.equal(f.sidecars.get('child').simulation.scriptId, 'script-child')
  } finally { f.close() }
})

test('handoff mounts a new ordinary context from the immutable receipt rather than current main', async () => {
  const f = fixture()
  try {
    f.active.add('parent')
    await f.service.prepareSimulation('parent', 'store/project', meta('parent'))
    const parent = f.makeAgent('parent')
    await f.service.bindAgent(parent, 'create', true)
    parent.model.select({ provider: 'real', model: 'normal' })
    await assert.rejects(f.service.ensureBound(parent), /Stop the active project script/)
    const target = { kind: 'Layer', id: 'b'.repeat(66), branch: 'main' }
    await f.service.completeSimulation('parent', target, 'completed')
    f.active.delete('parent')
    assert.deepEqual(f.calls, [], 'completion does not alter the graph before verifier')
    await f.service.ensureBound(parent)
    const fork = f.calls.find((call) => call[0] === 'fork')
    // 2.0 `branch fork <p> <name> main --at <commit>`
    assert.deepEqual(fork.slice(-2), ['main', { at: target.id }])
    assert.equal(f.calls.filter((call) => call[0] === 'mount').length, 1)
    assert.equal(f.service.binding('parent').simulation.continuation.id, target.id)
    assert.equal(f.sidecars.get('parent').simulation.handoffReady, true)
  } finally { f.close() }
})

test('a stopped or interrupted script without a pin continues from the project main like a fresh session', async () => {
  const f = fixture()
  try {
    f.active.add('parent')
    await f.service.prepareSimulation('parent', 'store/project', meta('parent'))
    const parent = f.makeAgent('parent')
    await f.service.bindAgent(parent, 'create', true)
    assert.equal(f.service.isSimulationManaged('parent'), true)
    await assert.rejects(f.service.forget('parent'), /Settle script execution/)
    // Stop mid-command: no confirmed continuation version.
    await f.service.completeSimulation('parent', undefined, 'failed', 'Stopped mid-command')
    f.active.delete('parent')
    assert.equal(f.service.isSimulationManaged('parent'), false, 'a terminal script never locks its session')
    parent.model.select({ provider: 'real', model: 'normal' })
    await f.service.ensureBound(parent)
    const fork = f.calls.find((call) => call[0] === 'fork')
    assert.equal(fork.length, 4, 'forked from the branch head, without a pinned commit')
    assert.equal(fork[3], 'main')
    assert.equal(f.calls.filter((call) => call[0] === 'mount').length, 1)
    assert.equal(f.service.binding('parent').state, 'bound')
    await f.service.forget('parent')
  } finally { f.close() }
})

test('after a host restart a running script record is not a lock and continues from main', async () => {
  const f = fixture()
  try {
    f.active.add('parent')
    await f.service.prepareSimulation('parent', 'store/project', meta('parent'))
    const parent = f.makeAgent('parent')
    await f.service.bindAgent(parent, 'create', true)
    // The new host has no live script binding; the persisted record still says running.
    f.active.clear()
    assert.equal(f.service.simulationOf('parent').state, 'running')
    assert.equal(f.service.isSimulationManaged('parent'), false)
    parent.model.select({ provider: 'real', model: 'normal' })
    await f.service.ensureBound(parent)
    assert.equal(f.calls.find((call) => call[0] === 'fork')[3], 'main')
    assert.equal(f.service.binding('parent').state, 'bound')
  } finally { f.close() }
})

test('two installations continue one archived conversation without sharing a mutable workspace', async () => {
  const a = fixture(), b = fixture()
  try {
    for (const f of [a, b]) {
      f.active.add('parent')
      await f.service.prepareSimulation('parent', 'store/project', meta('parent'))
      const agent = f.makeAgent('parent')
      await f.service.bindAgent(agent, 'create', true)
      agent.model.select({ provider: 'real', model: 'normal' })
      await f.service.completeSimulation('parent', { kind: 'Layer', id: 'c'.repeat(66), branch: 'main' }, 'completed')
      f.active.delete('parent')
      if (f === b) f.state.sessions.parent.resumeFromArchive = true
      await f.service.ensureBound(agent)
    }
    assert.notEqual(a.service.binding('parent').sandbox, b.service.binding('parent').sandbox)
    assert.notEqual(a.service.binding('parent').branch, b.service.binding('parent').branch)
    assert.deepEqual(a.calls.find(call => call[0] === 'fork').slice(-2), b.calls.find(call => call[0] === 'fork').slice(-2), 'both start at the same immutable files')
  } finally { a.close(); b.close() }
})
