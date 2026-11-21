import assert from 'node:assert/strict'
import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import test from 'node:test'
import { buildHome } from '../tools/perf-fixture.mjs'
import { pinned } from './routes-helper.mjs'

test('live: the tier S fixture counts history Commits from the pinned CLI', { skip: !pinned() && 'no pinned CLI' }, async (t) => {
  const dir = mkdtempSync(join(tmpdir(), 'dsh-tier-s-'))
  t.after(() => rmSync(dir, { recursive: true, force: true }))
  const manifest = await buildHome({ tier: 'S', dir, log: () => {} })
  // Each project's recipe writes 22 changes and makes two divergent merges.
  // Forks and publications return a commit ID too, but add no history Commit.
  assert.equal(manifest.commits, 48)
})
