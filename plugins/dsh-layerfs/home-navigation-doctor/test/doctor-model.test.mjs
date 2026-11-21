// The Doctor page's model (`src/client/doctor/model.ts`) and the shared wire
// helpers (`src/shared/doctor.ts`), driven by the stand-in world's answers:
// rows from the tree, reports and progress landing on rows, root causes and
// waiting rows, which buttons can be pressed, Check all's plan, fix parsing.
//   node --test --test-timeout=60000 test/doctor-model.test.mjs
// With LAYERFS_CALLS_JSON=<calls.json of the call surface>, the copied spec
// examples are also compared with that file.
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtempSync, readFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'
import { CHECK_TABLE, EXAMPLES, HOME, SCENARIOS, checksFor, doctorAnswer, fixAnswer, freshState, thingKeys, treeAnswer } from './fixtures/doctor-scenarios.mjs'

const root = fileURLToPath(new URL('..', import.meta.url))
const out = mkdtempSync(join(tmpdir(), 'dsh-doctor-model-'))
await build({
  absWorkingDir: root,
  stdin: { contents: "export * from './src/client/doctor/model.ts'\nexport * as wire from './src/shared/doctor.ts'\nexport * as table from './src/client/doctor/checks.ts'\nexport * as locale from './src/client/doctor/locale.ts'\n", resolveDir: root, loader: 'ts' },
  outfile: join(out, 'model.mjs'), bundle: true, format: 'esm', platform: 'node', target: 'node24', logLevel: 'warning',
})
const M = await import(pathToFileURL(join(out, 'model.mjs')).href)
test.after(() => rmSync(out, { recursive: true, force: true }))

const NOW = Date.parse('2026-11-18T10:00:00Z')
let seq = 0
/** The page's state over a scenario, and the stand-in answering as the host would relay it. */
function page(scenario) {
  const world = freshState(scenario)
  const tree = () => { const a = treeAnswer(world); return { at: NOW, home: { home: a.home, state: a.state, version: a.version, generation: a.generation, reason: a.reason, fix: a.fix }, tree: a.tree } }
  const p = { world, state: M.setTree(M.emptyState(), tree()) }
  p.thing = (key) => p.state.world.by[key] ?? assert.fail(`no row ${key}`)
  p.seen = (key) => M.stateOf(p.state, p.thing(key))
  p.view = (key, open = false) => M.viewOf(p.state, p.thing(key), 0, open)
  p.reread = () => { p.state = M.setTree(p.state, tree()) }
  /** One doctor call from start to end: its events, as `doctor-run` delivers them. Returns the events. */
  p.events = (kind, name, { full = false, group = null } = {}) => {
    const id = `run-${++seq}`
    const base = { id, kind, name: name ?? null, full, group, started: NOW, ended: null }
    const a = doctorAnswer(world, kind, name, { full })
    const list = [{ ...base, state: 'queued' }]
    if (a.refuse) list.push({ ...base, state: 'failed', ended: NOW, failure: { reason: a.refuse.code === 'Busy' ? 'busy' : 'lost', code: a.refuse.code } })
    else if (a.lost) list.push({ ...base, state: 'failed', ended: NOW, failure: { reason: 'lost', code: null } })
    else if (a.hang) list.push({ ...base, state: 'failed', ended: NOW, failure: { reason: 'timeout', code: 'Unknown' } })
    else list.push(...a.events.map((progress) => ({ ...base, state: 'running', progress })), { ...base, state: 'done', ended: NOW, report: a.report })
    return list
  }
  p.run = (kind, name, more) => { const list = p.events(kind, name, more); for (const e of list) p.state = M.onRun(p.state, e, NOW); return list.at(-1) }
  p.checkAll = () => {
    const calls = M.planCalls(M.allPlan(p.state))
    const lists = calls.map((c) => p.events(c.kind, c.name, { group: 'all-00000001' }))
    p.state = M.addRuns(p.state, lists.map((l) => l[0]), NOW)
    for (const l of lists) for (const e of l) p.state = M.onRun(p.state, e, NOW)
    return calls
  }
  return p
}
const st = (p, key) => { const o = p.seen(key); return o.why ? `${o.st}:${o.why}` : o.st }

test('the rows are the tree: every thing once, under its parent, with the facts its node has', () => {
  const p = page('healthy')
  const w = p.state.world
  assert.deepEqual(w.things.map((x) => x.key).sort(), thingKeys(p.world).sort())
  assert.equal(w.homeKey, `home:${HOME}`)
  const dev = p.thing('machine:dev')
  assert.deepEqual([dev.platform, dev.cpus, dev.memoryBytes, dev.parent], ['lima', 4, 8 * 1024 ** 3, w.homeKey])
  assert.deepEqual([p.thing('machine:native').cpus, p.thing('machine:native').memoryBytes], [null, null])
  // a Store's projects are bare names in the tree; a project's handle is store/project
  const api = p.thing('project:shop/api')
  assert.deepEqual([api.name, api.label, api.store, api.parent, api.machine], ['shop/api', 'api', 'shop', 'store:shop', 'machine:dev'])
  const pg = p.thing('store:analytics')
  assert.deepEqual([pg.engine, pg.parent, pg.machine, pg.connections], ['postgresql', w.homeKey, null, 2])
  assert.equal(p.thing('store:shop').connections, null)
  const ws = p.thing('workspace:tests:agent')
  assert.deepEqual([ws.label, ws.branch, ws.sandbox, ws.parent, ws.machine], ['agent', 'agent', 'tests', 'sandbox:tests', 'machine:native'])
  const box = p.thing('sandbox:sdk-1')
  assert.deepEqual([box.project, box.image, box.notListed], ['shop/api', 'node:22', false])
  // a Sandbox on a stopped machine: its Workspaces are not read
  assert.equal(page('mixed').thing('sandbox:build-1').notListed, false)
  assert.equal(p.thing('bucket:r2-main').provider, 'r2')
  assert.equal(M.tabOf(ws), 'sandboxes')
  assert.deepEqual(M.tabsOf(p.state).map((t) => [t.key, t.count, t.mark]), [['machines', 4, null], ['stores', 9, null], ['projects', 14, null], ['sandboxes', 7, null], ['backups', 3, null]])
})

