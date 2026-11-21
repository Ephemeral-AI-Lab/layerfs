// The plugin against the real LayerFS 2.0 call table: every call it sends (home, machine,
// store, backup, project, branch, commit, file, sandbox, workspace and the six doctors) is in
// the pinned CLI's `help --json`, with the options, `--with` sections, `--progress` and
// `--full` it passes, and every flag in a raw argv (presets, simulations) is one the call takes.
// Runs DSH_LAYERFS_CS20_CLI or the dev runtime pin with a throwaway LAYERFS_HOME.
//   node --test test/cs20-contract.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { execFileSync } from 'node:child_process'
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

import { runtimePin } from '../scripts/runtime-pin.mjs'

const root = fileURLToPath(new URL('..', import.meta.url))
const CLI = process.env.DSH_LAYERFS_CS20_CLI ?? runtimePin()?.cli
const skip = CLI && existsSync(CLI) ? false : `no 2.0 CLI ${CLI ?? 'selected'}`

/** The pinned table, read once in a home of its own. */
function table() {
  const home = mkdtempSync(join(tmpdir(), 'dsh-cs20-home-'))
  try {
    const text = execFileSync(CLI, ['help', '--json'], { encoding: 'utf8', timeout: 30_000, env: { ...process.env, LAYERFS_HOME: home } })
    const help = JSON.parse(text)
    const calls = new Map(help.calls.map((c) => [`${c.family}.${c.call}`, c]))
    return { help, calls }
  } finally { rmSync(home, { recursive: true, force: true }) }
}

/** Every production source file (src/), as text. */
function sources(dir = join(root, 'src'), out = []) {
  for (const name of readdirSync(dir)) {
    const path = join(dir, name)
    if (statSync(path).isDirectory()) sources(path, out)
    else if (/\.(ts|tsx|js|mjs)$/.test(name)) out.push({ path, text: readFileSync(path, 'utf8') })
  }
  return out
}

const BUILT = new Set(['home', 'machine', 'store', 'project', 'sandbox', 'backup', 'branch', 'commit', 'file', 'workspace'])
const FAMILY = [...BUILT].join('|')
// family and thing-kind words: `['project', 'bucket']` is a list of kinds, not an argv
const FAMILIES = new Set(['home', 'machine', 'store', 'project', 'sandbox', 'backup', 'bucket', 'branch', 'commit', 'file', 'workspace', 'logs', 'doctor'])
const optionNames = (c) => new Set((c.options ?? []).map((o) => o.name))
const withNames = (c) => new Set(Object.keys(c.form?.with ?? {}))

