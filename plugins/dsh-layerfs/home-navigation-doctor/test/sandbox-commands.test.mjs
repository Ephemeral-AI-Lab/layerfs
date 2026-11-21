// Sandbox page commands (src/host/routes/commands.ts) over a fake SDK: the
// per-Workspace logs, one command per Workspace, binary output, the tail, v1
// migration, log cleanup on unmount and close, and the routes that reach them.
//   node --test test/sandbox-commands.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

const root = fileURLToPath(new URL('..', import.meta.url))
const out = mkdtempSync(join(root, 'lib', '.test-sandbox-commands-'))
await build({
  absWorkingDir: root,
  stdin: { contents: "export * from './src/host/routes/commands.ts'; export { createGroups, route, errorAnswer } from './src/host/routes/router.ts'", resolveDir: root, loader: 'ts' },
  outfile: join(out, 'commands.mjs'), bundle: true, format: 'esm', platform: 'node', target: 'node24', external: ['@deepseek-ai/*'], logLevel: 'warning',
})
const M = await import(pathToFileURL(join(out, 'commands.mjs')).href)
const temps = []
test.after(() => { rmSync(out, { recursive: true, force: true }); for (const t of temps) rmSync(t, { recursive: true, force: true }) })

/**
 * A host whose lfs.workspace.exec is driven by the test, in the 2.0 shape:
 * the `--progress` start event names the exec id (`exec-N`), and the one
 * result `{ exit_code, stdout, stderr, exec }` carries the output. The test's
 * `exec(ws, command, out)` writes output with `out.stdout`/`out.stderr` and
 * answers `{ exit_code }` (or throws, or never settles).
 */
function host(exec, extra = {}) {
  const jobs = mkdtempSync(join(tmpdir(), 'lfs-cmd-'))
  temps.push(jobs)
  const events = []
  const calls = []
  let n = 0
  const rec = (name, answer) => async (...a) => { calls.push([name, ...a]); return typeof answer === 'function' ? answer(...a) : answer }
  const lfs = {
    workspace: {
      exec: async (ws, command, options) => {
        calls.push(['exec', ws, command, options])
        const id = `exec-${++n}`
        options.on_progress?.({ exec: id })
        const out = [], err = []
        const result = await exec(ws, command, { stdout: (b) => out.push(Buffer.from(b)), stderr: (b) => err.push(Buffer.from(b)) })
        // the result is JSON: its text is the bytes decoded as UTF-8; the long form counts the bytes
        const o = Buffer.concat(out), e = Buffer.concat(err)
        return { exit_code: 0, stdout: o.toString('utf8'), stderr: e.toString('utf8'), stdout_bytes: o.length, stderr_bytes: e.length, exec: id, ...result }
      },
      cancel: rec('cancel', { ok: true }),
      commit: rec('commit', { commit: 'c1' }),
      unmount: rec('unmount', { ok: true }),
      ...extra.workspace,
    },
    sandbox: { remove: rec('remove', { ok: true }), ...extra.sandbox },
  }
  return { jobs, events, calls, h: { lfs: () => lfs, jobs, emit: (type, data) => events.push([type, data]), log: () => {} } }
}

const settle = () => new Promise((r) => setTimeout(r, 10))
const done = async () => ({ exit_code: 0 })
const hash = (ws) => createHash('sha1').update(ws).digest('hex').slice(0, 16)
const wsFile = (jobs, ws) => join(jobs, 'sandbox-commands', ws.split(':')[0], `${hash(ws)}.json`)
const groups = (h) => M.createGroups({ ...h, home: h.jobs, root: h.jobs, stats: h.jobs, secrets: h.jobs, cli: () => null, daemon: () => null, discover: async () => ({}), ensureNative: async () => {} }, async () => ({}))
const call = (g, method, path, body = {}) => {
  const url = new URL(path, 'http://x')
  return M.route(g, method, url.pathname.split('/').filter(Boolean).map(decodeURIComponent), url.searchParams, body)
}

