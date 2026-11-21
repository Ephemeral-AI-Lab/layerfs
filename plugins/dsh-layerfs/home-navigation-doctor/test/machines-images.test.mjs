// New Sandbox's Image menu rules and the background create rows (docs/specs/sandbox-images.md §5):
// imagePlan (button label and note line), the preselected row (D7), the ready group's footer,
// a create's phase in words and its row's group and Store,
// and the Machines controller's `creations` source (admission, sandbox-progress, done, retry, dismiss).
//   node --test test/machines-images.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtempSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

const root = fileURLToPath(new URL('..', import.meta.url))
const out = mkdtempSync(join(root, 'lib', '.test-machines-images-'))
await build({
  absWorkingDir: root,
  stdin: { contents: "export * from './src/client/machines/images.ts'; export { MachinesController } from './src/client/machines/controller.ts'", resolveDir: root, loader: 'ts' },
  outfile: join(out, 'images.mjs'), bundle: true, format: 'esm', platform: 'node', target: 'node24', logLevel: 'warning',
})
const { displayRef, imagesStatus, imagePlan, imagesFooter, defaultPick, findPick, pickOf, phaseWords, phaseShare, reasonWords, sandboxGroups, creationStore, agoWords, dockerTag, usedByWords, imagesTotal, freeSpaceRows, freesWords, failureWords, needsSpace, removable, creatingOn, besideCreating, MachinesController } = await import(pathToFileURL(join(out, 'images.mjs')).href)
test.after(() => rmSync(out, { recursive: true, force: true }))

const D = (n) => `sha256:${String(n).repeat(64).slice(0, 64)}`
const row = (id, extra = {}) => ({ id, name: `img-${id.slice(7, 9)}`, refs: [`img-${id.slice(7, 9)}:1`], size: null, source: 'machine', sandboxes: 0, lastUsed: null, usable: true, ...extra })
const answer = (extra = {}) => ({
  machine: 'lima1', platform: 'lima', state: 'running', stage: 0, mode: 'images', architecture: 'arm64', free: null,
  default: null, observedAt: 0, stale: false, ready: [], docker: null, ...extra,
})

test('default selection: newest last_used, then the machine default, then the first usable; never an unusable row', () => {
  const a = row(D(1)), b = row(D(2)), c = row(D(3), { usable: false, reason: { kind: 'no-glibc' } })
  assert.equal(defaultPick(answer({ ready: [c, a, b] })).id, a.id, 'first usable when nothing else says')
  assert.equal(defaultPick(answer({ ready: [a, b], default: b.id })).id, b.id, 'the machine default')
  assert.equal(defaultPick(answer({ ready: [a, b, c], default: c.id })).id, a.id, 'an unusable default is passed over')
  const used = answer({ ready: [{ ...a, lastUsed: '2026-11-01T00:00:00Z' }, { ...b, lastUsed: '2026-11-10T00:00:00Z' }], default: a.id })
  assert.equal(defaultPick(used).id, b.id, 'the newest last_used wins over the default')
  assert.equal(defaultPick(answer({ ready: [c] })), null, 'nothing usable: LayerFS default (no image named)')
  assert.equal(defaultPick(answer({ mode: 'host' })), null, 'macOS native has no image')
  assert.equal(defaultPick(null), null)
  const docker = { available: true, images: [{ id: D(4), name: 'pg:17', refs: ['pg:17'], size: 1, architecture: 'arm64', os: 'linux', need: 3, fits: true, usable: true }] }
  assert.equal(defaultPick(answer({ ready: [], docker })), null, 'never a Docker row unless picked')
  assert.equal(findPick(answer({ ready: [a], docker }), D(4)).from, 'docker')
  assert.equal(findPick(answer({ ready: [a] }), 'gone'), null)
})

test('the pick is a row id plus display fields; the bundle image is marked so it is named in the locale', () => {
  const p = pickOf(row(D(1), { source: 'bundle', name: 'LayerFS default', refs: [] }), 'ready')
  assert.deepEqual(p, { id: D(1), ref: null, name: 'LayerFS default', from: 'ready', size: null, usable: true, bundle: true })
})

