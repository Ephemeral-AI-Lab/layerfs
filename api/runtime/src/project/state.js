// runtime.project.get_state(name, since): the UI's view of one project: its
// history from the Store (backend.store.project.get_history), its Sandboxes
// from their records (project.list_sandboxes) and each running one's
// Workspaces with their overlays (sandbox.list_workspaces,
// backend.daemon.workspace.get_storage), joined under one version token. A
// read starts no Sandbox: a recorded one whose container is gone or stopped
// is listed in `gone` with its refusal's detail. Moved from
// sdk/js/operator/inspect/snapshot.js (#428).
import { invokeClient, invokeClientBackend } from "@layerfs/agent/internal";

/** One Sandbox's Workspaces with their overlays, or its `gone` detail when it is not running. */
async function live(client, sandbox) {
  let answer;
  try {
    answer = await invokeClient(client, "sandbox.list_workspaces", { sandbox });
  } catch (error) {
    if (error?.code === "NoSuchSandbox") return { gone: error.detail ?? null };
    throw error;
  }
  await Promise.all(answer.workspaces.filter((w) => w.mounted && w.directory).map(async (workspace) => {
    workspace.overlay = await invokeClientBackend(client, "backend.daemon.workspace.get_storage", { sandbox, path: workspace.directory });
  }));
  return { answer };
}

export async function get_state(client, since) {
  const history = await invokeClientBackend(client, "backend.store.project.get_history", {});
  const listed = (await invokeClient(client, "project.list_sandboxes", {})).items.map(({ sandbox, ...rest }) => ({ id: sandbox, ...rest }));
  const reads = await Promise.all(listed.map(({ id }) => live(client, id)));
  const version = [history.version, ...reads.map((read, index) => read.answer?.version ?? `gone-${listed[index].id}`)].join(".");
  if (since === version) return { kind: "unchanged", version };
  return {
    kind: "ok", version, project: history.project, store: history.store,
    branches: history.branches, moreBranches: history.moreBranches, commits: history.commits, more: history.more,
    layers: history.layers, moreLayers: history.moreLayers,
    sandboxes: listed.flatMap(({ id, name, image }, index) => reads[index].answer ? [{
      id, name, image, status: "Ready",
      workspaces: reads[index].answer.workspaces.map((workspace) => ({ ...workspace, sandbox: id, path: workspace.directory })),
    }] : []),
    gone: listed.flatMap(({ id, name, image }, index) => reads[index].answer ? [] : [{ id, name, image, detail: reads[index].gone }]),
  };
}