// What the plugin passes besides the positionals, per call (src/host/routes/*.ts,
// service.ts, logs/source.ts, sessions/*.ts, presets/*.js, simulations/*.ts). `progress` is
// an `on_progress` callback (`--progress`); `with` is a `--with` section.
const SENT = {
  'home.init': {},
  'home.get': { with: ['platforms', 'tree'] },
  'home.log': { options: ['call', 'client', 'tag', 'outcome', 'since', 'limit', 'cursor'] },
  'home.clear_log': {},
  'machine.list': {},
  'machine.get': { with: ['live'] },
  'machine.images': { with: ['docker'] },
  'machine.load_image': { progress: true },
  'machine.prune': { options: ['unused'] },
  'machine.stats': { with: ['space'] },
  'machine.files': { options: ['depth', 'limit', 'cursor', 'byte_offset', 'byte_length'] },
  'machine.log': { options: ['limit', 'cursor'] },
  'machine.start': { progress: true },
  'machine.stop': { options: ['force'], progress: true },
  'machine.create': { options: ['cpus', 'memory', 'disk', 'image', 'os_image'], progress: true },
  'machine.remove': { progress: true },
  'machine.update': { options: ['cpus', 'memory', 'image', 'docker', 'wsl', 'daemon'] },
  // Storage (routes/storage.ts, jobs.ts; presets build/runner)
  'store.list': { with: ['stats'] },
  'store.get': { with: ['live', 'stats'] },
  'store.create': {},
  'store.remove': {},
  'store.save_auth': { options: ['user', 'password'] },
  'store.auths': {},
  'store.remove_auth': {},
  'store.connect': { options: ['auth', 'machine', 'synchronous_commit'] },
  'store.disconnect': { options: ['machine'] },
  'store.update': { options: ['auth', 'synchronous_commit'] },
  'store.doctor': { options: ['machine'] },
  'store.move': { progress: true },
  'store.compact': { progress: true },
  'backup.connect': { options: ['access_key', 'secret_key'] },
  'backup.disconnect': {},
  'backup.list': { options: ['bucket'] },
  'backup.create': { options: ['file'], progress: true },
  'backup.restore': { progress: true },
  // Sandboxes and Workspaces (routes/projects.ts, machines.ts, commands.ts, router.ts; sessions)
  'sandbox.create': { options: ['machine', 'image'], progress: true },
  'sandbox.list': { options: ['project', 'limit', 'cursor'] },
  'sandbox.get': { with: ['stats'] },
  'sandbox.remove': { options: ['force'] },
  'sandbox.files': { options: ['depth', 'limit', 'cursor', 'byte_offset', 'byte_length'] },
  'sandbox.log': { options: ['limit', 'cursor'] },
  'workspace.mount': { options: ['name'] },
  'workspace.exec': { progress: true },
  'workspace.cancel': { options: ['exec'] },
  'workspace.commit': { options: ['message'] },
  'workspace.unmount': { options: ['discard', 'force'] },
  'workspace.status': {},
  // Projects, Branches, Commits and files (routes/projects.ts, project-storage.ts; sessions; presets)
  'project.init': { progress: true },
  'project.list': { options: ['store', 'limit', 'cursor'] },
  'project.get': {},
  'project.stats': { options: ['branch', 'at'] },
  'project.export': { options: ['branch'] },
  'project.delete': {},
  'branch.list': { options: ['prefix', 'limit', 'cursor'] },
  'branch.get': {},
  'branch.fork': { options: ['at'] },
  'branch.merge': { options: ['into', 'message'] },
  'branch.publish': { options: ['at'] },
  'commit.list': { options: ['at', 'limit', 'cursor'] },
  'commit.get': { options: ['branch'] },
  'commit.diff': { options: ['from', 'path', 'limit', 'cursor'], with: ['patch'] },
  'file.read': { options: ['at', 'line', 'limit'] },
  'file.stat': { options: ['at'] },
  'file.list': { options: ['at', 'limit', 'cursor'] },
  'file.glob': { options: ['limit'] },
  'file.history': { options: ['at', 'traversal', 'limit', 'cursor'] },
  'file.blame': { options: ['at', 'line', 'limit'] },
  'file.write': { options: ['message'] },
  'file.apply': { options: ['message'] },
}

test('the switches the seam writes are in the 2.0 table', { skip }, () => {
  const { help } = table()
  const switches = new Set((help.switches ?? []).map((s) => s.name))
  for (const name of ['--detail', '--with', '--progress']) assert.ok(switches.has(name), `${name} is a 2.0 switch`)
  const detail = help.switches.find((s) => s.name === '--detail')
  for (const call of Object.keys(SENT)) assert.ok(detail.calls.includes(call), `${call} takes --detail (the UI reads its long form)`)
})

test('every call the plugin sends is in the 2.0 table, with its options, sections and progress', { skip }, () => {
  const { calls } = table()
  for (const [name, sent] of Object.entries(SENT)) {
    const c = calls.get(name)
    assert.ok(c, `${name} is a 2.0 call`)
    for (const option of sent.options ?? []) assert.ok(optionNames(c).has(option), `${name} takes ${option}`)
    for (const section of sent.with ?? []) assert.ok(withNames(c).has(section), `${name} has --with ${section}`)
    if (sent.progress) assert.ok(c.form?.progress && optionNames(c).has('on_progress'), `${name} reports progress`)
  }
  // the source agrees with the table above: any `lfs.<family>.x` call or raw
  // `['<family>', 'x', ...]` argv in src/ names a 2.0 call listed in SENT
  const found = new Set()
  for (const { path, text } of sources()) {
    for (const m of text.matchAll(new RegExp(`\\blfs(?:\\(\\))?\\.(${FAMILY})\\.([a-z_]+)\\b`, 'g'))) found.add(`${m[1]}.${m[2]}\t${path}`)
    for (const m of text.matchAll(new RegExp(`\\[\\s*'(${FAMILY})'\\s*,\\s*'([a-z_-]+)'\\s*[,\\]]`, 'g'))) if (!FAMILIES.has(m[2])) found.add(`${m[1]}.${m[2].replace(/-/g, '_')}\t${path}`)
  }
  assert.ok(found.size > 60, 'the scan found the plugin\'s calls')
  for (const entry of found) {
    const [name, path] = entry.split('\t')
    assert.ok(BUILT.has(name.split('.')[0]))
    assert.ok(calls.has(name), `${name} (${path.slice(root.length)}) is a 2.0 call`)
    assert.ok(name in SENT, `${name} (${path.slice(root.length)}) is listed in SENT`)
  }
})

