// The Files views' page-side reads (layerfs-project#178 "Files view"):
// src/client/machines/inspect-files.ts caps the tab's own files reads at two,
// drops a read nobody wants any more (never sent while it waits; the host told
// when it was), and turns pasted text and listed entries into paths and rows.
// Also the place a Files tab was left at (src/client/storage/tabs.ts).
//   node --test test/inspect-files-client.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtempSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

const root = fileURLToPath(new URL('..', import.meta.url))
const out = mkdtempSync(join(root, 'lib', '.test-inspect-files-client-'))
const bundle = async (name, contents) => {
  await build({ absWorkingDir: root, stdin: { contents, resolveDir: root, loader: 'ts' }, outfile: join(out, `${name}.mjs`), bundle: true, format: 'esm', platform: 'node', target: 'node24', external: ['@deepseek-ai/*', 'react', 'react/*'], logLevel: 'warning' })
  return import(pathToFileURL(join(out, `${name}.mjs`)).href)
}
const F = await bundle('files', "export * from './src/client/machines/inspect-files.ts'")
test.after(() => rmSync(out, { recursive: true, force: true }))

const tick = () => new Promise((r) => setImmediate(r))
/** A task whose end the test decides. */
function gate() {
  let open, fail
  const promise = new Promise((resolve, reject) => { open = resolve; fail = reject })
  return { promise, open, fail, started: 0, task() { this.started++; return promise } }
}

test('ReadQueue runs two reads at a time and the rest in order', async () => {
  const queue = new F.ReadQueue()
  assert.equal(queue.limit, 2)
  assert.equal(F.READS_AT_ONCE, 2)
  const gates = [gate(), gate(), gate(), gate()]
  const results = gates.map((g, i) => queue.run(() => g.task()).then((v) => `${i}:${v}`))
  assert.deepEqual(gates.map((g) => g.started), [1, 1, 0, 0])
  assert.deepEqual([queue.running, queue.queued], [2, 2])
  gates[1].open('b')
  await tick()
  assert.deepEqual(gates.map((g) => g.started), [1, 1, 1, 0], 'the third starts when a slot is free')
  gates[0].open('a'); gates[2].open('c'); gates[3].open('d')
  assert.deepEqual(await Promise.all(results), ['0:a', '1:b', '2:c', '3:d'])
  assert.deepEqual([queue.running, queue.queued], [0, 0])
})

test('ReadQueue: a failed or throwing task frees its slot', async () => {
  const queue = new F.ReadQueue(1)
  const a = gate(), b = gate()
  const first = queue.run(() => a.task()), second = queue.run(() => b.task())
  a.fail(new Error('refused'))
  await assert.rejects(first, /refused/)
  await tick()
  assert.equal(b.started, 1)
  b.open('ok')
  assert.equal(await second, 'ok')
  await assert.rejects(queue.run(() => { throw new Error('sync') }), /sync/)
  assert.equal(queue.running, 0)
  assert.equal(await queue.run(async () => 'after'), 'after')
})

test('ReadQueue: a waiting read whose signal aborts never starts', async () => {
  const queue = new F.ReadQueue()
  const a = gate(), b = gate(), c = gate(), d = gate()
  queue.run(() => a.task()); queue.run(() => b.task())
  const stop = new AbortController()
  const third = queue.run(() => c.task(), stop.signal)
  const fourth = queue.run(() => d.task())
  assert.equal(queue.queued, 2)
  stop.abort()
  await assert.rejects(third, (e) => e instanceof F.Dropped && e.code === 'Cancelled' && F.isDropped(e))
  assert.equal(queue.queued, 1)
  a.open(); b.open()
  await tick()
  assert.equal(c.started, 0, 'the dropped read was never started')
  assert.equal(d.started, 1, 'the read behind it took the slot')
  d.open('d')
  assert.equal(await fourth, 'd')
  // aborted before it is asked for: rejected at once, nothing queued
  const gone = new AbortController(); gone.abort()
  const e = gate()
  await assert.rejects(queue.run(() => e.task(), gone.signal), F.isDropped)
  assert.equal(e.started, 0)
})

