// Bundle the canonical Skill verbatim; record the SDK/Skill source seal.
import { cpSync, mkdirSync, readdirSync, readFileSync, writeFileSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';
import { sha, sdkHash } from '../scripts/runtime.mjs';

const root = fileURLToPath(new URL('../../', import.meta.url));
const target = join(root, 'claude/skills/layerfs-js-sdk');
rmSync(target, { recursive: true, force: true });
mkdirSync(target, { recursive: true });
cpSync(join(root, '.agents/skills/layerfs-js-sdk'), target, { recursive: true });
const hashes = {};
function walk(dir, prefix = '') {
  for (const entry of readdirSync(dir, { withFileTypes: true }).sort((a,b) => a.name.localeCompare(b.name))) {
    if (entry.isDirectory()) walk(join(dir,entry.name), join(prefix,entry.name));
    else hashes[join(prefix,entry.name)] = sha(join(dir,entry.name));
  }
}
walk(target);
cpSync(join(root,'LICENSE'), join(root,'claude/LICENSE'));
const compatibility = {
  schema: 1, pluginVersion: '0.1.0', claudeCode: { minimum: '2.1.295', documentedAt: '2.1.307', runtimeTested: false },
  layerfsRevision: execFileSync('git', ['rev-parse','HEAD'], { cwd: root, encoding: 'utf8' }).trim(),
  sdk: { version: JSON.parse(readFileSync(join(root,'sdk/js/package.json'),'utf8')).version, sha256: sdkHash(join(root,'sdk/js')) },
  skill: { source: '.agents/skills/layerfs-js-sdk', files: hashes },
  deployment: 'The control configuration records native addon and daemon hashes. Build both from the matching LayerFS source.'
};
writeFileSync(join(root,'claude/compatibility.json'), `${JSON.stringify(compatibility,null,2)}\n`);
console.log(`Copied ${Object.keys(hashes).length} Skill files and pinned SDK ${compatibility.sdk.version}.`);
