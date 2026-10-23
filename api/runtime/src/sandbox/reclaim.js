// runtime.sandbox.reclaim(project): drops the records of the project's
// Sandboxes whose containers are gone, and deletes stopped ones (one
// unforced Delete each), so their numbers are free again: { reclaimed: [ids] }.
// The reclaim that operator.sandbox.list did as a side effect (#428); the
// agent's project.list_sandboxes now only lists.
import { invokeClientBackend } from "@layerfs/agent/internal";

export async function reclaim(runtime, project) {
  const { reclaimed } = await runtime.clients.use(project, (client) => invokeClientBackend(client, "runtime.sandbox.reclaim", {}));
  return { reclaimed };
}