test('opening the page: nothing is checked; a stopped machine and what is on it say so from records', () => {
  const p = page('stopped')
  const head = M.headOf(p.state)
  assert.deepEqual(head.headline, { kind: 'notChecked' })
  assert.equal(head.counts.nc, head.total)
  assert.equal(st(p, 'machine:dev'), 'nc:never')
  assert.equal(st(p, 'machine:docker'), 'nc:stopped-self')
  assert.equal(st(p, 'store:sim-website-0ce'), 'nc:stopped')
  assert.equal(st(p, 'project:sim-website-0ce/website-0ce'), 'nc:stopped')
  assert.equal(p.seen('project:sim-website-0ce/website-0ce').by.key, 'machine:docker')
  // Start is the stopped machine's only button; what is on it has none
  const docker = p.view('machine:docker')
  assert.deepEqual([docker.canCheck, docker.word, docker.fix.button, docker.fix.line], [false, 'stopped', 'start', 'layerfs machine start docker'])
  const on = p.view('store:sim-website-0ce')
  assert.deepEqual([on.blocked, on.canCheck, on.fix], [true, false, null])
  const proj = p.view('project:sim-website-0ce/website-0ce')
  assert.deepEqual([proj.blocked, proj.canFull], [true, false])
  assert.deepEqual(M.tabsOf(p.state)[0].mark, 'nc')
  assert.equal(M.tabsOf(p.state)[0].stopped, 1)
})

test('a stopped machine whose own checks pass reads OK with Stopped beside it, and keeps Start', () => {
  const p = page('stopped')
  p.run('home')
  const docker = p.view('machine:docker')
  assert.deepEqual([docker.seen.st, docker.seen.why, docker.seen.stopped, docker.word], ['ok', null, true, 'stopped'])
  assert.deepEqual([docker.canCheck, docker.fix.button, docker.fix.line, docker.ownTrouble], [false, 'start', 'layerfs machine start docker', true])
  // the checks that apply to a stopped machine: its worker and its disk are not asked
  assert.deepEqual(docker.marks.map((m) => [m.check, m.r]), checksFor(p.world, 'machine', 'docker').map((c) => [c, 'ok']))
  assert.ok(!docker.marks.some((m) => /worker|capacity/.test(m.check)))
  // what is on it stays Not checked: its machine is stopped
  assert.deepEqual([st(p, 'store:sim-website-0ce'), st(p, 'project:sim-website-0ce/website-0ce')], ['nc:stopped', 'nc:stopped'])
  const tab = M.tabsOf(p.state)[0]
  assert.deepEqual([tab.mark, tab.stopped, tab.bad], [null, 1, 0])
  assert.equal(M.headOf(p.state).counts.problem, 0)
  // started: the records say ready, and it is a machine like any other
  fixAnswer(p.world, 'machine', 'start', ['docker'])
  p.reread()
  p.state = M.forget(p.state, 'machine:docker')
  assert.deepEqual([st(p, 'machine:docker'), p.view('machine:docker').word, p.view('machine:docker').fix, p.view('machine:docker').canCheck], ['nc:never', null, null, true])
})

test('severity: error and warning read Problem, notice reads Notice', () => {
  const p = page('healthy')
  const report = (severity) => ({ ok: false, findings: [{ machine: 'dev', check: 'machine.capacity', severity, message: 'its data disk is nearly full', fix: null, from: null, evidence: null }], not_checked: [], status: severity, counts: {}, observed_at: new Date(NOW).toISOString(),
    tree: { machine: 'dev', status: severity, coverage: 'shallow', blocked_by: null, reason: null, checks: [{ check: 'machine.record', outcome: 'pass', from: null }, { check: 'machine.capacity', outcome: severity, from: null }] } })
  const read = (severity) => { p.state = M.onRun(p.state, { id: `sev-${severity}`, kind: 'machine', name: 'dev', full: false, group: null, state: 'done', started: NOW, ended: NOW, report: report(severity) }, NOW); return p.view('machine:dev') }
  assert.deepEqual([read('error').seen.st, read('warning').seen.st, read('notice').seen.st], ['problem', 'problem', 'notice'])
  assert.deepEqual(read('warning').marks.map((m) => m.r), ['ok', 'bad'])
  assert.deepEqual(read('notice').marks.map((m) => m.r), ['ok', 'warn'])
  assert.equal(read('warning').word, 'check:machine.capacity')
  // a warning on a machine is its own trouble: what is on it is still checked
  assert.equal(st(p, 'store:shop'), 'nc:never')
  assert.equal(p.view('store:shop').blocked, false)
})

