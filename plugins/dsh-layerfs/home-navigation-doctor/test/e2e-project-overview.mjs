// Sidebar and project dialog states against an isolated dev server. Machine
// responses are simulated; no VM is started or stopped. The initial archive
// failure check uses the real host operation.
import { fileURLToPath } from 'node:url'
import assert from 'node:assert/strict'
import { mkdirSync } from 'node:fs'
import { join } from 'node:path'
import { openDev } from '../tools/e2e-lib.mjs'

const { browser, page, logs } = await openDev()
page.setDefaultTimeout(15_000)
const until = async (check, ms = 15_000) => { const end = Date.now() + ms; while (Date.now() < end) { if (await check()) return; await new Promise((r) => setTimeout(r, 150)) } throw new Error('timed out') }
const artifacts = join(process.env.DSH_LAYERFS_DEV_DIR ?? fileURLToPath(new URL('../.dev', import.meta.url)), 'project-overview-artifacts')
mkdirSync(artifacts, { recursive: true })
try {
  await page.waitForFunction(() => window.__dshLayerfs)
  const discovery = await page.evaluate(async () => {
    const api = window.__dshLayerfs
    return [await api.op('sessions.list', { project: 'missing-store/missing-project' }), await api.op('sessions.list', { project: 'missing-store/missing-project', localOnly: true })]
  })
  assert.ok(discovery[0].discoveryError, 'real host exposes archive failure')
  assert.equal(discovery[1].discoveryError, null, 'local-only reads require no machine access')
  await page.evaluate(() => {
    const api = window.__dshLayerfs
    const call = api.call.bind(api), op = api.op.bind(api)
    const fixture = window.__projectOverviewFixture = { state: 'stopped', starts: 0, sessions: 0, holdRecent: false }
    const row = { key: 'demo-store/compiler', name: 'compiler', store: 'demo-store', kind: 'sqlite', machine: 'build', created: 1794400000000, storeState: 'ready' }
    api.call = async (method, path, body) => {
      if (method === 'GET' && path === '/api/projects') return [row]
      if (method === 'GET' && path === '/api/machines') return { available: true, machines: [{ name: 'build', platform: 'lima', state: fixture.state, stores: ['demo-store'], sandboxes: [], cpus: 2, memory: 4000000000, disk: 20000000000 }], status: {}, platforms: [], stores: [], postgresql: [], sandboxes: [] }
      if (method === 'GET' && path === '/api/storage') return {
        machines: [{ name: 'native', platform: 'native', state: 'running', supports: null }, { name: 'build', platform: 'lima', state: fixture.state, supports: null }],
        stores: [{ name: 'demo-store', kind: 'sqlite', machine: 'build', projects: ['compiler'] }], buckets: [], jobs: [], at: Date.now(),
      }
      if (method === 'POST' && path === '/api/machines/build/start') {
        fixture.starts++
        await new Promise((resolve) => setTimeout(resolve, 400))
        fixture.state = 'ready'
        return {}
      }
      return call(method, path, body)
    }
    api.op = async (name, args) => {
      if (name === 'projects.catalog') return { rows: [row], scratchKey: null }
      if (name === 'projects.recent' && fixture.holdRecent) await new Promise((resolve) => { fixture.releaseRecent = resolve })
      if (name === 'projects.recent') return { projects: [{ project: row.key, projectId: null, lastActivity: 1794400000000, newestSession: 1794400000000, sessionIds: ['local-overview-conversation'] }] }
      if (name === 'sessions.list' && args.project === 'demo-store/compiler') return { sessions: [{ sessionId: 'local-overview-conversation', title: 'Saved local conversation', local: true, updatedAt: 1794400000000 }] }
      return op(name, args)
    }
    api.openProjectSession = async () => { fixture.sessions++; return { sessionId: 'new-overview-session', workspaceId: 'scratch' } }
  })
  // The sidebar's LayerFS view (docs/specs/layerfs-workspaces.md §4); there is no Projects page.
  await page.getByRole('button', { name: 'Show LayerFS projects', exact: true }).click()
  const list = page.locator('[data-layerfs-workspaces-view]')
  const project = list.locator('[data-layerfs-project]').filter({ has: page.locator('button[title^="compiler · demo-store"]') })
  await project.waitFor({ timeout: 25_000 })
  await project.getByText('Machine stopped', { exact: true }).waitFor()
  const start = project.getByRole('button', { name: 'Start machine build for compiler', exact: true })
  assert.equal(await start.isVisible(), true, 'a stopped machine offers Start instead of New session')
  await project.getByRole('button', { name: 'Toggle conversations in compiler', exact: true }).click()
  await list.getByText('Saved local conversation', { exact: true }).waitFor()
  assert.equal(await page.evaluate(() => window.__projectOverviewFixture.starts), 0, 'reading never starts a machine')
  await page.screenshot({ path: join(artifacts, 'stopped-desktop.png') })
  await start.click()
  await until(async () => (await page.evaluate(() => window.__projectOverviewFixture.starts)) === 1)
  await project.hover()
  const create = project.getByRole('button', { name: 'New session in compiler', exact: true })
  await until(async () => (await create.count()) === 1 && await create.isEnabled())
  await create.click()
  await until(async () => (await page.evaluate(() => window.__projectOverviewFixture.sessions)) === 1)

  // A remount paints the cached list even while its replacement read is pending.
  await page.evaluate(() => { window.__projectOverviewFixture.holdRecent = true })
  await list.getByRole('button', { name: 'Show DSH workspaces' }).click()
  await page.getByRole('button', { name: 'Show LayerFS projects' }).click()
  await page.waitForFunction(() => !!window.__projectOverviewFixture.releaseRecent)
  await project.getByRole('button', { name: 'Toggle conversations in compiler' }).waitFor()
  await page.evaluate(() => {
    const fixture = window.__projectOverviewFixture
    fixture.holdRecent = false
    fixture.releaseRecent()
  })

  // ⚙ opens the project dialog: the Store and where it runs (§3).
  await project.hover()
  await project.getByRole('button', { name: 'Set up compiler', exact: true }).click()
  const setup = page.getByRole('dialog', { name: 'compiler', exact: true })
  await setup.getByText('demo-store', { exact: true }).waitFor()
  await setup.locator('[data-machine="build"][aria-checked="true"]').getByText('Here now', { exact: true }).waitFor()
  await setup.locator('[data-machine="native"]').click()
  await setup.getByText(/^Moves the whole Store demo-store to native/).waitFor()
  await setup.getByRole('button', { name: 'Cancel', exact: true }).click()
  await page.setViewportSize({ width: 390, height: 844 })
  await page.waitForTimeout(300)
  await page.screenshot({ path: join(artifacts, 'setup-mobile.png') })
  assert.equal(await setup.evaluate((element) => element.scrollWidth > element.clientWidth), false, 'the dialog fits a phone viewport')
  await page.emulateMedia({ colorScheme: 'dark' })
  await page.waitForTimeout(300)
  await page.screenshot({ path: join(artifacts, 'setup-mobile-dark.png') })
  await page.keyboard.press('Escape')
  await setup.waitFor({ state: 'detached' })
  assert.deepEqual(logs, [], 'no browser errors')
  console.log('PASS: real archive errors, local conversations, explicit machine start, sidebar words, project dialog, responsive layout, and light/dark rendering')
} catch (error) { await page.screenshot({ path: join(artifacts, 'failure.png') }); console.error(await page.locator('body').innerText()); throw error } finally { await browser.close() }