test('imagePlan: label and note for ready, stopped, native, no answer, and the Stage 1 Docker cases', () => {
  const a = row(D(1))
  const ready = answer({ ready: [a] })
  assert.deepEqual(imagePlan(ready, pickOf(a, 'ready')), { label: 'create', note: { key: 'noteReady', values: { image: 'img-11:1', machine: 'lima1' } } })
  const bundle = row(D(2), { source: 'bundle', refs: [] })
  assert.deepEqual(imagePlan(answer({ ready: [bundle] }), pickOf(bundle, 'ready')).note, { key: 'noteReadyDefault', values: { machine: 'lima1' } })
  const stopped = imagePlan(answer({ state: 'stopped', ready: [a], stale: { at: 1, code: 'Unavailable' } }), pickOf(a, 'ready'))
  assert.deepEqual(stopped, { label: 'start', note: { key: 'noteStopped', values: { machine: 'lima1' } } })
  assert.deepEqual(imagePlan(answer({ mode: 'host', machine: 'native' }), null), { label: 'create', note: { key: 'nativeOwnSystem' } })
  // the answer not read yet (or failed): the machine list's state decides the button, no note
  assert.deepEqual(imagePlan(null, null, 'stopped'), { label: 'start', note: null })
  assert.deepEqual(imagePlan(undefined, null, 'running'), { label: 'create', note: null })
  assert.deepEqual(imagePlan(ready, null), { label: 'create', note: null }, 'LayerFS default: nothing to say')

  const pg = { id: D(4), name: 'postgres:17', refs: ['postgres:17'], size: 412e6, architecture: 'arm64', os: 'linux', need: 865.2e6, fits: true, usable: true }
  const s1 = answer({ stage: 1, free: 5e9, ready: [a], docker: { available: true, images: [pg] } })
  assert.deepEqual(imagePlan(s1, pickOf(pg, 'docker')), { label: 'create', note: { key: 'noteCopy', values: { image: 'postgres:17', size: '412 MB', machine: 'lima1' } } })
  const full = answer({ stage: 1, free: 4e8, state: 'running', docker: { available: true, images: [{ ...pg, fits: false }] } })
  assert.deepEqual(imagePlan(full, pickOf({ ...pg, fits: false }, 'docker')), { label: 'free', note: { key: 'noteNoSpace', values: { need: '865 MB', machine: 'lima1', free: '400 MB' } } })
  // no space outranks stopped in the note; the button still starts first
  const fullStopped = imagePlan({ ...full, state: 'stopped' }, pickOf({ ...pg, fits: false }, 'docker'))
  assert.equal(fullStopped.label, 'start')
  assert.equal(fullStopped.note.key, 'noteNoSpace')
  assert.equal(imagePlan(answer({ ready: [a], docker: { available: false, reason: 'not-running' } }), pickOf(a, 'ready')).note.key, 'noteDockerOff')
  assert.equal(imagePlan(answer({ ready: [a], docker: { available: false, reason: 'not-installed' } }), pickOf(a, 'ready')).note.key, 'noteDockerMissing')
  assert.equal(imagePlan(answer({ state: 'stopped', docker: { available: false, reason: 'not-running' } }), null).note.key, 'noteStopped', 'stopped outranks Docker off')
})

test('an unusable row says why in one tag', () => {
  assert.deepEqual(reasonWords({ kind: 'arch', value: 'amd64' }), { key: 'dockerOtherArch', values: { arch: 'amd64' } })
  assert.deepEqual(reasonWords({ kind: 'os', value: 'windows' }), { key: 'imageOnlyOs', values: { os: 'windows' } })
  assert.deepEqual(reasonWords({ kind: 'no-glibc' }), { key: 'imageNeedsGlibc' })
  assert.deepEqual(reasonWords({ kind: 'compressed' }), { key: 'imageCannotUnpack' })
  assert.deepEqual(reasonWords({ kind: 'device-node' }), { key: 'imageCannotUnpack' })
})

