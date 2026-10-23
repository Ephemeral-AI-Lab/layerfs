// sandbox.list_workspaces: the Workspaces mounted in one Sandbox, asked of
// its daemon, one page: each { sandbox, path, branch, base, activity,
// changed }, where `activity` is Idle or Busy and `changed` says whether it
// holds uncommitted changes (null when the daemon could not tell). Each also
// carries the daemon's record: `workspace`, `state`, `dirty`,
// `headMovedSinceMount` and `pendingMerge`; the page carries the daemon's
// `version`. Refused NoSuchSandbox when the Sandbox is not running.
import { invokeClient, page, pageOf, sandboxArg } from "../client.js";
export async function list_workspaces(client, sandbox, pageSize = 50, pagePos = 0) {
  const at = page(pageSize, pagePos);
  const value = await invokeClient(client, "sandbox.list_workspaces", { sandbox: sandboxArg(sandbox) });
  const items = (value.workspaces ?? []).filter((w) => w.mounted && w.directory).map((w) => ({
    sandbox,
    path: w.directory,
    branch: w.branch ?? null,
    base: w.base ?? null,
    activity: w.activity === "idle" ? "Idle" : "Busy",
    changed: w.dirty ? w.dirty.inodes + w.dirty.entries > 0 : null,
    workspace: w.workspace,
    state: w.activity,
    dirty: w.dirty ?? null,
    headMovedSinceMount: w.headMovedSinceMount ?? null,
    pendingMerge: !!w.pendingMerge,
  }));
  return { ...pageOf(items, at), version: value.version };
}
