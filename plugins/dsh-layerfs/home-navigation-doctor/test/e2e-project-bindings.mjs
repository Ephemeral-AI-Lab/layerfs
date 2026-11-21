// Live private-home proof: local history survives removal and name reuse;
// navigation/status reads never recreate execution resources.
import { fileURLToPath } from 'node:url'
import assert from 'node:assert/strict'
import { mkdirSync } from 'node:fs'
import { join } from 'node:path'
import { openDev, runTurn } from '../tools/e2e-lib.mjs'

const { browser, page, logs } = await openDev()
page.setDefaultTimeout(20_000)
const dev = process.env.DSH_LAYERFS_DEV_DIR ?? fileURLToPath(new URL('../.dev', import.meta.url))
const store = `binding${Date.now().toString(36)}`, project = `${store}/app`, encoded = encodeURIComponent(project)
const op = (name, args = {}) => page.evaluate(([name, args]) => window.__dshLayerfs.op(name, args), [name, args])
const http = async (method, path, body) => {
  const reply = await op('http', { method, path, body })
  if (reply.status < 200 || reply.status >= 300) throw Object.assign(new Error(`${method} ${path}: ${JSON.stringify(reply)}`), { code: reply.body?.code })
  return reply.body
}
async function init() {
  await http('POST', '/api/stores', { name: store })
  const job = await http('POST', '/api/projects', { store, name: 'app' })
  for (let i = 0; i < 120; i++) {
    const current = await http('GET', `/api/project-inits/${job.id}`)
    if (current.error) throw new Error(JSON.stringify(current.error))
    if (current.ended) return
    await new Promise((resolve) => setTimeout(resolve, 250))
  }
  throw new Error('project init timed out')
}
async function remove() {
  await http('DELETE', `/api/projects/${encoded}`)
  // The SDK can refuse removal while a concurrent status read holds a short
  // lease. Retry only that confirmed, side-effect-free refusal.
  for (let i = 0; i < 30; i++) {
    try { await http('DELETE', `/api/stores/${store}?confirm=${store}`); return }
    catch (error) { if (error.code !== 'InUse') throw error; await new Promise((resolve) => setTimeout(resolve, 150)) }
  }
  throw new Error('Store read leases did not settle')
}
const refusal = async (name, args) => page.evaluate(async ([name, args]) => {
  try { await window.__dshLayerfs.op(name, args); return null } catch (error) { return { code: error.code, message: error.message } }
}, [name, args])
try {
  await page.waitForFunction(() => window.__dshLayerfs)
  await init()
  let original = (await op('projects.catalog')).rows.find((row) => row.key === project && row.catalogState === 'available')
  assert.ok(original.projectId)
  const placed = await page.evaluate(([key, id]) => window.__dshLayerfs.openProjectSession(key, undefined, id), [project, original.projectId])
  const id = placed.sessionId
  // Placing a conversation claims an unclaimed project: its UUID replaces the `unclaimed:` listing id.
  original = (await op('projects.catalog')).rows.find((row) => row.key === project && row.catalogState === 'available')
  assert.ok(!original.projectId.startsWith('unclaimed:'))
  await page.waitForTimeout(700)
  let state = await op('session.state', { sessionId: id })
  assert.equal(state.execution.state, 'inactive')
  assert.equal(state.execution.available, true)
  assert.equal((await http('GET', `/api/projects/${encoded}/sandboxes`)).length, 0)
  const turn = await runTurn(page, { newSession: false, message: 'bash: "$DSH_LAYERFS_CLI" workspace exec "$DSH_LAYERFS_WS" "echo owned > owned.txt" --stream' })
  assert.ok(turn.ok, turn.error)
  state = await op('session.state', { sessionId: id })
  assert.equal(state.execution.state, 'active')
  const checkpoint = state.checkpoints.findLast((row) => row.commit)
  assert.ok(checkpoint)
  await op('session.mirror', { sessionId: id })
  await op('sessions.close', { sessionId: id })
  state = await op('session.state', { sessionId: id })
  assert.equal(state.execution.state, 'inactive')
  await op('sessions.open', { sessionId: id, project })
  await page.waitForTimeout(500)
  assert.equal((await op('session.state', { sessionId: id })).execution.state, 'inactive')
  assert.equal((await http('GET', `/api/projects/${encoded}/sandboxes`)).length, 0)

  await remove()
  const missing = (await op('projects.catalog')).rows.find((row) => row.projectId === original.projectId)
  assert.equal(missing.catalogState, 'removed')
  assert.ok((await op('sessions.list', { project, projectId: original.projectId, localOnly: true })).sessions.some((row) => row.sessionId === id))
  state = await op('session.state', { sessionId: id })
  assert.equal(state.execution.available, false)
  assert.ok(state.checkpoints.some((row) => row.commit === checkpoint.commit))
  await op('sessions.open', { sessionId: id, project })
  await page.getByText('done: 1 commands', { exact: false }).first().waitFor()

  await init()
  const rows = (await op('projects.catalog')).rows.filter((row) => row.key === project)
  const replacement = rows.find((row) => row.catalogState === 'available')
  assert.ok(replacement)
  assert.notEqual(replacement.projectId, original.projectId)
  assert.equal(rows.find((row) => row.projectId === original.projectId).catalogState, 'identity-mismatch')
  assert.equal((await op('sessions.list', { project, projectId: replacement.projectId })).sessions.length, 0)
  assert.ok((await op('sessions.list', { project, projectId: original.projectId })).sessions.some((row) => row.sessionId === id))
  assert.equal((await refusal('sessions.choose', { sessionId: id, project, projectId: replacement.projectId })).code, 'ProjectIdentityChanged')
  assert.equal((await refusal('checkpoints.restore', { sessionId: id, commit: checkpoint.commit })).code, 'ProjectIdentityChanged')
  assert.equal((await refusal('session.mirror', { sessionId: id })).code, 'ProjectIdentityChanged')
  const scoped = await op('http', { method: 'GET', path: `/api/projects/${encoded}/branches?projectId=${original.projectId}` })
  assert.equal(scoped.body.code, 'ProjectIdentityChanged')
  state = await op('session.state', { sessionId: id })
  assert.equal(state.execution.projectId, original.projectId)
  assert.equal(state.execution.available, false)
  assert.equal((await http('GET', `/api/projects/${encoded}/sandboxes`)).length, 0)
  // The picker (sidebar + → Other project…) lists the retained project by its identity
  // and says its name now belongs to another project; it can't start a session (docs/specs/layerfs-workspaces.md §2).
  await page.evaluate(() => localStorage.setItem('dsh-layerfs:workspaces-view', 'layerfs'))
  await page.reload({ waitUntil: 'networkidle' })
  await page.waitForFunction(() => window.__dshLayerfs)
  await page.locator('[data-layerfs-new-menu]').click()
  await page.getByRole('menuitem', { name: /^Other project/ }).click()
  const picker = page.getByRole('dialog', { name: 'Projects', exact: true })
  const retained = picker.locator(`li[data-layerfs-project="${original.projectId}"]`)
  await retained.waitFor({ timeout: 25_000 })
  await retained.getByText('Replaced', { exact: true }).waitFor()
  assert.equal(await retained.getByRole('button', { name: /^New session in app · / }).isDisabled(), true)
  mkdirSync(join(dev, 'binding-artifacts'), { recursive: true })
  await page.screenshot({ path: join(dev, 'binding-artifacts', 'retained-and-replacement.png') })
  await retained.hover()
  await retained.getByRole('button', { name: 'Set up app', exact: true }).click()
  const setup = page.getByRole('dialog', { name: 'app', exact: true })
  await setup.getByText('Machine', { exact: true }).waitFor()
  await page.getByRole('button', { name: 'Collapse sidebar', exact: true }).click().catch(() => {})
  await page.setViewportSize({ width: 320, height: 844 })
  await page.waitForTimeout(250)
  assert.equal(await setup.evaluate((node) => node.scrollWidth > node.clientWidth), false)
  await page.screenshot({ path: join(dev, 'binding-artifacts', 'retained-project-mobile.png') })
  assert.deepEqual(logs, [])
  console.log('PASS: lazy execution, retained local history/checkpoints, Store removal, same-name replacement isolation, and blocked restore/archive/read on identity mismatch')
} catch (error) {
  mkdirSync(join(dev, 'binding-artifacts'), { recursive: true })
  await page.screenshot({ path: join(dev, 'binding-artifacts', 'failure.png') })
  throw error
} finally {
  await http('DELETE', `/api/projects/${encoded}`).catch(() => {})
  await http('DELETE', `/api/stores/${store}?confirm=${store}`).catch(() => {})
  await browser.close()
}
