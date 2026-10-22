import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { loadConfig, controlRoot } from './runtime.mjs';
import { ownsWorkspace, owner, tracked, workspaceKey } from './sdk-entry.mjs';
import { requireThat } from './state.mjs';

const config = loadConfig(controlRoot(process.cwd()));
const native = await import(pathToFileURL(join(config.sdk, 'operator/index.js')).href);
export class LayerFSOperator extends native.LayerFSOperator {
  constructor(client) {
    super(client);
    for (const name of ['overlay','changes']) {
      const method = this.workspace[name];
      this.workspace[name] = (ws, ...args) => { ownsWorkspace(ws); return method(ws, ...args); };
    }
    const start = this.process.start;
    this.process.start = async (ws, command) => {
      ownsWorkspace(ws);
      return tracked('process.start', { workspace: ws }, () => start(ws, command), (state, result) => {
        state.processes[result.process] = { workspace: workspaceKey(ws), status: 'running' };
      });
    };
    const cancel = this.process.cancel;
    this.process.cancel = async (process) => {
      requireThat(owner().agent.processes[process], `process ${process} is not owned by this agent`);
      return tracked('process.cancel', { process }, () => cancel(process), (state) => { state.processes[process].status = 'cancelled'; });
    };
  }
}
