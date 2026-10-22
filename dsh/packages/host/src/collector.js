// Trace collection: each agent's bash calls append SDK trace lines to that
// agent's own file. The collector reads new lines as they arrive, keeps them
// per root session for the views, and writes each finished call to the
// calling agent's session as a plugin:layerfs/call record. Finished lines are
// also journaled per root session under the state folder, since trace files
// live in temporary storage.
import { appendFileSync, closeSync, existsSync, mkdirSync, openSync, readFileSync, readSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";
import { UNREACHABLE } from "@layerfs/port";
import { appendCall } from "./records.js";

const MAX_CHUNK = 1 << 20;

export class Collector {
  /**
   * @param {object} ctx plugin context (ctx.sessions)
   * @param {object} spec resolved Host settings
   * @param {import("./binding.js").BindingStore} bindings
   */
  constructor(ctx, spec, bindings) {
    this.ctx = ctx;
    this.spec = spec;
    this.bindings = bindings;
    /** sessionId -> { file, offset, rest, project } */
    this.tails = new Map();
    /** rootId -> { lines: [], seen: Set<agentId|id>, running: Map<id, line>, loaded } */
    this.roots = new Map();
    /** sessionId -> label from its subagent descriptor */
    this.labels = new Map();
    /** sessionId -> records waiting for the Session to be live again */
    this.pending = new Map();
    /** sessions whose files are read on the next turn of the event loop */
    this.soon = new Set();
  }

  /** The agent a Session belongs to: its id, root session, display name and folder. */
  agentOf(session) {
    const id = session.header.id;
    let root = session, guard = 0;
    while (root.header.origin === "subagent" && root.header.parentSession && guard++ < 16) {
      const parent = this.ctx.sessions.get(root.header.parentSession);
      if (!parent) break;
      root = parent;
    }
    const rootId = root === session && session.header.origin === "subagent" ? this.bindings.agent(id)?.root ?? id : root.header.id;
    const name = id === rootId ? "main" : this.labels.get(id) ?? this.bindings.agent(id)?.name ?? `agent-${String(id).slice(-4)}`;
    this.bindings.setAgent(id, rootId, name);
    return { id, root: rootId, name, cwd: root.header.cwd };
  }

  /**
   * Observes committed Session events. A parent's subagent/catalog entry names
   * its child; the child's own descriptor is written before its Session is
   * attached, so it is seen only when a later descriptor replaces it.
   */
  observe(session, event) {
    this.afterEvent(session.header.id);
    const data = event.data;
    if (!data || typeof data.label !== "string") return;
    const id = event.type === "subagent/catalog" ? data.childId : event.type === "subagent/descriptor" ? session.header.id : undefined;
    const label = data.label.trim().slice(0, 40);
    if (typeof id !== "string" || !label) return;
    this.labels.set(id, label);
    const known = this.bindings.agent(id);
    if (known) this.bindings.setAgent(id, known.root, label);
  }

  /** Starts following an agent's trace file and returns its path. */
  track(agent, project) {
    const file = join(this.spec.traceDir, `${agent.id}.jsonl`);
    if (!this.tails.has(agent.id)) {
      // lines already in the file were collected before this Host started
      const offset = existsSync(file) ? statSync(file).size : 0;
      this.tails.set(agent.id, { file, offset, rest: "", project: project.name });
      this.load(agent.root);
    } else this.tails.get(agent.id).project = project.name;
    return file;
  }

  /**
   * A traced agent's Session moved (a tool result, a turn ending): its last
   * lines are read and recorded before the Session may go idle. Records are
   * appended after the observer returns, since a Session refuses an append
   * while it publishes one.
   */
  afterEvent(sessionId) {
    if (!this.tails.has(sessionId) && !this.pending.has(sessionId)) return;
    if (this.soon.size === 0) setImmediate(() => {
      const ids = [...this.soon];
      this.soon.clear();
      try { this.poll(ids); } catch (error) { this.ctx.logger?.warn?.(`layerfs: trace collection failed: ${error.message}`); }
    });
    this.soon.add(sessionId);
  }

  /** Reads new bytes of the followed files (all of them by default). */
  poll(ids = [...this.tails.keys()]) {
    for (const sessionId of ids) {
      this.flush(sessionId);
      const tail = this.tails.get(sessionId);
      if (!tail || !existsSync(tail.file)) continue;
      const size = statSync(tail.file).size;
      if (size < tail.offset) { tail.offset = 0; tail.rest = ""; }
      while (tail.offset < size) {
        const length = Math.min(MAX_CHUNK, size - tail.offset), buffer = Buffer.alloc(length), fd = openSync(tail.file, "r");
        try { readSync(fd, buffer, 0, length, tail.offset); } finally { closeSync(fd); }
        tail.offset += length;
        const text = tail.rest + buffer.toString("utf8"), parts = text.split("\n");
        tail.rest = parts.pop();
        for (const raw of parts) this.take(sessionId, tail.project, raw, true);
      }
    }
  }

  /** One trace line from an agent's file. */
  take(sessionId, project, raw, live) {
    let line;
    try { line = JSON.parse(raw); } catch { return; }
    if (!line || line.layerfs_trace !== 1) return;
    const agent = this.bindings.agent(sessionId) ?? { root: sessionId, name: "main" };
    const view = this.view(agent.root);
    const own = { ...stripped(line), agent: agent.name, agentId: sessionId, project };
    if (line.phase === "start") { view.running.set(line.id, own); return; }
    // an agent's call found the Store container down: the Host starts it again (the call keeps its answer)
    if (live && line.ok === false && UNREACHABLE.test(String(line.message ?? ""))) this.onUnreachable?.();
    view.running.delete(line.id);
    if (!this.keep(view, own)) return;
    if (!live) return;
    this.journal(agent.root, own);
    const record = { agent: { id: sessionId, name: agent.name }, project, line: stripped(line) };
    if (!this.pending.has(sessionId)) this.pending.set(sessionId, []);
    this.pending.get(sessionId).push(record);
    this.flush(sessionId);
  }

  /** Appends an agent's waiting records once its Session is live. */
  flush(sessionId) {
    const records = this.pending.get(sessionId), session = records && this.ctx.sessions.get(sessionId);
    if (!session) return;
    this.pending.delete(sessionId);
    for (const record of records) appendCall(session, record);
  }

  view(rootId) {
    if (!this.roots.has(rootId)) this.roots.set(rootId, { lines: [], seen: new Set(), running: new Map(), loaded: false });
    return this.roots.get(rootId);
  }

  /** Adds a finished line once; false when the view has it already. */
  keep(view, line) {
    const key = `${line.agentId}|${line.id}`;
    if (view.seen.has(key)) return false;
    view.seen.add(key);
    view.lines.push(line);
    return true;
  }

  journalFile(rootId) { return join(this.spec.stateDir, "lines", `${encodeURIComponent(rootId)}.jsonl`); }

  journal(rootId, line) {
    try {
      mkdirSync(join(this.spec.stateDir, "lines"), { recursive: true });
      appendFileSync(this.journalFile(rootId), `${JSON.stringify(line)}\n`);
    } catch (error) { this.ctx.logger?.warn?.(`layerfs: the line journal could not be written: ${error.message}`); }
  }

  /** Loads the lines a root session's agents wrote before this Host started. */
  load(rootId) {
    const view = this.view(rootId);
    if (view.loaded) return view;
    view.loaded = true;
    const journal = this.journalFile(rootId);
    if (existsSync(journal)) for (const raw of readFileSync(journal, "utf8").split("\n")) {
      try { if (raw) this.keep(view, JSON.parse(raw)); } catch { /* a torn last line */ }
    }
    if (existsSync(this.spec.traceDir)) for (const entry of readdirSync(this.spec.traceDir)) {
      if (!entry.endsWith(".jsonl")) continue;
      const sessionId = entry.slice(0, -6), agent = this.bindings.agent(sessionId);
      if (!agent || agent.root !== rootId) continue;
      const file = join(this.spec.traceDir, entry), tail = this.tails.get(sessionId);
      const end = tail ? tail.offset : statSync(file).size, fd = openSync(file, "r"), buffer = Buffer.alloc(end);
      try { readSync(fd, buffer, 0, end, 0); } finally { closeSync(fd); }
      for (const raw of buffer.toString("utf8").split("\n")) this.take(sessionId, tail ? tail.project : null, raw, false);
    }
    view.lines.sort((x, y) => Date.parse(x.start) - Date.parse(y.start));
    return view;
  }

  /** What the views need for one root session. */
  snapshot(rootId) {
    const view = this.load(rootId);
    return { lines: view.lines, running: [...view.running.values()], agents: this.bindings.agentsOf(rootId) };
  }

  /**
   * What the journaled sessions know of a project's nodes that LayerFS does
   * not report: who made each Commit and Layer ({ real id: agent name }),
   * when each Layer was published ({ real id: epoch ms }), and when each
   * Branch was forked ({ name: epoch ms }; the latest fork of a reused name).
   * @param {string} project
   */
  nodesOf(project) {
    const authors = {}, times = {}, forks = {}, out = { authors, times, forks };
    for (const line of this.journaled(project)) {
      if (!line.ok) continue;
      const made = line.kind === "Committed" ? line.commit : line.method === "branch.publish" ? line.layer : null;
      if (made && !authors[made]) authors[made] = this.bindings.agent(line.agentId)?.name ?? line.agent;
      const at = Date.parse(line.start) + (line.ms || 0);
      if (line.method === "branch.publish" && line.layer) times[line.layer] = at;
      if ((line.kind === "Forked" || line.kind === "Merged") && line.branch) forks[line.branch] = Math.max(forks[line.branch] ?? 0, at);
    }
    return out;
  }

  /**
   * The project's Workspaces the agents' traces know, by `${sandbox}\n${path}`:
   * who mounted it, when, and when a call last used it (the newest mount of a
   * reused path; an unmount forgets it). Who the daemon has mounted is the
   * snapshot's answer; this only names the agent and its last use.
   * @param {string} project
   */
  mountsOf(project) {
    const lines = [...this.journaled(project)].sort((x, y) => Date.parse(x.start) - Date.parse(y.start)), out = {};
    for (const line of lines) {
      const sandbox = line.sandbox ?? line.args?.sandbox, path = line.path ?? line.args?.path;
      if (typeof sandbox !== "string" || typeof path !== "string") continue;
      const key = `${sandbox}\n${path}`, at = Date.parse(line.start) + (line.ms || 0);
      if (line.method === "workspace.mount" && line.ok) out[key] = { agent: this.bindings.agent(line.agentId)?.name ?? line.agent, at, lastAt: at };
      else if (line.method === "workspace.unmount" && line.ok) delete out[key];
      else if (out[key]) out[key].lastAt = Math.max(out[key].lastAt, at);
    }
    return out;
  }

  /** How many LayerFS calls of agents are running now, in any project. */
  runningCalls() {
    let n = 0;
    for (const view of this.roots.values()) for (const line of view.running.values()) if (line.project) n++;
    return n;
  }

  /** The project's Workspaces an agent's call is running in now, as `${sandbox}\n${path}`. */
  busyOf(project) {
    const out = new Set();
    for (const view of this.roots.values()) {
      for (const line of view.running.values()) {
        if (line.project === project && typeof line.args?.sandbox === "string" && typeof line.args?.path === "string") out.add(`${line.args.sandbox}\n${line.args.path}`);
      }
    }
    return out;
  }

  /** Every journaled line of `project`, file by file. */
  *journaled(project) {
    const dir = join(this.spec.stateDir, "lines");
    if (!existsSync(dir)) return;
    for (const entry of readdirSync(dir)) {
      if (!entry.endsWith(".jsonl")) continue;
      for (const raw of readFileSync(join(dir, entry), "utf8").split("\n")) {
        let line;
        try { line = raw ? JSON.parse(raw) : null; } catch { continue; /* a torn last line */ }
        if (line && line.project === project) yield line;
      }
    }
  }

  /** Polls on an interval until the returned disposer runs. */
  start() {
    const timer = setInterval(() => {
      try { this.poll(); } catch (error) { this.ctx.logger?.warn?.(`layerfs: trace collection failed: ${error.message}`); }
    }, this.spec.pollMs);
    timer.unref?.();
    return () => { clearInterval(timer); this.poll(); };
  }
}

/** A line as records and views keep it: no daemon telemetry or observation scope. */
function stripped(line) {
  const { daemon, scope, ...rest } = line;
  return rest;
}
