// runtime.remote.open_view / close_view / view_stats (#510): a live,
// read-only view of a Store file on a remote, read in place through ranged
// requests and never downloaded (core/docs/issues/506/README.md section 6).
// A view's projects are read like a Store's, with `{ view }` in place of
// `{ store }`: runtime.project.list({ view }) and get_state(name, since,
// { view }) (no Sandboxes), and the store path a view answers,
// `remote:<connection>/<file>#<view>`, opens agent clients on it. Every
// call that would change it is refused ReadOnly. A view is never a Store
// of the home and is never linked. It closes with close_view, after a quiet
// minute here (any call naming it is use), and after a quiet minute in the
// process that holds it.
import { invokeBackend } from "@layerfs/agent/internal";
import { callHome } from "../settings.js";
import { credentialsOf } from "../store/calls.js";
import { refuse } from "../store/registry.js";

const call = (op, args) => invokeBackend(op, args, {});
/** A view unused this long is closed. */
const IDLE_MS = 60_000;

export class Views {
  constructor(runtime) {
    this.runtime = runtime;
    /** id -> { view, users, timer } */
    this.held = new Map();
  }

  /** runtime.remote.open_view(remote, { file, credentials }): one stat of the file, then the reads that
   * open its Store: `{ view, storePath, remote, file, bytes, etag, sha256, heads, modified, projects }`. */
  async open(remote, options = {}) {
    if (typeof remote !== "string" || !remote) throw refuse("InvalidInput", "remote names one of the home's remotes");
    const file = options?.file;
    if (typeof file !== "string" || !file) throw refuse("InvalidInput", "file names a Store file of the remote (runtime.remote.list_stores)");
    const view = await call("runtime.remote.open_view", { home: callHome(this.runtime.settings), remote, file, ...credentialsOf(options) });
    const held = { view, users: 0, timer: null };
    this.held.set(view.view, held);
    this.#idle(view.view, held);
    return view;
  }

  /** The open view `id`, which a call naming it keeps open another quiet minute; NoSuchView when it was closed. */
  view(id) {
    const held = this.held.get(typeof id === "string" ? id : id?.view);
    if (!held) throw refuse("NoSuchView", `no open view ${id?.view ?? id}: it was closed or unused for a minute; open it again`);
    if (held.users === 0) this.#idle(held.view.view, held);
    return held.view;
  }

  /** The project `name` of view `id`, as a runtime project record. */
  project(id, name) {
    const view = this.view(id);
    if (!view.projects.includes(name)) throw refuse("NoSuchProject", `the remote file ${view.remote}/${view.file} holds no project ${name}`);
    return viewProject(view, name);
  }

  /** runtime.project.list({ view }): the view's projects, read-only. */
  projects(id) {
    const view = this.view(id);
    return view.projects.map((name) => viewProject(view, name));
  }

  /** Runs `work()` on view `id`, which stays open meanwhile. */
  async use(id, work) {
    const held = this.held.get(this.view(id).view);
    held.users++;
    clearTimeout(held.timer);
    try {
      return await work();
    } finally {
      if (--held.users === 0 && this.held.get(held.view.view) === held) this.#idle(held.view.view, held);
    }
  }

  #idle(id, held) {
    clearTimeout(held.timer);
    held.timer = setTimeout(() => this.close(id).catch(() => {}), IDLE_MS);
    held.timer.unref?.();
  }

  /** runtime.remote.close_view(view): `{ closed }`; its clients close first. */
  async close(id) {
    const key = typeof id === "string" ? id : id?.view;
    if (typeof key !== "string" || !key) throw refuse("InvalidInput", "close_view takes the view's id");
    const held = this.held.get(key);
    this.held.delete(key);
    if (held) clearTimeout(held.timer);
    await this.runtime.clients.forgetStore(storeOf(key));
    return call("runtime.remote.close_view", { home: callHome(this.runtime.settings), view: key });
  }

  /** runtime.remote.view_stats(view): requests and bytes sent, cache hits and misses (#510 §4.2). */
  stats(id) {
    return call("runtime.remote.view_stats", { home: callHome(this.runtime.settings), view: this.view(id).view });
  }

  async closeAll() {
    await Promise.all([...this.held.keys()].map((id) => this.close(id).catch(() => {})));
  }
}

/** The Store key a view's projects go by. */
const storeOf = (id) => `view:${id}`;

/** A view's project as a runtime project record (kind "view", read-only). */
function viewProject(view, name) {
  return {
    name, store: storeOf(view.view), kind: "view", view: view.view, storePath: view.storePath,
    remote: view.remote, file: view.file, volume: null, image: null, source: null, created: view.modified ?? null,
    initialized: true, shared: true, readOnly: true,
  };
}
