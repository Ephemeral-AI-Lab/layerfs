// The Sandbox page's client model: the daemon line and overlay helpers
// (src/client/machines/model.ts) and the terminals' layout, order, history
// and transcript (src/client/machines/terminal.ts). The command runner and
// its routes are in test/sandbox-commands.test.mjs.
//   node --test test/sandbox-page.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtempSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

const root = fileURLToPath(new URL('..', import.meta.url))
const out = mkdtempSync(join(root, 'lib', '.test-sandbox-page-'))
await build({
  absWorkingDir: root,
  stdin: { contents: "export { daemonLook, daemonSlots, overlayDirty, overlayCount, workspaceChanges } from './src/client/machines/model.ts'; export * from './src/client/machines/terminal.ts'; export * from './src/client/machines/workspace-usage.ts'", resolveDir: root, loader: 'ts' },
  outfile: join(out, 'page.mjs'), bundle: true, format: 'esm', platform: 'node', target: 'node24', external: ['@deepseek-ai/*'], logLevel: 'warning',
})
const M = await import(pathToFileURL(join(out, 'page.mjs')).href)
test.after(() => rmSync(out, { recursive: true, force: true }))

test('daemon line and overlay helpers', () => {
  // the 2.0 six states: ready reaches the daemon; broken (lost, unreachable) is down; stopped or blocked is not running
  const base = { state: 'ready', workspaces: [] }
  assert.equal(M.daemonLook({ ...base }).key, 'sbDaemonReachable')
  assert.equal(M.daemonLook({ ...base, state: 'broken', reason: 'lost' }).tone, 'danger')
  assert.equal(M.daemonLook({ ...base, state: 'stopped' }).key, 'sbDaemonStopped')
  assert.equal(M.daemonLook({ ...base, state: 'blocked', blockedBy: { machine: 'lima1', reason: 'stopped' } }).key, 'sbDaemonStopped')
  assert.equal(M.daemonLook({ ...base, unread: true }).key, 'sbDaemonUnknown')
  assert.deepEqual(M.daemonSlots({ ...base, daemonCapacity: { admission: { pool_vacant: 3, pool_running: 1, reserved_vacant: 1, reserved_running: 0 } } }), { free: 4, total: 5 })
  assert.equal(M.daemonSlots(base), null)
  // an in-place edit: one inode, no directory entry, still a change (the Sandboxes table's status)
  const edited = { ws: 'sb:a', branch: 'b', path: '/w', dirty: { inodes: 1, entries: 0 } }
  assert.equal(M.overlayDirty(edited.dirty), true)
  assert.equal(M.overlayCount(edited.dirty), 1)
  assert.equal(M.workspaceChanges(edited).key, 'wsUncommitted')
  assert.equal(M.workspaceChanges({ ...edited, dirty: { inodes: 0, entries: 0 } }).key, 'wsClean')
})

test('Workspaces keep creation order: numeric by mount path', () => {
  const ws = (n, name = null) => ({ ws: `sb:${name ?? `/r/workspaces/${n}`}`, name, branch: 'b', path: `/r/workspaces/${n}` })
  const ordered = M.orderWorkspaces([ws(10, 'j'), ws(2, 'b'), ws(1, 'a'), ws(3)])
  assert.deepEqual(ordered.map((w) => w.path.split('/').at(-1)), ['1', '2', '3', '10'])
  assert.equal(M.mountLabel('/Users/me/.layerfs/machine/native/run/lfs-1-box/workspaces/3'), 'workspaces/3')
  assert.equal(M.mountLabel('/w'), 'w')
})

