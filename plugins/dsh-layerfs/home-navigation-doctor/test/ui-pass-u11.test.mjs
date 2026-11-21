// Package U11: the fixes from review RV4 of U10. Components and hooks are
// driven through the small hook runtime in mini-react.mjs (rendered, pressed,
// rendered again), not matched as source text.
//   node --test test/ui-pass-u11.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync } from 'node:fs'
import { join, relative } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'
import { miniReact } from './mini-react.mjs'

const root = fileURLToPath(new URL('..', import.meta.url))
mkdirSync(join(root, 'lib'), { recursive: true })
const out = mkdtempSync(join(root, 'lib', '.test-ui-pass-u11-'))
await build({
  absWorkingDir: root,
  stdin: {
    contents: [
      "export { mount, elements, text } from 'react'",
      "export { SessionLine, archiveConversation } from './src/client/projects/ProjectList.tsx'",
      "export { archivable } from './src/client/projects/session-visibility.ts'",
      "export { locales as listLocales } from './src/client/projects/list-locale.ts'",
      "export { useListOnOpen } from './src/client/storage/BucketPage.tsx'",
      "export { StartFirst, machineNote } from './src/client/projects/setup/MachineTiles.tsx'",
      "export { locales as setupLocales, setupStateWord } from './src/client/projects/setup/locale.ts'",
      "export { locales as storageLocales } from './src/client/storage/locale.ts'",
      "export { locationWords } from './src/client/storage/parts.tsx'",
      "export { CopyId, copyText } from './src/client/ui/index.tsx'",
      "export { filesEn, filesZh } from './src/client/files/locale.ts'",
      "export { locales as viewsLocales } from './src/client/views/locale.ts'",
      "export { stateWords } from './src/client/views/model.ts'",
      "export { machinesStarted } from './src/client/presets/labels.ts'",
      "export { locales as presetsLocales } from './src/client/presets/locale.ts'",
    ].join('\n'),
    resolveDir: root, loader: 'tsx',
  },
  outfile: join(out, 'u11.mjs'), bundle: true, format: 'esm', platform: 'node', target: 'node24', logLevel: 'error',
  jsx: 'automatic', loader: { '.module.css': 'local-css', '.css': 'css' }, plugins: [miniReact],
})
const M = await import(pathToFileURL(join(out, 'u11.mjs')).href)
test.after(() => rmSync(out, { recursive: true, force: true }))

const src = (path) => readFileSync(join(root, path), 'utf8')
const fill = (text, vars = {}) => text.replace(/\{(\w+)\}/g, (_, k) => String(vars[k] ?? `{${k}}`))
const tOf = (dict, lang) => (key, vars) => fill(dict[lang][key] ?? key, vars)
const find = (tree, pred) => M.elements(tree).filter((e) => pred(e.props ?? {}, e))

// ---------------------------------------------------------------- FAIL 1

test('FAIL 1: the views\' session read (mount, events, the 10 s poll) asks for the read that changes nothing', () => {
  // the host side is test/sessions.test.mjs "an observed session.state …"; this is the one client caller
  const callers = []
  const walk = (dir) => { for (const f of readdirSync(dir)) { const p = join(dir, f); if (statSync(p).isDirectory()) walk(p); else if (/'session\.state'/.test(readFileSync(p, 'utf8'))) callers.push(relative(root, p)) } }
  walk(join(root, 'src/client'))
  assert.deepEqual(callers, ['src/client/views/hooks.ts'])
  assert.match(src('src/client/views/hooks.ts'), /api\.op<SessionStateView>\('session\.state', \{ sessionId: root, observe: true \}\)/)
  assert.match(src('src/host/sessions/index.ts'), /sessions\.view\(need\(args, 'sessionId'\), \{ observe: args\?\.observe === true \}\)/)
})

// ---------------------------------------------------------------- FAIL 2

const t = tOf(M.listLocales, 'en')
const line = (session, onArchive = async () => null) => M.mount(M.SessionLine, { session, title: 'Chat', selected: false, t, onOpen: () => {}, onArchive })
const archiveButtons = (tree) => find(tree, (p) => 'data-archive-session' in p)

