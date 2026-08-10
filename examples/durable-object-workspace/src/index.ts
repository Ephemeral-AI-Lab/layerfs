import { EphemeralFS } from "@ephemeralai/fs";
import {
  openCloudflareSqlite,
  type DurableObjectSQLiteStorage,
} from "@ephemeralai/fs-sqlite-cloudflare";

interface DurableObjectStateLike {
  readonly storage: DurableObjectSQLiteStorage;
}
export class FilesystemObject {
  readonly #ready: Promise<EphemeralFS>;
  constructor(state: DurableObjectStateLike) {
    this.#ready = openCloudflareSqlite({ storage: state.storage }).then((database) =>
      EphemeralFS.open({ database }),
    );
  }
  async fetch(): Promise<Response> {
    const filesystem = await this.#ready;
    return Response.json({
      capabilities: filesystem.capabilities,
      storage: await filesystem.maintenance.snapshotStorage(),
    });
  }
}
