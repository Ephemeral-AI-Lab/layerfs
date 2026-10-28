// @layerfs/backend (#428): calls about one opened project, answered by
// LayerFS's back end. `backend.store.*` is answered by the project's Store
// (in this process on Linux, in the worker on macOS and Windows) and needs no
// running Sandbox; `backend.daemon.*` needs the Sandbox holding the Workspace
// or process to be running, and is refused NoSuchSandbox otherwise.
//   const backend = LayerFSBackend.of(shop);   // shop from LayerFS.open or LayerFS.init
// `of` opens nothing and has no close: once `shop` is closed every backend
// call is refused InvalidInput, as `shop`'s own calls are.
import { LayerFSError } from "@layerfs/agent";
import { STATE } from "@layerfs/agent/internal";
import { get_comparable } from "./store/project/get_comparable.js";
import { get_history } from "./store/project/get_history.js";
import { get_storage as get_project_storage } from "./store/project/get_storage.js";
import { list_overwritten } from "./store/project/list_overwritten.js";
import { list_commit_storage } from "./store/branch/list_commit_storage.js";
import { get_storage as get_file_storage } from "./store/fs/get_storage.js";
import { list_revisions } from "./store/fs/list_revisions.js";
import { status } from "./daemon/workspace/status.js";
import { list_changes } from "./daemon/workspace/list_changes.js";
import { get_storage as get_workspace_storage } from "./daemon/workspace/get_storage.js";
import { start } from "./daemon/process/start.js";
import { list } from "./daemon/process/list.js";
import { cancel } from "./daemon/process/cancel.js";
import { get_log } from "./daemon/sandbox/get_log.js";

/** Held only by `of`: the constructor refuses without it. */
const FACTORY = Symbol("LayerFSBackend.of");

const bind = (shop, methods) =>
  Object.fromEntries(Object.entries(methods).map(([name, method]) => [name, async (...args) => method(shop, ...args)]));

export class LayerFSBackend {
  /** The backend of an opened agent client. */
  static of(shop) {
    if (!shop || !shop[STATE]) throw new LayerFSError("InvalidInput", "LayerFSBackend.of takes a client from LayerFS.open or LayerFS.init", null);
    return new LayerFSBackend(shop, FACTORY);
  }

  /** Not public: `LayerFSBackend.of(shop)` makes one. */
  constructor(shop, token = undefined) {
    if (token !== FACTORY) throw new LayerFSError("InvalidInput", "use LayerFSBackend.of(shop); there is no public constructor", null);
    this.store = {
      project: bind(shop, { get_history, get_storage: get_project_storage, get_comparable, list_overwritten }),
      branch: bind(shop, { list_commit_storage }),
      fs: bind(shop, { get_storage: get_file_storage, list_revisions }),
    };
    this.daemon = {
      workspace: bind(shop, { status, list_changes, get_storage: get_workspace_storage }),
      process: bind(shop, { start, list, cancel }),
      sandbox: bind(shop, { get_log }),
    };
  }
}
