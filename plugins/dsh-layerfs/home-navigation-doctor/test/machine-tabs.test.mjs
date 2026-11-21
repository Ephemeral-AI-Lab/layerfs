// The machine page's tabs (src/client/machines/machine-tabs.ts): which tabs a
// page has, the Sandboxes tab's cards (record + sampler values + disk, by
// filter and order) and what the Overview flags.
//   node --test test/machine-tabs.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtempSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

const root = fileURLToPath(new URL('..', import.meta.url))
const out = mkdtempSync(join(root, 'lib', '.test-machine-tabs-'))
await build({ absWorkingDir: root, stdin: { contents: "export * from './src/client/machines/machine-tabs.ts'", resolveDir: root, loader: 'ts' }, outfile: join(out, 'tabs.mjs'), bundle: true, format: 'esm', platform: 'node', target: 'node24', logLevel: 'warning' })
const T = await import(pathToFileURL(join(out, 'tabs.mjs')).href)
test.after(() => rmSync(out, { recursive: true, force: true }))

// 2.0 six states: `ready` runs; `gone` is broken `lost` (its daemon ended), `failed` broken with another reason
const STATE = { running: { state: 'ready' }, stopped: { state: 'stopped' }, gone: { state: 'broken', reason: 'lost' }, failed: { state: 'broken', reason: 'failed' } }
const sb = (name, state, extra = {}) => ({ name, ...STATE[state], machine: 'docker', project: 'qv2-store/qv2-probe', workspaces: 0, ...extra })
const needsYou = (b) => b.state === 'broken'

test('tabs: Overview, Sandboxes and Stores always; Images, Files and Worker log by what the machine and CLI have; no Usage or Disk tab (both live on the Overview)', () => {
  assert.deepEqual(T.machineTabs({ images: false, files: false, log: false }), ['overview', 'sandboxes', 'stores'])
  assert.deepEqual(T.machineTabs({ images: true, files: true, log: true }), ['overview', 'images', 'files', 'sandboxes', 'stores', 'log'])
  assert.deepEqual(T.machineTabs({ images: false, files: true, log: false }), ['overview', 'files', 'sandboxes', 'stores'])
  assert.deepEqual(T.machineTabs({ images: true, files: false, log: false }), ['overview', 'images', 'sandboxes', 'stores'])
})

test('Sandbox cards: those that need you first, joined with sampler values and disk by name; counts per filter', () => {
  const records = [sb('a', 'running'), sb('b', 'gone'), sb('c', 'running'), sb('d', 'stopped')]
  const metrics = [{ name: 'a', closedAt: null, memory: 10 }, { name: 'z', closedAt: 1794824100000 }, { name: 'y', closedAt: 1794824200000 }, { name: 'c', closedAt: null, memory: 30 }]
  const space = [{ name: 'b', bytes: 261 * 2 ** 20 }, { name: 'a', bytes: 26 * 2 ** 20 }, { name: 'd', bytes: 90 * 2 ** 20 }]
  const all = T.sandboxCards(records, metrics, space, needsYou, 'all', 'listed')
  assert.deepEqual(all.cards.map((c) => c.name), ['b', 'a', 'c', 'd'], 'gone first, then as listed')
  assert.deepEqual(all.counts, { all: 4, running: 2, attention: 1, closed: 2 })
  assert.equal(all.cards[1].metrics.memory, 10)
  assert.equal(all.cards[0].space.bytes, 261 * 2 ** 20)
  assert.equal(all.cards[2].space, null, 'no disk row: null, never 0')
  assert.ok(all.cards.every((c) => c.record), 'All holds only recorded Sandboxes')
  assert.deepEqual(T.sandboxCards(records, metrics, space, needsYou, 'running', 'listed').cards.map((c) => c.name), ['a', 'c'])
  assert.deepEqual(T.sandboxCards(records, metrics, space, needsYou, 'attention', 'listed').cards.map((c) => c.name), ['b'])
  const closed = T.sandboxCards(records, metrics, space, needsYou, 'closed', 'name').cards
  assert.deepEqual(closed.map((c) => [c.name, c.record]), [['y', null], ['z', null]], 'closed: the most recently closed first')
  // without the calls: cards from the records alone
  assert.deepEqual(T.sandboxCards(records, [], null, needsYou, 'all', 'listed').cards.map((c) => [c.name, c.metrics, c.space]), [['b', null, null], ['a', null, null], ['c', null, null], ['d', null, null]])
})

test('Sandbox cards: orders keep those that need you first; an unknown size sorts last', () => {
  const records = [sb('c', 'running'), sb('a', 'running'), sb('g', 'gone'), sb('b', 'running')]
  const metrics = [{ name: 'a', closedAt: null, memory: 5 }, { name: 'b', closedAt: null, memory: null }, { name: 'c', closedAt: null, memory: 50 }]
  const space = [{ name: 'a', bytes: 300 }, { name: 'b', bytes: null, reason: 'deadline' }, { name: 'c', bytes: 100 }]
  const order = (sort) => T.sandboxCards(records, metrics, space, needsYou, 'all', sort).cards.map((c) => c.name)
  assert.deepEqual(order('listed'), ['g', 'c', 'a', 'b'])
  assert.deepEqual(order('name'), ['g', 'a', 'b', 'c'])
  assert.deepEqual(order('disk'), ['g', 'a', 'c', 'b'])
  assert.deepEqual(order('memory'), ['g', 'c', 'a', 'b'])
})

test('Overview: gone Sandboxes and what LayerFS measured for them; other Sandboxes that need you', () => {
  const records = [sb('a', 'running'), sb('b', 'gone'), sb('c', 'gone'), sb('d', 'failed')]
  assert.deepEqual(T.sandboxAttention(records, [{ name: 'b', bytes: 100 }, { name: 'c', bytes: null, reason: 'deadline' }], needsYou), { gone: 2, goneBytes: 100, needsYou: 1 })
  assert.deepEqual(T.sandboxAttention(records, null, needsYou), { gone: 2, goneBytes: null, needsYou: 1 }, 'without machine.space: counted, no size')
  assert.deepEqual(T.sandboxAttention([sb('a', 'running')], [], needsYou), { gone: 0, goneBytes: null, needsYou: 0 })
})

test('the Overview Sandbox memory counts read values only', () => {
  const row = (memoryCurrent, extra = {}) => ({ gap: null, row: { live: true, usage: { memoryCurrent } }, ...extra })
  assert.equal(T.sandboxMemory([row(10), row(5), row(null), row(7, { gap: 'Timeout' }), { gap: null, row: { live: false, usage: { memoryCurrent: 99 } } }]), 15)
  assert.equal(T.sandboxMemory([row(null)]), null)
  assert.equal(T.sandboxMemory([]), null)
})

test('the Overview processor figure shows small shares exactly', () => {
  assert.equal(T.cpuShare(0), '0')
  assert.equal(T.cpuShare(0.0004), '<0.1')
  assert.equal(T.cpuShare(0.004), '0.4')
  assert.equal(T.cpuShare(0.0961), '9.6')
  assert.equal(T.cpuShare(0.234), '23')
  assert.equal(T.cpuShare(1.2), '100')
})
