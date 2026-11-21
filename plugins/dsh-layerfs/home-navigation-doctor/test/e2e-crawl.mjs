#!/usr/bin/env node
// Read-only crawl of every LayerFS panel, page, tab and dialog on a side server
// (docs/specs/backend-e2e-tests.md §4.5). Dialogs and menus are opened and
// dismissed, never submitted; Build/Reset/Remove/Delete/Stop/Start/Run/Check
// all/Diagnose are never clicked.
//
//   DSH_LAYERFS_DEV_DIR=$PWD/.dev-crawl node test/e2e-crawl.mjs [--only <panel>[,<panel>]] [--keep-going]
//     [--sizes 1280x820,900x800] [--schemes light,dark] [--headed]
//
// Seed the server first (a Store, projects and a fake-llm project session):
//   DSH_LAYERFS_DEV_DIR=$PWD/.dev-crawl node test/e2e-sessions.mjs --keep
//
// Per view: console and page errors, host requests (status >= 500, transport-unknown,
// 4xx not allowed below), horizontal overflow outside a horizontal scroller, unnamed
// controls and dialogs and duplicate ids (CDP accessibility tree). Around the crawl:
// a tools/lfs-check.mjs snapshot before and after; any difference fails (the crawl is
// read only). Screenshots go to <dev dir>/shots/ (for review, not compared).
// Without --keep-going the crawl stops visiting after the first view with a failure.
// A self-test first plants an unnamed button/dialog, a duplicate id and an overflowing
// element and exits 2 if any check misses them. Runtime: ~2 min per size×scheme (80
// views on the e2e-sessions seed), ~9 min for the default 4; narrow with --sizes/--schemes.
// Not covered: the scratch-folder menu (it needs a project-placed new session, i.e. a
// sessions.choose write, which a read-only crawl does not make).
// Panels (for --only): resources, store, machine, sandbox, project, overview, health,
// logs, settings, projects, hero, conversation.
import { fileURLToPath } from 'node:url'
import { mkdirSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { openDev } from '../tools/e2e-lib.mjs'
import { diff as lfsDiff, formatDiff, isWriteCall, snapshot } from '../tools/lfs-check.mjs'

const argv = process.argv.slice(2)
const opt = (name, fallback) => { const i = argv.indexOf(name); return i >= 0 ? argv[i + 1] : fallback }
const only = opt('--only') ? new Set(opt('--only').split(',')) : null
const keepGoing = argv.includes('--keep-going')
const sizes = opt('--sizes', '1280x820,900x800').split(',')
const schemes = opt('--schemes', 'light,dark').split(',')
const devDir = process.env.DSH_LAYERFS_DEV_DIR ?? fileURLToPath(new URL('../.dev', import.meta.url))
const shots = join(devDir, 'shots')
rmSync(shots, { recursive: true, force: true })
mkdirSync(shots, { recursive: true })

// ---------------------------------------------------------------- allow-lists
/** 4xx host answers that are a page's normal state, by `METHOD path` pattern; each with its reason. */
const ALLOW_4XX = [
  // (none yet: add `{ route: /^GET \/api\/…/, status: 404, reason: '…' }`)
]
/** Host operations whose refusal is a normal state, by name and code. */
const ALLOW_OP_ERRORS = [
  // (none yet: add `{ op: 'session.state', code: 'NotFound', reason: '…' }`)
]
/** Non-ok call-log outcomes that are a normal read result (lfs-check). */
const ALLOW_OUTCOMES = [
  // The project catalog learns that a remembered project is gone by asking for it; it then
  // remembers the answer (catalog.ts GONE_MS), so a second ask in one crawl is a failure.
  ...['NoSuchStore', 'NoSuchProject'].map((outcome) => ({ argv: /^project get /, outcome, once: true, reason: 'the project catalog checking a removed project (once per project)' })),
]
/** Overflowing elements that are fine, by descriptor pattern; each with its reason. */
const ALLOW_OVERFLOW = [
  // (none yet)
]
/** Console messages that are not this plugin's errors. */
const ALLOW_CONSOLE = [
  // (none yet)
]

// ---------------------------------------------------------------- browser setup
const [w0, h0] = sizes[0].split('x').map(Number)
const before = await snapshot(devDir)
const { browser, page, logs } = await openDev({ size: `${w0}x${h0}`, headless: !argv.includes('--headed') })
page.setDefaultTimeout(15000)
await page.waitForFunction(() => window.__dshLayerfs, null, { timeout: 30000 })
const cdp = await page.context().newCDPSession(page)
await cdp.send('DOM.enable')
await cdp.send('Accessibility.enable')

/** Wrap window.__dshLayerfs.call and .op (call goes through op's closure, so both are seen once). Idempotent. */
const instrument = () => page.evaluate(() => {
  const rec = window.__crawl ??= { calls: [], inflight: 0, last: 0 }
  const wrap = (api) => {
    if (!api || api.__crawlWrapped) return api
    const track = (entry, run) => {
      rec.calls.push(entry); rec.inflight++; rec.last = performance.now()
      const t0 = performance.now()
      return run().then((value) => { entry.status = 'ok'; return value }, (error) => {
        entry.status = typeof error?.status === 'number' ? error.status : 0
        entry.code = error?.code ?? null
        entry.transport = error?.transport ?? null
        entry.message = String(error?.message ?? error).slice(0, 300)
        throw error
      }).finally(() => { entry.ms = Math.round(performance.now() - t0); rec.inflight--; rec.last = performance.now() })
    }
    const call = api.call, op = api.op
    api.call = (method, path, body) => track({ kind: 'call', method, path: String(path) }, () => call(method, path, body))
    api.op = (name, args) => track({ kind: 'op', name }, () => op(name, args))
    api.__crawlWrapped = true
    return api
  }
  let current = wrap(window.__dshLayerfs)
  if (!Object.getOwnPropertyDescriptor(window, '__dshLayerfs')?.set) {
    Object.defineProperty(window, '__dshLayerfs', { configurable: true, get: () => current, set: (v) => { current = wrap(v) } })
  }
  return rec.calls.length
})
await instrument()

/** Wait until no host request is in flight and none started for 600 ms (at most `max` ms). */
async function quiesce(max = 8000) {
  const end = Date.now() + max
  await page.waitForTimeout(300)
  while (Date.now() < end) {
    const idle = await page.evaluate(() => window.__crawl && window.__crawl.inflight === 0 && performance.now() - window.__crawl.last > 600).catch(() => true)
    if (idle) break
    await page.waitForTimeout(150)
  }
  await page.waitForTimeout(250)
}

// ---------------------------------------------------------------- in-page checks
/** Shared in-page descriptor: a short selector path, and whether this plugin owns the element. */
const DESCRIBE = `(el) => {
  const clean = (c) => c.replace(/^_/, '').replace(/_[a-z0-9]{5}_\\d+$/, '')
  const part = (e) => {
    let s = e.tagName.toLowerCase()
    const role = e.getAttribute('role'); if (role) s += '[role=' + role + ']'
    const cls = [...e.classList].map(clean).filter(Boolean).slice(0, 2); if (cls.length) s += '.' + cls.join('.')
    const label = e.getAttribute('aria-label') || e.getAttribute('title'); if (label) s += '[' + JSON.stringify(label.slice(0, 40)) + ']'
    return s
  }
  const path = []
  for (let e = el, i = 0; e && e !== document.body && i < 4; e = e.parentElement, i++) path.unshift(part(e))
  // Plugin CSS Modules keep readable names (file_local); DSH's are hashed (_local_hash_n).
  let owner = 'dsh'
  for (let e = el; e && e !== document.body; e = e.parentElement) {
    if ([...e.attributes].some((a) => a.name.startsWith('data-layerfs')) || [...e.classList].some((c) => /^[a-z][A-Za-z0-9]*_[A-Za-z][A-Za-z0-9]*$/.test(c))) { owner = 'layerfs'; break }
  }
  const text = (el.innerText || el.textContent || '').trim().replace(/\\s+/g, ' ').slice(0, 60)
  return { selector: path.join(' > '), owner, text }
}`

async function overflowCheck() {
  return page.evaluate((describeSrc) => {
    const describe = eval(describeSrc)
    const W = window.innerWidth
    const scrollsX = (e) => { const o = getComputedStyle(e).overflowX; return o === 'auto' || o === 'scroll' }
    const found = []
    for (const el of document.body.querySelectorAll('*')) {
      const r = el.getBoundingClientRect()
      if (r.width === 0 || r.height === 0 || r.right <= W + 1 || r.left >= W - 1 || r.bottom <= 0 || r.top >= window.innerHeight) continue
      const s = getComputedStyle(el)
      if (s.visibility === 'hidden' || s.display === 'none' || Number(s.opacity) === 0) continue
      // Exempt: inside a horizontal scroller, or clipped (overflow hidden/clip, e.g. an ellipsis)
      // by an ancestor that itself ends inside the viewport.
      let inside = false
      for (let a = el.parentElement; a && a !== document.documentElement; a = a.parentElement) {
        if (scrollsX(a) && a.scrollWidth > a.clientWidth) { inside = true; break }
        const o = getComputedStyle(a).overflowX
        if ((o === 'hidden' || o === 'clip') && a.getBoundingClientRect().right < W - 1) { inside = true; break }
        if (getComputedStyle(a).display === 'none') { inside = true; break }
      }
      if (!inside) found.push(el)
    }
    const outer = found.filter((el) => !found.some((o) => o !== el && o.contains(el)))
    return {
      pageScroll: document.documentElement.scrollWidth > W + 1 ? document.documentElement.scrollWidth : null,
      items: outer.slice(0, 20).map((el) => ({ ...describe(el), right: Math.round(el.getBoundingClientRect().right), width: W })),
    }
  }, DESCRIBE)
}

const NAMED_ROLES = new Set(['button', 'link', 'textbox', 'searchbox', 'combobox', 'menuitem', 'menuitemcheckbox', 'menuitemradio', 'tab', 'checkbox', 'radio', 'switch', 'slider', 'spinbutton', 'option', 'treeitem'])
async function a11yCheck() {
  const { nodes } = await cdp.send('Accessibility.getFullAXTree')
  const bad = []
  for (const n of nodes) {
    if (n.ignored) continue
    const role = n.role?.value
    const name = String(n.name?.value ?? '').trim()
    if (name) continue
    if (NAMED_ROLES.has(role)) bad.push({ n, problem: `${role} without a name` })
    else if (role === 'dialog' || role === 'alertdialog') bad.push({ n, problem: `${role} without a name` })
  }
  const out = []
  for (const { n, problem } of bad.slice(0, 40)) {
    let d = { selector: `(backend node ${n.backendDOMNodeId})`, owner: 'unknown', text: '' }
    try {
      const { object } = await cdp.send('DOM.resolveNode', { backendNodeId: n.backendDOMNodeId })
      const r = await cdp.send('Runtime.callFunctionOn', { objectId: object.objectId, functionDeclaration: `function () { return (${DESCRIBE})(this.nodeType === 1 ? this : this.parentElement) }`, returnByValue: true })
      d = r.result.value ?? d
      await cdp.send('Runtime.releaseObject', { objectId: object.objectId }).catch(() => {})
    } catch {}
    out.push({ problem, ...d })
  }
  const dupIds = await page.evaluate(() => {
    const seen = new Map()
    for (const el of document.querySelectorAll('[id]')) { if (!el.id) continue; seen.set(el.id, (seen.get(el.id) ?? 0) + 1) }
    return [...seen].filter(([, n]) => n > 1).map(([id, n]) => ({ id, n }))
  })
  return { unnamed: out, dupIds }
}

// ---------------------------------------------------------------- issue registry
const issues = new Map() // key → { severity, category, message, views:Set, shot }
let currentView = '', currentShot = ''
function report(severity, category, key, message) {
  const k = `${severity}|${category}|${key}`
  const hit = issues.get(k) ?? { severity, category, message, views: new Set(), shot: currentShot }
  hit.views.add(currentView)
  issues.set(k, hit)
}
const failuresIn = (view) => [...issues.values()].some((i) => i.severity === 'fail' && i.views.has(view))

async function checkView(mark) {
  // Console and page errors.
  for (const line of logs.slice(mark.logs)) {
    const allowed = ALLOW_CONSOLE.find((a) => a.pattern.test(line))
    if (allowed) report('note', 'console', line, `${line} (allowed: ${allowed.reason})`)
    else report('fail', 'console', line.replace(/\d+/g, 'N').slice(0, 200), line.slice(0, 500))
  }
  // Host requests made while the view was shown.
  const calls = await page.evaluate((from) => window.__crawl.calls.slice(from), mark.calls)
  for (const c of calls) {
    if (c.status === 'ok' || c.status === undefined) continue
    if (c.kind === 'call') {
      const route = `${c.method} ${c.path.split('?')[0]}`
      const shape = route.replace(/[0-9a-f]{8}-[0-9a-f-]{27,}/g, ':id')
      const line = `${route} → ${c.status} ${c.code ?? ''}${c.transport ? ` (${c.transport})` : ''}: ${c.message ?? ''}`
      if (c.transport === 'transport-unknown') report('fail', 'host', `${shape}|transport`, line)
      else if (c.status >= 500 || c.status === 0) report('fail', 'host', `${shape}|${c.status}|${c.code}`, line)
      else if (c.status >= 400) {
        const allowed = ALLOW_4XX.find((a) => a.route.test(route) && (a.status === undefined || a.status === c.status))
        if (allowed) report('note', 'host', `${shape}|${c.status}`, `${line} (allowed: ${allowed.reason})`)
        else report('fail', 'host', `${shape}|${c.status}|${c.code}`, line)
      }
    } else {
      const allowed = ALLOW_OP_ERRORS.find((a) => a.op === c.name && (a.code === undefined || a.code === c.code))
      const line = `op ${c.name} → ${c.code ?? c.status}: ${c.message ?? ''}`
      if (allowed) report('note', 'host', `op ${c.name}|${c.code}`, `${line} (allowed: ${allowed.reason})`)
      else report('fail', 'host', `op ${c.name}|${c.code}`, line)
    }
  }
  // Horizontal overflow.
  const of = await overflowCheck()
  if (of.pageScroll) report('fail', 'overflow', `page|${currentView.split(' ')[0]}`, `the page scrolls horizontally (${of.pageScroll}px > viewport)`)
  for (const item of of.items) {
    const allowed = ALLOW_OVERFLOW.find((a) => a.pattern.test(item.selector))
    const msg = `${item.selector} right=${item.right}px > ${item.width}px ${item.text ? `“${item.text}”` : ''}`
    if (allowed) report('note', 'overflow', item.selector, `${msg} (allowed: ${allowed.reason})`)
    else report(item.owner === 'layerfs' ? 'fail' : 'note', item.owner === 'layerfs' ? 'overflow' : 'overflow (DSH-owned)', item.selector, msg)
  }
  // Accessibility.
  const ax = await a11yCheck()
  for (const u of ax.unnamed) {
    const layerfs = u.owner === 'layerfs'
    report(layerfs ? 'fail' : 'note', layerfs ? 'a11y' : 'a11y (DSH-owned)', `${u.problem}|${u.selector}`, `${u.problem}: ${u.selector}${u.text ? ` “${u.text}”` : ''}`)
  }
  for (const d of ax.dupIds) report('fail', 'a11y', `dup-id|${d.id.replace(/:r[0-9a-z]+:|«r[0-9a-z]+»/g, ':r:')}`, `duplicate id "${d.id}" ×${d.n}`)
}

// ---------------------------------------------------------------- navigation helpers
const sleep = (ms) => page.waitForTimeout(ms)
async function escapeAll() {
  for (let i = 0; i < 3; i++) {
    const open = await page.locator('[role=dialog]:visible, [role=menu]:visible, [role=listbox]:visible').count()
    if (!open) return
    await page.keyboard.press('Escape'); await sleep(250)
  }
  // A dialog that ignores Escape: its Cancel/Close button.
  const dialog = page.getByRole('dialog').last()
  if (await dialog.isVisible().catch(() => false)) {
    const close = dialog.getByRole('button', { name: /^(Cancel|Close)$/i })
    if (await close.count()) await close.first().click()
    await sleep(250)
  }
}
const nav = () => page.getByRole('navigation', { name: 'Global panels' })
async function panel(name) {
  await escapeAll()
  await nav().getByRole('button', { name, exact: true }).first().click()
  await sleep(400)
}
const openPanelId = (id) => page.evaluate((i) => window.__dshLayerfs.openPanel(i), id)
/** The Home page's navigation bar (Back/Forward, breadcrumb, Jump to, Refresh, Add). */
const bar = () => page.locator('header[aria-label="Home navigation"]')
async function resources() {
  await escapeAll()
  if (!(await bar().isVisible().catch(() => false))) await panel('LayerFS')
  await bar().waitFor()
}
const esc = (x) => x.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
/** A page with its own button on Home's head card: Health (its label may carry the findings count), Logs, Settings. */
async function centerPage(name) {
  await home()
  await page.getByRole('navigation', { name: 'Health, Logs and Settings' }).getByRole('button', { name: new RegExp(`^${name}\\b`) }).first().click()
  await sleep(400)
}
/** The Overview: the breadcrumb's Home crumb (absent when the Overview already shows). */
async function home() {
  await resources()
  const root = bar().getByRole('navigation', { name: 'Location' }).getByRole('button', { name: 'Home', exact: true })
  if (await root.count()) await root.first().click()
  await sleep(300)
}
/** Open a page through Jump to: the result named `label` (and, when given, living in `where`). */
async function jumpTo(label, where) {
  await resources()
  await bar().getByRole('button', { name: /^Jump to/ }).click()
  const box = page.getByRole('combobox', { name: 'Jump to' })
  await box.fill(where ? `${label} ${where}` : label)
  const option = page.getByRole('listbox', { name: 'Jump to' }).getByRole('option')
    .filter({ has: page.locator('span', { hasText: new RegExp(`^${esc(label)}$`) }) }).first()
  await option.click()
  await sleep(300)
}
/** Open an entry of the bar's Add menu, require its dialog, leave it open. */
async function addMenu(name) {
  await resources()
  await bar().getByRole('button', { name: 'Add', exact: true }).click()
  return openAndDismiss(page.getByRole('menuitem', { name, exact: true }))
}
/** The Overview on one of its sections. */
async function overviewTab(tab) {
  await home()
  await editor().getByRole('tab', { name: new RegExp(`^${tab}\\b`) }).first().click()
  await sleep(300)
}
/** The Home page showing. */
const editor = () => page.locator('[data-storage-page]:visible').first()
/** Open a dialog or menu from `trigger`, require it to appear, then dismiss it. */
async function openAndDismiss(trigger, kind = 'dialog') {
  await trigger.click()
  const shown = page.locator(kind === 'menu' ? '[role=menu]:visible' : '[role=dialog]:visible, [role=alertdialog]:visible').first()
  await shown.waitFor({ timeout: 5000 })
  await sleep(500)
  return shown
}
const SAFE = (name) => !/\b(Build|Reset|Remove|Delete|Stop|Start|Run|Check all|Diagnose|Close)\b/i.test(name)
const btn = (scope, name) => { if (!SAFE(String(name))) throw new Error(`crawler refuses to click ${name}`); return scope.getByRole('button', { name, exact: typeof name === 'string' }).first() }

// ---------------------------------------------------------------- discovered resources
const machines = before.machines ?? []
const stores = Object.keys(before.stores)
const projects = Object.keys(before.projects)
const sandboxes = Object.entries(before.sandboxes).filter(([, s]) => s.state === 'running').map(([name, s]) => ({ name, short: /^dsh-[0-9a-f]{12,}$/.test(name) ? name.slice(4, 11) : name, ...s }))
const sessionProject = projects.find((p) => p.endsWith('/demo')) ?? projects[0]
if (!sessionProject) {
  console.error(`crawl: ${devDir} has no project. Seed it first: DSH_LAYERFS_DEV_DIR=${devDir} node test/e2e-sessions.mjs --keep`)
  process.exit(2)
}

// ---------------------------------------------------------------- views
// Each view: { panel, name, go }. `go` navigates and may open/dismiss a dialog; the
// checks and the screenshot run after it with the view as it is left.
const views = []
const view = (panelName, name, go) => views.push({ panel: panelName, name, go })

view('resources', 'overview', async () => { await home(); await editor().getByRole('tablist', { name: /Projects, Stores/ }).waitFor() })
for (const tab of ['Projects', 'Stores', 'Machines']) {
  view('resources', `overview tab ${tab}`, () => overviewTab(tab))
}
// Stores: SQLite's two views (Stores, Buckets); PostgreSQL has one (its databases under their server)
for (const name of ['Stores', 'Buckets']) {
  view('resources', `overview stores sqlite ${name}`, async () => {
    await overviewTab('Stores')
    await editor().locator('[data-store-kind=sqlite]').first().click()
    await editor().getByRole('tablist', { name: 'Show' }).getByRole('tab', { name: new RegExp(`^${name}\\b`) }).first().click()
    await sleep(300)
  })
}
view('resources', 'overview stores postgresql', async () => {
  await overviewTab('Stores')
  await editor().locator('[data-store-kind=postgresql]').first().click()
  await sleep(300)
})
view('resources', 'overview doctor findings', async () => {
  await home()
  const show = editor().getByRole('button', { name: 'Show findings', exact: true })
  if (await show.count()) await show.first().click()
})
// The bar's Add menu.
view('resources', 'dialog New Store', () => addMenu('New Store'))
view('resources', 'dialog New machine', () => addMenu('New machine'))
view('resources', 'dialog Connect a database', () => addMenu('Connect a database'))
view('resources', 'dialog Add bucket', () => addMenu('Add bucket'))
view('resources', 'machines board', async () => {
  await overviewTab('Machines')
  await editor().locator('[data-machine]').first().waitFor()
})
view('resources', 'jump to', async () => {
  await resources()
  await bar().getByRole('button', { name: /^Jump to/ }).click()
  await page.getByRole('combobox', { name: 'Jump to' }).fill(projects[0]?.split('/')[1] ?? 'x'); await sleep(500)
})

for (const store of stores) {
  view('store', `store ${store}`, async () => { await jumpTo(store); await editor().getByRole('heading', { name: store, exact: true }).waitFor() })
  view('store', `store ${store} actions menu`, async () => { await jumpTo(store); await openAndDismiss(btn(editor(), `${store} actions`), 'menu') })
  view('store', `store ${store} dialog Add project`, async () => { await jumpTo(store); await openAndDismiss(btn(editor(), 'Add project')) })
  // The current crumb is a switcher only when the Store has siblings.
  if (stores.length > 1) view('store', `store ${store} switcher`, async () => { await jumpTo(store); await openAndDismiss(bar().getByRole('button', { name: store, exact: true }), 'menu') })
  view('store', `store ${store} details`, async () => {
    await jumpTo(store)
    const details = editor().getByRole('group', { name: 'Store details' })
    if (await details.count()) await details.locator('summary').first().click()
  })
}

for (const m of machines) {
  const open = () => jumpTo(m.name === 'native' ? 'LayerFS Home' : m.name)
  view('machine', `machine ${m.name}`, async () => { await open(); await editor().getByRole('heading', { level: 1 }).first().waitFor() })
  view('machine', `machine ${m.name} menu`, async () => { await open(); await openAndDismiss(btn(editor(), `More for ${m.name}`), 'menu') })
  view('machine', `machine ${m.name} details`, async () => {
    await open()
    const details = editor().getByRole('group', { name: 'Details' })
    if (await details.count()) await details.locator('summary').first().click()
  })
}

for (const s of sandboxes) {
  view('sandbox', `sandbox ${s.short}`, async () => { await jumpTo(s.name); await editor().getByRole('heading', { name: s.name, exact: true }).waitFor() })
  view('sandbox', `sandbox ${s.short} menu`, async () => { await jumpTo(s.name); await openAndDismiss(btn(editor(), `More for Sandbox ${s.name}`), 'menu') })
}
if (sandboxes.length) {
  view('sandbox', 'dialog New Sandbox', async () => {
    await overviewTab('Machines')
    await openAndDismiss(editor().getByRole('button', { name: 'New Sandbox', exact: true }).first())
  })
}

for (const project of projects) {
  const [store, name] = project.split('/')
  const open = async (tab) => {
    await jumpTo(name, store)
    const views = page.getByRole('tablist', { name: 'Project views' })
    await views.waitFor()
    if (tab) await views.getByRole('tab', { name: tab, exact: true }).click()
    await sleep(300)
  }
  for (const tab of ['Files', 'History', 'Sandboxes', 'Sessions', 'Stats']) view('project', `project ${project} ${tab}`, () => open(tab))
  view('project', `project ${project} Files folder`, async () => {
    await open('Files')
    const files = page.getByRole('tree', { name: 'Files' })
    const folder = files.getByRole('treeitem').filter({ has: page.getByRole('button') }).first()
    if (await folder.count()) await folder.getByRole('button').first().click()
  })
  for (const sub of ['Blame', 'History']) {
    view('project', `project ${project} Files ${sub}`, async () => {
      await open('Files')
      const fileView = page.getByRole('tablist', { name: 'File view' })
      if (await fileView.count()) await fileView.getByRole('tab', { name: sub, exact: true }).click()
    })
  }
  view('project', `project ${project} branch picker`, async () => {
    await open('Files')
    await btn(page.getByRole('complementary', { name: 'Files' }), 'Branch').click()
    await sleep(600)
  })
  view('project', `project ${project} History branches`, async () => {
    await open('History')
    const branches = page.getByRole('button', { name: /^Branches \d+$/ })
    if (await branches.count()) await branches.first().click()
    await sleep(500)
  })
  view('project', `project ${project} actions menu`, async () => { await open(null); await openAndDismiss(btn(page, `${name} actions`), 'menu') })
}

view('health', 'Health', async () => { await centerPage('Health'); await page.getByRole('region', { name: 'Health' }).first().waitFor() })
view('logs', 'Logs', async () => {
  await centerPage('Logs'); await page.getByRole('heading', { name: 'Logs', level: 1 }).waitFor()
  // Content check: with "Writes only" the table lists no read call (tools/lfs-check.mjs isWriteCall).
  const writes = page.getByRole('checkbox', { name: 'Writes only' })
  if (!(await writes.isChecked())) await writes.click()
  await quiesce(4000)
  const commands = await page.locator('table tr code').allInnerTexts()
  for (const text of commands) {
    const words = text.trim().split(/\s+/)
    const argv = words[0] === 'layerfs' ? words.slice(1) : words
    if (!isWriteCall(argv)) report('fail', 'content', `logs-writes-only|${argv.slice(0, 2).join(' ')}`, `Logs "Writes only" lists a read call: layerfs ${argv.slice(0, 2).join(' ')}`)
  }
})
view('logs', 'Logs reads', async () => {
  await centerPage('Logs')
  const reads = page.getByRole('button', { name: 'Show reads', exact: true })
  if (await reads.count()) await reads.click()
  else { const w = page.getByRole('checkbox', { name: 'Writes only' }); if (await w.isChecked()) await w.click() }
})
view('logs', 'Logs Refused', async () => { await centerPage('Logs'); await page.getByRole('tablist', { name: 'Outcome' }).getByRole('tab', { name: 'Refused' }).click() })
view('logs', 'Logs operation menu', async () => { await centerPage('Logs'); await page.getByRole('tablist', { name: 'Outcome' }).getByRole('tab', { name: 'All' }).click(); await openAndDismiss(btn(page, 'Operation'), 'menu') })
view('settings', 'Settings opens the setup modal', async () => { await centerPage('Settings'); await page.locator('[data-setup-modal]').waitFor(); await page.keyboard.press('Escape'); await page.locator('[data-setup-modal]').waitFor({ state: 'detached' }) })
view('settings', 'Setup modal runtime picker', async () => { await centerPage('Settings'); const m = page.locator('[data-setup-modal]'); await m.waitFor(); await m.getByRole('button', { name: 'Change…' }).first().click(); await m.getByRole('button', { name: 'Other build…' }).waitFor(); await page.keyboard.press('Escape') })

// The sidebar's LayerFS projects view and a project's Set up dialog (v3: no Projects page).
/** The full sidebar (Resources folds it to its rail on purpose: src/client/storage/index.tsx). */
const expandSidebar = async () => {
  await escapeAll()
  const open = page.getByRole('button', { name: 'Open sidebar', exact: true })
  if (await open.isVisible().catch(() => false)) { await open.click(); await sleep(400) }
}
const sidebarProjects = async () => {
  await expandSidebar()
  const show = page.getByRole('button', { name: 'Show LayerFS projects', exact: true })
  if (await show.count()) { await show.first().click(); await sleep(400) }
  await page.locator('div[role=region][aria-label="LayerFS Projects"]').first().waitFor()
}
// A row is labelled by the project's name, plus its Store when two projects share a name.
const rowName = (project) => new RegExp(`^Toggle conversations in ${project.split('/')[1]}( · ${project.split('/')[0]})?$`)
const sidebarRow = () => page.locator('[data-slot="sidebar.workspaces"] li[data-layerfs-project]').filter({ has: page.getByRole('button', { name: rowName(sessionProject) }) }).first()
view('projects', 'sidebar projects', async () => { await sidebarProjects(); await page.getByRole('button', { name: rowName(sessionProject) }).first().click() })
view('projects', 'sidebar project setup dialog', async () => {
  await sidebarProjects()
  const row = sidebarRow()
  await row.hover()
  await openAndDismiss(row.getByRole('button', { name: /^Set up / }).first())
})
view('projects', 'sidebar new menu', async () => { await sidebarProjects(); await openAndDismiss(btn(page, 'New session or project'), 'menu') })
view('projects', 'stock workspaces again', async () => {
  await expandSidebar()
  const back = page.getByRole('button', { name: 'Show DSH workspaces', exact: true })
  if (await back.count()) await back.first().click()
  await page.getByRole('button', { name: 'Show LayerFS projects', exact: true }).first().waitFor()
})

// The New Session row: project menu and its dialogs.
const heroChip = () => page.locator('[data-layerfs-hero] button[aria-label^="LayerFS project:"]').first()
const newSession = async () => {
  await escapeAll()
  await page.getByRole('button', { name: 'New session', exact: true }).first().click()
  await heroChip().waitFor()
  await sleep(300)
}
view('hero', 'New Session project menu', async () => { await newSession(); await openAndDismiss(heroChip(), 'menu') })
for (const item of ['New project…', 'Import a folder…']) {
  view('hero', `New Session dialog ${item}`, async () => {
    await newSession(); await openAndDismiss(heroChip(), 'menu')
    await page.getByRole('menuitem', { name: item, exact: true }).click()
    await page.getByRole('dialog', { name: item.replace('…', '') }).or(page.getByRole('dialog')).first().waitFor({ timeout: 8000 })
    await sleep(800)
  })
}

// One project session: Chat/Trajectory/Files/History/Sandbox.
const openSession = async (tab) => {
  await escapeAll()
  const current = await page.getByRole('tab', { name: 'Trajectory', exact: true }).count()
  if (!current) {
    const [store, name] = sessionProject.split('/')
    await jumpTo(name, store)
    await page.getByRole('tablist', { name: 'Project views' }).getByRole('tab', { name: 'Sessions', exact: true }).click()
    const rows = page.getByRole('region', { name: 'Sessions' }).getByRole('row').filter({ has: page.getByRole('button', { name: 'Open', exact: true }) })
    await rows.first().waitFor()
    // The session with the most checkpoints.
    const counts = await rows.evaluateAll((trs) => trs.map((tr) => Number(tr.cells?.[2]?.innerText) || 0))
    const best = counts.indexOf(Math.max(...counts))
    await rows.nth(best).getByRole('button', { name: 'Open', exact: true }).click()
    await page.getByRole('tab', { name: 'Trajectory', exact: true }).first().waitFor({ timeout: 20000 })
  }
  await page.getByRole('tab', { name: tab, exact: true }).first().click()
  await sleep(400)
}
for (const tab of ['Chat', 'Trajectory', 'Files', 'History', 'Sandbox']) view('conversation', `session ${tab}`, () => openSession(tab))
view('conversation', 'session back to Chat', () => openSession('Chat'))

// ---------------------------------------------------------------- self-test of the checks
// Plant an unnamed plugin-owned button, an unnamed dialog, a duplicate id and an element past the
// right edge; every check must see them, so a clean crawl means clean pages, not blind checks.
{
  await page.evaluate(() => {
    const box = document.createElement('div')
    box.id = 'crawl-selftest'; box.className = 'selftest_probe'
    box.innerHTML = '<button type="button"></button><div role="dialog"></div><i id="crawl-dup">a</i><i id="crawl-dup">b</i><div style="position:absolute;left:10px;top:10px;width:5000px;height:4px;background:transparent"></div>'
    document.body.appendChild(box)
  })
  const of = await overflowCheck()
  const ax = await a11yCheck()
  await page.evaluate(() => document.getElementById('crawl-selftest')?.remove())
  const missing = []
  if (!of.items.some((i) => i.selector.includes('selftest_probe') && i.owner === 'layerfs')) missing.push('overflow')
  if (!ax.unnamed.some((u) => u.problem === 'button without a name' && u.owner === 'layerfs')) missing.push('unnamed button')
  if (!ax.unnamed.some((u) => u.problem === 'dialog without a name')) missing.push('unnamed dialog')
  if (!ax.dupIds.some((d) => d.id === 'crawl-dup')) missing.push('duplicate id')
  if (missing.length) { console.error(`crawl: self-test failed, checks blind to: ${missing.join(', ')}`); await browser.close(); process.exit(2) }
  console.log('crawl: self-test ok (overflow, unnamed button/dialog, duplicate id detected)')
}

// ---------------------------------------------------------------- run
const selected = views.filter((v) => !only || only.has(v.panel))
if (!selected.length) { console.error(`no views for --only ${[...only].join(',')}`); process.exit(2) }
console.log(`crawl: ${selected.length} views × ${sizes.length} sizes × ${schemes.length} schemes on ${devDir}`)
const started = Date.now()
let stopped = false
outer: for (const size of sizes) {
  const [w, h] = size.split('x').map(Number)
  await page.setViewportSize({ width: w, height: h })
  for (const scheme of schemes) {
    await page.emulateMedia({ colorScheme: scheme })
    for (const v of selected) {
      currentView = `${v.name} @${size} ${scheme}`
      const file = `${size}-${scheme}-${v.panel}-${v.name}`.replace(/[^A-Za-z0-9._-]+/g, '_').slice(0, 150) + '.png'
      currentShot = join(shots, file)
      await instrument()
      const mark = { logs: logs.length, calls: await page.evaluate(() => window.__crawl.calls.length) }
      try {
        await v.go()
      } catch (e) {
        report('fail', 'crawler', `${v.panel}|${v.name}|${String(e.message).split('\n')[0].slice(0, 120)}`, `could not reach ${v.name}: ${String(e.message).split('\n')[0]}`)
      }
      await quiesce()
      await page.screenshot({ path: currentShot }).catch(() => {})
      await checkView(mark).catch((e) => report('fail', 'crawler', `check|${e.message}`, `checks failed on ${v.name}: ${e.message}`))
      await escapeAll().catch(() => {})
      process.stdout.write(failuresIn(currentView) ? 'F' : '.')
      if (!keepGoing && failuresIn(currentView)) { stopped = true; break outer }
    }
  }
}
console.log(`\ncrawl: visited in ${Math.round((Date.now() - started) / 1000)} s${stopped ? ' (stopped at the first failing view; --keep-going visits all)' : ''}`)
await escapeAll().catch(() => {})
await browser.close()

// ---------------------------------------------------------------- lfs-check
await new Promise((r) => setTimeout(r, 1500))
const after = await snapshot(devDir)
/** `once` outcomes: how often each command was answered so far. */
const seenOutcomes = new Map()
const delta = lfsDiff(before, after, {
  allowOutcomes: (line) => {
    const command = (line.argv ?? []).join(' ')
    const allowed = ALLOW_OUTCOMES.find((a) => a.outcome === line.outcome && a.argv.test(command))
    if (!allowed) return null
    const n = (seenOutcomes.get(command) ?? 0) + 1
    seenOutcomes.set(command, n)
    return allowed.once && n > 1 ? null : allowed.reason
  },
})
currentView = 'lfs-check'; currentShot = ''
for (const f of delta.failures) report('fail', `lfs-check ${f.kind}`, f.message.replace(/\d+/g, 'N'), f.message)
for (const n of delta.notes) report('note', `lfs-check ${n.kind}`, n.message.replace(/\d+/g, 'N'), n.message)

// ---------------------------------------------------------------- report
const all = [...issues.values()]
const byCategory = new Map()
for (const i of all) { const list = byCategory.get(i.category) ?? []; list.push(i); byCategory.set(i.category, list) }
console.log(`\n${formatDiff(delta)}\n`)
for (const severity of ['fail', 'note']) {
  const cats = [...byCategory].filter(([, list]) => list.some((i) => i.severity === severity))
  if (!cats.length) continue
  console.log(severity === 'fail' ? '==== FAILURES' : '==== NOTES (not failures)')
  for (const [cat, list] of cats) {
    const items = list.filter((i) => i.severity === severity)
    console.log(`-- ${cat} (${items.length})`)
    for (const i of items) {
      const views = [...i.views]
      console.log(`  • ${i.message}`)
      console.log(`      in ${views.length} view(s): ${views.slice(0, 3).join('; ')}${views.length > 3 ? '; …' : ''}`)
      if (i.shot) console.log(`      shot ${i.shot}`)
    }
  }
}
const failures = all.filter((i) => i.severity === 'fail').length
console.log(`\ncrawl: ${failures ? `FAIL (${failures} issue(s))` : 'PASS'} · ${all.length - failures} note(s) · screenshots in ${shots}`)
process.exit(failures ? 1 : 0)
