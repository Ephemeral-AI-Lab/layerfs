// Local structure/source checks. This is not Claude's official validator.
import assert from 'node:assert/strict';
import { readFileSync, readdirSync, existsSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';
import { sha, sdkHash } from '../scripts/runtime.mjs';

const root = fileURLToPath(new URL('../',import.meta.url));
const repo = fileURLToPath(new URL('../../',import.meta.url));
const json = (name) => JSON.parse(readFileSync(join(root,name),'utf8'));
const manifest = json('.claude-plugin/plugin.json');
const compatibility = json('compatibility.json');
assert.equal(manifest.name,'layerfs');
assert.equal(manifest.version,compatibility.pluginVersion);
assert.equal(json('settings.json').agent,'layerfs:lead');
assert.equal(sdkHash(join(repo,'sdk/js')),compatibility.sdk.sha256);
for (const [file,hash] of Object.entries(compatibility.skill.files)) {
  assert.equal(sha(join(root,'skills/layerfs-js-sdk',file)),hash,file);
  assert.equal(sha(join(repo,compatibility.skill.source,file)),hash,`upstream ${file}`);
}
const hooks = json('hooks/hooks.json').hooks;
for (const event of ['SessionStart','SubagentStart','PreToolUse','PostToolUse','PostToolUseFailure','Stop','SubagentStop','SessionEnd']) {
  assert(hooks[event]?.length,`missing ${event}`);
  for (const group of hooks[event]) for (const hook of group.hooks) {
    assert.equal(hook.type,'command');
    assert.deepEqual(hook.args,['${CLAUDE_PLUGIN_ROOT}/scripts/hook.mjs']);
    if (event === 'PreToolUse') assert.equal(hook.onFailure,'block');
  }
}
assert.equal(hooks.SubagentStart[0].matcher,'^layerfs:worker$');
for (const name of ['lead','worker']) {
  const text = readFileSync(join(root,`agents/${name}.md`),'utf8');
  assert(text.startsWith(`---\nname: ${name}\n`));
  assert(text.includes('  - layerfs:layerfs-js-sdk'));
  const tools = text.match(/^tools: (.+)$/m)?.[1];
  assert(tools?.includes('Bash'));
  assert(!/Read|Write|Edit|Glob|Grep|Worktree|PowerShell/.test(tools));
  if (name === 'lead') assert(tools.includes('Agent(layerfs:worker)'));
  else assert(!tools.includes('Agent'));
}
for (const dir of ['scripts','tools','tests']) for (const file of readdirSync(join(root,dir))) {
  if (file.endsWith('.mjs')) execFileSync(process.execPath,['--check',join(root,dir,file)],{ stdio:'pipe', timeout:5000 });
}
assert(existsSync(join(root,'LICENSE')));
console.log('Plugin structure, Agent declarations, Hook wiring, SDK pin, Skill parity and JavaScript syntax checked. No Claude process was run.');