/** An api whose GETs wait until the test answers them. */
function fakeApi() {
  const calls = [], ops = []
  return {
    calls, ops,
    call(method, path) { return new Promise((resolve, reject) => calls.push({ method, path, resolve, reject, rid: new URL(path, 'http://x').searchParams.get('rid') })) },
    op(name, args) { ops.push({ name, args }); return Promise.resolve({}) },
  }
}
const folder = (path, entries = []) => ({ kind: 'folder', path, entries, next: null, hasMore: false, reason: null, observedAt: 1, stale: false })

test('readFiles: the route, the query and the answer', async () => {
  const api = fakeApi(), queue = new F.ReadQueue()
  assert.equal(F.filesRoute('machine', 'my vm'), '/api/machines/my%20vm/files')
  assert.equal(F.filesRoute('sandbox', 'sb'), '/api/sandboxes/sb/files')
  const roots = F.readFiles(api, queue, '/api/sandboxes/sb/files', {})
  const page = F.readFiles(api, queue, '/api/sandboxes/sb/files', { path: '/a b/c', cursor: '/a b/c/x', depth: 1, limit: 200 })
  assert.equal(api.calls.length, 2)
  const q0 = new URL(api.calls[0].path, 'http://x').searchParams, q1 = new URL(api.calls[1].path, 'http://x').searchParams
  assert.deepEqual([...q0.keys()], ['rid'], 'the roots are asked without a path')
  assert.match(q0.get('rid'), /^files-/)
  assert.deepEqual([q1.get('path'), q1.get('cursor'), q1.get('depth'), q1.get('limit')], ['/a b/c', '/a b/c/x', '1', '200'])
  assert.notEqual(q0.get('rid'), q1.get('rid'), 'each read has its own id')
  api.calls[0].resolve(folder(null)); api.calls[1].resolve(folder('/a b/c'))
  assert.equal((await roots).path, null)
  assert.equal((await page).path, '/a b/c')
  // a file window by offset
  const win = F.readFiles(api, queue, '/api/machines/m/files', { path: '/f', offset: 262144, length: 1024 })
  const q2 = new URL(api.calls[2].path, 'http://x').searchParams
  assert.deepEqual([q2.get('offset'), q2.get('length')], ['262144', '1024'])
  api.calls[2].resolve({ kind: 'file', path: '/f', size: 9, offset: 262144, content: '', binary: false, next: null, observedAt: 1, stale: false })
  assert.equal((await win).kind, 'file')
  assert.equal(api.ops.length, 0, 'nothing is cancelled when reads answer')
})

test('readFiles: a tab sends two reads at a time; a third waits in the page', async () => {
  const api = fakeApi(), queue = new F.ReadQueue()
  const reads = ['/a', '/b', '/c', '/d'].map((path) => F.readFiles(api, queue, '/api/machines/m/files', { path }))
  assert.equal(api.calls.length, 2, 'two requests are in flight')
  api.calls[0].resolve(folder('/a'))
  await reads[0]
  await tick()
  assert.equal(api.calls.length, 3)
  assert.equal(new URL(api.calls[2].path, 'http://x').searchParams.get('path'), '/c', 'in the order asked')
  api.calls[1].resolve(folder('/b')); api.calls[2].resolve(folder('/c'))
  await tick(); await tick()
  api.calls[3].resolve(folder('/d'))
  assert.deepEqual((await Promise.all(reads)).map((v) => v.path), ['/a', '/b', '/c', '/d'])
})

test('readFiles: a waiting read that is dropped is never sent, and the host is not told', async () => {
  const api = fakeApi(), queue = new F.ReadQueue()
  F.readFiles(api, queue, '/r', { path: '/a' }); F.readFiles(api, queue, '/r', { path: '/b' })
  const stop = new AbortController()
  const waiting = F.readFiles(api, queue, '/r', { path: '/collapsed' }, stop.signal)
  stop.abort()
  await assert.rejects(waiting, F.isDropped)
  api.calls[0].resolve(folder('/a')); api.calls[1].resolve(folder('/b'))
  await tick(); await tick()
  assert.equal(api.calls.length, 2, 'no request for the collapsed folder')
  assert.equal(api.ops.length, 0, 'and no inspect.cancel: the host never heard of it')
})