// Owner 2026-11-19 (spec v20, package LO): LayerFS removes leftovers by itself at
// safe points and drops `home leftovers` and `home prune` with no alias. A pin from
// before LO still lists both; this asks only that the plugin never sends them.
test('the plugin sends neither home.leftovers nor home.prune, and has no route or page for them', () => {
  for (const gone of ['home.leftovers', 'home.prune']) assert.ok(!(gone in SENT), `${gone} is not listed in SENT`)
  const sends = [
    /\blfs(?:\(\))?\.home\.(?:leftovers|prune)\b/, // the seam's proxy
    /\[\s*'home'\s*,\s*'(?:leftovers|prune)'/, // a raw argv
    /\bhome\s+(?:leftovers|prune)\b/, // a command shown in a tip
    /call\s*===\s*'leftovers'/, // a discovery check
    /\/api\/doctor\/leftovers|'leftovers'/, // the old route
  ]
  let scanned = 0
  for (const { path, text } of sources()) {
    scanned++
    for (const re of sends) assert.ok(!re.test(text), `${path.slice(root.length)} sends or offers a removed call (${re})`)
  }
  assert.ok(scanned > 100, `the scan read the plugin's sources (${scanned})`)
})

test('every flag in a raw argv the plugin writes (presets, simulations) is a 2.0 switch or one the call takes', { skip }, () => {
  const { help, calls } = table()
  const switches = new Set((help.switches ?? []).map((s) => s.name))
  const flagsOf = (c) => {
    const out = new Set()
    for (const o of c.options ?? []) {
      if (o.kind === 'function') continue
      out.add(`--${o.name.replace(/_/g, '-')}`)
      if (o.shell) out.add(o.shell)
    }
    return out
  }
  let checked = 0
  for (const { path, text } of sources()) {
    // one argv literal on one line: ['family', 'call', ...] up to its closing bracket
    for (const m of text.matchAll(new RegExp(`\\[\\s*'(${FAMILY})'\\s*,\\s*'([a-z_-]+)'([^\\n]*)`, 'g'))) {
      if (FAMILIES.has(m[2])) continue
      const name = `${m[1]}.${m[2].replace(/-/g, '_')}`
      const c = calls.get(name)
      assert.ok(c, `${name} (${path.slice(root.length)}) is a 2.0 call`)
      for (const f of m[3].matchAll(/'(--[a-z][a-z-]*)'/g)) {
        checked++
        assert.ok(switches.has(f[1]) || flagsOf(c).has(f[1]), `${name} (${path.slice(root.length)}) takes ${f[1]}`)
      }
    }
  }
  assert.ok(checked > 30, `the scan checked the plugin's flags (${checked})`)
})

test('secrets go on stdin: the content options the plugin passes have the shell flags it writes', { skip }, () => {
  const { calls } = table()
  const option = (call, name) => (calls.get(call)?.options ?? []).find((o) => o.name === name)
  assert.equal(option('store.save_auth', 'password')?.shell, '--password-stdin')
  assert.equal(option('store.save_auth', 'password')?.kind, 'content')
  assert.equal(option('backup.connect', 'secret_key')?.shell, '--key-stdin')
  // `file write` takes its content as a positional read from stdin (the preset recipe's `input`)
  assert.equal(calls.get('file.write').args.at(-1).kind, 'content')
})

test('the 2.0 answers the plugin reads: paging, the project/file forms and the removed calls', { skip }, () => {
  const { calls } = table()
  const form = (name) => calls.get(name)?.form ?? {}
  const longOf = (name) => new Set(form(name).long ?? [])
  // removed in 2.0; the plugin never sends them
  for (const gone of ['file.diff', 'file.revision', 'project.remove_file', 'store.status', 'store.push', 'store.pull', 'sandbox.close', 'logs.sandbox', 'branch.commits']) assert.equal(calls.has(gone), false, `${gone} is gone`)
  // paged lists answer items + next_cursor (and the plugin pages by --cursor)
  for (const name of ['project.list', 'branch.list', 'commit.list', 'commit.diff', 'file.list', 'file.history', 'sandbox.list']) {
    assert.ok(longOf(name).has('items'), `${name} answers items`)
    assert.ok(optionNames(calls.get(name)).has('cursor'), `${name} pages by cursor`)
  }
  // the long fields the routes read
  for (const f of ['text', 'next_line', 'line_truncated', 'binary']) assert.ok(longOf('file.read').has(f), `file.read --detail has ${f}`)
  for (const f of ['type', 'size', 'mode']) assert.ok(longOf('file.stat').has(f), `file.stat --detail has ${f}`)
  for (const f of ['ranges', 'commits']) assert.ok(longOf('file.blame').has(f), `file.blame --detail has ${f}`)
  for (const f of ['file_count', 'dir_count', 'size']) assert.ok(longOf('project.stats').has(f), `project.stats --detail has ${f}`)
  for (const f of ['commit', 'unchanged']) assert.ok(longOf('workspace.commit').has(f), `workspace.commit --detail has ${f}`)
  assert.ok(longOf('home.init').has('outcome'), 'home.init --detail has outcome')
})

test('spec v19 forms the Machines page reads: image item states, load-image reuse and drops, prune skips, compact outcome', { skip }, () => {
  const { calls } = table()
  const form = (name) => calls.get(name)?.form ?? {}
  const longOf = (name) => new Set(form(name).long ?? [])
  const sometimes = (name) => new Set(form(name).short?.sometimes ?? [])
  // machine images: an item's six-state companions (absent when ready) and a partial copy's staged bytes
  const item = new Set(form('machine.images').parts?.items?.long ?? [])
  for (const f of ['state', 'op', 'reason', 'fix', 'staged_bytes', 'sandboxes_count', 'last_used_at', 'source']) assert.ok(item.has(f), `machine images items have ${f}`)
  // machine load-image: progress events, `reused` / `dropped` in the short form, dropped_count only when cut
  assert.equal(form('machine.load_image').progress, true)
  for (const f of ['reused', 'dropped']) assert.ok(sometimes('machine.load_image').has(f), `machine load-image may answer ${f}`)
  for (const f of ['dropped_count', 'expanded_bytes']) assert.ok(longOf('machine.load_image').has(f), `machine load-image --detail has ${f}`)
  // machine prune: `skipped` items `{ image, message }`, `removed`, `freed_bytes`
  assert.ok(sometimes('machine.prune').has('skipped'))
  assert.deepEqual(form('machine.prune').parts?.skipped?.short?.always, ['image', 'message'])
  for (const f of ['removed', 'freed_bytes']) assert.ok(longOf('machine.prune').has(f), `machine prune --detail has ${f}`)
  // store compact: progress, `outcome` and `freed_bytes` (a remote sqlite Store runs on its worker: no host-side block)
  assert.equal(form('store.compact').progress, true)
  for (const f of ['outcome', 'freed_bytes']) assert.ok(longOf('store.compact').has(f), `store compact --detail has ${f}`)
  // sandbox create reports its phases; workspace exec answers its exec id and whole-stream byte counts
  assert.equal(form('sandbox.create').progress, true)
  for (const f of ['exec', 'exit_code', 'stdout_bytes', 'stderr_bytes', 'cancelled']) assert.ok(longOf('workspace.exec').has(f), `workspace exec --detail has ${f}`)
  // the markers this pin proves are gone from the source
  for (const { path, text } of sources()) assert.ok(!text.includes('cs20: switch when V19 merges'), `${path} still waits on V19`)
})

// The owner's Storage commits (merged in U4): a PostgreSQL card's size is
// `store get --with stats` (routes/projects.ts storeStats, `stored_bytes` +
// `metadata_bytes`), and Compact is `store compact --progress` as a job
// (routes/storage.ts). Sent through the seam to the pinned CLI, which records
// each argv and runs it on a sqlite Store of a throwaway home.
test('the Storage card\'s size and Compact send `store get --detail --with stats` and `store compact --detail --progress`', { skip }, async () => {
  const src = sources()
  const has = (re) => src.some((f) => re.test(f.text))
  assert.ok(has(/this\.lfs\.store\.get\(name, \{ detail: true, with: \['stats'\] \}\)/), 'storeStats reads store get --with stats --detail')
  assert.ok(has(/this\.lfs\.store\.compact\(store, \{ on_progress, on_spawn, detail: true \}\)/), 'compact runs store compact with progress and --detail')
  const dir = mkdtempSync(join(tmpdir(), 'dsh-cs20-storage-'))
  try {
    const home = join(dir, 'home'), log = join(dir, 'argv.log'), wrapper = join(dir, 'layerfs')
    writeFileSync(wrapper, `#!/bin/sh\nprintf '%s\\n' "$*" >> '${log}'\nexec '${CLI}' "$@"\n`)
    chmodSync(wrapper, 0o755)
    const env = { ...process.env, LAYERFS_HOME: home }
    execFileSync(CLI, ['home', 'init'], { env, timeout: 30_000 })
    execFileSync(CLI, ['store', 'create', 's1'], { env, timeout: 30_000 })
    await build({
      absWorkingDir: root, stdin: { contents: "export { client } from './src/host/layerfs/cli.ts'\n", resolveDir: root, loader: 'ts' },
      outfile: join(dir, 'cli.mjs'), bundle: true, format: 'esm', platform: 'node', target: 'node24', logLevel: 'warning',
    })
    const { client } = await import(pathToFileURL(join(dir, 'cli.mjs')).href)
    const lfs = client({ cli: wrapper, home, keepReads: false })
    const record = await lfs.store.get('s1', { detail: true, with: ['stats'] })
    for (const key of ['stored_bytes', 'metadata_bytes']) assert.equal(typeof record.stats?.[key], 'number', `store get --with stats answers stats.${key}`)
    const phases = []
    const report = await lfs.store.compact('s1', { on_progress: (e) => phases.push(e.phase), on_spawn: () => {}, detail: true })
    assert.ok(['compacted', 'unchanged'].includes(report.outcome), `compact outcome ${report.outcome}`)
    assert.equal(typeof report.freed_bytes, 'number')
    assert.ok(report.kept_waste && typeof report.kept_waste === 'object', 'compact --detail answers kept_waste')
    assert.deepEqual(phases.slice(0, 1), ['mark'], 'an sqlite compaction reports its phases from mark')
    const sent = readFileSync(log, 'utf8').split('\n').filter((l) => l.startsWith('store '))
    assert.deepEqual(sent, ['store get --detail --with stats -- s1', 'store compact --detail --progress -- s1'])
  } finally { rmSync(dir, { recursive: true, force: true }) }
})

test('the Doctor page\'s argv is a 2.0 call: six doctors, --progress only where reported, --full only where taken', { skip }, async () => {
  const { help, calls } = table()
  const out = mkdtempSync(join(tmpdir(), 'dsh-cs20-doctor-'))
  try {
    await build({
      absWorkingDir: root,
      stdin: { contents: "export * from './src/shared/doctor.ts'\n", resolveDir: root, loader: 'ts' },
      outfile: join(out, 'doctor.mjs'), bundle: true, format: 'esm', platform: 'node', target: 'node24', logLevel: 'warning',
    })
    const D = await import(pathToFileURL(join(out, 'doctor.mjs')).href)
    const doctors = [...calls.keys()].filter((k) => k.endsWith('.doctor'))
    assert.equal(D.DOCTOR_KINDS.length, 6)
    assert.equal(doctors.length, 6, `the table has six doctors: ${doctors.join(', ')}`)
    for (const kind of D.DOCTOR_KINDS) {
      for (const full of [false, true]) {
        const argv = D.doctorArgv({ kind, name: kind === 'home' ? undefined : 'x', full })
        if (full && !D.FULL_KINDS.includes(kind)) { assert.equal(argv, null, `${kind} has no Full check`); continue }
        const [family, call, ...rest] = argv
        const c = calls.get(`${family}.${call}`)
        assert.ok(c, `${family}.${call} is a 2.0 call`)
        assert.ok(rest.includes('--detail'))
        assert.equal(rest.includes('--progress'), !!c.form?.progress, `${family} doctor: --progress exactly when the call reports progress`)
        if (full) assert.ok(optionNames(c).has('full'), `${family} doctor takes --full`)
        assert.equal(rest.includes('--'), (c.args ?? []).length > 0, `${family} doctor: a name exactly when the call takes one`)
      }
    }
    // the tree read gate: home.get's `with` declares `tree`
    assert.equal(D.hasTreeRead(help.calls), true)
  } finally { rmSync(out, { recursive: true, force: true }) }
})

// Coming in LayerFS (package FX): `store save-auth` without `--user` or the
// password, `backup connect` without both keys and `file edit` without the old
// and new strings are usage errors. Every call site sends them.
test('options 2.0 makes required are always sent: save-auth user + password, backup connect keys, file edit strings', () => {
  const src = sources()
  const sites = (re) => src.flatMap((f) => [...f.text.matchAll(re)].map((m) => ({ path: f.path.slice(root.length), call: m[0] })))
  const saveAuth = sites(/\.store\.save_auth\([^)]*\)/g)
  assert.ok(saveAuth.length > 0)
  for (const s of saveAuth) assert.match(s.call, /\buser\b/, `${s.path}: ${s.call}`), assert.match(s.call, /\bpassword\b/, `${s.path}: ${s.call}`)
  const connect = sites(/\.backup\.connect\([^)]*\)/g)
  assert.ok(connect.length > 0)
  for (const s of connect) assert.match(s.call, /access_key/, `${s.path}: ${s.call}`), assert.match(s.call, /secret_key/, `${s.path}: ${s.call}`)
  const argv = sites(/\['store', 'save-auth'[^\]]*\]/g)
  assert.ok(argv.length > 0)
  for (const s of argv) assert.ok(s.call.includes("'--user'") && s.call.includes("'--password-stdin'"), `${s.path}: ${s.call}`)
  // the plugin itself makes no file edit (agents do, through the CLI): none to check, and none may appear without its strings
  for (const s of sites(/\.file\.edit\([^)]*\)/g)) assert.ok(/old_string/.test(s.call) && /new_string/.test(s.call), `${s.path}: ${s.call}`)
  for (const s of sites(/\['file', 'edit'[^\]]*\]/g)) assert.ok(s.call.includes("'--old-string'") && s.call.includes("'--new-string'"), `${s.path}: ${s.call}`)
})

