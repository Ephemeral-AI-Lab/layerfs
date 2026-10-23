// backend.store.fs.list_revisions: the Commits that changed `path`, newest
// first, one page.
import { invokeClientBackend, page } from "@layerfs/agent/internal";
export function list_revisions(shop, ref, path, pageSize, pagePos) {
  return invokeClientBackend(shop, "backend.store.fs.list_revisions", { branch: ref, path, ...page(pageSize, pagePos) });
}
