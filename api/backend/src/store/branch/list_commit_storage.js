// backend.store.branch.list_commit_storage: each Commit of a Branch with the
// canonical, stored and metadata bytes it added, newest first, one page.
import { invokeClientBackend, page } from "@layerfs/agent/internal";
export function list_commit_storage(shop, branch, pageSize, pagePos) {
  return invokeClientBackend(shop, "backend.store.branch.list_commit_storage", { branch, ...page(pageSize, pagePos) });
}
