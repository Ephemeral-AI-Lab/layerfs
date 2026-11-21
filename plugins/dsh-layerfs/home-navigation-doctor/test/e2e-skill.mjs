#!/usr/bin/env node
// End-to-end: the dsh-layerfs skill and the Node SDK in a project session, in the real
// DSH web UI with the scripted fake model (dev/fake-llm). Needs a dev server started
// with DSH_LAYERFS_DEV_OVERLAY naming the fake model, and an SDK installed for the
// pinned CLI's commit (node scripts/install-sdk.mjs <sealed build>).
//   node test/e2e-skill.mjs [--keep]
// Checks: GET /api/status names the installed SDK; a project session's bash has
// $DSH_LAYERFS_SDK and a script imports it and calls LayerFS; that call is in the call
// log as a `node` line tagged with the session; the session's skill catalog lists
// `dsh-layerfs`, and the skill loads with its reference pages beside it.
import { fileURLToPath } from 'node:url'
import assert from 'node:assert/strict'
import { lstatSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, statSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { openDev, runTurn } from '../tools/e2e-lib.mjs'

const step = (m) => console.log(`\n== ${m}`)
const keep = process.argv.includes('--keep')
const dev = process.env.DSH_LAYERFS_DEV_DIR ?? fileURLToPath(new URL('../.dev', import.meta.url))
const { browser, page, logs } = await openDev()
await page.waitForFunction(() => window.__dshLayerfs, null, { timeout: 30000 })
const op = (name, args = {}) => page.evaluate(async ([n, a]) => {
  try { return { ok: true, value: await window.__dshLayerfs.op(n, a) } } catch (e) { return { ok: false, code: e.code, message: e.message } }
}, [name, args])
const must = async (name, args) => { const r = await op(name, args); if (!r.ok) throw new Error(`${name}: ${r.code} ${r.message}`); return r.value }
const openProject = (key) => page.evaluate(async (k) => window.__dshLayerfs.openProjectSession(k), key)
const http = async (method, path, body) => {
  const r = await must('http', { method, path, ...(body === undefined ? {} : { body }) })
  if (r.status < 200 || r.status >= 300) throw new Error(`${method} ${path}: ${r.status} ${JSON.stringify(r.body)}`)
  return r.body
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms))
const until = async (what, fn, ms = 30000) => {
  const end = Date.now() + ms
  for (;;) { const v = await fn(); if (v) return v; if (Date.now() > end) throw new Error(`timed out: ${what}`); await sleep(500) }
}
/** Files under `dir` (recursive) whose text contains `needle`. Symlinks (the profile links
 *  this checkout), node_modules and the LayerFS home (live mounts) are not walked. */
const grep = (dir, needle, out = []) => {
  let entries = []
  try { entries = readdirSync(dir) } catch { return out }
  for (const entry of entries) {
    if (entry === 'node_modules' || entry === 'layerfs-home') continue
    const path = join(dir, entry)
    let stat; try { stat = lstatSync(path) } catch { continue }
    if (stat.isSymbolicLink()) continue
    if (stat.isDirectory()) grep(path, needle, out)
    else if (stat.size < 8_000_000) { try { if (readFileSync(path, 'utf8').includes(needle)) out.push(path) } catch {} }
  }
  return out
}

/** Every tool result's text in a session's native log(s) under the dev dir. */
function toolOutputs(sessionId) {
  const out = []
  for (const path of grep(dev, sessionId).filter((p) => p.endsWith('.jsonl'))) {
    for (const line of readFileSync(path, 'utf8').split('\n')) {
      let event; try { event = JSON.parse(line) } catch { continue }
      if (event?.type !== 'tool/result') continue
      for (const part of event.data?.message?.content ?? []) if (part?.type === 'text') out.push(part.text)
    }
  }
  return out.join('\n')
}

