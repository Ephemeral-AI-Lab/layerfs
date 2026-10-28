// Agent clients the runtime holds, one per project, through @layerfs/agent's
// public API only. Copied from the dsh Host's layerfs-client.js (#428). The
// agent reads its deployment from the environment when a client is made, so
// the project's settings are set for that synchronous step only. A client is
// closed after a quiet minute; Workspaces are unaffected.
import { LayerFS } from "@layerfs/agent";
import { callHome } from "./settings.js";

const IDLE_MS = 60_000;

/** The environment one project's client (and its agents' bash calls) use. */
export function deploymentEnv(settings, platform, project) {
  return {
    HOME: callHome(settings),
    DOCKER: settings.docker,
    IMAGE: project.image ?? settings.image ?? "",
    DAEMON: settings.daemon ?? "",
    VOLUME: project.volume,
    STORE_PATH: platform.storePath(settings, project.volume),
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

  /** Runs `work(client)` with the project's open client. */
  async use(project, work) {
    let held = this.held.get(project.name);
    if (!held) {
      const { settings, platform } = this.runtime;
      // LayerFS.open reads the environment before its first await
      const opening = withEnv(deploymentEnv(settings, platform, project), () => LayerFS.open(project.name));
      this.held.set(project.name, (held = { opening, users: 0, timer: null }));
      opening.catch(() => { if (this.held.get(project.name) === held) this.held.delete(project.name); });
    }
    held.users++;
    clearTimeout(held.timer);
    try {
      return await work(await held.opening);
    } finally {
      if (--held.users === 0 && this.held.get(project.name) === held) {
        held.timer = setTimeout(() => this.forget(project.name), IDLE_MS);
        held.timer.unref?.();
      }
    }
  }

  /** Closes the project's client, if any. */
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
