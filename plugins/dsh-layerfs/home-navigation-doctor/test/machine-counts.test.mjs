// A machine's Stores and Sandboxes beyond the 20 names its record lists
// (review UR M4): the record's `stores_count` / `sandboxes_count`, and the
// whole store.list / sandbox.list rows on the machine.
//   node --test test/machine-counts.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdirSync, mkdtempSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

const root = fileURLToPath(new URL('..', import.meta.url))
mkdirSync(join(root, 'lib'), { recursive: true })
const out = mkdtempSync(join(root, 'lib', '.test-machine-counts-'))
await build({
  absWorkingDir: root, bundle: true, format: 'esm', platform: 'node', target: 'node24', logLevel: 'warning', outfile: join(out, 'm.mjs'),
  stdin: { contents: "export { inUse, fixAction, storesOn, sandboxesOn, heldStores, heldSandboxes } from './src/client/machines/model.ts'\nexport { machineOf } from './src/host/layerfs/machine-records.ts'\n", resolveDir: root, loader: 'ts' },
})
const M = await import(pathToFileURL(join(out, 'm.mjs')).href)
test.after(() => rmSync(out, { recursive: true, force: true }))

const names = (p, n) => Array.from({ length: n }, (_, i) => `${p}${i}`)

test('machineOf keeps the counts beside the cut name lists; inUse and a restart fix count all of them', () => {
  const m = M.machineOf({ machine: 'dev', platform: 'lima', state: 'broken', reason: 'outdated', fix: 'layerfs machine stop dev && layerfs machine start dev', stores: names('s', 20), stores_count: 23, sandboxes: names('b', 20), sandboxes_count: 31 })
  assert.equal(m.storesCount, 23)
  assert.equal(m.sandboxesCount, 31)
  assert.deepEqual(M.inUse(m), { stores: 23, sandboxes: 31 })
  assert.deepEqual(M.fixAction(m), { kind: 'forced', sandboxes: 31, line: 'layerfs machine stop dev && layerfs machine start dev' })
  const whole = M.machineOf({ machine: 'vm', platform: 'lima', state: 'ready', stores: ['a'], sandboxes: [] })
  assert.deepEqual([whole.storesCount, whole.sandboxesCount, M.inUse(whole)], [1, 0, { stores: 1, sandboxes: 0 }], 'a whole list is its own count')
})

test('storesOn / sandboxesOn: every listed row on the machine, plus a recorded name the list omits', () => {
  const m = M.machineOf({ machine: 'dev', platform: 'lima', state: 'ready', stores: names('s', 20), stores_count: 22, sandboxes: ['gone'], sandboxes_count: 1 })
  const view = {
    stores: [...names('s', 22).map((name) => ({ name, machine: 'dev', projects: 1, size: null })), { name: 'x', machine: 'other', projects: 0, size: null }],
    sandboxes: [...names('b', 25).map((name) => ({ name, machine: 'dev', project: 'p/q', workspaces: 0 })), { name: 'y', machine: 'other', project: 'p/q', workspaces: 0 }],
  }
  assert.equal(M.storesOn(view, m).length, 22)
  const boxes = M.sandboxesOn(view, m)
  assert.equal(boxes.length, 26, '25 listed and the recorded one the list omits')
  assert.ok(boxes.some((b) => b.name === 'gone' && b.project === null))
  assert.ok(!boxes.some((b) => b.machine === 'other'))
})
