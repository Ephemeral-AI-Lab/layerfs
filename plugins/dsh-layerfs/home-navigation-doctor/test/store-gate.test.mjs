// One LayerFS CLI process per Store at a time (cli.ts `storeOfCall` + the Store gate):
// two processes on one Store fail in LayerFS (a lease claim racing an exiting holder's
// lease removal: `Os NotFound`; on macOS a second SQLite opener: `Busy`), so the host
// runs reads and writes of one Store one after another, and other Stores alongside.
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { build } from 'esbuild'
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { pathToFileURL } from 'node:url'

const directory = mkdtempSync(join(tmpdir(), 'dsh-store-gate-'))
await build({ stdin: { contents: "export { execute, storeOfCall } from './src/host/layerfs/cli.ts'", resolveDir: process.cwd(), loader: 'ts' }, outfile: join(directory, 'cli.mjs'), bundle: true, platform: 'node', format: 'esm', target: 'node24' })
const { execute, storeOfCall } = await import(pathToFileURL(join(directory, 'cli.mjs')))
test.after(() => rmSync(directory, { recursive: true, force: true }))

test('storeOfCall names the Store a call opens, in both argv shapes', () => {
  assert.equal(storeOfCall(['file', 'write', 'demo-studio/website', 'b', '/x.md', '--message', 'm']), 'demo-studio')
  assert.equal(storeOfCall(['branch', 'fork', '--', 'demo-build-cache/compiler', 'f', 'main']), 'demo-build-cache')
  assert.equal(storeOfCall(['branch', 'list', '--limit', '100', '--', 'rs/a']), 'rs')
  assert.equal(storeOfCall(['commit', 'list', '--detail', '--', 'rs/a', 'main']), 'rs')
  // 2.0 `store get --with stats` opens the Store; `store get` alone reads its record
  assert.equal(storeOfCall(['store', 'get', '--with', 'stats', '--', 'rs']), 'rs')
  assert.equal(storeOfCall(['store', 'get', 'rs', '--with', 'stats']), 'rs', 'the preset driver\'s shape: positionals first')
  assert.equal(storeOfCall(['store', 'get', '--detail', '--', 'rs']), undefined, 'reads the record only')
  assert.equal(storeOfCall(['store', 'list']), undefined)
  assert.equal(storeOfCall(['project', 'list', '--limit', '1000']), undefined)
  assert.equal(storeOfCall(['machine', 'get', 'demo-build']), undefined)
  assert.equal(storeOfCall(['workspace', 'exec', '--', 'sbx', 'ls']), undefined)
})

test('calls on one Store never overlap; other Stores run alongside', { timeout: 20000 }, async () => {
  const home = join(directory, 'home'), log = join(directory, 'spans.ndjson'), binary = join(directory, 'fake-cli.mjs')
  mkdirSync(home); writeFileSync(join(home, 'layerfs.json'), '{}')
  // Each call records its span; it sleeps so overlaps would show.
  writeFileSync(binary, `#!/usr/bin/env node
import fs from 'node:fs'; const argv = process.argv.slice(2)
const start = Date.now(); const until = start + 60; while (Date.now() < until) {}
fs.appendFileSync(${JSON.stringify(log)}, JSON.stringify({ argv, start, end: Date.now() }) + '\\n')
console.log(JSON.stringify({ ok: true }))
`, { mode: 0o700 })
  const spec = { cli: binary, home }
  await Promise.all([
    execute(spec, ['file', 'write', 'a/p', 'b', '/x.md'], { input: 'x' }),
    execute(spec, ['store', 'get', '--with', 'stats', '--', 'a'], { read: true }),
    execute(spec, ['branch', 'list', '--', 'a/q'], { read: true }),
    execute(spec, ['file', 'write', 'a/q', 'b', '/y.md'], { input: 'y' }),
    execute(spec, ['file', 'write', 'b/p', 'b', '/x.md'], { input: 'z' }),
  ])
  const spans = readFileSync(log, 'utf8').trim().split('\n').map(JSON.parse)
  const of = (store) => spans.filter((s) => storeOfCall(s.argv) === store).sort((x, y) => x.start - y.start)
  const a = of('a')
  assert.equal(a.length, 4)
  for (let i = 1; i < a.length; i++) assert.ok(a[i].start >= a[i - 1].end, `Store a calls overlap: ${JSON.stringify(a)}`)
  const [b] = of('b')
  assert.ok(a.some((s) => s.start < b.end && b.start < s.end), 'a call on Store b ran alongside Store a')
})