test('a command runs in the background, its output arrives with its result and is logged in its Workspace file', async () => {
  let finish
  const { h, events, jobs, calls } = host((ws, command, opts) => new Promise((resolve) => {
    opts.stdout(Buffer.from('hello\n'))
    finish = () => { opts.stderr(Buffer.from('warn\n')); resolve({ exit_code: 3 }) }
  }))
  const c = new M.SandboxCommands(h)
  const started = c.start('sb1:build', { command: '  make  ' })
  assert.deepEqual([started.state, started.command, started.stdoutBinary, started.stderrBinary], ['running', 'make', false, false])
  assert.match(started.id, /^dsh-[0-9a-f]{16}$/)
  assert.equal(c.busy(), true)
  assert.equal(c.running('sb1:build'), true)
  // one `workspace exec --progress --detail`: 2.0 streams nothing while it runs; the start event named the exec id
  assert.deepEqual(calls[0].slice(0, 3), ['exec', 'sb1:build', 'make'])
  assert.equal(calls[0][3].detail, true)
  assert.equal(c.listWorkspace('sb1:build').items[0].stdout, '')
  assert.equal(c.listWorkspace('sb1:build').items[0].execId, 'exec-1')
  finish()
  await settle()
  const [e] = c.listWorkspace('sb1:build').items
  assert.deepEqual([e.state, e.exitCode, e.stdout, e.stderr, e.stdoutBytes], ['exited', 3, 'hello\n', 'warn\n', 6])
  assert.equal(c.busy(), false)
  assert.deepEqual(events.at(-1), ['sandbox-command', { type: 'sandbox-command', sandbox: 'sb1', ws: 'sb1:build', id: started.id, state: 'exited' }])
  const saved = JSON.parse(readFileSync(wsFile(jobs, 'sb1:build'), 'utf8'))
  assert.deepEqual([saved.version, saved.ws, saved.commands[0].id], [2, 'sb1:build', started.id])
  // a fresh host reads the log back
  assert.equal(new M.SandboxCommands(h).listWorkspace('sb1:build').items[0].exitCode, 3)
})

test('per-Workspace files keep 50 each; the Sandbox list merges them newest first, at most 50', async () => {
  const { h, jobs } = host(done)
  const c = new M.SandboxCommands(h)
  let t = 1000
  const realNow = Date.now
  Date.now = () => t++
  try {
    for (let i = 0; i < 55; i++) { c.start('sb3:a', { command: `a ${i}` }); await settle() }
    for (let i = 0; i < 3; i++) { c.start('sb3:b', { command: `b ${i}` }); await settle() }
  } finally { Date.now = realNow }
  const a = c.listWorkspace('sb3:a').items
  assert.equal(a.length, M.KEEP)
  assert.equal(a[0].command, 'a 54')
  assert.ok(a.every((e) => e.ws === 'sb3:a'))
  assert.deepEqual(c.listWorkspace('sb3:b').items.map((e) => e.command), ['b 2', 'b 1', 'b 0'])
  assert.deepEqual(readdirSync(join(jobs, 'sandbox-commands', 'sb3')).sort(), [`${hash('sb3:a')}.json`, `${hash('sb3:b')}.json`].sort())
  // a fresh host lists the Sandbox from its files
  const all = new M.SandboxCommands(h).list('sb3').items
  assert.equal(all.length, M.KEEP)
  assert.deepEqual(all.slice(0, 4).map((e) => e.command), ['b 2', 'b 1', 'b 0', 'a 54'])
})

test('one command per Workspace: a second start is refused 409 CommandRunning', async () => {
  const { h } = host(async (ws, command) => command === 'hang' ? new Promise(() => {}) : { exit_code: 0 })
  const c = new M.SandboxCommands(h)
  c.start('sb2:a', { command: 'hang' })
  assert.throws(() => c.start('sb2:a', { command: 'ls' }), { code: 'CommandRunning', status: 409 })
  // another Workspace of the Sandbox is free
  c.start('sb2:b', { command: 'ls' })
  assert.equal(M.errorAnswer(Object.assign(new Error('x'), { code: 'CommandRunning' })).status, 409)
})

