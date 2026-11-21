#!/usr/bin/env node
// Seed a perf data tier (docs/specs/backend-e2e-tests.md §3.3). Native only: SQLite
// Stores on the native machine, no Docker or Lima. Two modes:
//
//   node tools/perf-fixture.mjs --tier S|M [--dir <abs dir>] [--force]
//     In process, no server: the host routes from src (test/routes-helper.mjs, as
//     test/routes.test.mjs does) over the real pinned CLI on a private home
//     `<dir>/home` (default dir `.dev-perf-<tier>`). Manifest `<dir>/fixture.json`.
//     Used by test/perf/host.perf.mjs.
//
//   DSH_LAYERFS_DEV_DIR=$PWD/.dev-perf node tools/perf-fixture.mjs --tier S --server [--force]
//     Through a running side server's host API, plus one fake-llm project session
//     with two checkpointed turns (a bound Sandbox). Manifest
//     `<dev dir>/fixture-<tier>.json`. Used by test/perf/ui.perf.mjs.
//
// Tiers: S = 1 Store, 2 projects (`website`, `billing`): their recipe source trees
// and base history (35 plain CLI steps each: branch fork / file write / merge /
// publish, ~10 branches). M = 3 Stores, 12 projects (the 8 recipe profiles, 4 again
// as `<profile>-2`), each with ~150 generated extra files and 30 extra branches of 4
// Commits (~41 branches, ~146 Commits per project; ~1.75k Commits, ~2k files). Stores are seeded in parallel
// (one CLI lane per Store). Stores and projects are created through the host API;
// history goes straight to the pinned CLI (LFS_REAL_CLI, else the runtime pin).
// A manifest with the same fixture version and CLI commit is reused unless --force.
import { spawn, spawnSync } from 'node:child_process'
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { runtimePin } from '../scripts/runtime-pin.mjs'
import { baseProjectHistorySteps, stepEffect } from '../src/host/presets/recipe/recipe.js'
import { sourceFiles } from '../src/host/presets/recipe/presets.js'

export const FIXTURE_VERSION = 4
const root = fileURLToPath(new URL('..', import.meta.url))
const PROFILES = ['website', 'billing', 'embeddings', 'evaluation', 'compiler', 'sdk', 'design', 'gateway']
export const TIERS = {
  S: { stores: 1, projects: 2, extraFiles: 0, extraBranches: 0, writesPerBranch: 0 },
  M: { stores: 3, projects: 12, extraFiles: 150, extraBranches: 30, writesPerBranch: 4 },
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms))

/** The real CLI the fixture writes history with. */
export function realCli() {
  const cli = process.env.LFS_REAL_CLI ?? runtimePin()?.cli
  if (!cli) throw new Error('perf-fixture: no CLI (set LFS_REAL_CLI)')
  return cli
}

/** Run one CLI call on `home`; JSON stdout. */
function lfsAsync(cli, home, argv, input) {
  return new Promise((resolve, reject) => {
    const child = spawn(cli, argv, { env: { ...process.env, LAYERFS_HOME: home }, stdio: ['pipe', 'pipe', 'pipe'] })
    let out = '', err = ''
    child.stdout.on('data', (d) => { out += d })
    child.stderr.on('data', (d) => { err += d })
    child.on('error', reject)
    child.on('close', (code) => {
      let json = null
      try { json = JSON.parse(out) } catch { /* below */ }
      if (code !== 0 || !json) reject(new Error(`layerfs ${argv.join(' ')}: exit ${code} ${err || out}`.slice(0, 2000)))
      else resolve(json)
    })
    child.stdin.end(input ?? '')
  })
}

/** The projects of a tier: `[{ store, name, profile }]`, and the Store names. */
export function tierPlan(tier, tag) {
  const plan = TIERS[tier]
  if (!plan) throw new Error(`perf-fixture: unknown tier ${tier} (S or M)`)
  const stores = Array.from({ length: plan.stores }, (_, i) => `perf${tier.toLowerCase()}${tag}${plan.stores > 1 ? i + 1 : ''}`)
  const projects = Array.from({ length: plan.projects }, (_, i) => {
    const profile = PROFILES[i % PROFILES.length]
    const name = i < PROFILES.length ? profile : `${profile}-${Math.floor(i / PROFILES.length) + 1}`
    return { store: stores[i % stores.length], name, profile }
  })
  return { plan, stores, projects }
}

/** Source tree of one project: the recipe files plus generated ones (M). */
function writeSource(dir, p, plan) {
  const files = { ...sourceFiles(p.profile) }
  for (let i = 0; i < plan.extraFiles; i++) files[`generated/part-${String(i % 15).padStart(2, '0')}/note-${String(i).padStart(3, '0')}.md`] = `# Note ${i} of ${p.name}\n\nGenerated perf fixture file ${i}.\n`
  for (const [path, content] of Object.entries(files)) {
    mkdirSync(dirname(join(dir, path)), { recursive: true })
    writeFileSync(join(dir, path), content)
  }
  return Object.keys(files).length
}

