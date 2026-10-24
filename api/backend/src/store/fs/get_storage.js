// backend.store.fs.get_storage: a file's size at `ref` and its stored cost
// across its revisions (a bounded count).
import { invokeClientBackend } from "@layerfs/agent/internal";
export function get_storage(shop, ref, path) {
  return invokeClientBackend(shop, "backend.store.fs.get_storage", { branch: ref, path });
}