test('phase to words: Creating, Starting machine, Copying image (x of y), Unpacking image, Starting Sandbox, the failure', () => {
  assert.equal(phaseWords({ phase: 'queued' }).key, 'phaseCreating')
  assert.equal(phaseWords({ phase: 'done' }).key, 'phaseSandbox', 'done until the real row shows: never back to "Creating"')
  assert.equal(phaseWords({ phase: 'machine' }).key, 'phaseMachine')
  for (const phase of ['inspect', 'export']) assert.equal(phaseWords({ phase }).key, 'phaseCopy', 'before any bytes')
  // the host keeps the copy's bytes through its steps: every copy step with bytes reads the same (no flicker)
  for (const phase of ['inspect', 'export', 'send']) assert.deepEqual(phaseWords({ phase, done: 450e6, total: 485e6 }), { key: 'phaseCopyOf', values: { done: '450 MB', total: '485 MB' } })
  assert.equal(phaseWords({ phase: 'unpack' }).key, 'phaseUnpack', 'unpack in its own words, never a bare "Copying image"')
  assert.equal(phaseWords({ phase: 'unpack', done: 1, total: 4 }).key, 'phaseUnpack')
  assert.equal(phaseWords({ phase: 'send' }).key, 'phaseCopy', 'no bytes reported: no numbers')
  assert.deepEqual(phaseWords({ phase: 'send', done: 148e6, total: 412e6 }), { key: 'phaseCopyOf', values: { done: '148 MB', total: '412 MB' } })
  assert.equal(phaseWords({ phase: 'start' }).key, 'phaseSandbox')
  assert.deepEqual(phaseWords({ phase: 'failed', error: { code: 'NoSpace', message: 'lima1 has 0.4 GB free' } }), { key: 'createFailedRow', values: { reason: 'lima1 has 0.4 GB free' } })
  assert.equal(phaseShare({ phase: 'send', done: 1, total: 4 }), 0.25)
  assert.equal(phaseShare({ phase: 'export', done: 3, total: 4 }), 0.75, 'an export step keeping the copy\'s bytes keeps its bar')
  assert.equal(phaseShare({ phase: 'unpack', done: 1, total: 4 }), 0.25)
  assert.equal(phaseShare({ phase: 'unpack' }), null)
  assert.equal(phaseShare({ phase: 'send' }), null)
})

test('ready group footer: "as of" only for a stopped machine; a failed archive read says so with its message', () => {
  const a = row(D(1))
  assert.equal(imagesFooter(answer({ ready: [a] })), null, 'fresh rows: no footer')
  assert.equal(imagesFooter(null, 'stopped'), null)
  assert.equal(imagesFooter(answer({ state: 'running', ready: [a] }), 'stopped'), null, 'not stale: nothing, whatever the machine list says')
  const kept = { at: 1, code: 'Unavailable' }
  assert.deepEqual(imagesFooter(answer({ state: 'stopped', ready: [a], stale: kept })), { key: 'imagesAsOf', values: { machine: 'lima1' } })
  assert.deepEqual(imagesFooter(answer({ state: undefined, ready: [a], stale: kept }), 'stopped'), { key: 'imagesAsOf', values: { machine: 'lima1' } }, 'the machine list stands in for the state')
  const unread = { at: 1, code: 'EACCES', message: 'permission denied' }
  assert.deepEqual(imagesFooter(answer({ state: 'running', ready: [a], stale: unread })), { key: 'imagesReadFailed', values: { machine: 'lima1', message: 'permission denied' } }, 'running: never "as of"')
  assert.deepEqual(imagesFooter(answer({ state: 'unknown', ready: [], stale: { at: 1, code: 'Unknown' } })), { key: 'imagesReadFailed', values: { machine: 'lima1', message: 'Unknown' } }, 'no message: the code')
})

test('a worker of another LayerFS build: its own status and one line (not "stopped", not the raw message)', () => {
  const outdated = { at: 1, code: 'WorkerOutdated', message: 'machine demo-build is of another build; replace its worker' }
  const a = answer({ machine: 'demo-build', state: 'running', ready: [], stale: outdated })
  assert.equal(imagesStatus(a), 'outdated')
  assert.deepEqual(imagesFooter(a), { key: 'imagesWorkerOutdated', values: { machine: 'demo-build' } })
  assert.equal(imagesStatus(answer({ state: 'stopped', stale: outdated })), 'stopped', 'a stopped machine stays stopped')
  assert.equal(imagesStatus(answer({ state: 'running', stale: { at: 1, code: 'Unavailable', message: 'x' } })), 'failed')
  assert.equal(imagesStatus(answer({ state: 'running' })), 'fresh')
  assert.equal(imagesStatus(null, 'stopped'), 'stopped', 'the machine list stands in')
  assert.equal(imagesStatus(null, 'running'), 'fresh')
  assert.equal(imagePlan(a, null, 'running').label, 'create', 'a running machine never asks to start')
})

