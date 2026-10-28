// backend.store.project.get_storage: the Store's totals (logical, canonical,
// stored and metadata bytes; dedupe and compression ratios; what Init
// recorded).
import { invokeClientBackend } from "@layerfs/agent/internal";
export async function get_storage(shop) {
  const value = await invokeClientBackend(shop, "backend.store.project.get_storage", {});
  return { ...value, dedupe: value.dedupe === null ? null : Number(value.dedupe), compression: value.compression === null ? null : Number(value.compression) };
}