test('readFiles: a read in flight that is dropped tells the host, frees its slot and ignores the late answer', async () => {
  const api = fakeApi(), queue = new F.ReadQueue()
  const stop = new AbortController()
  const sent = F.readFiles(api, queue, '/r', { path: '/a' }, stop.signal)
  F.readFiles(api, queue, '/r', { path: '/b' })
  const next = F.readFiles(api, queue, '/r', { path: '/c' })
  assert.equal(api.calls.length, 2)
  stop.abort()
  await assert.rejects(sent, F.isDropped)
  assert.deepEqual(api.ops, [{ name: 'inspect.cancel', args: { rid: api.calls[0].rid } }], 'inspect.cancel names the read\'s own id')
  await tick()
  assert.equal(api.calls.length, 3, 'the waiting read starts at once')
  // the host answers the dropped request `Cancelled` (409): nobody sees it
  api.calls[0].reject(Object.assign(new Error('cancelled'), { code: 'Cancelled', status: 409 }))
  api.calls[2].resolve(folder('/c'))
  assert.equal((await next).path, '/c')
  assert.equal(api.ops.length, 1)
})

test('readFiles: a host failure is passed on; the host\'s own Cancelled reads as dropped', async () => {
  const api = fakeApi(), queue = new F.ReadQueue()
  const failed = F.readFiles(api, queue, '/r', { path: '/a' })
  api.calls[0].reject(Object.assign(new Error('no route'), { code: 'Unsupported', status: 501 }))
  await assert.rejects(failed, (e) => e.code === 'Unsupported' && !F.isDropped(e))
  const cancelled = F.readFiles(api, queue, '/r', { path: '/b' })
  api.calls[1].reject(Object.assign(new Error('cancelled'), { code: 'Cancelled', status: 409 }))
  await assert.rejects(cancelled, (e) => e instanceof F.Dropped)
  // a refusal is an answer, not a failure
  const refused = F.readFiles(api, queue, '/r', { path: '/c' })
  api.calls[2].resolve({ kind: 'refused', path: '/c', observedAt: 1, stale: { at: null, code: 'Busy', message: 'two reads are running' } })
  const view = await refused
  assert.equal(view.kind, 'refused')
  const error = F.refusalError(view.stale)
  assert.deepEqual([error.code, error.message], ['Busy', 'two reads are running'])
})

test('pastedPath: a path as a terminal prints it', () => {
  assert.equal(F.pastedPath('/etc/hosts'), '/etc/hosts')
  assert.equal(F.pastedPath('  /etc/hosts \n'), '/etc/hosts')
  assert.equal(F.pastedPath('"/var/my files/a.txt"'), '/var/my files/a.txt')
  assert.equal(F.pastedPath('\'/var/x\''), '/var/x')
  assert.equal(F.pastedPath('/var/my\\ files/a.txt'), '/var/my files/a.txt')
  assert.equal(F.pastedPath('/var//log///x/'), '/var/log/x')
  assert.equal(F.pastedPath('/'), '/')
  // `..` is LayerFS's to refuse (InvalidInput): it is passed as typed
  assert.equal(F.pastedPath('/a/../b'), '/a/../b')
  for (const bad of ['', '   ', 'etc/hosts', './x', '~/x', 'C:\\x', '/a\nb', '/a\0b', `/${'x'.repeat(5000)}`]) assert.equal(F.pastedPath(bad), null, JSON.stringify(bad.slice(0, 20)))
})

test('paths: parent, name, under, breadcrumb', () => {
  assert.deepEqual(['/a/b', '/a', '/', 'rel'].map(F.parentOf), ['/a', '/', null, null])
  assert.deepEqual(['/a/b.txt', '/a', '/'].map(F.baseName), ['b.txt', 'a', '/'])
  assert.equal(F.isUnder('/a/b', '/a'), true)
  assert.equal(F.isUnder('/a', '/a'), true)
  assert.equal(F.isUnder('/ab', '/a'), false)
  assert.equal(F.isUnder('/x', '/'), true)
  assert.deepEqual(F.crumbsOf('/a/b/c', '/a'), [{ name: 'b', path: '/a/b' }, { name: 'c', path: '/a/b/c' }])
  assert.deepEqual(F.crumbsOf('/a/b', null), [{ name: '/', path: '/' }, { name: 'a', path: '/a' }, { name: 'b', path: '/a/b' }])
})

