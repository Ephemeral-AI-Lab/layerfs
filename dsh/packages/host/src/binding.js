// Project bindings: the LayerFS projects, each dsh workspace folder's default
// project (the one chosen there last), which project each root session uses,
// and the names of the agents seen. A session may use any project; the folder
// only remembers a default. One JSON file under the state directory, replaced
// atomically on every change. Several dsh processes may share the directory
// (dsh web and a headless profile pointed at it): every change re-reads the
// file under a lock and applies itself to what is there, and reads pick up
// another process's changes, so no process overwrites another's bindings.
import { existsSync, mkdirSync, readFileSync, renameSync, rmdirSync, statSync, writeFileSync } from "node:fs";
import { join } from "node:path";

/**
 * @typedef {object} Project
 * @property {string} name LayerFS project name (also its LayerFS home entry)
 * @property {string} source the folder Init imported
 * @property {string} volume Docker volume holding the project's Store
 * @property {"sim"} [backend] "sim": a simulated project (sim/), with no volume
 * @property {number} created Unix epoch milliseconds
 */

/**
 * Version 1 kept each folder's projects in the folder; version 2 kept projects
 * by name, each attached to a folder; version 3 keeps only each folder's
 * default. A folder whose one attached project was its default by being the
 * only one keeps it as the default.
 */
function upgrade(data) {
  if (data.version === 3) return data;
  const projects = {}, workspaces = {}, attached = {};
  const add = (p, folder) => { const { folder: _, ...rest } = p; projects[p.name] = rest; if (folder) (attached[folder] ||= []).push(p.name); };
  if (data.version === 1) {
    for (const [folder, w] of Object.entries(data.workspaces)) { for (const p of w.projects) add(p, folder); workspaces[folder] = { lastUsed: w.lastUsed ?? null }; }
  } else {
    for (const p of Object.values(data.projects)) add(p, p.folder);
    for (const [folder, w] of Object.entries(data.workspaces)) workspaces[folder] = { lastUsed: w.lastUsed ?? null };
  }
  for (const [folder, names] of Object.entries(attached)) if (names.length === 1) (workspaces[folder] ||= { lastUsed: null }).lastUsed ??= names[0];
  return { ...data, version: 3, projects, workspaces };
}

export class BindingStore {
  /** @param {string} stateDir */
  constructor(stateDir) {
    this.file = join(stateDir, "bindings.json");
    this.lockDir = `${this.file}.lock`;
    this.stamp = null;
    this.depth = 0;
    this._data = { version: 3, projects: {}, workspaces: {}, sessions: {}, agents: {} };
    this.reload();
  }

  /** Reads the file again when another process replaced it since the last read. */
  reload() {
    let stat;
    try { stat = statSync(this.file); } catch { return; }
    const stamp = `${stat.ino}:${stat.size}:${stat.mtimeMs}`;
    if (stamp === this.stamp) return;
    this._data = upgrade(JSON.parse(readFileSync(this.file, "utf8")));
    this.stamp = stamp;
  }

  /** The current bindings, re-read when another process changed them. */
  get data() { if (this.depth === 0) this.reload(); return this._data; }

