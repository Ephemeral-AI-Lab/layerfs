// Two session modes: where a session works (placeOf: a project anchor binds,
// anywhere else is a plain folder session), the Scratch project made on first
// use, the pre-step guarantee (ensureBound) and the LayerFS session
// persistence row's import on a local miss, over fakes.
//   node --test test/sessions.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { build } from 'esbuild'
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

const root = fileURLToPath(new URL('..', import.meta.url))
// Inside the checkout, so the external DSH packages resolve from node_modules.
mkdirSync(join(root, 'lib'), { recursive: true })
const out = mkdtempSync(join(root, 'lib', '.test-sessions-'))
await build({
  absWorkingDir: root,
  stdin: {
    contents: "export * from './src/host/sessions/service.ts'\nexport * from './src/host/placement.ts'\nexport * as persistence from './src/host/session-persistence.ts'\nexport { initialState } from './src/host/sessions/state.ts'\nexport * from './src/host/sessions/availability.ts'\nexport * as tracing from './src/host/layerfs/tracing.ts'\n",
    resolveDir: root,
    loader: 'ts',
  },
  outfile: join(out, 'sessions.mjs'),
  bundle: true,
  format: 'esm',
  platform: 'node',
  target: 'node24',
  external: ['@deepseek-ai/*'],
  logLevel: 'warning',
})
const S = await import(pathToFileURL(join(out, 'sessions.mjs')).href)
test.after(() => rmSync(out, { recursive: true, force: true }))

class Refusal extends Error {
  constructor(code, message = code) { super(message); this.code = code }
}

/** A LayerfsSessions over an in-memory state and a fake lfs, without cordis. */
function sessions({ state = S.initialState(), lfs = {}, layerfsRoot = '/lfs-root' } = {}) {
  const s = Object.create(S.LayerfsSessions.prototype)
  const events = []
  const ctx = {
    layerfs: { root: layerfsRoot, scratchProject: 'dsh-scratch/scratch', lfs, emit: (type, data) => events.push({ type, data }), ensureNative: async () => {} },
    sessions: { get: () => undefined },
    logger: () => ({ warn() {}, info() {} }),
  }
  let value = state
  Object.defineProperty(s, 'ctx', { value: ctx })
  Object.defineProperty(s, 'state', { value: { read: () => value, update(fn) { const d = structuredClone(value); value = fn(d) ?? d; return value } } })
  Object.defineProperty(s, 'lanes', { value: new Map() })
  Object.defineProperty(s, 'storeHealth', { value: new Map() })
  Object.defineProperty(s, 'serverChecked', { value: new Map() })
  return { s, events, state: () => value }
}

const session = (id, header, events = []) => ({ id, header: { id, ...header }, snapshotEvents: () => events })
const ran = [{ type: 'turn/start', data: { turn: 1 } }]

test('placeOf: only recorded legacy project anchors bind; folder paths do not create ownership', () => {
  const state = S.initialState()
  state.projects['st/app'] = { project: 'st/app', anchor: '/lfs-root/projects/st/app', workspaceId: 'legacy-workspace' }
  const { s } = sessions({ state })
  assert.deepEqual(s.placeOf(session('a', { cwd: '/lfs-root/projects/st/app' })),
    { kind: 'project', project: 'st/app', anchor: '/lfs-root/projects/st/app' })
  assert.deepEqual(s.placeOf(session('b', { cwd: '/lfs-root/scratch' })), { kind: 'folder', cwd: '/lfs-root/scratch' })
  assert.deepEqual(s.placeOf(session('c', { cwd: '/Users/me/Documents/default-workspace' })), { kind: 'folder', cwd: '/Users/me/Documents/default-workspace' })
  assert.deepEqual(s.placeOf(session('d', { cwd: '/Users/me/repo' }, ran)), { kind: 'folder', cwd: '/Users/me/repo' })
  // A project anchor wins even when the session already ran (resumed after a restart).
  assert.equal(s.placeOf(session('e', { cwd: '/lfs-root/projects/st/app' }, ran)).kind, 'project')
  assert.deepEqual(s.placeOf(session('unowned', { cwd: '/lfs-root/projects/other/repo' })), { kind: 'folder', cwd: '/lfs-root/projects/other/repo' })
  state.projects['st/new'] = { project: 'st/new', anchor: '/lfs-root/projects/st/new' }
  assert.equal(s.placeOf(session('new-anchor', { cwd: '/lfs-root/projects/st/new' })).kind, 'folder', 'new anchors are not ownership')
})

test('placeOf: bindings, subagents and forks follow what they come from', () => {
  const state = S.initialState()
  state.sessions.p = { sessionId: 'p', kind: 'project', project: 'st/app', anchor: '/lfs-root/projects/st/app', scratch: '/x', state: 'bound' }
  state.sessions.old = { sessionId: 'old', kind: 'project', project: 'dsh-scratch/scratch', anchor: '/lfs-root/scratch', scratch: '/y', state: 'bound' }
  const { s } = sessions({ state })
  assert.deepEqual(s.placeOf(session('p', { cwd: '/elsewhere' }, ran)), { kind: 'project', project: 'st/app', anchor: '/lfs-root/projects/st/app' })
  // A binding from the one-mode era (the Scratch project) still binds to it.
  assert.deepEqual(s.placeOf(session('old', { cwd: '/lfs-root/scratch' }, ran)), { kind: 'project', project: 'dsh-scratch/scratch', anchor: '/lfs-root/scratch' })
  assert.deepEqual(s.placeOf(session('sub', { cwd: '/lfs-root/projects/st/app', origin: 'subagent', parentSession: 'p' })),
    { kind: 'shared', parent: 'p', anchor: '/lfs-root/projects/st/app' })
  // A subagent of a folder session (no parent binding) is a folder session too.
  assert.deepEqual(s.placeOf(session('sub2', { cwd: '/Users/me/repo', origin: 'subagent', parentSession: 'nobody' }, ran)),
    { kind: 'folder', cwd: '/Users/me/repo' })
  // DSH's Fork of a session started in the default workspace copies turns but follows the parent.
  assert.deepEqual(s.placeOf(session('fork', { cwd: '/Users/me/Documents/default-workspace', parentSession: 'p' }, ran)),
    { kind: 'project', project: 'st/app', anchor: '/lfs-root/projects/st/app' })
})

test('choose: a session placed in a project binds there; cleared placement is a folder session', async () => {
  const lfs = { project: { get: async (key) => { if (key === 'gone/x') throw new Refusal('NoSuchProject'); return { key } } } }
  const { s, state } = sessions({ lfs, layerfsRoot: mkdtempSync(join(tmpdir(), 'dsh-root-')) })
  const root = s.ctx.layerfs.root
  const sess = session('n', { cwd: '/Users/me/repo' })
  await s.choose('n', 'st/app')
  assert.deepEqual(state().pending.n, { project: 'st/app' })
  // The same session in the same folder now works in the project — no workspace change.
  assert.deepEqual(s.placeOf(sess), { kind: 'project', project: 'st/app', anchor: `${root}/projects/st/app` })
  assert.deepEqual(s.placeOfId('n'), { project: 'st/app' })
  // An unknown project never reaches the pending record.
  await assert.rejects(s.choose('n', 'gone/x'), (e) => e.code === 'NoSuchProject')
  assert.equal(state().pending.n?.project, 'st/app')
  await s.choose('n', null)
  assert.equal(state().pending.n, undefined)
  assert.deepEqual(s.placeOf(sess), { kind: 'folder', cwd: '/Users/me/repo' })
  // A recorded binding outranks the pending choice.
  state().sessions.n = { sessionId: 'n', kind: 'project', project: 'st/other', anchor: `${root}/projects/st/other`, scratch: '/x', state: 'bound' }
  await assert.rejects(s.choose('n', 'st/app'), (e) => e.code === 'SessionStarted')
  assert.equal(s.placeOf(sess).project, 'st/other')
  rmSync(root, { recursive: true, force: true })
})

test('choose: a blank session takes the layerfs preset with its project and the default without it', async () => {
  const lfs = { project: { get: async (key) => ({ key }) } }
  const { s } = sessions({ lfs, layerfsRoot: mkdtempSync(join(tmpdir(), 'dsh-root-')) })
  const root = s.ctx.layerfs.root
  const blank = session('np', { cwd: root })
  s.ctx.sessions.get = () => blank
  const selected = []
  let preset = 'compiler'
  const presets = { defaultId: 'compiler', presetForSession: () => preset, select: async (_a, id) => { selected.push(id); preset = id; return id } }
  const agent = { id: 'np', session: blank }
  s.ctx.get = (name) => (name === 'agentPresets' ? presets : name === 'agents' ? { get: () => agent } : undefined)
  s.moveTo = async () => {}
  await s.choose('np', 'st/app')
  await s.choose('np', 'st/other')
  assert.deepEqual(selected, ['layerfs', 'layerfs'], 'the hero chip shows the layerfs preset as soon as a project is chosen')
  await s.choose('np', null)
  assert.deepEqual(selected, ['layerfs', 'layerfs', 'compiler'], 'leaving the project restores the default preset')
  // A preset the person picked themselves is left alone when the project is cleared.
  await s.choose('np', 'st/app')
  preset = 'ptc'
  await s.choose('np', null)
  assert.deepEqual(selected, ['layerfs', 'layerfs', 'compiler', 'layerfs'])
  rmSync(root, { recursive: true, force: true })
})

