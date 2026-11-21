// The log pane's client model (src/client/machines/log-model.ts): the bounded
// window of loaded lines, pieces of a long line joined, the "log replaced"
// divider, a refused read kept as stale, and the poll interval's back-off.
//   node --test test/log-model.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtempSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

const root = fileURLToPath(new URL('..', import.meta.url))
const out = mkdtempSync(join(root, 'lib', '.test-log-model-'))
await build({
  absWorkingDir: root,
  stdin: { contents: "export * from './src/client/machines/log-model.ts'", resolveDir: root, loader: 'ts' },
  outfile: join(out, 'model.mjs'), bundle: true, format: 'esm', platform: 'node', target: 'node24', logLevel: 'warning',
})
const M = await import(pathToFileURL(join(out, 'model.mjs')).href)
test.after(() => rmSync(out, { recursive: true, force: true }))

const view = (texts, more = {}) => ({ lines: texts.map((t) => (typeof t === 'string' ? { text: t } : t)), cursor: 'c1', before: null, hasMore: false, observedAt: 1000, stale: false, ...more })
const texts = (state) => state.rows.map((r) => (r.kind === 'replaced' ? '---' : r.text))

test('a tail read replaces the window; a forward read appends', () => {
  let s = M.applyTail(M.emptyLog(), view(['a', 'b'], { before: 'b0' }))
  assert.deepEqual(texts(s), ['a', 'b'])
  assert.equal(s.cursor, 'c1'); assert.equal(s.before, 'b0'); assert.equal(s.loaded, true); assert.equal(s.live, true); assert.equal(s.readAt, 1000)
  s = M.applyForward(s, view(['c'], { cursor: 'c2', observedAt: 2000 }))
  assert.deepEqual(texts(s), ['a', 'b', 'c'])
  assert.equal(s.cursor, 'c2'); assert.equal(s.before, 'b0'); assert.equal(s.readAt, 2000)
  // an empty forward answer with no cursor keeps the cursor it has
  s = M.applyForward(s, view([], { cursor: null }))
  assert.equal(s.cursor, 'c2')
  assert.deepEqual(new Set(s.rows.map((r) => r.id)).size, 3, 'row ids are unique')
  // a later tail read starts over
  s = M.applyTail(s, view(['z'], { cursor: 'c9' }))
  assert.deepEqual(texts(s), ['z'])
})

// A real `logs.*` answer always carries a cursor: a log file that is not there
// yet answers `c1.0.0.0.0`, and a read after it reads the file from its start.
test('a log that does not exist yet: the empty tail has a cursor, and the first lines arrive as a forward read', () => {
  let s = M.applyTail(M.emptyLog(), view([], { cursor: 'c1.0.0.0.0' }))
  assert.deepEqual(texts(s), [])
  assert.equal(s.cursor, 'c1.0.0.0.0', 'the pane follows from this cursor: no second tail read')
  assert.equal(s.loaded, true); assert.equal(s.live, true); assert.equal(s.before, null); assert.equal(s.stale, null)
  // nothing yet
  s = M.applyForward(s, view([], { cursor: 'c1.0.0.0.0', observedAt: 2000 }))
  assert.deepEqual(texts(s), []); assert.equal(s.cursor, 'c1.0.0.0.0'); assert.equal(s.readAt, 2000)
  // the file is there: its first lines, with no "log replaced" divider
  s = M.applyForward(s, view(['worker started', 'ready'], { cursor: 'c1.0.2a.9f.40', observedAt: 3000 }))
  assert.deepEqual(texts(s), ['worker started', 'ready'])
  assert.equal(s.cursor, 'c1.0.2a.9f.40'); assert.ok(s.rows.every((r) => r.kind !== 'replaced'))
  s = M.applyForward(s, view(['next'], { cursor: 'c1.0.30.9f.40' }))
  assert.deepEqual(texts(s), ['worker started', 'ready', 'next'])
})

