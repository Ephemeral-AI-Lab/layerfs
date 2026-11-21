// Starting the Docker Engine behind the built-in docker machine (src/host/routes/docker-engine.ts):
// which need it answers, which Engine owns the socket, the launch and its wait, and
// Machines.start launching it first on macOS only (src/host/routes/machines.ts);
// and the client's engineStart, which keeps Start on such a blocked row (src/client/machines/needs.ts).
//   node --test test/docker-engine.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

const root = fileURLToPath(new URL('..', import.meta.url))
const out = mkdtempSync(join(tmpdir(), 'dsh-docker-engine-'))
await build({
  absWorkingDir: root,
  entryPoints: { engine: 'src/host/routes/docker-engine.ts', machines: 'src/host/routes/machines.ts', needs: 'src/client/machines/needs.ts' },
  outdir: out, bundle: true, format: 'esm', platform: 'node', target: 'node24', logLevel: 'warning', outExtension: { '.js': '.mjs' },
  packages: 'external',
})
const { engineSocket, launcherOf, socketTarget, startEngine } = await import(pathToFileURL(join(out, 'engine.mjs')).href)
const { Machines } = await import(pathToFileURL(join(out, 'machines.mjs')).href)
const { engineStart, readinessOf } = await import(pathToFileURL(join(out, 'needs.mjs')).href)
process.on('exit', () => rmSync(out, { recursive: true, force: true }))

const SOCK = '/var/run/docker.sock'
const DESKTOP = '/Users/u/.docker/run/docker.sock'

/** Fake host: files that exist, links, a clock that sleep advances, and a socket that answers after `upAfter` pings. */
function fakeDeps({ files = [], links = {}, upAfter = Infinity, runFails = null } = {}) {
  const calls = { run: [], pings: 0 }
  let now = 0
  const deps = {
    exists: (p) => files.includes(p),
    readlink: (p) => links[p] ?? null,
    run: async (cmd, args) => { calls.run.push([cmd, ...args]); if (runFails) throw Object.assign(new Error('exit 1'), { stderr: runFails }) },
    ping: async () => ++calls.pings > upAfter,
    sleep: async (ms) => { now += ms },
    now: () => now,
    home: '/Users/u',
    path: ['/usr/bin'],
  }
  return { deps, calls }
}

test('engineSocket: only a docker row whose every need is its socket', () => {
  const row = (missing, platform = 'docker') => ({ platform, ready: missing.length === 0, missing })
  assert.equal(engineSocket(row(['host:/var/run/docker.sock'])), SOCK)
  assert.equal(engineSocket(row([])), null)
  assert.equal(engineSocket(row(['host:/var/run/docker.sock', 'bundle:linux-arm64'])), null)
  assert.equal(engineSocket(row(['tool:docker'])), null)
  assert.equal(engineSocket(row(['host:/dev/fuse'])), null)
  assert.equal(engineSocket(row(['host:/var/run/docker.sock'], 'lima')), null)
  assert.equal(engineSocket(undefined), null)
})

test('launcherOf: the socket’s link names the Engine', () => {
  const desktop = fakeDeps({ files: ['/Applications/Docker.app'], links: { [SOCK]: DESKTOP } }).deps
  assert.deepEqual(launcherOf(SOCK, desktop), { name: 'Docker Desktop', command: 'open', args: ['-g', '-a', 'Docker'] })
  const colima = fakeDeps({ files: ['/Applications/Docker.app', '/opt/homebrew/bin/colima'], links: { [SOCK]: '/Users/u/.colima/default/docker.sock' } }).deps
  assert.deepEqual(launcherOf(SOCK, colima), { name: 'colima', command: '/opt/homebrew/bin/colima', args: ['start'] })
  const profile = fakeDeps({ files: ['/usr/local/bin/colima'], links: { [SOCK]: '/Users/u/.colima/work/docker.sock' } }).deps
  assert.deepEqual(launcherOf(SOCK, profile).args, ['start', '--profile', 'work'])
  const orb = fakeDeps({ files: ['/Users/u/Applications/OrbStack.app'], links: { [SOCK]: '/Users/u/.orbstack/run/docker.sock' } }).deps
  assert.equal(launcherOf(SOCK, orb).name, 'OrbStack')
  // Docker Desktop's socket and no Docker.app: nothing here can serve it
  assert.equal(launcherOf(SOCK, fakeDeps({ files: ['/opt/homebrew/bin/colima'], links: { [SOCK]: DESKTOP } }).deps), null)
  // a plain socket (no link) and colima only: colima
  assert.equal(launcherOf(SOCK, fakeDeps({ files: ['/opt/homebrew/bin/colima'] }).deps).name, 'colima')
})

test('socketTarget follows relative and chained links', () => {
  const { deps } = fakeDeps({ links: { '/a/sock': '../b/sock', '/b/sock': '/c/real.sock' } })
  assert.equal(socketTarget('/a/sock', deps), '/c/real.sock')
})

test('startEngine: an Engine that answers already is left alone', async () => {
  const { deps, calls } = fakeDeps({ upAfter: 0 })
  const reports = []
  assert.deepEqual(await startEngine(SOCK, deps, (r) => reports.push(r)), { started: false, engine: null })
  assert.deepEqual(calls.run, [])
  assert.deepEqual(reports, [])
})