test('an image is named by its :latest ref, else its first', () => {
  assert.equal(displayRef(['alpine:3', 'alpine:latest']), 'alpine:latest')
  assert.equal(displayRef(['pg:17', 'pg:17.2']), 'pg:17')
  assert.equal(displayRef([]), null)
  assert.equal(pickOf(row(D(1), { name: 'alpine:latest', refs: ['alpine:3', 'alpine:latest'] }), 'ready').ref, 'alpine:latest')
})

test('creation rows sit in their project\'s group (one made when the project has no Sandbox yet) and name the Store', () => {
  const box = (name, project) => ({ name, project, state: 'running' })
  const make = (id, project, startedAt) => ({ id, input: { project, name: `${id}-1`, machine: 'lima1' }, phase: 'queued', startedAt, updatedAt: startedAt })
  const rows = [box('a-1', 's1/web'), box('b-1', 's2/api'), box('a-2', 's1/web'), box('x-1', null)]
  const groups = sandboxGroups(rows, [make('n', 's3/new', 3), make('w', 's1/web', 2), make('f', 's1/fresh', 1)])
  assert.deepEqual(groups.map((g) => [g.project, g.rows.map((b) => b.name), g.creating.map((c) => c.id)]), [
    ['new', [], ['n']],
    ['fresh', [], ['f']],
    ['web', ['a-1', 'a-2'], ['w']],
    ['api', ['b-1'], []],
    ['', ['x-1'], []],
  ], 'creates join their group on top; a project with only creates gets its own group first')
  assert.deepEqual(sandboxGroups(rows, []).map((g) => g.project), ['web', 'api', ''], 'no creates: the real rows\' groups, in order')
  assert.equal(creationStore(make('w', 's1/web', 1)), 's1', 'the Store, as a real row shows it')
  assert.equal(creationStore(make('w', 'web', 1)), '', 'no Store in the key: none shown')
})

/** A host API double: scripted answers by `METHOD path`, recorded calls, and events to emit. */
function fakeApi(answers) {
  const calls = []
  const listeners = new Map()
  return {
    calls,
    emit: (type, data) => { for (const l of listeners.get(type) ?? []) l({ type, data }) },
    connected: () => true,
    onEvent: (type, l) => { if (!listeners.has(type)) listeners.set(type, new Set()); listeners.get(type).add(l); return () => listeners.get(type).delete(l) },
    call: async (method, path, body) => {
      calls.push([method, path, body])
      const a = answers[`${method} ${path}`]
      if (a instanceof Error) throw a
      return typeof a === 'function' ? a(body) : a
    },
    op: async () => ({}),
    openPanel: () => {},
  }
}
const tick = (ms = 0) => new Promise((r) => setTimeout(r, ms))
const creation = (id, phase, extra = {}) => ({ id, input: { project: 's/web', name: 'web-1', machine: 'lima1', image: D(1) }, phase, startedAt: 1, updatedAt: 1, ...extra })