test('the layerfs preset is refused without a project: a blank folder session moves off it', async () => {
  const { s, state } = sessions()
  const selected = []
  let preset = 'layerfs'
  const presets = {
    defaultId: 'layerfs', presetForSession: () => preset,
    list: async () => [{ id: 'layerfs' }, { id: 'broken', broken: 'x' }, { id: 'standard' }],
    select: async (_a, id) => { selected.push(id); preset = id; return id },
  }
  s.ctx.get = (name) => (name === 'agentPresets' ? presets : undefined)
  const blank = { id: 'f', session: session('f', { cwd: '/Users/me/repo' }) }
  await s.bindAgent(blank, 'create')
  assert.deepEqual(selected, ['standard'], 'a layerfs default falls back to the first other working preset')
  preset = 'layerfs'
  presets.defaultId = 'compiler'
  await s.enforceFolderPreset(blank)
  assert.deepEqual(selected, ['standard', 'compiler'], 'a layerfs pick falls back to the default')
  // A project session keeps it, and so does a started folder session.
  preset = 'layerfs'
  state().pending.f = { project: 'st/app' }
  await s.enforceFolderPreset(blank)
  await s.enforceFolderPreset({ id: 'r', session: session('r', { cwd: '/Users/me/repo' }, ran) })
  assert.deepEqual(selected, ['standard', 'compiler'])
})

test('ensureBound lets a folder session run untouched (stock DSH)', async () => {
  const { s, state } = sessions()
  const agent = { id: 'leg', session: session('leg', { cwd: '/Users/me/repo' }, ran) }
  await s.ensureBound(agent)
  assert.equal(state().sessions.leg, undefined, 'a folder session records no binding')
})

test('a running Preset blocks project steps but never a normal folder session', async () => {
  const state = S.initialState()
  state.sessions.proj = { sessionId: 'proj', kind: 'project', project: 'st/p', sandbox: 'box', ws: 'box:main', scratch: '/scratch', anchor: '/anchor', state: 'bound' }
  const { s } = sessions({ state })
  s.ctx.layerfs.exclusive = () => 'A Preset build holds the LayerFS home'
  await s.ensureBound({ id: 'normal', session: session('normal', { cwd: '/Users/me/repo' }, ran) })
  await assert.rejects(s.ensureBound({ id: 'proj', session: session('proj', { cwd: '/scratch' }, ran) }), (e) => e.code === 'ConnectBusy')
})

test('a first step waits for an in-flight project choice and runs under the chosen owner', async () => {
  let release
  const gate = new Promise((resolve) => { release = resolve })
  const { s } = sessions({ lfs: { project: { get: async () => { await gate; return { kind: 'sqlite', machine: 'native' } } } }, layerfsRoot: mkdtempSync(join(tmpdir(), 'dsh-choose-race-')) })
  const blank = session('blank', { cwd: '/Users/me/repo' })
  s.ctx.sessions.get = () => blank
  const choosing = s.choose('blank', 'st/app')
  const order = []
  // The bind itself is stubbed: this test asserts only where the step is placed.
  s.bindAgent = async (agent) => { order.push(['bind', s.placeOf(agent.session).kind]) }
  s.validateExecutionIdentity = async () => {}
  const step = s.ensureBound({ id: 'blank', session: blank }).then(() => order.push(['step']))
  await new Promise((resolve) => setTimeout(resolve, 20))
  assert.deepEqual(order, [], 'the step waits while the owner is being chosen')
  release()
  await choosing
  await step
  assert.deepEqual(order, [['bind', 'project'], ['step']], 'the first step binds the chosen project')
})

test('choose refuses changing a normal session owner once its first turn started', async () => {
  const { s, state } = sessions()
  s.ctx.sessions.get = () => session('started', { cwd: '/selected' }, ran)
  await assert.rejects(s.choose('started', 'st/app'), (e) => e.code === 'SessionStarted')
  assert.equal(state().pending.started, undefined)
})

test('durable project ownership survives missing global binding state', () => {
  const { s } = sessions()
  s.ctx.get = (name) => name === 'sessionPersistence' ? { ownerOf: () => ({ project: 'st/app' }) } : undefined
  assert.equal(s.placeOf(session('recovered', { cwd: '/old-local-scratch' }, ran)).project, 'st/app')
  assert.deepEqual(s.placeOfId('recovered'), { project: 'st/app' })
})

test('recovery reuses the persisted project execution context instead of starting from main', async () => {
  const scratch = mkdtempSync(join(root, '.dev-session-recovery-'))
  try {
    const checkpoints = [{ kind: 'turn', turn: 1, commit: 'saved', branch: 'existing-internal', changed: 1 }]
    const { s } = sessions({ layerfsRoot: scratch, lfs: {
      project: { get: async () => ({ store: 'st', engine: 'sqlite', machine: 'native', state: 'ready' }) },
      branch: { fork: async () => { throw new Error('recovery must not fork from main') } },
      // 2.0 `sandbox get --detail`: Workspaces `{ workspace, branch, path }`
      sandbox: { get: async (name) => ({ sandbox: name, state: 'ready', workspaces: [{ workspace: `${name}:main`, branch: 'existing-internal', path: scratch }] }) },
    } })
    const persistence = {
      ownerOf: () => ({ project: 'st/app' }),
      readSidecar: async () => ({ project: 'st/app', codeBranch: 'existing-internal', checkpoints }),
      migrateProject: async () => {}, updateSidecar: async () => {}, recordInvocation: async () => {},
    }
    s.ctx.get = (name) => name === 'sessionPersistence' ? persistence : undefined
    s.ctx.workingDirectory = { get: () => scratch }
    const agent = { id: 'recovered', session: session('recovered', { cwd: scratch }, [...ran, { type: 'turn/end', data: { turn: 1 } }]) }
    await s.ensureBound(agent)
    assert.equal(s.binding('recovered').branch, 'existing-internal')
    assert.deepEqual(s.checkpoints('recovered'), checkpoints)
    assert.equal(s.binding('recovered').scratch, scratch)
  } finally { rmSync(scratch, { recursive: true, force: true }) }
})

test('closed started session cannot change owner without loading its agent', async () => {
  const { s } = sessions()
  s.ctx.get = (name) => name === 'sessionPersistence' ? {
    ownerOf: () => ({ project: 'st/app' }),
    local: { stat: async () => ({}), open: async () => ({ read: async () => ({ events: ran }), close: async () => {} }) },
  } : undefined
  await assert.rejects(s.choose('closed', 'st/another'), (e) => e.code === 'SessionStarted')
})

test('selectScratch changes the selected local folder without changing ownership', async () => {
  const scratch = mkdtempSync(join(root, '.dev-scratch-selection-'))
  try {
    const state = S.initialState()
    state.pending.a = { project: 'st/app' }
    const { s } = sessions({ state })
    const changes = []
    const agent = { id: 'a', session: { ...session('a', { cwd: '/original' }), append: (type, data) => changes.push({ type, data }) } }
    s.ctx.get = (name) => name === 'agents' ? { get: () => agent } : undefined
    s.ctx.workingDirectory = { get: () => '/original' }
    await s.selectScratch('a', scratch)
    assert.deepEqual(changes, [{ type: 'working-directory/change', data: { cwd: scratch } }])
    assert.equal(s.placeOfId('a').project, 'st/app')
    assert.equal(state.pending.a.project, 'st/app')
    await assert.rejects(s.selectScratch('a', join(scratch, 'missing')), (e) => e.code === 'ScratchUnavailable')
  } finally { rmSync(scratch, { recursive: true, force: true }) }
})

test('forget preserves a selected scratch folder even under the project anchor', async () => {
  const root = mkdtempSync(join(tmpdir(), 'dsh-root-'))
  try {
    const state = S.initialState()
    const scratch = join(root, 'projects', 'st', 'app', 'user-folder')
    mkdirSync(scratch, { recursive: true })
    writeFileSync(join(scratch, 'keep.txt'), 'user data')
    state.sessions.a = { sessionId: 'a', kind: 'project', project: 'st/app', anchor: join(root, 'projects', 'st', 'app'), scratch, state: 'closed' }
    const { s } = sessions({ state, layerfsRoot: root })
    await s.forget('a')
    assert.ok(existsSync(join(scratch, 'keep.txt')))
  } finally { rmSync(root, { recursive: true, force: true }) }
})

test('project detach removes its normal workspace membership without changing the folder', async () => {
  const { s } = sessions()
  const ids = ['project', 'normal']
  const workspace = { sessionIds: ids, detachSession: async (id) => { ids.splice(ids.indexOf(id), 1) } }
  s.ctx.workspaceRegistry = { list: () => [workspace] }
  await s.detach('project')
  await s.detach('project')
  assert.deepEqual(ids, ['normal'])
})

test('close refuses to discard unsaved files after an uncertain checkpoint', async () => {
  const state = S.initialState()
  state.sessions.a = { sessionId: 'a', kind: 'project', project: 'st/app', state: 'bound', ws: 'sb:main', sandbox: 'sb', branch: 'internal' }
  let closed = false
  const { s } = sessions({ state, lfs: {
    workspace: { status: async () => ({ changes: [{ path: '/a', change: 'modified' }], overlay: { entries: 1 } }), commit: async () => { throw new Refusal('Unknown', 'uncertain save') } },
    sandbox: { remove: async () => { closed = true } },
  } })
  await assert.rejects(s.close('a'), (e) => e.code === 'Unknown')
  assert.equal(closed, false)
  assert.equal(s.binding('a').state, 'bound')
})

