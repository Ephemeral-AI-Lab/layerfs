// Sandbox image archives read on the host (src/host/images/archive.ts): the
// Stage-0 source of a machine's ready images (docs/specs/sandbox-images.md §4.1).
//   node --test test/image-archives.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { execFileSync } from 'node:child_process'
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

const root = fileURLToPath(new URL('..', import.meta.url))
const out = mkdtempSync(join(tmpdir(), 'dsh-image-archives-'))
await build({
  absWorkingDir: root,
  stdin: { contents: "export * from './src/host/images/archive.ts'\n", resolveDir: root, loader: 'ts' },
  outfile: join(out, 'mod.mjs'), bundle: true, format: 'esm', platform: 'node', target: 'node24', logLevel: 'warning',
})
const M = await import(pathToFileURL(join(out, 'mod.mjs')).href)
test.after(() => rmSync(out, { recursive: true, force: true }))

const hex = (c) => c.repeat(64)
/** A docker-archive: `files` written under a folder, then tarred with the system tar. */
function archive(name, files) {
  const dir = mkdtempSync(join(out, 'src-'))
  for (const [path, body] of Object.entries(files)) {
    mkdirSync(join(dir, path, '..'), { recursive: true })
    writeFileSync(join(dir, path), typeof body === 'string' || Buffer.isBuffer(body) ? body : JSON.stringify(body))
  }
  const file = join(out, name)
  execFileSync('tar', ['-cf', file, '-C', dir, ...Object.keys(files)])
  return file
}

test('archive: legacy docker save layout, digest from the config file name', () => {
  const file = archive('legacy.tar', {
    [`${hex('a')}.json`]: { os: 'linux', architecture: 'arm64', created: '2026-01-01T00:00:00Z' },
    'manifest.json': [{ Config: `${hex('a')}.json`, RepoTags: ['web:1'], Layers: ['0/layer.tar'] }],
    '0/layer.tar': 'x'.repeat(3000),
  })
  const images = M.readArchive(file)
  assert.deepEqual(images, [{ digest: `sha256:${hex('a')}`, tags: ['web:1'], os: 'linux', architecture: 'arm64', created: '2026-01-01T00:00:00Z', layers: 1, size: 3000, problem: null }])
  // kept per (path, size, mtime): a second read parses nothing
  assert.equal(M.readArchive(file), images)
})

test('archive: OCI layout with several images in manifest order, long entry names skipped by offset', () => {
  const long = `layers/${'d'.repeat(120)}/layer.tar`
  const file = archive('oci.tar', {
    [`blobs/sha256/${hex('b')}`]: { os: 'linux', architecture: 'amd64' },
    [`blobs/sha256/${hex('c')}`]: { os: 'linux', architecture: 'arm64' },
    [long]: 'y'.repeat(700),
    'manifest.json': [
      { Config: `blobs/sha256/${hex('c')}`, RepoTags: ['api:2', 'api:latest'], Layers: [long] },
      { Config: `blobs/sha256/${hex('b')}`, Layers: [] },
      { Config: 'not-a-digest', Layers: [] },
    ],
  })
  const images = M.readArchive(file)
  assert.deepEqual(images.map((i) => [i.digest, i.tags, i.architecture]), [[`sha256:${hex('c')}`, ['api:2', 'api:latest'], 'arm64'], [`sha256:${hex('b')}`, [], 'amd64']])
})

test('archive: no manifest.json is refused', () => {
  const file = archive('plain.tar', { 'hello.txt': 'hi' })
  assert.throws(() => M.readArchive(file), /no manifest\.json/)
})

/** A ustar header for an empty entry of `type` (enough for the runnable scan). */
function tarHeader(name, size, type) {
  const b = Buffer.alloc(512)
  b.write(name, 0, 100, 'utf8'); b.write('0000644\0', 100); b.write('0000000\0', 108); b.write('0000000\0', 116)
  b.write(`${size.toString(8).padStart(11, '0')}\0`, 124); b.write('00000000000\0', 136); b.fill(0x20, 148, 156); b.write(type, 156); b.write('ustar\0', 257); b.write('00', 263)
  let sum = 0; for (const x of b) sum += x
  b.write(`${sum.toString(8).padStart(6, '0')}\0 `, 148)
  return b
}

/** An uncompressed layer tar of `entries` ([name, type]) built from ustar headers. */
const layerOf = (entries) => Buffer.concat([...entries.map(([name, type]) => tarHeader(name, 0, type)), Buffer.alloc(1024)])

test('runnable: a glibc loader runs; none (musl) or a device node does not', () => {
  const glibc = layerOf([['usr/lib/ld-linux-aarch64.so.1', '2'], ['etc/os-release', '0']])
  const musl = layerOf([['lib/ld-musl-aarch64.so.1', '0']])
  const device = layerOf([['dev/null', '3']])
  const file = archive('runnable.tar', {
    [`blobs/sha256/${hex('1')}`]: { os: 'linux', architecture: 'arm64' },
    [`blobs/sha256/${hex('2')}`]: { os: 'linux', architecture: 'arm64' },
    [`blobs/sha256/${hex('3')}`]: { os: 'linux', architecture: 'arm64' },
    [`blobs/sha256/${hex('4')}`]: { os: 'linux', architecture: 'amd64' },
    'l/glibc.tar': glibc, 'l/musl.tar': musl, 'l/device.tar': device,
    'manifest.json': [
      { Config: `blobs/sha256/${hex('1')}`, RepoTags: ['debian:12'], Layers: ['l/glibc.tar'] },
      { Config: `blobs/sha256/${hex('2')}`, RepoTags: ['alpine:3'], Layers: ['l/musl.tar'] },
      { Config: `blobs/sha256/${hex('3')}`, RepoTags: ['minio:1'], Layers: ['l/glibc.tar', 'l/device.tar'] },
      // an amd64 image needs the x86-64 loader, not aarch64's
      { Config: `blobs/sha256/${hex('4')}`, RepoTags: ['x86:1'], Layers: ['l/glibc.tar'] },
    ],
  })
  assert.deepEqual(M.readArchive(file).map((i) => [i.tags[0], i.problem]), [['debian:12', null], ['alpine:3', 'no-glibc'], ['minio:1', 'device-node'], ['x86:1', 'no-glibc']])
})