test('creations: listed on activate, a row on admission, moved by events, dropped after the machines read that follows done', async () => {
  const api = fakeApi({
    'GET /api/machines': { available: true, machines: [], sandboxes: [], stores: [], postgresql: [], platforms: [], status: {} },
    'GET /api/sandbox-creates': { creations: [creation('c-old', 'failed', { input: { project: 's/a', name: 'a-1', machine: 'lima1' }, error: { code: 'Unknown', message: 'boom' } })] },
    'POST /api/projects/s%2Fweb/sandboxes': (body) => ({ creation: creation('c-1', 'queued', { input: { project: 's/web', ...body } }) }),
  })
  const c = new MachinesController(api)
  const face = c.face()
  const release = face.activate()
  try {
    await tick()
    assert.deepEqual(Object.keys(face.hooks.creations.getSnapshot()), ['c-old'], 'the host list on activate')
    const admitted = await face.createSandbox('s/web', { name: 'web-1', machine: 'lima1', image: D(1), start: true })
    assert.equal(admitted.id, 'c-1')
    const post = api.calls.find(([m, p]) => m === 'POST' && p.startsWith('/api/projects/'))
    assert.deepEqual(post[2], { name: 'web-1', machine: 'lima1', image: D(1), start: true, background: true }, 'always in the background; image is the id; start asked')
    assert.equal(face.hooks.creations.getSnapshot()['c-1'].phase, 'queued', 'the row shows on admission')

    api.emit('sandbox-progress', { creation: creation('c-1', 'machine', { updatedAt: 5 }) })
    assert.equal(face.hooks.creations.getSnapshot()['c-1'].phase, 'machine')
    api.emit('sandbox-progress', { creation: creation('c-1', 'queued', { updatedAt: 2 }) })
    assert.equal(face.hooks.creations.getSnapshot()['c-1'].phase, 'machine', 'an older event changes nothing')

    const reads = () => api.calls.filter(([m, p]) => m === 'GET' && p === '/api/machines').length
    const before = reads()
    api.emit('sandbox-progress', { creation: creation('c-1', 'done', { updatedAt: 9 }) })
    assert.equal(face.hooks.creations.getSnapshot()['c-1'].phase, 'done', 'kept until the machines read shows the Sandbox')
    await tick(650)
    assert.ok(reads() > before, 'done queues one machines read')
    await tick()
    assert.equal(face.hooks.creations.getSnapshot()['c-1'], undefined, 'dropped once that read landed')
  } finally { release(); c.dispose() }
})

test('creations: Retry re-sends the failed input as a new job; dismiss deletes it on the host', async () => {
  let n = 0
  const api = fakeApi({
    'GET /api/machines': { available: true, machines: [], sandboxes: [], stores: [], postgresql: [], platforms: [], status: {} },
    'GET /api/sandbox-creates': { creations: [creation('c-bad', 'failed', { error: { code: 'Unknown', message: 'boom' } })] },
    'POST /api/projects/s%2Fweb/sandboxes': (body) => ({ creation: creation(`c-new-${++n}`, 'queued', { input: { project: 's/web', ...body } }) }),
    'DELETE /api/sandbox-creates/c-new-1': undefined,
  })
  const c = new MachinesController(api)
  const face = c.face()
  const release = face.activate()
  try {
    await tick()
    const failed = face.hooks.creations.getSnapshot()['c-bad']
    await face.retryCreation(failed)
    const post = api.calls.find(([m]) => m === 'POST')
    assert.deepEqual(post[2], { name: 'web-1', machine: 'lima1', image: D(1), background: true }, 'the same input, project in the path')
    assert.deepEqual(Object.keys(face.hooks.creations.getSnapshot()), ['c-new-1'], 'the failed row gives way to the new job')
    await face.dismissCreation('c-new-1')
    assert.ok(api.calls.some(([m, p]) => m === 'DELETE' && p === '/api/sandbox-creates/c-new-1'))
    assert.deepEqual(face.hooks.creations.getSnapshot(), {})
  } finally { release(); c.dispose() }
})

// ---------------------------------------------------------------- Stage 1 (§5.1.1–5.1.3)

test('Stage 1 tags: used ago, size · copies once, greyed reasons; who uses an image', () => {
  const now = Date.parse('2026-11-14T12:00:00Z')
  assert.equal(agoWords(null, now), null, 'never used: the "ready" tag')
  assert.equal(agoWords('not a time', now), null)
  assert.deepEqual(agoWords('2026-11-14T11:59:30Z', now), { key: 'agoNow' })
  assert.deepEqual(agoWords('2026-11-14T11:48:00Z', now), { key: 'agoMinutes', values: { n: '12' } })
  assert.deepEqual(agoWords('2026-11-14T10:00:00Z', now), { key: 'agoHours', values: { n: '2' } })
  assert.deepEqual(agoWords('2026-11-01T12:00:00Z', now), { key: 'agoDays', values: { n: '13' } })
  assert.deepEqual(dockerTag({ usable: true, size: 412e6 }), { key: 'imageCopiesOnce', values: { size: '412 MB' } })
  assert.deepEqual(dockerTag({ usable: false, size: 1, reason: { kind: 'arch', value: 'amd64' } }), { key: 'dockerOtherArch', values: { arch: 'amd64' } })
  assert.deepEqual(dockerTag({ usable: false, size: 1, reason: { kind: 'os', value: 'windows' } }), { key: 'imageOnlyOs', values: { os: 'windows' } })
  assert.deepEqual([0, 1, 3].map(usedByWords), [{ key: 'imagesUnused' }, { key: 'imagesUsedByOne' }, { key: 'imagesUsedBy', values: { n: '3' } }])
})

