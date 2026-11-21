#!/usr/bin/env node
// Host route scaling: CLI spawns per GET route at tiers S and M, in process
// (docs/specs/backend-e2e-tests.md §4.2). No browser, no server, no lib/: the
// routes come from src through test/routes-helper.mjs (as the live case of
// test/routes.test.mjs), over the real pinned CLI behind the count shim
// (test/fixtures/cli-count.sh) on the fixture homes tools/perf-fixture.mjs
// builds (`.dev-perf-S/home`, `.dev-perf-M/home`; built on first use, then reused).
//
//   node test/perf/host.perf.mjs [--tier S|M] [--reps 5] [--only id,id] [--rebuild]
//
// Env: LFS_REAL_CLI (else the newest runtime in ~/.layerfs, read only). Never
// ~/.layerfs as a home, never a server.
//
// Each measurement is cold: new route groups (fresh in-memory caches) per
// repetition, the native daemon recorded once per tier (as the host does once per
// process). Per route: spawns (discovery `--version`/`help` apart), by
// `family.call`, p50/p95 duration over the repetitions. Spawn counts must not vary
// between repetitions. Scaling rule (both tiers measured): spawns(M) − spawns(S)
// must be within the route's declared allowance, a sum over the fixture's
// dimensions (Stores, projects, branches of the measured project); anything else
// that grows fails as N+1. `historyBranches` and `ledgerPages` are measured from the
// fixture: the Branches History reads up front, and the pages a first ledger build reads. A route with `sameAs` must spawn exactly what that
// route spawns alone (concurrent reads shared). Exit 1 on any breach, unstable count or failed route.
// Results: `.dev-perf-host/<git-short-sha>.json`.
import { fileURLToPath } from 'node:url'
import { execSync } from 'node:child_process'
import { mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { buildHome, realCli } from '../../tools/perf-fixture.mjs'
import { groupSpawns, percentile, spawnOffset, spawns } from '../../tools/perf-lib.mjs'
import { loadRoutes, testHost } from '../routes-helper.mjs'

const root = fileURLToPath(new URL('../..', import.meta.url))
const args = process.argv.slice(2)
const flag = (name, fallback) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : fallback }
const tiers = flag('--tier', null) ? [flag('--tier')] : ['S', 'M']
const reps = Number(flag('--reps', 5))
const only = flag('--only', null)?.split(',')
const shim = join(root, 'test', 'fixtures', 'cli-count.sh')
const out = join(root, '.dev-perf-host')
mkdirSync(out, { recursive: true })

const cli = realCli()
process.env.LFS_REAL_CLI = cli
const R = await loadRoutes()
// The client reads this many Branches' histories when History opens (src/client/files/ProjectHistory.tsx).
const INITIAL_BRANCHES = Number(readFileSync(join(root, 'src/client/files/ProjectHistory.tsx'), 'utf8').match(/const INITIAL_BRANCHES = (\d+)/)?.[1] ?? 8)
const enc = encodeURIComponent

/**
 * Routes. `path(fx)` for one GET, or `run(get, fx)` for a client-shaped sequence.
 * `allow`: spawns each unit of growth may add — { stores, projects, branches }.
 * `known`: the review finding a growth is expected from (still a breach).
 * `coldStats`: a fresh storage ledger folder per repetition (the first Stats visit).
 */
