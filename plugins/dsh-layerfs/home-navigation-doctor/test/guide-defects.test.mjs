// UI defects the UI guide's writers found (docs/guide/): the client halves.
// The host halves are in test/routes-features.test.mjs ("file compare",
// "file history and blame") and the Workspace handle in test/sandbox-page.test.mjs.
//   node --test test/guide-defects.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'
import { miniReact } from './mini-react.mjs'

const root = fileURLToPath(new URL('..', import.meta.url))
mkdirSync(join(root, 'lib'), { recursive: true })
const out = mkdtempSync(join(root, 'lib', '.test-guide-defects-'))
await build({
  absWorkingDir: root,
  stdin: {
    contents: [
      "export { compareFiles } from './src/client/files/BranchCompare.tsx'",
      "export { hunksOf } from './src/client/files/CommitDiff.tsx'",
      "export { pickedCompare, treeCounts } from './src/client/files/ProjectHistory.tsx'",
      "export { filesEn, filesZh } from './src/client/files/locale.ts'",
      "export { tagWords } from './src/client/logs/LogsPage.tsx'",
      "export { locales as logsLocales } from './src/client/logs/locale.ts'",
      "export { presetIdName } from './src/client/presets/labels.ts'",
      "export { locales as presetsLocales } from './src/client/presets/locale.ts'",
      "export { backupChip, placeWords } from './src/client/storage/model.ts'",
      "export { providerWords } from './src/client/storage/BucketPage.tsx'",
      "export { locales as storageLocales } from './src/client/storage/locale.ts'",
    ].join('\n'),
    resolveDir: root, loader: 'tsx',
  },
  outfile: join(out, 'guide.mjs'), bundle: true, format: 'esm', platform: 'node', target: 'node24', logLevel: 'error',
  jsx: 'automatic', loader: { '.module.css': 'local-css', '.css': 'css' }, plugins: [miniReact],
})
const M = await import(pathToFileURL(join(out, 'guide.mjs')).href)
test.after(() => rmSync(out, { recursive: true, force: true }))

const src = (path) => readFileSync(join(root, path), 'utf8')
const fill = (text, vars = {}) => text.replace(/\{(\w+)\}/g, (_, k) => String(vars[k] ?? `{${k}}`))
const tOf = (dict) => (key, vars) => fill(dict[key] ?? key, vars)

test('1: a modified file\'s hunk keeps its removed and kept lines on the before side', () => {
  const rows = [{ op: ' ', text: 'one', left_line: 1, right_line: 1 }, { op: '-', text: 'two', left_line: 2 }, { op: '+', text: 'three', right_line: 2 }]
  assert.deepEqual(M.hunksOf('/a.txt', rows, false), [{ path: '/a.txt', oldText: 'one\ntwo\n', newText: 'one\nthree\n' }])
  assert.deepEqual(M.hunksOf('/a.txt', [{ op: '+', text: 'hi', right_line: 1 }], true), [{ path: '/a.txt', oldText: null, newText: 'hi\n' }])
  // the client reads the side the host sends: before null is an added path, an object is a file that was there
  assert.match(src('src/client/files/CommitDiff.tsx'), /const added = r\.before === null \|\|/)
})

test('3: "Only owner changes" is offered only when LayerFS says per file whether owners changed', () => {
  const unknown = [{ path: '/a', type: 'file', attribution: null }, { path: '/d', type: 'dir', attribution: null }, { path: '/b', type: 'file', attribution: null }]
  assert.deepEqual(M.compareFiles(unknown, true), { ownersKnown: false, files: [unknown[0], unknown[2]] }, '2.0: every file, and no option')
  const known = [{ path: '/a', type: 'file', attribution: true }, { path: '/b', type: 'file', attribution: false }]
  assert.deepEqual(M.compareFiles(known, true), { ownersKnown: true, files: [known[0]] })
  assert.deepEqual(M.compareFiles(known, false).files, known)
})

