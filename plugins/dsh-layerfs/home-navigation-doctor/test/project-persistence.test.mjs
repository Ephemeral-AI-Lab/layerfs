import { test } from 'node:test'
import assert from 'node:assert/strict'
import { build } from 'esbuild'
import { mkdtempSync, mkdirSync, rmSync, readFileSync, writeFileSync, readdirSync, symlinkSync, chmodSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { createHash } from 'node:crypto'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { Context } from '@deepseek-ai/cordis'

const root = fileURLToPath(new URL('..', import.meta.url))
mkdirSync(join(root, 'lib'), { recursive: true })
const out = mkdtempSync(join(root, 'lib', '.test-project-persistence-'))
await build({ absWorkingDir: root, stdin: { contents: "export * from './src/host/session-persistence.ts'; export { SessionStore, planStream, PART_LIMIT, COMPACT_FLOOR } from './src/host/sessions/store.ts'", resolveDir: root, loader: 'ts' }, outfile: join(out, 'persistence.mjs'), bundle: true, format: 'esm', platform: 'node', target: 'node24', external: ['@deepseek-ai/*'] })
const persistence = await import(pathToFileURL(join(out, 'persistence.mjs')).href)
test.after(() => rmSync(out, { recursive: true, force: true }))

/** `branches`: share another fixture's project, as a second DSH reaching the same LayerFS project. */
async function fixture(branches = ['main']) {
  const dir = mkdtempSync(join(root, '.dev-test-persistence-'))
  const ctx = new Context()
  // One project whose branches hold its UUID (`dsh.project.<uuid>`) once claimed.
  // 2.0 long forms: a project row, Branch rows keyed by `branch` (listed by prefix), a ready Store
  const info = (key) => ({ project: key, store: key.split('/')[0], engine: 'sqlite', machine: 'native', created_at: '1970-01-01T00:00:00.100Z' })
  ctx.provide('layerfs', { root: join(dir, 'plugin'), projectSessionHome: join(dir, 'project-home'), home: join(dir, 'layerfs-home'), lfs: {
    project: { list: async () => ({ items: [info('store/project')] }), get: async (key) => info(key) },
    branch: { list: async (_, o = {}) => ({ items: branches.filter((name) => !o.prefix || name.startsWith(o.prefix)).map((branch) => ({ branch })) }), fork: async (_, name) => { if (!branches.includes(name)) branches.push(name); return { commit: `32${'a'.repeat(64)}` } } },
    store: { get: async () => ({ state: 'ready' }) },
  } })
  const fork = ctx.plugin(persistence, { root: join(dir, 'normal') })
  await fork.await()
  return { dir, ctx, branches, p: ctx.get('sessionPersistence'), close: async () => { await fork.dispose(); rmSync(dir, { recursive: true, force: true }) } }
}
const header = (id) => ({ version: 4, id, createdAt: 123, cwd: '/original/local/folder', isSeeded: false })

test('project ownership routes unpacked native logs, normal sessions stay stock, owner survives restart', async () => {
  const f = await fixture()
  try {
    const normal = await f.p.create(header('normal'))
    await normal.flush()
    await normal.close()
    const project = await f.p.create(header('project'))
    await project.flush()
    await f.p.migrateProject('project', 'store/project')
    assert.match((await f.p.stat('normal')).location.path, /\/normal\//)
    const location = (await f.p.stat('project')).location.path
    assert.match(location, /\/project-home\/projects\/[0-9a-f-]{36}\/sessions\//)
    assert.match(location, /\.jsonl$/)
    const physical = readFileSync(location, 'utf8').trim().split('\n').map(JSON.parse)
    assert.equal(physical[0].id, 'project')
    assert.equal(physical[0].cwd, '/original/local/folder')
    assert.deepEqual((await project.read()).events, [])
    await project.close()
    const stock = f.p.stock
    assert.equal(await stock.stat('project'), undefined)
    assert.equal((await f.p.list()).length, 2)
    await f.p.recordInvocation({ id: 'operation', sessionId: 'project', operation: 'file.write' })
    await f.p.recordInvocation({ id: 'operation', sessionId: 'project', operation: 'file.write', outcome: 'success' })
    assert.equal((await f.p.readSidecar('project')).invocations.length, 1)
    assert.equal((await f.p.readSidecar('project')).invocations[0].outcome, 'success')
    // A fresh facade and fresh backends must resolve the persisted owner locally.
    const fresh = new persistence.LayerfsSessionPersistence(f.ctx.isolate('sessionPersistence'), stock)
    fresh.service.lfs.project.get = async () => { throw new Error('offline') }
    assert.equal((await fresh.stat('project')).header.id, 'project')
    assert.deepEqual(fresh.ownerOf('project'), { project: 'store/project' })
    assert.deepEqual(fresh.ownedSessionIds('store/project'), ['project'])
    assert.equal(fresh.ownerOf('normal'), undefined)
  } finally { await f.close() }
})

test('clearing owner before work returns original handle to normal storage without rewriting header', async () => {
  const f = await fixture()
  try {
    const session = await f.p.create(header('clear'))
    await session.flush()
    await f.p.migrateProject(session, 'store/project')
    await f.p.migrateProject(session, null)
    await session.flush()
    assert.match((await f.p.stat('clear')).location.path, /\/normal\//)
    assert.deepEqual(session.header, { ...header('clear'), delegationDepth: 0 })
    await session.close()
  } finally { await f.close() }
})


test('legacy binding keeps stock log readable until validated migration', async () => {
  const f = await fixture()
  try {
    const session = await f.p.create(header('legacy'))
    await session.flush()
    f.ctx.provide('layerfsSessions', { state: { read: () => ({ sessions: { legacy: { project: 'store/project' } } }) } })
    assert.match((await f.p.stat('legacy')).location.path, /\/normal\//)
    await f.p.migrateProject('legacy', 'store/project')
    assert.match((await f.p.stat('legacy')).location.path, /\/project-home\//)
    assert.deepEqual((await session.read()).events, [])
    await session.close()
  } finally { await f.close() }
})


function archiveFixture(f, files) {
  const state = { projects: {}, sessions: {}, pending: {}, checkpoints: {}, mirror: {} }
  const applied = []
  const binding = { sessionId: 'portable', kind: 'project', project: 'store/project', branch: 'internal-code', scratch: '/original/local/folder' }
  const session = { header: header('portable'), snapshotEvents: () => [], inheritedEventCount: 0 }
  const sessions = {
    binding: (id) => id === 'portable' ? binding : undefined, lane: async (_, run) => run(), checkpoints: () => [],
    state: { read: () => state, update: (fn) => fn(state) },
    ensureAnchor: () => join(f.dir, 'safe-local-anchor'), recordedWorkspace: () => undefined, attach: async () => {},
  }
  f.ctx.layerfs.emit = () => {}
  f.ctx.layerfs.lfs.file = {
    apply: async (_, branch, changes) => {
      applied.push(changes)
      for (const change of changes) {
        if (change.op === 'write') files.set(change.path, change.content)
        if (change.op === 'rm') { if (!files.has(change.path)) throw Object.assign(new Error(change.path), { code: 'NotFound' }); files.delete(change.path) }
      }
    },
    read: async (_, branch, path) => ({ text: files.get(path) }),
  }
  f.ctx.layerfs.lfs.project = { ...f.ctx.layerfs.lfs.project, export: async (_, destination) => {
    for (const [path, content] of files) {
      const target = join(destination, path.replace(/^\//, ''))
      mkdirSync(dirname(target), { recursive: true })
      writeFileSync(target, content)
    }
  } }
  const liveSessions = { get: () => session }
  f.ctx.provide('sessions', liveSessions)
  return { store: new persistence.SessionStore(f.ctx, sessions), state, liveSessions, binding, sessions, applied }
}

test('portable archive preserves native JSONL and sidecar, imports safely and refuses changed history', async () => {
  const a = await fixture(), b = await fixture(a.branches)
  const files = new Map()
  try {
    const handle = await a.p.create(header('portable'))
    await handle.flush()
    await a.p.migrateProject('portable', 'store/project')
    await a.p.recordInvocation({ id: 'op', sessionId: 'portable', project: 'store/project', operation: 'file.write', versions: [{ kind: 'Commit', id: 'commit-1', effect: 'created' }] })
    const archiveA = archiveFixture(a, files)
    archiveA.liveSessions.get = () => undefined // Session already disposed; durable log still archives.
    await archiveA.store.mirror('portable')
    assert.equal(JSON.parse(files.get('/.dsh/session/manifest.json')).version, 3)
    assert.equal(JSON.parse(files.get('/.dsh/session/session.jsonl').trim()).id, 'portable')
    const archiveB = archiveFixture(b, files)
    assert.equal((await archiveB.store.import('store/project', 'portable')).imported, true)
    const imported = await b.p.open('portable', 'read')
    assert.equal(imported.header.cwd, '/original/local/folder')
    assert.equal((await b.p.readSidecar('portable')).invocations[0].versions[0].id, 'commit-1')
    assert.equal((await b.p.readSidecar('portable')).codeBranch, 'internal-code')
    assert.deepEqual(b.p.ownerOf('portable'), { project: 'store/project' })
    assert.ok(archiveB.state.sessions.portable.scratch.startsWith(b.dir))
    assert.equal((await archiveB.store.import('store/project', 'portable')).imported, false)
    await imported.close()
    await handle.close()
    // A changed archive cannot overwrite locally durable work.
    files.set('/.dsh/session/session.jsonl', 'corrupted')
    assert.equal((await archiveB.store.import('store/project', 'portable')).imported, false)
  } finally { await a.close(); await b.close() }
})

test('migration preserves native event identities and the exact fork-inherited cut', async () => {
  const f = await fixture()
  try {
    const meta = { ...header('fork'), parentSession: 'parent', isSeeded: true }
    const events = [{ seq: 0, time: 123, type: 'plugin:layerfs/test', ignorable: true, data: { toolCallId: 'original-tool-call', payload: 'unchanged' } }, { seq: 1, time: 123, type: 'session/end-seed', data: { inherited: true } }]
    const fork = await f.p.create(meta, { inheritedEventCount: 1 })
    await fork.append(events)
    await fork.flush()
    await f.p.migrateProject('fork', 'store/project')
    assert.equal(fork.inheritedEventCount, 1)
    assert.deepEqual((await fork.read()).events, events)
    await fork.close()
    const reopened = await f.p.open('fork', 'read')
    assert.equal(reopened.inheritedEventCount, 1)
    assert.deepEqual((await reopened.read()).events, events)
    await reopened.close()
  } finally { await f.close() }
})


test('shared-session archive retains parent provenance without recursive parent imports', async () => {
  const a = await fixture(), b = await fixture(a.branches)
  const files = new Map()
  try {
    const handle = await a.p.create(header('portable'))
    await handle.flush()
    await a.p.migrateProject('portable', 'store/project')
    const archiveA = archiveFixture(a, files)
    archiveA.binding.kind = 'shared'
    archiveA.binding.parent = 'original-parent'
    await archiveA.store.mirror('portable')
    const archiveB = archiveFixture(b, files)
    assert.equal((await archiveB.store.import('store/project', 'portable')).imported, true)
    assert.equal(archiveB.state.sessions.portable.kind, 'shared')
    assert.equal(archiveB.state.sessions.portable.parent, 'original-parent')
    assert.equal((await b.p.readSidecar('portable')).sharedParent, 'original-parent')
    await handle.close()
  } finally { await a.close(); await b.close() }
})

test('simulation archive/import preserves the exact handoff pin and never replays its private script', async () => {
  const a = await fixture(), b = await fixture(a.branches)
  const files = new Map()
  try {
    const handle = await a.p.create(header('portable'))
    await handle.flush()
    await a.p.migrateProject('portable', 'store/project')
    const simulation = {
      runId: 'finished-run', agentKey: 'release-main', scriptId: 'release-script', name: 'Billing release',
      hash: 'b'.repeat(64), ordinaryModel: { provider: 'real', model: 'next-model', reasoningEffort: 'high' },
      state: 'completed', handoffReady: true,
      continuation: { kind: 'Layer', id: '01' + 'a'.repeat(64), branch: 'main' },
    }
    await a.p.updateSidecar('portable', sidecar => { sidecar.simulation = simulation })
    const archiveA = archiveFixture(a, files)
    archiveA.binding.branch = undefined // The script owns resources until the separate LLM workspace is established.
    archiveA.binding.simulation = simulation
    await archiveA.store.mirror('portable')
    const archiveB = archiveFixture(b, files)
    assert.equal((await archiveB.store.import('store/project', 'portable')).imported, true)
    const imported = archiveB.state.sessions.portable
    assert.deepEqual(imported.simulation, simulation)
    assert.equal(imported.branch, undefined)
    assert.equal(imported.ws, undefined)
    assert.equal(imported.sandbox, undefined)
    assert.equal(imported.state, 'closed')
    assert.deepEqual((await b.p.readSidecar('portable')).simulation.continuation, simulation.continuation)
    const reopened = await b.p.open('portable', 'read')
    assert.deepEqual((await reopened.read()).events, [])
    await reopened.close()
    // Import reads only archive files and native logs; there is no local run package or executable script.
    assert.equal((await archiveB.store.import('store/project', 'portable')).imported, false)
    await handle.close()
  } finally { await a.close(); await b.close() }
})

test('imported unfinished simulation remains inert and requires reconciliation', async () => {
  const a = await fixture(), b = await fixture(a.branches)
  const files = new Map()
  try {
    const handle = await a.p.create(header('portable'))
    await handle.flush()
    await a.p.migrateProject('portable', 'store/project')
    const simulation = { runId: 'unfinished', agentKey: 'main', scriptId: 'main-script', name: 'Release', hash: 'c'.repeat(64), state: 'running', handoffReady: false }
    await a.p.updateSidecar('portable', sidecar => { sidecar.simulation = simulation })
    const archiveA = archiveFixture(a, files)
    archiveA.binding.branch = undefined
    archiveA.binding.simulation = simulation
    await archiveA.store.mirror('portable')
    const archiveB = archiveFixture(b, files)
    await archiveB.store.import('store/project', 'portable')
    assert.equal(archiveB.state.sessions.portable.simulation.state, 'interrupted')
    assert.equal(archiveB.state.sessions.portable.simulation.handoffReady, false)
    assert.equal(archiveB.state.sessions.portable.ws, undefined)
    assert.equal(archiveB.state.sessions.portable.branch, undefined)
    await handle.close()
  } finally { await a.close(); await b.close() }
})

test('simulation import never mounts an archived mutable code branch in place of its saved pin', async () => {
  const a = await fixture(), b = await fixture(a.branches)
  const files = new Map()
  try {
    const handle = await a.p.create(header('portable'))
    await handle.flush()
    await a.p.migrateProject('portable', 'store/project')
    const simulation = { runId: 'run', agentKey: 'main', scriptId: 'script', name: 'Release', hash: 'c'.repeat(64), state: 'completed', handoffReady: true, continuation: { kind: 'Commit', id: '02' + 'a'.repeat(64), branch: 'mutable-code' } }
    await a.p.updateSidecar('portable', sidecar => { sidecar.simulation = simulation })
    const archiveA = archiveFixture(a, files)
    archiveA.binding.branch = 'mutable-code'
    archiveA.binding.simulation = simulation
    await archiveA.store.mirror('portable')
    const archiveB = archiveFixture(b, files)
    await archiveB.store.import('store/project', 'portable')
    const imported = archiveB.state.sessions.portable
    assert.equal(imported.branch, undefined, 'remote mutable branch must not bypass pin-aware fork')
    assert.equal(imported.ws, undefined)
    assert.equal(imported.resumeFromArchive, true)
    assert.equal(imported.simulation.continuation.id, simulation.continuation.id)
    await handle.close()
  } finally { await a.close(); await b.close() }
})

test('one pinned export imports native JSONL, manifest and annotations with lines far beyond file.read windows', async () => {
  const a = await fixture(), b = await fixture(a.branches), files = new Map()
  try {
    const event = { seq: 0, time: 123, type: 'plugin:layerfs/large-tool-result', ignorable: true, data: { toolCallId: 'actual-native-tool-call', payload: 'λ🧪\n'.repeat(30000) } }
    const handle = await a.p.create(header('portable'))
    await handle.append([event]); await handle.flush()
    await a.p.migrateProject('portable', 'store/project')
    const invocations = Array.from({ length: 1500 }, (_, index) => ({ id: `invocation-${index}`, toolCallId: `native-${index}`, operation: 'file.write', outcome: 'success', versions: [{ kind: 'Commit', id: '02' + 'a'.repeat(64), effect: 'created' }] }))
    await a.p.updateSidecar('portable', sidecar => { sidecar.invocations = invocations })
    const archiveA = archiveFixture(a, files)
    archiveA.sessions.checkpoints = () => Array.from({ length: 1000 }, (_, index) => ({ kind: 'turn', turn: index, commit: '02' + 'a'.repeat(64), branch: 'internal-code', changed: 1 }))
    await archiveA.store.mirror('portable')
    for (const path of ['session.jsonl', 'manifest.json', 'layerfs.json']) assert.ok(Buffer.byteLength(files.get(`/.dsh/session/${path}`)) > 48 * 1024)
    const discoveredManifest = await archiveA.store.readManifest('store/project', 'dsh.session.portable')
    assert.equal(discoveredManifest.checkpoints.length, 1000, 'discovery reads a complete manifest beyond the line cap')
    const nativeBytes = files.get('/.dsh/session/session.jsonl')
    assert.ok(Buffer.byteLength(nativeBytes.split('\n')[1]) > 48 * 1024, 'one native row exceeds the line-window cap')
    const archiveB = archiveFixture(b, files)
    const exportSnapshot = b.ctx.layerfs.lfs.project.export
    let exports = 0
    b.ctx.layerfs.lfs.project.export = async (...args) => {
      exports++
      await exportSnapshot(...args)
      // A later archive mutation must not mix a different manifest/log/sidecar
      // into this import; the successful export is one immutable snapshot.
      files.set('/.dsh/session/session.jsonl', 'changed after pin')
      chmodSync(args[1], 0o500)
    }
    b.ctx.layerfs.lfs.file.read = async () => { throw new Error('bounded file windows cannot transport session archives') }
    await archiveB.store.import('store/project', 'portable')
    assert.equal(exports, 1)
    const imported = await b.p.open('portable', 'read')
    assert.deepEqual((await imported.read()).events, [event])
    assert.equal(readFileSync((await b.p.stat('portable')).location.path, 'utf8'), nativeBytes, 'native DSH bytes remain exact')
    assert.deepEqual((await b.p.readSidecar('portable')).invocations, invocations.map(row => ({ ...row, imported: true })), 'receipts arrive whole, marked imported')
    assert.equal(archiveB.state.checkpoints.portable.length, 1000)
    assert.deepEqual(readdirSync(join(b.ctx.layerfs.root, 'archive-imports')), [], 'only the owned export is cleaned, including read-only root metadata')
    await imported.close(); await handle.close()
  } finally { await a.close(); await b.close() }
})

test('archive readers reject exported symlink components without following or deleting their targets', async () => {
  const b = await fixture(), files = new Map()
  try {
    const archive = archiveFixture(b, files)
    const external = join(b.dir, 'outside-export')
    mkdirSync(external)
    const sentinel = join(external, 'manifest.json')
    writeFileSync(sentinel, '{"sessionId":"portable"}\n')
    b.ctx.layerfs.lfs.project.export = async (_, destination) => {
      mkdirSync(destination)
      symlinkSync(external, join(destination, '.dsh'))
    }
    await assert.rejects(archive.store.readManifest('store/project', 'dsh.session.portable'), error => error.code === 'InvalidInput' && /real directories/.test(error.message))
    assert.equal(readFileSync(sentinel, 'utf8'), '{"sessionId":"portable"}\n')
    assert.deepEqual(readdirSync(join(b.ctx.layerfs.root, 'archive-imports')), [])
    assert.equal(await b.p.stat('portable'), undefined)
  } finally { await b.close() }
})

test('failed exports leave no import state or owned temporary tree', async () => {
  const b = await fixture(), files = new Map()
  try {
    const archive = archiveFixture(b, files)
    b.ctx.layerfs.lfs.project.export = async () => { throw new Error('export disconnected') }
    await assert.rejects(archive.store.import('store/project', 'portable'), /export disconnected/)
    assert.equal(await b.p.stat('portable'), undefined)
    assert.equal(b.p.ownerOf('portable'), undefined)
    assert.deepEqual(readdirSync(join(b.ctx.layerfs.root, 'archive-imports')), [])
  } finally { await b.close() }
})

test('small manifests use file.read only after complete flags and exact UTF8 byte size prove the response is whole', async () => {
  const b = await fixture(), files = new Map()
  try {
    const archive = archiveFixture(b, files)
    const manifest = { sessionId: 'portable', title: 'λ🧪 project', count: 5 }
    const text = JSON.stringify(manifest) + '\n'
    let exports = 0
    b.ctx.layerfs.lfs.project.export = async () => { exports++; throw new Error('a complete small manifest needs no export') }
    for (const size of [Buffer.byteLength(text), BigInt(Buffer.byteLength(text)), String(Buffer.byteLength(text))]) {
      b.ctx.layerfs.lfs.file.read = async () => ({ kind: 'ok', text, start_line: 1, end_line: 1, line_truncated: false, truncated: false, next_line: null, size })
      assert.deepEqual(await archive.store.readManifest('store/project', 'dsh.session.portable'), manifest)
    }
    assert.equal(exports, 0)
  } finally { await b.close() }
})

test('clipped or unproven manifest windows fall back to one exact export', async () => {
  const b = await fixture(), files = new Map()
  try {
    const manifest = { sessionId: 'portable', title: 'λ🧪'.repeat(20000), count: 5 }
    const text = JSON.stringify(manifest) + '\n'
    files.set('/.dsh/session/manifest.json', text)
    const archive = archiveFixture(b, files)
    const exportSnapshot = b.ctx.layerfs.lfs.project.export
    let exports = 0
    b.ctx.layerfs.lfs.project.export = async (...args) => { exports++; return exportSnapshot(...args) }
    for (const response of [
      { kind: 'ok', text: text.slice(0, 40000), start_line: 1, end_line: 1, line_truncated: true, truncated: false, next_line: null, size: Buffer.byteLength(text) },
      { kind: 'ok', text: '{"sessionId":"wrong"}', start_line: 1, line_truncated: false, truncated: false, next_line: null, size: Buffer.byteLength(text) },
      { text: '{"sessionId":"unproven"}' },
    ]) {
      const before = exports
      b.ctx.layerfs.lfs.file.read = async () => response
      assert.deepEqual(await archive.store.readManifest('store/project', 'dsh.session.portable'), manifest)
      assert.equal(exports, before + 1)
    }
    assert.deepEqual(readdirSync(join(b.ctx.layerfs.root, 'archive-imports')), [])
  } finally { await b.close() }
})

test('permanent conversation owner blocks a recreated project while retaining local logs and metadata', async () => {
  const f = await fixture()
  try {
    const log = await f.p.create(header('retained'))
    const events = [{ seq: 0, time: 123, type: 'turn/start', data: { turn: 1 } }]
    await log.append(events); await log.flush()
    await f.p.migrateProject('retained', 'store/project')
    await f.p.updateSidecar('retained', (value) => { value.checkpoints = [{ commit: 'original-checkpoint' }] })
    const id = f.p.projectIdOf('retained')
    // Deleted and recreated under the same name: a new project without the UUID branch.
    f.branches.splice(0, f.branches.length, 'main')
    f.ctx.layerfs.lfs.project.get = async () => ({ project: 'store/project', store: 'store', name: 'project', kind: 'sqlite', machine: 'native', created: 200 })
    f.ctx.layerfs.lfs.project.list = async () => ({ items: [await f.ctx.layerfs.lfs.project.get()] })
    await assert.rejects(f.p.assertSessionIdentity('retained'), { code: 'ProjectIdentityChanged' })
    await assert.rejects(f.p.migrateProject('retained', 'store/project'), { code: 'ProjectIdentityChanged' })
    assert.equal(f.p.projectIdOf('retained'), id)
    assert.deepEqual((await log.read()).events, events)
    assert.equal((await f.p.readSidecar('retained')).checkpoints[0].commit, 'original-checkpoint')
    const rows = await f.p.catalogProjects()
    assert.equal(rows.find((row) => row.projectId === id).catalogState, 'identity-mismatch')
    await log.close()
  } finally { await f.close() }
})

test('machine changes do not change conversation identity or its local storage route', async () => {
  const f = await fixture()
  try {
    const log = await f.p.create(header('moved')); await log.flush()
    await f.p.migrateProject('moved', 'store/project')
    const before = (await f.p.stat('moved')).location.path
    const id = f.p.projectIdOf('moved')
    f.ctx.layerfs.lfs.project.get = async () => ({ project: 'store/project', store: 'store', name: 'project', kind: 'sqlite', machine: 'remote', created: 100 })
    assert.equal((await f.p.assertSessionIdentity('moved')).machine, 'remote')
    assert.equal(f.p.projectIdOf('moved'), id)
    assert.equal((await f.p.stat('moved')).location.path, before)
    await log.close()
  } finally { await f.close() }
})

test('a project carrying another UUID cannot import or reassign the conversation', async () => {
  const a = await fixture(), b = await fixture(), files = new Map()
  try {
    const log = await a.p.create(header('portable')); await log.flush()
    await a.p.migrateProject('portable', 'store/project')
    const original = archiveFixture(a, files)
    await original.store.mirror('portable')
    const restored = archiveFixture(b, files)
    // b's project at the same address claims its own UUID; the archive names a's.
    await assert.rejects(restored.store.import('store/project', 'portable'), { code: 'BindingRequired' })
    assert.equal(b.p.ownerOf('portable'), undefined)
    assert.equal(await b.p.local.stat('portable'), undefined)
    await log.close()
  } finally { await a.close(); await b.close() }
})

test('an archive changed by restore or another writer cannot overwrite locally pending history', async () => {
  const f = await fixture(), files = new Map()
  try {
    const log = await f.p.create(header('portable')); await log.flush()
    await f.p.migrateProject('portable', 'store/project')
    const archive = archiveFixture(f, files)
    await archive.store.mirror('portable')
    const manifest = JSON.parse(files.get('/.dsh/session/manifest.json'))
    const remoteHistory = files.get('/.dsh/session/session.jsonl')
    files.set('/.dsh/session/manifest.json', JSON.stringify({ ...manifest, digest: 'another-writer', count: 99 }))
    const event = { seq: 0, time: 123, type: 'turn/start', data: { turn: 1 } }
    await log.append([event]); await log.flush()
    await assert.rejects(archive.store.mirror('portable'), { code: 'ArchiveConflict' })
    assert.equal(files.get('/.dsh/session/session.jsonl'), remoteHistory)
    assert.deepEqual((await log.read()).events, [event])
    assert.match((await f.p.readSidecar('portable')).archive.error, /differs from the last confirmed save/)
    await log.close()
  } finally { await f.close() }
})


test('a failed first archive save is retried as a first save; another writer\'s archive still conflicts', async () => {
  const f = await fixture(), files = new Map()
  try {
    const log = await f.p.create(header('portable')); await log.flush()
    await f.p.migrateProject('portable', 'store/project')
    const archive = archiveFixture(f, files)
    const lfs = f.ctx.layerfs.lfs
    const refusal = (code) => Object.assign(new Error(code), { code })
    const archiveBranches = new Set()
    const fork = lfs.branch.fork
    lfs.branch.fork = async (project, name, from) => {
      if (!name.startsWith('dsh.session.')) return fork(project, name, from)
      if (archiveBranches.has(name)) throw refusal('Exists')
      archiveBranches.add(name); return {}
    }
    const read = lfs.file.read
    lfs.file.read = async (project, branch, path, options) => { if (!files.has(path)) throw refusal('NotFound'); return read(project, branch, path, options) }
    const apply = lfs.file.apply
    let failures = 1
    lfs.file.apply = async (...args) => { if (failures-- > 0) throw refusal('Unavailable'); return apply(...args) }
    await assert.rejects(archive.store.mirror('portable'), { code: 'Unavailable' })
    assert.equal(archive.state.mirror.portable.digest, undefined)
    assert.ok(archive.state.mirror.portable.error)
    // The branch exists without a manifest: the retry writes the first save.
    await archive.store.mirror('portable')
    const saved = archive.state.mirror.portable
    assert.ok(saved.digest); assert.equal(saved.error, undefined)
    assert.equal(JSON.parse(files.get('/.dsh/session/manifest.json')).digest, saved.digest)
    // An unconfirmed state never adopts an archive holding another save.
    const manifest = JSON.parse(files.get('/.dsh/session/manifest.json'))
    files.set('/.dsh/session/manifest.json', JSON.stringify({ ...manifest, digest: 'another-writer' }))
    archive.state.mirror.portable = { project: 'store/project', branch: saved.branch, count: 0, segments: [], error: 'earlier failure' }
    await assert.rejects(archive.store.mirror('portable'), { code: 'ArchiveConflict' })
    await log.close()
  } finally { await f.close() }
})

test('an owner recorded before project UUIDs keeps its history and is never adopted', async () => {
  const f = await fixture()
  try {
    const log = await f.p.create(header('legacy-owner')); await log.flush()
    await f.p.migrateProject('legacy-owner', 'store/project')
    await log.close()
    const ownersPath = join(f.dir, 'project-home', 'owners.json')
    const owners = JSON.parse(readFileSync(ownersPath, 'utf8'))
    const previousKey = owners.sessions['legacy-owner'].key
    const legacyKey = createHash('sha256').update(JSON.stringify([f.ctx.layerfs.home, 'store/project', 'store-id', '1'])).digest('hex')
    delete owners.sessions['legacy-owner'].identity
    owners.sessions['legacy-owner'].key = legacyKey
    const { renameSync } = await import('node:fs')
    renameSync(join(f.dir, 'project-home', 'projects', previousKey), join(f.dir, 'project-home', 'projects', legacyKey))
    writeFileSync(ownersPath, JSON.stringify(owners))
    const fresh = new persistence.LayerfsSessionPersistence(f.ctx.isolate('sessionPersistence'), f.p.stock)
    await assert.rejects(fresh.assertSessionIdentity('legacy-owner'), { code: 'ProjectIdentityChanged' })
    assert.equal(fresh.projectIdOf('legacy-owner'), legacyKey)
    assert.ok(await fresh.local.stat('legacy-owner'))
    assert.ok((await fresh.catalogProjects()).some((row) => row.projectId === legacyKey))
  } finally { await f.close() }
})

test('two DSH processes sharing a project-session home keep each other\'s owner routes', async () => {
  const f = await fixture()
  try {
    // B loads the shared index before A writes, as a second running DSH would.
    const b = new persistence.LayerfsSessionPersistence(f.ctx.isolate('sessionPersistence'), f.p.stock)
    assert.equal(b.ownerOf('one'), undefined)
    for (const [p, id] of [[f.p, 'one'], [b, 'two']]) {
      const log = await p.create(header(id)); await log.flush(); await log.close()
      await p.migrateProject(id, 'store/project')
    }
    const routes = JSON.parse(readFileSync(join(f.dir, 'project-home', 'owners.json'), 'utf8'))
    assert.deepEqual(Object.keys(routes.sessions).sort(), ['one', 'two'], 'the second writer merged, not replaced')
    assert.deepEqual(f.p.ownerOf('two'), { project: 'store/project' }, 'a replaced index is reloaded on read')
    assert.deepEqual(b.ownerOf('one'), { project: 'store/project' })
  } finally { await f.close() }
})

test('events appended while the owner is being chosen move with the session', async () => {
  const f = await fixture()
  try {
    const live = await f.p.create(header('racing'))
    const first = { seq: 0, time: 123, type: 'plugin:layerfs/test', ignorable: true, data: { n: 0 } }
    await live.append([first]); await live.flush()
    // The first turn starts right after migration flushed and copied the source.
    const late = { seq: 1, time: 124, type: 'turn/start', data: { turn: 1 } }
    const stock = f.p.stock, flush = stock.flush.bind(stock)
    stock.flush = async () => { await flush(); stock.flush = flush; await live.current.append([late]) }
    await f.p.migrateProject('racing', 'store/project')
    assert.match((await f.p.stat('racing')).location.path, /\/project-home\/projects\//)
    const reader = await f.p.open('racing', 'read')
    try { assert.deepEqual((await reader.read()).events.map((e) => e.type), ['plugin:layerfs/test', 'turn/start']) } finally { await reader.close() }
    await live.close()
  } finally { await f.close() }
})

test('Presets Reset deletion: log folder, owner route and sidecar go; a deletion record blocks implicit import', async () => {
  const f = await fixture()
  try {
    const kept = await f.p.create(header('kept'))
    await kept.flush(); await kept.close()
    for (const id of ['gone', 'loaded']) {
      const handle = await f.p.create(header(id))
      await handle.flush()
      await f.p.migrateProject(id, 'store/project')
      if (id === 'gone') await handle.close()
      else f.loaded = handle
    }
    await f.p.recordInvocation({ id: 'op', sessionId: 'gone', operation: 'file.write' })
    const goneDir = dirname((await f.p.stat('gone')).location.path)
    assert.ok(readdirSync(goneDir).includes('layerfs.json'))
    // Implicit import (open by id) would find these in the Store.
    const imports = []
    f.ctx.provide('layerfsSessions', { state: { read: () => ({ sessions: {}, pending: {} }) }, store: { locate: async () => 'store/project', import: async (project, id) => { imports.push(id); return { imported: false } } } })
    assert.deepEqual((await f.p.deleteSessions(['gone', 'loaded', 'never-owned'])).sort(), ['gone', 'loaded'])
    assert.throws(() => readdirSync(goneDir), /ENOENT/, 'the unpacked log folder and its sidecar are removed')
    assert.equal(f.p.ownerOf('gone'), undefined)
    assert.deepEqual(f.p.ownedSessionIds(), [])
    assert.deepEqual((await f.p.list()).map((s) => s.header.id), ['kept'], 'only the normal session is listed')
    assert.equal(await f.p.stat('gone'), undefined)
    assert.deepEqual(imports, [], 'a deleted session is not imported back implicitly')
    // The loaded session's writer was retired: later writes go nowhere.
    await f.loaded.append([{ type: 'message', data: {} }])
    await f.loaded.flush(); await f.loaded.close()
    assert.equal(await f.p.stat('loaded'), undefined)
    assert.ok(f.p.isDeleted('gone'))
    f.p.undelete('gone')
    assert.equal(f.p.isDeleted('gone'), false, 'an explicit open lifts the record')
    assert.equal((await f.p.stat('kept')).header.id, 'kept')
  } finally { await f.close() }
})


/** The paths one archive apply wrote or removed. */
const touched = (changes) => changes.filter((c) => c.op !== 'mkdir').map((c) => `${c.op} ${c.path.replace('/.dsh/session/', '')}`)

test('archive saves append only the added native rows and receipts; import rebuilds the exact log and sidecar', async () => {
  const a = await fixture(), b = await fixture(a.branches), files = new Map()
  try {
    const log = await a.p.create(header('portable')); await log.flush()
    await a.p.migrateProject('portable', 'store/project')
    const archive = archiveFixture(a, files)
    const turn = (seq) => ({ seq, time: 123 + seq, type: 'plugin:layerfs/test', ignorable: true, data: { payload: `row ${seq}` } })
    const receipt = (id) => ({ id, sessionId: 'portable', project: 'store/project', operation: 'file.write', outcome: 'success', versions: [] })
    await log.append([turn(0), turn(1)]); await log.flush()
    await a.p.recordInvocation(receipt('r0'))
    await archive.store.mirror('portable')
    assert.deepEqual(touched(archive.applied[0]), ['write session.jsonl', 'write invocations.jsonl', 'write layerfs.json', 'write manifest.json'])
    // Unchanged history writes nothing.
    await archive.store.mirror('portable')
    assert.equal(archive.applied.length, 1)
    await log.append([turn(2)]); await log.flush()
    await a.p.recordInvocation(receipt('r1'))
    await archive.store.mirror('portable')
    assert.deepEqual(touched(archive.applied[1]), ['write session.3.jsonl', 'write invocations.1.jsonl', 'write layerfs.json', 'write manifest.json'])
    assert.equal(files.get('/.dsh/session/session.3.jsonl').trim().split('\n').length, 1, 'a part holds only the added native row')
    assert.equal(JSON.parse(files.get('/.dsh/session/session.3.jsonl')).seq, 2)
    assert.equal(JSON.parse(files.get('/.dsh/session/layerfs.json')).invocations, undefined, 'receipts live in their own stream')
    await log.append([turn(3)]); await log.flush()
    await archive.store.mirror('portable')
    assert.deepEqual(touched(archive.applied[2]), ['write session.4.jsonl', 'write layerfs.json', 'write manifest.json'])
    const manifest = JSON.parse(files.get('/.dsh/session/manifest.json'))
    assert.deepEqual(manifest.log.map((p) => [p.file, p.from, p.to]), [['session.jsonl', 0, 3], ['session.3.jsonl', 3, 4], ['session.4.jsonl', 4, 5]])
    assert.equal(manifest.count, 4)
    const restored = archiveFixture(b, files)
    assert.equal((await restored.store.import('store/project', 'portable')).imported, true)
    const imported = await b.p.open('portable', 'read')
    assert.deepEqual((await imported.read()).events, (await log.read()).events)
    await imported.close()
    assert.deepEqual((await b.p.readSidecar('portable')).invocations.map((r) => r.id), ['r0', 'r1'])
    // The importer continues the same archive by appending.
    assert.deepEqual(restored.state.mirror.portable.streams, archive.state.mirror.portable.streams)
    // A tampered part fails the checksum; local history stays authoritative.
    files.set('/.dsh/session/session.4.jsonl', files.get('/.dsh/session/session.4.jsonl').replace('row 3', 'row X'))
    const third = await fixture(a.branches)
    try { await assert.rejects(archiveFixture(third, files).store.import('store/project', 'portable'), { code: 'InvalidInput' }) } finally { await third.close() }
    await log.close()
  } finally { await a.close(); await b.close() }
})

test('a changed archived prefix or too many parts rewrites the stream as one base file and removes its parts', async () => {
  const f = await fixture(), files = new Map()
  try {
    const log = await f.p.create(header('portable')); await log.flush()
    await f.p.migrateProject('portable', 'store/project')
    const archive = archiveFixture(f, files)
    const receipt = (id, outcome = 'success') => ({ id, sessionId: 'portable', project: 'store/project', operation: 'file.write', outcome, versions: [] })
    await f.p.recordInvocation(receipt('r0', 'unknown'))
    await archive.store.mirror('portable')
    await f.p.recordInvocation(receipt('r1'))
    await archive.store.mirror('portable')
    assert.ok(files.has('/.dsh/session/invocations.1.jsonl'))
    // A receipt settled in place changes the archived prefix.
    await f.p.recordInvocation(receipt('r0', 'success'))
    await archive.store.mirror('portable')
    assert.deepEqual(touched(archive.applied.at(-1)).filter((t) => t.includes('invocations')), ['rm invocations.1.jsonl', 'write invocations.jsonl'])
    assert.ok(!files.has('/.dsh/session/invocations.1.jsonl'))
    assert.deepEqual(JSON.parse(files.get('/.dsh/session/manifest.json')).invocations.map((p) => p.file), ['invocations.jsonl'])
    // Past PART_LIMIT parts the next save compacts.
    for (let seq = 0; seq <= persistence.PART_LIMIT; seq++) {
      await log.append([{ seq, time: 1, type: 'plugin:layerfs/test', ignorable: true, data: {} }]); await log.flush()
      await archive.store.mirror('portable')
    }
    const pieces = JSON.parse(files.get('/.dsh/session/manifest.json')).log
    assert.equal(pieces.length, 1, 'the parts were folded into the base')
    assert.equal(pieces[0].to, persistence.PART_LIMIT + 2)
    assert.deepEqual([...files.keys()].filter((p) => /session\.[0-9]+\.jsonl$/.test(p)), [])
    await log.close()
  } finally { await f.close() }
})

test('planStream appends while parts stay small and rewrites once they outgrow the base', () => {
  const lines = (n, size = 10) => Array.from({ length: n }, (_, i) => `${i}`.padEnd(size, 'x'))
  const first = persistence.planStream('session', lines(2), undefined, undefined)
  assert.deepEqual(first.pieces.map((p) => p.file), ['session.jsonl'])
  const grown = persistence.planStream('session', lines(3), first.mark, first.pieces)
  assert.deepEqual(grown.changes.map((c) => c.op + ' ' + c.path), ['write /.dsh/session/session.2.jsonl'])
  assert.equal(grown.changes[0].content, lines(3)[2] + '\n')
  // Parts larger than both the base and the floor force a rewrite.
  const big = lines(3 + 2, persistence.COMPACT_FLOOR)
  const rewritten = persistence.planStream('session', [...lines(3), ...big.slice(3)], grown.mark, grown.pieces)
  assert.deepEqual(rewritten.changes.map((c) => c.op + ' ' + c.path), ['rm /.dsh/session/session.2.jsonl', 'write /.dsh/session/session.jsonl'])
  // A different prefix never appends.
  const changed = persistence.planStream('session', ['other', ...lines(3).slice(1)], grown.mark, grown.pieces)
  assert.equal(changed.pieces.length, 1)
})

test('saves to one Store run one at a time and a session already queued is not queued twice', async () => {
  const lanes = new Map()
  const lane = (key, run) => { const next = (lanes.get(key) ?? Promise.resolve()).then(run, run); lanes.set(key, next.catch(() => {})); return next }
  const bindings = { a: { project: 'one/p' }, b: { project: 'one/q' }, c: { project: 'two/p' } }
  const store = new persistence.SessionStore(new Context(), { binding: (id) => bindings[id], lane })
  let running = 0, peak = 0
  const runs = []
  const release = []
  store.mirrorNow = async (id) => { runs.push(id); running++; peak = Math.max(peak, running); await new Promise((r) => release.push(r)); running-- }
  const first = store.mirror('a')
  assert.equal(store.mirror('a'), first, 'a save that has not started is shared')
  await new Promise((r) => setImmediate(r))
  // Running saves do not absorb later changes: the next request queues anew, once.
  const queuedA = store.mirror('a')
  assert.notEqual(queuedA, first)
  assert.equal(store.mirror('a'), queuedA)
  const b = store.mirror('b')
  const c = store.mirror('c')
  await new Promise((r) => setImmediate(r))
  assert.deepEqual(runs, ['a', 'c'], 'Store one runs a single save; Store two is independent')
  release.shift()(); await first
  await new Promise((r) => setImmediate(r))
  assert.deepEqual(runs, ['a', 'c', 'a'])
  while (release.length) { release.shift()(); await new Promise((r) => setImmediate(r)) }
  await Promise.all([queuedA, b, c])
  assert.deepEqual(runs.sort(), ['a', 'a', 'b', 'c'])
  assert.equal(peak, 2)
})

test('discover rereads a stored session manifest only when its Branch head moves, and shares concurrent walks (F22)', async () => {
  const store = new persistence.SessionStore(new Context(), { binding: () => undefined, lane: (_k, run) => run() })
  const heads = { 'dsh.session.a': 'c1', 'dsh.session.b': 'c2' }
  let lists = 0
  const reads = []
  const manifest = (name) => JSON.stringify({ sessionId: name.slice('dsh.session.'.length), projectId: 'p', title: `${name} @ ${heads[name]}`, count: 3, checkpoints: [] })
  const fake = {
    branch: {
      // 2.0 `branch list --prefix`: rows `{ branch, commit }`
      list: async (_p, o = {}) => {
        lists++
        await new Promise((r) => setTimeout(r, 5))
        const items = [{ branch: 'main', commit: 'm1' }, ...Object.keys(heads).map((branch) => ({ branch, commit: heads[branch] }))]
        return { items: items.filter((b) => !o.prefix || b.branch.startsWith(o.prefix)) }
      },
    },
    file: {
      read: async (_project, branch) => {
        reads.push(branch)
        const text = manifest(branch) + '\n'
        return { text, next_line: null, line_truncated: false, binary: false, size: Buffer.byteLength(text), start_line: 1, end_line: 1 }
      },
    },
  }
  Object.defineProperty(store, 'lfs', { get: () => fake })
  const first = await store.discover('s/p')
  assert.deepEqual(first.map((r) => r.sessionId).sort(), ['a', 'b'])
  assert.deepEqual(reads.sort(), ['dsh.session.a', 'dsh.session.b'])
  // Unchanged heads: the next walk lists Branches but reads no manifest.
  reads.length = 0
  const again = await store.discover('s/p')
  assert.deepEqual(reads, [], 'no manifest is reread while its Branch head is unchanged')
  assert.deepEqual(again.map((r) => r.title).sort(), first.map((r) => r.title).sort())
  // A save moves one head: only that manifest is read again, and its new contents show.
  heads['dsh.session.b'] = 'c3'
  const moved = await store.discover('s/p')
  assert.deepEqual(reads, ['dsh.session.b'])
  assert.equal(moved.find((r) => r.sessionId === 'b').title, 'dsh.session.b @ c3')
  // Concurrent callers share one walk (one Branch list).
  lists = 0
  const [x, y] = await Promise.all([store.discover('s/p'), store.discover('s/p')])
  assert.equal(lists, 1, 'concurrent discovery of one project lists its Branches once')
  assert.equal(x, y)
  // A Branch gone from the project leaves nothing kept.
  delete heads['dsh.session.a']
  await store.discover('s/p')
  assert.equal([...store.manifests.keys()].some((k) => k.endsWith('dsh.session.a')), false)
})