const ROUTES = [
  { id: 'projects', title: 'GET /api/projects', path: () => '/api/projects', allow: { stores: 1 } },
  { id: 'storage', title: 'GET /api/storage', path: () => '/api/storage', allow: { stores: 1 } },
  { id: 'machines', title: 'GET /api/machines', path: () => '/api/machines', allow: {} },
  {
    // The page's two readers of the machines view at once: they must share one read.
    id: 'machines.twice', title: 'GET /api/machines twice, concurrently', allow: {}, sameAs: 'machines',
    run: async (get) => { await Promise.all(['/api/machines', '/api/machines'].map((p) => get(p))) },
  },
  {
    // What the LayerFS and Resources pages ask for at once (machines twice: the sidebar and the page).
    id: 'overview.reads', title: 'GET /api/machines ×2 + /api/storage + /api/projects, concurrently', allow: { stores: 1 },
    known: 'finding 3: concurrent views were not shared (a second store list / machines read)',
    run: async (get) => { await Promise.all(['/api/machines', '/api/machines', '/api/storage', '/api/projects'].map((p) => get(p))) },
  },
  { id: 'store.stats', title: 'GET /api/stores/:s/stats', path: (fx) => `/api/stores/${enc(fx.store)}/stats`, allow: {} },
  { id: 'project', title: 'GET /api/projects/:p', path: (fx) => `/api/projects/${enc(fx.project)}`, allow: {} },
  { id: 'branches', title: 'GET /api/projects/:p/branches', path: (fx) => `/api/projects/${enc(fx.project)}/branches?pos=0`, allow: {} },
  { id: 'history.branches', title: 'GET /api/projects/:p/history', path: (fx) => `/api/projects/${enc(fx.project)}/history?pos=0`, allow: {} },
  { id: 'history.main', title: 'GET …/history?branch=main', path: (fx) => `/api/projects/${enc(fx.project)}/history?branch=main`, allow: {} },
  { id: 'history.side', title: 'GET …/history?branch=release-0.3', path: (fx) => `/api/projects/${enc(fx.project)}/history?branch=release-0.3`, allow: {} },
  {
    // One branch commits per Branch read up front is the floor (2.0 has no listing across
    // Branches), capped by INITIAL_BRANCHES; the branch get per Branch is gone (finding 1).
    id: 'history.page', title: `History tab open (branches page + first ${INITIAL_BRANCHES} branches, 4 at a time)`, allow: { historyBranches: 1 },
    run: historyOpen,
  },
  {
    // Reopening History (the tab again, a refresh): pages read at a head are kept, so only the
    // Branch list is read again however many Branches there are.
    id: 'history.reopen', title: 'History tab opened again (same host)', allow: {}, prime: historyOpen, run: historyOpen,
  },
  // main is a chain of Layers: commits?branch=main answers [] without a call, so read a Branch.
  { id: 'commits', title: 'GET …/commits?branch=release-0.3', path: (fx) => `/api/projects/${enc(fx.project)}/commits?branch=release-0.3`, allow: {} },
  { id: 'branch-commits', title: 'GET …/branch-commits (3 fixed branches)', path: (fx) => `/api/projects/${enc(fx.project)}/branch-commits?branch=release-0.1&branch=release-0.2&branch=release-0.3`, allow: {} },
  { id: 'fs.root', title: 'GET …/fs?branch=main&path=/', path: (fx) => `/api/projects/${enc(fx.project)}/fs?branch=main&path=%2F`, allow: {} },
  { id: 'file', title: 'GET …/file?path=/README.md', path: (fx) => `/api/projects/${enc(fx.project)}/file?branch=main&path=%2FREADME.md`, allow: {} },
  { id: 'sandboxes', title: 'GET …/sandboxes', path: (fx) => `/api/projects/${enc(fx.project)}/sandboxes`, allow: {} },
  { id: 'stats', title: 'GET …/stats?branch=main', path: (fx) => `/api/projects/${enc(fx.project)}/stats?branch=main`, allow: {} },
  {
    // The first visit builds the project's storage ledger once, in the background: every saved
    // Commit's counters, at 25 per branch commits --include-storage page (2.0 sums them nowhere).
    // It is kept on disk; storage.steady must stay flat (finding 2).
    id: 'storage.first', title: 'GET …/storage, first visit (+ its background ledger build)', allow: { ledgerPages: 1 }, coldStats: true,
    run: storageVisit,
  },
  { id: 'storage.steady', title: 'GET …/storage, ledger saved (+ any background build)', allow: {}, run: storageVisit },
].filter((r) => !only || only.includes(r.id))

// The History tab as the client opens it (ProjectHistory.tsx, filesApi.branchHead): the Branch
// list, then the first INITIAL_BRANCHES (main first), each read at its head with its fork point,
// none for a side Branch with nothing saved since its fork; four at a time.
async function historyOpen(get, fx) {
  const page = await get(`/api/projects/${enc(fx.project)}/history?pos=0`)
  const names = [...page.branches.filter((b) => b.name === 'main'), ...page.branches.filter((b) => b.name !== 'main')].slice(0, INITIAL_BRANCHES)
  const read = (b) => {
    if (b.name !== 'main' && b.head && b.head === b.forkedFrom) return null
    const q = new URLSearchParams({ branch: b.name, ...(b.head ? { at: b.head } : {}), ...(b.forkedFrom ? { fork: b.forkedFrom } : {}) })
    return get(`/api/projects/${enc(fx.project)}/history?${q}`)
  }
  for (let i = 0; i < names.length; i += 4) await Promise.all(names.slice(i, i + 4).map(read))
}

