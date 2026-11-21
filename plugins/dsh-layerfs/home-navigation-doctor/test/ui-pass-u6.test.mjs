// UI pass 1 on 3080, package U6: what the machine, project and Doctor pages
// say when something is in the way (a status word, LayerFS's message as the
// tip, one action), and the phone layouts of the project table and tab bars.
//   node --test test/ui-pass-u6.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdirSync, mkdtempSync, readFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

const root = fileURLToPath(new URL('..', import.meta.url))
mkdirSync(join(root, 'lib'), { recursive: true })
const out = mkdtempSync(join(root, 'lib', '.test-ui-pass-u6-'))
await build({
  absWorkingDir: root,
  stdin: {
    contents: [
      "export * as notices from './src/client/machines/notices.ts'",
      "export * as model from './src/client/machines/model.ts'",
      "export { locales as machineLocales } from './src/client/machines/locale.ts'",
      "export { locales as storageLocales } from './src/client/storage/locale.ts'",
      "export { commitTotalWords } from './src/client/storage/store-page.ts'",
      "export { serverCheckWords } from './src/client/storage/machine-stores.ts'",
      "export { wordsOf } from './src/client/doctor/words.ts'",
      "export { locales as doctorLocales } from './src/client/doctor/locale.ts'",
      "export { ProjectStorageLedgers } from './src/host/routes/project-storage.ts'",
      "export { CliError } from './src/host/layerfs/cli.ts'",
    ].join('\n'),
    resolveDir: root, loader: 'ts',
  },
  outfile: join(out, 'u6.mjs'), bundle: true, format: 'esm', platform: 'node', target: 'node24', logLevel: 'warning',
  packages: 'external',
})
const M = await import(pathToFileURL(join(out, 'u6.mjs')).href)
test.after(() => rmSync(out, { recursive: true, force: true }))

const N = M.notices
const src = (path) => readFileSync(join(root, path), 'utf8')
const fill = (text, vars = {}) => text.replace(/\{(\w+)\}/g, (_, k) => String(vars[k] ?? `{${k}}`))
const storageT = (key, vars) => fill(M.storageLocales.en[key] ?? key, vars)

const unreadable = { name: 'bad', platform: '', state: 'broken', reason: 'failed', fix: 'layerfs machine doctor bad', message: '/home/.layerfs/machine/bad/record.json: expected a field name at byte 36', stores: [], sandboxes: [] }