test('rows: entries keep the server\'s order and LayerFS\'s kind; deeper entries go under their folder', () => {
  const e = (path, kind, depth = 1, more = {}) => ({ path, depth, kind, size: null, mode: null, mtime: null, ...more })
  const groups = F.groupEntries([e('/r/b', 'folder'), e('/r/b/x', 'file', 2), e('/r/b/y', 'restricted', 2), e('/r/a', 'file'), e('/r/z', 'socket')], '/r')
  assert.deepEqual(groups.get('/r').map((r) => [r.name, r.kind, r.raw]), [['b', 'dir', 'folder'], ['a', 'file', 'file'], ['z', 'socket', 'socket']], 'not sorted: the server ranks')
  assert.deepEqual(groups.get('/r/b').map((r) => r.name), ['x', 'y'])
  // the roots are one flat list, whatever their paths
  const roots = F.groupEntries([e('/layerfs/data', 'folder', 1, { role: 'data' }), e('/layerfs/data/store', 'folder', 1, { role: 'stores' }), e('/', 'folder', 1, { role: 'machine' })], F.ROOTS)
  assert.equal(roots.get(F.ROOTS).length, 3)
  assert.equal(roots.size, 1)
  assert.equal(F.groupEntries([], '/empty').get('/empty').length, 0)
  assert.deepEqual(['folder', 'file', 'restricted', 'unreadable', 'system', 'mount', 'symlink', 'fifo', 'other', 'something-new'].map(F.actOf), ['open', 'read', 'restricted', 'unreadable', 'system', 'mount', 'symlink', 'other', 'other', 'other'])
  assert.equal(F.wholeOf([e('/layerfs/data', 'folder', 1, { role: 'data' }), e('/', 'folder', 1, { role: 'machine' })]).role, 'machine')
  assert.equal(F.wholeOf([e('/', 'folder', 1, { role: 'root' })]).role, 'root')
  assert.equal(F.wholeOf([e('/home', 'folder', 1, { role: 'home' })]), undefined, 'no / offered: no second view')
  assert.equal(F.wholeOf([e('/', 'folder', 1, { role: 'data' })]), undefined, 'a root that happens to be / is not the Whole filesystem view')
})

test('mtime in seconds or ms; a wait is shown from one second on', () => {
  assert.equal(F.mtimeMs(1794774161), 1794774161000)
  assert.equal(F.mtimeMs(1794774161000), 1794774161000)
  assert.deepEqual([null, undefined, 0, -1].map(F.mtimeMs), [null, null, null, null])
  assert.equal(F.waitedSeconds(1000, 1999), null)
  assert.equal(F.waitedSeconds(1000, 2000), 1)
  assert.equal(F.waitedSeconds(1000, 4700), 3)
})