test('Images section and Free space: total and free, removable rows, unused preselected, what the choice frees', () => {
  const rows = [
    row(D(1), { source: 'bundle', size: 310e6, sandboxes: 1 }),
    row(D(2), { source: 'imported', size: 780e6, sandboxes: 2 }),
    row(D(3), { source: 'imported', size: 412e6, sandboxes: 0 }),
    row(D(4), { source: 'machine', size: 120e6, sandboxes: 0 }),
  ]
  assert.deepEqual(imagesTotal(answer({ ready: rows, free: 12.4e9 })), { key: 'imagesTotal', values: { total: '1.6 GB', machine: 'lima1', free: '12.4 GB' } })
  assert.deepEqual(imagesTotal(answer({ ready: rows })), { key: 'imagesTotalOnly', values: { total: '1.6 GB', machine: 'lima1' } })
  assert.equal(removable(rows[0]), false, 'the bundle image has no Remove')
  const { rows: offered, preselected } = freeSpaceRows(answer({ ready: rows }))
  assert.deepEqual(offered.map((r) => r.id), [D(2), D(3), D(4)])
  assert.deepEqual(preselected, [D(3), D(4)], 'the unused ones checked')
  assert.deepEqual(freeSpaceRows(null), { rows: [], preselected: [] })
  assert.deepEqual(freesWords(offered, preselected), { key: 'freesSize', values: { size: '532 MB' } })
  assert.deepEqual(freesWords(offered, [D(3)], 0.8e9), { key: 'freesSizeNeed', values: { size: '412 MB', need: '800 MB' } })
})

test('a failed create says why in this locale (§4.4); Capacity offers Free space', () => {
  // 2.0 `Capacity`: detail { needed_bytes, free_bytes, path }
  assert.deepEqual(failureWords({ code: 'Capacity', message: 'x', detail: { needed_bytes: 865e6, free_bytes: 4e8, path: '/var/lib/layerfs' } }, 'lima1'), { key: 'imageNoSpace', values: { machine: 'lima1', need: '865 MB', free: '400 MB' } })
  assert.equal(failureWords({ code: 'Capacity', message: 'x' }, 'lima1'), null, 'no numbers: LayerFS\'s message')
  assert.deepEqual(failureWords({ code: 'Unsupported', message: 'x', detail: { architecture: 'amd64' } }, 'm'), { key: 'imageWrongArch', values: { arch: 'amd64' } })
  assert.deepEqual(failureWords({ code: 'Unsupported', message: 'x', detail: { os: 'macos' } }, 'm'), { key: 'imageHostOnly' })
  assert.deepEqual(failureWords({ code: 'Unsupported', message: 'x', detail: { os: 'windows' } }, 'm'), { key: 'imageWrongOs', values: { os: 'windows' } })
  assert.deepEqual(failureWords({ code: 'Unavailable', message: 'x', detail: { what: 'docker' } }, 'm'), { key: 'dockerNotRunning' })
  assert.deepEqual(failureWords({ code: 'Unavailable', message: 'x' }, 'lima1'), { key: 'machineStopped', values: { machine: 'lima1' } })
  assert.deepEqual(failureWords({ code: 'InvalidInput', message: 'no such image: nope:1' }, 'm'), { key: 'imageUnknown' })
  assert.equal(failureWords({ code: 'InvalidInput', message: 'bad name' }, 'm'), null)
  assert.deepEqual(failureWords({ code: 'Busy', message: 'x' }, 'lima1'), { key: 'machineBusy', values: { machine: 'lima1' } })
  assert.deepEqual(failureWords({ code: 'Exists', message: 'x' }, 'm'), { key: 'sandboxNameTaken' }, '2.0 says Exists')
  assert.deepEqual(failureWords({ code: 'Timeout', message: 'x' }, 'm'), { key: 'imageTimeout' })
  assert.equal(failureWords({ code: 'Unknown', message: 'boom' }, 'm'), null)
  const failed = { phase: 'failed', input: { machine: 'lima1' }, error: { code: 'Capacity', message: 'x', detail: { needed_bytes: 2e9, free_bytes: 1e9, path: null } } }
  assert.deepEqual(phaseWords(failed), { key: 'imageNoSpace', values: { machine: 'lima1', need: '2.0 GB', free: '1.0 GB' } })
  assert.deepEqual(phaseWords({ phase: 'failed', input: { machine: 'm' }, error: { code: 'Unknown', message: 'boom' } }), { key: 'createFailedRow', values: { reason: 'boom' } })
  assert.equal(needsSpace(failed), true)
  assert.equal(needsSpace({ phase: 'failed', error: { code: 'Unknown', message: '' } }), false)
  assert.equal(needsSpace({ phase: 'send', error: undefined }), false)
})

