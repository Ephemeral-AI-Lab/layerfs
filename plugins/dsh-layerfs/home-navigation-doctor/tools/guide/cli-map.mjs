#!/usr/bin/env node
// Generate the UI guide's CLI appendix: every host route (src/host/routes/router.ts)
// with the LayerFS CLI calls its handler makes.
//   node tools/guide/cli-map.mjs > docs/guide/cli-routes.md
//
// The router is read line by line: each `if (...) return <group>.<method>(` line
// becomes one route (method + path from its conditions, the comment above it),
// and the handler's body in the route group's file is searched for LayerFS calls
// (`lfs.<family>.<call>(`, `lfs().<family>.<call>(`, `noCall('<family>.<call>'`),
// following `this.<helper>(` calls inside the same class three levels deep.
// A route whose handler makes no call is "host only" (memory, files, jobs).
import { fileURLToPath } from 'node:url'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'

const root = fileURLToPath(new URL('../..', import.meta.url))
const routes = join(root, 'src/host/routes')
const read = (f) => readFileSync(join(routes, f), 'utf8')

/** The router's receivers -> the file that holds their class. */
const RECEIVERS = {
  storage: 'storage.ts', projects: 'projects.ts', machines: 'machines.ts',
  'g.commands': 'commands.ts', 'g.checks': 'doctor.ts', 'g.sandboxCreates': 'sandbox-creates.ts',
  'machines.metrics': 'metrics.ts', 'machines.loads': 'image-loads.ts',
}

/**
 * Calls a handler makes through a helper outside its class, which the search
 * cannot follow, and handlers whose calls are misleading as found.
 */
const OVERRIDES = {
  'projects.sandbox': ['sandbox get'],
  'machines.moveStore': ['store move (as a Storage job: push, pull, or pull then push)'],
  'machines.machineStats': ['none: the metrics sampler\'s last `machine stats` read'],
}

