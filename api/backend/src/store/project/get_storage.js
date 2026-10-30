// backend.store.project.get_storage: the Store's totals (logical, canonical,
// stored and metadata bytes; dedupe and compression ratios; what Init
// recorded; disk file sizes; opt-in commit totals by kind).
import { LayerFSError } from "@layerfs/agent";
import { invokeClientBackend } from "@layerfs/agent/internal";

export async function get_storage(shop, options = {}) {
  if (options === undefined) options = {};
  if (options === null || typeof options !== "object" || Array.isArray(options)) {
    throw new LayerFSError("InvalidInput", "options must be an object", null);
  }
  for (const key of Object.keys(options)) {
    if (key !== "commits") {
      throw new LayerFSError("InvalidInput", `unknown option ${key}`, null);
    }
  }
  if (options.commits !== undefined && typeof options.commits !== "boolean") {
    throw new LayerFSError("InvalidInput", "commits must be a boolean", null);
  }
  const args = options.commits !== undefined ? { commits: options.commits } : {};
  const value = await invokeClientBackend(shop, "backend.store.project.get_storage", args);
  return {
    ...value,
    dedupe: value.dedupe === null ? null : Number(value.dedupe),
    compression: value.compression === null ? null : Number(value.compression),
  };
}
