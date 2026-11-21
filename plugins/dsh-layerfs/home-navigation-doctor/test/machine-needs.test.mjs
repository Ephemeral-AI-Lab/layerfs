// The readiness ladder (src/client/machines/needs.ts): each LayerFS need sorted into the
// step that fixes it on this computer's OS, a platform's hardest step, and the host's
// own name for this computer (src/host/routes/computer.ts).
//   node --test test/machine-needs.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

const root = fileURLToPath(new URL('..', import.meta.url))
const out = mkdtempSync(join(tmpdir(), 'dsh-machine-needs-'))
await build({
  absWorkingDir: root, entryPoints: { needs: 'src/client/machines/needs.ts', computer: 'src/host/routes/computer.ts' }, outdir: out,
  bundle: true, format: 'esm', platform: 'node', target: 'node24', logLevel: 'warning', outExtension: { '.js': '.mjs' },
})
const { readinessOf, blockOf, fixOf, setupOf, machineBlock, engineStart, shortVersion, nativeGlyphOs, nativeNameKey } = await import(pathToFileURL(join(out, 'needs.mjs')).href)
const { osOf, prettyName } = await import(pathToFileURL(join(out, 'computer.mjs')).href)
process.on('exit', () => rmSync(out, { recursive: true, force: true }))

const row = (platform, missing = [], extra = {}) => ({ platform, ready: missing.length === 0, missing, how: 'created', version: null, ...extra })

test('a ready platform needs nothing', () => {
  assert.deepEqual(readinessOf(row('lima'), 'macos'), { step: 'ready', fixes: [] })
})

test('a missing tool is an install step with this OS’s commands', () => {
  const r = readinessOf(row('lima', ['tool:limactl']), 'macos')
  assert.equal(r.step, 'install')
  assert.deepEqual(r.fixes[0].commands, ['brew install lima'])
  assert.equal(r.fixes[0].why.values.tool, 'Lima')
  const docker = readinessOf(row('docker', ['tool:docker']), 'linux')
  assert.equal(docker.step, 'install')
  assert.ok(docker.fixes[0].commands.some((c) => c.includes('systemctl')), 'Linux gets the Docker service, not Homebrew')
})

test('a host fact no install fixes wins: Lima on Linux is unsupported, not "install Lima"', () => {
  const r = readinessOf(row('lima', ['tool:limactl', 'host:macos']), 'linux')
  assert.equal(r.step, 'unsupported')
  assert.equal(r.fixes[0].need, 'host:macos')
  // a tool LayerFS runs only on macOS is unsupported on another OS even when LayerFS reports just the tool
  assert.equal(readinessOf(row('lima', ['tool:limactl']), 'linux').step, 'unsupported')
})

test('Windows: native and Docker through WSL2 are unsupported', () => {
  assert.equal(readinessOf(row('native', ['host:macos-or-linux']), 'windows').step, 'unsupported')
  assert.equal(readinessOf(row('docker', ['host:wsl2-worker']), 'windows').step, 'unsupported')
})

test('Linux native: root, /dev/fuse and cgroup v2 are setup steps', () => {
  const r = readinessOf(row('native', ['host:root', 'host:/dev/fuse', 'host:cgroup2']), 'linux')
  assert.equal(r.step, 'setup')
  assert.deepEqual(r.fixes.map((f) => f.need), ['host:root', 'host:/dev/fuse', 'host:cgroup2'])
  assert.deepEqual(fixOf('host:/dev/fuse', 'native', 'linux').commands, ['sudo modprobe fuse'])
})

test('Docker’s socket that does not answer is a start step, per OS', () => {
  const mac = readinessOf(row('docker', ['host:/var/run/docker.sock']), 'macos')
  assert.equal(mac.step, 'start')
  assert.equal(mac.fixes[0].why.values.socket, '/var/run/docker.sock')
  assert.ok(mac.fixes[0].commands.includes('open -a Docker'))
  assert.deepEqual(readinessOf(row('docker', ['host:/var/run/docker.sock']), 'linux').fixes[0].commands, ['sudo systemctl start docker'])
})

test('steps order the fixes: install before start before runtime', () => {
  // `bundle:` from an older CLI names a platform, not a folder
  const r = readinessOf(row('docker', ['bundle:linux-arm64', 'host:/var/run/docker.sock', 'tool:docker']), 'macos')
  assert.equal(r.step, 'install')
  assert.deepEqual(r.fixes.map((f) => f.step), ['install', 'start', 'runtime'])
})