test('imagePlan Stage 1: a Docker pick that does not fit asks to free space; once it fits the button creates', () => {
  const pg = { id: D(4), name: 'postgres:17', refs: ['postgres:17'], size: 412e6, architecture: 'arm64', os: 'linux', need: 865.2e6, fits: false, usable: true }
  const full = answer({ stage: 1, free: 4e8, docker: { available: true, images: [pg] } })
  assert.equal(imagePlan(full, pickOf(pg, 'docker')).label, 'free')
  const freed = answer({ stage: 1, free: 2e9, docker: { available: true, images: [{ ...pg, fits: true }] } })
  assert.equal(imagePlan(freed, pickOf(pg, 'docker')).label, 'create')
  assert.equal(imagePlan(answer({ stage: 1, free: 4e8, ready: [row(D(1))], docker: { available: true, images: [pg] } }), pickOf(row(D(1)), 'ready')).label, 'create', 'a ready pick never needs space')
})

test('pruneImages posts the ids and reads the machines again', async () => {
  const api = fakeApi({
    'GET /api/machines': { available: true, machines: [], sandboxes: [], stores: [], postgresql: [], platforms: [], status: {} },
    'POST /api/machines/lima1/prune-images': { removed: [D(3)], freed: 412e6, refused: [] },
  })
  const c = new MachinesController(api)
  const face = c.face()
  try {
    assert.deepEqual(await face.pruneImages('lima1', [D(3)]), { removed: [D(3)], freed: 412e6, refused: [] })
    assert.deepEqual(api.calls[0], ['POST', '/api/machines/lima1/prune-images', { images: [D(3)] }])
    await tick()
    assert.ok(api.calls.some(([m, p]) => m === 'GET' && p === '/api/machines'))
  } finally { c.dispose() }
})

test('a create\'s row stands for its Sandbox while LayerFS records it as creating (the image copy shows), then the real row', () => {
  const job = (id, name, phase, startedAt = 1) => ({ id, input: { project: 's/p', name, machine: 'docker' }, phase, startedAt, updatedAt: startedAt })
  const all = { a: job('a', 'p-1', 'send', 2), b: job('b', 'p-2', 'failed', 1), c: job('c', 'p-3', 'done', 3), d: job('d', 'p-4', 'send', 4) }
  // 2.0: a Sandbox being created is `busy` with op `sandbox.create`; one that runs is `ready`
  const creatingRow = { state: 'busy', op: { name: 'sandbox.create', phase: 'export', started: 1 } }
  const boxes = [
    { name: 'p-1', machine: 'docker', ...creatingRow },
    { name: 'p-2', machine: 'docker', ...creatingRow },
    { name: 'p-3', machine: 'docker', state: 'ready' },
    { name: 'p-0', machine: 'docker', state: 'ready' },
  ]
  const creating = creatingOn(all, 'docker', boxes)
  assert.deepEqual(creating.map((c) => c.id), ['d', 'a', 'b'], 'running (record creating, or none yet) and failed creates, newest first; a done one whose Sandbox runs leaves')
  assert.deepEqual(besideCreating(boxes, creating).map((b) => b.name), ['p-2', 'p-3', 'p-0'], 'the creating record behind a running create is not drawn twice; a failed create\'s record stays')
  assert.deepEqual(creatingOn(all, 'lima1', boxes), [], 'other machines\' creates are not this machine\'s')
  assert.deepEqual(creatingOn({ a: job('a', 'p-1', 'done') }, 'docker', boxes).map((c) => c.id), ['a'], 'done while the read still says creating: the row stays until the next read')
})
