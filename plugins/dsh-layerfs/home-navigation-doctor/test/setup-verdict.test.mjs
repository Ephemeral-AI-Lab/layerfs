// The setup modal's piece words (src/client/setup/verdict.ts): LayerFS's
// unknown verdict split by what an old-build piece records, so a runtime built
// before build stamps says "Same commit" / "No build stamp" instead of a bare
// "Can't tell"; the en and zh dictionaries carry every key it names.
//   node --test test/setup-verdict.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtempSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

const root = fileURLToPath(new URL('..', import.meta.url))
const out = mkdtempSync(join(root, 'lib', '.test-setup-verdict-'))
await build({
  absWorkingDir: root,
  stdin: { contents: "export * from './src/client/setup/verdict.ts'; export { locales } from './src/client/setup/locale.ts'", resolveDir: root, loader: 'ts' },
  outfile: join(out, 'verdict.mjs'), bundle: true, format: 'esm', platform: 'node', target: 'node24', logLevel: 'warning',
})
const { verdictWords, locales } = await import(pathToFileURL(join(out, 'verdict.mjs')).href)
rmSync(out, { recursive: true, force: true })

const CLI = 'dd4a2cc40f5c5c23c903fcd8b1de90575663d71d'
const OTHER = '095f5887d9000000000000000000000000000000'

test('known verdicts keep their words', () => {
  assert.deepEqual(verdictWords({ verdict: 'matches', commit: CLI }, CLI), { tone: 'matches', key: 'vMatches' })
  assert.deepEqual(verdictWords({ verdict: 'other', commit: OTHER }, CLI), { tone: 'other', key: 'vOther' })
  assert.deepEqual(verdictWords({ verdict: 'missing', commit: null }, CLI), { tone: 'missing', key: 'vMissing' })
  assert.deepEqual(verdictWords(null, CLI), { tone: 'missing', key: 'vMissing' })
})

test('an unknown verdict says what the old-build piece records', () => {
  // the bundle: the CLI's commit, but no source.dirty
  assert.deepEqual(verdictWords({ verdict: 'unknown', commit: CLI }, CLI), { tone: 'unknown', key: 'vSameCommit' })
  // the daemon: no build stamp at all
  assert.deepEqual(verdictWords({ verdict: 'unknown', commit: null }, CLI), { tone: 'unknown', key: 'vNoStamp' })
  // the CLI's own commit unknown: nothing to compare with
  assert.deepEqual(verdictWords({ verdict: 'unknown', commit: CLI }, null), { tone: 'unknown', key: 'vUnknown' })
})

test('both dictionaries carry every verdict key', () => {
  for (const lang of ['en', 'zh']) {
    for (const key of ['vMatches', 'vOther', 'vUnknown', 'vMissing', 'vSameCommit', 'vNoStamp']) {
      assert.equal(typeof locales[lang][key], 'string', `${lang}.${key}`)
      assert.ok(locales[lang][key].length > 0, `${lang}.${key}`)
    }
  }
})