test('ensureBound binds a remote project: Sandbox on its machine, working directory the scratch folder', async () => {
  const calls = []
  const lfs = {
    project: { get: async () => ({ store: 'st', engine: 'sqlite', machine: 'lima1', state: 'ready' }) },
    branch: { fork: async (project, branch, from) => { calls.push(`fork ${project} ${branch} ${from}`); return { commit: 'f'.repeat(66) } } },
    sandbox: {
      get: async () => { throw new Refusal('NotFound') },
      // 2.0 `sandbox create <project> <sandbox> [--machine M]`
      create: async (project, name, options) => { calls.push(`sandbox ${project} ${name} ${options.machine ?? '(its Store\'s machine)'}`); return { ok: true } },
    },
    workspace: {
      mount: async (sandbox, branch, options) => { calls.push(`mount ${sandbox} ${branch} ${options.name}`); return { path: '/mnt/inside-the-guest', workspace: `${sandbox}:${options.name}`, commit: 'base-1' } },
    },
  }
  const { s, state } = sessions({ lfs, layerfsRoot: mkdtempSync(join(tmpdir(), 'dsh-root-')) })
  const selected = []
  s.ctx.get = (name) => (name === 'agentPresets' ? { select: async (_a, id) => { selected.push(id); return id } } : undefined)
  s.ctx.workingDirectory = { get: () => s.ctx.layerfs.root, set: async () => { throw new Error('selected scratch must stay unchanged') } }
  const anchor = join(s.ctx.layerfs.root, 'projects', 'st', 'app')
  state().pending.n1 = { project: 'st/app' }
  const appended = []
  const agent = { id: 'n1', session: { ...session('n1', { cwd: anchor }), seq: 0, append: (type, data) => appended.push([type, data]) } }
  await s.ensureBound(agent)
  const b = state().sessions.n1
  assert.equal(b.state, 'bound')
  assert.equal(b.machine, 'lima1')
  assert.equal(b.mount, undefined, 'a remote Workspace mounts inside its Sandbox, not on this host')
  assert.equal(b.ws, 'dsh-n1:main', 'the Workspace handle stays usable for exec/status/commit')
  assert.equal(b.scratch, s.ctx.layerfs.root)
  assert.equal(b.scratchManaged, false)
  assert.deepEqual(calls, ['fork st/app dsh.n1 main', 'sandbox st/app dsh-n1 lima1', 'mount dsh-n1 dsh.n1 main'], 'the machine is named whenever it is not native')
  assert.equal(appended.find(([type]) => type === 'working-directory/change'), undefined)
  assert.deepEqual(selected, ['layerfs'], 'the session is recomposed under the layerfs preset')
  rmSync(s.ctx.layerfs.root, { recursive: true, force: true })
})

test('ensureBound runs a PostgreSQL project where the session picked, else its last machine, else one its database was reached from, else native; never a broken or stopped one (spec v23)', async () => {
  const bind = async (machines, { picked, last, reached = [] } = {}) => {
    const calls = []
    const lfs = {
      // a PostgreSQL Store is on no machine: project.get names none
      project: { get: async () => ({ store: 'st', engine: 'postgresql', state: 'ready' }) },
      // `machine list --detail` (2.0): rows keyed `machine`, `supports` a list
      machine: { list: async () => ({ items: machines.map(({ name, ...rest }) => ({ machine: name, ...rest })) }) },
      store: { get: async () => ({ engine: 'postgresql', connections: [{ machine: null, reached: null }, ...reached.map((m) => ({ machine: m, reached: 'ok' }))] }) },
      branch: { fork: async () => ({ commit: 'f'.repeat(66) }) },
      sandbox: {
        get: async () => { throw new Refusal('NotFound') },
        list: async () => ({ items: last ? [{ sandbox: 'old', machine: last, created_at: '2026-11-19T00:00:00Z' }] : [] }),
        create: async (project, name, options) => { calls.push(`sandbox ${project} ${name} ${options.machine ?? 'native'}`); return { ok: true } },
      },
      workspace: { mount: async (sandbox, branch, options) => ({ path: '/mnt/inside', workspace: `${sandbox}:${options.name}`, commit: 'base-1' }) },
    }
    const { s, state } = sessions({ lfs, layerfsRoot: mkdtempSync(join(tmpdir(), 'dsh-root-')) })
    try {
      s.ctx.get = (name) => (name === 'agentPresets' ? { select: async (_a, id) => id } : undefined)
      s.ctx.workingDirectory = { get: () => s.ctx.layerfs.root, set: async () => {} }
      state().pending.n1 = { project: 'st/app', ...(picked ? { machine: picked } : {}) }
      await s.ensureBound({ id: 'n1', session: { ...session('n1', { cwd: join(s.ctx.layerfs.root, 'projects', 'st', 'app') }), seq: 0, append: () => {} } })
      assert.deepEqual(calls, [`sandbox st/app dsh-n1 ${state().sessions.n1.machine}`])
      // nothing Store-wide was written: no store-machines.json beside the plugin's state
      assert.equal(existsSync(join(s.ctx.layerfs.root, 'store-machines.json')), false)
      return state().sessions.n1.machine
    } finally { rmSync(s.ctx.layerfs.root, { recursive: true, force: true }) }
  }
  const m = (name, more = {}) => ({ name, supports: ['sqlite', 'postgresql_sandbox'], ...more })
  const fleet = [m('native'), m('docker'), m('vm1'), m('vm2', { state: 'stopped' }), m('vm3', { state: 'broken' }), m('old', { supports: ['sqlite'] })]
  assert.equal(await bind(fleet, { picked: 'vm1' }), 'vm1', 'the session\'s own pick')
  assert.equal(await bind(fleet, { last: 'vm1', reached: ['docker'] }), 'vm1', 'the project\'s last machine')
  assert.equal(await bind(fleet, { last: 'vm2', reached: ['vm3', 'docker'] }), 'docker', 'a stopped last machine and a broken reached one are skipped')
  assert.equal(await bind(fleet, { reached: ['old'] }), 'native', 'a machine that runs no PostgreSQL Sandbox is not the default')
  assert.equal(await bind(fleet, {}), 'native', 'nothing known: native')
})

test('ensureBound keeps native project local cwd as selected scratch, separate from the mount', async () => {
  const calls = []
  const lfs = {
    project: { get: async () => ({ store: 'st', engine: 'sqlite', machine: 'native', state: 'ready' }) },
    branch: { fork: async () => { calls.push('fork'); return { commit: 'f'.repeat(66) } } },
    sandbox: {
      get: async () => { throw new Refusal('NotFound') },
      create: async (_p, _n, options) => { calls.push(`sandbox ${options.machine ?? 'native'}`); return { ok: true } },
    },
    workspace: { mount: async (sandbox, _b, options) => { calls.push(`mount ${sandbox}`); return { path: `${s_root}/host-mount`, workspace: `${sandbox}:${options.name}` } } },
  }
  const { s, state } = sessions({ lfs, layerfsRoot: mkdtempSync(join(tmpdir(), 'dsh-root-')) })
  const s_root = s.ctx.layerfs.root
  s.ctx.workingDirectory = { get: () => s_root, set: async () => {} }
  const anchor = join(s_root, 'projects', 'st', 'app')
  state().pending.n2 = { project: 'st/app' }
  const appended = []
  const agent = { id: 'n2', session: { ...session('n2', { cwd: anchor }), seq: 0, append: (type, data) => appended.push([type, data]) } }
  await s.ensureBound(agent)
  const b = state().sessions.n2
  assert.equal(b.state, 'bound')
  assert.equal(b.mount, `${s_root}/host-mount`)
  assert.deepEqual(calls, ['fork', 'sandbox native', 'mount dsh-n2'])
  assert.equal(b.scratch, s_root)
  assert.equal(b.scratchManaged, false)
  assert.equal(appended.find(([type]) => type === 'working-directory/change'), undefined, 'selected local folder stays the working directory')
  rmSync(s_root, { recursive: true, force: true })
})

test('ensureScratchProject makes the Store and project once, tolerating Exists', async () => {
  const calls = []
  let made = false
  const lfs = {
    project: {
      get: async (key) => { calls.push(`get ${key}`); if (!made) throw new Refusal('NotFound'); return { project: key } },
      init: async (key, folder) => { calls.push(`init ${key}`); assert.ok(folder.includes('scratch-')); made = true; return {} },
    },
    store: { create: async (name) => { calls.push(`create ${name}`); throw new Refusal('Exists') } },
  }
  const { s } = sessions({ lfs, layerfsRoot: mkdtempSync(join(tmpdir(), 'dsh-root-')) })
  await Promise.all([s.ensureScratchProject(), s.ensureScratchProject()])
  await s.ensureScratchProject()
  assert.deepEqual(calls, ['get dsh-scratch/scratch', 'create dsh-scratch', 'init dsh-scratch/scratch'])
  rmSync(s.ctx.layerfs.root, { recursive: true, force: true })
})