test('the place a Files tab was left at: per machine and per Sandbox, and safe without storage', async () => {
  const store = new Map()
  globalThis.localStorage = { getItem: (k) => store.get(k) ?? null, setItem: (k, v) => { store.set(k, String(v)) }, removeItem: (k) => { store.delete(k) } }
  const T = await bundle('tabs', "export * from './src/client/storage/tabs.ts'")
  try {
    assert.equal(T.readFilesPlace('machine', 'docker'), null)
    T.writeFilesPlace('machine', 'docker', { view: 'whole', path: '/etc' })
    T.writeFilesPlace('sandbox', 'docker', { view: 'layerfs', path: null })
    assert.deepEqual(T.readFilesPlace('machine', 'docker'), { view: 'whole', path: '/etc' })
    assert.deepEqual(T.readFilesPlace('sandbox', 'docker'), { view: 'layerfs', path: null }, 'a Sandbox and a machine of one name are two places')
    assert.equal(T.readFilesPlace('machine', 'other'), null)
    // a link tells the open tab; an ordinary save does not
    const told = []
    const off = T.watchFilesPlace('machine', 'docker', (place) => told.push(place))
    T.writeFilesPlace('machine', 'docker', { view: 'whole', path: '/var' })
    assert.deepEqual(told, [])
    T.writeFilesPlace('machine', 'docker', { view: 'whole', path: '/var/log' }, true)
    T.writeFilesPlace('machine', 'elsewhere', { view: 'whole', path: '/x' }, true)
    assert.deepEqual(told, [{ view: 'whole', path: '/var/log' }])
    off()
    T.writeFilesPlace('machine', 'docker', { view: 'layerfs', path: null }, true)
    assert.equal(told.length, 1)
    // what the app wrote is kept in the app; what another load left in storage is read, and not trusted
    assert.deepEqual(JSON.parse(store.get('dsh-layerfs:inspect-files'))['machine:docker'], { view: 'layerfs', path: null })
    store.set('dsh-layerfs:inspect-files', '{"machine:left":{"view":"whole","path":"/var/log"},"machine:odd":{"view":"sideways","path":"/x"},"machine:rel":{"view":"whole","path":"etc"}}')
    assert.deepEqual(T.readFilesPlace('machine', 'left'), { view: 'whole', path: '/var/log' })
    assert.equal(T.readFilesPlace('machine', 'odd'), null, 'an unknown view is no place')
    assert.deepEqual(T.readFilesPlace('machine', 'rel'), { view: 'whole', path: null }, 'a path that is not absolute is dropped')
    store.set('dsh-layerfs:inspect-files', 'not json')
    assert.equal(T.readFilesPlace('machine', 'left'), null)
    // storage that throws (a private window): the place is kept while the app is open, and nothing breaks
    globalThis.localStorage = { getItem() { throw new Error('blocked') }, setItem() { throw new Error('blocked') }, removeItem() { throw new Error('blocked') } }
    assert.equal(T.readFilesPlace('machine', 'private'), null)
    assert.doesNotThrow(() => T.writeFilesPlace('machine', 'private', { view: 'whole', path: '/etc' }, true))
    assert.deepEqual(T.readFilesPlace('machine', 'private'), { view: 'whole', path: '/etc' })
  } finally { delete globalThis.localStorage }
})

test('rows of the real answers: a LayerFS folder met where it is not entered, a worker log, the Sandbox of a run folder', () => {
  const e = (path, kind, more = {}) => ({ path, depth: 1, kind, size: null, mode: null, mtime: null, ...more })
  // from `/`: a role makes a folder or a mount a link to its own path; among the LayerFS folders only a mount is one
  assert.equal(F.isLinkedRole(e('/layerfs/bundle', 'folder', { role: 'bundle' }), 'whole'), true)
  assert.equal(F.isLinkedRole(e('/layerfs/data', 'mount', { role: 'data', fs: 'ext4' }), 'whole'), true)
  assert.equal(F.isLinkedRole(e('/layerfs/data', 'mount', { role: 'data', fs: 'ext4' }), 'layerfs'), true)
  assert.equal(F.isLinkedRole(e('/layerfs/data/log', 'folder', { role: 'logs' }), 'layerfs'), false, 'a LayerFS folder inside another is opened in place')
  assert.equal(F.isLinkedRole(e('/etc', 'folder'), 'whole'), false)
  assert.equal(F.isLinkedRole(e('/mnt', 'mount', { fs: 'virtiofs' }), 'whole'), false)
  // a Workspace, a log and `/` itself have rows or views of their own
  assert.equal(F.isLinkedRole(e('/run/ws/1', 'mount', { role: 'workspace', workspace: 'w' }), 'whole'), false)
  assert.equal(F.isLinkedRole(e('/layerfs/data/log/worker.log', 'file', { role: 'log' }), 'whole'), false)
  assert.equal(F.isLinkedRole(e('/', 'folder', { role: 'machine' }), 'whole'), false)
  assert.deepEqual(['/layerfs/data/log/worker.log', '/layerfs/data/log/worker.1.log', '/x/daemon.log', '/x/worker.log.txt', '/x/myworker.log'].map(F.isWorkerLog), [true, true, false, false, false])
  assert.deepEqual(['layerfs-b2c56068-bd', 'lfs-0a1b2c3d-shop', 'layerfs-b2c56068-a-b', 'images', 'layerfs-xyz-bd', 'layerfs-b2c56068-'].map(F.sandboxOfRunFolder), ['bd', 'shop', 'a-b', null, null, null])
  // the data root's free space comes as LayerFS names it
  assert.equal(F.wholeOf([e('/layerfs/data', 'folder', { role: 'data', free_bytes: 5 }), e('/', 'folder', { role: 'machine' })]).role, 'machine')
  assert.equal(F.wholeOf([e('/home', 'folder', { role: 'home' }), e('/', 'folder', { role: 'root' })]).role, 'root')
  assert.equal(F.wholeOf([e('/layerfs/data', 'folder', { role: 'data' })]), undefined, 'no `/` in the roots: no second view')
})