test('a doctor lands one thing at a time, then its report', () => {
  const p = page('healthy')
  const list = p.events('machine', 'dev')
  p.state = M.addRuns(p.state, [list[0]], NOW)
  assert.deepEqual(['machine:dev', 'store:shop', 'sandbox:sdk-1', 'sandbox:funnels'].map((k) => st(p, k)), ['run', 'run', 'run', 'run'])
  assert.equal(st(p, 'project:shop/api'), 'nc:never', 'a machine doctor does not reach a project')
  assert.equal(M.offWhy(p.state, p.thing('store:shop'), 'check').why, 'busy')
  p.state = M.onRun(p.state, list[1], NOW)
  assert.deepEqual(['machine:dev', 'store:shop', 'sandbox:sdk-1'].map((k) => st(p, k)), ['ok', 'run', 'run'])
  p.state = M.onRun(p.state, list[2], NOW)
  assert.deepEqual(['store:shop', 'sandbox:sdk-1'].map((k) => st(p, k)), ['ok', 'run'])
  for (const e of list.slice(3)) p.state = M.onRun(p.state, e, NOW)
  assert.deepEqual(['machine:dev', 'store:shop', 'sandbox:sdk-1', 'sandbox:exp-1', 'sandbox:funnels'].map((k) => st(p, k)), ['ok', 'ok', 'ok', 'ok', 'ok'])
  assert.deepEqual(p.state.runs, {})
  // marks are the node's checks, in its order
  assert.deepEqual(p.view('machine:dev').marks.map((m) => [m.check, m.r]), checksFor(p.world, 'machine', 'dev').map((c) => [c, 'ok']))
  assert.deepEqual(p.view('store:shop').marks.map((m) => m.check), checksFor(p.world, 'store', 'shop'))
  assert.equal(M.offWhy(p.state, p.thing('store:shop'), 'check'), null)
})

test('a run this page did not start is followed from its events', () => {
  const p = page('healthy')
  const list = p.events('store', 'shop')
  p.state = M.onRun(p.state, list[1], NOW)   // the queued event was missed
  assert.equal(st(p, 'store:shop'), 'ok')
  assert.equal(st(p, 'project:shop/web'), 'run')
  for (const e of list.slice(2)) p.state = M.onRun(p.state, e, NOW)
  assert.equal(st(p, 'project:shop/web'), 'ok')
  assert.deepEqual(p.view('project:shop/web').marks.map((m) => m.check), ['project.record', 'project.listed'])
})

test('an outdated worker is the root cause: what is on its machine waits on it, with a link and no buttons', () => {
  const p = page('outdated2')
  p.run('machine', 'demo-build')
  const m = p.view('machine:demo-build')
  assert.equal(m.seen.st, 'problem')
  assert.equal(m.word, 'check:machine.worker.compatibility')
  assert.equal(m.seen.message, 'worker is from another build; 2 Sandboxes are on it')
  for (const key of ['store:demo-build-cache', 'project:demo-build-cache/sdk', 'sandbox:build-1', 'sandbox:build-2']) {
    const v = p.view(key)
    assert.deepEqual([v.seen.st, v.seen.why, v.seen.by.key, v.blocked, v.fix, v.marks], ['nc', 'waiting', 'machine:demo-build', true, null, null], key)
  }
  // Sandboxes are on it: the fix stops by force, which the page never runs. The button is shown, off, with the count; the line is there to copy.
  assert.deepEqual([m.fix.button, m.fix.fix, m.fix.line], ['restart', null, 'layerfs machine stop demo-build --force && layerfs machine start demo-build'])
  assert.deepEqual(m.fix.commands, ['layerfs machine stop demo-build --force', 'layerfs machine start demo-build'])
  assert.deepEqual(M.offWhy(p.state, m.thing, 'fix', m.fix), { why: 'sandboxes', n: 2 })
  // Check all leaves out what is under the failed machine
  const plan = M.allPlan(p.state)
  assert.ok(!plan.stores.some((x) => x.name === 'demo-build-cache') && !plan.sandboxes.some((x) => x.machine === 'machine:demo-build'))
  assert.equal(M.tabsOf(p.state)[0].bad, 1)
  assert.equal(M.tabsOf(p.state)[1].bad, 0, 'a waiting row is no trouble of its tab')
  assert.deepEqual(M.headOf(p.state).headline, { kind: 'problems', n: 1 })
})