test('ensureScratchProject retries after a failure', async () => {
  let attempts = 0
  const lfs = {
    project: { get: async () => { attempts++; if (attempts === 1) throw new Refusal('Unknown'); return {} } },
    store: { create: async () => {} },
  }
  const { s } = sessions({ lfs })
  await assert.rejects(s.ensureScratchProject(), (e) => e.code === 'Unknown')
  await s.ensureScratchProject()
  assert.equal(attempts, 2)
})

test('session persistence row: a local miss is imported from LayerFS, then answered', async () => {
  const { SessionPersistenceNotFoundError } = await import('@deepseek-ai/dsh-session-persistence')
  const stored = new Set(['local-1'])
  const local = {
    stat: async (id) => (stored.has(id) ? { header: { id } } : undefined),
    open: async (id) => { if (!stored.has(id)) throw new SessionPersistenceNotFoundError(id); return { id } },
  }
  const imports = []
  const store = {
    locate: async (id) => (id.startsWith('remote') ? 'st/app' : undefined),
    import: async (project, id) => { imports.push(`${project} ${id}`); stored.add(id); return { imported: true } },
  }
  // The row's service without cordis: its methods over `local` and the sessions area's store.
  const w = Object.create(S.persistence.LayerfsSessionPersistence.prototype)
  Object.defineProperty(w, 'local', { value: local })
  Object.defineProperty(w, 'ctx', { value: { get: (name) => (name === 'layerfsSessions' ? { store } : undefined), logger: () => ({ warn() {} }) } })
  assert.deepEqual(await w.stat('local-1'), { header: { id: 'local-1' } })
  assert.deepEqual(await w.open('remote-2', 'read'), { id: 'remote-2' })
  assert.deepEqual(await w.stat('remote-3'), { header: { id: 'remote-3' } })
  assert.equal(await w.stat('nowhere'), undefined)
  await assert.rejects(w.open('nowhere', 'read'), SessionPersistenceNotFoundError)
  assert.deepEqual(imports, ['st/app remote-2', 'st/app remote-3'])
})

test('session persistence row: before the sessions area is up, every call is the stock one', async () => {
  const local = { stat: async () => undefined, open: async () => { throw new Error('stock') } }
  const w = Object.create(S.persistence.LayerfsSessionPersistence.prototype)
  Object.defineProperty(w, 'local', { value: local })
  Object.defineProperty(w, 'ctx', { value: { get: () => undefined, logger: () => ({ warn() {} }) } })
  assert.equal(await w.stat('x'), undefined)
  await assert.rejects(w.open('x', 'read'), /stock/)
})

test('force-stopped machines invalidate only their mounts while preserving session history and ownership', () => {
  const state = S.initialState()
  state.sessions.parent = { sessionId: 'parent', kind: 'project', project: 'st/p', machine: 'vm', sandbox: 'box', ws: 'box:main', mount: '/mounted', scratch: '/scratch', anchor: '/anchor', state: 'bound' }
  state.sessions.child = { sessionId: 'child', kind: 'shared', project: 'st/p', parent: 'parent', ws: 'box:main', mount: '/mounted', scratch: '/scratch', anchor: '/anchor', state: 'bound' }
  state.sessions.other = { sessionId: 'other', kind: 'project', project: 'st/q', machine: 'native', sandbox: 'other-box', ws: 'other-box:main', scratch: '/local', anchor: '/anchor', state: 'bound' }
  state.pending.draft = { project: 'st/p' }
  state.checkpoints.parent = [{ commit: 'saved' }]
  state.mirror.parent = { branch: 'archive', count: 10 }
  const f = sessions({ state })
  f.s.machinesStopped(['vm'])
  const after = f.state()
  for (const id of ['parent', 'child']) {
    assert.equal(after.sessions[id].state, 'closed')
    assert.equal(after.sessions[id].ws, undefined)
    assert.equal(after.sessions[id].mount, undefined)
    assert.equal(after.sessions[id].project, 'st/p')
    assert.equal(after.sessions[id].scratch, '/scratch')
  }
  assert.equal(after.sessions.other.state, 'bound')
  assert.deepEqual(after.checkpoints, state.checkpoints)
  assert.deepEqual(after.mirror, state.mirror)
  assert.deepEqual(after.pending, state.pending)
  f.s.sandboxClosed('other-box')
  assert.equal(f.state().sessions.other.state, 'closed')
  assert.equal(f.state().sessions.other.ws, undefined)
  assert.deepEqual(f.state().checkpoints, state.checkpoints)
})

test('a running preset stop blocks new steps even when a session still has cached bound handles', async () => {
  const state = S.initialState()
  state.sessions.live = { sessionId: 'live', kind: 'project', project: 'st/p', machine: 'vm', sandbox: 'box', ws: 'box:main', scratch: '/scratch', anchor: '/anchor', state: 'bound' }
  const f = sessions({ state })
  f.s.ctx.layerfs.exclusive = () => 'Machines are stopping'
  await assert.rejects(f.s.ensureBound({ id: 'live' }), (error) => error.code === 'ConnectBusy')
})

test('a session recreates its force-stopped gone Sandbox on the retained branch', async () => {
  const root = mkdtempSync(join(tmpdir(), 'dsh-force-rebind-'))
  const state = S.initialState(), calls = []
  state.sessions.live = { sessionId: 'live', kind: 'project', project: 'st/p', machine: 'vm', sandbox: 'box', branch: 'saved-branch', scratch: root, anchor: root, state: 'closed' }
  const f = sessions({ state, layerfsRoot: root, lfs: {
    sandbox: {
      // 2.0: a Sandbox whose daemon is gone reads broken (lost)
      get: async (name) => ({ sandbox: name, state: 'broken', reason: 'lost', workspaces: [] }),
      remove: async (name) => { calls.push(['remove', name]); return { ok: true } },
      create: async (...args) => { calls.push(['create', ...args]); return { ok: true } },
    },
    workspace: { mount: async (name, branch) => { calls.push(['mount', name, branch]); return { workspace: 'box:main', commit: 'saved' } } },
  } })
  f.s.ctx.get = () => undefined
  f.s.ctx.workingDirectory = { get: () => root }
  try {
    await f.s.ensureBound({ id: 'live', session: session('live', { cwd: root }) })
    assert.deepEqual(calls, [['remove', 'box'], ['create', 'st/p', 'box', { machine: 'vm' }], ['mount', 'box', 'saved-branch']])
    assert.equal(f.state().sessions.live.state, 'bound')
    assert.equal(f.state().sessions.live.branch, 'saved-branch')
  } finally { rmSync(root, { recursive: true, force: true }) }
})

test('opening project history hydrates local context without activating execution', async () => {
  const dir = mkdtempSync(join(root, '.dev-lazy-open-'))
  try {
    const { s, state } = sessions({ layerfsRoot: dir })
    const checkpoints = [{ kind: 'turn', turn: 1, commit: 'saved', branch: 'retained', changed: 1 }]
    s.ctx.get = (name) => name === 'sessionPersistence' ? {
      ownerOf: () => ({ project: 'store/app' }), projectIdOf: () => 'permanent-id',
      readSidecar: async () => ({ codeBranch: 'retained', checkpoints }),
    } : undefined
    s.ctx.workingDirectory = { get: () => dir }
    const agent = { id: 'history', session: session('history', { cwd: dir }, ran) }
    await s.bindAgent(agent, 'resume')
    assert.equal(state().sessions.history.executionState, 'inactive')
    assert.equal(state().sessions.history.state, 'closed')
    assert.equal(state().sessions.history.projectId, 'permanent-id')
    assert.deepEqual(state().checkpoints.history, checkpoints)
    assert.equal(state().sessions.history.mount, undefined)
    assert.equal(state().sessions.history.ws, undefined)
  } finally { rmSync(dir, { recursive: true, force: true }) }
})

test('a cached active workspace cannot bypass permanent identity validation', async () => {
  const state = S.initialState()
  state.sessions.active = { sessionId: 'active', kind: 'project', project: 'store/app', anchor: '/anchor', scratch: '/scratch', state: 'bound', ws: 'box:main', sandbox: 'box', branch: 'retained' }
  state.checkpoints.active = [{ commit: 'saved' }]
  const f = sessions({ state })
  f.s.ctx.get = (name) => name === 'sessionPersistence' ? { assertSessionIdentity: async () => { throw new Refusal('ProjectIdentityChanged') } } : undefined
  await assert.rejects(f.s.ensureBound({ id: 'active', session: session('active', { cwd: '/scratch' }, ran) }), { code: 'ProjectIdentityChanged' })
  assert.equal(f.state().sessions.active.executionState, 'interrupted')
  assert.equal(f.state().sessions.active.project, 'store/app')
  assert.equal(f.state().checkpoints.active[0].commit, 'saved')
})

