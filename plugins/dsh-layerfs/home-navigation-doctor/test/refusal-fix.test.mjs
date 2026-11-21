// A refusal's `fix` line as the page's one action (review UR bug 1): which
// lines run, and the host call each makes (the route the page's own button takes).
//   node --test test/refusal-fix.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdirSync, mkdtempSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

const root = fileURLToPath(new URL('..', import.meta.url))
mkdirSync(join(root, 'lib'), { recursive: true })
const out = mkdtempSync(join(root, 'lib', '.test-refusal-fix-'))
await build({
  stdin: { contents: "export { fixCall } from './src/client/storage/fix-call.ts'\nexport { runnableFix } from './src/shared/doctor.ts'\n", resolveDir: root, loader: 'ts' },
  bundle: true, platform: 'node', format: 'esm', outfile: join(out, 'fix.mjs'), absWorkingDir: root, target: 'node24', logLevel: 'warning',
})
const M = await import(pathToFileURL(join(out, 'fix.mjs')).href)
process.on('exit', () => rmSync(out, { recursive: true, force: true }))

test('each runnable fix line makes the host call of the page\'s own button', async () => {
  const sent = []
  const api = { call: async (...a) => { sent.push(a); return {} }, op: async (...a) => { sent.push(['op', ...a]); return {} } }
  for (const line of [
    'layerfs machine start dev', 'layerfs machine stop dev', 'layerfs machine stop dev && layerfs machine start dev',
    'layerfs machine prune dev --unused', 'layerfs store compact shop', 'layerfs project delete shop/web',
    'layerfs backup create shop r2', 'layerfs home init', 'layerfs machine remove old',
    'layerfs store remove nokind', 'layerfs sandbox remove lfs-1a2b3c4d-box',
  ]) await M.fixCall(api, M.runnableFix(line))
  assert.deepEqual(sent, [
    ['POST', '/api/machines/dev/start', {}],
    ['POST', '/api/machines/dev/stop', {}],
    ['POST', '/api/machines/dev/restart', {}],
    ['POST', '/api/machines/dev/prune-images', { unused: true }],
    ['POST', '/api/stores/shop/compact', {}],
    ['DELETE', '/api/projects/shop%2Fweb'],
    ['POST', '/api/stores/shop/backup', { bucket: 'r2' }],
    ['op', 'setup.init'],
    ['DELETE', '/api/machines/old'],
    ['DELETE', '/api/stores/nokind?confirm=nokind'],
    ['DELETE', '/api/sandboxes/lfs-1a2b3c4d-box'],
  ])
})

// CUT (LayerFS 2.0, 2026-11-19): the fixes of old state the page offers as its one action.
test('the CUT fixes: a Store with no saved login opens the save-login dialog, docker\'s doctor runs, a record without vm_name is removed', async () => {
  assert.deepEqual(M.runnableFix('layerfs store save-auth <auth> --user <user> --password-stdin && layerfs store update compact-demo-pg --auth <auth>'), { kind: 'saveAuth', store: 'compact-demo-pg' })
  assert.deepEqual(M.runnableFix('layerfs machine doctor docker'), { kind: 'doctor', machine: 'docker' })
  assert.deepEqual(M.runnableFix('layerfs machine remove old'), { kind: 'removeMachine', machine: 'old' })
  // the cut's review: docker with no vm_name is reset by its remove; a Store with no kind and an lfs- Sandbox are removed (records only)
  assert.deepEqual(M.runnableFix('layerfs machine remove docker'), { kind: 'removeMachine', machine: 'docker' })
  assert.deepEqual(M.runnableFix('layerfs store remove nokind'), { kind: 'removeStore', store: 'nokind' })
  assert.deepEqual(M.runnableFix('layerfs sandbox remove lfs-1a2b3c4d-box'), { kind: 'removeSandbox', sandbox: 'lfs-1a2b3c4d-box' })
  // spec v23: a login the home lacks (`missing`) opens the save-login dialog for its Store (the person types the password there);
  // an old record (`outdated`) is Update, store update naming its own login again
  assert.deepEqual(M.runnableFix('layerfs store save-auth pg --user u --password-stdin && layerfs store update pg --auth pg'), { kind: 'saveAuth', store: 'pg' })
  assert.deepEqual(M.runnableFix('layerfs store update demo-experiments --auth demo-pg'), { kind: 'updateAuth', store: 'demo-experiments', auth: 'demo-pg' })
  // never run in part, never with a guessed login: a line whose names differ, or a longer one, stays a tip
  for (const line of [
    'layerfs store save-auth other --user u --password-stdin && layerfs store update pg --auth pg',
    'layerfs store save-auth <auth> --user <user> --password-stdin',
    'layerfs store update pg --auth <auth>',
    'layerfs machine remove old --force',
    'layerfs sandbox remove box --force',
    'layerfs store remove a b',
    'layerfs machine doctor',
    'mv /h /h.old && layerfs home init',
  ]) assert.equal(M.runnableFix(line), null, line)
  // the two that are Center's own are not host calls
  for (const fix of [{ kind: 'saveAuth', store: 'pg' }, { kind: 'doctor', machine: 'docker' }]) await assert.rejects(M.fixCall({ call: async () => assert.fail('no call'), op: async () => assert.fail('no op') }, fix))
})
