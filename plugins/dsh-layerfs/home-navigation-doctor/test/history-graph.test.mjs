// The history tree's model (src/client/files/tree-model.ts): one step per saved thing in
// saved order, lanes around a centered main, and what reached main; plus the page
// bookkeeping in graph-model.ts.
//   node --test test/history-graph.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

const root = fileURLToPath(new URL('..', import.meta.url))
const out = mkdtempSync(join(tmpdir(), 'dsh-history-graph-'))
await build({
  absWorkingDir: root, entryPoints: { graph: 'src/client/files/graph-model.ts', tree: 'src/client/files/tree-model.ts' }, outdir: out,
  bundle: true, format: 'esm', platform: 'node', target: 'node24', logLevel: 'warning', outExtension: { '.js': '.mjs' },
})
const { extendPage, movedCount, countedTotal } = await import(pathToFileURL(join(out, 'graph.mjs')).href)
const { treeLayout, branchParents } = await import(pathToFileURL(join(out, 'tree.mjs')).href)
process.on('exit', () => rmSync(out, { recursive: true, force: true }))

const branch = (name, head, forkedFrom = null, base = 'main') => ({ name, head, base, forkedFrom })
const commit = (id, branchName, parent, extra = {}) => ({ id, branch: branchName, message: `m-${id}`, at: Number(id.replace(/\D/g, '')) || 0, parent, ...extra })
const page = (items, next = null, head = null) => ({ items, next, head })
/** A Branch's page from its Commits oldest first. */
const run = (name, ids, from, extra = {}) => page(ids.map((id, i) => commit(id, name, i ? ids[i - 1] : from, extra[id])).reverse(), null, ids.at(-1))

test('treeLayout: every saved thing takes one step, in saved order, never before its parent', () => {
  const branches = [branch('main', '32b'), branch('a', 'a5', '32a'), branch('b', 'b6', '32a')]
  const pages = {
    main: page([commit('32b', 'main', '32a', { at: null, sourceCommit: 'a5', sourceBranch: 'a', sourceTime: 5 }), commit('32a', 'main', null, { at: null })], null, '32b'),
    a: run('a', ['a1', 'a3', 'a5'], '32a'),
    b: run('b', ['b2', 'b4', 'b6'], '32a'),
  }
  const layout = treeLayout(branches, pages)
  assert.deepEqual(layout.rows.map((r) => r.id), ['32a', 'a1', 'b2', 'a3', 'b4', 'a5', '32b', 'b6'])
  assert.deepEqual(layout.rows.map((r) => r.slot), [0, 1, 2, 3, 4, 5, 6, 7])
  assert.equal(layout.byName.get('a').lane * layout.byName.get('b').lane < 0, true, 'two Branches off main at once take opposite sides')
  assert.deepEqual(layout.layers.map((r) => r.n), [1, 2])
  assert.deepEqual(layout.links.map((l) => [l.from.id, l.to.id, l.publish]), [['a5', '32b', true]])
  assert.equal(layout.settled, true)
  assert.equal(layout.byName.get('a').live, true)
  assert.equal(layout.byName.get('b').live, false, 'b never reached main')
})

test('treeLayout: a merge follows its source; merged work counts as reaching main', () => {
  const branches = [branch('main', '32b'), branch('int', 'i9', '32a'), branch('w', 'w4', '32a'), branch('dead', 'd5', 'w4')]
  const pages = {
    main: page([commit('32b', 'main', '32a', { at: null, sourceCommit: 'i9', sourceBranch: 'int' }), commit('32a', 'main', null, { at: null })], null, '32b'),
    int: run('int', ['i1', 'i9'], '32a', { i9: { merged: 'w4', at: 2 } }),
    w: run('w', ['w3', 'w4'], '32a'),
    dead: run('dead', ['d5'], 'w4'),
  }
  const layout = treeLayout(branches, pages)
  const slot = (id) => layout.byId.get(id).slot
  assert.ok(slot('i9') > slot('w4'), 'the merge is placed after what it merged, whatever its clock says')
  assert.equal(layout.byId.get('i9').kind, 'merge')
  assert.equal(layout.byName.get('w').live, true)
  assert.equal(layout.byName.get('dead').live, false)
  assert.equal(layout.byName.get('dead').parent, 'w')
  assert.equal(branchParents(branches, pages).get('dead'), 'w')
})

