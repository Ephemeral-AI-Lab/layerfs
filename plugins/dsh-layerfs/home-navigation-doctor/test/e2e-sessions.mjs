#!/usr/bin/env node
// End-to-end: LayerFS project sessions in the real DSH web UI with the
// scripted fake model (dev/fake-llm). Needs a dev server started with
// DSH_LAYERFS_DEV_OVERLAY=dev/fake-llm.overlay.yml.
//   node test/e2e-sessions.mjs [--keep]
// Checks: a session placed in a project binds in its own workspace (branch,
// sandbox, mount, scratch), per-turn checkpoints, restore files to a checkpoint,
// branch session at a checkpoint (conversation fork + files fork), the LayerFS
// session store and persistence row, plain folder sessions (Scratch workspace
// and DSH's default workspace run as stock DSH), the Workspaces header switch to
// LayerFS projects (project page, new session), the New Session project control,
// the conversation's Files/History/Sandbox tabs (no LayerFS side panel) and the
// Logs page (checkpoint receipts, the call detail with its stored output, Reset).
import { fileURLToPath } from 'node:url'
import assert from 'node:assert/strict'
import { existsSync, mkdtempSync, readFileSync, writeFileSync, mkdirSync } from 'node:fs'
import { join } from 'node:path'
import { openDev, runTurn } from '../tools/e2e-lib.mjs'
import { dismiss, openPanel } from '../tools/ui.mjs'

const step = (m) => console.log(`\n== ${m}`)
const { browser, page, logs } = await openDev()
await page.waitForFunction(() => window.__dshLayerfs, null, { timeout: 30000 })
const op = (name, args = {}) => page.evaluate(async ([n, a]) => {
  try { return { ok: true, value: await window.__dshLayerfs.op(n, a) } } catch (e) { return { ok: false, code: e.code, message: e.message } }
}, [name, args])
const must = async (name, args) => { const r = await op(name, args); if (!r.ok) throw new Error(`${name}: ${r.code} ${r.message}`); return r.value }
// A session in the currently selected workspace, placed in the project.
const openProject = (key) => page.evaluate(async (k) => window.__dshLayerfs.openProjectSession(k), key)
const http = async (method, path, body) => {
  const r = await must('http', { method, path, ...(body === undefined ? {} : { body }) })
  if (r.status < 200 || r.status >= 300) throw new Error(`${method} ${path}: ${r.status} ${JSON.stringify(r.body)}`)
  return r.body
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms))
const until = async (what, fn, ms = 30000) => {
  const end = Date.now() + ms
  for (;;) { const v = await fn(); if (v) return v; if (Date.now() > end) throw new Error(`timed out: ${what}`); await sleep(500) }
}

const projectCommand = (command) => `bash: "$DSH_LAYERFS_CLI" workspace exec "$DSH_LAYERFS_WS" ${JSON.stringify(command)} --stream`