test('project removal retains bindings, pending choices, archive progress and checkpoint references', async () => {
  const state = S.initialState()
  state.sessions.history = { sessionId: 'history', kind: 'project', project: 'store/app', anchor: '/anchor', scratch: '/scratch', state: 'closed' }
  state.pending.draft = { project: 'store/app' }
  state.checkpoints.history = [{ commit: 'saved' }]
  state.mirror.history = { count: 5, project: 'store/app', branch: 'archive' }
  state.projects['store/app'] = { project: 'store/app', anchor: '/anchor', createdAt: 1 }
  const f = sessions({ state })
  await f.s.forgetProject('store/app')
  assert.equal(f.state().sessions.history.executionState, 'interrupted')
  assert.equal(f.state().sessions.history.project, 'store/app')
  assert.deepEqual(f.state().pending, state.pending)
  assert.deepEqual(f.state().checkpoints, state.checkpoints)
  assert.deepEqual(f.state().mirror, state.mirror)
  assert.deepEqual(f.state().projects, state.projects)
})

test('polling detects lost access and restored availability without replaying execution', async () => {
  let online = false, mounts = 0
  const state = S.initialState()
  state.sessions.history = { sessionId: 'history', kind: 'project', project: 'store/app', machine: 'vm', anchor: '/anchor', scratch: '/scratch', state: 'bound', ws: 'box:main', branch: 'retained' }
  state.checkpoints.history = [{ commit: 'saved' }]
  const f = sessions({ state, lfs: {
    machine: { get: async () => ({ machine: 'vm', state: online ? 'ready' : 'stopped' }) },
    // 2.0 `store get --detail`: a Store on a stopped machine reads blocked
    store: { get: async () => online ? { store: 'store', engine: 'sqlite', state: 'ready' } : { store: 'store', engine: 'sqlite', state: 'blocked', blocked_by: { machine: 'vm', reason: 'stopped' } } },
    workspace: { status: async () => ({}), mount: async () => { mounts++; return {} } },
  } })
  f.s.ctx.get = (name) => name === 'sessionPersistence' ? {
    assertSessionIdentity: async () => ({ key: 'store/app', store: 'store', machine: 'vm' }),
    projectIdOf: () => 'permanent-id', local: { stat: async () => ({ eventCount: 5 }) },
  } : undefined
  const unavailable = await f.s.view('history')
  assert.equal(unavailable.execution.available, false)
  assert.equal(unavailable.execution.state, 'interrupted')
  online = true
  const available = await f.s.view('history')
  assert.equal(available.execution.available, true)
  assert.equal(available.execution.state, 'interrupted')
  assert.equal(mounts, 0, 'status refresh never mounts a workspace')
  assert.equal(available.checkpoints[0].commit, 'saved')
})

test('an observed session.state (the views\' poll) changes nothing: a refused machine, Store or Workspace check leaves the session bound (RV4 FAIL 1)', async () => {
  for (const refused of ['machine-op', 'machine', 'store', 'workspace']) {
    const state = S.initialState()
    state.sessions.live = { sessionId: 'live', kind: 'project', project: 'store/app', machine: 'vm', anchor: '/anchor', scratch: '/scratch', state: 'bound', ws: 'box:main', sandbox: 'box', mount: '/mnt/box', branch: 'retained', executionState: 'active' }
    state.checkpoints.live = [{ commit: 'saved' }]
    const before = structuredClone(state)
    const f = sessions({ state, lfs: {
      // an image load on the machine (an `op`), a stopped machine, a blocked Store, a timed-out status
      machine: { get: async () => refused === 'machine-op' ? { machine: 'vm', state: 'ready', op: 'image load' } : { machine: 'vm', state: refused === 'machine' ? 'stopped' : 'ready' } },
      store: { get: async () => refused === 'store' ? { store: 'store', engine: 'sqlite', state: 'blocked', blocked_by: { machine: 'vm', reason: 'stopped' } } : { store: 'store', engine: 'sqlite', state: 'ready' } },
      workspace: { status: async () => { if (refused === 'workspace') throw new Refusal('Timeout', 'workspace status timed out'); return { overlay: { inode_count: 1 } } } },
    } })
    f.s.ctx.get = (name) => name === 'sessionPersistence' ? {
      assertSessionIdentity: async () => ({ key: 'store/app', store: 'store', machine: 'vm' }),
      projectIdOf: () => 'permanent-id', local: { stat: async () => ({ eventCount: 5 }) },
    } : undefined
    const view = await f.s.view('live', { observe: true })
    assert.equal(view.execution.available, false, refused)
    assert.ok(view.execution.reason, refused)
    assert.deepEqual(f.state().sessions, before.sessions, `${refused}: the binding is unchanged (still bound, mount and Workspace kept)`)
    assert.equal(f.state().sessions.live.state, 'bound')
    assert.notEqual(f.state().sessions.live.executionState, 'interrupted')
    assert.deepEqual(f.state().checkpoints, before.checkpoints)
    assert.deepEqual(f.events.filter((e) => e.type === 'layerfs-session'), [], `${refused}: no session event`)
    // without `observe` (an explicit read) the same refusal still interrupts, as before
    await f.s.view('live')
    assert.equal(f.state().sessions.live.executionState, 'interrupted', refused)
  }
})

test('an observed read never clears an availability error either; the explicit read does', async () => {
  const state = S.initialState()
  state.sessions.held = { sessionId: 'held', kind: 'project', project: 'store/app', machine: 'vm', anchor: '/anchor', scratch: '/scratch', state: 'closed', executionState: 'interrupted', error: 'Store down', interruption: { kind: 'availability', at: 1 }, branch: 'retained' }
  const f = sessions({ state, lfs: {
    machine: { get: async () => ({ machine: 'vm', state: 'ready' }) },
    store: { get: async () => ({ store: 'store', engine: 'sqlite', state: 'ready' }) },
    workspace: { status: async () => ({}) },
  } })
  f.s.ctx.get = (name) => name === 'sessionPersistence' ? {
    assertSessionIdentity: async () => ({ key: 'store/app', store: 'store', machine: 'vm' }),
    projectIdOf: () => 'permanent-id', local: { stat: async () => ({ eventCount: 5 }) },
  } : undefined
  await f.s.view('held', { observe: true })
  assert.equal(f.state().sessions.held.error, 'Store down')
  await f.s.view('held')
  assert.equal(f.state().sessions.held.error, undefined)
})

test('the session view reads workspace status --detail: its overlay counts are the line\'s unsaved files (review UR bug 2)', async () => {
  const state = S.initialState()
  state.sessions.live = { sessionId: 'live', kind: 'project', project: 'store/app', machine: 'vm', anchor: '/anchor', scratch: '/scratch', state: 'bound', ws: 'box:main', branch: 'retained' }
  const f = sessions({ state, lfs: {
    machine: { get: async () => ({ machine: 'vm', state: 'ready' }) },
    store: { get: async () => ({ store: 'store', engine: 'sqlite', state: 'ready' }) },
    // the long form carries `overlay`; the short form has none
    workspace: { status: async (_ws, o) => (o?.detail ? { workspace: 'box:main', overlay: { inode_count: 2, entry_count: 3 } } : { workspace: 'box:main' }) },
  } })
  f.s.ctx.get = (name) => name === 'sessionPersistence' ? {
    assertSessionIdentity: async () => ({ key: 'store/app', store: 'store', machine: 'vm' }),
    projectIdOf: () => 'permanent-id', local: { stat: async () => ({ eventCount: 5 }) },
  } : undefined
  const view = await f.s.view('live')
  assert.equal(view.execution.available, true)
  assert.deepEqual(view.status?.overlay, { inode_count: 2, entry_count: 3 })
})

test('a missing saved checkpoint blocks activation without falling back to project main', async () => {
  const state = S.initialState()
  state.sessions.history = { sessionId: 'history', kind: 'project', project: 'store/app', machine: 'vm', anchor: '/anchor', scratch: '/scratch', state: 'closed', sandbox: 'box', branch: 'retained' }
  state.checkpoints.history = [{ commit: 'missing-checkpoint', branch: 'retained' }]
  const calls = []
  const f = sessions({ state, lfs: {
    machine: { get: async () => ({ state: 'ready' }) }, store: { get: async () => ({ store: 'store', engine: 'sqlite', state: 'ready' }) },
    file: { list: async () => { throw new Refusal('NotFound') } },
    branch: { fork: async () => { calls.push('fork') } }, sandbox: { create: async () => { calls.push('sandbox') } },
  } })
  f.s.ctx.get = (name) => name === 'sessionPersistence' ? { assertSessionIdentity: async () => ({ key: 'store/app', store: 'store', machine: 'vm' }) } : undefined
  await assert.rejects(f.s.ensureBound({ id: 'history', session: session('history', { cwd: '/scratch' }, ran) }), { code: 'CheckpointUnavailable' })
  assert.deepEqual(calls, [])
  assert.equal(f.state().checkpoints.history[0].commit, 'missing-checkpoint')
})

