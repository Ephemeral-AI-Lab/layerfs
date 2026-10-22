// The original SDK still executes in this process. This entry adds workflow
// ownership and records; it is not an OS security boundary.
import { join } from 'node:path';
import { randomUUID } from 'node:crypto';
import { binding, read, write, update, locked, requireThat } from './state.mjs';
import { controlRoot, loadConfig, nativeSDK } from './runtime.mjs';

const root = controlRoot(process.cwd());
const config = loadConfig(root);
const sdk = await nativeSDK(config, root);
export const { LayerFSError, isRefusal, CODES } = sdk;
export const workspaceKey = (ws) => `${ws?.sandbox}:${ws?.path}`;
const writes = new Set(['write','edit','apply','mkdir','rm','cp','mv']);

export function owner() {
  const bound = binding();
  requireThat(bound.agent.config === config.identity, 'project configuration changed during the session');
  return bound;
}
export function ownsBranch(branch) {
  const { agent } = owner();
  requireThat(branch !== 'main' && agent.branches.includes(branch), `Branch ${branch} is not owned by ${agent.id}`);
}
export function ownsWorkspace(ws) {
  const { agent } = owner();
  requireThat(agent.workspaces[workspaceKey(ws)]?.status === 'mounted', `Workspace ${workspaceKey(ws)} is not owned/mounted by ${agent.id}`);
}
export async function branches(lf) {
  const items = [];
  for (let pos = 0; ;) {
    const page = await lf.project.list_branches(1000, pos);
    items.push(...page.items);
    if (!page.hasMore) return items;
    pos = page.nextPagePos ?? pos + page.items.length;
    requireThat(page.items.length, 'branch pagination did not advance');
  }
}
export async function version(lf, branch) {
  const result = (await branches(lf)).find((b) => b.name === branch);
  requireThat(result, `missing Branch/view ${branch}`);
  return { head: result.head ?? null, base: result.base ?? null };
}
export async function tracked(method, detail, action, saved) {
  const { path, agent } = owner();
  const file = join(agent.directory, 'operations', `${randomUUID()}.json`);
  const op = { method, agent: agent.id, call: process.env.CC_LAYERFS_CALL, detail, status: 'pending', started: new Date().toISOString() };
  write(file, op);
  let result;
  try { result = await action(); }
  catch (error) {
    write(file, { ...op, status: ['Unknown','Failed'].includes(error.code) || !error.code ? 'unknown' : 'refused', error: { code: error.code, message: error.message } });
    throw error;
  }
  // Persist the known result before updating ownership. If bookkeeping fails,
  // this receipt is evidence to inspect, never grounds to replay the mutation.
  write(file, { ...op, status: 'returned', result: { kind: result?.kind, branch: result?.branch, head: result?.head,
    exitCode: result?.exitCode, process: result?.process, publication: result?.publishedLayer?.id, source: result?.source,
    ...(method === 'workspace.mount' ? { workspace: result } : {}) }, ended: new Date().toISOString() });
  if (saved) {
    try { await update(path, (state) => saved(state, result)); }
    catch (error) { write(file, { ...read(file), status: 'bookkeeping-failed', error: error.message }); throw error; }
  }
  return result;
}

