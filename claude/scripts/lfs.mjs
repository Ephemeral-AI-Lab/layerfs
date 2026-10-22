#!/usr/bin/env node
// DSH's small command surface, with managed Branches and preserved failures.
import { readdirSync, existsSync } from 'node:fs';
import { join } from 'node:path';
import { LayerFS, branches, owner, recordValidation, adoptBranch, retainWorkspace, releaseWorkspace } from './sdk-entry.mjs';
import { LayerFSOperator } from './operator-entry.mjs';
import { recoverWorkspace, acknowledgeRecord, cleanWorker } from './recovery.mjs';
import { read } from './state.mjs';

const USAGE = `usage: node "$DSH_LAYERFS_CLI" <command> ...
  status | resources             Branches / owned Workspaces, jobs and uncertain calls
  log <branch> [n]               newest Commits
  ls <ref> [path] | cat <ref> <path> | grep <ref> <pattern> [path]
  diff <a> <b>                   changed files
  fork <name> [from]             fork and select a Branch (default current Branch)
  use <branch>                   select an owned Branch
  merge <a> <b> <name> [message]  merge and select the returned Branch
  write <branch> <path> [msg]    content from stdin
  run <branch> <command> [msg] [--validate|--discard]
  publish <branch> [commit]      Lead only, after merge main and successful validation
  keep <sandbox> <path> <reason> | release <sandbox> <path>
  commit <sandbox> <path> [msg] | unmount <sandbox> <path> [--discard]
  start <sandbox> <path> <command> <reason> | cancel <process>
  recover <sandbox> <path> <evidence>    inspect and register an orphan on an owned Branch
  acknowledge <record-file> <evidence> record an inspected uncertain outcome
  clean-worker <agent-id> <sandbox> <path>  Lead: stopped Worker's idle/clean resource
Every mutation is attempted once. Refusals exit 2. run returns the command exit code.
SDK scripts can use retainWorkspace, releaseWorkspace, adoptBranch, recordValidation
from DSH_LAYERFS_SDK for the same host bookkeeping.`;

const [command, ...rest] = process.argv.slice(2);
const discard = rest.includes('--discard');
const validate = rest.includes('--validate');
const args = rest.filter((a) => !['--discard','--validate'].includes(a));
const lf = new LayerFS(process.env.DSH_LAYERFS_PROJECT);
const op = new LayerFSOperator(lf);
const json = (value) => console.log(JSON.stringify(value, null, 2));
const need = (n) => { if (args.length < n) throw new Error(USAGE); };
const ws = () => { need(2); return { sandbox: args[0], path: args[1] }; };
const stdin = async () => { const parts = []; for await (const c of process.stdin) parts.push(c); return Buffer.concat(parts).toString('utf8'); };
const records = (dir) => existsSync(dir) ? readdirSync(dir).filter((n) => n.endsWith('.json')).map((n) => ({ file: join(dir,n), ...read(join(dir,n)) })) : [];
let exitCode = 0;

async function all(method) {
  const items = [];
  for (let pos = 0; ;) {
    const page = await method(pos);
    items.push(...page.items);
    if (!page.hasMore) return items;
    if (!page.items.length) throw new Error('pagination did not advance');
    pos = page.nextPagePos ?? pos + page.items.length;
  }
}
async function run() {
  need(2);
  if (discard && validate) throw new Error('--discard and --validate cannot be combined');
  const [branch, command, message] = args;
  const workspace = await lf.workspace.mount(branch);
  let saved = false;
  try {
    const result = await lf.workspace.exec(workspace, command, undefined, { stdout: process.stdout, stderr: process.stderr });
    exitCode = result.exitCode;
    if (!discard) {
      const commit = await lf.workspace.commit(workspace, message || `${command.split('\n')[0].slice(0,200)} (exit ${result.exitCode})`);
      saved = true;
      console.error(JSON.stringify({ exitCode, commit, workspace }));
      if (validate && exitCode === 0) json(await recordValidation(lf, branch, command, exitCode, commit));
    }
    if (discard || saved) await lf.workspace.unmount(workspace, discard);
  } catch (error) {
    // A successful Commit can be unmounted without discarding. Otherwise the
    // Workspace is the recovery surface for Conflict, Dirty or a lost reply.
    if (saved) await lf.workspace.unmount(workspace).catch((e) => console.error(`unmount: ${e.message}`));
    console.error(`Workspace retained unless explicitly unmounted: ${JSON.stringify(workspace)}`);
    throw error;
  }
}
const commands = {
  async status() { json({ project: lf.projectName, branch: owner().agent.branch, branches: await branches(lf) }); },
  async resources() {
    const { agent } = owner();
    const agents = agent.role === 'lead' ? records(join(agent.directory, 'agents')) : [agent];
    const ids = new Set(agents.map((a) => a.id));
    json({ agent, workers: agents.filter((a) => a.role === 'worker'),
      operations: records(join(agent.directory, 'operations')).filter((r) => ids.has(r.agent) && ['pending','unknown','bookkeeping-failed'].includes(r.status)),
      calls: records(join(agent.directory, 'calls')).filter((r) => ids.has(r.agent) && r.id !== process.env.CC_LAYERFS_CALL && ['running','interrupted'].includes(r.status)).map(({ env, command, ...record }) => record),
      processes: await op.process.list() });
  },
  async log() { need(1); json(await lf.branch.list_commits(args[0], Number(args[1] || 20))); },
  async ls() { need(1); json(await all((pos) => lf.fs.list(args[0], args[1] || '/', 1000, pos))); },
  async cat() { need(2); for (let line = 1; line;) { const page = await lf.fs.read(args[0], args[1], line); process.stdout.write(page.text); line = page.nextLine; } },
  async grep() { need(2); json(await all((pos) => lf.fs.grep(args[0], args[1], args[2] || '/', 1000, pos))); },
  async diff() { need(2); json(await all((pos) => lf.branch.diff_branches(args[0], args[1], 1000, pos))); },
  async fork() { need(1); json(await lf.branch.fork(args[0], args[1] || owner().agent.branch)); },
  async use() { need(1); await adoptBranch(args[0]); json({ branch: args[0] }); },
  async merge() { need(3); json(await lf.branch.merge(...args)); },
  async write() { need(2); json(await lf.fs.write(args[0], args[1], await stdin(), args[2])); },
  run,
  async publish() { need(1); json(await lf.branch.publish(...args)); },
  async keep() { need(3); await retainWorkspace(ws(), args[2]); },
  async release() { await releaseWorkspace(ws()); },
  async commit() { json(await lf.workspace.commit(ws(), args[2])); },
  async unmount() { json(await lf.workspace.unmount(ws(), discard)); },
  async start() { need(4); const workspace = ws(); const process = await op.process.start(workspace, args[2]); await retainWorkspace(workspace, args[3]); json(process); },
  async cancel() { need(1); json(await op.process.cancel(args[0])); },
  async recover() { need(3); json(await recoverWorkspace(ws(), args[2])); },
  async acknowledge() { need(2); json(await acknowledgeRecord(args[0], args[1])); },
  async 'clean-worker'() { need(3); json(await cleanWorker(args[0], { sandbox: args[1], path: args[2] })); }
};
try {
  if (!command || command === 'help' || command === '--help') console.log(USAGE);
  else {
    if (!commands[command]) throw new Error(USAGE);
    await commands[command]();
  }
} catch (error) { console.error(`${error.code ?? 'lfs'}: ${error.message}`); exitCode = 2; }
finally { await lf.close(); }
process.exitCode = exitCode;
