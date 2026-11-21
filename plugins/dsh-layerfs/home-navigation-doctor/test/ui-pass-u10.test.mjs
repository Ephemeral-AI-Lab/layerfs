// UI pass 2 on 3080, package U10: the bucket page's listing loop, Connect
// again, a broken machine's state and fix winning over a live read, blocked-by
// words, the broken PostgreSQL project panel, phone layouts, the minors, the
// machine.start audit and the coordinator's add-ons (pass 2 part B1).
//   node --test test/ui-pass-u10.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdirSync, mkdtempSync, readFileSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

const root = fileURLToPath(new URL('..', import.meta.url))
mkdirSync(join(root, 'lib'), { recursive: true })
const out = mkdtempSync(join(root, 'lib', '.test-ui-pass-u10-'))

// React, the DSH primitives and clsx are stubbed: the test reads the modules' pure exports, it renders nothing.
// Each importer gets a stub exporting exactly the names it imports from that package.
const stub = {
  name: 'stub-ui',
  setup(b) {
    b.onResolve({ filter: /^(react|react-dom|clsx|@deepseek-ai\/.*)(\/.*)?$/ }, (a) => ({ path: `${a.path}|${a.importer}`, namespace: 'stub' }))
    b.onLoad({ filter: /.*/, namespace: 'stub' }, (a) => {
      const [spec, importer] = a.path.split('|')
      const text = readFileSync(importer, 'utf8')
      // the automatic JSX runtime's own names, imported by esbuild rather than the source
      const names = new Set(spec.endsWith('jsx-runtime') ? ['jsx', 'jsxs', 'Fragment'] : [])
      const quoted = spec.replace(/[.*+?^${}()|[\]\\/]/g, '\\$&')
      for (const m of text.matchAll(new RegExp(`import\\s+(type\\s+)?([^'";]*?)\\s+from\\s+['"]${quoted}['"]`, 'g'))) {
        if (m[1]) continue
        const braces = /\{([^}]*)\}/.exec(m[2])
        for (const part of braces ? braces[1].split(',') : []) {
          const w = part.trim()
          if (!w || w.startsWith('type ')) continue
          names.add(w.split(/\s+as\s+/)[0].trim())
        }
      }
      const lines = ['const f = function Stub() { return null }', 'export default f', ...[...names].map((n) => `export const ${n} = f`)]
      return { contents: lines.join('\n'), loader: 'js' }
    })
  },
}
await build({
  absWorkingDir: root,
  stdin: {
    contents: [
      "export * as sm from './src/client/storage/model.ts'",
      "export * as doc from './src/shared/doctor.ts'",
      "export { probedRecord } from './src/host/layerfs/machine-records.ts'",
      "export { resolveFix } from './src/client/ui/index.tsx'",
      "export { bucketDraft, connectOutcome } from './src/client/storage/dialogs.tsx'",
      "export { brokenBlock } from './src/client/storage/StoreCard.tsx'",
      "export { tabName, keptInSight } from './src/client/ui/TabBar.tsx'",
      "export { missingKey } from './src/client/projects/ProjectList.tsx'",
      "export * as setup from './src/client/projects/setup/model.ts'",
      "export * as mm from './src/client/machines/model.ts'",
      "export { freeSpaceRows } from './src/client/machines/images.ts'",
      "export { causeOf } from './src/client/doctor/model.ts'",
      "export { rereadsTree, keptState, keepState } from './src/client/doctor/use-doctor.ts'",
      "export { locales as storageLocales } from './src/client/storage/locale.ts'",
      "export { locales as machineLocales } from './src/client/machines/locale.ts'",
      "export { locales as setupLocales } from './src/client/projects/setup/locale.ts'",
      "export { filesEn, filesZh } from './src/client/files/locale.ts'",
      "export * as vm from './src/client/views/model.ts'",
      "export { buildTrajectory } from './src/host/sessions/trace.ts'",
      "export { shortId } from './src/client/files/format.ts'",
      "export * as ls from './src/client/ui/locale-sync.ts'",
      "export { kindWords } from './src/client/logs/format.ts'",
      "export { locales as logsLocales } from './src/client/logs/locale.ts'",
      "export { locales as doctorLocales } from './src/client/doctor/locale.ts'",
      "export { locales as viewsLocales } from './src/client/views/locale.ts'",
      "export { wordsOf as doctorWords } from './src/client/doctor/words.ts'",
      "export { upTime, unitsOf } from './src/client/machines/metrics.ts'",
      "export { staleDefault } from './src/client/hero/HeroProject.tsx'",
      "export { locales as listLocales } from './src/client/projects/list-locale.ts'",
    ].join('\n'),
    resolveDir: root, loader: 'tsx',
  },
  outfile: join(out, 'u10.mjs'), bundle: true, format: 'esm', platform: 'node', target: 'node24', logLevel: 'error',
  jsx: 'automatic', loader: { '.module.css': 'local-css', '.css': 'css' }, plugins: [stub],
})
const M = await import(pathToFileURL(join(out, 'u10.mjs')).href)
test.after(() => rmSync(out, { recursive: true, force: true }))

const src = (path) => readFileSync(join(root, path), 'utf8')
const fill = (text, vars = {}) => text.replace(/\{(\w+)\}/g, (_, k) => String(vars[k] ?? `{${k}}`))
const st = (key, vars) => fill(M.storageLocales.en[key] ?? key, vars)
const keyed = (key, vars) => `${key}${vars ? ` ${JSON.stringify(vars)}` : ''}`
const CONNECT = 'layerfs backup connect bk https://minio.lan:9000/backups/team --access-key AKIA1 --key-stdin'