test('inline answers, refusals, cancels and a host restart', async () => {
  const { h, calls } = host(async (ws, command) => {
    if (command === 'bad') throw Object.assign(new Error('no such ws'), { code: 'NotMounted' })
    if (command === 'hang') return new Promise(() => {})
    return { exit_code: 0, stdout: 'inline' }
  })
  const c = new M.SandboxCommands(h)
  c.start('sb2:a', { command: 'echo' })
  await settle()
  c.start('sb2:a', { command: 'bad' })
  await settle()
  const hang = c.start('sb2:a', { command: 'hang' })
  await settle()
  const [h3, h2, h1] = c.listWorkspace('sb2:a').items
  assert.equal(h1.stdout, 'inline')
  assert.deepEqual([h2.state, h2.error.code], ['failed', 'NotMounted'])
  assert.equal(h3.state, 'running')
  await c.cancel('sb2:a', { id: hang.id })
  // `workspace cancel <ws> --exec <id>`: only this command, by the id its start event named
  assert.deepEqual(calls.at(-1), ['cancel', 'sb2:a', { exec: 'exec-3', detail: true }])
  await assert.rejects(c.cancel('sb2:a', { id: 'dsh-nope' }), { code: 'NotFound' })
  await assert.rejects(c.cancel('sb2:other', { id: hang.id }), { code: 'NotFound' })
  // what a restarted host sees: the command it was running has no known outcome
  assert.equal(new M.SandboxCommands(h).listWorkspace('sb2:a').items[0].state, 'unknown')
  assert.equal(new M.SandboxCommands(h).list('sb2').items[0].state, 'unknown')
})

test('inputs are checked', () => {
  const { h } = host(done)
  const c = new M.SandboxCommands(h)
  assert.throws(() => c.start('nocolon', { command: 'ls' }), { code: 'InvalidInput' })
  assert.throws(() => c.start('sb:a', { command: '   ' }), { code: 'InvalidInput' })
  assert.throws(() => c.start('sb:a', { command: 'x'.repeat(9000) }), { code: 'InvalidInput' })
  assert.throws(() => c.start('../etc:a', { command: 'ls' }), { code: 'InvalidInput' })
  assert.throws(() => c.list('../etc'), { code: 'InvalidInput' })
  assert.throws(() => c.listWorkspace('../etc:a'), { code: 'InvalidInput' })
})

test('output keeps only its tail, without a split UTF-8 sequence at the cut', async () => {
  const { h } = host(async (ws, cmd, opts) => {
    if (cmd === 'yes') for (let i = 0; i < 10; i++) opts.stdout(Buffer.alloc(16 * 1024, 97 + i))
    // 3-byte characters whose boundary does not fall on the cut
    else opts.stdout(Buffer.from('€'.repeat(30000) + 'end'))
    return { exit_code: 0 }
  })
  const c = new M.SandboxCommands(h)
  c.start('sb4:a', { command: 'yes' })
  await settle()
  const [e] = c.listWorkspace('sb4:a').items
  assert.equal(e.stdoutBytes, 160 * 1024)
  assert.equal(e.stdout.length, M.TAIL)
  assert.equal(e.stdout.at(-1), 'j')
  c.start('sb4:b', { command: 'euro' })
  await settle()
  const [u] = c.listWorkspace('sb4:b').items
  assert.equal(u.stdoutBinary, false)
  assert.ok(!u.stdout.includes('�'))
  assert.ok(u.stdout.endsWith('€end'))
  assert.ok(Buffer.byteLength(u.stdout) <= M.TAIL)
  // the unit: a cut in the middle of a character drops its leftover bytes
  assert.deepEqual(M.decodeTail(Buffer.from('€x').subarray(1), true), { text: 'x', binary: false })
})

test('binary output: a NUL or over 10% U+FFFD empties the text but keeps the byte count', async () => {
  const { h } = host(async (ws, cmd, opts) => {
    if (cmd === 'nul') opts.stdout(Buffer.from('abc\0def'))
    if (cmd === 'junk') opts.stderr(Buffer.from([0xff, 0xfe, 0x41, 0x42, 0xff, 0x43, 0x44, 0x45]))
    if (cmd === 'few') opts.stdout(Buffer.concat([Buffer.from('a'.repeat(100)), Buffer.from([0xff])]))
    return { exit_code: 0 }
  })
  const c = new M.SandboxCommands(h)
  for (const [ws, cmd] of [['sb6:a', 'nul'], ['sb6:b', 'junk'], ['sb6:c', 'few']]) c.start(ws, { command: cmd })
  await settle()
  const [nul] = c.listWorkspace('sb6:a').items
  assert.deepEqual([nul.stdoutBinary, nul.stdout, nul.stdoutBytes, nul.stderrBinary], [true, '', 7, false])
  const [junk] = c.listWorkspace('sb6:b').items
  assert.deepEqual([junk.stderrBinary, junk.stderr, junk.stderrBytes, junk.stdoutBinary], [true, '', 8, false])
  const [few] = c.listWorkspace('sb6:c').items
  assert.equal(few.stdoutBinary, false)
  assert.equal(few.stdout.length, 101)
})

