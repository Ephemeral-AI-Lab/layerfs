// Image copies to a machine in the background (src/host/routes/image-loads.ts,
// Machines.loadImage): `POST /api/machines/:id/load-image { image }` answers 202
// with the queued copy, `image-load { load }` events through the copy's phases,
// a copy of the same image already running answered instead of a second one,
// a failed copy kept with its refusal until dismissed, 501 without machine.load_image;
// and the Images tab's pure helpers (dockerRows, imageLabel, badgeHue, diskSegments, loadWords).
//   node --test test/image-loads.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'
import { loadRoutes, testHost } from './routes-helper.mjs'

const R = await loadRoutes()
const root = fileURLToPath(new URL('..', import.meta.url))
const out = mkdtempSync(join(root, 'lib', '.test-image-loads-'))
await build({
  absWorkingDir: root,
  stdin: { contents: "export * from './src/client/machines/images.ts'", resolveDir: root, loader: 'ts' },
  outfile: join(out, 'images.mjs'), bundle: true, format: 'esm', platform: 'node', target: 'node24', logLevel: 'warning',
})
const { dockerRows, imageLabel, badgeHue, diskSegments, loadWords, reasonWords } = await import(pathToFileURL(join(out, 'images.mjs')).href)
rmSync(out, { recursive: true, force: true })

class Refusal extends Error {
  constructor(code, message, detail = null) { super(message); Object.assign(this, { code, detail, transport: 'refusal' }) }
}

const ID = `sha256:${'b'.repeat(64)}`
const LOAD = { family: 'machine', call: 'load_image', args: [], options: [{ name: 'on_progress' }] }

/** A fake home whose machine.load_image reports a copy and waits for `release()` (or `fail(error)`). */
function fixture({ calls = [LOAD] } = {}) {
  const loads = []
  let wait = null
  const lfs = {
    machine: {
      load_image: (name, image, { on_progress }) => {
        loads.push([name, image])
        on_progress({ phase: 'inspect' })
        on_progress({ phase: 'send', done: 10, total: 100 })
        return new Promise((resolve, reject) => { wait = { resolve: () => resolve({ image, reused: false, outcome: 'created', machine: name, refs: ['postgres:17'], size: 100 }), reject } })
      },
    },
  }
  const dir = mkdtempSync(join(tmpdir(), 'dsh-image-loads-'))
  const t = testHost({ lfs, root: dir, home: join(dir, 'home'), discovery: async () => ({ identity: { version: '2.0.0-test', features: [] }, calls }) })
  const groups = R.createGroups(t.host, async () => ({}))
  const tick = () => new Promise((r) => setImmediate(r))
  return {
    loads,
    events: () => t.events.filter((e) => e.type === 'image-load').map((e) => e.data.load),
    http: (method, path, body) => R.handle(groups, { method, path, body }),
    release: async () => { await tick(); wait.resolve(); await tick(); await tick() },
    fail: async (error) => { await tick(); wait.reject(error); await tick(); await tick() },
    close: () => rmSync(dir, { recursive: true, force: true }),
  }
}

test('load-image: 202 with the queued copy, one copy per image while it runs, events to done, then gone', async () => {
  const f = fixture()
  try {
    const a = await f.http('POST', '/api/machines/docker/load-image', { image: ID })
    assert.equal(a.status, 202)
    assert.match(a.body.load.id, /^l-[0-9a-z]+-[0-9a-f]{4}$/)
    assert.deepEqual({ ...a.body.load, id: '', startedAt: 0, updatedAt: 0 }, { id: '', machine: 'docker', image: ID, phase: 'queued', startedAt: 0, updatedAt: 0 })
    await new Promise((r) => setImmediate(r))
    const again = await f.http('POST', '/api/machines/docker/load-image', { image: ID })
    assert.equal(again.body.load.id, a.body.load.id, 'a running copy of the same image is answered, not started again')
    assert.deepEqual(f.loads, [['docker', ID]])
    assert.equal((await f.http('DELETE', `/api/image-loads/${a.body.load.id}`)).status, 409, 'a running copy is not dismissed')
    const running = (await f.http('GET', '/api/image-loads')).body.loads
    assert.deepEqual(running.map((l) => [l.phase, l.done, l.total]), [['send', 10, 100]], 'the list carries the copy\'s bytes')
    await f.release()
    const phases = f.events().map((l) => l.phase)
    assert.deepEqual([phases[0], phases[1], phases.at(-1)], ['queued', 'inspect', 'done'])
    assert.deepEqual((await f.http('GET', '/api/image-loads')).body.loads, [])
  } finally { f.close() }
})