// One Stats visit, then wait for the ledger build it may have started (projects.close()
// awaits the build jobs) instead of polling, so the count does not depend on build time.
async function storageVisit(get, fx, groups) {
  await get(`/api/projects/${enc(fx.project)}/storage`)
  await groups.projects.close()
}

// ---------------------------------------------------------------- measure
async function measureTier(tier) {
  const fixture = await buildHome({ tier, force: args.includes('--rebuild'), log: () => {} })
  const home = fixture.home
  const countFile = join(out, `spawns-${tier}.log`)
  process.env.LFS_COUNT_FILE = countFile
  // Kept reads off: each repetition must spawn what a cold page would.
  const lfs = R.cli.client({ cli: shim, home, keepReads: false })
  let recorded
  const ensureNative = () => recorded ??= (process.platform === 'darwin' && fixture.daemon ? lfs.machine.update('native', { daemon: fixture.daemon }) : Promise.resolve())
  const scratch = mkdtempSync(join(out, `run-${tier}-`))
  const sharedStats = join(scratch, 'stat')
  const fx = { project: fixture.projects[0], store: fixture.projects[0].split('/')[0] }
  const listed = await R.cli.pages((paging) => lfs.branch.list(fx.project, { ...paging, detail: true }))
  const branches = listed.length
  // Branches History reads up front: main first, then side Branches with something saved, up to the cap
  const historyBranches = [listed.find((b) => b.branch === 'main'), ...listed.filter((b) => b.branch !== 'main')].filter(Boolean)
    .slice(0, INITIAL_BRANCHES).filter((b) => b.branch === 'main' || (b.saved_head?.id ?? b.head) !== b.forked_from).length
  const dims = { stores: fixture.stores.length, projects: fixture.projects.length, branches, historyBranches, ledgerPages: 0 }
  // A new CLI client per route group, as a freshly started host has: the client keeps polling
  // reads briefly (cli.ts KEPT_READS) and shares reads in flight, so a reused one would answer
  // later repetitions from the first and the variance check would measure the cache instead.
  const groupsFor = (stats) => R.createGroups(testHost({ lfs: R.cli.client({ cli: shim, home }), root: scratch, home, cli: shim, daemon: fixture.daemon, stats, discovery: () => R.cli.discover({ cli: shim }), ensureNative }).host, async () => ({ ok: true }))
  // Warm discovery and the native record outside any window (the host does both once),
  // and build the measured project's storage ledger in the shared folder (storage.steady).
  { const g = groupsFor(sharedStats); await R.handle(g, { method: 'GET', path: '/api/machines' }); await storageVisit(async (path) => (await R.handle(g, { method: 'GET', path })).body, fx, g) }
  // ledger pages a first Stats visit must read: ceil(saves / 25) per side Branch, from the ledger just built
  for (const file of readdirSync(join(sharedStats, 'project-storage'))) {
    const ledger = JSON.parse(readFileSync(join(sharedStats, 'project-storage', file), 'utf8'))
    const perBranch = new Map()
    for (const row of Object.values(ledger.commits ?? {})) perBranch.set(row[6], (perBranch.get(row[6]) ?? 0) + 1)
    dims.ledgerPages += [...perBranch.values()].reduce((sum, n) => sum + Math.ceil(n / 25), 0)
  }
  const results = {}
  for (const route of ROUTES) {
    const samples = []
    let error = null
    for (let rep = 0; rep < reps && !error; rep++) {
      const groups = groupsFor(route.coldStats ? join(scratch, `stat-${route.id}-${rep}`) : sharedStats)
      const get = async (path) => {
        const r = await R.handle(groups, { method: 'GET', path })
        if (r.status !== 200) throw new Error(`GET ${path}: ${r.status} ${JSON.stringify(r.body).slice(0, 300)}`)
        return r.body
      }
      if (route.prime) await route.prime(get, fx, groups)
      const offset = spawnOffset(countFile)
      const t0 = performance.now()
      try { route.run ? await route.run(get, fx, groups) : await get(route.path(fx)) } catch (e) { error = String(e?.message ?? e) }
      const ms = performance.now() - t0
      await groups.projects.close()
      const lines = spawns(countFile, offset)
      const g = groupSpawns(lines)
      samples.push({ ms, spawns: g.total, discovery: g.discovery, byCall: g.byCall, argv: lines.map((l) => l.argv) })
    }
    const counts = samples.map((s) => s.spawns)
    results[route.id] = {
      title: route.title, error, spawns: Math.max(...counts), counts, stable: new Set(counts.map((_, i) => JSON.stringify(samples[i].byCall))).size === 1,
      byCall: samples[0].byCall, p50: Math.round(percentile(samples.map((s) => s.ms), 50)), p95: Math.round(percentile(samples.map((s) => s.ms), 95)),
      argv: samples[0].argv, discovery: samples[0].discovery,
    }
  }
  rmSync(scratch, { recursive: true, force: true })
  return { tier, dims, fixture: { stores: fixture.stores, projects: fixture.projects.length, commits: fixture.commits, files: fixture.files, cliCommit: fixture.cliCommit }, routes: results }
}