test('page layout: one, two, 2×2 with a New tile, pages of four, maximize, full', () => {
  const ids = (n) => Array.from({ length: n }, (_, i) => `w${i + 1}`)
  const grid = (l) => (l.columns === 1 && l.rows === 1 ? 'one' : l.columns === 2 && l.rows === 1 ? 'two' : `${l.columns}x${l.rows}`)
  const shape = (l) => [grid(l), l.slots.join(','), l.page, l.pages]
  // without a measured area: the 2×2 page
  assert.deepEqual(shape(M.pageLayout([], 0, null, true)), ['one', 'new', 0, 1])
  assert.deepEqual(shape(M.pageLayout([], 0, null, false)), ['one', 'blank', 0, 1])
  assert.deepEqual(shape(M.pageLayout(ids(1), 0, null, true)), ['one', 'w1', 0, 1])
  assert.deepEqual(shape(M.pageLayout(ids(2), 0, null, true)), ['two', 'w1,w2', 0, 1])
  assert.deepEqual(shape(M.pageLayout(ids(3), 0, null, true)), ['2x2', 'w1,w2,w3,new', 0, 1])
  assert.deepEqual(shape(M.pageLayout(ids(3), 0, null, false)), ['2x2', 'w1,w2,w3,blank', 0, 1])
  assert.deepEqual(shape(M.pageLayout(ids(4), 0, null, true)), ['2x2', 'w1,w2,w3,w4', 0, 1])
  assert.deepEqual(shape(M.pageLayout(ids(5), 0, null, true)), ['2x2', 'w1,w2,w3,w4', 0, 2])
  assert.deepEqual(shape(M.pageLayout(ids(5), 1, null, true)), ['2x2', 'w5,new,blank,blank', 1, 2])
  assert.deepEqual(shape(M.pageLayout(ids(9), 2, null, true)), ['2x2', 'w9,new,blank,blank', 2, 3])
  // a page past the end clamps; a maximized Workspace shows alone
  assert.deepEqual(shape(M.pageLayout(ids(5), 7, null, true)), ['2x2', 'w5,new,blank,blank', 1, 2])
  assert.deepEqual(shape(M.pageLayout(ids(5), 0, 'w3', true)), ['one', 'w3', 0, 2])
  assert.deepEqual(shape(M.pageLayout(ids(5), 0, 'gone', true)), ['2x2', 'w1,w2,w3,w4', 0, 2])
  assert.deepEqual(shape(M.pageLayout(ids(16), 3, null, false)), ['2x2', 'w13,w14,w15,w16', 3, 4])
  assert.equal(M.pageOf(ids(9), 'w5'), 1)
  assert.equal(M.pageOf(ids(9), 'w9'), 2)
  assert.equal(M.pageOf(ids(9), 'nope'), 0)
})

test('page layout follows the area: as many panes as fit, no choice of layout', () => {
  const ids = (n) => Array.from({ length: n }, (_, i) => `w${i + 1}`)
  // each pane at least 380 × 220 with 8px gaps, at most 4 × 3
  assert.deepEqual(M.paneFit(370, 300), { columns: 1, rows: 1 })
  assert.deepEqual(M.paneFit(768, 448), { columns: 2, rows: 2 })
  assert.deepEqual(M.paneFit(767, 447), { columns: 1, rows: 1 })
  assert.deepEqual(M.paneFit(1156, 700), { columns: 3, rows: 3 })
  assert.deepEqual(M.paneFit(4000, 3000), { columns: 4, rows: 3 })
  assert.deepEqual(M.paneFit(0, 0), { columns: 1, rows: 1 })
  assert.deepEqual(M.paneFit(Number.NaN, 500), { columns: 1, rows: 2 })
  const wide = { columns: 3, rows: 2 }
  const at = (n, page = 0, max = null, add = true) => {
    const l = M.pageLayout(ids(n), page, max, add, wide)
    return [l.columns, l.rows, l.slots.join(','), l.page, l.pages]
  }
  // one page: only the columns and rows its Workspaces need
  assert.deepEqual(at(2), [2, 1, 'w1,w2', 0, 1])
  assert.deepEqual(at(3), [3, 1, 'w1,w2,w3', 0, 1])
  assert.deepEqual(at(4), [3, 2, 'w1,w2,w3,w4,new,blank', 0, 1])
  assert.deepEqual(at(6), [3, 2, 'w1,w2,w3,w4,w5,w6', 0, 1])
  // more pages keep the full grid so panes keep their size
  assert.deepEqual(at(7), [3, 2, 'w1,w2,w3,w4,w5,w6', 0, 2])
  assert.deepEqual(at(7, 1), [3, 2, 'w7,new,blank,blank,blank,blank', 1, 2])
  assert.deepEqual(at(7, 1, null, false), [3, 2, 'w7,blank,blank,blank,blank,blank', 1, 2])
  assert.deepEqual(at(7, 0, 'w7'), [1, 1, 'w7', 0, 2])
  // a narrow area: one pane a page
  assert.deepEqual(M.pageLayout(ids(3), 1, null, true, { columns: 1, rows: 1 }).slots, ['w2'])
  assert.equal(M.pageOf(ids(16), 'w7', wide), 1)
  assert.equal(M.pageOf(ids(16), 'w13', wide), 2)
})