test('startEngine: launches Docker Desktop and waits until it answers', async () => {
  const { deps, calls } = fakeDeps({ files: ['/Applications/Docker.app'], links: { [SOCK]: DESKTOP }, upAfter: 5 })
  const reports = []
  assert.deepEqual(await startEngine(SOCK, deps, (r) => reports.push(r)), { started: true, engine: 'Docker Desktop' })
  assert.deepEqual(calls.run, [['open', '-g', '-a', 'Docker']])
  assert.deepEqual(reports.map((r) => r.state), ['started', 'completed'])
  assert.ok(reports.every((r) => r.stage === 'docker-engine'))
})

test('startEngine: no Engine installed is Unsupported, and nothing runs', async () => {
  const { deps, calls } = fakeDeps({ links: { [SOCK]: DESKTOP } })
  await assert.rejects(startEngine(SOCK, deps, () => {}), (e) => e.code === 'Unsupported' && /Install Docker Desktop/.test(e.message))
  assert.deepEqual(calls.run, [])
})

test('startEngine: an Engine that never answers is a Timeout, reported failed', async () => {
  const { deps } = fakeDeps({ files: ['/Applications/Docker.app'], links: { [SOCK]: DESKTOP } })
  const reports = []
  await assert.rejects(startEngine(SOCK, deps, (r) => reports.push(r), 10_000), (e) => e.code === 'Timeout' && /Docker Desktop did not come up in 10 s/.test(e.message))
  assert.deepEqual(reports.map((r) => r.state), ['started', 'failed'])
})

test('startEngine: a launcher that fails says why', async () => {
  const { deps } = fakeDeps({ files: ['/opt/homebrew/bin/colima'], links: { [SOCK]: '/Users/u/.colima/default/docker.sock' }, runFails: 'FATA[0001] error starting vm' })
  await assert.rejects(startEngine(SOCK, deps, () => {}, 10_000), (e) => e.code === 'Unavailable' && /colima did not start: FATA\[0001\] error starting vm/.test(e.message))
})

/** Machines over a fake SDK: docker stopped, its platform row missing only the socket on `host`. */
function machinesOn(host, { missing = ['host:/var/run/docker.sock'] } = {}) {
  const log = []
  const events = []
  const lfs = {
    // the 2.0 calls: machine.get --detail names the platform; home.get --with platforms the host and its rows
    machine: {
      list: async () => ({ items: [{ machine: 'docker', platform: 'docker', state: 'stopped' }, { machine: 'vm', platform: 'lima', state: 'stopped' }] }),
      get: async (name) => ({ machine: name, platform: name === 'docker' ? 'docker' : 'lima', state: 'stopped' }),
      start: async (name) => { log.push(`machine.start ${name}`); return { ok: true, machine: name } },
    },
    home: {
      get: async () => ({ state: 'ready', platforms: { host, items: [{ platform: 'docker', ready: missing.length === 0, missing }] } }),
    },
  }
  const machines = new Machines({ lfs: () => lfs, emit: (type, data) => events.push(data), discover: async () => ({}) })
  machines.metrics.relist = () => {}
  const { deps, calls } = fakeDeps({ files: ['/Applications/Docker.app'], links: { [SOCK]: DESKTOP }, upAfter: 2 })
  deps.run = async (cmd, args) => { calls.run.push([cmd, ...args]); log.push('open Docker') }
  machines.engineDeps = () => deps
  return { machines, log, events }
}

test('Machines.start on macOS launches the Engine before machine.start, and reports its stage', async () => {
  const { machines, log, events } = machinesOn('macos')
  await machines.start('docker')
  assert.deepEqual(log, ['open Docker', 'machine.start docker'])
  assert.deepEqual(events.map((e) => `${e.progress.stage}:${e.progress.state}`), ['docker-engine:started', 'docker-engine:completed'])
})

test('Machines.start launches nothing on Linux, for another machine, or when more is missing', async () => {
  for (const [host, name, opts] of [['linux', 'docker', {}], ['macos', 'vm', {}], ['macos', 'docker', { missing: ['host:/var/run/docker.sock', 'bundle:x'] }]]) {
    const { machines, log } = machinesOn(host, opts)
    await machines.start(name)
    assert.deepEqual(log, [`machine.start ${name}`], `${host} ${name}`)
  }
})

test('client engineStart: Start stays on a docker row blocked only by its socket, on macOS', () => {
  const block = (missing, os = 'macos') => readinessOf({ platform: 'docker', ready: false, missing }, os)
  const docker = { platform: 'docker' }
  assert.equal(engineStart(docker, block(['host:/var/run/docker.sock']), 'macos'), true)
  assert.equal(engineStart(docker, block(['host:/var/run/docker.sock'], 'linux'), 'linux'), false)
  assert.equal(engineStart(docker, block(['tool:docker']), 'macos'), false)
  assert.equal(engineStart(docker, block(['host:/var/run/docker.sock', 'bundle:linux-arm64']), 'macos'), false)
  assert.equal(engineStart({ platform: 'lima' }, block(['host:/var/run/docker.sock']), 'macos'), false)
  assert.equal(engineStart(docker, null, 'macos'), false)
})