test('pieces of a long line are one visual line, within a read and across reads', () => {
  let s = M.applyTail(M.emptyLog(), view(['a', { text: 'long-', continued: true }, { text: 'er-', continued: true }, { text: 'line' }, 'b', { text: 'open-', continued: true }]))
  assert.deepEqual(texts(s), ['a', 'long-er-line', 'b', 'open-'])
  assert.equal(s.rows.at(-1).open, true)
  const id = s.rows.at(-1).id
  s = M.applyForward(s, view([{ text: 'still-', continued: true }], { before: 'x' }))
  assert.deepEqual(texts(s), ['a', 'long-er-line', 'b', 'open-still-'])
  assert.equal(s.marks.length, 0, 'a piece that continues a shown line starts no line: no mark')
  s = M.applyForward(s, view(['closed', 'next']))
  assert.deepEqual(texts(s), ['a', 'long-er-line', 'b', 'open-still-closed', 'next'])
  assert.equal(s.rows[3].id, id, 'the joined line keeps its row')
  assert.equal(s.rows[3].open, false)
})

test('earlier lines go in front and join a piece that continues into the first line shown', () => {
  let s = M.applyTail(M.emptyLog(), view(['tail-of-long', 'y'], { before: 'b2' }))
  s = M.applyEarlier(s, view(['w', { text: 'head-of-long-', continued: true }], { before: 'b1', cursor: 'ignored' }))
  assert.deepEqual(texts(s), ['w', 'head-of-long-tail-of-long', 'y'])
  assert.equal(s.before, 'b1'); assert.equal(s.cursor, 'c1', 'an earlier read never moves the forward cursor'); assert.equal(s.live, true)
  s = M.applyEarlier(s, view(['v'], { before: null }))
  assert.deepEqual(texts(s), ['v', 'w', 'head-of-long-tail-of-long', 'y'])
  assert.equal(s.before, null)
})

test('the window is bounded: the top is cut at a mark when one fits, the bottom when earlier lines push the end out', () => {
  const many = (n, tag) => Array.from({ length: n }, (_, i) => `${tag}${i}`)
  let s = M.applyTail(M.emptyLog(), view(many(1000, 't'), { before: 'b-tail' }))
  for (let i = 0; i < 5; i++) s = M.applyForward(s, view(many(1000, `f${i}-`), { cursor: `c${i}`, before: `m${i}` }))
  assert.equal(s.rows.length, 5000, 'cut at the first mark past the bound')
  assert.equal(s.rows[0].text, 'f0-0'); assert.equal(s.before, 'm0'); assert.equal(s.cutTop, false)
  // with no mark to cut at, the top is cut anywhere and says so
  let bare = M.applyTail(M.emptyLog(), view(many(1000, 't'), { before: 'b-tail' }))
  for (let i = 0; i < 5; i++) bare = M.applyForward(bare, view(many(1000, `f${i}-`), { cursor: `c${i}` }))
  assert.equal(bare.rows.length, M.MAX_ROWS); assert.equal(bare.before, null); assert.equal(bare.cutTop, true)
  // earlier lines past the bound: the end leaves the window, which is then not live and has no forward cursor
  let back = M.applyTail(M.emptyLog(), view(many(1000, 't'), { before: 'b5' }))
  for (let i = 4; i >= 0; i--) back = M.applyEarlier(back, view(many(1000, `e${i}-`), { before: i ? `b${i}` : null }))
  assert.equal(back.rows.length, M.MAX_ROWS); assert.equal(back.rows[0].text, 'e0-0'); assert.equal(back.live, false); assert.equal(back.cursor, null)
  // a few very long lines are bounded by characters
  const big = 'x'.repeat(1024 * 1024)
  let fat = M.applyTail(M.emptyLog(), view([big, big, big]))
  fat = M.applyForward(fat, view([big, big, big]))
  assert.ok(fat.rows.reduce((n, r) => n + r.text.length, 0) <= M.MAX_CHARS)
  assert.ok(fat.rows.length >= 1)
})

test('a replaced log: a divider, then the new log; nothing is shown twice', () => {
  let s = M.applyTail(M.emptyLog(), view(['old1', { text: 'old-open', continued: true }], { before: 'b-old' }))
  s = M.applyReplaced(s, view(['new1', 'new2'], { cursor: 'n1', before: null }))
  assert.deepEqual(texts(s), ['old1', 'old-open', '---', 'new1', 'new2'])
  assert.equal(s.rows[1].open, false, 'the old log\'s open line is closed')
  assert.equal(s.cursor, 'n1'); assert.equal(s.before, null); assert.equal(s.live, true)
  assert.equal(s.cutTop, true, 'the old log had earlier lines still to read: the top says they are gone, not "start of the log"')
  const whole = M.applyReplaced(M.applyTail(M.emptyLog(), view(['only'])), view(['new']))
  assert.equal(whole.cutTop, false, 'the old log was shown from its start: the top is still the start')
  s = M.applyForward(s, view(['new3'], { cursor: 'n2' }))
  assert.deepEqual(texts(s), ['old1', 'old-open', '---', 'new1', 'new2', 'new3'])
  assert.equal(M.logText(s.rows, 'replaced'), 'old1\nold-open\n--- replaced ---\nnew1\nnew2\nnew3')
})

