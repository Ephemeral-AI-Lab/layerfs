// The Store page's tabs and numbers (src/client/storage/store-page.ts): tabs
// per family, the database file's parts, growth runs broken at gaps, project
// shares and hues, card order and search, running Sandboxes and file words.
//   node --test test/store-page.test.mjs
import { test, after } from 'node:test'
import assert from 'node:assert/strict'
import { mkdirSync, mkdtempSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

const root = fileURLToPath(new URL('..', import.meta.url))
mkdirSync(join(root, 'lib'), { recursive: true })
const out = mkdtempSync(join(root, 'lib', '.test-store-page-'))
after(() => rmSync(out, { recursive: true, force: true }))
await build({ absWorkingDir: root, stdin: { contents: "export * from './src/client/storage/store-page.ts'", resolveDir: root, loader: 'ts' }, outfile: join(out, 'page.mjs'), bundle: true, format: 'esm', platform: 'node', target: 'node24', logLevel: 'warning' })
const P = await import(pathToFileURL(join(out, 'page.mjs')).href)

const project = (name, over = {}) => ({ name, created: 1000, totals: null, last: null, files: null, partial: false, building: null, error: null, ...over })
const totals = (saves, stored) => ({ saves, merges: 0, branches: 1, stored, metadata: 0, commitLogical: 0, canonical: 0 })

test('the third tab follows the Store family', () => {
  assert.deepEqual(P.storeTabs('sqlite'), ['overview', 'projects', 'backups'])
  assert.deepEqual(P.storeTabs('postgresql'), ['overview', 'projects', 'connections'])
})

test('file parts are shares of what LayerFS reported (2.0 `stats` bytes); unknown parts are left out', () => {
  const parts = P.fileParts({ stored_bytes: 2001045, metadata_bytes: 227705, reusable_bytes: 0, file_bytes: 2228224 })
  assert.deepEqual(parts.map((p) => p.key), ['stored', 'metadata', 'free'])
  assert.ok(Math.abs(parts.reduce((a, p) => a + p.share, 0) - 1) < 1e-9)
  assert.deepEqual(P.fileParts({ stored_bytes: 10, metadata_bytes: 5, reusable_bytes: null }).map((p) => p.key), ['stored', 'metadata'])
  assert.equal(P.fileParts({ stored_bytes: null, metadata_bytes: null, reusable_bytes: null }), null)
  assert.equal(P.fileParts(undefined), null)
  assert.equal(P.compressionOf({ canonical_bytes: 500, stored_bytes: 100 }), 5)
  assert.equal(P.compressionOf({ canonical_bytes: 500, stored_bytes: 0 }), null)
})

test('growth uses the newest series and breaks at gaps over the journal gap', () => {
  const pt = (observedAt, file) => ({ observedAt, stats: { file_bytes: file, stored_bytes: file / 2, metadata_bytes: 1 } })
  const journal = { gapAfterMs: 100, series: [
    { identity: 'old', startedAt: 0, points: [pt(1, 9)] },
    { identity: 'new', startedAt: 10, points: [pt(30, 3), pt(10, 1), pt(50, 4), pt(300, 8), pt(350, 9)] },
  ] }
  const runs = P.growthRuns(journal)
  assert.deepEqual(runs.map((r) => r.map((p) => p.at)), [[10, 30, 50], [300, 350]])
  assert.equal(runs[1][0].file, 8)
  assert.deepEqual(P.growthRuns({ gapAfterMs: 100, series: [] }), [])
})

test('shares skip projects with nothing counted and keep a hue by name order', () => {
  const rows = [project('web', { totals: totals(3, 300) }), project('api', { totals: totals(1, 100) }), project('docs')]
  const shares = P.projectShares(rows)
  assert.deepEqual(shares.map((s) => [s.name, s.share]), [['web', 0.75], ['api', 0.25]])
  assert.equal(P.hueOf('api', rows), 0)
  assert.equal(P.hueOf('docs', rows), 1)
  assert.equal(P.hueOf('web', rows), 2)
})

test('rows order by last commit, name, commits, branches or size, and search by name', () => {
  const rows = [
    project('b', { last: { id: 'x', time: 50, branch: 'main', message: '' }, totals: totals(1, 10) }),
    project('a', { created: 10, totals: totals(0, 99) }),
    project('c', { last: { id: 'y', time: 90, branch: 'main', message: '' }, totals: totals(2, 5) }),
  ]
  assert.deepEqual(P.orderProjects(rows, 'recent', '').map((p) => p.name), ['c', 'b', 'a'])
  assert.deepEqual(P.orderProjects(rows, 'name', '').map((p) => p.name), ['a', 'b', 'c'])
  assert.deepEqual(P.orderProjects(rows, 'size', '').map((p) => p.name), ['a', 'b', 'c'])
  assert.deepEqual(P.orderProjects(rows, 'commits', '').map((p) => p.name), ['c', 'b', 'a'])
  assert.deepEqual(P.orderProjects(rows, 'branches', '').map((p) => p.name), ['a', 'b', 'c'])
  assert.deepEqual(P.orderProjects(rows, 'name', ' B ').map((p) => p.name), ['b'])
  assert.deepEqual(rows.map((p) => p.name), ['b', 'a', 'c'], 'the input is not reordered')
})

test('a session-store save reads as its session and event range; other Commits keep their first line', () => {
  assert.deepEqual(P.commitLine({ id: 'x', time: 1, branch: 'dsh.session.939f81ffc31646d8914df60a5d', message: 'dsh session 939f81ff: events 7591..7616 (54925 bytes)' }),
    { session: { id: '939f81ff', range: '7591–7616' }, message: 'dsh session 939f81ff: events 7591..7616 (54925 bytes)', branch: null })
  assert.deepEqual(P.commitLine({ id: 'y', time: 1, branch: 'c1', message: 'b\n\nmore detail' }), { session: null, message: 'b', branch: 'c1' })
  assert.equal(P.commitLine({ id: 'z', time: 1, branch: 'main', message: '' }).message, '')
})

test('running Sandboxes count live daemons of this Store only', () => {
  const counts = P.runningByProject('s', [
    { project: 's/web', live: true }, { project: 's/web', live: true }, { project: 's/web', live: false },
    { project: 's/api', state: 'running' }, { project: 's/api', state: 'running', live: null },
    { project: 'other/web', live: true }, { project: null, live: true },
  ])
  assert.deepEqual([...counts], [['web', 2], ['api', 1]])
})

test('commit totals say whether every ledger finished, and the file figure marks a cut walk', () => {
  assert.deepEqual(P.commitTotal([project('a', { totals: totals(3, 1) }), project('b', { totals: totals(4, 1) })]), { commits: 7, complete: true, refused: null })
  assert.deepEqual(P.commitTotal([project('a', { totals: totals(3, 1), partial: true })]), { commits: 3, complete: false, refused: null })
  const refused = project('pg', { partial: true, error: { code: 'Unsupported', message: 'no PostgreSQL engine' } })
  assert.deepEqual(P.commitTotal([refused]), { commits: 0, complete: false, refused: 'no PostgreSQL engine' })
  assert.equal(P.stillCounting(refused), false, 'a refused ledger is not polled')
  assert.equal(P.stillCounting(project('a', { partial: true })), true)
  assert.equal(P.stillCounting({ ...refused, building: { branches: 0, branchesTotal: 1, saves: 0 } }), true, 'a new read after a refusal is polled')
  assert.equal(P.stillCounting(project('done', { totals: totals(1, 1) })), false)
  assert.equal(P.filesNumber({ files: 1234, complete: true }), (1234).toLocaleString())
  assert.equal(P.filesNumber({ files: 10000, complete: false }), `${(10000).toLocaleString()}+`)
  assert.equal(P.filesNumber({ files: null, complete: true }), null)
  assert.equal(P.filesNumber(null), null)
})