test('a machine fixed outside the page: an older "waiting on it" no longer holds, and Check all asks what it had left out', () => {
  const p = page('outdated2')
  const waiting = ['store:demo-build-cache', 'sandbox:build-1', 'sandbox:build-2']
  p.checkAll()
  assert.equal(st(p, 'machine:demo-build'), 'problem')
  for (const key of waiting) assert.equal(st(p, key), 'nc:waiting', key)
  // the worker is restarted in a terminal (the --force line the page gave to copy); the page has not heard
  p.world.outdated = []
  // the stand-in stamps its answers with the clock: later than any of them
  const LATER = Date.now() + 60_000
  const land = (events) => { for (const e of events) p.state = M.onRun(p.state, e.report ? { ...e, ended: LATER, report: { ...e.report, observed_at: new Date(LATER).toISOString() } } : e, LATER) }

  // Check all: what is under the machine the page thinks failed is left out of the first calls
  const group = 'all-00000002'
  const asked = M.planCalls(M.allPlan(p.state))
  assert.ok(!asked.some((c) => c.name === 'demo-build-cache' || c.name === 'build-1' || c.name === 'build-2'))
  p.state = M.openAll(p.state, group)
  const lists = asked.map((c) => p.events(c.kind, c.name, { group }))
  p.state = M.addRuns(p.state, lists.map((l) => l[0]), LATER)
  for (const l of lists) land(l)
  // its calls are over, the Check all is not: the page may still add to it
  assert.equal(p.state.all?.id, group)
  assert.equal(M.offWhy(p.state, p.thing('machine:dev'), 'check').why, 'all')
  // the home doctor found the machine working: nothing waits on it any more
  assert.equal(st(p, 'machine:demo-build'), 'ok')
  for (const key of waiting) assert.deepEqual([st(p, key), p.seen(key).by, p.view(key).blocked], ['nc:never', null, false], key)
  // what was left out because of it is asked now, once
  const more = M.allMore(p.state, asked)
  assert.deepEqual(more.map((c) => `${c.kind}:${c.name}`).sort(), [...waiting].sort())
  const lists2 = more.map((c) => p.events(c.kind, c.name, { group }))
  p.state = M.addRuns(p.state, lists2.map((l) => l[0]), LATER)
  for (const l of lists2) land(l)
  assert.deepEqual(M.allMore(p.state, [...asked, ...more]), [])
  p.state = M.closeAll(p.state, group)
  assert.equal(p.state.all, null)
  for (const key of [...waiting, 'project:demo-build-cache/sdk']) assert.equal(st(p, key), 'ok', key)
  assert.equal(M.headOf(p.state).headline.kind, 'allClear')

  // an answer that names a blocker the page has not looked at since still reads waiting
  const q = page('outdated2')
  q.run('store', 'demo-build-cache')
  assert.deepEqual([st(q, 'store:demo-build-cache'), q.seen('store:demo-build-cache').by.key], ['nc:waiting', 'machine:demo-build'])
})

test('an outdated worker with no Sandbox on its machine can be restarted from the row', () => {
  const p = page('outdated0')
  p.run('machine', 'demo-build')
  const m = p.view('machine:demo-build')
  assert.equal(m.fix.button, 'restart')
  assert.deepEqual(m.fix.fix.steps.map((s) => [s.family, s.call, s.args]), [['machine', 'stop', ['demo-build']], ['machine', 'start', ['demo-build']]])
  assert.equal(M.offWhy(p.state, m.thing, 'fix', m.fix), null)
  // the fix runs: Check is off on the machine and on what is on it, and Check all waits
  p.state = M.fixStep(p.state, 'machine:demo-build', 'fix-1', 'stop')
  assert.equal(M.offWhy(p.state, p.thing('machine:demo-build'), 'check').why, 'fix')
  assert.equal(M.offWhy(p.state, p.thing('store:demo-build-cache'), 'check').why, 'fix')
  assert.deepEqual(M.allOffWhy(p.state).why, 'fix')
  p.state = M.onFix(p.state, { id: 'fix-1', kind: 'machine', name: 'demo-build', fix: 'restart', step: 1, steps: ['stop', 'start'], state: 'running' })
  assert.equal(p.state.fixes['machine:demo-build'].step, 'start')
  // done: the stand-in is fixed, the old verdict is forgotten, then the machine's doctor and the doctor of each thing on it
  fixAnswer(p.world, 'machine', 'stop', ['demo-build']); fixAnswer(p.world, 'machine', 'start', ['demo-build'])
  p.reread()
  p.state = M.forget(p.state, 'machine:demo-build')
  assert.deepEqual(M.afterFix(p.state.world, 'machine:demo-build', null).map((wave) => wave.map((x) => x.key)), [['machine:demo-build'], ['store:demo-build-cache']])
  p.run('machine', 'demo-build'); p.run('store', 'demo-build-cache')
  p.state = M.fixEnd(p.state, 'machine:demo-build')
  assert.deepEqual(['machine:demo-build', 'store:demo-build-cache', 'project:demo-build-cache/sdk'].map((k) => st(p, k)), ['ok', 'ok', 'ok'])
  assert.equal(M.allOffWhy(p.state), null)
})

test('a refused fix leaves the row as it was, with the refusal beside the button', () => {
  const p = page('refused')
  p.state = M.fixStep(p.state, 'machine:docker', 'fix-9', 'start')
  p.state = M.fixEnd(p.state, 'machine:docker', { code: 'Unavailable', message: 'Docker Engine is not running' })
  const v = p.view('machine:docker')
  assert.deepEqual([v.seen.why, v.fix.button, v.ownTrouble], ['stopped-self', 'start', true])
  assert.deepEqual(p.state.refusals['machine:docker'], { code: 'Unavailable', message: 'Docker Engine is not running' })
  assert.equal(M.offWhy(p.state, v.thing, 'fix', v.fix), null, 'the button is back')
  p.state = M.fixStep(p.state, 'machine:docker', 'fix-10', 'start')
  assert.equal(p.state.refusals['machine:docker'], undefined, 'pressing again clears the refusal')
})