test('classifyStore: the 2.0 six states: blocked and broken are unavailable, busy waits, unknown never deactivates', () => {
  assert.equal(S.classifyStore({ store: 's', state: 'ready' }).state, 'available')
  assert.equal(S.classifyStore({ store: 's' }).state, 'available', 'a ready row may leave state out')
  assert.equal(S.classifyStore({ store: 's', state: 'busy', op: { name: 'move' } }).state, 'transient')
  assert.equal(S.classifyStore({ store: 's', state: 'ready', op: { name: 'compact' } }).state, 'transient')
  const blocked = S.classifyStore({ store: 's', state: 'blocked', blocked_by: { machine: 'vm', reason: 'stopped' } })
  assert.deepEqual(blocked, { state: 'unavailable', reason: 'The machine of Store s (vm) is stopped.' })
  assert.equal(S.classifyStore({ store: 's', state: 'broken', reason: 'unresolved', fix: 'layerfs store move s vm' }).state, 'unavailable')
  assert.match(S.classifyStore({ store: 's', state: 'broken', reason: 'unresolved', fix: 'layerfs store move s vm' }).reason, /Fix: layerfs store move s vm/)
  assert.equal(S.classifyStore(undefined).state, 'removed')
  // a PostgreSQL Store: the live read (`store get --with live`) decides
  assert.equal(S.classifyStore({ store: 'pg', engine: 'postgresql', state: 'ready' }, { store: 'pg', state: 'broken', reason: 'unreachable' }).state, 'unavailable')
  assert.equal(S.classifyStore({ store: 'pg', engine: 'postgresql', state: 'ready' }, { store: 'pg', state: 'ready' }).state, 'available')
  assert.equal(S.classifyStore({ store: 's', state: 'unknown' }).state, 'unknown')
  assert.equal(S.classifyStore({ store: 's', state: 'mystery' }).state, 'unknown')
  // CUT: a PostgreSQL Store naming no saved login is unavailable, and says what fixes it
  const noLogin = S.classifyStore({ store: 'old', engine: 'postgresql', auth: null, state: 'broken', reason: 'outdated', fix: 'layerfs store save-auth <auth> --user <user> --password-stdin && layerfs store update old --auth <auth>' })
  assert.equal(noLogin.state, 'unavailable')
  assert.match(noLogin.reason, /PostgreSQL Store old names no saved login; save one with Save a login/)
})

/** Store health poll fixture: an active owner + its subagent on `st`, another active session on `ok`. */
function polled(stores, statuses = {}) {
  const state = S.initialState()
  state.sessions.owner = { sessionId: 'owner', kind: 'project', project: 'st/app', machine: 'vm', sandbox: 'box', branch: 'dsh.owner', ws: 'box:main', mount: '/m', scratch: '/scratch', anchor: '/anchor', state: 'bound', executionState: 'active' }
  state.sessions.child = { sessionId: 'child', kind: 'shared', project: 'st/app', parent: 'owner', mount: '/m', scratch: '/scratch', anchor: '/anchor', state: 'bound', executionState: 'active' }
  state.sessions.calm = { sessionId: 'calm', kind: 'project', project: 'ok/app', machine: 'native', sandbox: 'calm-box', ws: 'calm-box:main', scratch: '/s2', anchor: '/a2', state: 'bound', executionState: 'active' }
  state.checkpoints.owner = [{ commit: 'saved', branch: 'dsh.owner' }]
  const calls = []
  const f = sessions({ state, lfs: {
    // 2.0: `store list --detail` rows, and a PostgreSQL Store's `store get --with live`
    store: {
      list: async () => { calls.push('list'); if (stores instanceof Error) throw stores; return { items: stores() } },
      get: async (name) => { calls.push(`get:${name}`); return statuses[name]?.() ?? { store: name, state: 'ready' } },
    },
    workspace: { mount: async () => { calls.push('mount') } }, sandbox: { create: async () => { calls.push('create') }, remove: async () => { calls.push('remove') } },
  } })
  return { ...f, calls }
}

test('the availability poll deactivates active conversations whose Store went down, keeping history and the retained branch', async () => {
  let down = true
  const f = polled(() => [down ? { store: 'st', engine: 'sqlite', state: 'blocked', blocked_by: { machine: 'vm', reason: 'stopped' } } : { store: 'st', engine: 'sqlite', state: 'ready' }, { store: 'ok', engine: 'sqlite', state: 'ready' }])
  assert.equal(f.s.watchesAvailability(), true)
  await f.s.refreshAvailability()
  await f.s.settled('owner')
  for (const id of ['owner', 'child']) {
    const b = f.state().sessions[id]
    assert.equal(b.state, 'closed')
    assert.equal(b.executionState, 'interrupted')
    assert.equal(b.mount, undefined)
    assert.equal(b.ws, undefined)
    assert.equal(b.interruption.kind, 'availability')
    assert.match(b.error, /machine of Store st \(vm\) is stopped/)
  }
  assert.equal(f.state().sessions.owner.sandbox, 'box', 'the Sandbox name is kept so the next message re-mounts it')
  assert.equal(f.state().sessions.owner.branch, 'dsh.owner')
  assert.deepEqual(f.state().checkpoints.owner, [{ commit: 'saved', branch: 'dsh.owner' }])
  assert.equal(f.state().sessions.calm.state, 'bound', 'other Stores are untouched')
  assert.ok(f.events.some((e) => e.type === 'layerfs-availability' && e.data.stores.st.state === 'unavailable'))
  assert.ok(!f.calls.includes('mount') && !f.calls.includes('create') && !f.calls.includes('remove'), 'polling never starts, mounts or removes anything')
  // Access returns: the error clears, execution stays interrupted until a new message.
  down = false
  await f.s.refreshAvailability()
  const after = f.state().sessions.owner
  assert.equal(after.error, undefined)
  assert.equal(after.executionState, 'interrupted')
  assert.equal(after.state, 'closed')
  assert.ok(!f.calls.includes('mount'))
  assert.equal(f.s.availability().st.state, 'available')
})

test('the availability poll never deactivates on a busy Store, an unknown state or a failed list', async () => {
  const busy = polled(() => [{ store: 'st', state: 'busy', op: { name: 'move' } }, { store: 'ok', state: 'unknown' }])
  await busy.s.refreshAvailability()
  assert.equal(busy.state().sessions.owner.state, 'bound')
  assert.equal(busy.state().sessions.calm.state, 'bound')
  const offline = polled(Object.assign(new Error('cli gone'), { code: 'Unavailable' }))
  await offline.s.refreshAvailability()
  assert.equal(offline.state().sessions.owner.state, 'bound')
})

test('a removed Store and an unreachable PostgreSQL server deactivate their conversations', async () => {
  const removed = polled(() => [{ store: 'ok', state: 'ready' }])
  await removed.s.refreshAvailability()
  await removed.s.settled('owner')
  assert.equal(removed.state().sessions.owner.executionState, 'interrupted')
  assert.match(removed.state().sessions.owner.error, /no longer listed/)
  const pg = polled(() => [{ store: 'st', engine: 'postgresql', state: 'ready' }, { store: 'ok', state: 'ready' }],
    { st: () => ({ store: 'st', engine: 'postgresql', state: 'broken', reason: 'unreachable', fix: 'layerfs store doctor st' }) })
  await pg.s.refreshAvailability()
  await pg.s.settled('owner')
  assert.equal(pg.state().sessions.owner.executionState, 'interrupted')
  assert.match(pg.state().sessions.owner.error, /PostgreSQL server of Store st cannot be reached from this computer/)
  assert.ok(pg.calls.includes('get:st'), 'its server is read live')
})

test('a tool call never re-activates an interrupted context mid-turn; only a new message does', async () => {
  const state = S.initialState()
  state.sessions.live = { sessionId: 'live', kind: 'project', project: 'st/p', machine: 'vm', sandbox: 'box', branch: 'b', scratch: '/scratch', anchor: '/anchor', state: 'closed', executionState: 'interrupted', error: 'Store down', interruption: { kind: 'availability', at: 1 } }
  const calls = []
  const f = sessions({ state, lfs: { sandbox: { create: async () => { calls.push('create') } }, workspace: { mount: async () => { calls.push('mount') } } } })
  f.s.ctx.get = () => undefined
  await assert.rejects(f.s.ensureBound({ id: 'live', session: session('live', { cwd: '/scratch' }, ran) }, { rebind: false }), (error) => error.code === 'Unavailable' && /Store down/.test(error.message))
  assert.deepEqual(calls, [])
})

test('pgPlacement: the last machine, else a reached one, else native; placeable means running, PostgreSQL Sandboxes, not broken (spec v23)', () => {
  const m = (name, more = {}) => ({ name, supports: ['sqlite', 'postgresql_sandbox'], ...more })
  const fleet = [m('native'), m('docker'), m('vm1', { state: 'stopped' }), m('vm2', { state: 'broken' })]
  assert.equal(S.pgPlacement(fleet, { last: 'docker' }), 'docker')
  assert.equal(S.pgPlacement(fleet, { last: 'vm1', reached: ['vm2', 'docker'] }), 'docker')
  assert.equal(S.pgPlacement(fleet, { last: 'gone' }), 'native')
  assert.equal(S.pgPlacement([], {}), 'native')
  assert.deepEqual(S.reachedFrom({ connections: [{ machine: null, reached: 'ok' }, { machine: 'docker', reached: 'failed' }, { machine: 'vm', reached: 'ok' }] }), ['native', 'vm'])
  assert.equal(S.lastUsed([{ machine: 'a', created_at: '2026-01-01T00:00:00Z' }, { machine: 'b', created_at: '2026-02-01T00:00:00Z' }]), 'b')
  assert.equal(S.placeable({ name: 'x' }), true, 'a machine that says nothing about support or state is placeable')
})

