// backend.store.project.list_overwritten: the `main` Layers one publication
// replaced without containing them, newest first, one page.
import { invokeClientBackend, page } from "@layerfs/agent/internal";
export function list_overwritten(shop, layer, pageSize, pagePos) {
  return invokeClientBackend(shop, "backend.store.project.list_overwritten", { layer, ...page(pageSize, pagePos) });
}