// ---------------------------------------------------------------- F2

test('F2: a bucket page asks LayerFS to list once per open, and never after a refusal', () => {
  const blank = { files: null, error: null }
  const ask = M.sm.autoList()
  assert.equal(ask('bk', blank, false), true, 'the open lists once')
  assert.equal(ask('bk', blank, false), false, 'a render with the answer still out asks nothing')
  assert.equal(ask('bk', { files: null, error: { code: 'refused', message: 'bad key' } }, false), false, 'the refusal is the result; no effect lists again')
  assert.equal(ask('bk', { files: [], error: null }, false), false, 'an empty listing is a result too')
  // the page opened again with the refusal remembered: no call; only List again (a press) or a change event asks
  const again = M.sm.autoList()
  assert.equal(again('bk', { files: null, error: { code: 'not_listed', message: 'm' } }, false), false)
  assert.equal(again('bk', blank, true), false, 'never while a listing runs')
  assert.equal(again('bk', undefined, false), false, 'never before the bucket is known')
  // another bucket in the same page component: its own one ask
  assert.equal(ask('other', blank, false), true)
  const page = src('src/client/storage/BucketPage.tsx')
  assert.equal(page.match(/useListOnOpen\(/g).length, 3, 'the hook and both pages (bucket, backup file)')
  assert.match(page, /const \[ask\] = useState\(autoList\)/)
  assert.doesNotMatch(page, /if \(!b\?\.files[^\n]*onList\(\)/, 'no "no files: list again" effect')
})

test('F2: LayerFS\'s not_listed sentence is the status tip; its fix comes back with the listing', () => {
  const page = src('src/client/storage/BucketPage.tsx')
  assert.match(page, /const tip = b\.error\?\.message \?\? b\.message/)
  assert.match(page, /<Signal state="error"[^>]*detail=\{tip\}/)
  assert.match(src('src/host/routes/storage.ts'), /const notListedError = [^\n]*fix: n\.fix/)
  assert.equal(M.sm.listStatusWords('refused', st), 'Not listed · key refused')
  assert.equal(M.sm.listStatusWords('Timeout', st), st('bucketStatusTimeout'))
})

test('F2 grep: the catalog reload for missing project rows asks once per missing set', () => {
  const rows = [{ row: { key: 's/a' } }, { row: { key: 's/b', catalogState: 'ok' } }, { row: { key: 's/c' } }]
  assert.equal(M.missingKey(rows, null), 's/a\ns/c')
  assert.equal(M.missingKey(rows, [{ key: 's/a' }]), 's/c')
  assert.equal(M.missingKey(rows, [{ key: 's/a' }, { key: 's/c' }]), '', 'nothing missing: nothing to ask')
  assert.equal(M.missingKey([...rows].reverse(), null), M.missingKey(rows, null), 'order does not make a new set')
  assert.match(src('src/client/projects/ProjectList.tsx'), /asked/, 'a ref remembers the set already asked')
})

// ---------------------------------------------------------------- F5

test('F5: LF4\'s connect-again fix is parsed (name and URL only) and is a Center fix, never a host call', () => {
  assert.deepEqual(M.doc.connectBucketFix(CONNECT), { bucket: 'bk', url: 'https://minio.lan:9000/backups/team' })
  assert.deepEqual(M.doc.runnableFix(CONNECT), { kind: 'connectBucket', bucket: 'bk', url: 'https://minio.lan:9000/backups/team' })
  assert.deepEqual(M.doc.connectBucketFix("layerfs backup connect bk 'r2://acct/b/p' --access-key K --key-stdin"), { bucket: 'bk', url: 'r2://acct/b/p' })
  for (const bad of ['layerfs backup connect bk ftp://x --access-key K --key-stdin', 'layerfs backup connect bk https://x --access-key K', `${CONNECT} && rm -rf /`, "layerfs backup connect bk 'https://x --access-key K --key-stdin", null]) {
    assert.equal(M.doc.connectBucketFix(bad), null, String(bad))
  }
  assert.match(src('src/client/storage/fix-call.ts'), /case 'connectBucket': return Promise\.reject/)
  assert.match(src('src/client/storage/StoragePage.tsx'), /fix\.kind === 'connectBucket'\) \{ reconnectRef\.current\(/)
})

test('F5: a refused or keyless bucket\'s one action is Connect again…, prefilled with its name and URL', () => {
  const refused = { name: 'bk', provider: 'minio', url: 'http://minio:9000/bk/layerfs', state: null, fix: null, error: { code: 'refused', message: 'key refused' } }
  assert.deepEqual(M.sm.reconnectOf(refused), { bucket: 'bk', provider: 'minio', url: 'http://minio:9000/bk/layerfs' })
  assert.deepEqual(M.sm.reconnectOf({ ...refused, error: { code: 'refused', message: 'm', fix: CONNECT } }).url, 'https://minio.lan:9000/backups/team', 'LayerFS\'s fix names the URL')
  assert.deepEqual(M.sm.reconnectOf({ name: 'old', provider: 'r2', state: 'broken', fix: null, error: null }), { bucket: 'old', provider: 'r2', url: null }, 'a record with no access key')
  assert.equal(M.sm.reconnectOf({ ...refused, error: { code: 'timeout', message: 'm' } }), null, 'a timeout is listed again, not reconnected')
  assert.equal(M.sm.reconnectOf({ name: 'x', provider: 'r2', state: 'broken', fix: 'layerfs backup disconnect x', error: null }), null, 'another fix stays that fix')
  const draft = M.bucketDraft({ bucket: 'bk', provider: 'minio', url: 'http://minio:9000/bk/layerfs' })
  assert.deepEqual(draft, { provider: 'minio', values: { name: 'bk', bucket: 'bk', prefix: 'layerfs', jurisdiction: '', endpoint: 'http://minio:9000', account: '' } })
  assert.deepEqual(M.bucketDraft({ bucket: 'r', provider: 'r2', url: 'r2://acct/b/p?jurisdiction=eu' }).values, { name: 'r', bucket: 'b', prefix: 'p', jurisdiction: 'eu', endpoint: '', account: 'acct' })
  assert.equal(M.bucketDraft({ bucket: 'n', provider: 'r2', url: null }).values.name, 'n', 'no URL known: the name only')
  assert.deepEqual(M.sm.bucketParts('http://h:9000/b'), { provider: 'minio', endpoint: 'http://h:9000', bucket: 'b', prefix: '' })
  const cards = src('src/client/storage/BucketCards.tsx')
  assert.match(cards, /data-reconnect/)
  assert.match(cards, /t\('connectAgain'\)/)
  for (const lang of ['en', 'zh']) for (const k of ['connectAgain', 'reconnectTitle', 'reconnectBody', 'reconnectAction', 'fixRunConnectBucket']) assert.ok(M.storageLocales[lang][k], `${lang}.${k}`)
})

// ---------------------------------------------------------------- F1

test('F1: a broken record (machine get) wins over a live read of unknown; a healthy record takes the live answer', () => {
  const record = { machine: 'docker', platform: 'docker', state: 'broken', reason: 'failed', fix: 'layerfs machine doctor docker', message: 'record unreadable' }
  const got = { machine: 'docker', state: 'unknown', reason: 'not_answered', live: { free_disk_bytes: 5 } }
  const merged = M.probedRecord(record, got)
  assert.deepEqual([merged.state, merged.reason, merged.fix, merged.message], ['broken', 'failed', 'layerfs machine doctor docker', 'record unreadable'])
  assert.deepEqual(merged.live, { free_disk_bytes: 5 }, 'the live facts still come through')
  assert.equal(M.probedRecord({ ...record, state: 'stopped', reason: null, fix: null }, got).state, 'unknown', 'not broken: the live read says')
  assert.equal(M.probedRecord(record, null), record)
  const host = src('src/host/routes/machines.ts')
  assert.match(host, /machineOf\(probedRecord\(/)
})

test('F1: a broken machine\'s tabs never advise machine start: its own fix stands, or none', () => {
  const fleet = [{ name: 'docker', state: 'broken', fix: 'layerfs machine doctor docker' }, { name: 'gone', state: 'broken', fix: null }, { name: 'dev', state: 'stopped', fix: null }]
  assert.deepEqual(M.doc.fleetFix('layerfs machine start docker', fleet), { line: 'layerfs machine doctor docker', fix: { kind: 'doctor', machine: 'docker' } })
  assert.equal(M.doc.fleetFix('layerfs machine stop gone', fleet), null, 'no fix of its own: no action')
  assert.deepEqual(M.doc.fleetFix('layerfs machine start dev', fleet).fix, { kind: 'start', machine: 'dev' }, 'a stopped machine keeps Start')
  assert.deepEqual(M.doc.fleetFix('layerfs store compact s', fleet).fix, { kind: 'compact', store: 's' })
  // resolveFix asks the page's runner; without one the line is a plain tip
  const runner = { resolve: (line) => M.doc.fleetFix(line, fleet) }
  assert.equal(M.resolveFix(runner, 'layerfs machine start docker').line, 'layerfs machine doctor docker')
  assert.deepEqual(M.resolveFix(null, 'layerfs machine start dev'), { line: 'layerfs machine start dev', fix: null })
  assert.equal(M.resolveFix(runner, null), null)
  assert.match(src('src/client/storage/StoragePage.tsx'), /resolve: \(line\) => fleetFix\(line, fleet\)/)
  for (const file of ['src/client/storage/HomeOverview.tsx', 'src/client/storage/ProjectTable.tsx', 'src/client/storage/StoreCard.tsx', 'src/client/machines/Inventory.tsx', 'src/client/files/States.tsx']) {
    assert.match(src(file), /resolveFix\(/, file)
  }
})

test('F1: Remove for a built-in follows LayerFS: enabled when its fix is machine remove', () => {
  const parts = src('src/client/machines/parts.tsx')
  assert.match(parts, /builtIn && !fixRemoves\(m\)/, 'the greyed "Built in" Remove only when LayerFS gives no remove fix (U12: alone or as the last step of a chain)')
  assert.match(parts, /const recordOnly = fixAction\(m\)\?\.kind === 'remove'/)
  // fix loop 2 (M FAIL 5): a record that still holds Stores names them first; otherwise the record-only words as before
  assert.match(parts, /recordOnly \? \(m\.stores\.length \? t\('removeRecordHeldBody'[^]*?\) : t\('removeRecordBody'/)
  assert.match(src('src/client/machines/Spec.tsx'), /if \(BUILT_IN\.has\(m\.platform\) && !recordOnly\) return null/)
  const docker = { name: 'docker', platform: 'docker', state: 'broken', reason: 'failed', fix: 'layerfs machine remove docker' }
  assert.equal(M.mm.fixAction(docker)?.kind, 'remove')
  assert.equal(M.mm.runsFix(docker), true)
  assert.equal(M.mm.fixRemoves(docker), true)
  assert.equal(M.mm.fixRemoves({ ...docker, sandboxes: [], fix: 'layerfs machine stop docker && layerfs machine start docker' }), false)
  for (const lang of ['en', 'zh']) assert.ok(M.machineLocales[lang].removeRecordBody.includes('{name}'), lang)
})

// ---------------------------------------------------------------- F3

test('F3: blocked by a broken machine reads as a word, the machine\'s tip and Repair <m>, never the raw reason', () => {
  assert.equal(M.sm.blockedWords({ machine: 'lima1', reason: 'engine_down: docker.sock refused' }, keyed), 'machineBrokenBlock {"machine":"lima1"}')
  assert.equal(st('machineBrokenBlock', { machine: 'lima1' }), 'Blocked · lima1 broken')
  assert.equal(M.brokenBlock({ state: 'blocked', blockedBy: { machine: 'm', reason: 'unreachable' } }), 'm')
  assert.equal(M.brokenBlock({ state: 'blocked', blockedBy: { machine: 'm', reason: 'stopped' } }), null, 'stopped keeps Start')
  assert.equal(M.brokenBlock({ state: 'ready', blockedBy: null }), null)
  const card = src('src/client/storage/StoreCard.tsx')
  assert.match(card, /export function MachineRepair/)
  assert.match(card, /t\('repairMachine', \{ machine/)
  assert.match(src('src/client/storage/HomeCards.tsx'), /<MachineRepair /)
  for (const lang of ['en', 'zh']) assert.ok(M.storageLocales[lang].repairMachine.includes('{machine}'), lang)
})

// ---------------------------------------------------------------- F4

test('F4: a refusal with a runnable fix on the project panel: the word, the message as tip, the fix; no code chip, no Retry', () => {
  const states = src('src/client/files/States.tsx')
  const branch = states.slice(states.indexOf('a refusal with a fix the page runs'), states.indexOf('const code = errorCode(error)'))
  assert.match(branch, /title=\{errorText\(error\)\}/, 'LayerFS\'s message is the tip')
  assert.match(branch, /fixWord\?\.\(resolved\.fix\) \?\? title/)
  assert.match(branch, /<FixAction line=\{resolved\.line\}/)
  assert.doesNotMatch(branch, /retryLabel|onRetry|<code/, 'no Retry, no code chip on that path')
  assert.match(src('src/client/files/ProjectFiles.tsx'), /fix\.kind === 'saveAuth' \? t\('files_noSavedLogin'\)/)
  assert.equal(M.filesEn.files_noSavedLogin, 'Broken · no saved login')
  assert.ok(M.filesZh.files_noSavedLogin && M.filesZh.files_inTheWay && M.filesZh.files_branchesAll)
  // the project page of a broken Store shows the same word and fix on its files and history tabs
  assert.match(src('src/client/storage/ProjectPage.tsx'), /function StoreBroken/)
})

// ---------------------------------------------------------------- F7

test('F7: machine names wrap at phone width (the full name in the tip); the selected tab is brought back into sight', () => {
  const css = src('src/client/machines/machines.module.css')
  const phone = css.slice(css.lastIndexOf('@container (max-width: 520px)'))
  assert.match(phone, /\.fleetRow \.blockName \{[^}]*white-space: normal;/s)
  assert.match(phone, /\.fleetRow \.blockGlyph \{[^}]*display: none;/s)
  assert.match(src('src/client/machines/Board.tsx'), /className=\{css\.blockName\} onClick=\{onOpen\} title=\{t\('openMachine', \{ name \}\)\}/)
  assert.equal(M.keptInSight({ left: 400, width: 80 }, { scrollLeft: 0, clientWidth: 300 }), 208, 'right of the view: scrolled to it')
  assert.equal(M.keptInSight({ left: 10, width: 80 }, { scrollLeft: 200, clientWidth: 300 }), 0, 'left of it')
  assert.equal(M.keptInSight({ left: 50, width: 80 }, { scrollLeft: 0, clientWidth: 300 }), null, 'in sight: left alone')
  const bar = src('src/client/ui/TabBar.tsx')
  assert.match(bar, /window\.addEventListener\('resize', inSight\)/)
})

// ---------------------------------------------------------------- minors

test('M1, M2, M7, M8, M11: refused stats read —, plurals, counts not "None", the Branch column says what it counts, slot labels', () => {
  assert.match(src('src/client/storage/ProjectTable.tsx'), /const unread = !!trouble\?\.store \|\| !!p\.error/)
  assert.equal(st('msMoreProjectOne'), '+1 project')
  assert.equal(st('bcMoreFileOne'), '+1 file')
  assert.equal(M.storageLocales.en.msNone, undefined, 'no "PostgreSQL None"')
  assert.match(src('src/client/storage/MachineStores.tsx'), /msDatabases/)
  assert.ok(M.storageLocales.en.spColBranchesTitle && M.storageLocales.zh.spColBranchesTitle)
  assert.equal(fill(M.machineLocales.en.slotsOfWs, { n: 1, max: 16 }), '1/16 Workspaces')
  assert.equal(M.machineLocales.en.slotsOf, undefined)
})

test('M3: tab buttons and the fix button are named by their words', () => {
  assert.equal(M.tabName({ label: 'Sandboxes', count: 2, hint: 'one needs you' }), 'Sandboxes, 2, one needs you')
  assert.equal(M.tabName({ label: 'Images', count: null }), 'Images')
  assert.match(src('src/client/ui/TabBar.tsx'), /aria-label=\{tabName\(x\)\}/)
  // U12 F5: the fix button's name is its words, plus the thing it acts on where the runner names it (`Update (compact-demo-pg)`)
  assert.match(src('src/client/ui/index.tsx'), /aria-label=\{runner\.named\?\.\(fix, words\) \?\? words\}/)
})

test('M4: the Save-login labels sit over their inputs; the saved-login line only with saved logins', () => {
  const pg = src('src/client/storage/pg-fields.tsx')
  assert.match(pg, /passwordLabel/)
  assert.match(pg, /pgLoginField/)
  assert.match(src('src/client/storage/dialogs.tsx'), /pgLoginBodyNew/)
  for (const lang of ['en', 'zh']) assert.ok(M.storageLocales[lang].pgLoginBodyNew && M.storageLocales[lang].pgLoginField, lang)
})

test('M5, M6: a lost Sandbox reads Lost (danger) everywhere, a word and Close on its card, no prose', () => {
  assert.equal(M.sm.sandboxStateWords({ state: 'broken', reason: 'lost' }, st), 'Lost')
  assert.equal(M.sm.sandboxStateWords({ state: 'broken', reason: 'missing' }, st), 'Lost')
  assert.equal(M.machineLocales.en.stateLost, 'Lost')
  assert.match(src('src/client/storage/ProjectSandboxes.tsx'), /bad: info\.state === 'broken'/)
  const cards = src('src/client/machines/SandboxCards.tsx')
  assert.match(cards, /data-sb-gone-word/)
  assert.match(cards, /<CloseGone name=\{card\.name\} \/>/)
  assert.doesNotMatch(cards, /t\('sbGoneBody'\)\}<\/p>/, 'the sentence is a tip, not card text')
})

test('M9, M10, M12, M13, M14: state words capitalised, short digests, loading before data, the PG row wraps, the Doctor keeps results', () => {
  // capitalised state words: the locale's own words now, in zh too (test/ui-pass-u11.test.mjs NOTE 9 renders them)
  const digest = `sha256:${'ab12'.repeat(16)}`
  assert.equal(M.doc.shortDigest(digest), 'sha256:ab12ab12ab12…')
  assert.equal(M.doc.shortDigest('ubuntu:24.04'), 'ubuntu:24.04')
  assert.match(src('src/client/doctor/Row.tsx'), /title=/)
  assert.match(src('src/client/storage/StoragePage.tsx'), /!view \? <Loading label=\{t\('readingStores'\)\} \/>/)
  assert.match(src('src/client/storage/MachineStores.module.css'), /\.pgServer \{\s*flex-wrap: wrap;/)
  const api = {}
  assert.equal(M.keptState(api), undefined)
  M.keepState(api, { world: null })
  assert.equal(M.keptState(api), undefined, 'nothing drawn yet: nothing kept')
  const shown = { world: { homeKey: 'home' }, results: {} }
  M.keepState(api, shown)
  assert.equal(M.keptState(api), shown, 'reopened: the earlier results draw at once')
})

// ---------------------------------------------------------------- machine.start audit

test('machine.start: no Create, Move, New Store or Home hero starts a machine; each start is that machine\'s own press', () => {
  const run = src('src/client/projects/setup/run.ts')
  assert.doesNotMatch(run, /start-store|startsWith\('start:'\)/, 'Create has no start steps')
  const dialog = src('src/client/projects/setup/ProjectDialog.tsx')
  assert.doesNotMatch(dialog, /ensure\(/, 'Move no longer starts the current and the chosen machine')
  assert.match(dialog, /<StartFirst /)
  const store = src('src/client/storage/dialogs.tsx')
  assert.equal(store.match(/startMachine\(/g)?.length ?? 0, 1, 'one start, behind StartFirst\'s own button')
  assert.match(store, /start=\{\(m\) => startMachine\(api, m\)\}/)
  const tiles = src('src/client/projects/setup/MachineTiles.tsx')
  assert.match(tiles, /aria-label=\{words\.start\(m\)\}[\s\S]{0,200}?onClick=\{guard\(\(\) => press\(m\)\)\}/, 'one Start per machine, named')
  const hero = src('src/client/storage/HomeOverview.tsx')
  assert.doesNotMatch(hero, /startMachine|t\('ovStart'\)/, 'the hero opens the machine page instead of starting it')
  const view = { machines: [{ name: 'native', state: 'ready' }, { name: 'docker', state: 'stopped' }, { name: 'old', state: 'broken' }] }
  assert.deepEqual(M.setup.needsStart({ kind: 'sqlite', store: { machine: 'docker' }, machines: ['docker'] }, view), ['docker'])
  assert.deepEqual(M.setup.stoppedOf(['old', 'native'], view), [], 'a broken machine is not offered Start')
  for (const lang of ['en', 'zh']) for (const k of ['stoppedFirst', 'startNamed', 'needsStart']) assert.ok(M.setupLocales[lang][k], `${lang}.${k}`)
  assert.equal(M.setupLocales.en.startsOnCreate, undefined)
})

// ---------------------------------------------------------------- add-ons (pass 2 part B1)

test('add-on 1, 2: New Store and New project default to the page\'s machine, else native; never broken or one Create would start', () => {
  const machines = [
    { name: 'docker', platform: 'docker', state: 'broken', reason: 'failed', supports: ['sqlite'] },
    { name: 'native', platform: 'native', state: 'ready', supports: ['sqlite'] },
    { name: 'm2', platform: 'lima', state: 'ready', supports: ['sqlite'] },
    { name: 'm3', platform: 'lima', state: 'stopped', supports: ['sqlite'] },
  ]
  assert.equal(M.setup.defaultMachine(machines), 'native')
  assert.equal(M.setup.defaultMachine(machines, new Map(), 'm2'), 'm2', 'opened from m2\'s page')
  assert.equal(M.setup.defaultMachine(machines, new Map(), 'docker'), 'native', 'from a broken machine\'s page: native')
  assert.equal(M.setup.defaultMachine(machines, new Map(), 'm3'), 'native', 'a stopped machine is the person\'s pick, its start named')
  const page = src('src/client/storage/StoragePage.tsx')
  assert.match(page, /kind: 'new-store', machine: active\.startsWith\('machine:'\) \? active\.slice\(8\) : undefined/)
  const dialogs = src('src/client/storage/dialogs.tsx')
  assert.match(dialogs, /defaultMachine\(machines, undefined, opened\)/)
  assert.match(dialogs, /swap\(\{ kind: 'new-store', machine \}\)/)
  assert.match(src('src/client/projects/setup/MachineTiles.tsx'), /m\.state === 'broken'\) && value !== m\.name/, 'a broken tile is not picked')
})

test('add-on 3: connecting again at the same URL says "Key replaced; links kept"', () => {
  assert.equal(M.connectOutcome({ outcome: 'rotated' }), 'rotated')
  assert.equal(M.connectOutcome({ outcome: 'connected' }), null)
  assert.equal(M.connectOutcome(null), null)
  assert.equal(st('keyRotated'), 'Key replaced; links kept')
  assert.ok(M.storageLocales.zh.keyRotated && M.storageLocales.zh.keyRotatedTip)
  const dialogs = src('src/client/storage/dialogs.tsx')
  assert.match(dialogs, /if \(connectOutcome\(answer\) === 'rotated'\) setRotated\(true\)/)
  assert.match(dialogs, /data-rotated role="status"/)
  assert.match(src('src/host/routes/storage.ts'), /secret_key: secret, detail: true \}/)
})

test('add-on 4: an image LayerFS refused to remove is no longer checked or offered', () => {
  const ready = [
    { id: 'a', refs: ['a:1'], size: 10, sandboxes: 0, usable: true, source: 'docker', state: 'ready' },
    { id: 'b', refs: ['b:1'], size: 20, sandboxes: 0, usable: true, source: 'docker', state: 'ready' },
  ]
  const before = M.freeSpaceRows({ ready })
  assert.deepEqual(before.preselected, before.rows.map((r) => r.id))
  const held = new Map([[before.rows[0].id, 'is used by a Sandbox']])
  const after = M.freeSpaceRows({ ready }, held)
  assert.deepEqual(after.preselected, before.rows.slice(1).map((r) => r.id))
  const dialog = src('src/client/machines/MachineImages.tsx')
  assert.match(dialog, /setHeld\(\(now\) => new Map\(\[\.\.\.now, \.\.\.answer\.refused\.map/)
  assert.match(dialog, /disabled=\{busy \|\| used \|\| no !== undefined\}/)
  assert.match(dialog, /t\('imageNotRemoved'\)/)
  assert.ok(M.machineLocales.zh.imageNotRemoved)
})

test('add-on 5: a new Store placed on a machine reads "Created on <m>", not "Pushed to"', () => {
  const job = { id: 'j', kind: 'push', store: 's', target: 'm2', file: null, started: 1, ended: 2, phase: null, done: null, total: null, error: null, outcome: 'succeeded' }
  assert.equal(M.sm.endedWords({ ...job, made: true }, st), 'Created on m2')
  assert.equal(M.sm.endedWords(job, st), 'Pushed to m2', 'a push of an existing Store keeps its words')
  assert.equal(M.sm.doingWords({ ...job, ended: null, made: true }, st), 'Creating on m2')
  assert.match(src('src/client/projects/setup/run.ts'), /made \? \{ store, made \} : \{ store \}/)
  assert.match(src('src/client/storage/dialogs.tsx'), /placeStore\(api, store, place, true\)/)
  assert.match(src('src/host/routes/router.ts'), /moveStore\(body\?\.store, id, body\?\.made === true\)/)
  assert.match(src('src/host/routes/storage.ts'), /if \(own\) own\.made = true/)
})

test('add-on 6: an unreadable machine record: no Add Store or New Sandbox, honest disk and readiness, the Doctor row follows the records', () => {
  const unreadable = { name: 'm2', platform: '', state: 'broken', reason: 'failed' }
  assert.equal(M.mm.unreportedDiskKey(unreadable), 'diskRecordUnreadable')
  assert.equal(M.mm.unreportedDiskKey({ name: 'x', platform: 'lima', state: 'stopped' }), 'diskWhileStopped')
  assert.equal(M.mm.unreportedDiskKey({ name: 'x', platform: 'lima', state: 'ready' }), 'diskUnreported')
  assert.equal(M.mm.unreportedDiskKey({ name: 'x', platform: 'lima', state: 'broken', reason: 'unreachable' }), 'diskUnreported', 'not "while stopped" for a machine that is not stopped')
  const board = src('src/client/machines/Board.tsx')
  assert.match(board, /const noUse = unreadableRecord\(m\) \? t\('recordUnreadableNoRemove'\) : null/)
  assert.equal(board.match(/disabled=\{[^}]*noUse !== null\}/g)?.length, 2, 'Add Store and New Sandbox, with the reason Remove gives')
  assert.match(src('src/client/machines/Spec.tsx'), /unreadable \? t\('readinessRecordUnreadable'\)/)
  for (const lang of ['en', 'zh']) for (const k of ['diskRecordUnreadable', 'readinessRecordUnreadable']) assert.ok(M.machineLocales[lang][k], `${lang}.${k}`)
  assert.equal(M.rereadsTree({ state: 'done', group: null }), true, 'a row\'s own check reads the tree again (platform mark, facts)')
  assert.equal(M.rereadsTree({ state: 'done', group: 'all-1' }), false, 'Check all reads it once at its end')
  assert.equal(M.rereadsTree({ state: 'failed', group: null }), false)
  assert.match(src('src/client/doctor/use-doctor.ts'), /if \(rereadsTree\(run\)\) refresh\(\)/)
})

test('add-on 7: a Sandbox just made does not read "Daemon not running" from its create', () => {
  assert.match(src('src/host/routes/projects.ts'), /this\.sandboxMade\(name\)/)
  assert.match(src('src/host/routes/router.ts'), /projects\.sandboxMade = \(name\) => machines\.metrics\.created\(name\)/)
  // the sampler itself: test/metrics.test.mjs "a Sandbox just made drops its earlier points"
})

test('add-on 8: a Doctor row waiting on a stopped machine links to that machine like a row waiting on a broken one', () => {
  const m2 = { key: 'machine:m2', kind: 'machine', name: 'm2' }
  assert.equal(M.causeOf({ st: 'nc', why: 'stopped', by: m2 }, 'store:s'), m2)
  assert.equal(M.causeOf({ st: 'nc', why: 'waiting', by: m2 }, 'store:s'), m2)
  assert.equal(M.causeOf({ st: 'nc', why: 'stopped', by: m2 }, 'machine:m2'), null, 'never a link to itself')
  assert.equal(M.causeOf({ st: 'ok', why: null, by: null }, 'store:s'), null)
  assert.match(src('src/client/doctor/Row.tsx'), /const waitsOn = causeOf\(o, key\)/)
})

// ---------------------------------------------------------------- add-on 2 (pass 2 part B2)

const TAGGED = `12${'13b97716'.padEnd(64, 'c')}`
const zhOf = (dict) => (key, vars) => fill(dict.zh[key] ?? key, vars)

test('B2 #1: "N unsaved" counts changed entries, not inodes; it follows tool calls and a poll', () => {
  assert.equal(M.vm.unsavedCount({ inode_count: 4, entry_count: 3 }), 3, '3 new files: 3, not the root\'s inode too')
  assert.equal(M.vm.unsavedCount({ inode_count: 1, entry_count: 0 }), 1, 'an edit in place names no entry: its inode counts')
  assert.equal(M.vm.unsavedCount({ inode_count: 0, entry_count: 0 }), 0)
  assert.equal(M.vm.unsavedCount(null), 0)
  assert.match(src('src/client/views/FileViews.tsx'), /const pending = unsavedCount\(view\.status\?\.overlay\)/)
  const hooks = src('src/client/views/hooks.ts')
  assert.match(hooks, /api\.onEvent\('layerfs-operation', mine\)/)
  assert.match(hooks, /setInterval\([^\n]*SESSION_POLL_MS\)/)
})

test('B2 #2: the session map lists dsh\'s own mounted Workspace on its Sandbox card (as sandbox get does), off the flow', () => {
  const binding = { project: 's/p', branch: 'dsh.f5d2', sandbox: 'dsh-f5d2', ws: 'dsh-f5d2:main', state: 'bound' }
  const model = M.vm.buildSandboxMap([], binding, {}, { at: 1, sandboxes: [{ name: 'dsh-f5d2', workspaces: [{ id: 'dsh-f5d2:main', branch: 'dsh.f5d2' }] }] })
  const box = model.sandboxes.find((b) => b.name === 'dsh-f5d2')
  assert.equal(box.reserved, 1, 'the 1/16 count')
  assert.deepEqual(box.own, [{ id: 'dsh-f5d2:main', name: 'main', branch: 'dsh.f5d2' }], 'and the Workspace itself, so the card never says "No workspaces mounted"')
  assert.deepEqual(box.workspaces, [], 'not a flow node')
  const view = src('src/client/views/SandboxView.tsx')
  assert.match(view, /data-own-ws=\{ws\.id\}/)
  assert.match(view, /shownWs\.length === 0 && !\(box\.own\?\.length && !ghost\)/)
  assert.ok(M.viewsLocales.zh.sessionWorkspace)
})

test('B2 #3: the Trajectory counts every version the session made, its turn checkpoints and unlinked tool commits too', () => {
  const answer = M.buildTrajectory({
    sessionId: 's1', project: 'st/p', branch: 'dsh.s1', events: [],
    // the agent's own commit of its mounted Workspace: linked to no version (not the bound Workspace), its kind says it made one
    invocations: [{ id: 'r1', sessionId: 's1', toolCallId: 'call-1', turn: 1, operation: 'workspace.commit', outcome: 'success', kind: 'Committed', versions: [] }],
    checkpoints: [{ turn: 1, kind: 'turn', commit: TAGGED, branch: 'dsh.s1', changed: 2, message: 'turn 1', at: 1 }, { turn: 2, kind: 'turn', commit: TAGGED, changed: 0, message: '', at: 2 }],
  })
  assert.deepEqual(answer.counts, { versionsCreated: 2, callsWithVersions: 1 })
  const none = M.buildTrajectory({ sessionId: 's1', project: null, branch: null, events: [], invocations: [], checkpoints: [] })
  assert.deepEqual(none.counts, { versionsCreated: 0, callsWithVersions: 0 })
})

test('B2 #4: short ids keep the tag (LayerFS\'s 14-character label, a prefix commit get accepts); the full id is the tip and the copy', () => {
  assert.equal(M.shortId(TAGGED), TAGGED.slice(0, 14))
  assert.ok(M.shortId(TAGGED).startsWith('1213b97716'), 'never "13b97716"')
  assert.equal(M.shortId(`32${'a'.repeat(64)}`), `32${'a'.repeat(12)}`)
  assert.equal(M.shortId('abc'), 'abc')
  assert.equal(M.shortId(null), '—')
  const ui = src('src/client/ui/index.tsx')
  assert.match(ui, /export function CopyId/)
  // the press copies the full id (driven in test/ui-pass-u11.test.mjs NOTE 11, B2 #4)
  assert.match(src('src/client/files/ProjectHistory.tsx'), /<CopyId id=\{selected\.id\} short=\{shortId\(selected\.id\)\}/)
  assert.match(src('src/client/views/TrajectoryParts.tsx'), /title=\{`\$\{title\}\\n\$\{version\.id\}`\}/)
  assert.match(src('src/client/logs/LogsPage.tsx'), /const shortHex = \(id: string \| undefined\) => id \? shortId\(id\)/)
})

test('B2 #5: the zh gaps: Doctor check words, Logs saved kind, uptime units, times, Sandbox, state words, the machine chip', () => {
  const tz = zhOf(M.doctorLocales)
  const words = M.doctorWords(tz)
  assert.equal(words.rowWord('check:machine.configuration'), '配置')
  assert.equal(words.rowWord('check:store.connection'), '连接')
  assert.equal(M.doctorWords((k, v) => fill(M.doctorLocales.en[k] ?? k, v)).rowWord('check:machine.configuration'), 'Configuration', 'English unchanged')
  for (const key of Object.keys(M.doctorLocales.en)) assert.ok(key in M.doctorLocales.zh, key)
  assert.equal(M.kindWords('Committed', zhOf(M.logsLocales)), '已提交')
  assert.equal(M.kindWords('committed', zhOf(M.logsLocales)), '已提交')
  assert.equal(M.kindWords('exit:0', zhOf(M.logsLocales)), 'exit:0', 'another word as it came')
  const zhUnits = M.unitsOf(zhOf(M.machineLocales))
  assert.equal(M.upTime(0, 16 * 60_000, zhUnits), '16 分钟')
  assert.equal(M.upTime(0, 16 * 60_000), '16 min')
  M.ls.syncLocale('zh')
  assert.equal(M.ls.timeLocale(), 'zh-CN')
  assert.doesNotMatch(M.ls.clockTime(Date.UTC(2026, 0, 1, 18, 52, 16)), /AM|PM/, 'the 24-hour zh clock')
  M.ls.syncLocale('en')
  assert.equal(M.ls.timeLocale(), undefined)
  assert.equal(M.storageLocales.zh.ptSandboxes, 'Sandbox')
  assert.equal(M.viewsLocales.zh.tabSandbox, 'Sandbox', 'one term: the table and the tab agree')
  for (const dict of [M.storageLocales.zh, M.viewsLocales.zh]) for (const [k, v] of Object.entries(dict)) assert.ok(!v.includes('沙箱') || v.includes('沙箱隔离'), k)
  assert.equal(M.vm.stateWords('active', zhOf(M.viewsLocales)), '运行中')
  assert.equal(M.vm.stateWords('ready', zhOf(M.viewsLocales)), '就绪')
  assert.match(src('src/client/storage/MachineStores.tsx'), /state \? storeStateWords\(state, t\) : null/)
  // no browser-locale clock is left in the client
  for (const file of ['src/client/machines/Board.tsx', 'src/client/logs/LogsPage.tsx', 'src/client/machines/SandboxOverview.tsx', 'src/client/files/HistoryGraph.tsx']) {
    assert.doesNotMatch(src(file), /new Date\([^)]*\)\.toLocale(Date|Time)?String\(/, file)
  }
})

test('B2 #6: switching the locale redraws the session map\'s memoized rows', () => {
  const view = src('src/client/views/SandboxView.tsx')
  assert.match(view, /const revision = useLocaleRevision\(\)/)
  assert.match(view, /\[props\.t, revision\]/)
  assert.match(src('src/client/index.tsx'), /ctx\.locale\.subscribe\?\.\(followLocale\)/)
})

test('B2 #7: no removed project as the composer default; what each checkpoint count counts; Archive on a session row', () => {
  const rows = [{ key: 's/live' }, { key: 's/gone', catalogState: 'removed' }, { key: 's/swap', catalogState: 'identity-mismatch' }]
  assert.equal(M.staleDefault({ project: 's/gone' }, rows), true)
  assert.equal(M.staleDefault({ project: 's/swap' }, rows), true)
  assert.equal(M.staleDefault({ project: 's/live' }, rows), false)
  assert.equal(M.staleDefault({ project: 's/gone' }, null), false, 'not before the catalog is read')
  assert.equal(M.staleDefault({ project: null }, rows), false)
  assert.ok(M.storageLocales.en.colCheckpointsTitle && M.storageLocales.zh.colCheckpointsTitle)
  assert.match(M.filesEn.files_checkpointCount, /forked/)
  assert.match(src('src/client/storage/ProjectPage.tsx'), /title=\{t\('colCheckpointsTitle'\)\}/)
  // the row's Archive sends sessions.archive with its own id (driven in test/ui-pass-u11.test.mjs FAIL 2)
  assert.ok(M.listLocales.zh.archiveSession.includes('{session}'))
  const host = src('src/host/sessions/index.ts')
  assert.match(host, /'sessions\.archive': async/)
  assert.match(host, /ctx\.workspaceRegistry\.archiveSession\(id as never\)/)
  assert.match(host, /filter\(\(r\) => !archived\.has\(r\.sessionId\)\)/)
})