const tag = Date.now().toString(36)
const store = `e2e${tag}`, name = 'demo', project = `${store}/${name}`
let failed = false
try {
  step(`store + project ${project}`)
  const fixtureRoot = join(process.env.DSH_LAYERFS_DEV_DIR ?? fileURLToPath(new URL('../.dev', import.meta.url)), 'fixtures')
  mkdirSync(fixtureRoot, { recursive: true })
  const src = mkdtempSync(join(fixtureRoot, 'sessions-'))
  writeFileSync(join(src, 'README.md'), '# demo\n')
  mkdirSync(join(src, 'src')); writeFileSync(join(src, 'src', 'main.js'), 'console.log(1)\n')
  await http('POST', '/api/stores', { name: store })
  const init = await http('POST', '/api/projects', { store, name, source: src })
  await until('project init', async () => (await http('GET', `/api/project-inits/${init.id}`)).step === 'done', 60000)

  step('a session in the selected workspace, placed in the project')
  const placed = await openProject(project)
  assert.ok(placed.sessionId && placed.workspaceId)
  await page.waitForTimeout(1500)
  // The project owns the session; selecting scratch must keep that same session.
  const owningWorkspace = (await must('sessions.scratchOptions')).workspaces.find((workspace) => workspace.workspaceId === placed.workspaceId)
  const scratch = join(fixtureRoot, `scratch-${tag}`)
  mkdirSync(scratch, { recursive: true })
  await page.getByRole('button', { name: /^Scratch folder:/ }).click()
  await page.getByRole('menuitem', { name: 'Enter a folder path…' }).click()
  await page.getByRole('textbox', { name: 'Local folder path' }).fill(scratch)
  await page.getByRole('button', { name: 'Select folder', exact: true }).click()
  await until('scratch selected without changing ownership', async () => {
    const value = await must('session.place', { sessionId: placed.sessionId })
    return value.place?.project === project && value.place?.scratch === scratch
  })
  assert.equal(await page.getByRole('button', { name: 'Choose workspace', exact: true }).isVisible(), false, 'project mode uses its scratch selector')
  await must('sessions.choose', { sessionId: placed.sessionId, project: null })
  assert.equal((await must('session.place', { sessionId: placed.sessionId })).place.project, null)
  await must('sessions.choose', { sessionId: placed.sessionId, project })
  assert.equal((await must('session.place', { sessionId: placed.sessionId })).place.scratch, owningWorkspace.path, 'normal mode restores the original workspace folder')
  await must('sessions.scratch', { sessionId: placed.sessionId, cwd: scratch })
  const t1 = await runTurn(page, { message: `${projectCommand('echo one > one.txt')}\nbash: echo note > scratch-note.txt`, newSession: false })
  assert.ok(t1.ok, `turn 1: ${t1.error}`)
  // The placed blank keeps its id when the first turn materializes it.
  const sid = placed.sessionId
  assert.equal(t1.sessionId, sid, 'the turn ran in the placed session')
  let state = await until('bound + 1 checkpoint', async () => {
    const s = await must('session.state', { sessionId: sid })
    return s.binding?.state === 'bound' && s.checkpoints.some((c) => c.kind === 'turn') ? s : null
  })
  const b = state.binding
  console.log({ branch: b.branch, sandbox: b.sandbox, mount: b.mount, scratch: b.scratch })
  assert.equal(b.project, project)
  assert.ok(existsSync(join(b.mount, 'one.txt')), 'one.txt in the mount')
  assert.ok(existsSync(join(b.mount, 'README.md')), 'project files in the mount')
  assert.equal(b.scratch, scratch, 'chosen local folder retained at materialization')
  assert.ok(existsSync(b.scratch), 'scratch folder')
  assert.ok(existsSync(join(b.scratch, 'scratch-note.txt')), 'local shell writes selected scratch')
  assert.ok(!existsSync(join(b.mount, 'scratch-note.txt')), 'scratch writes stay outside project')
  const annotations = await must('session.annotations', { sessionId: sid })
  const execs = annotations.invocations.filter((item) => item.operation === 'workspace.exec')
  assert.ok(execs.length > 0 && execs.every((item) => item.origin === 'tool' && item.toolCallId && item.versions.length === 0), 'exec calls retain exact tool identities with no checkpoint links')
  assert.ok(annotations.invocations.some((item) => item.operation === 'workspace.commit' && item.origin === 'host' && item.versions.some((version) => version.effect === 'created')), 'host save owns its confirmed checkpoint')
  const cp1 = state.checkpoints.find((c) => c.kind === 'turn')
  assert.ok(cp1.commit, 'turn 1 committed')

  step('turn 2')
  const t2 = await runTurn(page, { message: projectCommand('echo two > two.txt'), newSession: false })
  assert.ok(t2.ok, `turn 2: ${t2.error}`)
  state = await until('2 checkpoints', async () => {
    const s = await must('session.state', { sessionId: sid })
    return s.checkpoints.filter((c) => c.kind === 'turn' && c.commit).length >= 2 ? s : null
  })
  assert.ok(existsSync(join(state.binding.mount, 'two.txt')))

  step('unchanged turn records no commit')
  const t3 = await runTurn(page, { message: 'bash: ls', newSession: false })
  assert.ok(t3.ok)
  await sleep(1000)
  state = await must('session.state', { sessionId: sid })
  assert.equal(state.checkpoints.filter((c) => c.kind === 'turn' && c.commit).length, 2, 'ls made no checkpoint')

  step('restore files to turn 1')
  const restored = await must('checkpoints.restore', { sessionId: sid, commit: cp1.commit })
  const mount = restored.binding.mount
  assert.ok(existsSync(join(mount, 'one.txt')), 'one.txt after restore')
  assert.ok(!existsSync(join(mount, 'two.txt')), 'two.txt gone after restore')
  console.log({ restoredBranch: restored.binding.branch, mount })

  step('branch the session at turn 1')
  await sleep(1500)
  state = await must('session.state', { sessionId: sid })
  const turn1 = state.checkpoints.find((c) => c.kind === 'turn' && c.turn === cp1.turn)
  assert.ok(turn1.endSeq !== undefined, 'turn 1 end seq recorded')
  const branched = await must('checkpoints.branch', { sessionId: sid, commit: turn1.commit, turn: turn1.turn })
  const child = branched.sessionId
  assert.ok(child && child !== sid)
  await page.waitForTimeout(2000)
  const t4 = await runTurn(page, { message: projectCommand('ls > listing.txt'), newSession: false })
  assert.ok(t4.ok, `child turn: ${t4.error}`)
  const cstate = await until('child bound', async () => {
    const s = await must('session.state', { sessionId: child })
    return s.binding?.state === 'bound' ? s : null
  })
  assert.equal(cstate.binding.from?.session, sid)
  assert.ok(existsSync(join(cstate.binding.mount, 'one.txt')), 'child has turn 1 files')
  assert.ok(!existsSync(join(cstate.binding.mount, 'two.txt')), 'child lacks turn 2 files')
  const listing = readFileSync(join(cstate.binding.mount, 'listing.txt'), 'utf8')
  assert.match(listing, /one\.txt/)

  step('LayerFS session store')
  await must('session.mirror', { sessionId: sid })
  const listed = await must('sessions.list', { project })
  console.log(listed.sessions.map((s) => ({ id: s.sessionId, stored: s.stored, cps: s.checkpoints, title: s.title })))
  const row = listed.sessions.find((s) => s.sessionId === sid)
  assert.ok(row?.stored, 'session stored in the project')
  const files = await http('GET', `/api/projects/${encodeURIComponent(project)}/fs?branch=${encodeURIComponent(`dsh.session.${sid.replace(/^session-/, '').replace(/[^A-Za-z0-9]/g, '').toLowerCase()}`)}&path=${encodeURIComponent('/.dsh/session')}`)
  console.log(JSON.stringify(files).slice(0, 300))

  step('Folder session (Scratch workspace): plain DSH, no binding')
  const scratchWs = await must('sessions.prepareScratch')
  assert.ok(scratchWs.workspaceId)
  await page.waitForTimeout(1500)
  const ts = await runTurn(page, { message: 'bash: echo s > s.txt', newSession: false })
  assert.ok(ts.ok, `scratch turn: ${ts.error}`)
  const runtime = await must('runtime.status')
  assert.equal(runtime.sessionPersistence, 'layerfs', 'the LayerFS session persistence row runs')
  const sstate = await must('session.state', { sessionId: ts.sessionId })
  assert.equal(sstate.binding, null, 'a Scratch-workspace session binds to nothing')
  assert.equal(sstate.checkpoints.length, 0, 'no checkpoints for a folder session')
  assert.ok(existsSync(join(scratchWs.anchor, 's.txt')), 's.txt in the workspace folder itself')

  step('Default workspace session: also a plain folder session')
  const defaultRow = await page.locator('button[aria-label^="New session in "]').evaluateAll((els) => els.map((e) => e.getAttribute('aria-label').replace(/^New session in /, '')))
  const defaultTitle = defaultRow.find((title) => /default/i.test(title))
  assert.ok(defaultTitle, `default workspace row in ${JSON.stringify(defaultRow)}`)
  const td = await runTurn(page, { workspace: defaultTitle, message: 'bash: echo d > d.txt' })
  assert.ok(td.ok, `default workspace turn: ${td.error}`)
  const dstate = await must('session.state', { sessionId: td.sessionId })
  assert.equal(dstate.binding, null)
  assert.equal(dstate.checkpoints.length, 0)
  // A folder session that already ran keeps running as stock DSH (not refused, not moved).
  const tl = await runTurn(page, { message: 'bash: echo l > l.txt', newSession: false })
  assert.ok(tl.ok, `second folder turn: ${tl.error}`)
  const lstate = await must('session.state', { sessionId: td.sessionId })
  assert.equal(lstate.binding, null)

  step('UI: the footer switch shows projects with sessions, a row folds its sessions, and the header menu opens the project picker')
  await dismiss(page)
  assert.equal(await page.locator('[data-layerfs-workspace-mount], [data-layerfs-work-areas], [data-layerfs-projects-section]').count(), 0, 'no injected sections')
  await page.evaluate(() => localStorage.removeItem('dsh-layerfs:workspaces-view'))
  const footerSwitch = page.getByRole('button', { name: 'Show LayerFS projects', exact: true })
  await footerSwitch.waitFor({ timeout: 15000 })
  await footerSwitch.click()
  const view = page.locator('[data-layerfs-workspaces-view]')
  await view.waitFor({ timeout: 15000 })
  assert.equal(await page.locator('[data-slot="sidebar.workspaces"] [data-row-key^="workspace:"]').count(), 0, 'the LayerFS view replaces the stock list')
  // Rows are named by the project alone; the Store and machine are in the tooltip (docs/specs/layerfs-workspaces.md §4).
  const rowTitle = `button[title^="${name} · ${store}"]`
  const projectRow = view.locator('[data-layerfs-project]').filter({ has: page.locator(rowTitle) }).first()
  const fold = projectRow.locator(rowTitle)
  await projectRow.waitFor({ timeout: 15000 })
  assert.equal(await projectRow.locator('[role=img][data-tone]').count(), 0, 'no status lights')
  await fold.click()
  assert.equal(await fold.getAttribute('aria-expanded'), 'true', 'a row click folds its sessions out')
  await projectRow.hover()
  await projectRow.getByRole('button', { name: new RegExp(`^Set up ${name}( · |$)`) }).click()
  // ⚙ opens the project dialog in configure mode (docs/specs/layerfs-workspaces.md §3).
  const setup = page.getByRole('dialog', { name, exact: true })
  await setup.getByText('Machine', { exact: true }).waitFor({ timeout: 15000 })
  await setup.locator(`[data-machine="native"][aria-checked="true"]`).waitFor()
  await setup.getByRole('button', { name: 'Done', exact: true }).click()
  await setup.waitFor({ state: 'detached' })
  // The header's + → Other project… opens the picker; there is no Projects page.
  await view.locator('[data-layerfs-new-menu]').click()
  await page.getByRole('menuitem', { name: /^Other project/ }).click()
  const picker = page.getByRole('dialog', { name: 'Projects', exact: true })
  await picker.getByRole('button', { name: new RegExp(`^New session in ${name} · ${store}$`) }).waitFor({ timeout: 15000 })
  await picker.getByRole('button', { name: 'Close', exact: true }).click()
  await picker.waitFor({ state: 'detached' })
  await projectRow.hover()
  await projectRow.getByRole('button', { name: new RegExp(`^New session in ${name}( · |$)`) }).click()
  await page.waitForTimeout(1500)
  const tp = await runTurn(page, { message: projectCommand('echo picked > picked.txt'), newSession: false })
  assert.ok(tp.ok, `picked project turn: ${tp.error}`)
  const pstate = await until('picked session bound', async () => {
    const s = await must('session.state', { sessionId: tp.sessionId })
    return s.binding?.state === 'bound' ? s : null
  })
  assert.equal(pstate.binding.project, project, 'the picked project')
  assert.ok(existsSync(join(pstate.binding.mount, 'picked.txt')))
  assert.ok(existsSync(join(pstate.binding.mount, 'README.md')), 'project files')
  await view.locator('[data-layerfs-view-switch]').click()
  await page.locator('[data-slot="sidebar.workspaces"] [data-row-key^="workspace:"]').first().waitFor({ timeout: 15000 })
  assert.equal(await view.count(), 0, 'switching back restores the stock browser')

  step('UI: the New Session project control shows no project, then switches and creates one')
  await page.locator(`[role=treeitem][data-row-key="session:${ts.sessionId}"]`).first().click()
  await page.getByRole('button', { name: 'New session', exact: true }).first().click()
  const heroChip = page.locator('[data-layerfs-hero] button[aria-label^="LayerFS project:"]').first()
  await heroChip.waitFor({ timeout: 15000 })
  await until('hero shows no project', async () => (await heroChip.innerText()).trim() === 'No project')
  await heroChip.click()
  // Rows show where the project runs; the Store is in the tooltip.
  await page.getByRole('menuitem').filter({ has: page.locator(`[title^="${name} · ${store}"]`) }).first().click()
  await until('hero switched to the project', async () => (await page.locator('[data-layerfs-hero] button[aria-label^="LayerFS project:"]').first().innerText()).trim() === name)
  await page.locator('[data-layerfs-hero] button[aria-label^="LayerFS project:"]').first().click()
  // New project… opens the project dialog in create mode (docs/specs/layerfs-workspaces.md §2-3).
  await page.getByRole('menuitem', { name: 'New project…' }).click()
  const draft = page.getByRole('dialog', { name: 'New project', exact: true })
  // One page: Project, Store, Machine; the rail says which are ready and jumps to them.
  await draft.getByLabel('Name', { exact: true }).fill('blank')
  await draft.getByRole('navigation').getByRole('button', { name: /^Store/ }).click()
  await draft.getByRole('tab', { name: /^Existing/ }).click()
  await draft.getByRole('radio', { name: new RegExp(`^${store.replace(/[.]/g, '\\.')}\\b`) }).click()
  await draft.getByRole('navigation').locator('button[data-ready]').filter({ hasText: /^Store/ }).waitFor()
  // The fixture Store is on native: keep it there instead of the docker default.
  await draft.getByRole('radio', { name: /^LayerFS Home/ }).click()
  await draft.getByRole('button', { name: 'Create and open' }).click()
  await draft.waitFor({ state: 'detached', timeout: 60000 })
  await until('hero on the new project', async () => (await page.locator('[data-layerfs-hero] button[aria-label^="LayerFS project:"]').first().innerText().catch(() => '')).trim() === 'blank', 60000)
  const tb = await runTurn(page, { message: projectCommand('echo b > b.txt'), newSession: false })
  assert.ok(tb.ok, `new project turn: ${tb.error}`)
  const bstate = await until('new project session bound', async () => {
    const s = await must('session.state', { sessionId: tb.sessionId })
    return s.binding?.state === 'bound' ? s : null
  })
  assert.equal(bstate.binding.project, `${store}/blank`)

  step('UI: a project session has Files, History and Sandbox tabs; no LayerFS side panel')
  await must('sessions.open', { sessionId: sid, project })
  for (const name of ['Files', 'History', 'Sandbox']) await page.getByRole('tab', { name, exact: true }).first().waitFor({ timeout: 15000 })
  await page.getByRole('tab', { name: 'Sandbox', exact: true }).first().click()
  await page.getByText('Agents', { exact: true }).first().waitFor({ timeout: 15000 })
  assert.equal(await page.locator('[data-composer-seat] [contenteditable="true"]:visible').count(), 0, 'LayerFS tabs hide the composer')
  assert.equal(await page.getByRole('button', { name: new RegExp(`^Project ${project.replace('/', '\\/')}`) }).count(), 0, 'no LayerFS header button')
  assert.ok(!(await page.locator(`[role=treeitem][data-row-key="session:${sid}"]`).isVisible()), 'project conversation is excluded from normal workspace rows')
  await page.waitForTimeout(700)
  await page.screenshot({ path: '/tmp/e2e-sessions-sandbox.png' })
  await page.getByRole('tab', { name: 'Chat', exact: true }).first().click()

  step("Sandbox create: a project session's names no image (LayerFS's default rule, docs/specs/sandbox-images.md D6, §4.5)")
  const creates = (await must('logs.list', { project, limit: 1000 })).lines.filter((line) => line.argv[0] === 'sandbox' && line.argv[1] === 'create' && line.argv[2] === project)
  assert.ok(creates.length >= 1, `the call log has the project's sandbox create (${creates.length})`)
  for (const line of creates) assert.ok(!line.argv.some((a) => a === '--image' || a.startsWith('--image=')), `no --image injected: ${line.argv.join(' ')}`)

  step('Logs: the call log binds checkpoints to their calls; Reset with confirm')
  state = await must('session.state', { sessionId: sid })
  const bound = state.checkpoints.filter((c) => c.kind === 'turn' && c.commit && c.receipt)
  assert.ok(bound.length >= 2, 'turn checkpoints carry the receipt of the save that made them')
  const own = await must('logs.list', { tag: `dsh:${sid}:`, writes: true, limit: 1000 })
  console.log({ source: own.source, lines: own.lines.length })
  const commitLine = own.lines.find((line) => line.argv[0] === 'workspace' && line.argv[1] === 'commit' && line.tag === `dsh:${sid}:${bound[0].receipt}`)
  assert.ok(commitLine, 'the turn save has a workspace commit line tagged with its receipt')
  assert.equal(commitLine.dsh?.sessionId, sid)
  assert.equal(commitLine.dsh?.match, 'tag')
  assert.equal(commitLine.dsh?.turn, bound[0].turn, 'the line is labelled with its turn')
  assert.equal(commitLine.dsh?.checkpoint?.kind, 'turn')
  await openPanel(page, 'Logs')
  await page.getByRole('heading', { name: 'Logs', level: 1 }).waitFor({ timeout: 15000 })
  const logRows = page.getByRole('table', { name: 'Calls' }).locator('tbody tr')
  const commitRow = logRows.filter({ hasText: 'layerfs workspace commit' }).filter({ hasText: `turn ${bound[0].turn}` }).first()
  await commitRow.waitFor({ timeout: 15000 })
  await page.screenshot({ path: '/tmp/e2e-sessions-logs.png' })

  step('Logs: a row opens the call detail with its command, session and output')
  assert.equal(commitLine.output, true, 'the host keeps the output of a plugin call')
  // Click the time cell: the command cell's popover may sit over the row below.
  await commitRow.locator('td').first().click()
  const detail = page.getByRole('dialog', { name: 'Call workspace commit' })
  await detail.waitFor({ timeout: 10000 })
  assert.match(await detail.getByTestId('logs-detail-command').innerText(), /^layerfs workspace commit /, 'the detail shows the full command')
  await detail.getByText(`turn ${bound[0].turn}`).first().waitFor({ timeout: 10000 })
  await detail.getByRole('button', { name: 'Open session', exact: true }).waitFor({ timeout: 10000 })
  await detail.getByRole('button', { name: 'Calls of this checkpoint', exact: true }).waitFor({ timeout: 10000 })
  const stdout = detail.getByTestId('logs-detail-output').locator('pre[data-stream=stdout]')
  await stdout.waitFor({ timeout: 15000 })
  assert.match(await stdout.innerText(), /Committed/, 'the output section shows the stored stdout')
  await page.screenshot({ path: '/tmp/e2e-sessions-logs-detail.png' })
  await detail.getByRole('button', { name: 'Close', exact: true }).click()
  await detail.waitFor({ state: 'detached', timeout: 10000 })
  await page.getByRole('button', { name: 'Reset…', exact: true }).click()
  const resetDialog = page.getByRole('dialog')
  await resetDialog.getByText('Reset the call log?').waitFor({ timeout: 10000 })
  await resetDialog.getByRole('button', { name: 'Reset', exact: true }).click()
  await page.getByText(/ calls removed\.$/).first().waitFor({ timeout: 15000 })
  const afterReset = await must('logs.list', { tag: `dsh:${sid}:`, limit: 1000 })
  assert.equal(afterReset.lines.length, 0, "Reset removed the session's calls")

  if (!process.argv.includes('--keep')) {
    step('cleanup')
    for (const id of [child, sid, ts.sessionId, td.sessionId, tp.sessionId, tb.sessionId]) await must('sessions.forget', { sessionId: id })
    assert.ok(existsSync(join(scratch, 'scratch-note.txt')), 'forgetting a project session preserves user-selected scratch files')
    await http('DELETE', `/api/projects/${encodeURIComponent(`${store}/blank`)}`)
    await http('DELETE', `/api/projects/${encodeURIComponent(project)}`)
    await http('DELETE', `/api/stores/${encodeURIComponent(store)}?confirm=${encodeURIComponent(store)}`).catch((e) => console.log('store kept:', e.message))
  }

  console.log('\nE2E PASS')
} catch (error) {
  failed = true
  console.error('\nE2E FAIL:', error?.stack ?? error)
  await page.screenshot({ path: '/tmp/e2e-sessions-fail.png' }).catch(() => {})
} finally {
  if (logs.length) console.log('console:', logs.slice(0, 20).join('\n'))
  await browser.close()
  process.exit(failed ? 1 : 0)
}
