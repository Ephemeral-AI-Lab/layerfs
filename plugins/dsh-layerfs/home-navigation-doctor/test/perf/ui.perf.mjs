#!/usr/bin/env node
// Per-page CLI spawn budgets on a side server (docs/specs/backend-e2e-tests.md §4.3).
//
// Start the side server with the count shim in a FRESH dev dir (a CLI pinned in
// Settings → runtime.json would win over DSH_LAYERFS_CLI; src/host/service.ts):
//
//   R=~/.layerfs/runtime/<id>          # the pinned runtime (read only)
//   DSH_LAYERFS_DEV_DIR=$PWD/.dev-perf \
//   DSH_LAYERFS_DEV_OVERLAY=<fake-llm overlay naming this checkout's dev/fake-llm/index.js> \
//   LFS_REAL_CLI=$R/bin/layerfs LFS_COUNT_FILE=$PWD/.dev-perf/spawns.log \
//   DSH_LAYERFS_CLI=$PWD/test/fixtures/cli-count.sh \
//   DSH_LAYERFS_DAEMON=$R/bin/layerfs-daemon DSH_LAYERFS_BUNDLE=$R/bundle/linux-aarch64 \
//   node scripts/dev.mjs --port 3095
//
// The run takes ~14 min and serves the linked checkout's lib/; if others rebuild
// it meanwhile, the run fails as not comparable. To measure a fixed build, serve
// a frozen APFS clone instead (relink the private profile once, then start dev.mjs
// from the clone):
//   S=$PWD/.dev-perf-snap; mkdir -p $S; for f in lib package.json cordis.patch.yml README.md node_modules scripts dev; do cp -cR $f $S/; done
//   DSH_HOME=$PWD/.dev-perf/state node $S/node_modules/@deepseek-ai/dsh/lib/bin.js plugin --profile web add link:$S
//   ... node $S/scripts/dev.mjs --port 3095   (same env as above)
//
// then seed and measure:
//
//   DSH_LAYERFS_DEV_DIR=$PWD/.dev-perf LFS_REAL_CLI=$R/bin/layerfs node tools/perf-fixture.mjs --tier S --server
//   DSH_LAYERFS_DEV_DIR=$PWD/.dev-perf node test/perf/ui.perf.mjs [--tier S] [--update-baseline]
//       [--only id,id] [--cooldown 31000] [--learn 25000]
//
// Each page is measured cold and alone: wait `--cooldown` (longer than every
// host read cache: platforms 30 s, Store join 15 s, project storage 15 s) with
// no page open, boot a fresh browser context, run the page's unmeasured
// pre-steps, then measure one action (perf-lib actionWindow: spawns by
// `family.call`, discovery apart, host requests, time to first content).
// Page pollers are frozen with page.clock; the host's own periodic `store list`
// (bound project session) is learned while idle and subtracted (tools/perf-lib.mjs).
// Before measuring, each project's storage ledger is built (unmeasured) so the
// Stats tab measures its steady read, not the one-time per-branch build.
//
// Spawn totals per page are hard budgets against test/perf/baseline.json
// (exit 1 on any page above its baseline or a window that never settled);
// latency is recorded only. Results: <dev dir>/results/<git-short-sha>-<tier>.json.
import { fileURLToPath } from 'node:url'
import { chromium } from 'playwright-core'
import { execSync } from 'node:child_process'
import { existsSync, mkdirSync, readFileSync, statSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { CHROME } from '../../tools/e2e-lib.mjs'
import { devUrl } from '../../tools/ui.mjs'
import { actionWindow, bootPage, countFile, groupHostCalls, groupSpawns, instrument, learnBackground, quiesce, spawnOffset, spawns } from '../../tools/perf-lib.mjs'

const root = fileURLToPath(new URL('../..', import.meta.url))
const args = process.argv.slice(2)
const flag = (name, fallback) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : fallback }
const tier = flag('--tier', 'S')
const cooldown = Number(flag('--cooldown', 31000))
const learnMs = Number(flag('--learn', 25000))
const only = flag('--only', null)?.split(',')
const updateBaseline = args.includes('--update-baseline')
const dev = process.env.DSH_LAYERFS_DEV_DIR ?? join(root, '.dev')
const file = countFile(dev)
const fx = JSON.parse(readFileSync(join(dev, `fixture-${tier}.json`), 'utf8'))
const url = devUrl()
const sleep = (ms) => new Promise((r) => setTimeout(r, ms))
if (!existsSync(file)) { console.error(`ui.perf: no count file ${file}: is the side server running with the count shim?`); process.exit(2) }