/** cli.ts bundled into `dir` and its client over a wrapper that records each argv to `log`. */
async function recordingClient(dir, home) {
  const log = join(dir, 'argv.log'), wrapper = join(dir, 'layerfs')
  writeFileSync(wrapper, `#!/bin/sh\nprintf '%s\\n' "$*" >> '${log}'\nexec '${CLI}' "$@"\n`)
  chmodSync(wrapper, 0o755)
  await build({
    absWorkingDir: root, stdin: { contents: "export { client } from './src/host/layerfs/cli.ts'\n", resolveDir: root, loader: 'ts' },
    outfile: join(dir, 'cli.mjs'), bundle: true, format: 'esm', platform: 'node', target: 'node24', logLevel: 'warning',
  })
  const { client } = await import(pathToFileURL(join(dir, 'cli.mjs')).href)
  return { lfs: client({ cli: wrapper, home, keepReads: false }), sent: () => (existsSync(log) ? readFileSync(log, 'utf8').split('\n').filter(Boolean) : []) }
}

// LayerFS N-F: an option other than `--with` and a repeatable one (`--env`) given twice is
// a usage error (exit 2). The seam writes each option once and joins `--with` sections;
// a raw argv the plugin writes (presets, simulations) never repeats a flag either.
test('no option is given twice: the seam joins --with, and no raw argv repeats a flag', { skip }, async () => {
  const { help, calls } = table()
  const repeatable = new Set(['--with'])
  for (const c of help.calls) for (const o of c.options ?? []) if (o.repeat) repeatable.add(`--${o.name.replace(/_/g, '-')}`)
  let argvs = 0
  for (const { path, text } of sources()) {
    for (const m of text.matchAll(new RegExp(`\\[\\s*'(${FAMILY})'\\s*,\\s*'([a-z_-]+)'([^\\n]*)`, 'g'))) {
      if (FAMILIES.has(m[2]) || !calls.has(`${m[1]}.${m[2].replace(/-/g, '_')}`)) continue
      argvs++
      // up to the literal's own closing bracket, ignoring nested lists' brackets
      let depth = 1, end = 0
      for (; end < m[3].length && depth > 0; end++) depth += m[3][end] === '[' ? 1 : m[3][end] === ']' ? -1 : 0
      const flags = [...m[3].slice(0, end).matchAll(/'(--[a-z][a-z-]*)'/g)].map((f) => f[1])
      for (const f of new Set(flags)) if (!repeatable.has(f)) assert.ok(flags.filter((x) => x === f).length === 1, `${m[1]} ${m[2]} (${path.slice(root.length)}) gives ${f} once`)
    }
  }
  assert.ok(argvs > 30, `the scan read the plugin's raw argvs (${argvs})`)
  const dir = mkdtempSync(join(tmpdir(), 'dsh-cs20-repeat-'))
  try {
    const home = join(dir, 'home')
    const env = { ...process.env, LAYERFS_HOME: home }
    execFileSync(CLI, ['home', 'init'], { env, timeout: 30_000 })
    // the rule exists in this pin: a repeated --detail is exit 2
    let status = 0
    try { execFileSync(CLI, ['machine', 'list', '--detail', '--detail'], { env, timeout: 30_000, stdio: 'pipe' }) } catch (e) { status = e.status }
    assert.equal(status, 2, 'a repeated option is a usage error')
    const { lfs, sent } = await recordingClient(dir, home)
    const got = await lfs.home.get({ detail: true, with: ['platforms', 'tree', 'platforms'] })
    assert.ok(got.platforms && got.tree, 'both sections answered')
    assert.deepEqual(sent().filter((l) => l.startsWith('home get')), ['home get --detail --with platforms,tree --'])
  } finally { rmSync(dir, { recursive: true, force: true }) }
})