test('could not check: the call failed (its children say whose), or LayerFS itself gave no verdict', () => {
  const p = page('timeout')
  // refused Busy: the Sandbox could not be checked; nothing under it was attempted
  p.run('sandbox', 'sdk-1')
  const box = p.seen('sandbox:sdk-1')
  assert.deepEqual([box.st, box.why, box.reason, box.code], ['nc', 'could', 'busy', 'Busy'])
  assert.equal(p.view('sandbox:sdk-1').word, 'could')
  // the reply was lost
  p.run('bucket', 'minio-local')
  assert.deepEqual([p.seen('bucket:minio-local').why, p.seen('bucket:minio-local').reason], ['could', 'lost'])
  // LayerFS's own budget: the report's node carries the reason and every check not checked
  p.run('store', 'qv2-store')
  const store = p.view('store:qv2-store')
  assert.deepEqual([store.seen.why, store.seen.reason, store.canCheck, store.ownTrouble], ['could', 'timeout', true, true])
  assert.ok(store.marks.length === checksFor(p.world, 'store', 'qv2-store').length && store.marks.every((m) => m.r === 'nc' && m.reason === 'timeout'))
  assert.equal(st(p, 'project:qv2-store/qv2-probe'), 'nc:never', 'its projects were not attempted')
  assert.deepEqual(M.tabsOf(p.state).map((t) => t.could), [0, 1, 0, 1, 1])
  assert.deepEqual(M.headOf(p.state).headline, { kind: 'notChecked' })

  // a call that failed on a thing with rows under it: they wait on that call's thing
  const q = page('healthy')
  q.world.fail['machine:dev'] = 'hang'
  q.run('machine', 'dev')
  assert.deepEqual([q.seen('machine:dev').why, q.seen('machine:dev').reason], ['could', 'timeout'])
  assert.deepEqual([q.seen('store:shop').why, q.seen('store:shop').by.key], ['parent', 'machine:dev'])
  assert.equal(q.view('store:shop').canCheck, true, 'its own Check can still be pressed')
  // the next press works
  q.run('machine', 'dev')
  assert.deepEqual(['machine:dev', 'store:shop'].map((k) => st(q, k)), ['ok', 'ok'])
})

test('Check all: the home doctor and one doctor per Store and Sandbox it can reach; nothing else can start meanwhile', () => {
  const p = page('mixed')
  const plan = M.allPlan(p.state)
  assert.equal(plan.home.kind, 'home')
  assert.deepEqual(plan.stores.map((x) => x.name).sort(), ['2026-08-16', 'analytics', 'demo-build-cache', 'demo-experiments', 'demo-studio', 'qv2-store', 'shop', 'sim-website-c69'])
  assert.equal(plan.sandboxes.length, 7)
  const calls = M.planCalls(plan)
  assert.deepEqual(calls[0], { kind: 'home' })
  assert.ok(calls.slice(1).every((c) => (c.kind === 'store' || c.kind === 'sandbox') && c.name && !c.full))

  const lists = calls.map((c) => p.events(c.kind, c.name, { group: 'all-00000001' }))
  p.state = M.addRuns(p.state, lists.map((l) => l[0]), NOW)
  assert.equal(M.isAllGroup('all-00000001'), true)
  assert.equal(M.offWhy(p.state, p.thing('machine:dev'), 'check').why, 'all')
  assert.equal(M.offWhy(p.state, p.thing('project:shop/api'), 'full').why, 'all')
  assert.equal(M.headOf(p.state).headline.kind, 'checking')
  assert.equal(M.headOf(p.state).headline.done, 0)
  const total = M.headOf(p.state).headline.total
  for (const l of lists) for (const e of l) p.state = M.onRun(p.state, e, NOW)
  assert.equal(p.state.all, null)
  assert.deepEqual(p.state.runs, {})
  assert.ok(total > 30)

  // what the mixed home reads afterwards
  assert.equal(st(p, 'machine:demo-build'), 'problem')
  // the checks after the failed one were not made
  assert.deepEqual(p.view('machine:demo-build').marks.filter((m) => m.r !== 'ok').map((m) => [m.check, m.r, m.reason]), [['machine.worker.compatibility', 'bad', null], ['machine.worker.ping', 'nc', 'outdated'], ['machine.capacity', 'nc', 'outdated']])
  assert.equal(st(p, 'store:demo-build-cache'), 'nc:waiting')
  assert.equal(st(p, 'sandbox:build-1'), 'nc:waiting')
  assert.equal(st(p, 'sandbox:exp-1'), 'problem')
  assert.equal(p.view('sandbox:exp-1').word, 'check:sandbox.daemon_connect')
  // its Workspace was named by the Sandbox's doctor and waits on the Sandbox
  assert.deepEqual([st(p, 'workspace:exp-1:agent'), p.seen('workspace:exp-1:agent').by.key], ['nc:waiting', 'sandbox:exp-1'])
  assert.equal(st(p, 'workspace:sdk-1:agent'), 'ok')
  // a fix off the allow list is text to copy, never a button
  assert.deepEqual([p.view('sandbox:exp-1').fix.button, p.view('sandbox:exp-1').fix.line], [null, 'layerfs sandbox remove exp-1'])
  // the two notices: a project its Store does not list, and a Store whose backup is behind (on the Store row, not the bucket's)
  const probe = p.view('project:qv2-store/qv2-probe')
  assert.deepEqual([probe.seen.st, probe.word, probe.fix.button], ['notice', 'check:project.listed', 'delete'])
  const shop = p.view('store:shop')
  assert.deepEqual([shop.seen.st, shop.word, shop.fix.button, shop.fix.line], ['notice', 'check:store.backups', 'backup', 'layerfs backup create shop r2-old'])
  assert.equal(st(p, 'bucket:r2-old'), 'ok')
  assert.deepEqual(p.view('bucket:r2-old').marks.map((m) => m.check), checksFor(p.world, 'bucket', 'r2-old'))
  // the stopped machine was looked at by the home doctor: OK, Stopped
  assert.deepEqual([st(p, 'machine:docker'), p.view('machine:docker').word], ['ok', 'stopped'])
  const head = M.headOf(p.state)
  assert.deepEqual(head.headline, { kind: 'problems', n: 2 })
  assert.equal(head.counts.notice, 2)
  assert.deepEqual(M.tabsOf(p.state).map((t) => t.mark), ['bad', 'warn', 'warn', 'bad', null])
  // a closed folder counts the trouble inside it
  assert.deepEqual(p.view('machine:dev').inside, { bad: 1, warn: 1 })
  assert.deepEqual(p.view('machine:dev', true).inside, { bad: 0, warn: 0 })

  // a row's check or a Full check holds Check all back, and says which
  const one = p.events('store', 'shop')
  p.state = M.addRuns(p.state, [one[0]], NOW)
  assert.deepEqual([M.allOffWhy(p.state).why, M.allOffWhy(p.state).thing.key], ['check', 'store:shop'])
  for (const e of one) p.state = M.onRun(p.state, e, NOW)
  const full = p.events('project', 'shop/api', { full: true })
  p.state = M.addRuns(p.state, [full[0]], NOW)
  assert.deepEqual([M.allOffWhy(p.state).why, M.allOffWhy(p.state).thing.key], ['full', 'project:shop/api'])
  assert.equal(M.running(p.state, 'full', 'project:shop/api').id, full[0].id)
})