test('FAIL 2: Archive shows only on conversations DSH holds that are not running; a Store-only row has none', () => {
  const stored = { sessionId: 'st', title: 'x', local: false, updatedAt: 1, state: 'stored' }
  assert.equal(archiveButtons(line(stored).tree).length, 0, 'found only in the Store: not DSH\'s to archive')
  assert.equal(archiveButtons(line({ ...stored, local: true }).tree).length, 0, 'a stored row is never archivable')
  for (const local of [
    { sessionId: 'a', title: 'x', local: true, updatedAt: 1, state: 'closed', executionState: 'inactive' },
    { sessionId: 'b', title: 'x', local: true, updatedAt: 1, state: 'local' },
    { sessionId: 'c', title: 'x', local: true, updatedAt: 1, state: 'failed', executionState: 'interrupted' },
  ]) assert.equal(archiveButtons(line(local).tree).length, 1, local.sessionId)
  for (const running of [
    { sessionId: 'd', title: 'x', local: true, updatedAt: 1, state: 'bound', executionState: 'active' },
    { sessionId: 'e', title: 'x', local: true, updatedAt: 1, state: 'closed', executionState: 'activating' },
  ]) assert.equal(archiveButtons(line(running).tree).length, 0, running.sessionId)
})

test('FAIL 2: a refused Archive reads "Not archived" with DSH\'s message as the tip and one action; a press again archives', async () => {
  const refusal = 'cannot archive session \'a\': no such session'
  const answers = [refusal, null]
  const asked = []
  const view = line({ sessionId: 'a', title: 'x', local: true, updatedAt: 1, state: 'closed' }, async () => { asked.push('a'); return answers.shift() })
  archiveButtons(view.tree)[0].props.onClick()
  const refused = await view.settle()
  const notice = find(refused, (p) => p['data-archive-refused'] === 'a')
  assert.equal(notice.length, 1, 'the refusal is on screen, not dropped')
  const word = find(notice[0], (p) => p.role === 'alert')[0]
  assert.equal(M.text(word), 'Not archived')
  assert.equal(word.props.title, refusal)
  const again = find(notice[0], (p) => typeof p.onClick === 'function')
  assert.equal(again.length, 1, 'one action')
  assert.equal(M.text(again[0]), 'Try again')
  again[0].props.onClick()
  const done = await view.settle()
  assert.equal(find(done, (p) => 'data-archive-refused' in p).length, 0, 'archived: the refusal is gone')
  assert.deepEqual(asked, ['a', 'a'])
  // the call: DSH's refusal comes back as its message, an archive as null
  assert.equal(await M.archiveConversation({ op: async () => { throw new Error(refusal) } }, 'a'), refusal)
  const sent = []
  assert.equal(await M.archiveConversation({ op: async (name, args) => { sent.push([name, args]); return { archived: true } } }, 'a'), null)
  assert.deepEqual(sent, [['sessions.archive', { sessionId: 'a' }]])
  for (const lang of ['en', 'zh']) for (const k of ['archiveRefused', 'archiveAgain']) assert.ok(M.listLocales[lang][k], `${lang}.${k}`)
})

// ---------------------------------------------------------------- F2 (RV4 NOTE 12)

test('F2: an open bucket page asks LayerFS to list once, never on later renders, never after a refusal', () => {
  const calls = []
  const Page = ({ name, b, listing }) => { M.useListOnOpen(name, b, listing, () => calls.push(name)); return null }
  const blank = { files: null, error: null }
  const page = M.mount(Page, { name: 'bk', b: blank, listing: false })
  assert.equal(calls.length, 1, 'the open lists once')
  page.render({ name: 'bk', b: blank, listing: true })
  page.render({ name: 'bk', b: { files: null, error: { code: 'refused', message: 'bad key' } }, listing: false })
  page.render({ name: 'bk', b: { files: null, error: { code: 'refused', message: 'bad key' } }, listing: false })
  assert.equal(calls.length, 1, 'renders with the answer, or its refusal, ask nothing')
  // opened again with the refusal remembered: no call
  M.mount(Page, { name: 'bk', b: { files: null, error: { code: 'not_listed', message: 'm' } }, listing: false })
  assert.equal(calls.length, 1)
  // before the bucket is known: nothing; once known, the one ask
  const late = M.mount(Page, { name: 'bk', b: undefined, listing: false })
  assert.equal(calls.length, 1)
  late.render({ name: 'bk', b: blank, listing: false })
  assert.equal(calls.length, 2)
  // a listed bucket opened again: nothing to ask
  M.mount(Page, { name: 'bk', b: { files: [], error: null }, listing: false })
  assert.equal(calls.length, 2)
})