test('a refused read keeps what is shown, marked stale; a first refusal shows the host\'s kept tail', () => {
  let s = M.applyTail(M.emptyLog(), view(['a']))
  s = M.applyRefused(s, view([], { stale: { at: 900, code: 'Unavailable', message: 'down' }, observedAt: 3000 }))
  assert.deepEqual(texts(s), ['a']); assert.equal(s.stale.code, 'Unavailable'); assert.equal(s.readAt, 1000)
  s = M.applyForward(s, view(['b']))
  assert.equal(s.stale, null)
  let first = M.applyRefused(M.emptyLog(), view(['kept'], { stale: { at: 500, code: 'Busy', message: 'full' } }))
  assert.deepEqual(texts(first), ['kept']); assert.equal(first.readAt, 500); assert.equal(first.loaded, true)
  first = M.applyRefused(M.emptyLog(), view([], { stale: { at: null, code: 'Unsupported', message: 'no' } }))
  assert.equal(first.loaded, true); assert.equal(first.rows.length, 0); assert.equal(first.readAt, null)
})

test('a refused "Earlier lines" read: InvalidInput ends earlier paging; another refusal keeps it', () => {
  const s = M.applyTail(M.emptyLog(), view(['a'], { before: 'b0' }))
  const gone = M.applyEarlierRefused(s, view([], { before: 'b0', stale: { at: null, code: 'InvalidInput', message: 'rotated' } }))
  assert.equal(gone.before, null); assert.equal(gone.cutTop, true); assert.equal(gone.stale.code, 'InvalidInput'); assert.deepEqual(texts(gone), ['a'])
  const busy = M.applyEarlierRefused(s, view([], { before: 'b0', stale: { at: null, code: 'Busy', message: 'full' } }))
  assert.equal(busy.before, 'b0'); assert.equal(busy.cutTop, false); assert.equal(busy.stale.code, 'Busy')
})

test('polling: 2 s, doubling to 10 s while nothing is new; final refusals stop it', () => {
  assert.equal(M.nextInterval(M.POLL_MS, true), 2000)
  assert.equal(M.nextInterval(2000, false), 4000)
  assert.equal(M.nextInterval(4000, false), 8000)
  assert.equal(M.nextInterval(8000, false), 10000)
  assert.equal(M.nextInterval(10000, false), 10000)
  assert.equal(M.nextInterval(10000, true), 2000)
  for (const code of ['Unsupported', 'NotFound', 'NoSuchSandbox', 'NoSuchMachine', 'CliIncompatible']) assert.equal(M.stopsPolling(code), true, code)
  for (const code of ['Unavailable', 'Busy', 'Timeout', 'Unknown', 'InvalidInput']) assert.equal(M.stopsPolling(code), false, code)
  assert.equal(M.stopsPolling('Unavailable', true), true, 'a lost or gone target: any refusal is final')
})

test('find: rows and parts, case-insensitive; the divider never matches', () => {
  const rows = [{ id: 1, kind: 'line', text: 'Error: one' }, { id: 2, kind: 'replaced', text: '' }, { id: 3, kind: 'line', text: 'fine' }, { id: 4, kind: 'line', text: 'an ERROR and an error' }]
  assert.deepEqual(M.findRows(rows, 'error'), [0, 3])
  assert.deepEqual(M.findRows(rows, ''), [])
  assert.deepEqual(M.matchParts('an ERROR and an error', 'error'), [{ text: 'an ', hit: false }, { text: 'ERROR', hit: true }, { text: ' and an ', hit: false }, { text: 'error', hit: true }])
  assert.deepEqual(M.matchParts('plain', ''), [{ text: 'plain', hit: false }])
  assert.ok(M.matchParts('a'.repeat(5000), 'a').length <= 401, 'very many matches are not cut into thousands of spans')
})