test('the fixes after Check all on the mixed home', () => {
  const p = page('mixed')
  p.checkAll()
  // back up the Store: its notice goes
  fixAnswer(p.world, 'backup', 'create', ['shop', 'r2-old'])
  p.state = M.forget(p.state, 'store:shop')
  assert.deepEqual(M.afterFix(p.state.world, 'store:shop', null).map((w) => w.map((x) => x.key)), [['store:shop']])
  p.run('store', 'shop')
  assert.equal(st(p, 'store:shop'), 'ok')
  // delete the project the Store does not list: its row goes with the tree, and its Store is checked
  fixAnswer(p.world, 'project', 'delete', ['qv2-store/qv2-probe'])
  p.reread()
  assert.equal(p.state.world.by['project:qv2-store/qv2-probe'], undefined)
  assert.deepEqual(M.afterFix(p.state.world, 'project:qv2-store/qv2-probe', 'store:qv2-store').map((w) => w.map((x) => x.key)), [['store:qv2-store']])
  p.run('store', 'qv2-store')
  assert.equal(M.headOf(p.state).counts.notice, 0)
})

test('a Full check covers its thing alone; a stopped one says how long it ran', () => {
  const p = page('slowfull')
  p.run('project', 'shop/api')
  assert.deepEqual(M.covers(p.state.world, 'project:shop/api', true), ['project:shop/api'])
  const list = p.events('project', 'shop/api', { full: true })
  p.state = M.addRuns(p.state, [list[0]], NOW)
  assert.equal(st(p, 'project:shop/api'), 'run')
  assert.equal(M.offWhy(p.state, p.thing('project:shop/api'), 'check').why, 'busy')
  p.state = M.onRun(p.state, { ...list[0], state: 'stopped', ended: NOW + 83_000 }, NOW + 83_000)
  assert.equal(st(p, 'project:shop/api'), 'ok', 'the earlier check still stands')
  assert.equal(p.state.notes['project:shop/api'].stopped, 83_000)
  assert.equal(p.state.notes['project:shop/api'].fullAt, undefined)
  // run to its end: the data check is on the node, and the row remembers when its data was read
  p.run('project', 'shop/api', { full: true })
  assert.deepEqual(p.view('project:shop/api').marks.map((m) => m.check), ['project.record', 'project.listed', 'project.mount_holders', 'project.data'])
  assert.ok(p.state.notes['project:shop/api'].fullAt > 0)
  p.run('project', 'shop/api')
  assert.deepEqual(p.view('project:shop/api').marks.map((m) => m.check), ['project.record', 'project.listed'])
  assert.ok(p.state.notes['project:shop/api'].fullAt > 0, 'a later Check does not forget the Full check')
})

test('a page that opens later is seeded with what the host holds', () => {
  const a = page('mixed')
  const done = [a.run('machine', 'demo-build'), a.run('sandbox', 'exp-1'), a.run('store', 'shop')]
  const running = a.events('project', 'shop/api', { full: true })[0]
  const p = page('mixed')
  p.state = M.seed(p.state, { last: done, running: [{ ...running, state: 'running' }], fixes: [{ id: 'fix-7', kind: 'machine', name: 'docker', fix: 'start', step: 0, steps: ['start'], state: 'running' }] }, NOW)
  assert.deepEqual(['machine:demo-build', 'sandbox:exp-1', 'store:shop', 'project:shop/api'].map((k) => st(p, k)), ['problem', 'problem', 'notice', 'run'])
  assert.deepEqual(p.state.fixes['machine:docker'], { id: 'fix-7', step: 'start' })
  // a call this page followed that the host no longer runs is dropped on the next seed
  p.state = M.seed(p.state, { last: done, running: [], fixes: [] }, NOW)
  assert.deepEqual(p.state.runs, {})
  assert.equal(st(p, 'project:shop/api'), 'ok')
})

test('a Store some of whose probes could not be made reads Not checked with the checks that were', () => {
  const p = page('healthy')
  const node = { store: 'analytics', status: 'not_checked', coverage: 'shallow', blocked_by: null, reason: null,
    checks: [{ check: 'store.record', outcome: 'pass', from: null }, { check: 'store.connection', outcome: 'pass', from: 'native' }, { check: 'store.connection', outcome: 'not_checked', from: 'dev' }] }
  const report = { ok: false, findings: [], not_checked: [{ store: 'analytics', check: 'store.connection', blocked_by: { machine: 'dev', reason: 'stopped' }, reason: null }], status: 'not_checked', counts: {}, observed_at: new Date(NOW).toISOString(), tree: { ...node, projects: [] } }
  p.state = M.onRun(p.state, { id: 'run-pg', kind: 'store', name: 'analytics', full: false, group: null, state: 'done', started: NOW, ended: NOW, report }, NOW)
  const v = p.view('store:analytics')
  assert.deepEqual([v.seen.st, v.seen.why, v.blocked, v.canCheck], ['nc', 'partial', false, true])
  assert.deepEqual(v.marks.map((m) => [m.check, m.from, m.r, m.reason]), [['store.record', null, 'ok', null], ['store.connection', 'native', 'ok', null], ['store.connection', 'dev', 'nc', 'stopped']])
})