// LayerFS N-G: `help --json` marks the options a call needs `optional: false` (with
// `required: true`). The seam refuses a call that leaves one out before spawning, and
// every call site sends them (the static check above).
test('the table\'s required options are the three pairs the call sites send; the seam refuses one left out', { skip }, async () => {
  const { help } = table()
  const required = {}
  for (const c of help.calls) for (const o of c.options ?? []) if (o.optional === false || o.required === true) (required[`${c.family}.${c.call}`] ??= []).push(o.name)
  assert.deepEqual(required, { 'store.save_auth': ['user', 'password'], 'backup.connect': ['access_key', 'secret_key'], 'file.edit': ['old_string', 'new_string'] },
    'a new required option needs its call sites checked (the test above)')
  const dir = mkdtempSync(join(tmpdir(), 'dsh-cs20-required-'))
  try {
    const home = join(dir, 'home')
    execFileSync(CLI, ['home', 'init'], { env: { ...process.env, LAYERFS_HOME: home }, timeout: 30_000 })
    const { lfs, sent } = await recordingClient(dir, home)
    await assert.rejects(lfs.store.save_auth('x', { password: 'p' }), (e) => e.code === 'InvalidInput' && /user is required/.test(e.message))
    await assert.rejects(lfs.backup.connect('b', 'https://example.invalid', { secret_key: 's' }), (e) => e.code === 'InvalidInput' && /access_key is required/.test(e.message))
    assert.deepEqual(sent().filter((l) => /^(store save-auth|backup connect)/.test(l)), [], 'nothing spawned')
  } finally { rmSync(dir, { recursive: true, force: true }) }
})

