// Explicit, evidence-based recovery helpers. Nothing here replays a Store call
// or automatically discards a Workspace.
import { join, resolve, dirname } from 'node:path';
import { read, write, update, locked, requireThat } from './state.mjs';
import { owner, ownsBranch, tracked, workspaceKey } from './sdk-entry.mjs';
import { controlRoot, loadConfig, nativeSDK } from './runtime.mjs';

export async function recoverWorkspace(ws, reason) {
  requireThat(typeof reason === 'string' && reason.trim(), 'record the evidence connecting this Workspace to this agent');
  const root = controlRoot(process.cwd());
  const config = loadConfig(root);
  const sdk = await nativeSDK(config, root);
  const lf = new sdk.LayerFS(config.project);
  try {
    const status = await lf.workspace.status(ws);
    ownsBranch(status.branch);
    await update(owner().path, (state) => {
      state.workspaces[workspaceKey(ws)] = { ws, branch: status.branch, status: 'mounted', keep: reason, recovered: new Date().toISOString() };
    });
    return status;
  } finally { await lf.close(); }
}

export async function acknowledgeRecord(file, evidence) {
  requireThat(typeof evidence === 'string' && evidence.trim(), 'give the inspected outcome and evidence; this never retries the operation');
  const { agent } = owner();
  const target = resolve(file);
  requireThat([join(agent.directory,'operations'),join(agent.directory,'calls')].includes(dirname(target)), 'record must belong to this session');
  await update(target, (record) => {
    requireThat(record.agent === agent.id, 'record belongs to another agent');
    requireThat(['pending','unknown','bookkeeping-failed','interrupted','running'].includes(record.status), 'record does not need reconciliation');
    record.reconciled = { previous: record.status, evidence, at: new Date().toISOString() };
    record.status = 'inspected';
  });
  return read(target);
}

// The Lead may clean a stopped Worker's registered idle/clean Workspace.
// Inspection and unmount are native calls, not guessed path enumeration.
export async function cleanWorker(agentId, ws) {
  const { agent: lead } = owner();
  requireThat(lead.role === 'lead', 'only Lead can clean a stopped Worker');
  const { agentPath } = await import('./state.mjs');
  const path = agentPath(lead.directory, agentId);
  const root = controlRoot(process.cwd());
  const sdk = await nativeSDK(loadConfig(root),root);
  const lf = new sdk.LayerFS(process.env.DSH_LAYERFS_PROJECT);
  try {
    return await locked(path, async () => {
      const worker = read(path);
      const resource = worker.workspaces[workspaceKey(ws)];
      requireThat(worker.role === 'worker' && worker.stopped && resource?.status === 'mounted' && !resource.keep,
        'Worker must have stopped, and the registered Workspace must not be retained');
      const status = await lf.workspace.status(ws);
      requireThat(status.branch === resource.branch && status.activity === 'Idle' && !status.overlay.inodes && !status.overlay.entries && !status.overlay.pendingMerge, 'Workspace is busy, dirty or changed ownership; ask its owner to resolve it');
      const result = await tracked('workspace.clean-worker', { worker: agentId, workspace: ws }, () => lf.workspace.unmount(ws));
      worker.workspaces[workspaceKey(ws)].status = 'unmounted';
      write(path, worker);
      return result;
    });
  } finally { await lf.close(); }
}