test('Workspace CPU: a rate between reads, a new series when the group is made again', () => {
  const u = (cpuUsec, memoryCurrent = 1) => ({ cpuUsec, memoryCurrent, memoryPeak: null, pids: 1 })
  let tr = M.step(undefined, u(1_000_000), 10_000)
  assert.deepEqual(tr.rates, [])
  assert.equal(M.latestRate(tr), null)
  // 2 s of CPU over 1 s: two processors
  tr = M.step(tr, u(3_000_000), 11_000)
  assert.equal(M.latestRate(tr), 2)
  assert.equal(M.cpuWords(M.latestRate(tr)), '2.0')
  // a read too soon after the last adds nothing
  assert.equal(M.step(tr, u(9_000_000), 11_500), tr)
  // the count went down: the group was made again, no negative rate
  tr = M.step(tr, u(500_000), 12_000)
  assert.equal(M.latestRate(tr), null)
  tr = M.step(tr, u(750_000), 13_000)
  assert.equal(M.latestRate(tr), 0.25)
  assert.equal(M.cpuWords(0.25), '0.25')
  assert.equal(M.cpuWords(12.4), '12')
  // nothing runs (usage null): a gap, then a fresh start
  tr = M.step(tr, null, 14_000)
  assert.equal(M.latestRate(tr), null)
  tr = M.step(tr, u(100), 15_000)
  assert.equal(M.latestRate(tr), null)
  // reads a minute apart (a hidden page) are not one series
  assert.equal(M.latestRate(M.step(tr, u(60_100), 80_000)), null)
  assert.equal(M.sparkPoints([1], 60, 10), null)
  assert.equal(M.sparkPoints([0, null, 2], 60, 10), '0.0,10.0 60.0,0.0')
  // only Workspaces whose worker reports usage are tracked
  const tracked = M.recordUsage([{ ws: 'sb:a', branch: 'b', path: '/w/1', usage: u(5) }, { ws: 'sb:b', branch: 'b', path: '/w/2' }], 1_000)
  assert.deepEqual([...tracked.keys()], ['sb:a'])
  for (let i = 0; i < 40; i++) tr = M.step(tr, u(100 + i), 100_000 + i * 2000)
  assert.equal(tr.rates.length, M.SERIES_LENGTH)
})

test('history steps like a shell: older on ↑, back to the draft on ↓, repeats once', () => {
  const cmds = ['ls', 'ls', 'make', 'cd src']
  let s = M.historyStep(cmds, -1, -1, 'dra')
  assert.deepEqual(s, { cursor: 0, text: 'ls' })
  s = M.historyStep(cmds, s.cursor, -1, 'dra')
  assert.deepEqual(s, { cursor: 1, text: 'make' })
  s = M.historyStep(cmds, s.cursor, -1, 'dra')
  assert.deepEqual(s, { cursor: 2, text: 'cd src' })
  assert.deepEqual(M.historyStep(cmds, 2, -1, 'dra'), { cursor: 2, text: 'cd src' })
  assert.deepEqual(M.historyStep(cmds, 0, 1, 'dra'), { cursor: -1, text: 'dra' })
  assert.deepEqual(M.historyStep(cmds, -1, 1, 'dra'), { cursor: -1, text: 'dra' })
  assert.deepEqual(M.historyStep([], -1, -1, 'x'), { cursor: -1, text: 'x' })
})

test('transcript: oldest first, cleared rows hidden, tails, binary and running time', () => {
  const e = (id, startedAt, extra = {}) => ({ id, ws: 'sb:a', sandbox: 'sb', command: id, state: 'exited', startedAt, finishedAt: startedAt + 40, exitCode: 0, stdout: '', stderr: '', stdoutBytes: 0, stderrBytes: 0, error: null, ...extra })
  const rows = M.transcript([
    e('c', 300, { state: 'running', finishedAt: null, exitCode: null }),
    e('b', 200, { stdout: 'tail', stdoutBytes: 70000, stderrBinary: true, stderr: '', stderrBytes: 12 }),
    e('a', 100),
  ], 150, 1300)
  assert.deepEqual(rows.map((r) => r.id), ['b', 'c'])
  assert.equal(rows[0].stdoutOmitted, 69996)
  assert.equal(rows[0].stderrBinary, true)
  assert.equal(rows[0].ms, 40)
  assert.equal(rows[1].ms, 1000)
  assert.equal(M.duration(40), '40ms')
  assert.equal(M.duration(18200), '18s')
  assert.equal(M.duration(72000), '1m 12s')
  assert.equal(M.duration(null), '')
})

test('pane state outlives a pane and can be forgotten', () => {
  let calls = 0
  const off = M.paneStore.subscribe(() => calls++)
  M.paneStore.patch('sb:a', { draft: 'make' })
  assert.equal(M.paneStore.get('sb:a').draft, 'make')
  assert.equal(M.paneStore.get('sb:a').cursor, -1)
  M.paneStore.forget('sb:a')
  assert.equal(M.paneStore.get('sb:a').draft, '')
  off()
  assert.equal(calls, 2)
})

test('a mounted Workspace opens by the handle LayerFS answers, else by the name it gives', () => {
  assert.equal(M.mountedWorkspace('box', { workspace: 'box:agent', path: '/run/box/workspaces/1' }, '', 'agent'), 'box:agent')
  assert.equal(M.mountedWorkspace('box', { workspace: 'box:mine' }, 'mine', 'agent'), 'box:mine')
  // never the mount's folder path
  assert.equal(M.mountedWorkspace('box', { path: '/run/box/workspaces/1' }, '', 'Feat/X'), 'box:feat-x')
  assert.equal(M.mountedWorkspace('box', {}, 'mine', 'agent'), 'box:mine')
})
