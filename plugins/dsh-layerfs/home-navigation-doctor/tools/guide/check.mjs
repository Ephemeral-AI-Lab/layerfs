#!/usr/bin/env node
// Check the UI guide (docs/guide): every image a page names exists, every
// relative link and #anchor resolves, no image is unused, no failed shot is
// left, every page's component and modal tables have a "LayerFS call" column,
// and no page shows a token-looking string.
//   node tools/guide/check.mjs        (exit 1 and a list of problems when any)
import { fileURLToPath } from 'node:url'
import { existsSync, readdirSync, readFileSync } from 'node:fs'
import { join, dirname, resolve } from 'node:path'

const dir = fileURLToPath(new URL('../../docs/guide/', import.meta.url))
const pages = readdirSync(dir).filter((f) => f.endsWith('.md'))
const problems = []
const say = (file, what) => problems.push(`${file}: ${what}`)

/** GitHub-style heading anchors of a markdown file (with -1, -2 for repeats). */
function anchorsOf(text) {
  const seen = new Map()
  const out = new Set()
  for (const line of text.split('\n')) {
    const m = /^#{1,6}\s+(.*)$/.exec(line)
    if (!m) continue
    let slug = m[1].trim().toLowerCase().replace(/`/g, '').replace(/<[^>]+>/g, '').replace(/[^\p{L}\p{N}\s_-]/gu, '').replace(/\s/g, '-')
    const n = seen.get(slug) ?? 0
    seen.set(slug, n + 1)
    out.add(n ? `${slug}-${n}` : slug)
  }
  for (const m of text.matchAll(/<a (?:id|name)="([^"]+)"/g)) out.add(m[1])
  return out
}

const anchors = new Map(pages.map((p) => [p, anchorsOf(readFileSync(join(dir, p), 'utf8'))]))
const usedImages = new Set()
const GENERATED = new Set(['cli-routes.md'])
const NO_TABLE_RULE = new Set(['README.md', 'CONTRIBUTING.md', 'cli-map.md', 'cli-routes.md', 'components.md'])

for (const page of pages) {
  const text = readFileSync(join(dir, page), 'utf8')
  // strip fenced code: links inside are examples
  const body = text.replace(/```[\s\S]*?```/g, '')
  for (const m of body.matchAll(/!\[[^\]]*\]\(([^)\s]+)\)/g)) {
    const target = resolve(dir, m[1])
    usedImages.add(target)
    if (!existsSync(target)) say(page, `missing image ${m[1]}`)
  }
  for (const m of body.matchAll(/(?<!!)\[[^\]]*\]\(([^)\s]+)\)/g)) {
    const href = m[1]
    if (/^(https?:|mailto:)/.test(href)) continue
    const [path, hash] = href.split('#')
    const file = path ? resolve(dir, path) : join(dir, page)
    if (!existsSync(file)) { say(page, `broken link ${href}`); continue }
    // a screenshot linked from a table cell ("[menu](img/x.png)") counts as used
    if (file.endsWith('.png')) usedImages.add(file)
    const name = file.startsWith(dir) ? file.slice(dir.length) : null
    if (hash && name && anchors.has(name) && !anchors.get(name).has(hash)) say(page, `missing anchor ${href}`)
  }
  if (!NO_TABLE_RULE.has(page) && !GENERATED.has(page)) {
    const headers = body.split('\n').filter((l) => /^\|\s*(Component|Modal)\s*\|/.test(l))
    if (!headers.length) say(page, 'no component table')
    for (const h of headers) if (!/LayerFS call/.test(h)) say(page, `table without a LayerFS call column: ${h.slice(0, 80)}`)
  }
  if (/token=[A-Za-z0-9_-]{12,}|\b[A-Za-z0-9]{40,}\b(?<![0-9a-f]{40,})/.test(body)) say(page, 'a token-looking string')
}

const img = join(dir, 'img')
if (existsSync(img)) {
  for (const f of readdirSync(img)) {
    const full = join(img, f)
    if (f.startsWith('_failed-')) say('img', `failed shot left: ${f}`)
    else if (f.endsWith('.png') && !usedImages.has(full)) say('img', `unused image: ${f}`)
  }
}

if (problems.length) { console.log(problems.join('\n')); console.log(`\n${problems.length} problem(s)`); process.exit(1) }
console.log(`guide ok: ${pages.length} pages, ${usedImages.size} images`)