test('Presets Reset deletes this home\'s project sessions only; loaded ones are archived with activity stopped first', async () => {
  const state = S.initialState()
  state.sessions.bound = { sessionId: 'bound', kind: 'project', project: 'demo/app', sandbox: 'dsh-bound', anchor: '/a', scratch: '/s', state: 'bound' }
  state.sessions.child = { sessionId: 'child', kind: 'shared', project: 'demo/app', parent: 'bound', anchor: '/a', scratch: '/s', state: 'bound' }
  state.sessions.folder = { sessionId: 'folder', kind: 'project', project: 'other/app', anchor: '/a', scratch: '/s', state: 'closed' }
  state.pending.draft = { project: 'demo/app' }
  state.checkpoints.bound = [{ commit: 'c1' }]
  state.mirror.bound = { count: 3, project: 'demo/app', branch: 'dsh.session.bound' }
  state.projects['demo/app'] = { project: 'demo/app', anchor: '/a', createdAt: 1 }
  const closed = [], archived = [], deleted = [], forgotten = []
  const f = sessions({ state, lfs: { sandbox: { remove: async (name, options) => { closed.push([name, options]); return { ok: true } } } } })
  // Owner routes: one in this home's Store, one in a same-named Store of another home (other Store ID).
  const owners = { bound: { project: 'demo/app', storeId: 'here' }, archivedOnly: { project: 'demo/old', storeId: 'here' }, foreign: { project: 'demo/app', storeId: 'elsewhere' } }
  const persistence = {
    ownedSessionIds: () => Object.keys(owners),
    ownerOf: (id) => owners[id] && { project: owners[id].project },
    storeIdOf: (id) => owners[id]?.storeId,
    deleteSessions: async (ids) => { deleted.push(...ids); return ids },
  }
  Object.assign(f.s.ctx, {
    get: (name) => (name === 'sessionPersistence' ? persistence : undefined),
    sessions: { get: (id) => (id === 'bound' ? { id } : undefined) },
    workspaceRegistry: { archiveSession: async (id, options) => { archived.push([id, options]) } },
  })
  Object.defineProperty(f.s, 'store', { value: { forget: (id) => forgotten.push(id) } })
  const stores = [{ name: 'demo', id: 'here' }]
  const listed = f.s.projectConversations(stores).map((c) => c.id)
  assert.deepEqual(listed.sort(), ['archivedOnly', 'bound', 'child', 'draft'])
  assert.deepEqual(f.s.projectConversations([{ name: 'demo' }]).map((c) => c.id).sort(), ['archivedOnly', 'bound', 'child', 'draft', 'foreign'], 'without a Store ID the name decides')
  const ids = await f.s.deleteProjectConversations(stores)
  assert.deepEqual(ids.sort(), ['archivedOnly', 'bound', 'child', 'draft'])
  assert.deepEqual(archived, [['bound', { stopActivity: true }]], 'only the loaded session is archived, with its activity stopped')
  assert.deepEqual(closed, [['dsh-bound', { force: true }]])
  assert.deepEqual(deleted.sort(), ids.sort())
  assert.deepEqual(forgotten.sort(), ids.sort())
  assert.deepEqual(Object.keys(f.state().sessions), ['folder'], 'another Store\'s binding stays')
  assert.deepEqual(f.state().pending, {})
  assert.deepEqual(f.state().checkpoints, {})
  assert.deepEqual(f.state().mirror, {})
  assert.deepEqual(Object.keys(f.state().projects), ['demo/app'], 'project workspaces are not this step\'s to drop')
  assert.ok(f.events.some((e) => e.type === 'layerfs-workspaces-changed' && e.data.project === 'demo/app'))
})

// ---------------------------------------------------------------- checkpoint receipts

const hex = (c) => c.repeat(66)
/** A fake lfs whose calls go through the real trace path (receipt id + log tag). */
function traced(operation, answer, tags, target) {
  return (...args) => S.tracing.traceInvocation(operation, async (tag) => { tags.push(tag); return answer(...args) }, undefined, target?.(...args))
}
function receiptFixture(lfs, state) {
  const recorded = []
  const f = sessions({ state, lfs })
  f.s.ctx.get = (name) => name === 'sessionPersistence' ? { recordInvocation: async (row) => { recorded.push(row) }, updateSidecar: async () => {} } : undefined
  return { ...f, recorded }
}
const boundState = (extra = {}) => {
  const state = S.initialState()
  state.sessions.a = { sessionId: 'a', kind: 'project', project: 'st/app', state: 'bound', ws: 'sb:main', sandbox: 'sb', branch: 'internal', ...extra }
  return state
}

test('a turn-end save carries its receipt; UpToDate binds as unchanged; a refused save keeps the refusal receipt', async () => {
  const tags = []
  // 2.0 answers: `workspace status --detail` counts its changes; `workspace commit` answers `commit` (+ `unchanged`)
  let answer = { commit: hex('a') }
  const { s, recorded } = receiptFixture({ workspace: {
    status: async () => ({ changes: [{ path: '/a', change: 'modified' }, { path: '/b', change: 'added' }], changes_count: 2 }),
    commit: traced('workspace.commit', () => { if (answer instanceof Error) throw answer; return answer }, tags, (ws) => ws),
  } }, boundState())
  const saved = await s.checkpoint('a', 'turn', 1, 'dsh turn 1')
  assert.equal(saved.commit, hex('a')); assert.equal(saved.changed, 2)
  assert.equal(saved.receipt, recorded[0].id)
  assert.equal(tags[0], `dsh:a:${saved.receipt}`, 'tagged with the owning session id')
  assert.deepEqual(recorded[0].versions, [{ kind: 'Commit', id: hex('a'), effect: 'created' }])
  answer = { commit: hex('a'), unchanged: true }
  const same = await s.checkpoint('a', 'manual', 1, 'Checkpoint')
  assert.equal(same.changed, 0); assert.equal(same.commit, hex('a')); assert.equal(same.receipt, recorded[1].id)
  assert.notEqual(same.receipt, saved.receipt)
  answer = Object.assign(new Refusal('Conflict', 'markers'), { transport: 'refusal' })
  const failed = await s.checkpoint('a', 'manual', 1, 'Checkpoint')
  assert.equal(failed.commit, null); assert.equal(failed.error.code, 'Conflict'); assert.equal(failed.receipt, recorded[2].id)
  assert.equal(recorded[2].outcome, 'refusal')
  assert.deepEqual(s.checkpoints('a').map((c) => c.receipt), recorded.map((r) => r.id))
})

test('a restore checkpoint carries the successful fork receipt', async () => {
  const tags = []
  const state = boundState()
  state.checkpoints.a = [{ kind: 'turn', turn: 1, seq: 3, endSeq: 4, commit: hex('c'), branch: 'gone-branch', changed: 1, message: 't1', at: 1 }]
  const { s, recorded } = receiptFixture({
    workspace: {
      status: async () => ({ changes_count: 0 }),
      commit: traced('workspace.commit', () => ({ commit: hex('c'), unchanged: true }), tags, (ws) => ws),
      unmount: async () => ({ ok: true }), mount: async () => ({ workspace: 'sb:main', path: '/mnt' }),
    },
    branch: { fork: traced('branch.fork', (project, branch, from) => { if (from !== 'main') throw new Refusal('NotFound'); return { commit: hex('c') } }, tags) },
    sandbox: { get: async (name) => ({ sandbox: name, state: 'ready', workspaces: [] }) },
  }, state)
  await s.restore('a', hex('c'))
  const restore = s.checkpoints('a').at(-1)
  assert.equal(restore.kind, 'restore')
  const forks = recorded.filter((r) => r.operation === 'branch.fork')
  assert.equal(forks.length, 2); assert.equal(forks[1].outcome, 'success')
  assert.equal(restore.receipt, forks[1].id)
  assert.deepEqual(forks[1].versions, [{ kind: 'Commit', id: hex('c'), effect: 'referenced' }])
  assert.ok(tags.includes(`dsh:a:${restore.receipt}`))
})

test('a branched child base checkpoint carries the fork receipt, tagged with the child session', async () => {
  const tags = []
  const state = boundState()
  state.checkpoints.a = [{ kind: 'turn', turn: 1, seq: 3, endSeq: 4, commit: hex('d'), branch: 'internal', changed: 1, message: 't1', at: 1 }]
  state.sessions.child = { sessionId: 'child', kind: 'project', project: 'st/app', state: 'bound', ws: 'sbc:main', sandbox: 'sbc', branch: 'first' }
  const { s, recorded } = receiptFixture({
    workspace: { unmount: async () => ({ ok: true }), mount: async () => ({ workspace: 'sbc:main', path: '/mnt' }) },
    branch: { fork: traced('branch.fork', () => ({ commit: hex('d') }), tags) },
    sandbox: { get: async (name) => ({ sandbox: name, state: 'ready', workspaces: [] }) },
  }, state)
  const persistence = s.ctx.get('sessionPersistence')
  s.ctx.get = (name) => name === 'sessionPersistence' ? persistence : name === 'sessionController' ? { fork: async () => ({ sessionId: 'child' }) } : undefined
  await s.branchAt('a', hex('d'), 1)
  const [baseCheckpoint] = s.checkpoints('child')
  assert.equal(baseCheckpoint.kind, 'base')
  assert.equal(recorded.length, 1); assert.equal(recorded[0].sessionId, 'child')
  assert.equal(baseCheckpoint.receipt, recorded[0].id)
  assert.equal(tags[0], `dsh:child:${baseCheckpoint.receipt}`)
})