test('a v1 Sandbox log is split into Workspace files on first read, then removed', () => {
  const { h, jobs } = host(done)
  const entry = (id, ws, startedAt, extra = {}) => ({ id, ws, sandbox: 'sb7', command: id, state: 'exited', startedAt, finishedAt: startedAt + 1, exitCode: 0, stdout: 'o', stderr: '', stdoutBytes: 1, stderrBytes: 0, error: null, ...extra })
  mkdirSync(join(jobs, 'sandbox-commands'), { recursive: true })
  const v1 = join(jobs, 'sandbox-commands', 'sb7.json')
  writeFileSync(v1, JSON.stringify({ version: 1, commands: [
    entry('c4', 'sb7:b', 40, { state: 'running' }), entry('c3', 'sb7:a', 30), entry('c2', 'sb7:b', 20, { stdout: 'x\0y' }), entry('c1', 'sb7:a', 10), entry('c0', 'other:a', 5),
  ] }))
  const c = new M.SandboxCommands(h)
  const a = c.listWorkspace('sb7:a').items
  assert.deepEqual(a.map((e) => e.id), ['c3', 'c1'])
  assert.equal(a[0].stdoutBinary, false)
  assert.equal(existsSync(v1), false)
  const b = JSON.parse(readFileSync(wsFile(jobs, 'sb7:b'), 'utf8'))
  assert.deepEqual([b.version, b.ws, b.commands.map((e) => e.id)], [2, 'sb7:b', ['c4', 'c2']])
  const [c4, c2] = c.listWorkspace('sb7:b').items
  assert.equal(c4.state, 'unknown')
  assert.deepEqual([c2.stdoutBinary, c2.stdout], [true, ''])
  assert.deepEqual(new M.SandboxCommands(h).list('sb7').items.map((e) => e.id), ['c4', 'c3', 'c2', 'c1'])
})

test('routes: start, list both ways, cancel, commit', async () => {
  const { h, calls } = host(async (ws, command) => command === 'hang' ? new Promise(() => {}) : { exit_code: 0 })
  const g = groups(h)
  const entry = await call(g, 'POST', '/api/workspaces/sb5:a/commands', { command: 'ls' })
  assert.equal(entry.ws, 'sb5:a')
  await settle()
  await call(g, 'POST', '/api/workspaces/sb5:b/commands', { command: 'pwd' })
  await settle()
  assert.deepEqual((await call(g, 'GET', '/api/workspaces/sb5:a/commands')).items.map((e) => e.id), [entry.id])
  assert.deepEqual((await call(g, 'GET', '/api/sandboxes/sb5/commands')).items.map((e) => e.command), ['pwd', 'ls'])
  const hang = await call(g, 'POST', '/api/workspaces/sb5:a/commands', { command: 'hang' })
  await assert.rejects(call(g, 'POST', '/api/workspaces/sb5:a/commands', { command: 'ls' }), { code: 'CommandRunning' })
  await call(g, 'POST', '/api/workspaces/sb5:a/cancel', { id: hang.id })
  assert.deepEqual(calls.at(-1), ['cancel', 'sb5:a', { exec: 'exec-3', detail: true }])
  assert.deepEqual(await call(g, 'POST', '/api/workspaces/sb5:a/commit', { message: ' save ' }), { commit: 'c1' })
  assert.deepEqual(calls.at(-1), ['commit', 'sb5:a', { detail: true, message: 'save' }])
  await call(g, 'POST', '/api/workspaces/sb5:a/commit', {})
  assert.deepEqual(calls.at(-1), ['commit', 'sb5:a', { detail: true }])
  await assert.rejects(call(g, 'POST', '/api/workspaces/bad/commit', {}), { code: 'InvalidInput' })
})

