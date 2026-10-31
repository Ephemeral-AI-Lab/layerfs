// Agent clients the runtime holds, one per project, through @layerfs/agent's
// public API only. Copied from the dsh Host's layerfs-client.js (#428). The
// agent reads its deployment from the environment when a client is made, so
// the project's settings are set for that synchronous step only. A client is
// closed after a quiet minute; Workspaces are unaffected.
import { LayerFS } from "@layerfs/agent";
import { keyOf } from "./project/list.js";
import { callHome } from "./settings.js";

const IDLE_MS = 60_000;

/** The environment one project's client (and its agents' bash calls) use. A
 * platform may replace it whole (`deploymentEnv`), as a `vm:<provider>` home
 * does: its deployment is native Sandboxes in the VM's namespace backend, not
 * host Docker. A project of a PostgreSQL Store (#506) has no volume; its
 * Store path is `postgresql:<store>`, and its connection's URL is added only
 * when the call gave one (`project.url`). */
export function deploymentEnv(settings, platform, project) {
  if (platform.deploymentEnv) return platform.deploymentEnv(settings, project);
  if (project.kind === "postgresql") {
    return {
      HOME: callHome(settings),
      DOCKER: settings.docker,
      IMAGE: project.image ?? settings.image ?? "",
      DAEMON: settings.daemon ?? "",
      VOLUME: "",
      STORE_PATH: `postgresql:${project.store}`,
      STORE: project.store,
      ...(project.url ? { STORE_POSTGRESQL: JSON.stringify({ url: project.url }) } : {}),
      CONTAINER_PREFIX: settings.containerPrefix,
    };
  }
  return {
    HOME: callHome(settings),
    DOCKER: settings.docker,
    IMAGE: project.image ?? settings.image ?? "",
    DAEMON: settings.daemon ?? "",
    VOLUME: project.volume,
    STORE_PATH: platform.storePath(settings, project.volume),
    // a project of a shared Store (#506) is addressed by Store and name; "" leaves it unset
    STORE: project.shared ? project.store : "",
    CONTAINER_PREFIX: settings.containerPrefix,
  };
}

/** Runs `make` with LAYERFS_<name> set from `env`, restoring them before it returns. */
export function withEnv(env, make) {
  const saved = {};
  for (const [name, value] of Object.entries(env)) { saved[name] = process.env[`LAYERFS_${name}`]; process.env[`LAYERFS_${name}`] = value; }
  try {
    return make();
  } finally {
    for (const [name, value] of Object.entries(saved)) {
      if (value === undefined) delete process.env[`LAYERFS_${name}`];
      else process.env[`LAYERFS_${name}`] = value;
    }
  }
}

export class Clients {
  constructor(runtime) {
    this.runtime = runtime;
    /** name -> { opening, users, timer } */
    this.held = new Map();
  }

  /** Runs `work(client)` with the project's open client. A project of a
   * PostgreSQL Store gets a client for this call alone, closed after it,
   * so its URL is never held between calls. */
  async use(project, work) {
    if (project.kind === "postgresql") {
      const { settings, platform } = this.runtime;
      const client = await withEnv(deploymentEnv(settings, platform, project), () => LayerFS.open(project.name));
      try {
        return await work(client);
      } finally {
        await client.close();
      }
    }
    const key = keyOf(project);
    let held = this.held.get(key);
    if (!held) {
      const { settings, platform } = this.runtime;
      // LayerFS.open reads the environment before its first await
      const opening = withEnv(deploymentEnv(settings, platform, project), () => LayerFS.open(project.name));
      this.held.set(key, (held = { opening, users: 0, timer: null }));
      opening.catch(() => { if (this.held.get(key) === held) this.held.delete(key); });
    }
    held.users++;
    clearTimeout(held.timer);
    try {
      return await work(await held.opening);
    } finally {
      if (--held.users === 0 && this.held.get(key) === held) {
        held.timer = setTimeout(() => this.forget(key), IDLE_MS);
        held.timer.unref?.();
      }
    }
  }

  /** Closes the clients of every project of the Store `store` (a project with a Store of its own is keyed by its name). */
  async forgetStore(store) {
    await Promise.all([...this.held.keys()].filter((key) => key === store || key.startsWith(`${store}/`)).map((key) => this.forget(key)));
  }

  /** Closes the client of the project keyed `name` (keyOf), if any. */
  async forget(name) {
    const held = this.held.get(name);
    if (!held) return;
    this.held.delete(name);
    clearTimeout(held.timer);
    const client = await held.opening.catch(() => null);
    await client?.close();
  }

  async closeAll() {
    await Promise.all([...this.held.keys()].map((name) => this.forget(name)));
  }
}