const started = Date.now()
const runs = {}
for (const tier of tiers) runs[tier] = await measureTier(tier)

// ---------------------------------------------------------------- report
const pad = (s, n) => String(s).padEnd(n)
const lpad = (s, n) => String(s).padStart(n)
const failures = []
const S = runs.S, M = runs.M
const allowed = (route) => S && M ? Object.entries(route.allow ?? {}).reduce((sum, [dim, k]) => sum + k * Math.max(0, M.dims[dim] - S.dims[dim]), 0) : null
for (const tier of tiers) console.log(`tier ${tier}: ${runs[tier].dims.stores} Stores, ${runs[tier].dims.projects} projects, ${runs[tier].dims.branches} branches in the measured project, ${runs[tier].fixture.commits} history Commits`)
console.log(`\n${pad('route', 18)}${tiers.map((t) => `${lpad(`${t} spawns`, 10)}${lpad(`${t} p50/p95`, 14)}`).join('')}${S && M ? `${lpad('Δ', 5)}${lpad('allow', 7)}  verdict` : ''}`)
const verdicts = {}
for (const route of ROUTES) {
  const cells = tiers.map((t) => {
    const r = runs[t].routes[route.id]
    const twin = route.sameAs && runs[t].routes[route.sameAs]
    if (twin && !r.error && r.spawns !== twin.spawns) failures.push(`${route.id} (${t}): ${r.spawns} spawns, not shared with one ${route.sameAs} read (${twin.spawns}): ${JSON.stringify(r.byCall)}`)
    if (r.error) failures.push(`${route.id} (${t}): ${r.error}`)
    if (!r.stable) failures.push(`${route.id} (${t}): spawns vary between repetitions ${JSON.stringify(r.counts)}`)
    return `${lpad(r.error ? 'ERR' : `${r.spawns}${r.stable ? '' : '~'}`, 10)}${lpad(`${r.p50}/${r.p95} ms`, 14)}`
  }).join('')
  let tail = ''
  if (S && M) {
    const d = M.routes[route.id].spawns - S.routes[route.id].spawns, a = allowed(route)
    const ok = d <= a
    verdicts[route.id] = { delta: d, allowed: a, ok, known: route.known ?? null }
    if (!ok) failures.push(`${route.id}: N+1 — spawns grew by ${d} (S ${S.routes[route.id].spawns} → M ${M.routes[route.id].spawns}), allowance ${a}; M by call ${JSON.stringify(M.routes[route.id].byCall)}${route.known ? ` [KNOWN: ${route.known}]` : ''}`)
    tail = `${lpad(d, 5)}${lpad(a, 7)}  ${ok ? 'ok' : route.known ? 'N+1 (known)' : 'N+1'}`
  }
  console.log(`${pad(route.id, 18)}${cells}${tail}`)
}
const last = runs[tiers.at(-1)]
console.log(`\nby call (${last.tier}):`)
for (const route of ROUTES) console.log(`  ${pad(route.id, 16)} ${Object.entries(last.routes[route.id].byCall).map(([k, n]) => n > 1 ? `${k}×${n}` : k).join(' ')}`)

const sha = (() => { try { return execSync('git rev-parse --short HEAD', { cwd: root }).toString().trim() } catch { return 'nogit' } })()
const resultPath = join(out, `${sha}.json`)
writeFileSync(resultPath, `${JSON.stringify({ sha, at: new Date().toISOString(), seconds: Math.round((Date.now() - started) / 1000), reps, initialBranches: INITIAL_BRANCHES, runs, verdicts }, null, 2)}\n`)
console.log(`\nresults: ${resultPath} (${Math.round((Date.now() - started) / 1000)} s)`)
if (failures.length) { console.log(`\nFAIL:\n  ${failures.join('\n  ')}`); process.exit(1) }
console.log('PASS')
