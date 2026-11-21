// Static guard (spec docs/specs/backend-e2e-tests.md §5 V1): every LayerFS call
// the host makes exists in the pinned CLI's call table (`help --json`), with
// the options it passes. Scans src/host for SDK calls (`lfs.<family>.<call>(`,
// `lfs().…`, `this.lfs.…`) and for literal argv arrays (`['machine', 'stop', …, '--force']`
// handed to execute / the preset driver). Calls a CLI may lack are allowed only
// where a discovery gate guards them (GATED, checked to be present).
//   node --test test/call-table.test.mjs
import { fileURLToPath } from 'node:url'
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { execFileSync } from 'node:child_process'
import { mkdtempSync, readFileSync, readdirSync, rmSync, statSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, relative } from 'node:path'
import { pinned } from './routes-helper.mjs'

const root = fileURLToPath(new URL('..', import.meta.url))
const pin = pinned()

/** The pinned CLI's call table, read with a throwaway LAYERFS_HOME (never ~/.layerfs). */
function pinnedHelp() {
  const home = mkdtempSync(join(tmpdir(), 'dsh-call-table-'))
  try {
    return JSON.parse(execFileSync(pin.cli, ['help', '--json'], { env: { ...process.env, LAYERFS_HOME: home }, encoding: 'utf8', timeout: 15000 }))
  } finally { rmSync(home, { recursive: true, force: true }) }
}

// ---------------------------------------------------------------- source scanning

/** Index just past the string/template literal starting at `i`. */
function skipString(src, i) {
  const q = src[i]
  let j = i + 1
  while (j < src.length) {
    const c = src[j]
    if (c === '\\') { j += 2; continue }
    if (c === q) return j + 1
    if (q === '`' && c === '$' && src[j + 1] === '{') {
      let depth = 1
      j += 2
      while (j < src.length && depth) {
        const d = src[j]
        if (d === '"' || d === "'" || d === '`') { j = skipString(src, j); continue }
        if (d === '{') depth++
        else if (d === '}') depth--
        j++
      }
      continue
    }
    j++
  }
  return j
}

/** Index just past a regex literal at `i`, or i + 1 when it is not one. */
function skipRegex(src, i) {
  let j = i + 1, inClass = false
  while (j < src.length) {
    const c = src[j]
    if (c === '\n') return i + 1
    if (c === '\\') { j += 2; continue }
    if (c === '[') inClass = true
    else if (c === ']') inClass = false
    else if (c === '/' && !inClass) { j++; while (/[a-z]/.test(src[j] ?? '')) j++; return j }
    j++
  }
  return i + 1
}

/** The source with comments blanked (newlines kept, so line numbers hold). */
export function stripComments(src) {
  let out = '', i = 0, last = ''
  while (i < src.length) {
    const c = src[i], d = src[i + 1]
    if (c === '/' && d === '/') { while (i < src.length && src[i] !== '\n') { out += ' '; i++ } continue }
    if (c === '/' && d === '*') {
      const end = src.indexOf('*/', i + 2), stop = end < 0 ? src.length : end + 2
      out += src.slice(i, stop).replace(/[^\n]/g, ' ')
      i = stop
      continue
    }
    if (c === '"' || c === "'" || c === '`') { const j = skipString(src, i); out += src.slice(i, j); i = j; last = c; continue }
    if (c === '/' && (last === '' || '(,=:[!&|?{};+-*%<>~^'.includes(last) || /\b(return|typeof|in|of)$/.test(out.trimEnd()))) {
      const j = skipRegex(src, i); out += src.slice(i, j); i = j; last = '/'; continue
    }
    out += c
    if (!/\s/.test(c)) last = c
    i++
  }
  return out
}

/** Index of the bracket closing the one at `open`. */
function matchClose(src, open) {
  const pairs = { '(': ')', '[': ']', '{': '}' }
  const stack = []
  for (let i = open; i < src.length; i++) {
    const c = src[i]
    if (c === '"' || c === "'" || c === '`') { i = skipString(src, i) - 1; continue }
    if (pairs[c]) stack.push(pairs[c])
    else if (c === ')' || c === ']' || c === '}') { if (stack.pop() !== c) return -1; if (!stack.length) return i }
  }
  return -1
}