test('treeLayout: a Branch forked from its parent\'s last checkpoint carries on along its lane', () => {
  const branches = [branch('main', '32a'), branch('r1', 'a2', '32a'), branch('r2', 'b4', 'a2'), branch('r3', 'c6', 'b4'), branch('side', 'x5', 'b3')]
  const pages = {
    main: page([commit('32a', 'main', null, { at: null })], null, '32a'),
    r1: run('r1', ['a1', 'a2'], '32a'),
    r2: run('r2', ['b3', 'b4'], 'a2'),
    r3: run('r3', ['c6'], 'b4'),
    side: run('side', ['x5'], 'b3'),
  }
  const layout = treeLayout(branches, pages)
  const lane = (name) => layout.byName.get(name).lane
  assert.equal(lane('r2'), lane('r1'))
  assert.equal(lane('r3'), lane('r1'))
  assert.notEqual(lane('side'), lane('r1'), 'a fork from the middle takes its own lane')
  assert.equal(Math.sign(lane('side')), Math.sign(lane('r1')), 'on its parent\'s side of main')
  assert.equal(layout.reach, 2)
})

test('treeLayout: Branches that run one after another reuse lanes', () => {
  const branches = [branch('main', '32a')]
  const pages = { main: page([commit('32a', 'main', null, { at: null })], null, '32a') }
  for (let i = 0; i < 30; i++) {
    const ids = Array.from({ length: 20 }, (_, k) => `c${i * 100 + k + 1}`)
    branches.push(branch(`w${i}`, ids.at(-1), '32a'))
    pages[`w${i}`] = run(`w${i}`, ids, '32a')
  }
  const layout = treeLayout(branches, pages)
  assert.equal(layout.rows.length, 601)
  assert.ok(layout.reach <= 2, `30 Branches in sequence stay near main (reach ${layout.reach})`)
})

test('treeLayout: nothing saved yet is a bud on its fork point; unread history stays attached', () => {
  const branches = [branch('main', '32b'), branch('empty', 'a2', 'a2'), branch('a', 'a9', '32a'), branch('old', 'o5', '32zz')]
  const pages = {
    main: page([commit('32b', 'main', '32a', { at: null })], '32a', '32b'),
    // Only a's newest page is read: a8's parent is on a page not read yet.
    a: page([commit('a9', 'a', 'a8'), commit('a8', 'a', 'a7')], 'a7', 'a9'),
    old: run('old', ['o5'], '32zz'),
  }
  const layout = treeLayout(branches, pages)
  assert.equal(layout.settled, false)
  assert.equal(layout.byName.get('a').partial, true)
  assert.equal(layout.byName.get('a').live, true, 'nothing is greyed until everything is read')
  assert.equal(layout.byName.has('empty'), false)
  assert.deepEqual(layout.buds.get('a9') ?? layout.buds.get('a2') ?? [], layout.byId.has('a2') ? ['empty'] : [])
  const zz = layout.byId.get('32zz')
  assert.ok(zz && zz.commit === null && zz.slot < layout.byId.get('32b').slot, 'an unread fork Layer still opens the trunk')
  assert.equal(layout.layers.every((r) => r.n === undefined), true, 'Layers are not numbered until main is read in full')
})

test('treeLayout: a deep, wide history lays out without recursion', () => {
  const branches = [branch('main', '32a')]
  const pages = { main: page([commit('32a', 'main', null, { at: null })], null, '32a') }
  let from = '32a'
  for (let i = 0; i < 40; i++) {
    const ids = Array.from({ length: 500 }, (_, k) => `c${i * 1000 + k + 1}`)
    branches.push(branch(`w${i}`, ids.at(-1), from))
    pages[`w${i}`] = run(`w${i}`, ids, from)
    from = ids[250]
  }
  const started = Date.now()
  const layout = treeLayout(branches, pages)
  assert.equal(layout.rows.length, 20001)
  assert.ok(Date.now() - started < 2000)
  for (const r of layout.rows) if (r.commit?.parent) assert.ok(layout.byId.get(r.commit.parent).slot < r.slot)
})