// LayerFS N-D and Q: the machine answers the Machines page reads. An unreadable record
// is a `machine list` item (broken, failed, no platform, fix `layerfs machine doctor`);
// `machine get` refuses it `Failed` with that fix, so the page does not probe it; the
// other lists still answer. Native's live probe answers its free disk.
test('machine answers: an unreadable record is listed broken/failed and not probed; native\'s live probe has its free disk', { skip }, async () => {
  const dir = mkdtempSync(join(tmpdir(), 'dsh-cs20-machines-'))
  try {
    const home = join(dir, 'home')
    const env = { ...process.env, LAYERFS_HOME: home }
    execFileSync(CLI, ['home', 'init'], { env, timeout: 30_000 })
    mkdirSync(join(home, 'machine', 'bad'), { recursive: true })
    writeFileSync(join(home, 'machine', 'bad', 'record.json'), '{not json')
    const { lfs, sent } = await recordingClient(dir, home)
    const items = (await lfs.machine.list({ detail: true })).items
    const bad = items.find((m) => m.machine === 'bad')
    assert.deepEqual([bad.state, bad.reason, bad.platform, bad.fix], ['broken', 'failed', null, 'layerfs machine doctor bad'])
    assert.ok(items.some((m) => m.machine === 'native' && m.state === 'ready'), 'the other machines are listed')
    await assert.rejects(lfs.machine.get('bad', { detail: true, with: ['live'] }), (e) => e.code === 'Failed' && e.fix === 'layerfs machine doctor bad')
    for (const family of ['store', 'sandbox']) assert.ok(Array.isArray((await lfs[family].list({ detail: true })).items), `${family} list answers beside it`)
    const native = await lfs.machine.get('native', { detail: true, with: ['live'] })
    assert.equal(typeof native.live?.disk_free_bytes, 'number', 'native live answers disk_free_bytes')
    // the page's probe set (routes/machines.ts readView): every machine whose record reads
    const { unreadableRecord } = await import(pathToFileURL(join(root, 'src/host/layerfs/machine-records.ts')).href)
    assert.deepEqual(items.filter((m) => !unreadableRecord(m)).map((m) => m.machine).sort(), items.map((m) => m.machine).filter((n) => n !== 'bad').sort())
    assert.ok(sent().includes('machine get --detail --with live -- native'))
  } finally { rmSync(dir, { recursive: true, force: true }) }
})

