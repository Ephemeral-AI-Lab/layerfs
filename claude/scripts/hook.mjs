import { appendFileSync, readdirSync, existsSync, mkdirSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { controlRoot, loadConfig, checkDeployment, environment, nativeSDK, pluginRoot } from './runtime.mjs';
import { read, write, optional, update, locked, key, branchName, sessionDir, agentPath, callPath, requireThat } from './state.mjs';

const quote = (value) => `'${String(value).replaceAll("'", "'\\''")}'`;
const context = (event, text) => ({ hookSpecificOutput: { hookEventName: event, additionalContext: text } });
const deny = (reason) => ({ hookSpecificOutput: { hookEventName: 'PreToolUse', permissionDecision: 'deny', permissionDecisionReason: `LayerFS: ${reason}` } });
const files = (dir) => existsSync(dir) ? readdirSync(dir).filter((n) => n.endsWith('.json')).map((n) => read(join(dir, n))) : [];

function agentId(input) {
  if (input.agent_type === 'layerfs:worker') {
    requireThat(input.agent_id, 'Worker identity is missing; refusing to use the Lead binding');
    return input.agent_id;
  }
  requireThat(input.agent_type === 'layerfs:lead' && !input.agent_id, 'start the main session with --agent layerfs:lead; only layerfs:worker may delegate');
  return 'lead';
}
function bound(input, root, config) {
  const dir = sessionDir(root, input.session_id);
  const session = read(join(dir, 'session.json'));
  requireThat(session.id === input.session_id && session.config === config.identity && session.status === 'ready', 'session is not bound to this deployment');
  const path = agentPath(dir, agentId(input));
  const agent = read(path);
  requireThat(agent.status === 'ready' && agent.config === config.identity && agent.directory === dir,
    `agent binding is ${agent.status} or has moved; inspect ${path}`);
  return { dir, path, agent };
}
function describe(agent) {
  const held = Object.values(agent.workspaces).filter((w) => w.status === 'mounted');
  return `LayerFS project is in the Store; cwd is control/scratch only. Role: ${agent.role}. Current Branch: ${agent.branch}.
Every Bash gets CC_LAYERFS_BRANCH and the DSH_LAYERFS_* SDK variables. Read cat "$DSH_LAYERFS_SKILL" before the first SDK operation; its "Inside dsh" section applies here too. No npm SDK install is needed.
Use fs.* for project reads/edits, explicit Workspace mount → exec → commit → unmount for project commands. Bash itself is not automatically a Workspace.
Fork/merge returns a new Branch and selects it for later calls; use the returned Branch in this script. main is read-only. Only Lead publishes after merging current main and validating.
Native /rewind does not undo SDK changes. Inspect resources with node "$DSH_LAYERFS_CLI" resources. Retained Workspaces: ${JSON.stringify(held)}.`;
}
async function createAgent(path, data, lf, source) {
  return locked(path, async () => {
    const existing = optional(path);
    if (existing) {
      requireThat(existing.status === 'ready' && existing.config === data.config && existing.directory === data.directory,
        `incomplete or moved Branch binding; inspect ${path}, do not replay its fork`);
      return existing;
    }
    const state = { ...data, source, branches: [], workspaces: {}, processes: {}, integrations: {}, validation: null, status: 'starting' };
    write(path, state);
    try {
      const result = await lf.branch.fork(state.branch, source);
      state.fork = result;
      state.branches = [result.branch];
      state.status = 'ready';
      write(path, state);
      return state;
    } catch (error) {
      state.status = state.fork ? 'bookkeeping-failed' : 'fork-not-confirmed';
      state.error = error.message;
      write(path, state);
      throw error;
    }
  });
}

export async function handle(input, { deploymentCheck = checkDeployment } = {}) {
  requireThat(input.session_id && input.cwd, 'Hook input needs session_id and cwd');
  const event = input.hook_event_name;
  const root = controlRoot(input.cwd);
  const config = loadConfig(root);
  const dir = sessionDir(root, input.session_id);
  if (event === 'SessionStart') {
    agentId(input);
    const sessionFile = join(dir, 'session.json');
    const previous = optional(sessionFile);
    requireThat(!previous || previous.config === config.identity, 'cannot change project/deployment during a session');
    requireThat(previous || !['resume','compact'].includes(input.source), 'missing original binding; restore the control records before resuming');
    write(sessionFile, { ...previous, id: input.session_id, config: config.identity, status: 'binding' });
    deploymentCheck(config);
    const { LayerFS } = await nativeSDK(config, root);
    const lf = new LayerFS(config.project);
    let lead;
    try {
      lead = await createAgent(agentPath(dir, 'lead'), { id: 'lead', role: 'lead', branch: branchName(input.session_id, 'lead'), config: config.identity, directory: dir }, lf, 'main');
    } finally { await lf.close(); }
    write(sessionFile, { id: input.session_id, config: config.identity, project: config.project, status: 'ready', started: previous?.started ?? new Date().toISOString() });
    if (process.env.CLAUDE_ENV_FILE) {
      const common = environment(config, root, {}, {});
      appendFileSync(process.env.CLAUDE_ENV_FILE, `\n${Object.entries(common).map(([k,v]) => `export ${k}=${quote(v)}`).join('\n')}\n`);
    }
    return context(event, describe(lead));
  }
  if (event === 'SubagentStart') {
    requireThat(input.agent_type === 'layerfs:worker' && input.agent_id, 'expected a managed Worker');
    const session = read(join(dir, 'session.json'));
    requireThat(session.status === 'ready' && session.config === config.identity, 'Lead session binding failed');
    const lead = read(agentPath(dir, 'lead'));
    const { LayerFS } = await nativeSDK(config, root);
    const lf = new LayerFS(config.project);
    let worker;
    try {
      // Pin the current committed version; later Lead writes cannot move it.
      let parent;
      for (let pos = 0; ;) {
        const page = await lf.project.list_branches(1000, pos);
        parent = page.items.find((b) => b.name === lead.branch);
        if (parent || !page.hasMore) break;
        requireThat(page.items.length, 'branch pagination did not advance');
        pos = page.nextPagePos ?? pos + page.items.length;
      }
      requireThat(parent && (parent.head || parent.base), 'cannot resolve the Lead version');
      worker = await createAgent(agentPath(dir, input.agent_id), { id: input.agent_id, role: 'worker', branch: branchName(input.session_id, input.agent_id), config: config.identity, directory: dir }, lf, parent.head || parent.base);
    } finally { await lf.close(); }
    return context(event, describe(worker));
  }
  const { path, agent } = bound(input, root, config);
  if (event === 'PreToolUse') {
    await update(path, (state) => { delete state.stopped; });
    const allowed = agent.role === 'lead' ? ['Bash','Skill','Agent','TaskStop','AskUserQuestion'] : ['Bash','Skill','TaskStop'];
    requireThat(allowed.includes(input.tool_name), `${input.tool_name} is outside the LayerFS tool set`);
    if (input.tool_name === 'Agent') {
      requireThat(input.tool_input?.subagent_type === 'layerfs:worker', 'delegate only to layerfs:worker');
      requireThat(!input.tool_input.isolation && !input.tool_input.team_name, 'use LayerFS Branches instead of Git worktrees or agent teams');
      return {};
    }
    if (input.tool_name !== 'Bash') return {};
    requireThat(input.tool_use_id && typeof input.tool_input?.command === 'string', 'Bash identity or command is missing');
    const file = callPath(dir, input.tool_use_id);
    requireThat(!existsSync(file), `call already registered; inspect ${file}, do not replay it`);
    mkdirSync(join(dir, 'traces'), { recursive: true, mode: 0o700 });
    const env = environment(config, root, {
      CC_LAYERFS_AGENT_RECORD: path, CC_LAYERFS_ROLE: agent.role, CC_LAYERFS_BRANCH: agent.branch,
      CC_LAYERFS_CALL: input.tool_use_id, DSH_LAYERFS_TRACE: join(dir, 'traces', `${key(agent.id)}.jsonl`),
      DSH_LAYERFS_TRACE_STEP: input.tool_use_id, DSH_LAYERFS_TRACE_START: '1'
    }, {});
    write(file, { id: input.tool_use_id, agent: agent.id, command: input.tool_input.command, cwd: root, env, status: 'prepared', prepared: new Date().toISOString() });
    return { hookSpecificOutput: { hookEventName: event, updatedInput: { ...input.tool_input,
      command: `${quote(process.execPath)} ${quote(join(pluginRoot, 'scripts/run-call.mjs'))} ${quote(file)}` },
      additionalContext: `LayerFS ${agent.role}; Branch ${agent.branch}. Original command recorded in ${file}.` } };
  }
  if (event === 'PostToolUse' || event === 'PostToolUseFailure') {
    if (input.tool_name === 'Bash' && input.tool_use_id) {
      const file = callPath(dir, input.tool_use_id);
      if (existsSync(file)) await update(file, (call) => {
        call.hook = event;
        call.observed = new Date().toISOString();
        call.toolError = input.error;
        // A background task may still be live; PostToolUse is not its completion.
        call.task = input.tool_response?.task_id ?? input.tool_response?.backgroundTaskId;
        call.outputFile = input.tool_response?.output_file ?? input.tool_response?.outputFile;
      });
    }
    return {};
  }
  if (event === 'Stop' || event === 'SubagentStop') {
    const pending = files(join(dir, 'operations')).filter((o) => o.agent === agent.id && ['pending','unknown','bookkeeping-failed'].includes(o.status));
    const held = Object.values(agent.workspaces).filter((w) => w.status === 'mounted' && !w.keep);
    const calls = files(join(dir, 'calls')).filter((c) => c.agent === agent.id && ['running','interrupted'].includes(c.status));
    await update(path, (state) => { state.lastReport = input.last_assistant_message; state.stopped = new Date().toISOString(); });
    if ((pending.length || held.length || calls.length) && !input.stop_hook_active) return {
      decision: 'block', reason: `LayerFS has ${held.length} unretained Workspaces, ${pending.length} uncertain operations and ${calls.length} unfinished/interrupted calls. Inspect lfs resources, commit/unmount or explicitly retain long jobs; report unknown outcomes. Do not discard automatically.`
    };
    return {};
  }
  if (event === 'SessionEnd') {
    await update(join(dir, 'session.json'), (session) => { session.ended = new Date().toISOString(); session.reason = input.reason; });
    return {};
  }
  return {};
}

export async function dispatch(input, dependencies) {
  try {
    return await handle(input, dependencies);
  } catch (error) {
    if (input?.hook_event_name === 'PreToolUse') return deny(error.message);
    else if (['SessionStart','SubagentStart'].includes(input?.hook_event_name)) {
      return context(input.hook_event_name, `LayerFS binding failed: ${error.message}. Do not perform project work. PreToolUse will refuse unbound operations.`);
    } else throw error;
  }
}
if (process.argv[1] === fileURLToPath(import.meta.url)) {
  try {
    let text = ''; for await (const chunk of process.stdin) text += chunk;
    console.log(JSON.stringify(await dispatch(JSON.parse(text))));
  } catch (error) {
    console.error(`LayerFS hook: ${error.stack ?? error}`);
    process.exitCode = 1;
  }
}
