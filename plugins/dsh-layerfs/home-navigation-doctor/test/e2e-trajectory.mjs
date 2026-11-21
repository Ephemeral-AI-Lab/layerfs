#!/usr/bin/env node
// End-to-end: the LayerFS trajectory (docs/specs/layerfs-trajectory.md) with
// the scripted fake model. Needs a dev server started with a fake-llm overlay.
//   DSH_LAYERFS_DEV_DIR=<dir> node test/e2e-trajectory.mjs [--keep]
// Checks: `session.trajectory` joins a tool call that writes and commits
// through the LayerFS CLI to its LayerFS calls and the version it created
// (with its branch), and a plain call to none; `session.trajectory.record`
// reads the call in full. The project session's Trajectory tab is this
// plugin's (the "Only LayerFS writes" filter), shows the committing call with
// its `+` chip, opens a call in the inspector, filters to LayerFS writes, and
// the chip opens the History tab on that commit; Chat's Inspect on a call
// opens it on that call. Each mode shows one Trajectory tab: a plain folder
// session (DSH mode) keeps DSH's stock Trajectory (its "Duration" toolbar), and
// returning to the project session brings ours back.
import { fileURLToPath } from 'node:url'
import assert from 'node:assert/strict'
import { mkdirSync, mkdtempSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { openDev, runTurn } from '../tools/e2e-lib.mjs'
import { dismiss } from '../tools/ui.mjs'

const step = (m) => console.log(`\n== ${m}`)
const shots = process.env.E2E_SHOTS ?? '/tmp'
const { browser, page, logs } = await openDev()
await page.waitForFunction(() => window.__dshLayerfs, null, { timeout: 30000 })
const op = (name, args = {}) => page.evaluate(async ([n, a]) => {
  try { return { ok: true, value: await window.__dshLayerfs.op(n, a) } } catch (e) { return { ok: false, code: e.code, message: e.message } }
}, [name, args])
const must = async (name, args) => { const r = await op(name, args); if (!r.ok) throw new Error(`${name}: ${r.code} ${r.message}`); return r.value }
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
const tools = (turn) => turn.records.filter((r) => r.kind === 'tool')

const tag = Date.now().toString(36)
const store = `e2etj${tag}`, name = 'demo', project = `${store}/${name}`
let failed = false, sid
try {
  step(`store + project ${project}`)
  const fixtureRoot = join(process.env.DSH_LAYERFS_DEV_DIR ?? fileURLToPath(new URL('../.dev', import.meta.url)), 'fixtures')
  mkdirSync(fixtureRoot, { recursive: true })
  const src = mkdtempSync(join(fixtureRoot, 'trajectory-'))
  writeFileSync(join(src, 'README.md'), '# demo\n')
  await http('POST', '/api/stores', { name: store })
  const init = await http('POST', '/api/projects', { store, name, source: src })
  await until('project init', async () => (await http('GET', `/api/project-inits/${init.id}`)).step === 'done', 60000)

  step('one turn: a call that writes and commits through LayerFS, and a plain call')
  const placed = await page.evaluate(async (k) => window.__dshLayerfs.openProjectSession(k), project)
  sid = placed.sessionId
  await page.waitForTimeout(1500)
  const writeAndCommit = `"$DSH_LAYERFS_CLI" workspace exec "$DSH_LAYERFS_WS" "echo traced > traced.txt" --stream && "$DSH_LAYERFS_CLI" workspace commit "$DSH_LAYERFS_WS" --message "trajectory e2e"`
  const turn = await runTurn(page, { message: `bash: ${writeAndCommit}\nbash: echo plain`, newSession: false })
  assert.ok(turn.ok, `turn: ${turn.error}`)
  const state = await until('bound', async () => {
    const s = await must('session.state', { sessionId: sid })
    return s.binding?.branch ? s : null
  })
  const branch = state.binding.branch

  step('session.trajectory joins the call to its LayerFS calls and the version it created')
  const answer = await until('receipts collected', async () => {
    const a = await must('session.trajectory', { sessionId: sid })
    return a.turns.some((x) => tools(x).some((c) => c.ops.some((o) => o.operation === 'workspace.commit'))) ? a : null
  })
  assert.equal(answer.logLoaded, true)
  assert.equal(answer.branch, branch)
  const t1 = answer.turns.find((x) => x.turn === 1)
  assert.ok(t1, 'turn 1')
  assert.ok(t1.records.some((r) => r.kind === 'user' && /echo plain/.test(r.text)), 'the user message is a record')
  assert.ok(t1.records.some((r) => r.kind === 'assistant'), 'the model step is a record')
  const committing = tools(t1).find((c) => c.ops.some((o) => o.operation === 'workspace.commit'))
  const plain = tools(t1).find((c) => c !== committing)
  assert.equal(committing.name, 'bash')
  assert.match(committing.summary, /workspace commit/)
  assert.ok(committing.endedAt >= committing.at, 'the call has its result time')
  assert.deepEqual(committing.ops.map((o) => o.operation), ['workspace.exec', 'workspace.commit'], 'two LayerFS calls, in order')
  const commitOp = committing.ops[1]
  assert.equal(commitOp.kind, 'Committed')
  assert.equal(commitOp.versions.length, 1)
  const version = commitOp.versions[0]
  assert.equal(version.effect, 'created')
  assert.equal(version.branch, branch, 'opens on the session branch')
  assert.equal(committing.ops[0].exit, 0, 'exec exit code from the stored output')
  assert.ok(plain && plain.ops.length === 0 && plain.summary === 'echo plain', 'a call without LayerFS calls')
  assert.ok(answer.counts.versionsCreated >= 1 && answer.counts.callsWithVersions >= 1, `counts ${JSON.stringify(answer.counts)}`)
  const detail = await must('session.trajectory.record', { sessionId: sid, seq: committing.seq })
  assert.equal(detail.kind, 'tool')
  assert.match(detail.args, /workspace commit/)
  assert.equal(typeof detail.result, 'string')
  console.log({ branch, version: version.id.slice(0, 10) })

  step('a finished subagent\'s trajectory has its conversation log (read from disk once DSH unloads it)')
  const childCall = `"$DSH_LAYERFS_CLI" file write "$DSH_LAYERFS_PROJECT" "$DSH_LAYERFS_BRANCH" /child.txt --message "child write" <<< child`
  const parentTurn = await runTurn(page, { message: `subagent: traj-child | bash: ${childCall}\\n`, newSession: false })
  assert.ok(parentTurn.ok, `parent turn: ${parentTurn.error}`)
  const child = await until('the child\'s tagged call', async () => {
    const lines = (await must('logs.list', { project, limit: 200 })).lines ?? []
    const line = lines.find((l) => l.argv?.[0] === 'file' && l.argv?.[1] === 'write' && l.dsh?.sessionId && l.dsh.sessionId !== sid)
    return line?.dsh.sessionId
  })
  const childAnswer = await until('the child\'s trajectory with its log', async () => {
    const a = await must('session.trajectory', { sessionId: child })
    return a.logLoaded && a.turns.some((x) => tools(x).some((c) => c.ops.some((o) => o.operation === 'file.write'))) ? a : null
  }, 60000)
  assert.ok(childAnswer.turns.some((x) => x.records.some((r) => r.kind === 'user')), 'the child\'s prompt is a record')
  const childRecord = tools(childAnswer.turns.find((x) => tools(x).length))[0]
  const childDetail = await must('session.trajectory.record', { sessionId: child, seq: childRecord.seq })
  assert.ok(childDetail, 'the child\'s call reads in full')

  step('UI: the project session\'s Trajectory tab is the LayerFS trajectory')
  await must('sessions.open', { sessionId: sid, project })
  const tab = page.getByRole('tab', { name: 'Trajectory', exact: true }).first()
  await tab.waitFor({ timeout: 15000 })
  await tab.click()
  const onlyWrites = page.getByRole('checkbox', { name: 'Only LayerFS writes' }).first()
  await onlyWrites.waitFor({ timeout: 15000 })
  assert.equal(await page.getByRole('tab', { name: 'LayerFS', exact: true }).count(), 0, 'the old LayerFS tab is gone')
  assert.equal(await page.getByRole('tab', { name: 'Trajectory', exact: true }).count(), 1, 'one Trajectory tab in LayerFS mode')
  await page.getByText('echo plain', { exact: true }).first().waitFor({ timeout: 15000 })
  const chip = page.locator(`button[aria-label^="New version ${version.id.slice(2, 10)}"]`).first()
  await chip.waitFor({ timeout: 5000 })
  await page.getByRole('group', { name: 'Trajectory timeline' }).first().waitFor({ timeout: 5000 })
  await page.waitForTimeout(400)
  await page.screenshot({ path: join(shots, 'e2e-trajectory-tab.png') })

  step('UI: a tool row opens the inspector; its LayerFS tab offers Open in History')
  // The row's main button (the user message mentions the command too: pick the TOOL row).
  await page.locator('button[aria-pressed]').filter({ hasText: 'TOOL' }).filter({ hasText: 'workspace commit' }).first().click()
  const inspector = page.getByRole('complementary', { name: 'Record details' })
  await inspector.waitFor({ timeout: 5000 })
  await inspector.getByText(committing.callId, { exact: true }).waitFor({ timeout: 5000 })
  await inspector.getByRole('tab', { name: 'Payload', exact: true }).click()
  await inspector.getByText(/workspace commit/).first().waitFor({ timeout: 10000 })
  await inspector.getByRole('tab', { name: 'LayerFS', exact: true }).click()
  await inspector.getByRole('button', { name: 'Open in History', exact: true }).first().waitFor({ timeout: 5000 })
  await page.screenshot({ path: join(shots, 'e2e-trajectory-inspector.png') })
  await inspector.getByRole('button', { name: 'Close details' }).click()
  await inspector.waitFor({ state: 'detached', timeout: 5000 })

  step('UI: Only LayerFS writes hides the plain call and keeps the committing one')
  await onlyWrites.check()
  await until('plain call hidden', async () => (await page.getByText('echo plain', { exact: true }).count()) === 0, 5000)
  await chip.waitFor({ timeout: 5000 })
  await onlyWrites.uncheck()
  await page.getByText('echo plain', { exact: true }).first().waitFor({ timeout: 5000 })

  step('UI: the version chip opens History on that commit')
  await chip.click()
  await page.getByRole('tab', { name: 'History', exact: true, selected: true }).first().waitFor({ timeout: 10000 })
  await page.locator(`code[title="${version.id}"]`).first().waitFor({ timeout: 20000 })
  await page.waitForTimeout(600)
  await page.screenshot({ path: join(shots, 'e2e-trajectory-history.png') })
  await page.getByRole('tab', { name: 'Chat', exact: true }).first().click()

  step('UI: Chat\'s Inspect on a tool call opens the LayerFS trajectory on that call')
  await page.getByRole('tab', { name: 'Chat', exact: true }).first().click()
  await page.getByText(/^Completed in/).first().click()
  await page.getByText('Ran commands').first().click()
  await page.getByText('echo plain').last().click()
  await page.getByRole('button', { name: 'Inspect' }).first().click()
  await page.getByRole('tab', { name: 'Trajectory', exact: true, selected: true }).first().waitFor({ timeout: 10000 })
  await page.getByRole('checkbox', { name: 'Only LayerFS writes' }).first().waitFor({ timeout: 10000 })
  await page.getByRole('tab', { name: 'Payload', exact: true }).first().waitFor({ timeout: 10000 })
  await page.getByRole('tab', { name: 'Chat', exact: true }).first().click()

  step('UI: a plain folder session (DSH mode) keeps DSH\'s stock Trajectory')
  await dismiss(page)
  const rows = await page.locator('button[aria-label^="New session in "]').evaluateAll((els) => els.map((e) => e.getAttribute('aria-label').replace(/^New session in /, '')))
  const defaultTitle = rows.find((title) => /default/i.test(title))
  assert.ok(defaultTitle, `default workspace row in ${JSON.stringify(rows)}`)
  const folder = await runTurn(page, { workspace: defaultTitle, message: 'bash: echo folder' })
  assert.ok(folder.ok, `folder turn: ${folder.error}`)
  assert.equal((await must('session.state', { sessionId: folder.sessionId })).binding, null, 'a folder session')
  const stockTab = page.getByRole('tab', { name: 'Trajectory', exact: true })
  await stockTab.first().waitFor({ timeout: 15000 })
  await page.waitForTimeout(1000)
  assert.equal(await stockTab.count(), 1, 'one Trajectory tab in DSH mode')
  await stockTab.first().click()
  await page.getByText('Duration', { exact: true }).first().waitFor({ timeout: 15000 })
  assert.equal(await page.getByRole('checkbox', { name: 'Only LayerFS writes' }).count(), 0, 'not the LayerFS trajectory')
  await page.screenshot({ path: join(shots, 'e2e-trajectory-stock.png') })
  await page.getByRole('tab', { name: 'Chat', exact: true }).first().click()

  step('UI: back in the project session (LayerFS mode) there is again one Trajectory tab, ours')
  await must('sessions.open', { sessionId: sid, project })
  await page.getByRole('tab', { name: 'Files', exact: true }).first().waitFor({ timeout: 15000 })
  await page.waitForTimeout(1500)
  const again = page.getByRole('tab', { name: 'Trajectory', exact: true })
  assert.equal(await again.count(), 1, 'one Trajectory tab after switching modes')
  await again.first().click()
  await page.getByRole('checkbox', { name: 'Only LayerFS writes' }).first().waitFor({ timeout: 15000 })
  await page.getByRole('tab', { name: 'Chat', exact: true }).first().click()

  step('UI: attachments still work in LayerFS mode (DSH\'s ui-attachment declares the stock Trajectory)')
  const imgs = await page.locator('img').count()
  const png = join(fixtureRoot, 'attach.png')
  writeFileSync(png, Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==', 'base64'))
  await page.locator('input[type=file]').first().setInputFiles(png)
  await until('attachment chip', async () => (await page.locator('img').count()) > imgs, 10000)

  if (!process.argv.includes('--keep')) {
    step('cleanup')
    await must('sessions.forget', { sessionId: sid })
    await http('DELETE', `/api/projects/${encodeURIComponent(project)}`)
    await http('DELETE', `/api/stores/${encodeURIComponent(store)}?confirm=${encodeURIComponent(store)}`).catch((e) => console.log('store kept:', e.message))
  }
  console.log('\nE2E PASS')
} catch (error) {
  failed = true
  console.error('\nE2E FAIL:', error?.stack ?? error)
  await page.screenshot({ path: join(shots, 'e2e-trajectory-fail.png') }).catch(() => {})
} finally {
  if (logs.length) console.log('console:', logs.slice(0, 20).join('\n'))
  await browser.close()
  process.exit(failed ? 1 : 0)
}