const store = fx.stores[0]
const project = fx.session.project ?? fx.projects[0]
const projectName = project.split('/')[1]
const sandboxShort = (fx.session.sandbox ?? '').replace(/^dsh-/, '').slice(0, 7)

// ---------------------------------------------------------------- pages
const openPanel = (id) => async (page) => { await page.evaluate((i) => window.__dshLayerfs.openPanel(i), id) }
const click = (locate) => async (page) => { await locate(page).click({ timeout: 10000 }) }
const visible = (locate) => async (page) => { await locate(page).waitFor({ state: 'visible', timeout: 15000 }) }
const selected = (name) => visible((p) => p.locator(`[role=tab][aria-selected=true]`).filter({ hasText: name }).first())
const tab = (name) => (p) => p.getByRole('tab', { name, exact: true }).first()
const tabRe = (re) => (p) => p.getByRole('tab', { name: re }).first()
const tree = (re) => (p) => p.getByRole('treeitem', { name: re }).first()
// The project's card on the Resources overview: the nearest ancestor of its name holding exactly one Open button.
const openProject = (p) => p.getByText(projectName, { exact: true }).locator('xpath=ancestor::*[count(.//button[normalize-space()="Open"])=1][1]').getByRole('button', { name: 'Open', exact: true }).first()
const resources = openPanel('layerfs-storage')
const resourcesReady = visible(tree(/^Overview/))
const toProject = [resources, click(openProject)]
const toSession = [async (page) => { await page.evaluate((sid) => window.__dshLayerfs.openSession(sid), fx.session.id) }]

const PAGES = [
  { id: 'resources.projects', title: 'Resources overview (Projects)', action: resources, ready: visible(openProject) },
  { id: 'resources.stores', title: 'Resources overview: Stores tab', pre: [resources], action: click(tabRe(/^Stores\s*\d/)), ready: selected('Stores') },
  { id: 'resources.machines', title: 'Resources overview: Machines tab', pre: [resources], action: click(tabRe(/^Machines\s*\d/)), ready: selected('Machines') },
  // Stores › SQLite › Buckets (once the Backups tab; the id is kept so baselines still compare)
  { id: 'resources.backups', title: 'Resources overview: Stores › Buckets', pre: [resources, click(tabRe(/^Stores\s*\d/)), click((p) => p.locator('[data-store-kind=sqlite]').first())], action: click(tabRe(/^Buckets\s*\d/)), ready: selected('Buckets') },
  { id: 'store', title: `Store page (${store})`, pre: [resources], action: click(tree(new RegExp(`^${store}`))), ready: selected(store) },
  { id: 'machine.native', title: 'Native machine page', pre: [resources], action: click(tree(/^LayerFS Home/)), ready: selected('LayerFS Home') },
  ...(sandboxShort ? [{ id: 'sandbox', title: `Sandbox page (${sandboxShort})`, pre: [resources], action: click(tree(new RegExp(`^${sandboxShort}`))), ready: selected(sandboxShort) }] : []),
  { id: 'project.files', title: `Project ${projectName}: Files`, pre: [resources], action: click(openProject), ready: visible((p) => p.getByRole('tab', { name: 'Files', exact: true, selected: true })) },
  ...['History', 'Sandboxes', 'Sessions', 'Stats'].map((name) => ({ id: `project.${name.toLowerCase()}`, title: `Project ${projectName}: ${name}`, pre: toProject, action: click(tab(name)), ready: visible((p) => p.getByRole('tab', { name, exact: true, selected: true }).first()) })),
  // Health and Logs are pages behind the buttons on Home's head card (Settings opens the setup modal).
  ...['Health', 'Logs'].map((name) => ({ id: name.toLowerCase(), title: name, pre: [resources], action: click((p) => p.getByRole('navigation', { name: 'Health, Logs and Settings' }).getByRole('button', { name: new RegExp(`^${name}\\b`) }).first()) })),
  { id: 'session.chat', title: 'Project session: open (Chat)', action: toSession[0], ready: visible((p) => p.getByRole('tab', { name: 'Trajectory', exact: true })) },
  ...['Trajectory', 'Files', 'History', 'Sandbox'].map((name) => ({ id: `session.${name.toLowerCase()}`, title: `Project session: ${name}`, pre: toSession, action: click(tab(name)), ready: visible((p) => p.getByRole('tab', { name, exact: true, selected: true }).first()) })),
].filter((p) => !only || only.includes(p.id))

