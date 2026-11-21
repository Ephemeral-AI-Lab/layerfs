// The `layerfs` Remote's events stream: one abort listener per stream, removed
// when the stream ends (no listener per wait).
//   node --test test/remote-events.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { build } from 'esbuild'
import { getEventListeners } from 'node:events'
import { mkdirSync, mkdtempSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

const root = fileURLToPath(new URL('..', import.meta.url))
mkdirSync(join(root, 'lib'), { recursive: true })
const out = mkdtempSync(join(root, 'lib', '.test-remote-'))
await build({ absWorkingDir: root, entryPoints: [join(root, 'src/host/remote.ts')], outfile: join(out, 'remote.mjs'), bundle: true, format: 'esm', platform: 'node', target: 'node24', external: ['@deepseek-ai/*', 'zod'], logLevel: 'warning' })
const { LayerfsRemote } = await import(pathToFileURL(join(out, 'remote.mjs')).href)
test.after(() => rmSync(out, { recursive: true, force: true }))

test('the events stream keeps one abort listener across many waits and removes it at the end', async () => {
  const remote = Object.create(LayerfsRemote.prototype)
  Object.defineProperty(remote, 'listeners', { value: new Set() })
  const controller = new AbortController()
  const stream = remote.events(controller.signal)
  assert.deepEqual((await stream.next()).value, { type: 'ready' })
  for (let i = 0; i < 20; i++) {
    const next = stream.next()
    await new Promise((resolve) => setImmediate(resolve))
    remote.emit('tick', i)
    assert.deepEqual((await next).value, { type: 'tick', data: i })
  }
  assert.equal(getEventListeners(controller.signal, 'abort').length, 1)
  const ending = stream.next()
  await new Promise((resolve) => setImmediate(resolve))
  controller.abort()
  assert.equal((await ending).done, true)
  assert.equal(getEventListeners(controller.signal, 'abort').length, 0)
  assert.equal(remote.listeners.size, 0)
})