test('archive writes stay untagged and record no receipt', async () => {
  const recorded = [], tags = []
  await S.tracing.withTraceContext({ origin: 'host', sessionId: 'a', project: 'st/app', record: async (row) => { recorded.push(row) } }, () =>
    S.tracing.withoutTraceContext(() => S.tracing.traceInvocation('file.apply', async (tag) => { tags.push(tag); return { commit: hex('e') } })))
  assert.deepEqual(tags, [undefined]); assert.deepEqual(recorded, [])
})

// ---------------------------------------------------------------- review fixes

test('choosing a project with an unclaimed listing id claims it (the one claiming path)', async () => {
  const asserted = []
  const lfs = { project: { get: async (key) => ({ key }) } }
  const { s, state } = sessions({ lfs, layerfsRoot: mkdtempSync(join(tmpdir(), 'dsh-root-')) })
  s.ctx.get = (name) => name === 'sessionPersistence' ? { catalog: { assert: async (...args) => { asserted.push(args) } } } : undefined
  await s.choose('n', 'st/app', 'unclaimed:st/app')
  assert.deepEqual(asserted, [['st/app', 'unclaimed:st/app', { claim: true }]])
  assert.deepEqual(state().pending.n, { project: 'st/app' })
  rmSync(s.ctx.layerfs.root, { recursive: true, force: true })
})

test('a turn with changes (an in-place edit counts as one) or a pending merge is checkpointed; an unchanged turn is not', async () => {
  // 2.0 `workspace status --detail`: `changes` lists the first 20, `changes_count` all of them
  let status = { changes: [{ path: '/a.txt', change: 'modified' }], changes_count: 1, pending_merge: false, branch_moved: false }
  let commits = 0
  const { s } = receiptFixture({ workspace: {
    status: async () => status,
    commit: async () => { commits++; return { commit: hex('f') } },
  } }, boundState())
  const saved = await s.checkpoint('a', 'turn', 1, 'dsh turn 1')
  assert.equal(commits, 1)
  assert.equal(saved.commit, hex('f')); assert.equal(saved.changed, 1)
  status = { changes: [{ path: '/a', change: 'modified' }], changes_count: 3, pending_merge: false, branch_moved: false }
  assert.equal((await s.checkpoint('a', 'turn', 2, 'dsh turn 2')).changed, 3, 'changed is changes_count, not the listed page')
  status = { changes: [], changes_count: 0, pending_merge: true, branch_moved: false }
  assert.ok(await s.checkpoint('a', 'turn', 3, 'dsh turn 3'), 'a pending merge is saved')
  status = { changes: [], changes_count: 0, pending_merge: false, branch_moved: false }
  assert.equal(await s.checkpoint('a', 'turn', 4, 'dsh turn 4'), undefined, 'an unchanged turn records nothing')
  assert.equal(commits, 3)
})

test('a restore whose mount fails keeps the forked branch, unbound, and a retry forks the next generation', async () => {
  const state = boundState()
  state.checkpoints.a = [{ kind: 'turn', turn: 1, seq: 3, endSeq: 4, commit: hex('c'), branch: 'internal', changed: 1, message: 't1', at: 1 }]
  const forks = []
  let failMount = 1
  const { s } = receiptFixture({
    workspace: {
      status: async () => ({ changes_count: 0 }),
      commit: async () => ({ commit: hex('c'), unchanged: true }),
      unmount: async () => ({ ok: true }),
      mount: async (sandbox, branch) => { if (failMount-- > 0) throw new Refusal('Unavailable', 'worker gone'); return { workspace: `${sandbox}:main`, path: '/mnt', commit: hex('c') } },
    },
    branch: { fork: async (project, branch) => { if (forks.includes(branch)) throw new Refusal('Exists'); forks.push(branch); return { commit: hex('c') } } },
    sandbox: { get: async (name) => ({ sandbox: name, state: 'ready', workspaces: [] }) },
  }, state)
  await assert.rejects(s.restore('a', hex('c')), (e) => e.code === 'Unavailable')
  const failed = s.binding('a')
  assert.equal(failed.branch, S.branchOf('a', 1), 'the forked branch is recorded at once')
  assert.equal(failed.restores, 1)
  assert.equal(failed.state, 'failed'); assert.equal(failed.ws, undefined); assert.equal(failed.mount, undefined)
  assert.match(failed.error, /worker gone/)
  assert.equal(s.checkpoints('a').at(-1).kind, 'restore')
  // Retried: no `Exists`, the next generation is forked and mounted.
  const next = await s.restore('a', hex('c'))
  assert.deepEqual(forks, [S.branchOf('a', 1), S.branchOf('a', 2)])
  assert.equal(next.branch, S.branchOf('a', 2)); assert.equal(next.state, 'bound'); assert.equal(next.ws, 'sb:main')
  assert.equal(s.binding('a').state, 'bound')
})

test('a branched child whose mount fails keeps its forked branch for the next message', async () => {
  const state = boundState()
  state.checkpoints.a = [{ kind: 'turn', turn: 1, seq: 3, endSeq: 4, commit: hex('d'), branch: 'internal', changed: 1, message: 't1', at: 1 }]
  state.sessions.child = { sessionId: 'child', kind: 'project', project: 'st/app', state: 'bound', ws: 'sbc:main', sandbox: 'sbc', branch: 'first' }
  const { s } = receiptFixture({
    workspace: { unmount: async () => ({ ok: true }), mount: async () => { throw new Refusal('Unavailable', 'worker gone') } },
    branch: { fork: async () => ({ commit: hex('d') }) },
    sandbox: { get: async (name) => ({ sandbox: name, state: 'ready', workspaces: [] }) },
  }, state)
  const persistence = s.ctx.get('sessionPersistence')
  s.ctx.get = (name) => name === 'sessionPersistence' ? persistence : name === 'sessionController' ? { fork: async () => ({ sessionId: 'child' }) } : undefined
  await assert.rejects(s.branchAt('a', hex('d'), 1), (e) => e.code === 'Unavailable')
  const child = s.binding('child')
  assert.equal(child.branch, S.branchOf('child', 1))
  assert.equal(child.state, 'failed'); assert.equal(child.ws, undefined)
  assert.deepEqual(child.from, { session: 'a', commit: hex('d'), turn: 1 })
})

// Quarry v2 FIX-LOG F21: a readiness read failing under host load interrupted a running team.
test('a failed readiness check keeps a bound execution whose Workspace still answers (F21)', async () => {
  const state = S.initialState()
  state.sessions.owner = { sessionId: 'owner', kind: 'project', project: 'st/app', anchor: '/a', scratch: '/s', machine: 'docker', branch: 'dsh.owner', sandbox: 'dsh-owner', ws: 'dsh-owner:main', state: 'bound', executionState: 'active', createdAt: 0, updatedAt: 0 }
  state.sessions.child = { sessionId: 'child', kind: 'shared', project: 'st/app', parent: 'owner', anchor: '/a', scratch: '', state: 'bound', createdAt: 0, updatedAt: 0 }
  let answers = true
  const { s, state: now } = sessions({ state, lfs: {
    machine: { get: async () => ({ state: 'unknown' }) },
    store: { get: async () => ({ store: 'st', state: 'unknown' }) },
    workspace: { status: async () => { if (!answers) throw new Refusal('NotFound'); return { workspace: 'dsh-owner:main' } } },
  } })
  s.ctx.get = (name) => name === 'sessionPersistence' ? { assertSessionIdentity: async () => ({ kind: 'sqlite', machine: 'docker', store: 'st', key: 'st/app' }) } : undefined
  await s.validateExecutionIdentity('child')
  assert.equal(now().sessions.owner.state, 'bound', 'the owner stays bound')
  assert.equal(now().sessions.owner.ws, 'dsh-owner:main')
  assert.equal(now().sessions.owner.interruption, undefined)
  // A Workspace that no longer answers still interrupts, as before.
  answers = false
  s.identityChecked.clear()
  await assert.rejects(s.validateExecutionIdentity('child'), (e) => e.code === 'Unavailable')
  assert.equal(now().sessions.owner.state, 'closed')
  assert.equal(now().sessions.owner.executionState, 'interrupted')
})

// Quarry v2 FIX-LOG F23: re-activation over a still-mounted Workspace left `ws` unset.
test('re-activating an interrupted binding over a still-mounted Workspace restores its handle (F23)', async () => {
  const mounts = []
  const { s } = sessions({ lfs: {
    sandbox: { get: async (name) => ({ sandbox: name, state: 'ready', workspaces: [{ workspace: 'dsh-owner:main', branch: 'dsh.owner', path: '/layerfs/run/main' }] }) },
    workspace: { mount: async (...args) => { mounts.push(args); return { workspace: 'dsh-owner:main' } }, unmount: async () => ({ ok: true }) },
  } })
  const binding = { sessionId: 'owner', kind: 'project', project: 'st/app', machine: 'docker', branch: 'dsh.owner', sandbox: 'dsh-owner', state: 'closed', executionState: 'interrupted' }
  const mount = await s.mount(binding)
  assert.equal(mount, undefined, 'a docker Workspace has no host path')
  assert.equal(binding.ws, 'dsh-owner:main', 'the Workspace handle is restored')
  assert.deepEqual(mounts, [], 'nothing is remounted')
})