test('a refusal\'s fix line the page runs: exactly the listed lines, any other option or command stays a tip', () => {
  const { runnableFix } = M.wire
  assert.deepEqual(runnableFix('layerfs machine start dev'), { kind: 'start', machine: 'dev' })
  assert.deepEqual(runnableFix('layerfs machine stop dev'), { kind: 'stop', machine: 'dev' })
  assert.deepEqual(runnableFix('layerfs machine stop dev && layerfs machine start dev'), { kind: 'restart', machine: 'dev' })
  assert.deepEqual(runnableFix('layerfs machine prune dev --unused'), { kind: 'prune', machine: 'dev' })
  assert.deepEqual(runnableFix('layerfs store compact shop'), { kind: 'compact', store: 'shop' })
  assert.deepEqual(runnableFix('layerfs project delete shop/web'), { kind: 'delete', project: 'shop/web' })
  assert.deepEqual(runnableFix('layerfs backup create shop r2'), { kind: 'backup', store: 'shop', bucket: 'r2' })
  assert.deepEqual(runnableFix('layerfs  home   init'), { kind: 'homeInit' })
  for (const line of [
    'layerfs machine stop dev --force', 'layerfs machine prune dev', 'layerfs machine prune dev --unused --all', 'layerfs machine prune dev sha256:aa',
    'layerfs store compact shop --full', 'layerfs workspace mount s1 --branch b', 'layerfs sandbox create p s2', 'layerfs home init --force',
    'layerfs store compact shop && rm -rf ~', 'layerfs machine stop dev --force && layerfs machine start dev', 'sudo layerfs home init', '', null,
  ]) assert.equal(runnableFix(line), null, String(line))
})

test('a fix line is run only when the whole line is on the allow list', () => {
  const { parseFix, forcedRestart, fixCommands } = M.wire
  assert.deepEqual(parseFix('layerfs machine stop dev && layerfs machine start dev').kind, 'restart')
  assert.deepEqual(parseFix('layerfs machine start docker'), { kind: 'start', steps: [{ family: 'machine', call: 'start', args: ['docker'] }] })
  assert.deepEqual(parseFix('layerfs project delete qv2-store/qv2-probe').kind, 'delete')
  assert.deepEqual(parseFix('layerfs backup create shop r2-old').steps[0].args, ['shop', 'r2-old'])
  for (const line of [
    'layerfs machine stop dev --force && layerfs machine start dev',
    'layerfs machine stop dev && layerfs machine start other',
    'layerfs machine stop dev',
    'layerfs machine start dev && layerfs machine stop dev',
    'layerfs machine start dev && rm -rf ~',
    'layerfs machine start dev; rm -rf ~',
    'layerfs machine start $(whoami)',
    'layerfs machine start dev --force',
    'layerfs machine remove dev',
    'layerfs sandbox remove exp-1',
    'layerfs project delete justaname',
    'layerfs backup create shop',
    'layerfs store delete shop',
    'open -a Docker',
    'sudo layerfs machine start dev',
    '', null, undefined, `layerfs machine start ${'x'.repeat(600)}`,
  ]) assert.equal(parseFix(line), null, String(line))
  assert.equal(forcedRestart('layerfs machine stop dev --force && layerfs machine start dev'), 'dev')
  assert.equal(forcedRestart('layerfs machine stop dev --force && layerfs machine start other'), null)
  assert.equal(forcedRestart('layerfs machine stop dev && layerfs machine start dev'), null)
  assert.deepEqual(fixCommands('layerfs machine stop dev --force && layerfs machine start dev'), ['layerfs machine stop dev --force', 'layerfs machine start dev'])
})

test('the doctor calls as the CLI takes them and as a person types them', () => {
  const { doctorArgv, doctorCommand } = M.wire
  assert.deepEqual(doctorArgv({ kind: 'home' }), ['home', 'doctor', '--detail', '--progress'])
  assert.deepEqual(doctorArgv({ kind: 'machine', name: 'dev' }), ['machine', 'doctor', '--detail', '--progress', '--', 'dev'])
  assert.deepEqual(doctorArgv({ kind: 'project', name: 'shop/api', full: true }), ['project', 'doctor', '--detail', '--progress', '--full', '--', 'shop/api'])
  assert.deepEqual(doctorArgv({ kind: 'bucket', name: 'r2-main', full: true }), ['backup', 'doctor', '--detail', '--full', '--', 'r2-main'])
  for (const call of [{ kind: 'workspace', name: 'tests:agent' }, { kind: 'machine' }, { kind: 'home', name: 'x' }, { kind: 'store', name: 'shop', full: true }]) assert.equal(doctorArgv(call), null)
  assert.equal(doctorCommand({ kind: 'home' }), 'layerfs home doctor')
  assert.equal(doctorCommand({ kind: 'bucket', name: 'r2-main', full: true }), 'layerfs backup doctor r2-main --full')
  assert.equal(M.humanCheck('sandbox.daemon_ready'), 'Daemon ready')
  assert.equal(M.humanCheck('machine.worker.compatibility'), 'Worker compatibility')
})