// ---------------------------------------------------------------- run
const browser = await chromium.launch({ executablePath: CHROME, headless: true })
const started = Date.now()
// The side server serves the lib/ of the checkout its private profile links
// (scripts/dev.mjs: this one, or a frozen clone). A rebuild mid-run hot-reloads
// the client and leaves the host stale until a restart: the run is not comparable.
const served = (() => {
  try { return JSON.parse(readFileSync(join(dev, 'state', 'profiles', 'web', 'package.json'), 'utf8')).dependencies['dsh-layerfs'].replace(/^link:/, '') } catch { return root }
})()
const libStamp = () => ['client.js', 'index.js'].map((f) => { try { return statSync(join(served, 'lib', f)).mtimeMs } catch { return 0 } }).join(',')
const libAtStart = libStamp()
const idle = async (ms) => { const at = spawnOffset(file); await sleep(ms); return spawns(file, at) }
// Host-side periodic spawns, learned with no page open.
let background = learnBackground(await idle(learnMs))
console.log(`ui.perf: tier ${tier}, ${PAGES.length} pages, cooldown ${cooldown} ms; host background: ${background.map((b) => `${b.argv} every ${b.period} ms`).join(', ') || 'none'}`)

// Steady state first: each project's storage ledger (the Stats tab) is built
// once per new Commit (one `branch commits --include-storage` per branch, then
// persisted). Build it now, unmeasured, so Stats measures the steady read.
{
  const context = await browser.newContext()
  const page = await context.newPage()
  await instrument(page)
  await bootPage(page, url, { file, background, clock: false })
  for (const key of fx.projects) {
    const path = `/api/projects/${encodeURIComponent(key)}/storage`
    for (let i = 0; i < 120; i++) {
      const view = await page.evaluate((p) => window.__dshLayerfs.call('GET', p), path)
      if (!view?.building) break
      await sleep(500)
    }
  }
  await context.close()
}

const results = {}
const measure = async (def) => {
  const context = await browser.newContext({ viewport: { width: 1280, height: 820 } })
  const page = await context.newPage()
  const errors = []
  page.on('pageerror', (e) => errors.push(e.message))
  page.on('console', (m) => { if (m.type() === 'error') errors.push(m.text()) })
  try {
    await instrument(page)
    await bootPage(page, url, { file, background })
    for (const step of def.pre ?? []) {
      await step(page)
      await quiesce(page, { file, offset: spawnOffset(file), background })
    }
    const r = await actionWindow(page, def.action, { file, background, ready: def.ready })
    const g = groupSpawns(r.spawns)
    return {
      title: def.title, spawns: g.total, byCall: g.byCall, discovery: r.discovery, background: r.background.length,
      hostCalls: r.hostCalls.length, hostByName: groupHostCalls(r.hostCalls), failedHostCalls: r.hostCalls.filter((c) => c.ok === false),
      ttfcMs: r.ttfcMs, windowMs: r.ms, settled: r.settled, longtasks: r.longtasks, argv: r.spawns.map((s) => s.argv), errors,
    }
  } catch (error) {
    return { title: def.title, error: String(error?.message ?? error).split('\n')[0], errors }
  } finally {
    await context.close()
  }
}
const cool = async () => {
  if (!cooldown) return
  const learned = learnBackground(await idle(cooldown))
  for (const b of learned) background = [...background.filter((x) => x.argv !== b.argv), b]
}
for (const def of PAGES) {
  await cool()
  let x = await measure(def)
  // A UI step that failed (a click or wait timing out) is retried once, cold again;
  // counts are never retried.
  if (x.error) { console.log(`  ${def.id}: ${x.error}; retrying once`); await cool(); x = { ...(await measure(def)), retriedAfter: x.error } }
  results[def.id] = x
  console.log(x.error ? `  ${def.id}: ERROR ${x.error}` : `  ${def.id}: ${x.spawns} spawns (${x.discovery} discovery, ${x.background} background), ${x.hostCalls} host calls, ttfc ${x.ttfcMs ?? '-'} ms`)
}
await browser.close()

