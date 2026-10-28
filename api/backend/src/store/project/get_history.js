// backend.store.project.get_history: the project's Branches, their newest
// Commits and its Layers, read from the Store alone (no Sandbox), with a
// `version` token: `since` that version answers { kind: "unchanged", version }
// until history moves. Commits are cut per Branch; `more` gives the cursor
// branch.list_commits continues each cut Branch from.
import { invokeClientBackend } from "@layerfs/agent/internal";
export function get_history(shop, since) {
  return invokeClientBackend(shop, "backend.store.project.get_history", since === undefined ? {} : { since });
}
