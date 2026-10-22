// Small control records, never project contents or Store authority.
import { mkdirSync, readFileSync, writeFileSync, renameSync, rmSync, existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { createHash, randomUUID } from 'node:crypto';
import { setTimeout as delay } from 'node:timers/promises';

export class PolicyError extends Error {
  constructor(message) { super(message); this.name = 'LayerFSPluginError'; this.code = 'PluginPolicy'; }
}
export const requireThat = (ok, message) => { if (!ok) throw new PolicyError(message); };
export const key = (value) => createHash('sha256').update(String(value)).digest('hex').slice(0, 24);
export const read = (path) => JSON.parse(readFileSync(path, 'utf8'));
export function write(path, value) {
  mkdirSync(dirname(path), { recursive: true, mode: 0o700 });
  const tmp = `${path}.${randomUUID()}.tmp`;
  writeFileSync(tmp, `${JSON.stringify(value, null, 2)}\n`, { mode: 0o600 });
  renameSync(tmp, path);
}
export const optional = (path) => existsSync(path) ? read(path) : null;
export const sessionDir = (root, session) => join(root, '.layerfs-claude', 'sessions', key(session));
export const agentPath = (dir, agent) => join(dir, 'agents', `${key(agent)}.json`);
export const callPath = (dir, call) => join(dir, 'calls', `${key(call)}.json`);
export const branchName = (session, agent) => `cc-${key(session)}-${agent === 'lead' ? 'lead' : `w-${key(agent)}`}`;

// Wait only for the local record lock. No SDK operation is retried. A crashed
// owner leaves its lock for explicit operator inspection, not guessed recovery.
export async function locked(path, action) {
  mkdirSync(dirname(path), { recursive: true, mode: 0o700 });
  const lock = `${path}.lock`;
  const until = Date.now() + 2000;
  for (;;) {
    try { mkdirSync(lock); break; }
    catch (error) {
      if (error.code !== 'EEXIST') throw error;
      requireThat(Date.now() < until, `record locked: ${lock}; inspect its owner before removing it`);
      await delay(20);
    }
  }
  try {
    write(join(lock, 'owner.json'), { pid: process.pid, started: new Date().toISOString() });
    return await action();
  } finally { rmSync(lock, { recursive: true }); }
}
export async function update(path, action) {
  return locked(path, async () => {
    const value = read(path);
    const result = await action(value);
    write(path, value);
    return result;
  });
}
export function binding() {
  const path = process.env.CC_LAYERFS_AGENT_RECORD;
  requireThat(path, 'no managed agent binding; run from a LayerFS Claude session');
  const agent = read(path);
  requireThat(agent.status === 'ready', `agent binding is ${agent.status}; inspect ${path}`);
  return { path, agent };
}