// ---------------------------------------------------------------- report
const sha = (() => { try { return execSync('git rev-parse --short HEAD', { cwd: root }).toString().trim() } catch { return 'nogit' } })()
const libChanged = libStamp() !== libAtStart
const out = { libChanged, served, sha, dirty: (() => { try { return execSync('git status --porcelain', { cwd: root }).toString().trim().length > 0 } catch { return null } })(), tier, at: new Date().toISOString(), seconds: Math.round((Date.now() - started) / 1000), cooldown, background, fixture: { stores: fx.stores, projects: fx.projects, commits: fx.commits, cliCommit: fx.cliCommit }, pages: results }
mkdirSync(join(dev, 'results'), { recursive: true })
const resultPath = join(dev, 'results', `${sha}-${tier}.json`)
writeFileSync(resultPath, `${JSON.stringify(out, null, 2)}\n`)

const baselinePath = join(root, 'test', 'perf', 'baseline.json')
const baseline = existsSync(baselinePath) ? JSON.parse(readFileSync(baselinePath, 'utf8')) : {}
const base = baseline[tier]?.pages ?? {}
const pad = (s, n) => String(s).padEnd(n)
const lpad = (s, n) => String(s).padStart(n)
console.log(`\n${pad('page', 22)}${lpad('spawns', 7)}${lpad('base', 6)}${lpad('disc', 5)}${lpad('bg', 4)}${lpad('host', 6)}${lpad('ttfc', 7)}  calls`)
const failures = []
if (libChanged) failures.push('lib/ was rebuilt during the run (client hot-reloaded; host stale until restart): rerun after restarting the side server')
for (const [id, r] of Object.entries(results)) {
  if (r.error) { failures.push(`${id}: ${r.error}`); console.log(`${pad(id, 22)} ERROR ${r.error}`); continue }
  const b = base[id]?.spawns
  if (!r.settled) failures.push(`${id}: the window never settled`)
  if (b !== undefined && r.spawns > b) failures.push(`${id}: ${r.spawns} spawns > budget ${b} (${JSON.stringify(r.byCall)} vs ${JSON.stringify(base[id].byCall)})`)
  const calls = Object.entries(r.byCall).map(([k, n]) => n > 1 ? `${k}×${n}` : k).join(' ')
  console.log(`${pad(id, 22)}${lpad(r.spawns, 7)}${lpad(b ?? '-', 6)}${lpad(r.discovery, 5)}${lpad(r.background, 4)}${lpad(r.hostCalls, 6)}${lpad(r.ttfcMs ?? '-', 7)}  ${calls}`)
}
const unbudgeted = Object.keys(results).filter((id) => !results[id].error && base[id] === undefined)
if (unbudgeted.length && !updateBaseline) console.log(`\nno baseline yet for: ${unbudgeted.join(', ')} (run with --update-baseline)`)
if (updateBaseline) {
  // A full run replaces the tier's pages (dropping removed ones); --only updates just those.
  const pages = only ? { ...base } : {}
  for (const [id, r] of Object.entries(results)) if (!r.error && r.settled) pages[id] = { spawns: r.spawns, byCall: r.byCall, ...(base[id]?.note ? { note: base[id].note } : {}) }
  baseline[tier] = { sha, at: out.at, fixture: out.fixture, pages }
  writeFileSync(baselinePath, `${JSON.stringify(baseline, null, 2)}\n`)
  console.log(`\nbaseline written: ${baselinePath}`)
}
console.log(`\nresults: ${resultPath} (${out.seconds} s)`)
if (failures.length) { console.log(`\nFAIL:\n  ${failures.join('\n  ')}`); process.exit(updateBaseline ? 0 : 1) }
console.log('PASS')