test('load-image: a failed copy keeps its refusal until dismissed; nothing is sent again', async () => {
  const f = fixture()
  try {
    const a = (await f.http('POST', '/api/machines/docker/load-image', { image: ID })).body.load
    await f.fail(new Refusal('NoSpace', 'not enough space', { need: 200, free: 50 }))
    const listed = (await f.http('GET', '/api/image-loads')).body.loads
    assert.deepEqual(listed.map((l) => [l.id, l.phase, l.error]), [[a.id, 'failed', { code: 'NoSpace', message: 'not enough space', detail: { need: 200, free: 50 } }]])
    assert.deepEqual(f.loads.length, 1)
    assert.deepEqual((await f.http('DELETE', `/api/image-loads/${a.id}`)).body.loads, [])
  } finally { f.close() }
})

test('load-image: 501 without machine.load_image, 400 without an image', async () => {
  const f = fixture({ calls: [] })
  try {
    const r = await f.http('POST', '/api/machines/docker/load-image', { image: ID })
    assert.equal(r.status, 501)
    assert.deepEqual(f.loads, [])
    const g = fixture()
    try { assert.equal((await g.http('POST', '/api/machines/docker/load-image', {})).status, 400) } finally { g.close() }
  } finally { f.close() }
})

const view = (images) => ({ machine: 'docker', stage: 1, mode: 'images', free: 1000, ready: [{ id: 'r', size: 300, sandboxes: 2 }, { id: 's', size: 100, sandboxes: 0 }], docker: images === null ? null : { available: true, images } })
const img = (name, usable = true, refs = [name]) => ({ id: `sha256:${name}`, name, refs, size: 10, need: 21, fits: true, usable, ...(usable ? {} : { reason: { kind: 'os', value: '' } }) })

test('dockerRows: usable first, unusable after, narrowed by any ref', () => {
  const v = view([img('alpine:3.19', false), img('postgres:17', true, ['postgres:17', 'pg:latest']), img('rust:1.85')])
  const all = dockerRows(v)
  assert.deepEqual(all.usable.map((r) => r.name), ['postgres:17', 'rust:1.85'])
  assert.deepEqual(all.unusable.map((r) => r.name), ['alpine:3.19'])
  assert.equal(all.total, 3)
  assert.deepEqual(dockerRows(v, ' PG ').usable.map((r) => r.name), ['postgres:17'], 'any ref matches, case-blind')
  assert.equal(dockerRows(view(null)).total, 0)
})

test('loadWords and reasonWords: copy phases, a failure, an image with no OS', () => {
  assert.deepEqual(loadWords({ phase: 'queued', machine: 'docker' }), { key: 'imgCopyQueued' })
  assert.equal(loadWords({ phase: 'send', done: 1048576, total: 2097152, machine: 'docker' }).key, 'phaseCopyOf')
  assert.equal(loadWords({ phase: 'unpack', machine: 'docker' }).key, 'phaseUnpack')
  assert.equal(loadWords({ phase: 'failed', error: { code: 'Capacity', message: 'x', detail: { needed_bytes: 2, free_bytes: 1, path: null } }, machine: 'docker' }).key, 'imageNoSpace')
  assert.deepEqual(loadWords({ phase: 'failed', error: { code: 'Weird', message: 'boom' }, machine: 'docker' }), { key: 'imgCopyFailed', values: { reason: 'boom' } })
  assert.deepEqual(reasonWords({ kind: 'os', value: '' }), { key: 'imageNoOs' })
  assert.deepEqual(reasonWords({ kind: 'os', value: 'windows' }), { key: 'imageOnlyOs', values: { os: 'windows' } })
})

test('imageLabel, badgeHue and diskSegments: names in parts, a stable colour, the disk bar', () => {
  assert.deepEqual(imageLabel('cgr.dev/chainguard/minio:latest', ['cgr.dev/chainguard/minio:latest']), { registry: 'cgr.dev/chainguard/', repo: 'minio', tags: ['latest'] })
  assert.deepEqual(imageLabel('alpine:latest', ['alpine:3', 'alpine:latest', 'other:1']), { registry: '', repo: 'alpine', tags: ['latest', '3'] }, 'the name\'s tag first, only this repository\'s tags')
  assert.deepEqual(imageLabel('localhost:5000/app:dev', ['localhost:5000/app:dev']), { registry: 'localhost:5000/', repo: 'app', tags: ['dev'] }, 'a registry port is not a tag')
  assert.deepEqual(imageLabel('LayerFS default', []), { registry: '', repo: 'LayerFS default', tags: [] })
  assert.equal(badgeHue('postgres'), badgeHue('postgres'))
  assert.ok(badgeHue('postgres') >= 0 && badgeHue('postgres') < 6)
  assert.equal(diskSegments([{ id: 'a', size: 10 }], null, 5), null)
  assert.deepEqual(diskSegments([{ id: 'a', size: 10 }, { id: 'b', size: null }], 100, 60), { images: [{ id: 'a', bytes: 10, share: 0.1 }, { id: 'b', bytes: 0, share: 0 }], imagesBytes: 10, rest: 30, free: 60, total: 100 })
})