test('routes: unmount refuses a running command unless forced, passes options and LayerFS refusals, and drops the log', async () => {
  let refusal = null
  const { h, calls, jobs, events } = host(
    async (ws, command) => command === 'hang' ? new Promise(() => {}) : { exit_code: 0 },
    { workspace: { unmount: async (...a) => { calls.push(['unmount', ...a]); if (refusal) throw refusal; return { kind: 'Unmounted' } } } },
  )
  const g = groups(h)
  await call(g, 'POST', '/api/workspaces/sb8:a/commands', { command: 'ls' })
  await settle()
  assert.equal(existsSync(wsFile(jobs, 'sb8:a')), true)
  // LayerFS's Dirty and Busy come back as they were (status from errorAnswer), the log stays
  refusal = Object.assign(new Error('has changes'), { code: 'Dirty' })
  await assert.rejects(call(g, 'DELETE', '/api/workspaces/sb8:a'), (e) => e === refusal)
  refusal = Object.assign(new Error('busy'), { code: 'Busy', detail: { execs: ['x1'] } })
  await assert.rejects(call(g, 'DELETE', '/api/workspaces/sb8:a'), (e) => e === refusal)
  assert.deepEqual(M.errorAnswer(refusal).status, 409)
  assert.equal(existsSync(wsFile(jobs, 'sb8:a')), true)
  refusal = null
  // a command running here: refused before LayerFS is asked
  await call(g, 'POST', '/api/workspaces/sb8:a/commands', { command: 'hang' })
  const before = calls.length
  await assert.rejects(call(g, 'DELETE', '/api/workspaces/sb8:a?discard=1'), { code: 'CommandRunning', status: 409 })
  assert.equal(calls.length, before)
  // forced: both options pass through, the log goes and clients are told
  assert.deepEqual(await call(g, 'DELETE', '/api/workspaces/sb8:a?discard=1&force=1'), { kind: 'Unmounted' })
  assert.deepEqual(calls.at(-1), ['unmount', 'sb8:a', { discard: true, force: true }])
  assert.equal(existsSync(wsFile(jobs, 'sb8:a')), false)
  assert.deepEqual(events.at(-1), ['sandbox-command', { type: 'sandbox-command', sandbox: 'sb8', ws: 'sb8:a', id: null, state: 'unmounted' }])
  assert.deepEqual((await call(g, 'GET', '/api/workspaces/sb8:a/commands')).items, [])
  // a plain unmount of an idle Workspace passes no options
  await call(g, 'DELETE', '/api/workspaces/sb8:b')
  assert.deepEqual(calls.at(-1), ['unmount', 'sb8:b', {}])
})

test('routes: removing a Sandbox (sandbox remove) removes its command logs, v1 file included', async () => {
  const { h, calls, jobs } = host(done)
  const g = groups(h)
  await call(g, 'POST', '/api/workspaces/sb9:a/commands', { command: 'ls' })
  await call(g, 'POST', '/api/workspaces/sb9:b/commands', { command: 'ls' })
  await settle()
  writeFileSync(join(jobs, 'sandbox-commands', 'sb9.json'), '{"version":1,"commands":[]}')
  const dir = join(jobs, 'sandbox-commands', 'sb9')
  assert.equal(readdirSync(dir).length, 2)
  // a refused removal keeps them
  h.lfs().sandbox.remove = async () => { throw Object.assign(new Error('in use'), { code: 'InUse' }) }
  await assert.rejects(call(g, 'DELETE', '/api/sandboxes/sb9'), { code: 'InUse' })
  assert.equal(existsSync(dir), true)
  h.lfs().sandbox.remove = async (...a) => { calls.push(['remove', ...a]); return { ok: true, outcome: 'removed', sandbox: 'sb9', unmounted: [] } }
  assert.deepEqual(await call(g, 'DELETE', '/api/sandboxes/sb9?force=1'), { ok: true, outcome: 'removed', sandbox: 'sb9', unmounted: [] })
  assert.deepEqual(calls.at(-1), ['remove', 'sb9', { detail: true, force: true }])
  assert.equal(existsSync(dir), false)
  assert.equal(existsSync(join(jobs, 'sandbox-commands', 'sb9.json')), false)
  assert.deepEqual((await call(g, 'GET', '/api/sandboxes/sb9/commands')).items, [])
})
