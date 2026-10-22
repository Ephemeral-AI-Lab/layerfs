// Test double for the native addon only. The real JS SDK and plugin execute.
const fs = require('node:fs');
const initial = () => ({ n: 0, branches: { main: { name: 'main', head: null, base: 'layer0' } }, workspaces: {}, processes: {}, calls: [] });
const load = () => fs.existsSync(process.env.LFS_TEST_STATE) ? JSON.parse(fs.readFileSync(process.env.LFS_TEST_STATE)) : initial();
exports.storeOpen = async (args) => ({ value: { opened: args.storePath } });
exports.storeClose = async () => ({ value: {} });
async function call(method,args) {
  const state = load();
  state.calls.push({ method, args });
  let value;
  let refused;
  const fail = (code) => { refused = { code, message: code, detail: null }; };
  const branch = state.branches[args.branch];
  const wkey = `${args.sandbox}:${args.path}`;
  const ws = state.workspaces[wkey];
  if (method === 'project.list_branches') value = { items: Object.values(state.branches), hasMore: false };
  else if (method === 'branch.fork' || method === 'branch.merge') {
    if (state.branches[args.name]) fail('Exists');
    else {
      const from = state.branches[method === 'branch.fork' ? args.from : args.branch] || { head: args.from, base: 'layer0' };
      const merged = method === 'branch.merge';
      value = { kind: merged ? 'Merged' : 'Forked', branch: args.name, base: from.base, head: merged ? `h${++state.n}` : from.head };
      state.branches[args.name] = { ...value, name: args.name };
    }
  } else if (method === 'branch.publish') {
    state.branches.main.base = `layer${++state.n}`;
    value = { kind: 'Added', source: { branch: args.branch, commit: args.commit }, publishedLayer: { id: state.branches.main.base }, overwrote: [] };
  } else if (method === 'workspace.mount') {
    value = { sandbox: '1', path: `/workspaces/${++state.n}`, image: args.deploy.image };
    state.workspaces[`1:${value.path}`] = { ...value, branch: args.branch, dirty: false };
  } else if (method.startsWith('workspace.') && !ws) fail('NotMounted');
  else if (method === 'workspace.exec') {
    ws.dirty = !args.command.includes('no-change');
    ws.conflict = args.command.includes('conflict');
    ws.merged = args.command.includes('merge-during-test');
    ws.unmountUnknown = args.command.includes('unmount-unknown');
    if (args.command === 'unknown') fail('Unknown');
    else value = { exitCode: args.command.includes('fail') ? 7 : 0, stdout: 'out\n', stderr: 'err\n' };
  } else if (method === 'workspace.commit') {
    if (ws.conflict) fail('Conflict');
    else if (!ws.dirty) value = { kind: 'UpToDate', head: state.branches[ws.branch].head };
    else { state.branches[ws.branch].head = `h${++state.n}`; ws.dirty = false; value = { kind: 'Committed', head: state.branches[ws.branch].head, merged: ws.merged ? {from:'other',head:'other'} : null }; }
  } else if (method === 'workspace.unmount') {
    if (ws.unmountUnknown) { fail('Unknown'); refused.detail = { operation: 'unmount', outcome: 'uncertain' }; }
    else if (ws.dirty && !args.discard) fail('Dirty');
    else { delete state.workspaces[wkey]; value = { kind: 'Unmounted' }; }
  } else if (method === 'workspace.status') value = { branch: ws.branch, activity: 'Idle', overlay: { inodes: ws.dirty ? 1 : 0, entries: 0, pendingMerge: ws.conflict || false } };
  else if (/^fs\.(write|edit|apply|mkdir|rm|cp|mv)$/.test(method)) {
    branch.head = `h${++state.n}`;
    value = { kind: 'Committed', head: branch.head };
  } else if (method === 'fs.read') value = { text: 'hello\n', nextLine: null };
  else if (method === 'operator.process.start') {
    const process = `proc${++state.n}`;
    state.processes[process] = { process, sandbox: args.sandbox, path: args.path };
    value = { process, kind: 'running' };
  } else if (method === 'operator.process.cancel') { delete state.processes[args.process]; value = { kind: 'cancelled' }; }
  else if (method === 'operator.process.list') value = { items: Object.values(state.processes) };
  else value = { items: [], hasMore: false };
  fs.writeFileSync(process.env.LFS_TEST_STATE,JSON.stringify(state));
  return refused ? { refused } : { value };
}
exports.call = call;
exports.operatorCall = call;