/** Top-level comma-separated pieces of an argument list. */
function splitArgs(text) {
  const out = []
  let depth = 0, start = 0
  for (let i = 0; i < text.length; i++) {
    const c = text[i]
    if (c === '"' || c === "'" || c === '`') { i = skipString(text, i) - 1; continue }
    if ('([{'.includes(c)) depth++
    else if (')]}'.includes(c)) depth--
    else if (c === ',' && depth === 0) { out.push(text.slice(start, i)); start = i + 1 }
  }
  if (text.slice(start).trim()) out.push(text.slice(start))
  return out.map((s) => s.trim()).filter(Boolean)
}

/** Identifier keys of the object literals in `text` (shorthand, `k: v`; spreads skipped, function bodies ignored). */
function objectKeys(text) {
  const keys = [], stack = []
  let i = 0, prev = ''
  while (i < text.length) {
    const c = text[i]
    if (c === '"' || c === "'" || c === '`') { i = skipString(text, i); prev = 'str'; continue }
    if (/\s/.test(c)) { i++; continue }
    if (text.startsWith('=>', i)) { prev = '=>'; i += 2; continue }
    if (text.startsWith('...', i)) { prev = '...'; i += 3; continue }
    if (c === '{') { stack.push(prev === '=>' || prev === ')' ? 'block' : 'obj'); prev = '{'; i++; continue }
    if (c === '(' || c === '[') { stack.push(c); prev = c; i++; continue }
    if (c === '}' || c === ')' || c === ']') { stack.pop(); prev = c; i++; continue }
    const id = /^[A-Za-z_$][\w$]*/.exec(text.slice(i))
    if (id) {
      if (stack.at(-1) === 'obj' && (prev === '{' || prev === ',')) {
        const after = /^\s*(.)/.exec(text.slice(i + id[0].length))?.[1]
        if (after === ':' || after === ',' || after === '}') keys.push(id[0])
      }
      prev = 'id'
      i += id[0].length
      continue
    }
    prev = c
    i++
  }
  return keys
}

const lineOf = (src, index) => src.slice(0, index).split('\n').length
/** SDK-only options (cli.ts `client`): never CLI flags, or the 2.0 global flags it writes itself (`detail` -> --detail, `with` -> --with, checked against the row's `form.with`). */
const SDK_LOCAL = new Set(['on_spawn', 'timeout_ms', 'signal', 'detail', 'with'])
/** Options the SDK rewrites into another CLI option (cli.ts `client`); none since the `{ archive, digest }` image went (sandbox-images.md §4.3). */
const SDK_TRANSLATIONS = []

/**
 * Every call the source makes that the call table lacks, as `{ site, file, line }`:
 * `site` is `family.call` (no such call), `family.call option x` (SDK option) or
 * `family.call --flag` (argv flag). `seen` counts the call sites checked.
 */