// ---------------------------------------------------------------- NOTE 3 and 12: every machine start is a press that names the machine

/**
 * Every path that sends `machine start` (a client POST to /start or /restart,
 * a `kind: 'start'` dispatch, startMachine(…), the host's machine.start or a
 * `machine start` argv), by file, with why it is allowed. A new path fails here.
 */
const START_SITES = {
  'src/client/machines/controller.ts': 'the machine page\'s act(): its action type and the POSTs to that machine\'s own /start and /restart',
  'src/client/machines/model.ts': 'a fix\'s kind (a type and a classification): sends nothing',
  'src/client/machines/parts.tsx': 'the machine card\'s own Start / power button and its own fix',
  'src/client/machines/ImageMenu.tsx': 'that machine\'s own Restart worker (its images menu)',
  'src/client/projects/ProjectList.tsx': 'the project row\'s "Start <machine>"',
  'src/client/projects/setup/ProjectDialog.tsx': 'StartFirst (twice) and the project row\'s "Start <machine>"',
  'src/client/projects/setup/run.ts': 'startMachine itself (called only from StartFirst)',
  'src/client/storage/HomeOverview.tsx': 'a trouble row\'s "Restart <machine>\'s worker"',
  'src/client/storage/StoragePage.tsx': 'the Store page\'s "Start <machine>" on a stopped machine',
  'src/client/storage/dialogs.tsx': 'StartFirst and the place dialog\'s "Start <machine>"',
  'src/host/presets/build.js': 'the preset script listing (shown, not run)',
  'src/host/presets/environment-reset.js': 'a confirmed Reset: its confirm names the machines',
  'src/host/presets/runner.js': 'a confirmed Build: its confirm names the machines',
  'src/host/routes/doctor.ts': 'the Doctor fix of that machine\'s own row',
  'src/host/routes/machines.ts': 'the POST /api/machines/<m>/start route',
  'src/host/routes/projects.ts': 'a Sandbox create that asks for it (`start: true`; no page sends it)',
  'src/shared/doctor.ts': 'a fix\'s kind (classification): sends nothing',
}
const START = [/\/api\/machines\/\$\{[^}]*\}\/(start|restart)`/g, /kind: 'start'/g, /startMachine\(/g, /machine\.start\(/g, /\['machine', 'start'/g]

test('NOTE 3: every path that sends machine start is a known press site; a new one fails here', () => {
  const found = new Set()
  const walk = (dir) => {
    for (const f of readdirSync(dir)) {
      const p = join(dir, f)
      if (statSync(p).isDirectory()) { walk(p); continue }
      const text = readFileSync(p, 'utf8')
      if (START.some((re) => text.match(re))) found.add(relative(root, p))
    }
  }
  walk(join(root, 'src'))
  assert.deepEqual([...found].sort(), Object.keys(START_SITES).sort())
})

test('NOTE 3: each start a page press sends names its machine in the label or the confirm (en and zh)', () => {
  for (const lang of ['en', 'zh']) {
    assert.match(fill(M.listLocales[lang].startNamed, { machine: 'demo-build' }), /demo-build/, `${lang} project row`)
    assert.match(fill(M.setupLocales[lang].startNamed, { machine: 'demo-build' }), /demo-build/, `${lang} dialog row`)
    assert.match(fill(M.storageLocales[lang].ovRestartWorker, { machine: 'demo-build' }), /demo-build/, `${lang} Restart worker`)
    assert.match(fill(M.storageLocales[lang].ovStopUnreachable, { machine: 'demo-build' }), /demo-build/, `${lang} hero Stop`)
    assert.match(fill(M.presetsLocales[lang].confirmStarts, { machines: 'demo-build, demo-research' }), /demo-build, demo-research/, `${lang} preset confirm`)
  }
  // the visible text, not only the accessible name
  assert.match(src('src/client/projects/ProjectList.tsx'), /onClick=\{startGuard\(\(\) => void actions\.start\(availability\.machine!\)\)\}>\{t\('startNamed', \{ machine: availability\.machine \?\? '' \}\)\}/)
  assert.match(src('src/client/projects/setup/ProjectDialog.tsx'), /\{t\('startNamed', \{ machine: availability\.machine \?\? '' \}\)\}<\/Button>/)
  assert.match(src('src/client/storage/HomeCards.tsx'), /t\('ovRestartWorker', \{ machine: m\.name \}\)/)
})

test('NOTE 3: Build and Reset name every machine they may start before the press (studio: demo-build, demo-research, docker)', () => {
  const studio = { projectGroups: [{ machine: 'native' }, { machine: 'docker' }, { machine: 'demo-build' }, { machine: 'demo-research' }] }
  const plan = { scope: 'environment', startMachines: ['demo-build'] }
  assert.deepEqual(M.machinesStarted(studio, plan, true), ['demo-build', 'demo-research', 'docker'], 'never native')
  assert.deepEqual(M.machinesStarted(studio, plan, false), ['demo-build'], 'a Reset alone starts only what it needs to reach')
  assert.deepEqual(M.machinesStarted(studio, { scope: 'machines', stopMachines: ['demo-build'] }, false), [], 'Stop machines starts none')
  assert.match(src('src/client/presets/PresetsSection.tsx'), /starts=\{machinesStarted\(preset, dialog\.plan, dialog\.kind === 'build'\)\}/)
})

test('NOTE 14: the Home hero\'s worker-down item has Stop <machine> again (a press), beside Open', () => {
  const hero = src('src/client/storage/HomeOverview.tsx')
  assert.match(hero, /secondary: \{ label: t\('ovStopUnreachable', \{ machine: place\(m\.name\) \}\), title: t\('ovStopUnreachableTitle'[^\n]*run: \(\) => mc\.act\(\{ kind: 'stop', name: m\.name \}\) \}/)
  for (const lang of ['en', 'zh']) assert.ok(M.storageLocales[lang].ovStopUnreachableTitle.includes('{machine}'), lang)
})

// ---------------------------------------------------------------- NOTE 8, 9, 10: state words

test('NOTE 8: the session map\'s execution states read in the page\'s words, capitalised in en, never raw in zh', () => {
  const en = tOf(M.viewsLocales, 'en'), zh = tOf(M.viewsLocales, 'zh')
  assert.equal(M.stateWords('interrupted', en), 'Interrupted')
  assert.equal(M.stateWords('activating', en), 'Activating')
  assert.equal(M.stateWords('inactive', en), 'Inactive')
  assert.equal(M.stateWords('ready', en), 'Ready')
  assert.equal(M.stateWords('active', en), 'Active')
  assert.equal(M.stateWords('interrupted', zh), '已中断')
  assert.equal(M.stateWords('activating', zh), '正在激活')
  assert.equal(M.stateWords('inactive', zh), '未激活')
  assert.equal(M.stateWords('closed', zh), '未激活', 'a closed binding without an execution state')
  assert.equal(M.stateWords('failed', zh), '已中断')
})

test('NOTE 9: a machine tile and the Move picker give a machine\'s state in the locale\'s words', () => {
  const view = { computer: { os: 'mac' } }
  const zh = M.locationWords(tOf(M.storageLocales, 'zh'), view)
  assert.equal(M.machineNote({ name: 'docker', state: 'broken' }, zh), '损坏')
  assert.equal(M.machineNote({ name: 'docker', state: 'busy' }, zh), '忙碌')
  assert.equal(M.machineNote({ name: 'docker', state: 'broken' }, M.locationWords(tOf(M.storageLocales, 'en'), view)), 'Broken')
  const setupZh = tOf(M.setupLocales, 'zh')
  assert.equal(M.setupStateWord('broken', setupZh), '损坏', 'the Storage pages\' word')
  assert.equal(M.setupStateWord('blocked', setupZh), '受阻')
  assert.equal(M.setupStateWord('weird', setupZh), '未知', 'another word reads Unknown, never raw')
  assert.match(src('src/client/storage/dialogs.tsx'), /\$\{storeStateWords\(m\.state \?\? 'unknown', t\)\}/)
})

test('NOTE 10: a refused Start in StartFirst is a word on that machine\'s line with LayerFS\'s message as the tip; its Start is the action', async () => {
  const setup = tOf(M.setupLocales, 'en')
  const words = { stopped: (m) => setup('stoppedFirst', { machine: m }), start: (m) => setup('startNamed', { machine: m }), refused: (m) => setup('startRefused', { machine: m }) }
  const message = 'Machine docker could not start: Docker Engine is not running. Fix: open Docker Desktop.'
  let started = 0
  const view = M.mount(M.StartFirst, { names: ['docker', 'lima1'], words, start: async () => { throw new Error(message) }, onStarted: () => { started++ } })
  const starts = find(view.tree, (p) => typeof p.onClick === 'function')
  assert.deepEqual(starts.map((b) => b.props['aria-label']), ['Start docker', 'Start lima1'], 'one Start per machine, named')
  // each Start is behind the press guard (it settles from when it is drawn): press once it has settled
  await new Promise((resolve) => setTimeout(resolve, 650))
  starts[0].props.onClick({ stopPropagation() {} })
  const after = await view.settle()
  const refused = find(after, (p) => p['data-start-refused'] === 'docker')
  assert.equal(refused.length, 1)
  assert.equal(M.text(refused[0]), 'Not started · docker')
  assert.equal(refused[0].props.title, message)
  assert.equal(refused[0].props.role, 'alert')
  assert.ok(!M.elements(after).some((e) => typeof e.props?.children === 'string' && e.props.children.includes('Docker Engine')), 'the message is a tip, not body text')
  assert.ok(M.text(after).includes('Stopped · lima1'), 'the other machine keeps its word')
  assert.equal(started, 0)
  for (const lang of ['en', 'zh']) {
    assert.ok(M.setupLocales[lang].startRefused.includes('{machine}'), lang)
    assert.ok(M.storageLocales[lang].nsStartRefused.includes('{machine}'), lang)
  }
})

// ---------------------------------------------------------------- NOTE 11 and B2 #4: CopyId

test('NOTE 11: a refused clipboard write is "Could not copy" in the tip, with no unhandled rejection', async () => {
  assert.equal(await M.copyText('id', { writeText: async () => { throw new Error('denied') } }), 'failed')
  assert.equal(await M.copyText('id', undefined), 'failed', 'no clipboard')
  assert.equal(await M.copyText('id', { writeText: () => { throw new Error('sync') } }), 'failed')
  const written = []
  assert.equal(await M.copyText('id', { writeText: async (v) => { written.push(v) } }), 'copied')
  assert.deepEqual(written, ['id'])
  // the component, pressed in a page with no clipboard (node): the full id stays the tip, the failure joins it
  const id = `12${'a'.repeat(64)}`
  const chip = M.mount(M.CopyId, { id, short: id.slice(0, 14), copyLabel: 'Copy', copiedLabel: 'Copied', failedLabel: M.filesEn.files_copyFailed })
  assert.equal(chip.tree.props.title, `${id}\nCopy`)
  chip.tree.props.onClick({ stopPropagation() {}, currentTarget: { querySelector: () => null } })
  const failed = await chip.settle()
  assert.equal(failed.props.title, `${id}\n${M.filesEn.files_copyFailed}`)
  assert.match(failed.props['aria-label'], /Could not copy/)
  assert.equal(failed.props['data-copy'], 'failed')
  assert.ok(M.filesZh.files_copyFailed)
  // with a clipboard: the press copies the full id, not the short label (B2 #4)
  const copied = []
  Object.defineProperty(globalThis.navigator, 'clipboard', { value: { writeText: async (v) => { copied.push(v) } }, configurable: true })
  try {
    const ok = M.mount(M.CopyId, { id, short: id.slice(0, 14), copyLabel: 'Copy', copiedLabel: 'Copied', failedLabel: M.filesEn.files_copyFailed })
    ok.tree.props.onClick({ stopPropagation() {}, currentTarget: { querySelector: () => null } })
    const done = await ok.settle()
    assert.deepEqual(copied, [id])
    assert.equal(done.props.title, `${id}\nCopied`)
  } finally { delete globalThis.navigator.clipboard }
})