  /**
   * One change: under the directory's lock, re-reads the file, applies `change`
   * to its current content and writes it back, so concurrent processes' changes
   * all survive. A lock older than 10 s is a crashed holder's and is taken over.
   */
  update(change) {
    if (this.depth > 0) return change(this._data);
    const deadline = Date.now() + 10_000;
    for (;;) {
      try { mkdirSync(this.lockDir); break; } catch (error) {
        if (error.code !== "EEXIST") throw error;
        try { if (Date.now() - statSync(this.lockDir).mtimeMs > 10_000) { rmdirSync(this.lockDir); continue; } } catch { continue; }
        if (Date.now() > deadline) throw Object.assign(new Error(`bindings are locked: ${this.lockDir}`), { code: "busy" });
        Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 5);
      }
    }
    this.depth++;
    try {
      this.stamp = null;
      this.reload();
      const result = change(this._data);
      this.save();
      return result;
    } finally { this.depth--; try { rmdirSync(this.lockDir); } catch {} }
  }

  save() {
    const tmp = `${this.file}.${process.pid}.tmp`;
    writeFileSync(tmp, JSON.stringify(this._data, null, 1) + "\n");
    renameSync(tmp, this.file);
    const stat = statSync(this.file);
    this.stamp = `${stat.ino}:${stat.size}:${stat.mtimeMs}`;
  }

  /**
   * Every project a session in `folder` may use, those imported from the
   * folder first, and the folder's default.
   * @param {string} folder @returns {{ projects: Project[], lastUsed: string|null }}
   */
  workspace(folder) {
    const all = this.projects(), lastUsed = this.data.workspaces[folder]?.lastUsed ?? null;
    return { projects: [...all.filter((p) => p.source === folder), ...all.filter((p) => p.source !== folder)], lastUsed };
  }

  /** Each workspace folder's default project, as { folder: name }. */
  defaults() {
    return Object.fromEntries(Object.entries(this.data.workspaces).filter(([, w]) => w.lastUsed).map(([folder, w]) => [folder, w.lastUsed]));
  }

  /** @returns {Project[]} every project, oldest first */
  projects() {
    return Object.values(this.data.projects).sort((a, b) => a.created - b.created);
  }

  /** Every project name: names are unique per LayerFS home. */
  allNames() {
    return new Set(Object.keys(this.data.projects));
  }

  /** Adds a project, as the default of the folder it was imported for. @param {string} folder @param {Project} project */
  addProject(folder, project) {
    this.update((data) => { data.projects[project.name] = project; this.setDefault(folder, project.name); });
  }

  /** @param {string} name @returns {Project|null} */
  byName(name) {
    return Object.hasOwn(this.data.projects, name) ? this.data.projects[name] : null;
  }

  /**
   * Makes a project the default of a workspace folder: a new session there
   * uses it until it chooses another. Sessions that already chose keep theirs.
   * @param {string} folder @param {string} name
   */
  setDefault(folder, name) {
    this.update((data) => {
      if (!this.byName(name)) throw Object.assign(new Error(`no LayerFS project named ${name}`), { code: "not-found", status: 404 });
      (data.workspaces[folder] ||= { lastUsed: null }).lastUsed = name;
    });
  }

  /** The session's own choice: { project, locked, folder }, or null before one is made. */
  session(rootId) {
    return this.data.sessions[rootId] ?? null;
  }

  /**
   * The project a root session uses: its choice, else its folder's default.
   * Null when neither names one (a session without LayerFS).
   * @param {string} rootId @param {string|undefined} folder @returns {Project|null}
   */
  projectFor(rootId, folder) {
    const chosen = this.session(rootId);
    if (chosen) return chosen.project ? this.byName(chosen.project) : null;
    const name = folder ? this.data.workspaces[folder]?.lastUsed : null;
    return name ? this.byName(name) : null;
  }

  /**
   * Chooses a session's project (null: no LayerFS), any project; a project
   * also becomes the folder's default. Refused once locked.
   * @returns {boolean} whether the choice was taken
   */
  choose(rootId, folder, name) {
    return this.update((data) => {
      const current = this.session(rootId);
      if (current && current.locked) return false;
      if (name !== null && !this.byName(name)) throw Object.assign(new Error(`no LayerFS project named ${name}`), { code: "not-found" });
      data.sessions[rootId] = { project: name, locked: false, folder };
      if (name !== null) (data.workspaces[folder] ||= { lastUsed: null }).lastUsed = name;
      return true;
    });
  }

  /**
   * Forgets a removed project: no folder uses it, and sessions that chose it
   * have no project (a later project of the same name is not theirs).
   * @param {string} name
   */
  remove(name) {
    this.update((data) => {
      delete data.projects[name];
      for (const w of Object.values(data.workspaces)) if (w.lastUsed === name) w.lastUsed = null;
      // the project is gone, so the session may choose again: a lock it kept would
      // leave it with no project and both of the client's buttons disabled
      for (const choice of Object.values(data.sessions)) if (choice.project === name) { choice.project = null; choice.locked = false; }
    });
  }

  /** Locks a session's project at its first LayerFS-bound bash call. */
  lock(rootId, name, folder) {
    if (this.session(rootId)?.locked) return;
    this.update((data) => {
      const current = this.session(rootId);
      if (current && current.locked) return;
      data.sessions[rootId] = { project: name, locked: true, folder };
    });
  }

  /** @param {string} sessionId @returns {{ root: string, name: string }|null} */
  agent(sessionId) {
    return this.data.agents[sessionId] ?? null;
  }

  /** Records an agent's root session and display name. */
  setAgent(sessionId, root, name) {
    const known = this.data.agents[sessionId];
    if (known && known.root === root && known.name === name) return;
    this.update((data) => { data.agents[sessionId] = { root, name }; });
  }

  /** Agents of one root session, as { id: name }. */
  agentsOf(rootId) {
    return Object.fromEntries(Object.entries(this.data.agents).filter(([, a]) => a.root === rootId).map(([id, a]) => [id, a.name]));
  }
}
