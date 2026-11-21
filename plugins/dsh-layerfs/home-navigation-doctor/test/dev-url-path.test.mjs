import assert from 'node:assert/strict'
import { execFileSync } from 'node:child_process'
import { copyFileSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { test } from 'node:test'

test('devUrl reads the default profile relative to a checkout with URL-sensitive characters', (t) => {
  const root = mkdtempSync(join(tmpdir(), 'LayerFS 验收 %#-'))
  t.after(() => rmSync(root, { recursive: true, force: true }))
  mkdirSync(join(root, 'tools'))
  mkdirSync(join(root, '.dev'))
  copyFileSync(new URL('../tools/ui.mjs', import.meta.url), join(root, 'tools/ui.mjs'))
  writeFileSync(join(root, '.dev/url.txt'), 'http://localhost/example\n')
  const env = { ...process.env }
  delete env.DSH_LAYERFS_DEV_DIR
  const url = execFileSync(process.execPath, ['--input-type=module', '-e', "import { devUrl } from './tools/ui.mjs'; process.stdout.write(devUrl())"], { cwd: root, env, encoding: 'utf8' })
  assert.equal(url, 'http://localhost/example')
})