test('the Linux bundle is fixed in setup, in the folder LayerFS named', () => {
  const r = readinessOf(row('lima', ['bundle:/Users/me/.cache/layerfs/bundle/2.0.0/linux-aarch64']), 'macos')
  assert.equal(r.step, 'runtime')
  assert.equal(setupOf(r), 'runtime', 'its button opens the setup modal at Runtime')
  assert.deepEqual(r.fixes[0].how, { key: 'fixBundleIn', values: { folder: '/Users/me/.cache/layerfs/bundle/2.0.0/linux-aarch64' } })
  assert.deepEqual(r.fixes[0].commands, [], 'no dev commands: the setup modal is the fix')
  // an older CLI names no folder: the one setup.status resolved, else none (never a guessed bundle/<version>/)
  assert.deepEqual(readinessOf(row('lima', ['bundle:linux-arm64']), 'macos', '2.0.0', { bundleDir: '/opt/bundle' }).fixes[0].how, { key: 'fixBundleIn', values: { folder: '/opt/bundle' } })
  assert.deepEqual(fixOf('bundle:linux-arm64', 'lima', 'macos').how, { key: 'fixBundle' })
  // a harder step leads: the bundle fix stays in the panel, the row's button is not the setup modal's
  assert.equal(setupOf(readinessOf(row('lima', ['tool:limactl', 'bundle:/b']), 'macos')), null)
})

test('docker’s socket: denied is the docker group, not Start; other sockets link to Settings', () => {
  const socket = { path: '/var/run/docker.sock', source: 'default', state: 'denied' }
  const denied = row('docker', ['host:/var/run/docker.sock'], { socket, candidates: [{ path: '/var/run/docker.sock', source: 'default' }] })
  const linux = readinessOf(denied, 'linux')
  assert.equal(linux.step, 'setup')
  assert.deepEqual(linux.fixes[0].why, { key: 'needDockerDenied', values: { socket: '/var/run/docker.sock' } })
  assert.deepEqual(linux.fixes[0].commands, ['sudo usermod -aG docker $USER'])
  assert.equal(linux.fixes[0].socket, true)
  assert.equal(engineStart({ platform: 'docker' }, readinessOf(denied, 'macos'), 'macos'), false, 'Start cannot open a socket this user may not open')
  const silent = readinessOf({ ...denied, socket: { ...socket, state: 'silent' } }, 'macos')
  assert.equal(silent.step, 'start')
  assert.equal(silent.fixes[0].socket, true)
  // an older CLI: no candidates, no link
  assert.equal(readinessOf(row('docker', ['host:/var/run/docker.sock']), 'macos').fixes[0].socket, undefined)
})

test('machineBlock reads a view without a LayerFS version (New project’s tiles)', () => {
  const view = { platforms: [row('docker', ['host:/var/run/docker.sock'])], computer: { os: 'linux', name: null, arch: 'x86_64' } }
  assert.equal(machineBlock({ platform: 'docker' }, view).step, 'start')
  assert.equal(machineBlock({ platform: 'native' }, view), null)
})

test('an unknown need stays in view in LayerFS’s words', () => {
  const r = readinessOf(row('lima', ['host:kvm']), 'linux')
  assert.equal(r.step, 'setup')
  assert.deepEqual(r.fixes[0].why, { key: 'needHost', values: { thing: 'kvm' } })
  assert.equal(readinessOf(row('lima', [], { ready: false }), 'macos').step, 'setup', 'not ready with no reason is not ready')
})

test('blockOf: a machine is blocked only while its platform is not ready', () => {
  const platforms = [row('docker', ['host:/var/run/docker.sock'], { how: 'built_in' }), row('lima')]
  assert.equal(blockOf(platforms, 'docker', 'macos').step, 'start')
  assert.equal(blockOf(platforms, 'lima', 'macos'), null)
  assert.equal(blockOf(platforms, 'microsandbox', 'macos'), null, 'a platform LayerFS did not list blocks nothing')
})

test('versions are shown short', () => {
  assert.equal(shortVersion('container CLI version 1.6.1 (build: release, commit: unspeci)'), '1.6.1')
  assert.equal(shortVersion('limactl version 2.2.1'), '2.2.1')
  assert.equal(shortVersion('29.5.2'), '29.5.2')
  assert.equal(shortVersion('dev'), 'dev')
  assert.equal(shortVersion(null), null)
})

test('the native machine is named and marked by this computer’s OS', () => {
  assert.deepEqual(['macos', 'linux', 'windows', 'other'].map(nativeNameKey), ['nativeMac', 'nativeComputer', 'nativePc', 'nativeComputer'])
  assert.deepEqual(['macos', 'linux', 'windows'].map(nativeGlyphOs), ['mac', 'linux', 'pc'])
})

test('host: LayerFS’s and Node’s OS words, and an os-release name', () => {
  assert.deepEqual(['macos', 'darwin', 'linux', 'windows', 'win32', 'freebsd', null].map(osOf), ['macos', 'macos', 'linux', 'windows', 'windows', 'other', 'other'])
  assert.equal(prettyName('NAME="Ubuntu"\nPRETTY_NAME="Ubuntu 24.04.1 LTS"\nID=ubuntu\n'), 'Ubuntu 24.04.1 LTS')
  assert.equal(prettyName('ID=alpine\n'), null)
})