test('a Workspace is read from its saved files: its Branch at its base, under the mount path', async () => {
  const root = '/run/sb/workspaces/4'
  const source = { root, project: 's/p', branch: 'care-001', base: 'c0ffee' }
  const calls = []
  const api = {
    call: async (method, path) => {
      calls.push(path)
      const u = new URL(path, 'http://x')
      if (u.pathname.endsWith('/fs')) {
        if (u.searchParams.get('path') === '/README.md') throw Object.assign(new Error('not a folder'), { code: 'InvalidInput' })
        return u.searchParams.get('path') === '/'
          ? { items: [{ name: 'src', path: '/src', kind: 'dir', size: null }, { name: 'README.md', path: '/README.md', kind: 'file', size: 12 }, { name: 'l', path: '/l', kind: 'symlink', size: null }], next: 2 }
          : { items: [{ name: 'a.ts', path: '/src/a.ts', kind: 'file', size: 3 }], next: null }
      }
      return { path: u.searchParams.get('path'), size: 12, binary: false, text: 'hello\n', next_line: Number(u.searchParams.get('line')) === 1 ? 2 : null }
    },
  }
  const queue = new F.ReadQueue()
  const top = await F.readWorkspace(api, queue, source, { path: root, depth: 1 })
  assert.equal(top.kind, 'folder')
  assert.deepEqual(top.entries.map((e) => [e.path, e.kind, e.depth]), [[`${root}/src`, 'folder', 1], [`${root}/README.md`, 'file', 1], [`${root}/l`, 'symlink', 1]])
  assert.equal(top.next, '2')
  assert.match(calls[0], /branch=care-001/)
  assert.match(calls[0], /pin=c0ffee/)
  assert.match(calls[0], /\/api\/projects\/s%2Fp\/fs/)
  const page = await F.readWorkspace(api, queue, source, { path: root, cursor: '2' })
  assert.match(calls.at(-1), /pos=2/)
  assert.equal(page.kind, 'folder')
  const sub = await F.readWorkspace(api, queue, source, { path: `${root}/src`, depth: 1 })
  assert.deepEqual(sub.entries.map((e) => e.path), [`${root}/src/a.ts`])
  const file = await F.readWorkspace(api, queue, source, { path: `${root}/README.md`, offset: 1 })
  assert.deepEqual([file.kind, file.content, file.offset, file.next], ['file', 'hello\n', 1, 2])
  // Go to path on a file: not a folder, so read as one
  const gone = await F.readWorkspace(api, queue, source, { path: `${root}/README.md`, depth: 1 })
  assert.equal(gone.kind, 'file')
  assert.equal(F.readKey(F.WS_ROOTS), F.ROOTS)
  assert.equal(F.readKey(F.SB_ROOTS), F.ROOTS)
  assert.equal(F.isWorkspaceEntry({ path: root, depth: 1, kind: 'mount', size: null, mode: null, mtime: null, role: 'workspace' }), true)
  assert.equal(F.isWorkspaceEntry({ path: '/tmp', depth: 1, kind: 'folder', size: null, mode: null, mtime: null, role: 'home' }), false)
})