test('FAIL 3: an unreadable machine page keeps Remove off, with the reason its list row gives', () => {
  const spec = src('src/client/machines/Spec.tsx')
  const zone = spec.slice(spec.indexOf('export function DangerZone'))
  assert.match(zone, /const unreadable = unreadableRecord\(m\)/)
  assert.match(zone, /unreadable \? t\('recordUnreadableNoRemove'\)/, 'the same words as the row menu (parts.tsx)')
  assert.match(zone, /disabled=\{!!why \|\| pending !== null\}/)
  assert.match(src('src/client/machines/parts.tsx'), /disabledReason=\{unreadableRecord\(m\) \? t\('recordUnreadableNoRemove'\)/)
})

test('FAIL 4: a project whose history could not be read: a word, LayerFS\'s message as the tip, its fix as the one action', async () => {
  const table = src('src/client/storage/ProjectTable.tsx')
  assert.doesNotMatch(table, /p\.error \? p\.error\.message/, 'no raw message as the subtitle')
  assert.match(table, /<HistoryUnread error=\{p\.error\}/)
  assert.match(table, /detail=\{<>\{error\.message\}/, 'the message is the dot\'s tip')
  // the fix as the one action (U10: through resolveFix, so a broken machine's start or stop is its own fix instead)
  assert.match(table, /const resolved = resolveFix\(fixes, error\.fix\)/)
  assert.match(table, /<FixAction line=\{line\} fix=\{fix\}/)
  assert.equal(M.storageLocales.en.spHistoryUnread, 'History not read')
  assert.ok(M.storageLocales.zh.spHistoryUnread)
  // the host keeps the refusal's fix line beside its message
  const home = mkdtempSync(join(tmpdir(), 'u6-ledger-'))
  try {
    const refusal = new M.CliError('Unavailable', 'io error: No such file or directory (os error 2)', null, 'refusal', 'layerfs machine start docker')
    const lfs = () => ({ branch: { list: async () => { throw refusal } } })
    const ledgers = new M.ProjectStorageLedgers(home, home, lfs, () => {})
    await ledgers.view('shop/web', 1)
    await ledgers.settle('shop/web', 1)
    const view = await ledgers.view('shop/web', 1)
    assert.deepEqual({ ...view.error, detail: undefined }, { code: 'Unavailable', message: 'io error: No such file or directory (os error 2)', detail: undefined, fix: 'layerfs machine start docker' })
  } finally { rmSync(home, { recursive: true, force: true }) }
})

test('FAIL 5: a stop refused InUse names the Sandboxes from its sandboxes field, else related, else the message', () => {
  const message = 'machine m runs Sandboxes [m-sb, m-sb2]; close them first'
  assert.deepEqual(N.inUseSandboxes({ code: 'InUse', message, detail: { sandboxes: ['m-sb', 'm-sb2'] } }), ['m-sb', 'm-sb2'])
  assert.deepEqual(N.inUseSandboxes({ code: 'InUse', message, detail: { sandboxes: [{ name: 'a' }, { sandbox: 'b' }] } }), ['a', 'b'])
  assert.deepEqual(N.inUseSandboxes({ code: 'InUse', message, detail: { related: [{ kind: 'sandbox', name: 'r1' }, { kind: 'store', name: 's' }] } }), ['r1'])
  assert.deepEqual(N.inUseSandboxes({ code: 'InUse', message, detail: null }), ['m-sb', 'm-sb2'])
  assert.equal(N.inUseSandboxes({ code: 'Busy', message, detail: { sandboxes: ['x'] } }), null)
  assert.equal(N.inUseSandboxes({ code: 'InUse', message: 'store s holds project p', detail: null }), null, 'not about Sandboxes: the plain refusal')
  const en = M.machineLocales.en
  assert.equal(fill(en[N.closeFirstKey(2)], { n: 2 }), 'Close its 2 Sandboxes first')
  assert.equal(en[N.closeFirstKey(1)], 'Close its Sandbox first')
  const page = src('src/client/machines/MachineDetail.tsx')
  assert.match(page, /title=\{refusalOf\(error\)\.message\}/, 'LayerFS\'s message is the tip, no code prefix')
  assert.match(page, /onClick=\{\(\) => setTab\('sandboxes'\)\}>\{t\('inUseOpenSandboxes'\)\}/, 'one action: the Sandboxes tab')
})

test('FAIL 6: at phone width the project table keeps the name column (Sessions and Sandboxes leave the row)', () => {
  const css = src('src/client/storage/ProjectTable.module.css')
  const phone = css.slice(css.indexOf('@container (max-width: 480px)'))
  assert.ok(phone.length > 30, 'a phone rule')
  assert.match(phone, /grid-template-columns: 20px minmax\(0, 1fr\) minmax\(56px, auto\) 28px/)
  assert.match(phone, /\.colSessions,\s*\.colSandboxes \{\s*display: none;/)
  assert.match(css, /\.head > \[role='columnheader'\] \{[^}]*text-overflow: ellipsis/)
})

test('FAIL 7, 8: the project, machine and Sandbox pages use the shared tab bar (scroll with fades, never squeezed)', () => {
  for (const file of ['src/client/storage/ProjectPage.tsx', 'src/client/machines/MachineDetail.tsx', 'src/client/machines/SandboxDetail.tsx']) {
    const s = src(file)
    assert.doesNotMatch(s, /SegmentedTabs/, file)
    assert.match(s, /<TabBar /, file)
  }
  const bar = src('src/client/ui/TabBar.tsx')
  assert.match(bar, /new ResizeObserver\(inSight\)/, 'fades and the selected tab follow width changes (inSight calls fades)')
  assert.match(bar, /bar\.scrollLeft = /, 'the selected tab is scrolled into sight')
  const css = src('src/client/ui/TabBar.module.css')
  assert.match(css, /\.tab \{[^}]*flex: none;[^}]*white-space: nowrap;/s, 'labels keep their width')
  assert.match(css, /margin-inline: -4px;/, 'a page\'s top inset survives')
})

test('FAIL 9: an unreadable machine says "Record unreadable" (the message is the tip), Run diagnostics, and no Start hint', () => {
  const spec = src('src/client/machines/Spec.tsx')
  assert.match(spec, /<Item label=\{t\('downBecause'\)\} tone="danger" title=\{m\.message \?\? undefined\}>\{t\('stateUnreadable'\)\}<\/Item>/)
  assert.match(spec, /m\.fix && !unreadable/)
  const view = { readErrors: undefined }
  assert.deepEqual(N.sandboxesEmptyKey(unreadable, view, false), { key: 'sandboxesRecordUnreadable', home: false })
  assert.doesNotMatch(M.machineLocales.en.sandboxesRecordUnreadable, /Start/)
  assert.equal(N.sandboxesEmptyKey({ ...unreadable, platform: 'lima', state: 'stopped', reason: undefined }, view, false).key, 'sandboxesAfterStart')
  assert.equal(N.sandboxesEmptyKey({ ...unreadable, platform: 'docker', state: 'stopped', reason: undefined }, view, false).key, 'sandboxesAfterWorker')
  assert.equal(N.sandboxesEmptyKey({ ...unreadable, platform: 'lima', state: 'ready', reason: undefined }, view, true).key, 'noSandboxesHere')
  for (const file of ['src/client/machines/Inventory.tsx', 'src/client/machines/SandboxCards.tsx']) assert.match(src(file), /sandboxesEmptyKey\(m, view, running\)/, file)
})

test('minor: a busy machine says its op once ("Stopping", not "Stopping · stop · stop")', () => {
  const op = { id: 'o', name: 'stop', phase: 'stop', started: null, uncertain: false }
  assert.equal(M.model.opWords(op), 'stop')
  assert.equal(M.model.opWords({ ...op, name: 'store.move', phase: 'send' }), 'store.move · send')
  const look = M.model.machineState({ ...unreadable, platform: 'lima', state: 'busy', reason: undefined, op })
  assert.equal(look.label, 'stateStopping')
  assert.equal(N.chipOpWords(look), null)
  assert.equal(N.chipOpWords({ ...look, op: { ...op, phase: 'drain' } }), 'drain')
  assert.equal(N.chipOpWords({ label: 'stateBusy', state: 'busy', op: { ...op, name: 'machine.repair', phase: 'boot' } }), 'machine.repair · boot')
})

test('minor: the Doctor page re-reads the tree on Refresh; a Not checked row shows no old "data read"', () => {
  const page = src('src/client/storage/StoragePage.tsx')
  assert.match(page, /const refresh = \(\) => \{[^}]*setPageRefresh\(\(n\) => n \+ 1\)/)
  assert.match(page, /<HealthView api=\{api\} t=\{doctorT\} ts=\{t\} refreshKey=\{pageRefresh\} \/>/)
  const hook = src('src/client/doctor/use-doctor.ts')
  assert.match(hook, /if \(lastKey\.current === refreshKey\) return[\s\S]*readTree\(\)\.then\(\(\) => resync\(\)\)/)
  const t = (key, vars) => fill(M.doctorLocales.en[key] ?? key, vars)
  const w = M.wordsOf(t)
  const thing = { key: 'project:s/p', kind: 'project', name: 'p', parent: null }
  const state = { world: { by: { [thing.key]: thing }, kids: {} }, notes: { [thing.key]: { fullAt: Date.parse('2026-11-19T09:03:00Z') } } }
  assert.equal(w.fact(state, { thing, seen: { st: 'nc' }, depth: 1 }), '')
  assert.match(w.fact(state, { thing, seen: { st: 'ok' }, depth: 1 }), /^data read /)
})

test('minor: the machine Stores tab and Overview count the same PostgreSQL Stores', () => {
  const m = { ...unreadable, name: 'vm', platform: 'lima', state: 'ready', reason: undefined, sandboxes: [] }
  const view = {
    postgresql: [{ name: 'a', connectedFrom: [{ machine: 'native', reached: 'ok' }, { machine: 'vm', reached: 'ok' }], projects: 1 }, { name: 'b', connectedFrom: [{ machine: 'native', reached: null }], projects: 1 }, { name: 'c', connectedFrom: [], projects: 0 }],
    sandboxes: [{ name: 'sb', machine: 'vm', project: 'b/web', workspaces: 0 }],
  }
  assert.deepEqual(N.pgStoresHere(view, m), ['a', 'b'], 'connected from here (spec v23), or has a Sandbox here now')
  const en = M.machineLocales.en
  assert.equal(en[N.pgSupportKey(0)], 'Supported · none connected from here')
  assert.equal(fill(en[N.pgSupportKey(2)], { n: 2 }), 'Supported · 2 Stores connected from here')
  assert.match(src('src/client/machines/Spec.tsx'), /pgStoresHere\(view, m\)\.length/)
})

test('minor: a reachable server without a measured latency says "Reachable", never "— ms"', () => {
  assert.equal(M.serverCheckWords({ state: 'ok', latencyMs: null }, storageT), 'Reachable')
  assert.equal(M.serverCheckWords({ state: 'ok', latencyMs: 12 }, storageT), 'Reachable · 12 ms')
  assert.equal(M.serverCheckWords({ state: 'failed', latencyMs: null }, storageT), storageT('hsUnreachable'))
})

test('minor: New workspace is not offered on a Sandbox whose daemon is gone; off with its reason otherwise', () => {
  assert.deepEqual(N.workspaceOffer({ state: 'broken', reason: 'lost' }, 0, 16), { show: false, why: null })
  assert.deepEqual(N.workspaceOffer({ state: 'broken', reason: 'missing' }, 1, 16), { show: false, why: null })
  assert.deepEqual(N.workspaceOffer({ state: 'broken', reason: 'unreachable' }, 0, 16), { show: true, why: 'termAddOffline' })
  assert.deepEqual(N.workspaceOffer({ state: 'ready' }, 16, 16), { show: true, why: 'termFull' })
  assert.deepEqual(N.workspaceOffer({ state: 'ready' }, 3, 16), { show: true, why: null })
  assert.match(src('src/client/machines/SandboxTerminals.tsx'), /\{offer\.show && \(/)
})

test('minor: "1 commit", not "1 commits"', () => {
  const say = (c) => { const w = M.commitTotalWords(c); return storageT(w.key, { n: w.n }) }
  assert.equal(say({ commits: 1, complete: true, refused: null }), '1 commit')
  assert.equal(say({ commits: 34, complete: true, refused: null }), '34 commits')
  assert.equal(say({ commits: 0, complete: false, refused: 'x' }), storageT('spCommitsUnread'))
})

test('minor: a long image digest in the prune dialog is cut and the text wraps', () => {
  const msg = 'cgr.dev/chainguard/minio:latest: image sha256:4692462f35d97d7e82c30371d82f057703c5d9489bcae72601059aa0 is used by a Sandbox'
  assert.equal(N.shortDigests(msg), 'cgr.dev/chainguard/minio:latest: image sha256:4692462f35d9… is used by a Sandbox')
  assert.equal(N.shortDigests('fbfb4059d212 short'), 'fbfb4059d212 short', 'a short id stays')
  // U12 F13 (fix loop FAIL-3): the word per image is "Not removed"; its tip is "used by a Sandbox" or the cut message, never the full digest
  assert.match(src('src/client/machines/MachineImages.tsx'), /title=\{refusedInUse\(r\) \? t\('pruneWhyInUse'\) : shortDigests\(r\.message\)\}>\s*\{t\('pruneRefusedWord', \{ image: names\.get\(r\.id\) \?\? r\.id \}\)\}/)
  assert.match(src('src/client/machines/machines.module.css'), /\.errorText \{[^}]*overflow-wrap: anywhere;/)
})

test('every new label has English and Chinese words', () => {
  const keys = {
    machine: ['inUseCloseOne', 'inUseClose', 'inUseOpenSandboxes', 'pgSupportedNone', 'sandboxesRecordUnreadable', 'termAddOffline'],
    storage: ['spHistoryUnread', 'spCommitsTotalOne', 'msReachable'],
  }
  for (const k of keys.machine) { assert.ok(M.machineLocales.en[k], k); assert.ok(M.machineLocales.zh[k], k); assert.notEqual(M.machineLocales.zh[k], M.machineLocales.en[k], k) }
  for (const k of keys.storage) { assert.ok(M.storageLocales.en[k], k); assert.ok(M.storageLocales.zh[k], k) }
})
