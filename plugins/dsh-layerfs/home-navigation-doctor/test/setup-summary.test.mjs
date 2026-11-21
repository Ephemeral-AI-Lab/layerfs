// The Set up LayerFS modal's two sections as data (src/client/setup/summary.ts):
// the build row (one build, unstamped, mixed, incomplete, none, failed) and the
// Machines rows (order, unsupported left out, Install / Set up / Check / none).
//   node --test test/setup-summary.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtempSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

const root = fileURLToPath(new URL('..', import.meta.url))
const out = mkdtempSync(join(root, 'lib', '.test-setup-summary-'))
await build({
  absWorkingDir: root,
  stdin: { contents: "export * from './src/client/setup/summary.ts'; export { locales } from './src/client/setup/locale.ts'", resolveDir: root, loader: 'ts' },
  outfile: join(out, 'summary.mjs'), bundle: true, format: 'esm', platform: 'node', target: 'node24', logLevel: 'warning',
})
const { buildWords, platformRows, canRun, locales } = await import(pathToFileURL(join(out, 'summary.mjs')).href)
rmSync(out, { recursive: true, force: true })

const CLI = 'dd4a2cc40f5c5c23c903fcd8b1de90575663d71d'
const OTHER = '095f5887d9a1b2c3d4e5f60718293a4b5c6d7e8f'
const name = (k) => locales.en[k]
const piece = (verdict, commit = CLI) => ({ path: '/x', commit, dirty: false, verdict })
const status = (over = {}) => ({
  runtime: { ok: true, cli: '/rt/bin/layerfs', commit: CLI, version: '2.0.0', error: null, sdk: null, ...over.runtime },
  pieces: { cli: piece('matches'), daemon: piece('matches'), bundle: piece('matches'), ...over.pieces },
})

test('the build row: one build is Ready with no line, with or without a daemon', () => {
  assert.deepEqual(buildWords(status(), name), { state: 'ready', word: 'stateReady', line: null, values: {} })
  assert.equal(buildWords(status({ pieces: { daemon: null } }), name).line, null)
})

test('pieces built before build stamps pass with no line (Details says what they record)', () => {
  const b = buildWords(status({ pieces: { daemon: piece('unknown', null), bundle: piece('unknown') } }), name)
  assert.deepEqual([b.state, b.line], ['ready', null])
})

test('a piece from another build is named with both commits; two are summed up', () => {
  const one = buildWords(status({ pieces: { bundle: piece('other', OTHER) } }), name)
  assert.deepEqual(one, { state: 'mixed', word: 'sumMixed', line: 'buildMixedOne', values: { piece: 'Linux bundle', theirs: '095f5887' } })
  assert.equal(locales.en.buildMixedOne.replace('{piece}', one.values.piece).replace('{theirs}', one.values.theirs), 'Linux bundle from 095f5887')
  const both = buildWords(status({ pieces: { daemon: piece('other', OTHER), bundle: piece('other', OTHER) } }), name)
  assert.deepEqual([both.state, both.line, both.values], ['mixed', 'buildMixedBoth', {}])
})

test('a missing bundle or daemon is incomplete; no runtime, or a broken one, says so', () => {
  assert.deepEqual([buildWords(status({ pieces: { bundle: null } }), name).state, buildWords(status({ pieces: { bundle: null } }), name).line], ['incomplete', 'buildNoBundle'])
  assert.equal(buildWords(status({ pieces: { daemon: piece('missing', null) } }), name).line, 'buildNoDaemon')
  assert.deepEqual([buildWords(status({ runtime: { cli: null, ok: false } }), name).state, buildWords(status({ runtime: { cli: null, ok: false } }), name).line], ['none', null])
  const failed = buildWords(status({ runtime: { ok: false, error: { code: 'X', message: 'boom' } } }), name)
  assert.deepEqual([failed.state, failed.values], ['failed', { message: 'boom' }])
})

const platform = (platform, over = {}) => ({ platform, how: '', backend: null, kernel: null, sqlite: null, postgresql: { store_calls: '', sandbox: false, reason: null }, ready: true, missing: [], version: null, options: [], description: '', ...over })

test('machines: native first, then docker, lima, apple-container; what this computer never runs is left out', () => {
  const rows = platformRows([
    platform('apple-container', { ready: false, missing: ['tool:container'] }),
    platform('lima'),
    platform('microsandbox', { ready: false, missing: ['host:macos-or-linux'] }),
    platform('docker', { ready: false, missing: ['host:/var/run/docker.sock'] }),
    platform('native'),
  ], 'macos')
  assert.deepEqual(rows.map((r) => [r.platform, r.state, r.action]), [
    ['native', 'ready', null],
    ['docker', 'notRunning', 'check'],
    ['lima', 'available', null],
    ['apple-container', 'notInstalled', 'install'],
  ])
  assert.deepEqual(canRun(rows), { n: 2, total: 4 })
})

test('machines: a host fact to set up offers Set up; a missing bundle offers nothing here', () => {
  const rows = platformRows([platform('native'), platform('docker', { ready: false, missing: ['bundle:/b'] }), platform('lima', { ready: false, missing: ['host:/dev/fuse'] })], 'linux')
  assert.deepEqual(rows.map((r) => [r.platform, r.state, r.action]), [['native', 'ready', null], ['docker', 'needsBundle', null], ['lima', 'needsSetup', 'setup']])
})

test('machines: not ready with nothing reported offers Check, not an empty how-to', () => {
  const [row] = platformRows([platform('lima', { ready: false, missing: [] })], 'macos')
  assert.deepEqual([row.state, row.action], ['needsSetup', 'check'])
})

test('every key the sections name exists in en and zh', () => {
  const keys = ['sectionLayerfs', 'toFix', 'canRunCount', 'folderTitle', 'runtimeTitle', 'runtimeUnnamed', 'choose', 'details', 'buildMixedOne', 'buildMixedBoth', 'buildNoBundle', 'buildNoDaemon', 'buildIncomplete', 'buildFailed', 'daemonRecordShort', 'stateReady', 'stateAvailable', 'stateNotInstalled', 'stateNotRunning', 'stateNeedsSetup', 'stateNeedsBundle', 'check', 'install', 'setUp', 'machinesNone', 'sumMixed', 'sumNoRuntime', 'sumRuntimeFailed', 'terminal']
  for (const lang of ['en', 'zh']) for (const k of keys) assert.ok(typeof locales[lang][k] === 'string' && locales[lang][k].length > 0, `${lang}.${k}`)
})