export class LayerFS extends sdk.LayerFS {
  constructor(projectName, options = {}) {
    requireThat(projectName === config.project, 'this session is bound to another project');
    requireThat(options.storePath === undefined && options.storePolicy === undefined && options.daemonHost === undefined, 'deployment overrides belong in the control configuration');
    owner();
    super(projectName, options);
    for (const name of writes) {
      const method = this.fs[name];
      this.fs[name] = async (...args) => {
        ownsBranch(args[0]);
        return tracked(`fs.${name}`, { branch: args[0] }, () => method(...args));
      };
    }
    this.project.init = async () => { requireThat(false, 'import is an explicit operator setup step, not a session operation'); };
    for (const name of ['fork','merge']) {
      const method = this.branch[name];
      this.branch[name] = async (...args) => {
        if (name === 'merge') ownsBranch(args[0]);
        const newName = args[name === 'fork' ? 0 : 2];
        requireThat(newName !== 'main', 'main is read-only');
        const main = name === 'merge' && [args[0],args[1]].includes('main') ? await version(this, 'main') : null;
        const result = await tracked(`branch.${name}`, { name: newName, from: name === 'fork' ? args[1] ?? 'main' : args[0], ...(name === 'merge' ? { with: args[1] } : {}) }, () => method(...args), (state, value) => {
          requireThat(value.branch === newName, 'unexpected Branch result; inspect the operation receipt');
          state.branches.push(value.branch);
          state.branch = value.branch;
          if (main) state.integrations[value.branch] = main;
          state.validation = null;
        });
        return result;
      };
    }
    const publish = this.branch.publish;
    this.branch.publish = async (branch, commit) => {
      ownsBranch(branch);
      requireThat(owner().agent.role === 'lead', 'only the Lead publishes');
      return locked(join(config.home, '.claude-plugin-locks', config.project, 'publish'), async () => {
        const { agent } = owner();
        const current = await version(this, branch);
        const main = await version(this, 'main');
        requireThat(agent.validation?.branch === branch && agent.validation.head === current.head && current.head !== null,
          'validate this exact Branch version first: lfs run <branch> <check> --validate');
        requireThat(JSON.stringify(agent.integrations[branch]) === JSON.stringify(main), 'merge current main before validating and publishing');
        requireThat(commit === undefined || commit === current.head, 'publish the validated Commit');
        return tracked('branch.publish', { branch, commit: current.head }, () => publish(branch, current.head));
      });
    };
    const mount = this.workspace.mount;
    this.workspace.mount = async (branch, ...args) => {
      ownsBranch(branch);
      return tracked('workspace.mount', { branch }, () => mount(branch, ...args), (state, ws) => {
        state.workspaces[workspaceKey(ws)] = { ws, branch, status: 'mounted', keep: false };
      });
    };
    for (const name of ['exec','status','commit','unmount']) {
      const method = this.workspace[name];
      this.workspace[name] = async (ws, ...args) => {
        ownsWorkspace(ws);
        if (name === 'status') return method(ws, ...args);
        return tracked(`workspace.${name}`, { workspace: ws, ...(name === 'unmount' ? { discard: args[0] === true } : {}) }, () => method(ws, ...args), (state, result) => {
          const resource = state.workspaces[workspaceKey(ws)];
          resource.last = { method: name, kind: result?.kind, head: result?.head, exitCode: result?.exitCode };
          if (name === 'unmount') resource.status = 'unmounted';
        });
      };
    }
  }
}

export async function retainWorkspace(ws, reason) {
  ownsWorkspace(ws);
  requireThat(typeof reason === 'string' && reason.trim(), 'give the reason for retaining this Workspace');
  await update(owner().path, (state) => { state.workspaces[workspaceKey(ws)].keep = reason; });
}
export async function releaseWorkspace(ws) {
  ownsWorkspace(ws);
  await update(owner().path, (state) => { state.workspaces[workspaceKey(ws)].keep = false; });
}
export async function adoptBranch(branch) {
  ownsBranch(branch);
  await update(owner().path, (state) => { state.branch = branch; });
}
// Call only after a successful check and Commit, including any generated state.
// The host cannot decide whether a test command is a meaningful product test.
export async function recordValidation(lf, branch, description, exitCode, savedCommit) {
  ownsBranch(branch);
  requireThat(exitCode === 0 && typeof description === 'string' && description.trim(), 'a successful named validation is required');
  const checked = await version(lf, branch);
  requireThat(savedCommit && savedCommit.head === checked.head && !savedCommit.merged,
    'validation needs the check Workspace Commit result, with no concurrent merge; inspect and check again');
  const main = await version(lf, 'main');
  const { path, agent } = owner();
  requireThat(JSON.stringify(agent.integrations[branch]) === JSON.stringify(main), 'merge current main before validating');
  await update(path, (state) => { state.validation = { branch, ...checked, description, at: new Date().toISOString() }; });
  return read(path).validation;
}