/** History argv steps of one project: the recipe's base history plus extra branches (M). */
function historySteps(p, plan) {
  const key = `${p.store}/${p.name}`
  const steps = baseProjectHistorySteps(p.store, p.profile).map((s) => ({ argv: s.argv.map((a) => a === `${p.store}/${p.profile}` ? key : a), input: s.input }))
  for (let b = 0; b < plan.extraBranches; b++) {
    const branch = `perf-${String(b).padStart(2, '0')}`
    steps.push({ argv: ['branch', 'fork', key, branch, 'main'] })
    // `file write` needs an existing folder: the generated ones (extraFiles > 0 whenever extraBranches is).
    for (let w = 0; w < plan.writesPerBranch; w++) steps.push({ argv: ['file', 'write', key, branch, `/generated/part-${String(b % 15).padStart(2, '0')}/${branch}-change-${w}.md`, '--message', `${branch}: change ${w}`], input: `# ${branch} change ${w}\n` })
  }
  return steps
}

/**
 * Seed a tier through `http(method, path, body)` (the host API) and the CLI.
 * @returns {{ stores, projects, steps, commits, files }}
 */
export async function seed({ tier, http, cli, home, fixtures, log = console.log }) {
  const tag = Date.now().toString(36).slice(-5)
  const { plan, stores, projects } = tierPlan(tier, tag)
  mkdirSync(fixtures, { recursive: true })
  const until = async (what, fn, ms = 300000) => {
    const end = Date.now() + ms
    for (;;) { const v = await fn(); if (v) return v; if (Date.now() > end) throw new Error(`timed out: ${what}`); await sleep(300) }
  }
  for (const store of stores) { log(`store ${store}`); await http('POST', '/api/stores', { name: store }) }
  let steps = 0, commits = 0, files = 0
  // One lane per Store: concurrent CLI processes on one Store are not safe outside the host's lane.
  await Promise.all(stores.map(async (store) => {
    for (const p of projects.filter((x) => x.store === store)) {
      const key = `${p.store}/${p.name}`
      const src = mkdtempSync(join(fixtures, `${p.name}-`))
      files += writeSource(src, p, plan)
      const init = await http('POST', '/api/projects', { store: p.store, name: p.name, source: src })
      await until(`init ${key}`, async () => {
        const s = await http('GET', `/api/project-inits/${init.id}`)
        if (s.step === 'failed' || (s.ended && s.step !== 'done')) throw new Error(`init ${key}: ${JSON.stringify(s.error ?? s)}`)
        return s.step === 'done'
      })
      rmSync(src, { recursive: true, force: true })
      const history = historySteps(p, plan)
      for (const s of history) {
        const out = await lfsAsync(cli, home, s.argv, s.input)
        steps++
        if (stepEffect(s, out).commit) commits++
      }
      log(`project ${key}: ${history.length} history steps`)
    }
  }))
  return { stores, projects: projects.map((p) => `${p.store}/${p.name}`), steps, commits, files }
}

/** Reuse a manifest made by this fixture version with this CLI. */
function reusable(path, cliCommit) {
  if (!existsSync(path)) return null
  const m = JSON.parse(readFileSync(path, 'utf8'))
  return m.version === FIXTURE_VERSION && m.cliCommit === cliCommit ? m : null
}

/**
 * Build (or reuse) an in-process tier home: `<dir>/home`, manifest `<dir>/fixture.json`.
 * @returns the manifest.
 */
