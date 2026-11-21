// The stand-in CLIs for the files calls and the monitoring fields
// (test/fixtures/cli-inspect-files.mjs, cli-inspect-fields.mjs, chained by
// cli-standin-lib.mjs): they must answer as layerfs-project#178 describes, or
// the UI checks run against them check nothing. Never evidence about a real build.
//   node --test test/inspect-standin.test.mjs
import { fileURLToPath } from 'node:url'
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { spawn, spawnSync } from 'node:child_process'
import { chmodSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, symlinkSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

const filesShim = fileURLToPath(new URL('./fixtures/cli-inspect-files.mjs', import.meta.url))
const fieldsShim = fileURLToPath(new URL('./fixtures/cli-inspect-fields.mjs', import.meta.url))

/** A fixture folder, a tree that is a target's `/`, and a last CLI that answers `help --json`, `sandbox get` and `workspace exec`. */
function fixture(targets = {}) {
  const dir = mkdtempSync(join(tmpdir(), 'dsh-standin-'))
  const root = join(dir, 'root')
  const put = (path, text = '', mode) => { mkdirSync(join(root, path, '..'), { recursive: true }); writeFileSync(join(root, path), text); if (mode !== undefined) chmodSync(join(root, path), mode) }
  const last = join(dir, 'last-cli.mjs')
  writeFileSync(last, `#!/usr/bin/env node
import { appendFileSync } from 'node:fs'
const a = process.argv.slice(2)
appendFileSync(${JSON.stringify(join(dir, 'last.log'))}, JSON.stringify({ argv: a, real: process.env.LFS_REAL_CLI ?? null, rest: process.env.LFS_REAL_CLI_REST ?? null }) + '\\n')
if (a[0] === 'help') console.log(JSON.stringify({ calls: [{ family: 'sandbox', call: 'get' }] }))
else if (a[0] === 'sandbox' && a[1] === 'get') console.log(JSON.stringify({ name: a.at(-1), state: 'running', workspaces: [{ ws: a.at(-1) + ':one', name: 'one', directory: ${JSON.stringify(join(dir, 'run', 'workspaces', '1'))} }, { ws: a.at(-1) + ':two', name: 'two', directory: ${JSON.stringify(join(dir, 'run', 'workspaces', '2'))} }] }))
else if (a[0] === 'workspace' && a[1] === 'exec') { await new Promise((r) => setTimeout(r, Number(process.env.LAST_SLEEP ?? 0))); console.log('ran'); }
else { console.error(JSON.stringify({ error: 'Unsupported', message: 'no such call' })); process.exit(125) }
`)
  chmodSync(last, 0o755)
  const write = (config) => writeFileSync(join(dir, 'files.json'), JSON.stringify(config))
  write({ machine: { m: { root, ...targets } }, sandbox: {} })
  const env = (chain, more = {}) => ({ ...process.env, LFS_INSPECT_DIR: dir, LFS_REAL_CLI: chain[0], LFS_REAL_CLI_REST: chain.slice(1).join(':'), ...more })
  const run = (shim, args, chain = [last], more = {}) => {
    const r = spawnSync(process.execPath, [shim, ...args], { encoding: 'utf8', timeout: 20000, maxBuffer: 1 << 26, env: env(chain, more) })
    let out = null, err = null
    try { out = JSON.parse(r.stdout) } catch { /* not JSON */ }
    try { err = JSON.parse(r.stderr.trim().split('\n').at(-1)) } catch { /* not JSON */ }
    return { status: r.status, out, err, stdout: r.stdout, stderr: r.stderr }
  }
  // as the host calls it: options, then `--`, then the name and the path
  const files = (path, ...options) => run(filesShim, ['machine', 'files', ...options, '--', 'm', ...(path === undefined ? [] : [path])])
  return { dir, root, last, put, write, run, files, env, control: (c) => writeFileSync(join(dir, 'control.json'), JSON.stringify(c)), done: () => rmSync(dir, { recursive: true, force: true }) }
}
const names = (answer) => answer.entries.map((e) => e.path)

test('files: the roots answer: roles in the declared order, only roots that exist, then / when offered', () => {
  const f = fixture({ whole: 'machine', roots: [{ role: 'data', path: '/layerfs/data', free_bytes: 5, fs: 'ext4' }, { role: 'stores', path: '/layerfs/data/store' }, { role: 'image', path: '/layerfs/image' }, { role: 'workspace', path: '/ws/1', kind: 'mount', workspace: 'w1', fs: 'nfs' }] })
  try {
    f.put('layerfs/data/store/s/record.json', '{}')
    const roots = f.files().out
    assert.deepEqual([roots.kind, roots.next, roots.has_more], ['folder', null, false])
    assert.deepEqual(roots.entries.map((e) => [e.role, e.path, e.kind, e.depth]), [['data', '/layerfs/data', 'folder', 1], ['stores', '/layerfs/data/store', 'folder', 1], ['workspace', '/ws/1', 'mount', 1], ['machine', '/', 'folder', 1]], 'a root that is not there is left out; a declared mount is listed without a stat')
    assert.deepEqual([roots.entries[0].free_bytes, roots.entries[0].free, roots.entries[0].fs], [5, undefined, undefined], 'the data root says what is left as `free_bytes`; a folder root is a folder in the roots, whatever it is mounted on')
    assert.equal(typeof roots.entries[3].mode, 'number', 'a machine\'s / carries its mode')
    assert.deepEqual([roots.entries[2].workspace, roots.entries[2].fs, roots.entries[2].size], ['w1', 'nfs', null])
    f.write({ machine: { m: { root: f.root, roots: [{ role: 'stores', path: '/layerfs/data/store' }] } } })
    assert.deepEqual(f.files().out.entries.map((e) => e.path), ['/layerfs/data/store'], 'without `whole` no / is offered')
    assert.equal(f.run(filesShim, ['machine', 'files', '--', 'other']).err.error, 'NotFound')
    assert.equal(f.run(filesShim, ['sandbox', 'files', '--', 'sb']).err.error, 'NoSuchSandbox')
  } finally { f.done() }
})

test('files: a LayerFS folder met in a listing carries its role; from / it is not entered, and on its own mount it is a mount; logs are listed, not read', () => {
  const run = '/layerfs/data/machine/native/run'
  const f = fixture({ whole: 'machine', run_folders: [run], overlay: { '/layerfs/state/secret': { kind: 'restricted' } }, roots: [
    { role: 'data', path: '/layerfs/data', free_bytes: 5, fs: 'ext4' }, { role: 'stores', path: '/layerfs/data/store' }, { role: 'run_folders', path: run },
    { role: 'images', path: `${run}/images` }, { role: 'logs', path: '/layerfs/data/log' }, { role: 'state', path: '/layerfs/state' }, { role: 'bundle', path: '/layerfs/bundle' },
  ] })
  try {
    f.put('layerfs/data/store/s/store.sqlite', 'db')
    f.put('layerfs/data/store/s/open/42', '')
    for (const n of ['worker.log', 'worker.1.log', 'other.txt']) f.put(`layerfs/data/log/${n}`, n)
    f.put('layerfs/data/machine/native/run/rf/daemon.log', 'd')
    f.put('layerfs/data/machine/native/run/rf/tmp/daemon.log', 'not the daemon\'s')
    f.put('layerfs/data/machine/native/run/images/x.2.lock', '')
    f.put('layerfs/data/machine/native/run/images/x.2.json', '{}')
    f.put('layerfs/state/secret', 's')
    f.put('layerfs/bundle/bundle.json', '{}')
    const by = (answer) => Object.fromEntries(answer.entries.map((e) => [e.path, e]))
    // from /: listed with their roles, never entered, whatever the depth; the one on a mount of its own is a mount
    const top = by(f.files('/layerfs', '--depth', '4').out)
    assert.deepEqual(Object.keys(top), ['/layerfs/bundle', '/layerfs/data', '/layerfs/state'])
    assert.deepEqual([top['/layerfs/bundle'].kind, top['/layerfs/bundle'].role, top['/layerfs/bundle'].fs], ['folder', 'bundle', undefined])
    assert.deepEqual([top['/layerfs/data'].kind, top['/layerfs/data'].fs, top['/layerfs/data'].role, top['/layerfs/data'].mode, top['/layerfs/data'].mtime], ['mount', 'ext4', 'data', null, null])
    assert.ok(by(f.files('/', '--depth', '3').out)['/layerfs/data'] && !by(f.files('/', '--depth', '3').out)['/layerfs/data/store'], 'a deep walk from / stops at a LayerFS folder')
    // by its own path a root is read, and the roots inside it carry their roles
    const data = by(f.files('/layerfs/data', '--depth', '6').out)
    assert.deepEqual(['store', 'log', 'machine/native/run', 'machine/native/run/images'].map((n) => [data[`/layerfs/data/${n}`].kind, data[`/layerfs/data/${n}`].role]), [['folder', 'stores'], ['folder', 'logs'], ['folder', 'run_folders'], ['folder', 'images']])
    assert.equal(data['/layerfs/data/machine'].role, undefined)
    // logs: worker.log and its rotated part in the logs root, a run folder's daemon.log; nothing else
    assert.deepEqual(['log/worker.log', 'log/worker.1.log', 'log/other.txt', 'machine/native/run/rf/daemon.log', 'machine/native/run/rf/tmp/daemon.log'].map((n) => [data[`/layerfs/data/${n}`].kind, data[`/layerfs/data/${n}`].role]),
      [['file', 'log'], ['file', 'log'], ['file', undefined], ['file', 'log'], ['file', undefined]])
    for (const path of ['/layerfs/data/log/worker.log', `${run}/rf/daemon.log`]) {
      const r = f.files(path)
      assert.deepEqual([r.status, r.err.error], [125, 'InvalidInput'], path)
      assert.match(r.err.message, /is a log; read it with the `logs` calls/)
    }
    assert.equal(f.files('/layerfs/data/log/other.txt').out.content, 'other.txt')
    // what LayerFS keeps private: a Store's leases, lock files, the worker's secret
    assert.deepEqual(['store/s/open', 'store/s/store.sqlite', 'machine/native/run/images/x.2.lock', 'machine/native/run/images/x.2.json'].map((n) => data[`/layerfs/data/${n}`].kind), ['restricted', 'restricted', 'restricted', 'file'])
    assert.equal(data['/layerfs/data/store/s/open/42'], undefined, 'a restricted folder is not entered')
    assert.equal(by(f.files('/layerfs/state').out)['/layerfs/state/secret'].kind, 'restricted')
    // a missing path, in LayerFS's words
    assert.deepEqual([f.files('/layerfs/data/nope').err.error, f.files('/layerfs/data/nope').err.message], ['NotFound', 'machine.files: nothing is at /layerfs/data/nope'])
  } finally { f.done() }
})

test('files: a folder lists in byte order, pages by limit and cursor, goes deeper by depth, and stops at a declared time limit', () => {
  const f = fixture({ whole: 'machine' })
  try {
    for (const n of ['b', 'a', 'Z', 'c/x', 'c/y/z']) f.put(`d/${n}`, n)
    const page = f.files('/d').out
    assert.deepEqual(names(page), ['/d/Z', '/d/a', '/d/b', '/d/c'])
    assert.deepEqual(page.entries.map((e) => [e.kind, e.depth]), [['file', 1], ['file', 1], ['file', 1], ['folder', 1]])
        assert.equal(page.entries[1].size, 1)
    assert.deepEqual([page.next, page.has_more, 'reason' in page], ['/d/c', false, false], '`next` is the last entry\'s path also on a last page')
    assert.ok(page.entries.every((e) => e.mode <= 0o7777 && e.mtime > 1e12) && page.entries[3].size === null, 'permission bits, ms, and a size for regular files only')
    mkdirSync(join(f.root, 'none'))
    assert.deepEqual([f.files('/none').out.entries, f.files('/none').out.next], [[], null], 'an empty first page has no next')
    // pages
    const one = f.files('/d', '--limit', '2').out
    assert.deepEqual([names(one), one.next, one.has_more, one.reason], [['/d/Z', '/d/a'], '/d/a', true, 'limit'])
    const two = f.files('/d', '--limit', '2', '--cursor', one.next).out
    assert.deepEqual([names(two), two.has_more], [['/d/b', '/d/c'], false])
    // deeper: a folder before its entries, in tree order
    const deep = f.files('/d', '--depth', '3').out
    assert.deepEqual(deep.entries.map((e) => [e.path, e.depth]), [['/d/Z', 1], ['/d/a', 1], ['/d/b', 1], ['/d/c', 1], ['/d/c/x', 2], ['/d/c/y', 2], ['/d/c/y/z', 3]])
    // a page cut short by time
    f.write({ machine: { m: { root: f.root, whole: 'machine', deadline: { '/d': 3 } } } })
    const cut = f.files('/d').out
    assert.deepEqual([names(cut).length, cut.has_more, cut.reason, cut.next], [3, true, 'deadline', '/d/b'])
    assert.deepEqual(names(f.files('/d', '--cursor', cut.next).out), ['/d/c'])
    // bad options
    for (const bad of [['--limit', '0'], ['--limit', '1001'], ['--depth', '0'], ['--depth', '33'], ['--depth', 'x']]) assert.equal(f.files('/d', ...bad).err.error, 'InvalidInput', bad.join(' '))
  } finally { f.done() }
})

test('files: a file is read by window; a binary file has no content', () => {
  const f = fixture({ whole: 'machine' })
  try {
    f.put('t.txt', 'héllo wörld\n'.repeat(1000))
    f.put('empty', '')
    f.put('bin', Buffer.from([1, 2, 0, 3]))
    const first = f.files('/t.txt', '--length', '100').out
    assert.deepEqual([first.kind, first.offset, first.binary, first.size], ['file', 0, false, Buffer.byteLength('héllo wörld\n') * 1000])
    assert.ok(Buffer.byteLength(first.content) <= 100 && Buffer.byteLength(first.content) >= 97, 'cut back to a whole character')
    assert.equal(first.next, Buffer.byteLength(first.content))
    const second = f.files('/t.txt', '--offset', String(first.next), '--length', '100').out
    assert.equal(first.content + second.content, 'héllo wörld\n'.repeat(1000).slice(0, (first.content + second.content).length), 'the windows join without a torn character')
    const whole = f.files('/t.txt').out
    assert.deepEqual([whole.next, whole.content.length], [null, 12000])
    assert.deepEqual([f.files('/empty').out.size, f.files('/empty').out.content, f.files('/empty').out.next], [0, '', null])
    const bin = f.files('/bin').out
    assert.deepEqual([bin.binary, bin.content, bin.size], [true, '', 4])
    assert.equal(f.files('/t.txt', '--length', String((1 << 20) + 1)).err.error, 'InvalidInput', 'a window is at most 1 MiB')
  } finally { f.done() }
})

test('files: kinds that are listed and never opened, and the refusals for their paths', () => {
  const f = fixture({ whole: 'machine', o_r_outside: '/layerfs', run_folders: ['/layerfs/data/machine/native/run'], overlay: { '/proc': { kind: 'system' }, '/mnt/share': { kind: 'mount', fs: 'virtiofs' } } })
  try {
    f.put('etc/hosts', 'x', 0o644)
    f.put('etc/shadow', 'secret', 0o600)
    symlinkSync('hosts', join(f.root, 'etc', 'link'))
    chmodSync(join(f.root, 'etc'), 0o755)
    assert.equal(spawnSync('mkfifo', [join(f.root, 'etc', 'pipe')]).status, 0)
    chmodSync(join(f.root, 'etc', 'pipe'), 0o644)
    mkdirSync(join(f.root, 'mnt'), { recursive: true })
    chmodSync(join(f.root, 'mnt'), 0o755)
    f.put('layerfs/data/sandbox/s/control.json', '{}')
    f.put('layerfs/data/sandbox/s/record.json', '{}')
    f.put('layerfs/data/store/s/store.sqlite', 'db')
    f.put('layerfs/data/machine/native/run/rf/config/key', 'k')
    f.put('layerfs/data/machine/native/run/rf/overlay/1.sqlite', 'o')
    mkdirSync(join(f.root, 'layerfs/data/machine/native/run/rf/workspaces/1'), { recursive: true })
    const kinds = (path) => Object.fromEntries(f.files(path).out.entries.map((e) => [e.path.slice(path.length === 1 ? 1 : path.length + 1), e.kind]))
    assert.deepEqual(kinds('/etc'), { hosts: 'file', link: 'symlink', pipe: 'other', shadow: 'unreadable' })
    assert.equal(f.files('/etc').out.entries.find((e) => e.kind === 'symlink').target, 'hosts')
    assert.deepEqual(kinds('/'), { etc: 'folder', layerfs: 'folder', mnt: 'folder', proc: 'system' }, 'an entry the disk cannot show is added to its folder')
    assert.deepEqual(f.files('/mnt').out.entries.map((e) => [e.kind, e.fs]), [['mount', 'virtiofs']])
    assert.deepEqual(kinds('/layerfs/data/sandbox/s'), { 'control.json': 'restricted', 'record.json': 'file' })
    assert.deepEqual(kinds('/layerfs/data/store/s'), { 'store.sqlite': 'restricted' })
    assert.deepEqual(kinds('/layerfs/data/machine/native/run/rf'), { config: 'restricted', overlay: 'restricted', workspaces: 'folder' })
    assert.deepEqual(f.files('/layerfs/data/machine/native/run/rf/workspaces').out.entries.map((e) => [e.kind, e.size]), [['mount', null]], 'a Workspace mount is listed by name, never stat\'ed')
    // their paths are refused
    for (const path of ['/etc/shadow', '/etc/link', '/etc/pipe', '/proc', '/proc/1', '/mnt/share', '/layerfs/data/sandbox/s/control.json', '/layerfs/data/machine/native/run/rf/config', '/layerfs/data/machine/native/run/rf/config/key', '/layerfs/data/machine/native/run/rf/workspaces/1', '/etc/../etc/hosts', '/etc//hosts', 'etc/hosts']) {
      const r = f.files(path)
      assert.deepEqual([r.status, r.err?.error], [125, 'InvalidInput'], path)
    }
    assert.equal(f.files('/etc/nope').err.error, 'NotFound')
    assert.equal(f.files('/etc/hosts').out.content, 'x')
  } finally { f.done() }
})

test('files: confines, a stopped target, hangs and refusals from the control file, and the spawn log', () => {
  const f = fixture({ roots: [{ role: 'stores', path: '/layerfs/store' }], confine: ['/layerfs/store'] })
  try {
    f.put('layerfs/store/a', '1')
    f.put('etc/hosts', 'x')
    assert.deepEqual(names(f.files('/layerfs/store').out), ['/layerfs/store/a'])
    assert.equal(f.files('/etc/hosts').err.error, 'InvalidInput', 'outside the confines')
    assert.equal(f.files('/').err.error, 'InvalidInput')
    // stopped
    f.write({ machine: { m: { root: f.root, state: 'stopped' } }, sandbox: { sb: { root: f.root, whole: 'root', home: '/home', state: 'stopped', roots: [{ role: 'home', path: '/home' }] } } })
    f.put('home/x', '1')
    assert.equal(f.files().err.error, 'Unavailable', 'a stopped machine reads nothing')
    const sb = (...args) => f.run(filesShim, ['sandbox', 'files', '--', 'sb', ...args])
    assert.deepEqual(names(sb('/home').out), ['/home/x'], 'a stopped Sandbox\'s home is read')
    assert.equal(sb('/etc').err.error, 'Unavailable', 'the rest of it is not')
    assert.ok(sb().out.entries.some((e) => e.role === 'home'))
    // the control file: per path, per target, for every target
    f.write({ machine: { m: { root: f.root, whole: 'machine' } } })
    f.control({ refuse: { 'files-machine:m:/etc': { code: 'Busy', message: 'two reads are running' } }, delay_ms: { 'files-machine:m:/layerfs': 700 } })
    assert.deepEqual([f.files('/etc').status, f.files('/etc').err], [125, { error: 'Busy', message: 'two reads are running' }])
    assert.equal(f.files('/etc/hosts').out.content, 'x', 'another path is not refused')
    const t0 = Date.now()
    assert.ok(f.files('/layerfs').out.entries.length === 1 && Date.now() - t0 >= 650, 'a hang delays the answer')
    f.control({ refuse: { 'files-machine:m:': { code: 'Unavailable', message: 'roots' }, 'files-sandbox:*': { code: 'NoSuchSandbox', message: 'lost' } } })
    assert.equal(f.files().err.error, 'Unavailable', 'the empty path is the roots read')
    assert.equal(f.run(filesShim, ['sandbox', 'files', '--', 'any']).err.error, 'NoSuchSandbox')
    f.control({})
    // every files call is one line in spawns.log; calls passed on are not
    const before = readFileSync(join(f.dir, 'spawns.log'), 'utf8').split('\n').filter(Boolean).length
    f.files('/etc')
    f.run(filesShim, ['sandbox', 'get', 'sb'])
    const lines = readFileSync(join(f.dir, 'spawns.log'), 'utf8').split('\n').filter(Boolean)
    assert.equal(lines.length, before + 1)
    assert.deepEqual(JSON.parse(lines.at(-1).slice(lines.at(-1).indexOf(' ') + 1)), ['machine', 'files', '--', 'm', '/etc'])
  } finally { f.done() }
})

test('files: help --json gains the calls it answers; the rest goes down the chain unchanged', () => {
  const f = fixture()
  try {
    const help = f.run(filesShim, ['help', '--json']).out
    const has = (family, call) => help.calls.some((c) => c.family === family && c.call === call)
    assert.ok(has('sandbox', 'get') && has('sandbox', 'files') && has('machine', 'files'))
    f.write({ calls: ['sandbox.files'], machine: {}, sandbox: {} })
    const some = f.run(filesShim, ['help', '--json']).out
    assert.ok(some.calls.some((c) => c.family === 'sandbox' && c.call === 'files') && !some.calls.some((c) => c.family === 'machine' && c.call === 'files'))
    const refused = f.run(filesShim, ['machine', 'files', '--', 'm'])
    assert.deepEqual([refused.status, refused.err.error], [125, 'Unsupported'], 'a call it does not answer is the next CLI\'s to refuse')
    // files -> fields -> last: each hands the next the rest of the chain
    writeFileSync(join(f.dir, 'last.log'), '')
    const got = f.run(filesShim, ['sandbox', 'get', 'sb'], [fieldsShim, f.last])
    assert.equal(got.status, 0)
    assert.equal(got.out.workspace_capacity, 16, 'the fields wrapper in the chain patched the answer')
    const seen = readFileSync(join(f.dir, 'last.log'), 'utf8').trim().split('\n').map((l) => JSON.parse(l))
    assert.deepEqual(seen.map((s) => s.argv), [['sandbox', 'get', 'sb']])
    assert.equal(seen[0].rest, '', 'the last CLI is handed an empty rest')
    const unknown = f.run(filesShim, ['store', 'list'], [fieldsShim, f.last])
    assert.deepEqual([unknown.status, unknown.err.error], [125, 'Unsupported'], 'exit code and stderr come back up the chain')
  } finally { f.done() }
})

test('fields: sandbox get gains workspace_capacity, running (recorded execs, oldest first) and overlay_bytes; each can be absent or null', async () => {
  const f = fixture()
  try {
    mkdirSync(join(f.dir, 'run', 'overlay'), { recursive: true })
    writeFileSync(join(f.dir, 'run', 'overlay', '1.sqlite'), Buffer.alloc(8192))
    const get = () => f.run(fieldsShim, ['sandbox', 'get', 'sb']).out
    let got = get()
    assert.equal(got.workspace_capacity, 16)
    assert.deepEqual(got.workspaces.map((w) => [w.name, w.running, w.overlay_bytes]), [['one', [], 8192], ['two', [], null]], 'the overlay file\'s size; null when it is not there')
    // a command that runs through the wrapper is recorded while it runs
    const child = spawn(process.execPath, [fieldsShim, 'workspace', 'exec', 'sb:two', 'sleep 20', '--exec-id', 'x-1'], { env: f.env([f.last], { LAST_SLEEP: '3000', LAYERFS_LOGS_TAG: 'dsh:s1:r1' }), stdio: 'ignore' })
    const ended = new Promise((resolve) => child.on('exit', resolve))
    const recorded = () => { try { return readdirSync(join(f.dir, 'execs')).length } catch { return 0 } }
    for (let i = 0; i < 60 && !recorded(); i++) await new Promise((r) => setTimeout(r, 50))
    f.write({ machine: {}, sandbox: {} })
    writeFileSync(join(f.dir, 'fields.json'), JSON.stringify({ extra_running: { 'sb:two': [{ exec_id: 'old', started: 5, tag: null, program: 'make' }] } }))
    got = get()
    const two = got.workspaces[1].running
    assert.deepEqual(two.map((r) => [r.exec_id, r.program, r.tag]), [['old', 'make', null], ['x-1', 'sleep', 'dsh:s1:r1']], 'oldest first, with the call\'s tag and the command\'s first word')
    assert.ok(Math.abs(two[1].started - Date.now()) < 10000)
    assert.deepEqual(got.workspaces[0].running, [])
    assert.equal(await ended, 0)
    writeFileSync(join(f.dir, 'fields.json'), '{}')
    assert.deepEqual(get().workspaces[1].running, [], 'gone when the command ends')
    // an older worker, and fields that were not observed
    writeFileSync(join(f.dir, 'fields.json'), JSON.stringify({ workspace_capacity: 'absent', running: 'absent', overlay_bytes: 'absent' }))
    got = get()
    assert.ok(!('workspace_capacity' in got) && got.workspaces.every((w) => !('running' in w) && !('overlay_bytes' in w)))
    writeFileSync(join(f.dir, 'fields.json'), JSON.stringify({ workspace_capacity: 8, running: 'null', overlay_bytes: 'null' }))
    got = get()
    assert.deepEqual([got.workspace_capacity, got.workspaces[0].running, got.workspaces[0].overlay_bytes], [8, null, null])
  } finally { f.done() }
})