/** Method bodies of one file: name -> body text (the first definition wins). */
function methodsOf(file) {
  const src = read(file)
  const out = new Map()
  const re = /\n  (?:(?:async|private|public|static|readonly|override|get)\s+)*([A-Za-z_$][\w$]*)\s*(?:<[^>\n]*>)?\s*\(/g
  const starts = [...src.matchAll(re)].map((m) => ({ name: m[1], at: m.index }))
  starts.forEach((s, i) => {
    if (!out.has(s.name)) out.set(s.name, src.slice(s.at, starts[i + 1]?.at ?? src.length))
  })
  return out
}

const FILES = new Map()
const methods = (file) => { if (!FILES.has(file)) FILES.set(file, methodsOf(file)); return FILES.get(file) }

/** LayerFS calls named in a body. */
function callsIn(body) {
  const found = new Set()
  for (const m of body.matchAll(/\blfs(?:\(\))?\??\.([a-z_]+)\??\.([a-z_]+)\s*\(/g)) found.add(`${m[1]} ${m[2]}`)
  for (const m of body.matchAll(/noCall\('([a-z_]+)\.([a-z_]+)'/g)) found.add(`${m[1]} ${m[2]}`)
  return found
}

/** Calls made by `name` in `file`, following this.<helper>( within the class. */
function callsOf(file, name, depth = 3, seen = new Set()) {
  const key = `${file}:${name}`
  if (seen.has(key)) return new Set()
  seen.add(key)
  const body = methods(file).get(name)
  if (!body) return new Set()
  const found = callsIn(body)
  if (depth > 0) {
    for (const m of body.matchAll(/this\.([A-Za-z_$][\w$]*)\s*\(/g)) {
      for (const c of callsOf(file, m[1], depth - 1, seen)) found.add(c)
    }
  }
  return found
}

/** One router condition list -> method and path. */
function pathOf(cond, block) {
  const all = `${block ?? ''} && ${cond}`
  if (/group === 'projects'/.test(all) && !/!id/.test(cond) && !/\bid\b/.test(cond)) cond = `id && ${cond}`
  return pathOfAll(`${block ?? ''} && ${cond}`)
}

function pathOfAll(all) {
  const method = /method === '(\w+)'/.exec(all)?.[1] ?? (/method !== 'GET'/.test(all) ? 'POST|DELETE' : 'GET')
  const group = /group === '([\w-]+)'/.exec(all)?.[1]
  const id = /\bid === '([\w-]+)'/.exec(all)?.[1] ?? (/(^|[^!\w.])id\b(?! ===)/.test(all.replace(/!id/g, '')) ? ':id' : null)
  const sub = /\bsub === '([\w-]+)'/.exec(all)?.[1] ?? (/(^|[^!\w])sub\b(?! ===)/.test(all.replace(/!sub/g, '')) ? ':sub' : null)
  return { method, path: ['/api', group, id, sub].filter(Boolean).join('/') }
}

const router = read('router.ts').split('\n')
const start = router.findIndex((l) => l.startsWith('export async function route('))
const rows = []
let comment = []
let block = null
let noId = false
for (let i = start + 1; i < router.length; i++) {
  const line = router[i]
  if (line.startsWith('}')) break
  if (/^ {4}if \(!id\) \{$/.test(line)) { noId = true; continue }
  if (noId && /^ {4}\}$/.test(line)) { noId = false; continue }
  const trimmed = line.trim()
  if (trimmed.startsWith('//')) { comment.push(trimmed.replace(/^\/\/\s?/, '')); continue }
  const opens = /^ {2}if \((.*)\) \{$/.exec(line)
  if (opens && !/return/.test(line)) {
    // a group block (`if (group === 'stores') {`) or a multi-line route
    const inner = router.slice(i + 1, router.findIndex((l, j) => j > i && /^ {2}\}/.test(l))).join('\n')
    if (!/method [!=]== '/.test(opens[1]) && /^\s+if \(.*\) return/m.test(inner)) { block = opens[1]; comment = []; continue }
    const { method, path } = pathOf(opens[1], null)
    const helpers = [...inner.matchAll(/projects\.(\w+)\(/g)].map((h) => h[1])
    const calls = callsIn(inner)
    for (const h of helpers) for (const c of callsOf('projects.ts', h)) calls.add(c)
    rows.push({ method, path, handler: 'router.ts (inline)', calls, note: comment.join(' ') })
    comment = []
    continue
  }
  if (/^ {2}\}/.test(line)) { block = null; continue }
  const m = /if \((.*)\) return (?:await )?([\w.]+?)\.(\w+)\(/.exec(line)
  if (!m) { if (trimmed && !trimmed.startsWith('const') && !trimmed.startsWith('if (projectId)')) comment = []; continue }
  const [, found, receiver, fn] = m
  const cond = noId ? `!id && ${found}` : found
  if (receiver === 'g.presets' || receiver === 'g.checks') {
    rows.push({ ...pathOf(cond, block), handler: receiver === 'g.presets' ? 'presets/manager.ts' : 'routes/doctor.ts', calls: new Set([receiver === 'g.presets' ? 'many (a recipe of calls)' : '<family> doctor, fix steps']), note: comment.join(' ') })
    comment = []
    continue
  }
  const file = RECEIVERS[receiver]
  const handler = file ? `${file.replace('.ts', '')}.${fn}` : `${receiver}.${fn}`
  const calls = OVERRIDES[handler] ? new Set(OVERRIDES[handler]) : receiver === 'projects.lfs.workspace' ? new Set([`workspace ${fn}`]) : file ? callsOf(file, fn) : new Set()
  rows.push({ ...pathOf(cond, block), handler, calls, note: comment.join(' ') })
  comment = []
}

// Storage writes read the Storage view again afterwards; list those reads once, not on every row.
const VIEW = callsOf('storage.ts', 'readView')
for (const r of rows) {
  if (!r.handler.startsWith('storage.') || ['storage.view', 'storage.check'].includes(r.handler)) continue
  const before = r.calls.size
  for (const c of VIEW) r.calls.delete(c)
  if (r.calls.size < before) r.reread = true
}

const esc = (s) => s.replace(/\|/g, '\\|')
const out = [
  '# Host routes and their LayerFS calls',
  '',
  '<!-- Generated by tools/guide/cli-map.mjs from src/host/routes. Do not edit by hand; rerun it. -->',
  '',
  'Every button in the UI calls one of these host routes (through the `http` operation), and the route runs the pinned `layerfs` CLI. The per-interaction view is [cli-map.md](cli-map.md); this table is the full list, generated from `src/host/routes/`.',
  '',
  `Storage writes marked ↻ then read the Storage view again (${[...VIEW].sort().map((c) => `\`${c.replace(/_/g, '-')}\``).join(', ')}).`,
  '',
  '| Route | Handler | LayerFS calls | Router note |',
  '|---|---|---|---|',
  ...rows.map((r) => `| \`${r.method} ${r.path}\` | \`${r.handler}\` | ${r.calls.size ? [...r.calls].sort().map((c) => c.includes(' (') || c.startsWith('none') ? c : `\`${c.replace(/_/g, '-')}\``).join(', ') : 'host only'}${r.reread ? ' ↻' : ''} | ${esc(r.note)} |`),
  '',
]
process.stdout.write(out.join('\n'))
