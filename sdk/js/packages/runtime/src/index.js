// @layerfs/runtime (#428): what an integration host runs, on a LayerFS home
// rather than one project, so it works with no project, a removed one, or
// the worker down. Agents don't import it: no skill teaches it, and it needs
// deployment settings an agent's environment doesn't have.
//   const runtime = await LayerFSRuntime.open({ home, image, docker, bundle, … });
//   try { await runtime.status(); } finally { await runtime.close(); }
// Destructive calls name what they destroy (`confirm`); every call is one
// attempt; an uncertain outcome keeps what exists and reports Unknown.
//
// On macOS and Windows, Store calls run in the home's worker (platform/),
// and this process's @layerfs/agent clients reach it through the endpoint
// the platform writes. That route is open only for the dsh adapter
// (AGENTS.md exceptions): `adapter: "dsh"` sets LAYERFS_WORKER_ENDPOINT and
// LAYERFS_WORKER_ADAPTER for this process; any other adapter gets no worker.
import { LayerFSError } from "@layerfs/agent";
import { Clients } from "./clients.js";
import { resolveSettings } from "./settings.js";
import { status } from "./status.js";
import linux from "./platform/linux/index.js";
import macos from "./platform/macos/index.js";
import windows from "./platform/windows/index.js";
import { list } from "./project/list.js";
import { InitJobs } from "./project/jobs.js";
import { prepare, runInit } from "./project/init.js";
import { get_state } from "./project/state.js";
import { remove } from "./project/remove.js";
import { reclaim } from "./sandbox/reclaim.js";
import { clean_leftovers, leftovers } from "./sandbox/leftovers.js";
import { subscribe } from "./events/subscribe.js";

const PLATFORMS = { linux, macos, windows };
/** Held only by `open`: the constructor refuses without it. */
const FACTORY = Symbol("LayerFSRuntime.open");

export class LayerFSRuntime {
  /** A runtime on the LayerFS home `settings.home` (settings.js lists the rest). Opens nothing else. */
  static async open(settings = {}) {
    return new LayerFSRuntime(resolveSettings(settings), FACTORY);
  }

  /** Not public: `LayerFSRuntime.open(settings)` makes one. */
  constructor(settings, token = undefined) {
    if (token !== FACTORY) throw new LayerFSError("InvalidInput", "use `await LayerFSRuntime.open(settings)`; there is no public constructor", null);
    this.settings = settings;
    this.platform = PLATFORMS[settings.platform];
    this.closed = false;
    const endpoint = this.platform.endpoint(settings);
    if (endpoint && settings.adapter === "dsh") {
      process.env.LAYERFS_WORKER_ENDPOINT = endpoint;
      process.env.LAYERFS_WORKER_ADAPTER = "dsh";
    }
    this.clients = new Clients(this);
    const jobs = new InitJobs((prepared, report) => runInit(this, prepared, report));
    const live = (method) => async (...args) => {
      if (this.closed) throw new LayerFSError("InvalidInput", "this LayerFS runtime is closed; open another with LayerFSRuntime.open", null);
      return method(...args);
    };
    /** A project this runtime knows: its record, else its home entry. */
    const known = async (name) => {
      const found = (await list(this)).find((p) => p.name === name);
      if (!found) throw new LayerFSError("NoSuchProject", `no LayerFS project named ${name} in ${settings.home}`, null);
      if (!found.volume) throw new LayerFSError("InvalidInput", `the runtime has no record of ${name}'s volume (made outside this runtime)`, null);
      return found;
    };
    this.status = live(() => status(this));
    this.worker = {
      start: live(() => this.platform.start(settings)),
      restart: live((options) => {
        if (options?.confirm !== "restart") throw new LayerFSError("InvalidInput", "a restart cuts off every project's Store calls; confirm it with { confirm: \"restart\" }", null);
        return this.platform.restart(settings);
      }),
    };
    this.project = {
      list: live(() => list(this)),
      start_init: live(async (name, folder, options) => jobs.begin(await prepare(this, name, folder, options, jobs.pending()))),
      get_init: live(async (job) => jobs.get(job?.id ?? job)),
      get_state: live(async (name, since) => this.clients.use(await known(name), (client) => get_state(client, since))),
      remove: live((name, options) => remove(this, name, options)),
    };
    this.sandbox = {
      reclaim: live(async (name) => reclaim(this, await known(name))),
      list_leftovers: live(() => leftovers(this)),
      clean_leftovers: live((names) => clean_leftovers(this, names)),
    };
    this.events = { subscribe: (options) => { if (this.closed) throw new LayerFSError("InvalidInput", "this LayerFS runtime is closed", null); return subscribe(this, options); } };
  }

  /** Closes the runtime's project clients (Workspaces stay mounted) and, on
   * Windows, the worker it started (it drains first). A second close does nothing. */
  async close() {
    if (this.closed) return;
    this.closed = true;
    await this.clients.closeAll();
    await this.platform.close?.(this.settings);
  }
}

if (typeof Symbol.asyncDispose === "symbol") {
  Object.defineProperty(LayerFSRuntime.prototype, Symbol.asyncDispose, { value() { return this.close(); }, writable: true, configurable: true });
}