test('the spec examples are reports the page reads, and every check id is one of the frozen ones', () => {
  const frozen = new Set(Object.values(CHECK_TABLE).flat().map((c) => c.check))
  const banned = /^(host\.|backup\.)|^store\.reachable$|^workspace\.record$/
  const ids = new Set()
  const walk = (node) => {
    for (const c of node.checks ?? []) ids.add(c.check)
    for (const list of Object.keys(M.wire.CHILD_LISTS)) for (const child of node[list] ?? []) walk(child)
  }
  for (const name of ['home.doctor', 'machine.doctor', 'store.doctor', 'project.doctor', 'sandbox.doctor', 'backup.doctor']) {
    const report = EXAMPLES.calls[name].long
    assert.ok(M.wire.isReport(report), `${name}: the long example is a report`)
    walk(report.tree)
    for (const f of [...report.findings, ...report.not_checked]) if (f.check) ids.add(f.check)
  }
  // every scenario's every doctor answers the same shape with the same ids
  for (const scenario of Object.keys(SCENARIOS)) {
    const world = freshState(scenario)
    for (const key of thingKeys(world)) {
      const [kind, ...rest] = key.split(':')
      if (kind === 'workspace') continue
      for (const full of kind === 'project' || kind === 'bucket' ? [false, true] : [false]) {
        const a = doctorAnswer(world, kind, kind === 'home' ? undefined : rest.join(':'), { full })
        if (!a.report) continue
        assert.ok(M.wire.isReport(a.report), `${scenario} ${key}`)
        walk(a.report.tree)
        for (const f of [...a.report.findings, ...a.report.not_checked]) {
          if (f.check) ids.add(f.check)
          if (f.reason != null) assert.ok(['timeout', 'busy', 'lost'].includes(f.reason), `${scenario} ${key}: ${f.reason}`)
        }
        assert.equal(a.events.length, a.events.at(-1).total)
      }
    }
  }
  for (const id of ids) { assert.ok(frozen.has(id), `${id} is in doctor.checks`); assert.doesNotMatch(id, banned) }
  assert.ok(ids.has('store.backups') && ids.has('project.data') && ids.has('store.connection'))
  for (const [kind, list] of Object.entries(CHECK_TABLE)) for (const c of list) assert.ok(c.check.startsWith(`${kind}.`), `${c.check} is a ${kind} check`)
})

test('the page\'s check table is the contract\'s doctor.checks', () => {
  assert.deepEqual(JSON.parse(JSON.stringify(M.table.CHECKS)), CHECK_TABLE)
  assert.equal(M.table.whatOf('machine.worker.compatibility'), 'the worker is from this build')
  assert.equal(M.table.whatOf('some.check.of.a.later.build'), '', 'a check the table does not know still draws, without a tip sentence')
  assert.deepEqual(M.table.quickChecks('project').map((c) => c.check), ['project.record', 'project.listed'])
})

test('the copied spec examples match calls.json when it is given', { skip: !process.env.LAYERFS_CALLS_JSON && 'set LAYERFS_CALLS_JSON to compare' }, () => {
  const spec = JSON.parse(readFileSync(process.env.LAYERFS_CALLS_JSON, 'utf8'))
  const rows = new Map(spec.families.flatMap((f) => f.calls).map((c) => [c.call, c]))
  for (const key of ['version', 'frozen', 'base']) assert.equal(EXAMPLES[key], spec[key], key)
  const tree = rows.get('home.get').with.tree.example
  assert.deepEqual(EXAMPLES.calls['home.get'].tree, typeof tree === 'string' ? JSON.parse(tree) : tree)
  assert.deepEqual(EXAMPLES.doctor, spec.doctor, 'the doctor section (with doctor.checks) changed in calls.json')
  for (const [name, kept] of Object.entries(EXAMPLES.calls)) {
    const row = rows.get(name) ?? assert.fail(`calls.json has no ${name}`)
    for (const key of ['cli', 'progress']) {
      if (key in kept) assert.equal(kept[key], row[key], `${name}: ${key}`)
    }
    const long = typeof row.examples?.long === 'string' ? JSON.parse(row.examples.long) : row.examples?.long
    assert.deepEqual(kept.long, long, `${name}: the long example changed in calls.json`)
  }
})

test('the page says everything in both languages: the same keys, the same placeholders', () => {
  const { en, zh } = M.locale.locales
  const holes = (text) => [...String(text).matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort().join(',')
  const keys = Object.keys(en).filter((k) => k.startsWith('dp') || k.startsWith('chk_'))
  assert.ok(keys.length > 100)
  for (const k of keys) {
    assert.equal(typeof zh[k], 'string', `zh has ${k}`)
    assert.ok(zh[k].trim(), `zh ${k} is not empty`)
    assert.equal(holes(zh[k]), holes(en[k]), `placeholders of ${k}`)
  }
  // the words the coordinator fixed
  assert.equal(en.dpOffSandboxes, '{n} Sandboxes are on it. Copy the fix to stop it anyway.')
  assert.equal(en.dpOffSandboxesOne, '1 Sandbox is on it. Copy the fix to stop it anyway.')
  assert.deepEqual([en.dpFactConnectionsOne, en.dpFactConnections], ['1 connection', '{n} connections'])
  assert.deepEqual([en.dpTipWriteTested, en.dpTipWriteNever, en.dpTipFullStopped], ['Write tested at {time}.', 'Write never tested: Full check only.', 'Full check stopped at {elapsed}.'])
})