test('extendPage: a moved head keeps the pages loaded below the old head', () => {
  const chain = (from, to, name = 'w') => Array.from({ length: from - to + 1 }, (_, i) => commit(`c${from - i}`, name, from - i > 1 ? `c${from - i - 1}` : 'l1'))
  const old = page(chain(60, 1), null, 'c60')
  // Three new Commits: the fresh first page (20) reaches c60.
  const fresh = page(chain(63, 44), 'c43', 'c63')
  const joined = extendPage(fresh, old)
  assert.equal(joined.head, 'c63')
  assert.equal(joined.items.length, 63)
  assert.deepEqual(joined.items.slice(0, 4).map((c) => c.id), ['c63', 'c62', 'c61', 'c60'])
  assert.equal(joined.next, null, 'the old pages were complete')
  // Too many new Commits for one page: the caller has to read on.
  assert.equal(extendPage(page(chain(100, 81), 'c80', 'c100'), old), undefined)
  // Same head: the deeper of the two stays.
  assert.equal(extendPage(page(chain(60, 41), 'c40', 'c60'), old), old)
  // The old read was shallower than the fresh page's tail: the fresh tail and cursor win.
  const shallow = page(chain(60, 59), 'c58', 'c60')
  const deeper = extendPage(page(chain(61, 42), 'c41', 'c61'), shallow)
  assert.equal(deeper.items.length, 20)
  assert.equal(deeper.next, 'c41')
})

test('movedCount: counts without a read when the page tells it', () => {
  const b = (head, fork = 'l1') => branch('w', head, fork)
  const items = (ids) => ids.map((id, i) => commit(id, 'w', ids[i + 1] ?? 'l1'))
  assert.equal(movedCount({ head: 'c9', count: 9 }, undefined, b('c9')), 9, 'unchanged head')
  assert.equal(movedCount(undefined, undefined, b('l1')), 0, 'nothing saved since the fork')
  assert.equal(movedCount(undefined, page(items(['c3', 'c2', 'c1']), null, 'c3'), b('c3')), 3, 'a complete page')
  assert.equal(movedCount({ head: 'c40', count: 40 }, page(items(['c42', 'c41', 'c40', 'c39']), 'c38', 'c42'), b('c42')), 42, 'two new above the counted head')
  assert.equal(movedCount({ head: 'c10', count: 10 }, page(items(['c42', 'c41']), 'c40', 'c42'), b('c42')), null, 'the counted head is past the page: the host counts')
  assert.equal(movedCount(undefined, page(items(['c42', 'c41']), 'c40', 'c42'), b('c42')), null, 'nothing counted yet')
  assert.equal(movedCount(undefined, page(items(['c3']), null, 'c2'), b('c3')), null, 'a page of another head')
})

test('countedTotal: only when every Branch is counted at its head; a headless Branch never crashes it', () => {
  const branches = [branch('a', 'c2', 'l1'), { name: 'fresh', base: 'l1', forkedFrom: 'l1' }]
  assert.equal(countedTotal(branches, {}), null, 'a headless Branch with no count: no total (it used to read undefined.count)')
  assert.equal(countedTotal(branches, { a: { head: 'c2', count: 2 } }), null)
  assert.equal(countedTotal(branches, { a: { head: 'c2', count: 2 }, fresh: { head: null, count: 0 } }), 2)
  assert.equal(countedTotal(branches, { a: { head: 'c1', count: 1 }, fresh: { head: null, count: 0 } }), null, 'a moved head waits for its count')
  assert.equal(countedTotal([], {}), null)
})


test('treeLayout: a Branch keeps the lane it had while the history fills in', () => {
  const branches = [branch('main', '32a'), branch('a', 'a9', '32a'), branch('b', 'b8', '32a'), branch('c', 'c7', '32a')]
  const main = page([commit('32a', 'main', null, { at: null })], null, '32a')
  const before = treeLayout(branches, { main, a: page([commit('a9', 'a', 'a5')], 'a5', 'a9'), b: run('b', ['b6', 'b8'], '32a') })
  const lanes = new Map(before.branches.map((b) => [b.name, b.lane]))
  // Older checkpoints of a, and all of c, land: a and b stay where they were.
  const after = treeLayout(branches, { main, a: run('a', ['a1', 'a5', 'a9'], '32a'), b: run('b', ['b6', 'b8'], '32a'), c: run('c', ['c2', 'c7'], '32a') }, lanes)
  assert.equal(after.byName.get('a').lane, lanes.get('a'))
  assert.equal(after.byName.get('b').lane, lanes.get('b'))
  assert.ok(![lanes.get('a'), lanes.get('b')].includes(after.byName.get('c').lane))
})
