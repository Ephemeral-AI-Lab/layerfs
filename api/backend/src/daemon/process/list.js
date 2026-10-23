// backend.daemon.process.list: the project's background processes in every
// running Sandbox, each with its `sandbox`, one page.
import { invokeClientBackend, page, pageOf } from "@layerfs/agent/internal";
export async function list(shop, pageSize = 50, pagePos = 0) {
  const at = page(pageSize, pagePos);
  const value = await invokeClientBackend(shop, "backend.daemon.process.list", {});
  return pageOf(value.items ?? [], at);
}
