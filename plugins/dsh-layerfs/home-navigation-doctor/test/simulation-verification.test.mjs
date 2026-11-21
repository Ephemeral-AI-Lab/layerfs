import { test } from 'node:test'
import assert from 'node:assert/strict'
import { build } from 'esbuild'
import { mkdtempSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
const root = fileURLToPath(new URL('..', import.meta.url))
const out = mkdtempSync(join(root, 'lib', '.test-simulation-verify-'))
await build({ absWorkingDir: root, entryPoints: ['src/host/simulations/verify.ts'], outfile: join(out, 'verify.mjs'), bundle: true, platform: 'node', format: 'esm', target: 'node24', external: ['@deepseek-ai/*'] })
const { simulationVerificationQuery, verifySimulation } = await import(pathToFileURL(join(out, 'verify.mjs')).href)
test.after(() => rmSync(out, { recursive: true, force: true }))

test('workload verifier filters only exact bound archive branches and preserves 2.0 cursor paging', async () => {
  const manifest = { store: 'fresh', project: 'billing', projectRef: 'fresh/billing-unique' }
  const journal = { records: [{ type: 'binding', sessionId: 'a-a' }] }
  const calls = []
  const query = simulationVerificationQuery(manifest, journal, async argv => {
    calls.push(argv)
    if (argv[0] !== 'branch') return { text: 'saved file' }
    const at = argv.indexOf('--cursor')
    if (at < 0) return { items: [{ branch: 'dsh.session.aa', commit: 'c1' }], next_cursor: 'k1' }
    return { items: [{ branch: 'main', commit: 'c2' }, { branch: 'dsh.session.unowned', commit: 'c3' }, { branch: 'release', commit: 'c4' }] }
  })
  // the verifier's own pages over the filtered inventory (the bound session's archive left out)
  assert.deepEqual(await query(['branch', 'list', 'fresh/billing', '--limit', '2']), { items: [{ branch: 'main', commit: 'c2' }, { branch: 'dsh.session.unowned', commit: 'c3' }], next_cursor: '2' })
  assert.deepEqual(await query(['branch', 'list', 'fresh/billing', '--limit', '2', '--cursor', '2']), { items: [{ branch: 'release', commit: 'c4' }] })
  assert.equal(calls.length, 2, 'LayerFS is listed once, by its cursor')
  assert.deepEqual(calls[1].slice(-2), ['--cursor', 'k1'])
  await query(['file', 'read', 'fresh/billing', 'main', '/invoice.md'])
  assert.equal(calls[2][2], manifest.projectRef)
})
test('verification cannot begin with an unconfirmed workload mutation', async () => {
  let queried = false
  await assert.rejects(verifySimulation({}, { steps: [{ key: 'unfinished' }] }, { receipts: {}, append: async () => {} }, async () => { queried = true }), /incomplete simulation step/)
  assert.equal(queried, false)
})
test('the project identity branch is internal to verification, other dsh.project names are not', async () => {
  const manifest = { store: 'fresh', project: 'billing', projectRef: 'fresh/billing-unique' }
  const id = '4ec17d97-180b-4a24-9a95-5661f186be8b'
  const query = simulationVerificationQuery(manifest, { records: [] }, async () => ({ items: [{ branch: 'main' }, { branch: `dsh.project.${id}` }, { branch: 'dsh.project.not-a-uuid' }] }))
  assert.deepEqual((await query(['branch', 'list', 'fresh/billing'])).items.map(branch => branch.branch), ['main', 'dsh.project.not-a-uuid'])
})
