// UI pass 3 on 3080, package U12a: click safety for power actions and fixes
// (F6), machine-named power controls (R FAIL 4), LayerFS's multi-command fix
// run step by step (R FAIL 1), an outdated record is not a worker that does
// not answer (R FAIL 2), the Files tab refusal as word + tip + fix (R FAIL 3),
// and a Doctor check that waits for its turn says so (F12).
//   node --test test/ui-pass-u12.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdirSync, mkdtempSync, readFileSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'
import { freshState, treeAnswer } from './fixtures/doctor-scenarios.mjs'

const root = fileURLToPath(new URL('..', import.meta.url))
mkdirSync(join(root, 'lib'), { recursive: true })
const out = mkdtempSync(join(root, 'lib', '.test-ui-pass-u12-'))

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
      "export * as doc from './src/shared/doctor.ts'",
      "export * as mm from './src/client/machines/model.ts'",
      "export * as dm from './src/client/doctor/model.ts'",
      "export { fixCall } from './src/client/storage/fix-call.ts'",
      "export { pressAllowed, guardedPress, PRESS_SETTLE_MS } from './src/client/ui/index.tsx'",
      "export { chainSteps } from './src/client/storage/StoragePage.tsx'",
      "export * as notices from './src/client/machines/notices.ts'",
      "export * as sm from './src/client/storage/model.ts'",
      "export * as metrics from './src/client/machines/metrics.ts'",
      "export { blockedOf } from './src/client/files/States.tsx'",
      "export { errorAnswer } from './src/host/routes/router.ts'",
      "export { refusalOf as cliRefusalOf } from './src/host/layerfs/cli.ts'",
      "export { refusedInUse } from './src/client/machines/MachineImages.tsx'",
      "export { classifyStore } from './src/host/sessions/availability.ts'",
      "export { PG_URL_LOGIN, loginInUrlFix } from './src/client/storage/pg-fields.tsx'",
      "export * as ms from './src/client/storage/machine-stores.ts'",
      "export { stoppedBlock, storeNotice } from './src/client/storage/StoreCard.tsx'",
      "export { locales as machineLocales } from './src/client/machines/locale.ts'",
      "export { locales as storageLocales } from './src/client/storage/locale.ts'",
      "export { locales as doctorLocales } from './src/client/doctor/locale.ts'",
      "export { filesEn, filesZh } from './src/client/files/locale.ts'",
    ].join('\n'),
    resolveDir: root, loader: 'tsx',
  },
  outfile: join(out, 'u12.mjs'), bundle: true, format: 'esm', platform: 'node', target: 'node24', logLevel: 'error',
  jsx: 'automatic', loader: { '.module.css': 'local-css', '.css': 'css' }, plugins: [stub],
})
const M = await import(pathToFileURL(join(out, 'u12.mjs')).href)
test.after(() => rmSync(out, { recursive: true, force: true }))

const src = (path) => readFileSync(join(root, path), 'utf8')
const CHAIN = 'layerfs store remove sim-website-0cefe19cfdbb6fe8d224 && layerfs machine remove docker'
// the owner's docker record on 3080 (no vm_name): broken outdated, LayerFS's fix is the two-command chain
const docker = { name: 'docker', platform: 'docker', state: 'broken', reason: 'outdated', fix: CHAIN, message: 'docker has no vm_name; remove its record', stores: ['sim-website-0cefe19cfdbb6fe8d224'], sandboxes: [] }

// ---------------------------------------------------------------- R FAIL 1

test('R FAIL 1: a multi-command fix is a chain the page runs step by step; anything else in it keeps it a tip', () => {
  assert.deepEqual(M.doc.runnableFix(CHAIN), { kind: 'chain', steps: [{ kind: 'removeStore', store: 'sim-website-0cefe19cfdbb6fe8d224' }, { kind: 'removeMachine', machine: 'docker' }] })
  assert.equal(M.doc.removesMachine(M.doc.runnableFix(CHAIN)), 'docker')
  assert.equal(M.doc.removesSomething(M.doc.runnableFix(CHAIN)), true, 'a chain that removes asks the confirm press')
  // unchanged: stop && start is the restart; save-auth stays the login dialog; --force and unknown commands stay tips
  assert.deepEqual(M.doc.runnableFix('layerfs machine stop m && layerfs machine start m'), { kind: 'restart', machine: 'm' })
  assert.equal(M.doc.runnableFix('layerfs store save-auth <auth> --user <user> --password-stdin && layerfs store update s --auth <auth>').kind, 'saveAuth')
  assert.equal(M.doc.runnableFix('layerfs machine stop m --force && layerfs machine start m'), null)
  assert.equal(M.doc.runnableFix('layerfs store remove s && rm -rf /'), null)
  assert.equal(M.doc.runnableFix('layerfs store remove s && layerfs machine doctor m'), null, 'a step that opens something is not chained')
  assert.equal(M.doc.runnableFix(Array(5).fill('layerfs store remove s').join(' && ')), null, 'at most four steps')
  assert.equal(M.doc.removesSomething({ kind: 'chain', steps: [{ kind: 'start', machine: 'a' }, { kind: 'stop', machine: 'b' }] }), false)
})