const tag = Date.now().toString(36)
const store = `e2e${tag}`, name = 'skill', project = `${store}/${name}`
let failed = false
try {
  step('runtime status names the installed SDK')
  const status = await http('GET', '/api/status')
  const runtime = status.runtime ?? status
  assert.ok(runtime.commit, 'the CLI is discovered')
  assert.equal(runtime.sdk?.commit, runtime.commit, `an SDK for ${runtime.commit} is installed (looked in ${runtime.sdkDir})`)

  step(`store + project ${project}`)
  const fixtureRoot = join(dev, 'fixtures')
  mkdirSync(fixtureRoot, { recursive: true })
  const src = mkdtempSync(join(fixtureRoot, 'skill-'))
  writeFileSync(join(src, 'README.md'), '# skill\n')
  await http('POST', '/api/stores', { name: store })
  const init = await http('POST', '/api/projects', { store, name, source: src })
  await until('project init', async () => (await http('GET', `/api/project-inits/${init.id}`)).step === 'done', 60000)

  step('a project session with a scratch folder')
  const placed = await openProject(project)
  assert.ok(placed.sessionId)
  await page.waitForTimeout(1500)
  const scratch = join(fixtureRoot, `scratch-${tag}`)
  mkdirSync(scratch, { recursive: true })
  await page.getByRole('button', { name: /^Scratch folder:/ }).click()
  await page.getByRole('menuitem', { name: 'Enter a folder path…' }).click()
  await page.getByRole('textbox', { name: 'Local folder path' }).fill(scratch)
  await page.getByRole('button', { name: 'Select folder', exact: true }).click()
  await until('scratch selected', async () => (await must('session.place', { sessionId: placed.sessionId })).place?.scratch === scratch)

  step('bash has $DSH_LAYERFS_SDK and a script calls LayerFS through it')
  const script = `const { default: l } = await import(process.env.DSH_LAYERFS_SDK); const r = await l.branch.list(process.env.DSH_LAYERFS_PROJECT); console.log('SDKOK ' + r.items.length + ' ' + Object.keys(l).filter((k) => typeof l[k] === 'object' && k !== 'table').join(','))`
  await runTurn(page, { message: `bash: echo "SDKVAR=[$DSH_LAYERFS_SDK]"\nbash: node --input-type=module -e ${JSON.stringify(script)}`, newSession: false, timeoutMs: 90000 })
  const text = toolOutputs(placed.sessionId)
  assert.match(text, /SDKVAR=\[file:\/\/[^\]]+agent-bin\/sdk\.mjs\]/, 'the variable names the generated module')
  assert.match(text, /SDKOK \d+ project,branch,commit,file,sandbox,workspace/, 'the SDK answered with the six families')

  step('the SDK call is a node line in the call log, tagged with the session')
  const listed = await until('node line', async () => {
    const page1 = await must('logs.list', { client: 'node', limit: 50 })
    return (page1.lines ?? []).find((line) => line.argv?.[0] === 'branch' && line.argv?.[1] === 'list' && String(line.tag ?? '').includes(placed.sessionId))
  })
  assert.equal(listed.outcome, 'ok')

  step('the session catalog lists the dsh-layerfs skill')
  const logs = grep(join(dev, 'project-sessions'), placed.sessionId).filter((path) => path.endsWith('.jsonl'))
  const catalog = logs.filter((path) => readFileSync(path, 'utf8').includes('`dsh-layerfs`:'))
  assert.ok(catalog.length, `the session's skill catalog lists dsh-layerfs (${logs.join(', ') || 'no session log'})`)

  step('the runtime context states the LayerFS project setting')
  const context = logs.map((path) => readFileSync(path, 'utf8')).join('\n')
  assert.ok(context.includes('## LayerFS project session'), 'the project block is in the runtime context')
  assert.ok(context.includes(`| Project | \\"${project}\\" |`) || context.includes(`| Project | "${project}" |`), 'the Project row names the project')
  assert.ok(/Session Branch[^|]*\| \\?"dsh\./.test(context), 'the Branch row names the dsh. session Branch')
  assert.ok(context.includes('Load the `dsh-layerfs` skill'), 'it points to the skill')

  step('file calls plus one Workspace per bash call: saved at once, checkpoint counts them')
  const before = (await must('session.state', { sessionId: placed.sessionId })).checkpoints.filter((c) => c.kind === 'turn').length
  const L = '"$DSH_LAYERFS_CLI"', SBX = '${DSH_LAYERFS_WS%%:*}'
  await runTurn(page, { message: [
    // As the skill says: a write refused by a busy Store is retried, after checking an Unknown landed nowhere.
    `bash: for i in 1 2 3 4 5; do ${L} file read "$DSH_LAYERFS_PROJECT" "$DSH_LAYERFS_BRANCH" /policy.txt >/dev/null 2>&1 && break; printf 'policy-ok\\n' | ${L} file write "$DSH_LAYERFS_PROJECT" "$DSH_LAYERFS_BRANCH" /policy.txt -m "file write" && break; sleep $((2 + RANDOM % 4)); done`,
    `bash: S=${SBX}; ${L} workspace mount "$S" "$DSH_LAYERFS_BRANCH" --name e2e-run >/dev/null && ${L} workspace exec "$S:e2e-run" 'mkdir -p .cache && echo c > .cache/x && cat policy.txt' --stream && ${L} workspace commit "$S:e2e-run" -m "cache" >/dev/null && ${L} workspace unmount "$S:e2e-run" >/dev/null && echo RUN-DONE`,
  ].join('\n'), newSession: false, timeoutMs: 90000 })
  const outputs = toolOutputs(placed.sessionId)
  assert.match(outputs, /policy-ok\s+RUN-DONE/, 'a Workspace mounted after the file write sees it, and commits and unmounts')
  const after = await until('a checkpoint for the turn', async () => {
    const turns = (await must('session.state', { sessionId: placed.sessionId })).checkpoints.filter((c) => c.kind === 'turn')
    return turns.length > before ? turns.at(-1) : null
  }, 60000)
  assert.ok(after.commit, `the checkpoint has a commit (${JSON.stringify(after.error ?? null)})`)
  assert.ok(after.changed >= 2, `the checkpoint counts the moved files (changed=${after.changed})`)
  const sbx = await must('http', { method: 'GET', path: `/api/sandboxes/${encodeURIComponent((await must('session.state', { sessionId: placed.sessionId })).binding.sandbox)}` }).catch(() => null)
  if (sbx?.body?.workspaces) assert.deepEqual(sbx.body.workspaces.map((w) => w.name), ['main'], 'nothing left mounted but dsh\'s Workspace')

  step('the skill loads by name with its reference pages beside it')
  await runTurn(page, { message: 'bash: echo skill-check', newSession: false })
  const skill = readFileSync(new URL('../skills/dsh-layerfs/SKILL.md', import.meta.url), 'utf8')
  assert.match(skill, /^---\nname: dsh-layerfs\n/)
  for (const page of ['methods/file', 'methods/workspace', 'methods/branch', 'methods/commit', 'methods/project', 'methods/sandbox', 'rules/parallel', 'rules/conflicts', 'rules/lww', 'rules/cargo']) {
    assert.ok(statSync(new URL(`../skills/dsh-layerfs/references/${page}.md`, import.meta.url)).size > 0, page)
  }
  console.log('\nOK')
} catch (error) {
  failed = true
  console.error(error)
  console.error(logs.join('\n'))
} finally {
  if (!keep) {
    try { await http('DELETE', `/api/projects/${encodeURIComponent(project)}`) } catch (error) { console.error(`cleanup: ${error.message}`) }
    try { await http('DELETE', `/api/stores/${encodeURIComponent(store)}?confirm=${encodeURIComponent(store)}`) } catch (error) { console.error(`cleanup: ${error.message}`) }
  }
  await browser.close()
  process.exit(failed ? 1 : 0)
}