test('5: a "Compare with…" pick ends at a Branch (its head) as well as at a version', () => {
  const picking = { branch: 'feat', pin: 'p1' }
  assert.deepEqual(M.pickedCompare(picking, { branch: 'main' }, 'h-main'), { from: picking, to: { branch: 'main' } })
  assert.deepEqual(M.pickedCompare(picking, { branch: 'feat', pin: 'p2' }, 'h-feat'), { from: picking, to: { branch: 'feat', pin: 'p2' } })
  assert.equal(M.pickedCompare(picking, { branch: 'feat', pin: 'p1' }, 'h-feat'), null, 'the version picked first')
  assert.equal(M.pickedCompare(picking, { branch: 'feat' }, 'p1'), null, 'a Branch whose head is the version picked first')
  // the Branch tag and the finder go through the pick while picking
  assert.match(src('src/client/files/ProjectHistory.tsx'), /const openBranch = \(name: string\) => \{\n\s+\/\/[^\n]*\n\s+if \(picking\) \{ setFinderOpen\(false\); if \(completePick\(\{ branch: name \}\)\) return \}/)
})

test('6: the Store\'s Backups tab says a backup\'s state and provider in the words the bucket page uses', () => {
  const t = tOf(M.storageLocales.en)
  assert.deepEqual(['current', 'store_changed', 'bucket_changed'].map((s) => M.backupChip(s, t).label), ['Backed up', 'Store changed', 'Bucket changed'])
  assert.equal(M.providerWords('minio', t), 'MinIO')
  const links = src('src/client/storage/StoreLinks.tsx')
  assert.match(links, /\{backupChip\(state, t\)\.label\}/)
  assert.match(links, /providerWords\(bucket\.provider, t\)/)
  for (const lang of ['en', 'zh']) assert.equal(Object.keys(M.storageLocales[lang]).some((k) => k.startsWith('spBackupState_')), false)
})

test('7: a Store on the native machine is in the LayerFS Home, on every page', () => {
  assert.equal(M.placeWords('native', tOf(M.storageLocales.en)), 'LayerFS Home')
  assert.equal(M.placeWords('docker', tOf(M.storageLocales.en)), 'docker')
  assert.match(src('src/client/hero/HeroProject.tsx'), /m === 'native' \? t\('home'\) : m/)
  assert.match(src('src/client/projects/setup/ProjectDialog.tsx'), /placeName\(s\.machine \?\? 'native', wordsOf\(t\)\)/)
})

test('8, 9: the Move dialog\'s notice names the move; the new-database screen has Cancel', () => {
  assert.doesNotMatch(M.storageLocales.en.heldNow, /push/)
  assert.match(src('src/client/storage/ConnectDatabase.tsx'), /check\?\.state === 'ready' \|\| screen\.kind === 'new' \? <Button variant="outline" onClick=\{onClose\}>\{t\('cancel'\)\}/)
})

test('10: a session tag reads without DSH\'s session- prefix', () => {
  const t = tOf(M.logsLocales.en)
  assert.equal(M.tagWords('dsh:session-ef98a1b2c3d4e5f6:', t), 'session ef98a1b2c3d4')
  assert.equal(M.tagWords('dsh:abc:', t), 'session abc')
  assert.equal(M.tagWords('other', t), 'other')
})

test('11: the history legend counts are singular for one', () => {
  const t = tOf(M.filesEn)
  assert.equal(M.treeCounts(1, 1, 1, t), '1 Branch · 1 checkpoint · 1 Layer')
  assert.equal(M.treeCounts(20, 552, 3, t), '20 Branches · 552 checkpoints · 3 Layers')
  for (const k of ['files_treeBranches', 'files_treeBranchOne', 'files_treeCheckpoints', 'files_treeCheckpointOne', 'files_treeLayers', 'files_treeLayerOne']) assert.ok(M.filesZh[k], k)
})

test('12: the script drawer names the preset as its card does, and describes any preset', () => {
  const t = tOf(M.presetsLocales.en)
  assert.equal(M.presetIdName(t, 'quickstart'), 'Starter')
  assert.equal(M.presetIdName(t, 'studio'), 'Studio')
  assert.doesNotMatch(M.presetsLocales.en.scriptBodyAll, /eight|four/)
  assert.match(src('src/client/presets/ScriptDrawer.tsx'), /t\('scriptTitleAll', \{ name: presetIdName\(t, preset\) \}\)/)
})

test('15: the unused RuntimeCard is gone', () => {
  assert.equal(existsSync(join(root, 'src/client/machines/Runtime.tsx')), false)
})
