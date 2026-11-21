// The LayerFS home directories a bound session may write: its own Store,
// Sandbox and machine only.  node --test test/sandbox-policy.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { build } from 'esbuild'
import { mkdirSync, mkdtempSync, rmSync, statSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

const root = fileURLToPath(new URL('..', import.meta.url))
mkdirSync(join(root, 'lib'), { recursive: true })
const out = mkdtempSync(join(root, 'lib', '.test-sandbox-policy-'))
await build({ absWorkingDir: root, entryPoints: [join(root, 'src/host/sandbox-policy.ts')], outfile: join(out, 'policy.mjs'), bundle: true, format: 'esm', platform: 'node', target: 'node24', external: ['@deepseek-ai/*'], logLevel: 'warning' })
const { sessionHomeRoots } = await import(pathToFileURL(join(out, 'policy.mjs')).href)
test.after(() => rmSync(out, { recursive: true, force: true }))

test('a bound session writes only its own Store, Sandbox and machine in the LayerFS home', () => {
  const home = mkdtempSync(join(tmpdir(), 'dsh-policy-home-'))
  try {
    for (const dir of ['store/mine', 'store/other', 'sandbox/dsh-a', 'sandbox/dsh-b', 'machine/native', 'machine/vm']) mkdirSync(join(home, dir), { recursive: true })
    assert.deepEqual(sessionHomeRoots(home, { project: 'mine/app', sandbox: 'dsh-a', machine: 'native' }),
      [join(home, 'store/mine'), join(home, 'sandbox/dsh-a'), join(home, 'machine/native')])
    assert.deepEqual(sessionHomeRoots(home, { project: 'mine/app', sandbox: 'dsh-a' }).at(-1), join(home, 'machine/native'), 'no machine recorded means native')
    assert.deepEqual(sessionHomeRoots(home, { project: 'pg/app', sandbox: 'dsh-missing', machine: 'vm' }), [join(home, 'machine/vm')], 'missing directories are not granted')
    assert.deepEqual(sessionHomeRoots(home, { project: '../x', sandbox: '..', machine: '../../etc' }), [], 'path traversal is never granted')
    assert.deepEqual(sessionHomeRoots(undefined, { project: 'mine/app' }), [])
    assert.deepEqual(sessionHomeRoots(home, { project: 'mine/app', sandbox: 'dsh-a', machine: 'native' }, { createSandboxes: true }),
      [join(home, 'store/mine'), join(home, 'sandbox'), join(home, 'machine/native')], 'a session that may create Sandboxes gets the records folder')
    writeFileSync(join(home, 'layerfs.lock'), '')
    for (const opts of [{}, { createSandboxes: true }]) for (const root of sessionHomeRoots(home, { project: 'mine/app', sandbox: 'dsh-a', machine: 'native' }, opts))
      assert.ok(statSync(root).isDirectory(), `only directories are granted (DSH refuses file roots): ${root}`)
  } finally { rmSync(home, { recursive: true, force: true }) }
})