test('R FAIL 1: docker\'s card and page offer the chain, never Run diagnostics; Remove… is its fix, with a reason when off', () => {
  assert.deepEqual(M.mm.fixAction(docker), { kind: 'chain', line: CHAIN })
  assert.equal(M.mm.runsFix(docker), true, 'Board.tsx and MachineDetail.tsx show Run diagnostics only when !runsFix')
  assert.equal(M.mm.fixRemoves(docker), true)
  // a chain about another machine stays a tip to copy
  assert.equal(M.mm.fixAction({ ...docker, fix: 'layerfs store remove s && layerfs machine remove other' }).kind, 'copy')
  const parts = src('src/client/machines/parts.tsx')
  assert.match(parts, /if \(fix\?\.kind === 'remove' \|\| fix\?\.kind === 'chain'\) \{/, 'PowerButton runs the chain through FixAction')
  assert.match(parts, /builtIn && !fixRemoves\(m\)/, 'a built-in whose fix removes it gets Remove…')
  assert.match(parts, /pending !== null \? t\('removeWaitPending'\)/, 'Remove… off while an action runs says why')
  assert.match(src('src/client/machines/Spec.tsx'), /const recordOnly = fixRemoves\(m\)/)
  // FixAction drives the steps itself: one attempt each, the step it is at, the first refusal ends it
  const ui = src('src/client/ui/index.tsx')
  const fa = ui.slice(ui.indexOf('export function FixAction'), ui.indexOf('/** The words a refusal needs'))
  assert.match(fa, /for \(let i = 0; i < steps\.length; i\+\+\) \{ setAt\(i\); await runner\.run\(steps\[i\]!\) \}/)
  assert.match(fa, /runner\.step\?\.\(at \+ 1, steps\.length, steps\[at\]!\)/)
  assert.match(fa, /const removes = removesSomething\(fix\)/)
  assert.match(fa, /if \(armedFor !== line\)/, 'an armed confirm never runs a different line')
  const center = src('src/client/storage/StoragePage.tsx')
  assert.match(center, /case 'chain': return t\('fixRunChain'/)
  assert.match(center, /fix\.kind === 'chain' \? t\('fixConfirmChain'/)
  for (const lang of ['en', 'zh']) {
    const L = M.storageLocales[lang]
    assert.ok(L.fixRunChain.includes('{n}') && L.fixConfirmChain.includes('{n}') && L.fixChainStep.includes('{step}'), lang)
    assert.ok(M.machineLocales[lang].removeWaitPending, lang)
  }
})

test('R FAIL 1: a chain\'s host calls run in order, one attempt each, and stop at the first refusal', async () => {
  const calls = []
  const api = { call: async (method, path) => { calls.push(`${method} ${path}`); if (path.includes('/machines/')) throw Object.assign(new Error('refused'), { code: 'InUse' }) } }
  await assert.rejects(M.fixCall(api, M.doc.runnableFix(CHAIN)), /refused/)
  assert.deepEqual(calls, ['DELETE /api/stores/sim-website-0cefe19cfdbb6fe8d224?confirm=sim-website-0cefe19cfdbb6fe8d224', 'DELETE /api/machines/docker'])
  calls.length = 0
  const failing = { call: async (method, path) => { calls.push(`${method} ${path}`); throw new Error('first refused') } }
  await assert.rejects(M.fixCall(failing, M.doc.runnableFix(CHAIN)), /first refused/)
  assert.equal(calls.length, 1, 'nothing after the refused step runs')
})

// ---------------------------------------------------------------- R FAIL 2

test('R FAIL 2: an outdated record whose fix removes it is not a worker that does not answer; no Stop for it', () => {
  assert.equal(M.mm.recordOutdated(docker), true)
  assert.equal(M.mm.workerDown(docker), false)
  assert.equal(M.mm.machineState(docker).label, 'stateRecordOutdated')
  // an outdated worker that a restart repairs is still down, worded as before
  const worker = { ...docker, name: 'b', fix: 'layerfs machine stop b && layerfs machine start b' }
  assert.equal(M.mm.workerDown(worker), true)
  assert.equal(M.mm.machineState(worker).label, 'stateOutdated')
  const home = src('src/client/storage/HomeOverview.tsx')
  const branch = home.slice(home.indexOf('} else if (recordOutdated(m)) {'), home.indexOf("} else if (look.key === 'needs') {"))
  assert.match(branch, /ovRecordOutdatedStores/)
  assert.doesNotMatch(branch, /secondary|kind: 'stop'/, 'the head never offers Stop for it')
  assert.match(src('src/client/machines/MachineDetail.tsx'), /recordOutdated\(m\) \|\| vmGone\(m\) \? \(heldStores\(m\) === 1 \? 'storeBlockedRecordOne' : 'storesBlockedRecord'\)/, 'an outdated record (and, since fix loop 2, a VM gone) blocks its Store until the record is removed')
  for (const lang of ['en', 'zh']) {
    assert.ok(M.storageLocales[lang].ovRecordOutdatedStores.includes('{stores}'), lang)
    assert.ok(M.machineLocales[lang].stateRecordOutdated && M.machineLocales[lang].storeBlockedRecordOne && M.machineLocales[lang].storesBlockedRecord.includes('{n}'), lang)
  }
  assert.doesNotMatch(M.machineLocales.en.storeBlockedRecordOne, /answers/)
})

// ---------------------------------------------------------------- R FAIL 3

test('R FAIL 3: the Files tab refusal is a word, LayerFS\'s words as the tip and its fix; never the code word or prose inline', () => {
  const states = src('src/client/files/States.tsx')
  const fallback = states.slice(states.indexOf('const code = errorCode(error)'), states.indexOf('export function blockedOf'))
  assert.doesNotMatch(fallback, /failureDetail|<code/, 'no inline sentence, no code chip')
  assert.match(fallback, /title=\{tip\}/, 'LayerFS\'s words, code and fix line are the tip')
  assert.match(states, /title = blockedWord\?\.\(blockedOf\(error\) \?\? blockedOf\(refusalOf\(error\)\.detail\)\) \?\? title/, 'a blocked refusal is worded by what blocks it, on both paths (the error itself, else its detail)')
  // both LayerFS shapes: `Failed` with the machine's chain (runnable now), and the blocked refusal with reason + fix
  assert.deepEqual(M.blockedOf({ blocked_by: { machine: 'docker', reason: 'outdated' } }), { machine: 'docker', reason: 'outdated' })
  assert.deepEqual(M.blockedOf({ blockedBy: { machine: 'docker' } }), { machine: 'docker', reason: null })
  assert.equal(M.blockedOf(null), null)
  const files = src('src/client/files/ProjectFiles.tsx')
  assert.match(files, /removesMachine\(fix\) \? t\('files_blockedMachine'/)
  assert.match(files, /blockedWord=\{\(b\) => \(b\?\.machine \? t\('files_blockedMachine'/)
  for (const L of [M.filesEn, M.filesZh]) assert.ok(L.files_blockedMachine.includes('{machine}') && L.files_blocked && L.files_fix)
})

// ---------------------------------------------------------------- R FAIL 4

test('R FAIL 4: every Start, Restart and Stop names its machine in its accessible name', () => {
  const parts = src('src/client/machines/parts.tsx')
  const power = parts.slice(parts.indexOf('export function PowerButton'), parts.indexOf('/** Seconds since'))
  const buttons = power.match(/<Button [^]*?onClick=/g)
  assert.ok(buttons.length >= 3)
  for (const b of buttons) assert.match(b, /aria-label=\{named\(/, b.slice(0, 80))
  const menu = parts.slice(parts.indexOf('export function MachineMenu'), parts.indexOf('/** Whether a machine has settings'))
  assert.equal((menu.match(/t\('powerFor', \{ action: /g) ?? []).length, 3, 'Restart worker, Stop worker and Start/Stop name the machine')
  const row = src('src/client/doctor/Row.tsx')
  assert.match(row, /label=\{w\.t\('dpFixNamed', \{ fix: fixLabel, name: w\.nameOf\(x\) \}\)\}/, 'the Doctor row\'s Start names its thing')
  for (const lang of ['en', 'zh']) {
    assert.ok(M.machineLocales[lang].powerFor.includes('{action}') && M.machineLocales[lang].powerFor.includes('{name}'), lang)
    assert.ok(M.doctorLocales[lang].dpFixNamed.includes('{name}'), lang)
  }
})

// ---------------------------------------------------------------- F6

test('F6: a press only acts for the identity it was drawn for, after the control has settled', () => {
  const S = M.PRESS_SETTLE_MS
  assert.ok(S >= 400)
  assert.equal(M.pressAllowed({ identity: 'demo-build:start', at: 0 }, 'demo-build:start', S), true)
  assert.equal(M.pressAllowed({ identity: 'demo-build:start', at: 0 }, 'demo-build:start', S - 1), false, 'a button that just appeared under the pointer ignores the press')
  assert.equal(M.pressAllowed({ identity: 'docker:stop', at: 0 }, 'demo-build:start', 10 * S), false, 'a stale handler never acts on another row')
  // the guard is on every power and fix control
  for (const [file, pattern] of [
    ['src/client/machines/parts.tsx', /const guard = usePressGuard\(`\$\{m\.name\}:/],
    ['src/client/storage/HomeCards.tsx', /const guard = usePressGuard\(`restart:\$\{m\.name\}/],
    ['src/client/storage/HomeOverview.tsx', /const guard = usePressGuard\(`\$\{n\.key\}:/],
    ['src/client/ui/index.tsx', /const guard = usePressGuard\(line\)/],
    ['src/client/doctor/Row.tsx', /settle=\{`\$\{key\}:\$\{v\.fix\.line\}:\$\{o\.st\}/],
  ]) assert.match(src(file), pattern, file)
  // the head's Stop asks a second press that names the machine
  assert.match(src('src/client/storage/HomeOverview.tsx'), /confirm: t\('ovStopConfirm', \{ machine: place\(m\.name\) \}\)/)
  for (const lang of ['en', 'zh']) assert.ok(M.storageLocales[lang].ovStopConfirm.includes('{machine}'), lang)
  // dialogs keep their clicks: a portal's press never reaches the row or card that rendered it
  assert.match(src('src/client/storage/dialogs.tsx'), /props\.dialog \? <Contained><OpenDialog \{\.\.\.props\} \/><\/Contained> : null/)
  assert.match(src('src/client/doctor/DoctorPage.tsx'), /<Contained><Modal open/)
  assert.equal((src('src/client/machines/parts.tsx').match(/return <Contained><(Settings|ForceStop|Remove)Dialog/g) ?? []).length, 3)
})

/** The Doctor page's state over a fixture world (the stopped scenario: docker stopped, its Start offered from records). */
function doctorPage(scenario) {
  const world = freshState(scenario)
  const a = treeAnswer(world)
  return M.dm.setTree(M.dm.emptyState(), { at: 0, home: { home: a.home, state: a.state, version: a.version, generation: a.generation, reason: a.reason, fix: a.fix }, tree: a.tree })
}

test('F6: the Doctor runs a fix only while its row offers that very line, unchecked and free', () => {
  const state = doctorPage('stopped')
  const line = 'layerfs machine start docker'
  assert.equal(M.dm.fixOffered(state, 'machine:docker', line), true)
  assert.equal(M.dm.fixOffered(state, 'machine:docker', 'layerfs machine start demo-build'), false, 'another line')
  assert.equal(M.dm.fixOffered(state, 'machine:gone', line), false, 'a row no longer in the tree')
  // the row re-renders to Checking (its own check, or one that covers it): its Start is not run
  const checking = M.dm.addRuns(state, [{ id: 'r1', kind: 'machine', name: 'docker', full: false, group: null }], 0)
  assert.equal(M.dm.fixOffered(checking, 'machine:docker', line), false)
  const fixing = M.dm.fixStep(state, 'machine:docker', 'f1', 'start')
  assert.equal(M.dm.fixOffered(fixing, 'machine:docker', line), false)
  assert.match(src('src/client/doctor/use-doctor.ts'), /if \(!fixOffered\(ref\.current, thing\.key, row\.line\)\) return/)
})

// ---------------------------------------------------------------- F12

test('F12: a check the host has not started reads Waiting to start with Stop, never a silent Checking', () => {
  const state = M.dm.addRuns(doctorPage('healthy'), [{ id: 'r1', kind: 'store', name: 'qv2-store', full: false, group: null }], 1000)
  const key = 'store:qv2-store'
  assert.equal(M.dm.waitingRun(state, key, 1000 + M.dm.WAIT_SHOWN_MS - 1), undefined, 'a moment to start first')
  assert.equal(M.dm.waitingRun(state, key, 1000 + M.dm.WAIT_SHOWN_MS)?.id, 'r1')
  // its process started (a `running` event, with or without progress): no longer waiting
  const started = M.dm.onRun(state, { id: 'r1', kind: 'store', name: 'qv2-store', full: false, group: null, state: 'running', started: 1000, ended: null }, 2000)
  assert.equal(started.runs.r1.spawned, true)
  assert.equal(M.dm.waitingRun(started, key, 99_000), undefined)
  // a page that opens follows what the host holds, keeping whether it started
  const held = M.dm.seed(doctorPage('healthy'), { last: [], fixes: [], running: [{ id: 'r2', kind: 'store', name: 'qv2-store', full: false, group: null, state: 'queued', started: 0, ended: null }] }, 0)
  assert.equal(M.dm.waitingRun(held, key, 5000)?.id, 'r2')
  const heldRunning = M.dm.seed(doctorPage('healthy'), { last: [], fixes: [], running: [{ id: 'r3', kind: 'store', name: 'qv2-store', full: false, group: null, state: 'running', started: 0, ended: null }] }, 0)
  assert.equal(M.dm.waitingRun(heldRunning, key, 5000), undefined)
  const row = src('src/client/doctor/Row.tsx')
  assert.match(row, /const waiting = full \? undefined : waitingRun\(state, key, ctx\.now\)/)
  assert.match(row, /onPress=\{\(\) => on\.stop\(x\)\}>\{w\.t\('dpStop'\)\}/)
  assert.match(src('src/client/doctor/use-doctor.ts'), /\?\? Object\.values\(ref\.current\.runs\)\.find\(\(r\) => r\.target === thing\.key && !r\.spawned\)/, 'Stop ends a waiting call')
  assert.match(src('src/client/doctor/DoctorPage.tsx'), /r\.kind === 'full' \|\| !r\.spawned/, 'the clock ticks while a call waits')
  for (const lang of ['en', 'zh']) assert.ok(M.doctorLocales[lang].dpWaitingTurn && M.doctorLocales[lang].dpStopWaitLabel.includes('{name}'), lang)
})

test('F5: Store and connection controls and every fix button name what they act on (visible words stay short)', () => {
  const links = src('src/client/storage/StoreLinks.tsx')
  for (const key of ['pgChangeLogin', 'pgCheck']) assert.match(links, new RegExp(`aria-label=\\{t\\('ariaFor', \\{ action: t\\('${key}'\\), name: s\\.name \\}\\)\\}`), key)
  for (const key of ['pgCheck', 'edit', 'pgForget']) assert.match(links, new RegExp(`aria-label=\\{t\\('ariaForOn', \\{ action: t\\('${key}'\\), name: place\\(name\\), store: s\\.name \\}\\)\\}`), key)
  // the PostgreSQL Store card's Update (and every other fix button) goes through FixAction, whose name comes from the runner
  assert.match(src('src/client/ui/index.tsx'), /aria-label=\{runner\.named\?\.\(fix, words\) \?\? words\}/)
  assert.match(src('src/client/storage/StoreCard.tsx'), /<FixAction line=\{resolved\.line\} fix=\{resolved\.fix\} runner=\{fixes\}/)
  const center = src('src/client/storage/StoragePage.tsx')
  assert.match(center, /named: \(fix, words\) => \{/)
  assert.match(center, /return target && !words\.includes\(target\) \? t\('ariaFor', \{ action: words, name: target \}\) : words/)
  const fill = (text, vars) => text.replace(/\{(\w+)\}/g, (_, k) => vars[k])
  assert.equal(fill(M.storageLocales.en.ariaFor, { action: M.storageLocales.en.fixRunUpdateAuth, name: 'compact-demo-pg' }), 'Update (compact-demo-pg)')
  for (const lang of ['en', 'zh']) {
    assert.ok(M.storageLocales[lang].ariaFor.includes('{name}'), lang)
    assert.ok(M.storageLocales[lang].ariaForOn.includes('{name}') && M.storageLocales[lang].ariaForOn.includes('{store}'), lang)
  }
})

// ---------------------------------------------------------------- review fix loop

test('review FAIL-1: a chain that removes data asks the Store Remove\'s typed-name confirm, listing every step', () => {
  const chain = M.doc.runnableFix(CHAIN)
  assert.equal(M.doc.typedNameOf(chain), 'sim-website-0cefe19cfdbb6fe8d224', 'the Store step\'s name is typed, as the standalone Store Remove asks')
  assert.equal(M.doc.typedNameOf(M.doc.runnableFix('layerfs project delete s/p && layerfs machine remove m')), 's/p')
  // a chain with no data-removing step keeps the single confirm press
  const plain = M.doc.runnableFix('layerfs machine prune m --unused && layerfs machine remove m')
  assert.equal(plain.kind, 'chain')
  assert.equal(M.doc.typedNameOf(plain), null)
  assert.equal(M.doc.removesSomething(plain), true, 'still a second press')
  assert.equal(M.doc.typedNameOf(M.doc.runnableFix('layerfs store remove s')), null, 'a single Store remove keeps its own confirm')
  // every step listed, numbered, the data step saying so
  const L = M.storageLocales.en
  const t = (key, vars) => L[key].replace(/\{(\w+)\}/g, (_, k) => vars[k])
  const label = (f) => f.kind === 'removeMachine' ? t('fixRunRemoveRecord', { name: f.machine }) : f.kind
  assert.deepEqual(M.chainSteps(chain, label, t), ['1. Remove Store sim-website-0cefe19cfdbb6fe8d224 and its data', '2. Remove docker’s record'])
  const ui = src('src/client/ui/index.tsx')
  const fa = ui.slice(ui.indexOf('export function FixAction'), ui.indexOf('/** The words a refusal needs'))
  assert.match(fa, /if \(fix\.kind === 'chain' && typed\) \{\n\s+if \(!runner\.confirmChain \|\| !\(await runner\.confirmChain\(fix, typed\)\)\) return/, 'no typed confirm, no run')
  assert.match(fa, /\} else if \(removes && step === 'idle'\) \{ setStep\('confirm'\); return \}/)
  const center = src('src/client/storage/StoragePage.tsx')
  assert.match(center, /kind: 'confirm', title: t\('fixChainTitle', \{ n: fix\.steps\.length \}\), body: t\('fixChainBody'\), action: t\('fixChainAction'\), name: typed,/)
  assert.match(center, /steps: chainSteps\(fix, label, t\), cancelled: \(\) => resolve\(false\), run: async \(\) => \{ resolve\(true\) \}/)
  const dialogs = src('src/client/storage/dialogs.tsx')
  assert.match(dialogs, /\{steps\?\.length \? <ol data-steps=\{steps\.length\}>/)
  assert.match(dialogs, /onClose=\{\(\) => \{ dialog\.cancelled\?\.\(\); onClose\(\) \}\}/)
  assert.match(dialogs, /<TextField label=\{t\('typeToConfirm', \{ name \}\)\}/, 'the same typed-name field as Store Remove')
  for (const lang of ['en', 'zh']) for (const key of ['fixChainTitle', 'fixChainBody', 'fixChainAction', 'fixStepRemoveStore', 'fixStepDelete']) assert.ok(M.storageLocales[lang][key], `${lang} ${key}`)
})

test('review NOTE-1: the settle is visible (off and aria-disabled, one re-render at its end) and keyboard presses go through it', () => {
  const seen = { current: { identity: 'demo-build:start', at: 1000 } }
  let ran = 0
  let clock = 1000 + 100
  const press = M.guardedPress(seen, 'demo-build:start', M.PRESS_SETTLE_MS, () => { ran++ }, () => clock)
  const keyboardClick = () => { const e = { detail: 0, key: 'Enter', stopped: false, stopPropagation() { this.stopped = true } }; press(e); return e }
  assert.equal(keyboardClick().stopped, true, 'Enter inside the settle is dropped, as a pointer press is')
  assert.equal(ran, 0)
  clock = 1000 + M.PRESS_SETTLE_MS
  keyboardClick()
  assert.equal(ran, 1, 'after the settle Enter acts')
  seen.current = { identity: 'docker:stop', at: 0 }
  clock = 99_999
  keyboardClick()
  assert.equal(ran, 1, 'a handler drawn for another identity never acts, by keyboard either')
  const ui = src('src/client/ui/index.tsx')
  assert.match(ui, /const settling = settleMs > 0 && !pressAllowed\(seen\.current, identity, Date\.now\(\), settleMs\)/)
  assert.match(ui, /setTimeout\(\(\) => rerender\(\(n\) => n \+ 1\), Math\.max\(0, seen\.current\.at \+ settleMs - Date\.now\(\)\) \+ 5\)/, 'one timer re-render at at + settleMs')
  for (const [file, n] of [['src/client/machines/parts.tsx', 5], ['src/client/storage/HomeOverview.tsx', 2], ['src/client/storage/HomeCards.tsx', 1], ['src/client/ui/index.tsx', 1]]) {
    assert.equal((src(file).match(/aria-disabled=\{guard\.settling \|\| undefined\}/g) ?? []).length, n, file)
    assert.ok((src(file).match(/disabled=\{[^}]*guard\.settling\}/g) ?? []).length >= n, `${file} visibly off`)
  }
  assert.match(src('src/client/doctor/Row.tsx'), /aria-disabled=\{busy \|\| off \|\| guard\.settling \|\| undefined\}/, 'the Doctor button is drawn off by its aria-disabled style')
})

// ---------------------------------------------------------------- U12b

test('F7: the "close its Sandbox first" panel follows the Sandbox list and goes when they are gone', () => {
  const refusal = { code: 'InUse', message: 'Sandboxes run', detail: { sandboxes: ['sbm', 'sbm2'] } }
  assert.deepEqual(M.notices.inUseNow(refusal, ['sbm', 'sbm2', 'other']), { names: ['sbm', 'sbm2'], cleared: false })
  assert.deepEqual(M.notices.inUseNow(refusal, ['sbm2']), { names: ['sbm2'], cleared: false })
  assert.deepEqual(M.notices.inUseNow(refusal, []), { names: [], cleared: true })
  assert.equal(M.notices.inUseNow({ code: 'Busy', message: 'x' }, []), null)
  const detail = src('src/client/machines/MachineDetail.tsx')
  assert.match(detail, /const held = lastError !== null \? inUseNow\(lastError, sandboxesOn\(view, m\)\.map\(\(b\) => b\.name\)\) : null/)
  assert.match(detail, /const error = held\?\.cleared \? null : lastError/)
})

test('F11: a failed job\'s notice goes after a later success on its Store, or when the Store is gone', () => {
  const failed = { id: 'a', kind: 'compact', store: 's', ended: 10, error: { code: 'MountsActive' } }
  const stores = [{ name: 's' }]
  assert.equal(M.sm.failureSuperseded(failed, [failed], stores), false)
  assert.equal(M.sm.failureSuperseded(failed, [failed, { id: 'b', kind: 'compact', store: 's', ended: 20, error: null }], stores), true)
  assert.equal(M.sm.failureSuperseded(failed, [failed, { id: 'b', kind: 'compact', store: 's', ended: 5, error: null }], stores), false, 'an earlier success does not')
  assert.equal(M.sm.failureSuperseded(failed, [failed, { id: 'b', kind: 'backup', store: 's', ended: 20, error: null }], stores), false, 'another kind does not')
  assert.equal(M.sm.failureSuperseded(failed, [failed], []), true, 'its Store removed')
  assert.match(src('src/client/storage/HomeOverview.tsx'), /visibleFailures\(props\.jobs, view\.stores, props\.dismissed\)/)
})

test('F10: an in-use compaction is a word, its holders as the tip and Open <Sandbox>; never "failed in mark" or a raw refusal', () => {
  const t = (key, vars) => `${key} ${JSON.stringify(vars)}`
  const job = { id: 'j', kind: 'compact', store: 's', target: 's', started: 1, ended: 2, phase: 'mark', error: { code: 'MountsActive', message: 'mounts active: 2 holder(s)', phase: 'mark' } }
  assert.match(M.sm.endedWords(job, t), /^jobInUse /)
  assert.equal(M.sm.inUseJob(job), true)
  assert.match(M.sm.endedWords({ ...job, error: { code: 'Capacity', message: 'full', phase: 'mark' } }, t), /"phase":"stepMark[ "]/, 'a step in words, not its code')
  assert.deepEqual(M.sm.storeHolders('s', [{ name: 'sbn', project: 's/p' }, { name: 'x', project: 'other/p' }, { name: 'y', project: null }]).map((b) => b.name), ['sbn'])
  const home = src('src/client/storage/HomeOverview.tsx')
  assert.match(home, /const \{ holders: \[holder\], tip \} = jobEnd\(j, fleet\?\.sandboxes \?\? \[\], t,/)
  assert.match(home, /holder \? \{ label: t\('jobOpenSandbox', \{ name: holder\.name \}\), run: \(\) => props\.onOpen\(holderTab\(holder\)\) \}/)
  const tt = (key, vars) => (vars ? `${key} ${JSON.stringify(vars)}` : key)
  const end = M.sm.jobEnd(job, [{ name: 'sbn', machine: 'lima1', project: 's/p' }], tt)
  assert.deepEqual(end.holders.map((b) => b.name), ['sbn'])
  assert.equal(end.tip.split('\n')[0], 'jobInUseHolders {"holders":"sbn"}', 'the holders lead the tip')
  assert.equal(M.sm.holderTab(end.holders[0]), 'sandbox:lima1/sbn')
  assert.equal(M.sm.holderTab({ name: 'x', machine: null }), 'sandbox:native/x')
  const panel = src('src/client/storage/JobProgress.tsx')
  assert.doesNotMatch(panel, /<Refusal error=\{j\.error\}/, 'no raw refusal under the job')
  assert.match(panel, /title=\{tip\}/)
  for (const lang of ['en', 'zh']) for (const key of ['jobInUse', 'jobInUseHolders', 'jobOpenSandbox']) assert.ok(M.storageLocales[lang][key], `${lang} ${key}`)
  assert.doesNotMatch(M.storageLocales.en.jobFailed, / in \{phase\}/)
})

test('F13: an image the free-space dialog could not remove reads Not removed; why and the message are the tip, no digest inline', () => {
  const images = src('src/client/machines/MachineImages.tsx')
  const refused = images.slice(images.indexOf('function Refused'), images.indexOf('export function ImagesTab'))
  assert.match(refused, /\{t\('pruneRefusedWord', \{ image: names\.get\(r\.id\) \?\? r\.id \}\)\}/)
  assert.doesNotMatch(refused, /t\('pruneRefused',/, 'the message is no longer the text')
  assert.doesNotMatch(refused, /r\.message\]/, 'no raw message (full sha256) in the tip')
  for (const lang of ['en', 'zh']) assert.ok(!M.machineLocales[lang].pruneRefusedWord.includes('{message}') && M.machineLocales[lang].pruneWhyInUse, lang)
})

test('F9: a record that cannot be read shows as such (no Running, no Stop) and the banners keep the path in their tip', () => {
  const err = { code: 'Failed', message: '/Users/x/.layerfs/machine/cs20ui-m/record.json: unexpected end at byte 14' }
  assert.equal(M.mm.unreadableNamed(err), 'cs20ui-m')
  assert.equal(M.mm.unreadableNamed({ code: 'Failed', message: 'no such thing' }), null)
  const view = { available: true, machines: [{ name: 'cs20ui-m', platform: 'lima', state: 'ready', reason: null, fix: null, stores: [], sandboxes: [] }, { name: 'b', platform: 'lima', state: 'ready', stores: [], sandboxes: [] }] }
  const stale = M.mm.staleView(view, err)
  const m = stale.machines[0]
  assert.equal(M.mm.unreadableRecord(m), true)
  assert.equal(M.mm.powerOf(m), null, 'no Start or Stop')
  assert.equal(M.mm.machineState(m).label, 'stateUnreadable')
  assert.equal(stale.machines[1], view.machines[1], 'the others as last read')
  assert.equal(M.mm.staleView(view, { code: 'Busy', message: 'later' }), view)
  const board = src('src/client/machines/Board.tsx')
  assert.doesNotMatch(board, /<Refusal error=\{read\.error\}/, 'no raw path or parse error inline')
  assert.match(board, /t\('staleUnreadable', \{ name: unreadableNamed\(read\.error\)! \}\)/)
  assert.match(board, /title=\{`\$\{e\.error\.code\}: \$\{e\.error\.message\}`\}>\{t\('partUnreadWord'/)
  assert.match(src('src/client/machines/controller.ts'), /data: staleView\(prev\.data, error\)/)
  assert.match(src('src/client/machines/Spec.tsx'), /live\?\.error && m\.state !== 'ready' && m\.state !== 'stopped' &&/, 'never "Down because: Running"')
})

test('F14: a stats read from before the machine\'s last start is not shown as its facts; a start reads again at once', () => {
  const read = { stats: { resources: {} }, at: 100, stale: null, reading: false, loaded: true }
  assert.equal(M.metrics.sinceStart(read, null), read)
  assert.equal(M.metrics.sinceStart(read, 50), read, 'read after the start')
  assert.deepEqual(M.metrics.sinceStart(read, 200), { stats: null, at: null, stale: null, reading: true, loaded: false })
  // the poller reads at once on refresh
  let reads = 0
  const clock = { now: () => 1000, setTimeout: () => 1, clearTimeout: () => {} }
  const poller = new M.metrics.MetricsPoller(async () => { reads++; return { at: 1000, machines: {}, sandboxes: {} } }, () => true, clock)
  poller.watch('m')
  return new Promise((resolve) => setTimeout(() => {
    const before = reads
    poller.refresh()
    setTimeout(() => {
      assert.ok(reads > before, 'refresh polled now')
      poller.dispose()
      assert.match(src('src/client/machines/controller.ts'), /status: 'success', endedAt: Date\.now\(\) \}\)\)\n\s+\/\/ a started machine's facts changed: read its stats now \(F14\)\n\s+this\.metrics\.refresh\(\)/)
      assert.match(src('src/client/machines/MachineStats.tsx'), /sinceStart\(statsReadOf\(snapshot, m\.name\), startedAt\)/)
      resolve()
    }, 10)
  }, 10))
})

// ---------------------------------------------------------------- U12b fix loop

test('U12b FAIL-1: a failed restore as a new Store keeps its Home notice though that Store was never made', () => {
  const restore = { id: 'r', kind: 'restore', store: 'shop-restored', ended: 10, error: { code: 'Failed', message: 'bad file' } }
  const stores = [{ name: 'shop' }]
  assert.equal(M.sm.failureSuperseded(restore, [restore], stores), false, 'stays visible')
  assert.equal(M.sm.failureSuperseded(restore, [restore, { id: 'r2', kind: 'restore', store: 'shop-restored', ended: 20, error: null }], stores), true, 'a later restore that worked ends it')
  const compact = { id: 'c', kind: 'compact', store: 'gone', ended: 10, error: { code: 'MountsActive', message: 'x' } }
  assert.equal(M.sm.failureSuperseded(compact, [compact], stores), true, 'other kinds: their Store gone still ends it')
})

test('U12b FAIL-2: the Store page and Store cards word an in-use job like Home: holders tip, one action Open <Sandbox>', () => {
  const panel = src('src/client/storage/JobProgress.tsx')
  assert.match(panel, /const \{ holders: \[holder\], tip \} = jobEnd\(j, sandboxes, t,/, 'the same tip as Home')
  const action = panel.slice(panel.indexOf('const action ='), panel.indexOf('return (', panel.indexOf('const action =')))
  assert.match(action, /resolved\?\.fix && fixes \? <FixAction[^]*: holder && onOpen \? <Button[^]*t\('jobOpenSandbox', \{ name: holder\.name \}\)[^]*: onRetry \? <Button[^]*t\('jobRetry'\)/, 'one action: fix, else Open <Sandbox>, else Retry')
  assert.equal((panel.match(/\{action\}/g) ?? []).length, 1)
  assert.doesNotMatch(panel, /function JobFix|<JobFix/, 'no second (fix) action beside Retry')
  assert.match(panel, /mc\.useMachines\(\(s\) => \(s\.data\?\.available \? s\.data\.sandboxes : NONE\)\)/)
  assert.match(panel, /onOpen=\{mc\.openStorageTab\}/)
  for (const file of ['src/client/storage/StorePage.tsx', 'src/client/storage/MachineStores.tsx']) assert.match(src(file), /<JobPanel job=/, file)
})

test('U12b FAIL-3: a refused image tip has digests cut (no raw message), and Open Sandboxes is its one action when in use', () => {
  const images = src('src/client/machines/MachineImages.tsx')
  const refused = images.slice(images.indexOf('function Refused'), images.indexOf('export function ImagesTab'))
  assert.match(refused, /title=\{refusedInUse\(r\) \? t\('pruneWhyInUse'\) : shortDigests\(r\.message\)\}/)
  assert.match(refused, /refused\.some\(refusedInUse\) \? <Button[^]*go\(\{ kind: 'machine', name: machine, tab: 'sandboxes' \}\)[^]*t\('inUseOpenSandboxes'\)/)
  assert.match(images, /<Refused refused=\{refused\} names=\{names\} machine=\{machine\.name\} onLeave=\{close\} \/>/, 'the dialog closes before it goes')
  assert.equal(M.refusedInUse({ id: 'a', code: 'InUse', message: 'x' }), true)
  assert.equal(M.refusedInUse({ id: 'a', code: 'Failed', message: 'image sha256:abc is used by a sandbox' }), true)
  assert.equal(M.refusedInUse({ id: 'a', code: 'Failed', message: 'disk error' }), false)
  for (const lang of ['en', 'zh']) assert.ok(M.machineLocales[lang].inUseOpenSandboxes, lang)
})

test('U12b LF10: blocked_by beside error (CLI JSON, Node SDK error) reaches the Files tab word', () => {
  const blocked_by = { machine: 'rt9lf', reason: 'outdated' }
  const line = { error: 'Unavailable', message: 'Store rt9lfs is blocked by machine rt9lf (outdated): …', fix: 'layerfs store remove rt9lfs && layerfs machine remove rt9lf', blocked_by }
  // the host's CLI path: with and without --detail ("detail": null)
  for (const frame of [line, { ...line, detail: null }]) {
    const r = M.cliRefusalOf(frame)
    assert.deepEqual(r.detail, { blocked_by }, JSON.stringify(frame))
    assert.deepEqual(M.blockedOf(r.detail), blocked_by)
  }
  // an SDK-shaped error (err.blocked_by, err.detail null) through the host's error answer
  const sdk = Object.assign(new Error(line.message), { code: 'Unavailable', detail: null, fix: line.fix, blocked_by })
  const body = M.errorAnswer(sdk).body
  assert.deepEqual(body.detail, { blocked_by })
  assert.deepEqual(M.blockedOf(body.detail), blocked_by)
  // the page reads it where it arrives: on the error itself, else its detail
  assert.deepEqual(M.blockedOf(sdk), blocked_by)
  assert.equal(M.errorAnswer(Object.assign(new Error('x'), { code: 'Unavailable', detail: { blocked_by: { machine: 'a', reason: 'stopped' } }, blocked_by })).body.detail.blocked_by.machine, 'a', 'detail wins on a clash')
})

test('U12b notes: unknown step in words, refresh during a poll reads again, F7 refusal spent once cleared', async () => {
  const t = (key) => key
  assert.equal(M.sm.stepWords('rehash-v9', t), 'stepWorking')
  for (const lang of ['en', 'zh']) assert.ok(M.storageLocales[lang].stepWorking, lang)
  let reads = 0, release
  const clock = { now: () => 1000, setTimeout: () => 1, clearTimeout: () => {} }
  const poller = new M.metrics.MetricsPoller(() => { reads++; return new Promise((r) => { release = () => r({ at: 1000, since: 1, epoch: 'e', machines: {}, sandboxes: {} }) }) }, () => true, clock)
  poller.watch('m')
  await new Promise((r) => setTimeout(r, 5))
  assert.equal(reads, 1)
  poller.refresh() // while the first poll flies
  release()
  await new Promise((r) => setTimeout(r, 5))
  assert.equal(reads, 2, 'the refresh was not cancelled by the in-flight poll')
  poller.dispose()
  assert.match(src('src/client/machines/MachineDetail.tsx'), /useEffect\(\(\) => \{ if \(held\?\.cleared\) clearError\(\) \}, \[held\?\.cleared\]\)/)
  const notices = src('src/client/machines/notices.ts')
  assert.ok(notices.indexOf('export function inUseSandboxes') < notices.indexOf('An InUse refusal against the Sandboxes there are now'), 'inUseNow\'s doc sits by its own function')
})

// ---------------------------------------------------------------- Step 7 fix loop 1 (R1 on 3080 at d7c583e)

test('R1 F6: a broken PostgreSQL Store makes its sessions unavailable (owner Q-R3): word, tip, one fix; no Open or New session', () => {
  const row = { store: 'demo-experiments', engine: 'postgresql', auth: 'demo', state: 'broken', reason: 'outdated', fix: 'layerfs store update demo-experiments' }
  // the live read answers reach; the record's outdated holds until its fix
  assert.equal(M.classifyStore(row, { state: 'ready' }).state, 'unavailable')
  assert.equal(M.classifyStore(row).state, 'unavailable')
  assert.equal(M.classifyStore({ ...row, reason: 'missing' }, { state: 'ready' }).state, 'unavailable')
  assert.equal(M.classifyStore({ ...row, reason: 'unreachable' }, { state: 'ready' }).state, 'available', 'reach is the live read\'s to answer')
  assert.equal(M.classifyStore({ ...row, state: 'ready', reason: null }, { state: 'ready' }).state, 'available')
  assert.equal(M.sm.sessionsUnavailable({ state: 'broken' }), true)
  assert.equal(M.sm.sessionsUnavailable({ state: 'blocked' }), true)
  assert.equal(M.sm.sessionsUnavailable({ state: 'ready' }), false)
  assert.equal(M.sm.sessionsUnavailable({ state: 'busy' }), false)
  assert.equal(M.sm.sessionsUnavailable(undefined), false)
  const page = src('src/client/storage/ProjectPage.tsx')
  const sessions = page.slice(page.indexOf('function Sessions('), page.indexOf('function Stats('))
  assert.match(sessions, /const down = sessionsUnavailable\(store\)/)
  assert.match(sessions, /const create = down \? undefined :/)
  assert.match(sessions, /\{down && store \? <StoreBroken s=\{store\} t=\{t\} \/> : null\}/)
  assert.match(sessions, /\{down \? null : <Button variant="outline" size="sm"[^]*t\('sessionsOpen'\)\}<\/Button>\}/)
  assert.match(page, /<Sessions api=\{api\} t=\{t\} project=\{project\} tick=\{tick\} store=\{store\} \/>/)
  const table = src('src/client/storage/ProjectTable.tsx')
  assert.match(table, /const blockedSession = sessionsUnavailable\(store\) \? t\('ovSessionBlocked'\) : undefined/)
  assert.match(table, /down=\{sessionsUnavailable\(store\)\}/)
  assert.match(table, /action=\{<button type="button" disabled=\{down\} title=\{down \? t\(\'ovSessionBlocked\'\)/)
  assert.match(table, /className=\{clsx\(css\.line, css\.sessionLine\)\} disabled=\{down\}/)
  assert.match(table, /store: \{ name: s\.name, kind: s\.kind, state: s\.state \}/)
  assert.match(src('src/client/storage/StorePage.tsx'), /<StoreProjects api=\{api\} store=\{\{ name: s\.name, kind: s\.kind, state: s\.state \}\}/)
})

test('R1 F3: New project offers a PostgreSQL Store only running, not-broken machines that run PostgreSQL Sandboxes', () => {
  assert.equal(M.sm.pgPlaceable({ state: 'stopped', supports: ['postgresql_sandbox'] }), false)
  assert.equal(M.sm.pgPlaceable({ state: 'broken', supports: ['postgresql_sandbox'] }), false)
  assert.equal(M.sm.pgPlaceable({ state: 'ready', supports: ['sqlite'] }), false)
  assert.equal(M.sm.pgPlaceable({ state: 'ready', supports: ['postgresql_sandbox'] }), true)
  // default: last used, then connected from (reached), then native; never a stopped one
  const machines = [{ name: 'native', state: 'ready' }, { name: 'demo-research', state: 'stopped' }, { name: 'lima1', state: 'ready' }]
  assert.equal(M.sm.pgDefaultMachine({ connectedFrom: [{ machine: 'demo-research', reached: 'ok' }, { machine: 'lima1', reached: 'ok' }] }, machines), 'lima1')
  assert.equal(M.sm.pgDefaultMachine({ connectedFrom: [{ machine: 'demo-research', reached: 'ok' }] }, machines), 'native')
  assert.equal(M.sm.pgDefaultMachine({ connectedFrom: [] }, machines, 'lima1'), 'lima1')
  const dialog = src('src/client/projects/setup/ProjectDialog.tsx')
  assert.match(dialog, /const offered = kind === 'postgresql' \? machines\.filter\(\(m\) => pgPlaceable\(m\) && !blocks\.has\(m\.name\)\) : machines/)
  assert.match(dialog, /<LocationPicker machines=\{offered\} value=\{chosenPg\}/)
  assert.match(dialog, /2: chosen\.length > 0 && chosen\.every\(\(m\) => offered\.some/)
  assert.match(dialog, /const pgPicked = pgChoice && offered\.some\(\(m\) => m\.name === pgChoice\) \? pgChoice : null/)
})

test('R1 F1: Home words an outdated or login-missing PostgreSQL Store as needing its update, with the one fix; never "down"', () => {
  const home = src('src/client/storage/HomeOverview.tsx')
  assert.match(home, /const needsKey = s\.kind !== 'postgresql' \? null : needsLogin\(s\) \|\| s\.reason === 'missing' \? 'ovStoreNeedsLogin' : s\.reason === 'outdated' \? 'ovStoreNeedsUpdate' : null/)
  assert.match(home, /t\(pgDown \? 'ovPgUnreachable' : needsKey \?\? 'ovStoreDown', \{ store: s\.name \}\)/)
  assert.match(home, /action: fix && fixes \? \{ label: fixes\.label\(fix\), run: \(\) => fixes\.run\(fix\) \} : \{ label: t\('ovOpen'\)/)
  for (const lang of ['en', 'zh']) for (const key of ['ovStoreNeedsUpdate', 'ovStoreNeedsLogin']) assert.ok(M.storageLocales[lang][key], `${lang} ${key}`)
  assert.doesNotMatch(M.storageLocales.en.ovStoreNeedsUpdate, /down/)
})

test('R1 F2: Connect a database names each Check, Connect and New database by its Store or server', () => {
  const cd = src('src/client/storage/ConnectDatabase.tsx')
  assert.match(cd, /aria-label=\{t\('cdCheckTitle', \{ store: s\.name \}\)\} onClick=\{\(\) => onCheck\(s\)\}>\{t\('cdCheck'\)\}/)
  assert.match(cd, /aria-label=\{t\('cdConnectMachineNamed', \{ store: s\.name \}\)\} onClick=\{\(\) => onCheck\(s\)\}>\{t\('pgConnectMachineShort'\)\}/)
  assert.match(cd, /aria-label=\{t\('cdNewOnServerNamed', \{ server: g\.server \}\)\} onClick=\{\(\) => onNew\(g\)\}/)
  for (const lang of ['en', 'zh']) for (const key of ['cdCheckTitle', 'cdConnectMachineNamed', 'cdNewOnServerNamed']) assert.match(M.storageLocales[lang][key], /\{(store|server)\}/, `${lang} ${key}`)
})

test('R1 F4/F5: the Connection status parts stand apart; the password input stays inside its column', () => {
  const links = src('src/client/storage/StoreLinks.tsx')
  const from = links.indexOf('className={css.linkCheck} data-state={broken')
  const line = links.slice(from, links.indexOf('className={css.linkActions}', from))
  assert.equal((line.match(/css\.linkPart/g) ?? []).length, 4)
  const pageCss = src('src/client/storage/StorePage.module.css')
  assert.match(pageCss, /\.linkCheck \{[^}]*display: flex;[^}]*gap: 4px 6px;/)
  assert.match(pageCss, /\.linkPart \+ \.linkPart::before \{\s*content: '·';/)
  assert.match(src('src/client/storage/Storage.module.css'), /\.passwordPart \.passwordInput \{[^}]*box-sizing: border-box;[^}]*max-width: 100%;/)
})

// ---------------------------------------------------------------- Step 7 fix loop 2 (M and R2 on 3080)

test('M2 (V23-13): Connect a database refuses a URL naming a login, with LayerFS\'s fix line; nothing is created; other options go through', () => {
  const base = 'postgresql://127.0.0.1:56968/x'
  for (const q of ['?passfile=/x', '?sslpassword=s', '?password=p', '?user=u']) assert.ok(M.PG_URL_LOGIN.test(base + q), q)
  assert.ok(M.PG_URL_LOGIN.test('postgresql://app@db:5432/x'), 'a user part')
  assert.ok(M.PG_URL_LOGIN.test('postgresql://app:pw@db:5432/x'), 'a password part')
  // V23-13 (spec NOTE): sslkey/sslcert name a client-certificate file, not a secret: allowed
  assert.equal(M.PG_URL_LOGIN.test(base + '?sslkey=/k.pem'), false)
  assert.equal(M.PG_URL_LOGIN.test(base + '?sslmode=require'), false)
  assert.equal(M.loginInUrlFix('postgresql://app:pw@db:5432/orders?sslmode=require&passfile=/x', 'orders'),
    'layerfs store save-auth <auth> --user app --password-stdin && layerfs store connect orders postgresql://db:5432/orders?sslmode=require --auth <auth>')
  assert.equal(M.loginInUrlFix(base + '?sslpassword=s&user=u', ''), `layerfs store save-auth <auth> --user u --password-stdin && layerfs store connect <store> ${base} --auth <auth>`)
  const cd = src('src/client/storage/ConnectDatabase.tsx')
  assert.match(cd, /const loginInUrl = !server && PG_URL_LOGIN\.test\(withScheme\(url\)\)/)
  assert.match(cd, /const urlError = loginInUrl \? t\('cdLoginInUrl'\)/)
  assert.match(cd, /const valid = [^\n]*!urlError/, 'Connect stays off: nothing is created')
  assert.match(cd, /\{loginInUrl \? <p className=\{scss\.formNote\} data-login-fix>[^\n]*loginInUrlFix\(url, storeName\)/)
  assert.match(cd, /url: `\$\{pgUrl\(\{ user: '', host, database \}\)\}\$\{options\}`/, 'its other options are not dropped')
  for (const lang of ['en', 'zh']) for (const key of ['cdLoginInUrl', 'cdLoginFix']) assert.ok(M.storageLocales[lang][key], `${lang} ${key}`)
})

test('M3b: opening Connect a machine runs no check; the named "Check <store> from <machine>" does', () => {
  const cd = src('src/client/storage/ConnectDatabase.tsx')
  assert.match(cd, /machine: startCheck\.machine \?\? fixed \?\? 'native', state: 'ready' \}/)
  assert.doesNotMatch(cd, /useEffect\(\(\) => \{ if \(screen\.kind === 'check' && screen\.check\.state === 'running'\) void check/, 'no check on open')
  assert.match(cd, /check\?\.state === 'ready'\s*\? <Button variant="primary" onClick=\{onRun\}>\{t\('cdCheckFromNamed', \{ store: check\.store, machine: place\(check\.machine\) \}\)\}/)
  assert.match(cd, /if \(screen\.check\.state === 'ready'\) setScreen\(\{ kind: 'check', check: \{ \.\.\.screen\.check, machine \} \}\)/, 'choosing From before the press checks nothing')
  for (const lang of ['en', 'zh']) assert.match(M.storageLocales[lang].cdCheckFromNamed, /\{store\}[^]*\{machine\}|\{machine\}[^]*\{store\}/)
})

test('M9: a lost VM (missing) is "VM gone": not worker down, no Stop or Restart, LayerFS\'s fix as the one action', () => {
  const gone = { name: 'lm', platform: 'lima', state: 'broken', reason: 'missing', fix: 'layerfs machine remove lm', stores: ['bs'], sandboxes: [] }
  assert.equal(M.mm.vmGone(gone), true)
  assert.equal(M.mm.workerDown(gone), false)
  assert.equal(M.mm.powerOf(gone), null)
  assert.equal(M.mm.machineState(gone).label, 'stateVmGone')
  assert.equal(M.mm.vmGone({ ...gone, platform: 'native' }), false)
  assert.equal(M.mm.workerDown({ ...gone, reason: 'not_answering' }), true)
  const home = src('src/client/storage/HomeOverview.tsx')
  assert.match(home, /\} else if \(vmGone\(m\)\) \{[^]*?t\('ovVmGoneStores'[^]*?action: fix && fixes \? \{ label: fixes\.label\(fix\), run: \(\) => fixes\.run\(fix\) \}/)
  assert.match(src('src/client/machines/Spec.tsx'), /vmGone\(m\) \? t\('readinessVmGone'\)/)
  assert.match(src('src/client/machines/MachineDetail.tsx'), /t\(recordOutdated\(m\) \|\| vmGone\(m\) \?/)
  for (const lang of ['en', 'zh']) {
    for (const key of ['ovVmGone', 'ovVmGoneStores']) assert.ok(M.storageLocales[lang][key], `${lang} ${key}`)
    for (const key of ['stateVmGone', 'readinessVmGone']) assert.ok(M.machineLocales[lang][key], `${lang} ${key}`)
  }
  assert.doesNotMatch(M.storageLocales.en.ovVmGoneStores, /answer/)
})

test('R2-2: a Store on a stopped machine shows a dot, word, tip and its fix; its size tip says stopped, not "does not answer"', () => {
  const ms = src('src/client/storage/MachineStores.tsx')
  const status = ms.slice(ms.indexOf('function StoreStatus'), ms.indexOf('function SqliteCard'))
  assert.match(status, /<StateDot state=\{s\.state === 'broken' \? 'error' : 'idle'\} size=\{8\} \/>/)
  assert.match(status, /const tip = useStoreTip\(s, t\) \?\? s\.fix \?\? undefined/)
  assert.match(ms, /t\(stoppedBlock\(s\) \? 'msSizeStaleStoppedTitle' : 'msSizeStaleTitle'/)
  const card = src('src/client/storage/StoreCard.tsx')
  assert.match(card, /\(s\.state === 'broken' \|\| s\.state === 'blocked'\) \? resolveFix\(fixes, s\.fix\)/)
  assert.match(card, /return s\.state === 'blocked' \? <StoreFixAction store=\{s\} t=\{t\} \/> : null/, 'a blocked Store with a fix from LayerFS: that fix (since loop 3, a stopped machine with none gets Start <m>)')
  for (const lang of ['en', 'zh']) assert.doesNotMatch(M.storageLocales[lang].msSizeStaleStoppedTitle, /answer|响应/)
})

test('M4/M5: a timed-out reach check is a server failure (Give own address); the record-only remove names the Stores it holds', () => {
  assert.equal(M.ms.failedStep('Unknown', 'LayerFS did not answer in time and was stopped'), 'server')
  assert.equal(M.ms.failedStep('Unknown', 'something else'), null)
  assert.equal(M.ms.failedStep('Unavailable', 'connection refused (os error 61)'), 'server')
  const parts = src('src/client/machines/parts.tsx')
  assert.match(parts, /recordOnly \? \(m\.stores\.length \? t\('removeRecordHeldBody', \{ name: m\.name, stores: m\.stores\.join\(', '\) \}\) : t\('removeRecordBody'/)
  for (const lang of ['en', 'zh']) assert.match(M.machineLocales[lang].removeRecordHeldBody, /\{stores\}/)
})

test('M8/M1: Delete Sandbox titles and the platform Create name their resource', () => {
  for (const lang of ['en', 'zh']) for (const key of ['deleteTitle', 'forceDeleteTitle']) assert.match(M.machineLocales[lang][key], /\{name\}/, `${lang} ${key}`)
  assert.match(src('src/client/machines/Sandboxes.tsx'), /t\(confirm === 'force' \? 'forceDeleteTitle' : 'deleteTitle', \{ name \}\)/)
  assert.match(src('src/client/machines/AddMachineDialog.tsx'), /aria-label=\{t\('createPlatformNamed', \{ platform: kind\(p\.platform\) \}\)\}/)
  for (const lang of ['en', 'zh']) assert.match(M.machineLocales[lang].createPlatformNamed, /\{platform\}/)
})

test('M10: a login header follows its Stores\' worst reach (no green Ready over red chips); the reach errors stay in its tip', () => {
  const ms = src('src/client/storage/MachineStores.tsx')
  assert.match(ms, /const failed = g\.state === 'ready' \? g\.stores\.flatMap\(\(st\) => \(st\.connectedFrom \?\? \[\]\)\.filter\(\(c\) => c\.reached === 'failed'\)\) : \[\]/)
  assert.match(ms, /data-health=\{healthOf\(state\)\} \/>\{failed\.length \? t\('pgLoginNotReached'\) : storeStateWords\(g\.state, t\)\}/)
  assert.match(ms, /title=\{\[loginTip\(g, t, now\), \.\.\.causes\]\.join\('\\n'\)\}/)
  for (const lang of ['en', 'zh']) assert.ok(M.storageLocales[lang].pgLoginNotReached, lang)
})

// ---------------------------------------------------------------- Step 7 fix loop 3 (K on 3080)

test('K FAIL 2 (V23-13): a login URL leaves Connect disabled, with its reason beside it; the URL input is named', () => {
  const cd = src('src/client/storage/ConnectDatabase.tsx')
  assert.match(cd, /<Button variant="primary" type="submit" form="lfs-cd-new" disabled=\{!!why\} aria-disabled=\{!!why \|\| undefined\} title=\{why \?\? undefined\}>\{t\('connect'\)\}/)
  assert.match(cd, /const why = valid \? null : urlError \?\?/, 'a login URL (urlError) is the first reason')
  assert.match(cd, /const valid = [^\n]*!urlError/, 'a login URL makes the form not valid')
  assert.match(cd, /useEffect\(\(\) => \{ onWhy\(why\) \}, \[why\]\)/)
  assert.match(cd, /screen\.kind === 'new' && why \? <span className=\{scss\.modalHint\} data-connect-why>\{why\}<\/span>/, 'the reason is shown, not only a tooltip')
  assert.match(cd, /onWhy=\{setNewWhy\}/)
  assert.match(src('src/client/storage/parts.tsx'), /spellCheck=\{false\} aria-label=\{label\}/, 'TextField inputs (the Connection URL) carry their label as their name')
})

test('K FAIL 1 (R2-2): a Store blocked by its stopped machine (fix null) reads Waiting on <m>, with a tip and one guarded Start <m>', () => {
  const t = (key, vars) => `${key} ${JSON.stringify(vars ?? {})}`
  const s = { name: 'demo-build-cache', kind: 'sqlite', state: 'blocked', blockedBy: { machine: 'demo-build', reason: 'stopped' }, fix: null }
  assert.equal(M.stoppedBlock(s), true)
  assert.equal(M.stoppedBlock({ ...s, blockedBy: { machine: 'demo-build', reason: 'not_answering' } }), false, 'a broken machine keeps its Repair/fix route')
  assert.equal(M.storeNotice(s, null, t), 'waitingOnMachine {"machine":"demo-build"}')
  const card = src('src/client/storage/StoreCard.tsx')
  assert.match(card, /if \(stoppedBlock\(s\) && s\.blockedBy\?\.machine && !s\.message\) return t\('waitingOnMachineTip', \{ machine: s\.blockedBy\.machine, store: s\.name \}\)/)
  assert.match(card, /if \(stoppedBlock\(s\) && !s\.fix && s\.blockedBy\?\.machine && onStart\) return <MachineStartAction machine=\{s\.blockedBy\.machine\} t=\{t\} onStart=\{onStart\} \/>/)
  const press = card.slice(card.indexOf('export function MachineStartAction'))
  assert.match(press, /const guard = usePressGuard\(`start:\$\{machine\}`\)/, 'the same press guard (600 ms settle) as other power controls')
  assert.match(press, /disabled=\{guard\.settling\} aria-disabled=\{guard\.settling \|\| undefined\} aria-label=\{t\('startMachineNamed', \{ machine \}\)\}/)
  assert.match(press, /onClick=\{guard\(\(e\) => \{ e\.stopPropagation\(\); onStart\(\) \}\)\}/)
  // the existing Store start (StoragePage's StoreActions.start), so no new machine-start press site (u11 NOTE 3 list unchanged)
  const ms = src('src/client/storage/MachineStores.tsx')
  assert.match(ms, /<StoreAction store=\{s\} t=\{t\} onStart=\{onStart\} \/>/)
  assert.equal((ms.match(/words=\{notice\} onStart=\{on\.start\} \/>/g) ?? []).length, 1, 'the sqlite card only')
  for (const lang of ['en', 'zh']) {
    assert.match(M.storageLocales[lang].waitingOnMachine, /\{machine\}/)
    assert.match(M.storageLocales[lang].waitingOnMachineTip, /\{machine\}[^]*\{store\}/)
    assert.match(M.storageLocales[lang].startMachineNamed, /\{machine\}/)
  }
})

// ---------------------------------------------------------------- follow-up: every power press guarded

test('every machine start/stop/restart press sits behind the press guard (no exception)', () => {
  const sites = [
    ['src/client/storage/StorePage.tsx', /machineState === 'stopped' \? <MachineStartAction machine=\{s\.machine \?\? ''\} t=\{t\} onStart=\{on\.start\} \/>/],
    ['src/client/storage/StoreCard.tsx', /onClick=\{guard\(\(e\) => \{ e\.stopPropagation\(\); onStart\(\) \}\)\}/],
    ['src/client/projects/ProjectList.tsx', /disabled=\{actions\.busy \|\| startGuard\.settling\} aria-disabled=\{startGuard\.settling \|\| undefined\}[^\n]*onClick=\{startGuard\(\(\) => void actions\.start\(availability\.machine!\)\)\}/],
    ['src/client/projects/setup/ProjectDialog.tsx', /disabled=\{busy \|\| startGuard\.settling\} aria-disabled=\{startGuard\.settling \|\| undefined\}[^\n]*\n\s*onClick=\{startGuard\(\(\) => void go\(\(\) => api\.call\('POST', `\/api\/machines\//],
    ['src/client/projects/setup/MachineTiles.tsx', /disabled=\{busy !== null \|\| guard\.settling\} aria-disabled=\{guard\.settling \|\| undefined\}[^]*?onClick=\{guard\(\(\) => press\(m\)\)\}/],
    ['src/client/storage/dialogs.tsx', /disabled=\{starting \|\| startGuard\.settling\} aria-disabled=\{startGuard\.settling \|\| undefined\}[^]*?onClick=\{startGuard\(\(\) => \{ setStarting\(true\); api\.call\('POST', `\/api\/machines\//],
    ['src/client/machines/MachineImages.tsx', /disabled=\{restarting\.pending !== null \|\| restartGuard\.settling\} aria-disabled=\{restartGuard\.settling \|\| undefined\} onClick=\{restartGuard\(\(\) => void restarting\.restart\(\)\)\}/],
    ['src/client/machines/ImageMenu.tsx', /if \(id === RESTART\) \{ restartGuard\(\(\) => void restarting\.restart\(\)\)/],
    ['src/client/machines/parts.tsx', /disabled=\{pending !== null \|\| guard\.settling\} aria-disabled=\{guard\.settling \|\| undefined\}\n\s*onClick=\{guard\(\(\) => void run\('stop', \{ kind: 'stop', name: m\.name, force: true \}\)/],
    ['src/client/presets/PresetsSection.tsx', /disabled=\{waiting \|\| data\.busy \|\| confirmGuard\.settling\} aria-disabled=\{confirmGuard\.settling \|\| undefined\} aria-busy=\{waiting\} onClick=\{confirmGuard\(\(\) => \{ void execute\(\) \}\)\}/],
  ]
  for (const [file, re] of sites) assert.match(src(file), re, file)
  // the machine menu's power items: every onSelect that runs start/stop/restart goes through guarded()
  const parts = src('src/client/machines/parts.tsx')
  const menu = parts.slice(parts.indexOf('export function MachineMenu'), parts.indexOf('{forceStopOf('))
  assert.equal((menu.match(/onSelect=\{\(\) => \{ close\(\); void run\(/g) ?? []).length, 0, 'no unguarded power item')
  assert.equal((menu.match(/onSelect=\{guarded\(\(\) => \{ close\(\); void run\(/g) ?? []).length, 3)
  assert.equal((menu.match(/guard\.settling\}/g) ?? []).length, 3)
  // the earlier sites keep theirs: power button, fix button, Home need actions, Restart worker card, FixAction
  assert.match(parts, /onClick=\{guard\(\(e\) => \{ e\.stopPropagation\(\); void run\(power,/)
  assert.match(src('src/client/storage/HomeOverview.tsx'), /onClick=\{guard\(\(\) => \{ if \(n\.secondary!\.confirm && !armed\) setArmed\(true\); else go\(n\.secondary!\.run\) \}\)\}/)
  assert.match(src('src/client/storage/HomeCards.tsx'), /onClick=\{guard\(\(\) => \{ setBusy\(true\)/)
})