export function violations(files, help) {
  const calls = new Map(help.calls.map((c) => [`${c.family}.${c.call}`, c]))
  const families = new Set([...help.calls.map((c) => c.family), 'logs'])
  const switches = new Map()
  for (const s of help.switches ?? []) for (const call of s.calls) switches.set(call, [...(switches.get(call) ?? []), s.name])
  const found = []
  let seen = 0
  for (const { file, text } of files) {
    const src = stripComments(text)
    // SDK calls: lfs.<family>.<call>( with lfs, lfs(), this.lfs, host.lfs(), ctx.layerfs.lfs …
    for (const m of src.matchAll(/\blfs(?:\(\))?\??\.([a-z]+)\??\.([a-z_]+)\s*\(/g)) {
      seen++
      const key = `${m[1]}.${m[2]}`, line = lineOf(src, m.index)
      const entry = calls.get(key)
      if (!entry) { found.push({ site: key, file, line }); continue }
      const open = m.index + m[0].length - 1, close = matchClose(src, open)
      const args = close < 0 ? [] : splitArgs(src.slice(open + 1, close))
      const last = args.at(-1) ?? '', slot = entry.args[args.length - 1]
      const objectish = last.startsWith('{') || (/\?\s*\{/.test(last) && /:\s*\{[^]*\}\s*\)?$/.test(last))
      if (!(args.length > entry.args.length || (objectish && slot?.kind !== 'json'))) continue
      const names = new Set(entry.options.map((o) => o.name))
      for (const option of new Set(objectKeys(last))) {
        if (!names.has(option) && !SDK_LOCAL.has(option)) found.push({ site: `${key} option ${option}`, file, line })
        for (const t of SDK_TRANSLATIONS) if (t.call === key && t.key === option && !names.has(t.becomes)) found.push({ site: `${key} option ${t.becomes}`, file, line })
      }
    }
    // literal argv arrays: ['family', 'call', …, '--flag', …]
    for (const m of src.matchAll(/\[\s*(['"])([a-z]+)\1\s*,\s*(['"])([a-z][a-z_-]*)\3/g)) {
      if (!families.has(m[2]) || families.has(m[4])) continue
      seen++
      const key = `${m[2]}.${m[4].replaceAll('-', '_')}`, line = lineOf(src, m.index)
      const entry = calls.get(key)
      if (!entry) { found.push({ site: key, file, line }); continue }
      const close = matchClose(src, m.index)
      const body = close < 0 ? '' : src.slice(m.index, close + 1)
      for (const f of body.matchAll(/(['"])(--?[a-z][a-z-]*)\1/g)) {
        const flag = f[2]
        // an option by its flag, or a content option by its shell flag (`--password-stdin`, `--key-stdin`)
        if (entry.options.some((o) => `--${o.name.replaceAll('_', '-')}` === flag || o.shell === flag) || (switches.get(key) ?? []).includes(flag)) continue
        found.push({ site: `${key} ${flag}`, file, line })
      }
    }
    // the doctor report: a top-level command, not a call
    for (const m of src.matchAll(/\[\s*'doctor'\s*\]/g)) {
      seen++
      if (!(help.commands ?? []).some((c) => c.name === 'doctor')) found.push({ site: 'doctor', file, line: lineOf(src, m.index) })
    }
  }
  return { found, seen }
}

/**
 * Calls a pinned CLI may lack, sent only behind a discovery gate. Each must
 * still be gated: `gate` must match within `lines` lines above the call site.
 */
const GATED = [
  // machine.images (Stage 1 of sandbox-images.md): only in stage1Images, which sandboxImages calls only at imageStage 1 (checked below)
  { site: /^machine\.images$/, file: 'src/host/routes/machines.ts', gate: /cliFeatures\(this\.host\)\)\.(images|imageStage)\b|private async stage1Images\(/, lines: 16 },
  // machine.prune (Stage 1): Machines.pruneImages refuses 501 without imagePrune
  { site: /^machine\.prune$/, file: 'src/host/routes/machines.ts', gate: /cliFeatures\(this\.host\)\)\.imagePrune\b/, lines: 16 },
  // machine.stop --force: Machines.stop refuses 501 without forceStop
  { site: /^machine\.stop (option force|--force)$/, file: 'src/host/routes/machines.ts', gate: /cliFeatures\(this\.host\)\)\.forceStop\b/, lines: 3 },
  // the native call log: only when the CLI's table has home.log and home.clear_log (else the stand-in)
  { site: /^home\.(log|clear_log)$/, file: 'src/host/logs/source.ts', gate: /await layerfs\.nativeLogs\(\)/, lines: 20 },
  // the worker and daemon logs (layerfs-project#178): Machines.machineLog / sandboxLog refuse 501 without the call
  { site: /^machine\.log$/, file: 'src/host/routes/machines.ts', gate: /cliFeatures\(this\.host\)\)\.inspect\.machineLog\b/, lines: 4 },
  { site: /^sandbox\.log$/, file: 'src/host/routes/machines.ts', gate: /cliFeatures\(this\.host\)\)\.inspect\.sandboxLog\b/, lines: 4 },
  // the machine's and the Sandbox's files (layerfs-project#178): Machines.machineFiles / sandboxFiles refuse 501 without the call
  { site: /^machine\.files$/, file: 'src/host/routes/machines.ts', gate: /cliFeatures\(this\.host\)\)\.inspect\.machineFiles\b/, lines: 4 },
  { site: /^sandbox\.files$/, file: 'src/host/routes/machines.ts', gate: /cliFeatures\(this\.host\)\)\.inspect\.files\b/, lines: 4 },
  // a machine's observed resources (layerfs-project#164): Machines.machineStats refuses 501 without the call
  { site: /^machine\.stats$/, file: 'src/host/routes/machines.ts', gate: /cliFeatures\(this\.host\)\)\.inspect\.machineStats\b/, lines: 10 },
  // doctor.fix (layerfs-project#180) has no entry: this host never calls it (the Doctor page shows the command as text)
  // the home family (layerfs-setup.md §3): the setup check uses home.get / home.init only when the call table has both (else one machine.list)
  { site: /^home\.(get|init)$/, file: 'src/host/service.ts', gate: /this\.homeCalls\(\)/, lines: 8 },
  // GC (layerfs-project#41): the route refuses `Unsupported` when the call table has no store.compact, and the view hides the menu row
  { site: /^store\.compact$/, file: 'src/host/routes/storage.ts', gate: /const call = await this\.compactCall\(\)\n\s+if \(!call\) throw refuse\(409, 'Unsupported'/, lines: 6 },
  // GC (layerfs-project#41): project.delete replaced project.remove; Projects.remove sends the one the call table has
  { site: /^project\.(delete|remove)$/, file: 'src/host/routes/projects.ts', gate: /const hasDelete = \(await this\.host\.discover\(\)\)\.calls\.some\(/, lines: 3 },
  // the Doctor page (call surface 2.0): the records tree is read only when the call table lists `tree` among `home get`'s --with values; else the route refuses `Unsupported`
  { site: /^home\.get --(with|detail)$/, file: 'src/host/routes/doctor.ts', gate: /const hasTree = hasTreeRead\(\(await this\.host\.discover\(\)\)\.calls\)\n\s+if \(!hasTree\) throw refuse\(501, 'Unsupported'/, lines: 6 },
  // the preset Reset's force-close refuses up front (preflight) when machine.stop lacks the bool force option
  { site: /^machine\.stop --force$/, file: 'src/host/presets/machine-stop.js', gate: /option\.name === 'force' && option\.kind === 'bool'/, lines: 45 },
]

/**
 * Real bugs this guard found; reported by a todo test (they do not fail the run)
 * until fixed. Remove an entry once its call is gated or gone.
 */
const KNOWN_BUGS = []

/** Splits violations into gated (gate seen), known bugs, and failures. */
export function classify(found, files) {
  const byFile = new Map(files.map((f) => [f.file, stripComments(f.text).split('\n')]))
  const gated = [], known = [], failures = []
  for (const v of found) {
    const rule = GATED.find((g) => g.site.test(v.site) && g.file === v.file)
    if (rule) {
      const window = (byFile.get(v.file) ?? []).slice(Math.max(0, v.line - 1 - rule.lines), v.line).join('\n')
      if (rule.gate.test(window)) { gated.push({ ...v, rule }); continue }
    }
    if (KNOWN_BUGS.some((k) => k.site.test(v.site) && k.file === v.file)) { known.push(v); continue }
    failures.push(v)
  }
  return { gated, known, failures }
}

function sources() {
  const walk = (dir) => readdirSync(dir).flatMap((f) => {
    const p = join(dir, f)
    return statSync(p).isDirectory() ? walk(p) : /\.(ts|js|mjs)$/.test(f) ? [p] : []
  })
  return walk(join(root, 'src/host')).map((p) => ({ file: relative(root, p), text: readFileSync(p, 'utf8') }))
}

const skip = pin ? false : 'no pinned LayerFS CLI (set DSH_LAYERFS_CLI or install a runtime in ~/.layerfs/runtime)'
const help = pin ? pinnedHelp() : null
const files = sources()
const scan = help ? violations(files, help) : null
const result = scan ? classify(scan.found, files) : null
const show = (list) => list.map((v) => `  ${v.site}  (${v.file}:${v.line})`).join('\n')

// V1: a call or option the pinned CLI lacks fails here instead of on 3080 as usage-incompatible.
test('every LayerFS call and option in src/host exists in the pinned CLI call table', { skip }, () => {
  assert.ok(scan.seen > 100, `the scan found only ${scan.seen} call sites: the patterns no longer match the source`)
  assert.deepEqual(result.failures, [], `calls or options missing from the pinned CLI (${pin.cli}), not behind a discovery gate:\n${show(result.failures)}`)
})

// V1: each allow-listed call is still sent only behind its discovery gate, and the allow-list has no stale entry.
test('calls the pinned CLI may lack stay behind their discovery gates', { skip }, () => {
  for (const rule of GATED) {
    // a CLI that has the call needs no gate: the rule then has no sites
    const sites = scan.found.filter((v) => rule.site.test(v.site) && v.file === rule.file)
    for (const v of sites) assert.ok(result.gated.some((g) => g.site === v.site && g.line === v.line), `${v.site} at ${v.file}:${v.line} is not behind ${rule.gate}`)
  }
  // against the 2.0.0 table (no stop force, no logs family) these are gated sites; Stage 0 makes no machine.images call
  const lacks = (family, call, option) => { const c = help.calls.find((x) => x.family === family && x.call === call); return !c || (option && !c.options.some((o) => o.name === option)) }
  const expected = [lacks('machine', 'stop', 'force') && 'machine.stop option force',
    lacks('home', 'log') && 'home.log', lacks('home', 'clear_log') && 'home.clear_log',
    lacks('machine', 'log') && 'machine.log', lacks('sandbox', 'log') && 'sandbox.log',
    lacks('sandbox', 'files') && 'sandbox.files', lacks('machine', 'stats') && 'machine.stats',
    lacks('home', 'get') && 'home.get', lacks('home', 'init') && 'home.init'].filter(Boolean)
  for (const site of expected) assert.ok(result.gated.some((g) => g.site === site), `the scan no longer sees the gated ${site} call site`)
  // any SDK translation a rule relies on is still there
  const sdk = readFileSync(join(root, 'src/host/layerfs/cli.ts'), 'utf8')
  for (const t of SDK_TRANSLATIONS) assert.match(sdk, t.proof, `cli.ts no longer translates ${t.key} into ${t.becomes}: update SDK_TRANSLATIONS`)
  // the gates are discovery reads (featuresOf over the call table), not constants
  const util = readFileSync(join(root, 'src/host/routes/util.ts'), 'utf8')
  assert.match(util, /forceStop: option\('machine', 'stop', 'force'\)/)
  assert.match(util, /images = !!call\('machine', 'images'\)/)
  assert.match(util, /imageRefs = optionOf\('sandbox', 'create', 'image'\)\?\.kind === 'image'/)
  assert.doesNotMatch(util, /image_archive/)
  assert.match(util, /machineLog: !!call\('machine', 'log'\), sandboxLog: !!call\('sandbox', 'log'\)/)
  assert.match(util, /files: !!call\('sandbox', 'files'\), machineFiles: !!call\('machine', 'files'\), machineStats: !!call\('machine', 'stats'\)/)
  assert.doesNotMatch(util, /sandboxLive/, '2.0 sandbox list has no `live` option')
  assert.match(util, /machineSpace: withValues\(call\('machine', 'stats'\)\)\.includes\('space'\)/)
  // doctor.fix is never called from this host: the page shows the command, the person runs it
  for (const f of files) assert.doesNotMatch(stripComments(f.text), /\.doctor\??\.fix\s*\(|\[\s*'doctor'\s*,\s*'fix'/, `${f.file} calls doctor.fix`)
  // stage1Images (the only machine.images call) runs only when discovery says Stage 1
  const machines = readFileSync(join(root, 'src/host/routes/machines.ts'), 'utf8')
  assert.match(machines, /const stage = \(await cliFeatures\(this\.host\)\)\.imageStage\n/)
  assert.match(machines, /if \(stage === 1\) return this\.stage1Images\(/)
  assert.equal(machines.match(/this\.stage1Images\(/g).length, 1)
})

// Sanity (V1 as found): the guard flags machine.images when the gate is missing, and accepts it gated.
test('the guard would have caught an ungated machine.images', { skip }, () => {
  const file = 'src/host/routes/machines.ts'
  const ungated = [{ file, text: 'class M {\n  async images(name) {\n    return this.lfs.machine.images(name)\n  }\n}\n' }]
  const a = classify(violations(ungated, help).found, ungated)
  if (help.calls.some((c) => c.family === 'machine' && c.call === 'images')) return
  assert.deepEqual(a.failures.map((v) => v.site), ['machine.images'])
  const gated = [{ file, text: "class M {\n  async images(name) {\n    if (!(await cliFeatures(this.host)).images) return { mode: 'unsupported' }\n    return this.lfs.machine.images(name)\n  }\n}\n" }]
  assert.deepEqual(classify(violations(gated, help).found, gated).failures, [])
  // an unknown option in an object literal and an unknown argv flag are flagged too
  const options = [{ file: 'x.ts', text: "await lfs.branch.list(project, { page_size: 1, nope: true })\nrun(['store', 'list', '--bogus'])\n" }]
  assert.deepEqual(violations(options, help).found.map((v) => v.site).sort(), ['branch.list option nope', 'store.list --bogus'])
})

// KNOWN BUG list: each entry still reproduces (todo until the product code is fixed).
test('no known ungated calls the pinned CLI lacks', { skip, todo: !skip && result.known.length > 0 }, () => {
  assert.deepEqual(result.known, [], `KNOWN BUG: ungated calls the pinned CLI lacks:\n${show(result.known)}`)
})