// CUT (LayerFS a2a13a3892, owner 2026-11-19): old state is not read as before. A PostgreSQL
// Store recorded with no saved login is `broken` `outdated` and refused with the save-auth
// fix; the built-in docker and a created machine whose record has no vm_name are refused
// with `machine doctor docker` / `machine remove <m>`; `workspace exec` env is an object.
// Each fix line LayerFS answers is the page's one action (runnableFix), never a guess.
test('CUT old state: each refusal\'s fix is the page\'s one action; exec env is an object', { skip }, async () => {
  const dir = mkdtempSync(join(tmpdir(), 'dsh-cs20-cut-'))
  try {
    const home = join(dir, 'home')
    const env = { ...process.env, LAYERFS_HOME: home }
    execFileSync(CLI, ['home', 'init'], { env, timeout: 30_000 })
    const record = (kind, name, value) => {
      mkdirSync(join(home, kind, name), { recursive: true, mode: 0o700 })
      writeFileSync(join(home, kind, name, 'record.json'), JSON.stringify(value), { mode: 0o600 })
    }
    record('store', 'pg', { name: 'pg', kind: 'postgresql', url: 'postgresql://owner@db.invalid:5432/team', schema: 'layerfs', password: 'env:NOPE' })
    record('machine', 'docker', { v: 1, name: 'docker', platform: 'docker', created: 1795069751311 })
    record('machine', 'old', { v: 1, name: 'old', platform: 'lima', created: 1795069751311 })
    const out = join(dir, 'fix')
    await build({
      stdin: { contents: "export { runnableFix } from './src/shared/doctor.ts'\nexport { storeRow } from './src/host/routes/storage.ts'\n", resolveDir: root, loader: 'ts' },
      bundle: true, platform: 'node', format: 'esm', outfile: join(out, 'fix.mjs'), absWorkingDir: root, target: 'node24', logLevel: 'warning', packages: 'external',
    })
    const { runnableFix, storeRow } = await import(pathToFileURL(join(out, 'fix.mjs')).href)
    const { lfs, sent } = await recordingClient(dir, home)
    const pg = (await lfs.store.list({ detail: true, with: ['stats'] })).items.find((s) => s.store === 'pg')
    assert.deepEqual([pg.state, pg.reason, pg.auth], ['broken', 'outdated', null])
    assert.deepEqual(runnableFix(pg.fix), { kind: 'saveAuth', store: 'pg' })
    const row = storeRow(pg, [])
    assert.deepEqual([row.state, row.reason, row.postgresql.auth, row.fix], ['broken', 'outdated', null, pg.fix])
    await assert.rejects(lfs.store.compact('pg', { detail: true }), (e) => e.code === 'InvalidInput' && /names no saved login/.test(e.message) && runnableFix(e.fix)?.kind === 'saveAuth')
    // docker: `machine doctor docker` at a2a13a3892; the cut's review makes it `machine remove docker` (records only, the built-in comes back fresh)
    await assert.rejects(lfs.machine.stop('docker', {}), (e) => e.code === 'Failed' && /no vm_name/.test(e.message) && ['doctor', 'removeMachine'].includes(runnableFix(e.fix)?.kind) && runnableFix(e.fix).machine === 'docker')
    await assert.rejects(lfs.machine.stop('old', {}), (e) => e.code === 'Failed' && JSON.stringify(runnableFix(e.fix)) === JSON.stringify({ kind: 'removeMachine', machine: 'old' }))
    // that fix removes the record alone: no `forgotten` outcome
    const removed = await lfs.machine.remove('old', { detail: true })
    assert.deepEqual([removed.ok, removed.outcome], [true, 'removed'])
    // exec env: an object only; a list or a text is refused by the seam before spawning
    for (const bad of [['A=1'], 'A=1', { A: 1 }]) await assert.rejects(lfs.workspace.exec('box:ws', 'true', { env: bad }), (e) => e.code === 'InvalidInput' && /env is an object/.test(e.message))
    assert.deepEqual(sent().filter((l) => l.startsWith('workspace exec')), [], 'nothing spawned')
  } finally { rmSync(dir, { recursive: true, force: true }) }
})