export async function buildHome({ tier, dir = join(root, `.dev-perf-${tier}`), force = false, log = console.log } = {}) {
  const cli = realCli()
  const cliCommit = JSON.parse(spawnSync(cli, ['--version', '--json'], { encoding: 'utf8' }).stdout).commit
  const manifestPath = join(dir, 'fixture.json')
  const found = !force && reusable(manifestPath, cliCommit)
  if (found) return found
  rmSync(dir, { recursive: true, force: true })
  const home = join(dir, 'home')
  mkdirSync(home, { recursive: true, mode: 0o700 })
  const { loadRoutes, testHost } = await import('../test/routes-helper.mjs')
  const R = await loadRoutes()
  const daemonPath = process.env.DSH_LAYERFS_DAEMON ?? join(dirname(cli), 'layerfs-daemon')
  const pin = { daemon: existsSync(daemonPath) ? daemonPath : null }
  const lfs = R.cli.client({ cli, home })
  let recorded
  const t = testHost({
    lfs, root: dir, home, cli, daemon: pin?.daemon ?? null,
    discovery: () => R.cli.discover({ cli }),
    ensureNative: () => recorded ??= (process.platform === 'darwin' && pin?.daemon ? lfs.machine.update('native', { daemon: pin.daemon }) : Promise.resolve()),
  })
  const groups = R.createGroups(t.host, async () => ({ ok: true }))
  const http = async (method, path, body) => {
    const r = await R.handle(groups, { method, path, body })
    if (r.status < 200 || r.status >= 300) throw new Error(`${method} ${path}: ${r.status} ${JSON.stringify(r.body)}`)
    return r.body
  }
  const started = Date.now()
  try {
    // No Sandbox: each one keeps a native daemon process running for the life of the fixture.
    const made = await seed({ tier, http, cli, home, fixtures: join(dir, 'src'), log })
    const manifest = { version: FIXTURE_VERSION, tier, mode: 'home', cli, cliCommit, daemon: pin.daemon, home, createdAt: new Date().toISOString(), seconds: Math.round((Date.now() - started) / 1000), ...made }
    writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`)
    return manifest
  } finally {
    await groups.projects.close()
    rmSync(join(dir, 'src'), { recursive: true, force: true })
  }
}

/** Seed through a running side server, plus one fake-llm project session. */
async function buildServer({ tier, force }) {
  const { openDev, runTurn } = await import('./e2e-lib.mjs')
  const dev = process.env.DSH_LAYERFS_DEV_DIR ?? join(root, '.dev')
  const home = process.env.DSH_LAYERFS_HOME ?? join(dev, 'layerfs-home')
  const manifestPath = join(dev, `fixture-${tier}.json`)
  const cli = realCli()
  const cliCommit = JSON.parse(spawnSync(cli, ['--version', '--json'], { encoding: 'utf8' }).stdout).commit
  if (!force && reusable(manifestPath, cliCommit)) { console.log(`perf-fixture: tier ${tier} already seeded (${manifestPath}); --force to reseed`); return }
  const started = Date.now()
  const { browser, page, logs } = await openDev()
  try {
    await page.waitForFunction(() => window.__dshLayerfs, null, { timeout: 30000 })
    const http = async (method, path, body) => {
      const r = await page.evaluate(async ([m, p, b]) => {
        try { return { ok: true, value: await window.__dshLayerfs.op('http', { method: m, path: p, ...(b === undefined ? {} : { body: b }) }) } } catch (e) { return { ok: false, message: `${e.code} ${e.message}` } }
      }, [method, path, body])
      if (!r.ok) throw new Error(`${method} ${path}: ${r.message}`)
      if (r.value.status < 200 || r.value.status >= 300) throw new Error(`${method} ${path}: ${r.value.status} ${JSON.stringify(r.value.body)}`)
      return r.value.body
    }
    const made = await seed({ tier, http, cli, home, fixtures: join(dev, 'fixtures') })
    // One fake-llm project session in the first project: two checkpointed turns.
    const project = made.projects[0]
    const placed = await page.evaluate(async (k) => window.__dshLayerfs.openProjectSession(k), project)
    await sleep(1500)
    const projectCommand = (command) => `bash: "$DSH_LAYERFS_CLI" workspace exec "$DSH_LAYERFS_WS" ${JSON.stringify(command)} --stream`
    for (const [i, command] of ['echo one > perf-one.txt', 'echo two > perf-two.txt'].entries()) {
      const t = await runTurn(page, { message: projectCommand(command), newSession: false, timeoutMs: 120000 })
      if (!t.ok) throw new Error(`session turn ${i + 1}: ${t.error}`)
    }
    let state
    for (let i = 0; i < 120; i++) {
      state = await page.evaluate(async (sid) => window.__dshLayerfs.op('session.state', { sessionId: sid }), placed.sessionId)
      if (state.checkpoints.filter((c) => c.kind === 'turn' && c.commit).length >= 2) break
      await sleep(500)
    }
    const session = { id: placed.sessionId, workspaceId: placed.workspaceId, project, sandbox: state.binding?.sandbox ?? null, machine: state.binding?.machine ?? 'native', branch: state.binding?.branch ?? null }
    const manifest = { version: FIXTURE_VERSION, tier, mode: 'server', cli, cliCommit, createdAt: new Date().toISOString(), seconds: Math.round((Date.now() - started) / 1000), ...made, session }
    writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`)
    console.log(`perf-fixture: tier ${tier} seeded in ${manifest.seconds} s → ${manifestPath}`)
    if (logs.length) console.log('page errors:', logs.slice(0, 5))
  } finally {
    await browser.close()
  }
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? '').href) {
  const args = process.argv.slice(2)
  const flag = (name, fallback) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : fallback }
  const tier = flag('--tier', 'S')
  const force = args.includes('--force')
  if (args.includes('--server')) await buildServer({ tier, force })
  else {
    const started = Date.now()
    const m = await buildHome({ tier, dir: flag('--dir', undefined), force })
    console.log(`perf-fixture: tier ${tier}: ${m.stores.length} Stores, ${m.projects.length} projects, ${m.steps} history steps, ${m.commits} Commits, ${m.files} files (${Math.round((Date.now() - started) / 1000)} s) → ${m.home}`)
  }
}
