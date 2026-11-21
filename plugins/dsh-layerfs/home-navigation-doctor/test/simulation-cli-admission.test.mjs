import { test } from 'node:test'
import assert from 'node:assert/strict'
import { build } from 'esbuild'
import { mkdtempSync, rmSync, writeFileSync, readFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { pathToFileURL } from 'node:url'

test('cancel after queued CLI admission prevents the mutation process from spawning and releases its slot', { timeout: 10000 }, async t => {
  const directory = mkdtempSync(join(tmpdir(), 'dsh-cli-admission-'))
  t.after(() => rmSync(directory, { recursive: true, force: true }))
  await build({ stdin: { contents: "export { execute } from './src/host/layerfs/cli.ts'", resolveDir: process.cwd(), loader: 'ts' }, outfile: join(directory, 'cli.mjs'), bundle: true, platform: 'node', format: 'esm', target: 'node24' })
  const { execute } = await import(pathToFileURL(join(directory, 'cli.mjs')))
  const binary = join(directory, 'fake-cli.mjs'), log = join(directory, 'spawned.jsonl'), release = join(directory, 'release')
  writeFileSync(binary, `#!/usr/bin/env node\nimport {appendFileSync,existsSync} from 'node:fs';\nappendFileSync(process.env.ADMISSION_LOG,JSON.stringify(process.argv[2])+'\\n');\nif(process.argv[2]==='hold'){const timer=setInterval(()=>{if(existsSync(process.env.ADMISSION_RELEASE)){clearInterval(timer);console.log('{}')}},5)}else console.log('{}');\n`, { mode: 0o700 })
  const spec = { cli: binary, home: directory }
  const options = { lane: { name: 'admission-proof', limit: 1 }, env: { ADMISSION_LOG: log, ADMISSION_RELEASE: release } }
  let announceSpawn
  const spawned = new Promise(resolve => { announceSpawn = resolve })
  const first = execute(spec, ['hold'], { ...options, onSpawn: announceSpawn })
  await spawned
  const controller = new AbortController()
  let beforeSpawn = false, mutationSpawned = false
  const queued = execute(spec, ['mutate'], { ...options, beforeSpawn: () => { beforeSpawn = true; controller.signal.throwIfAborted() }, onSpawn: () => { mutationSpawned = true } })
  const refusal = assert.rejects(queued, /cancelled while queued/)
  assert.equal(beforeSpawn, false, 'second command is waiting behind active admission')
  controller.abort(new Error('cancelled while queued'))
  writeFileSync(release, '')
  await first
  await refusal
  assert.equal(beforeSpawn, true, 'cancellation is rechecked only after the queued command is admitted')
  assert.equal(mutationSpawned, false)
  await execute(spec, ['next'], options)
  assert.deepEqual(readFileSync(log, 'utf8').trim().split('\n').map(JSON.parse), ['hold', 'next'])
})
