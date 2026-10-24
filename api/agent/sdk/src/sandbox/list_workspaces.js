// sandbox.list_workspaces: the Workspaces mounted in one Sandbox, asked of
// its daemon, one page: each { sandbox, path, branch, base, activity,
// changed }, where `changed` says whether it holds uncommitted changes (null
// when the daemon could not tell) and `activity` maps the daemon's state:
// idle -> Idle; committing, attaching -> Busy; closing -> Closing;
// uncertain -> Uncertain; failed -> Failed; any other state is passed
// through as the daemon named it, never reported as Busy. Each also
// carries the daemon's record: `workspace`, `state`, `dirty`,
// `headMovedSinceMount` and `pendingMerge`; the page carries the daemon's
// `version`. Refused NoSuchSandbox when the Sandbox is not running.
import { invokeClient, page, pageOf, sandboxArg } from "../client.js";

const ACTIVITY = { idle: "Idle", committing: "Busy", attaching: "Busy", closing: "Closing", uncertain: "Uncertain", failed: "Failed" };
export async function list_workspaces(client, sandbox, pageSize = 50, pagePos = 0) {
  const at = page(pageSize, pagePos);
  const value = await invokeClient(client, "sandbox.list_workspaces", { sandbox: sandboxArg(sandbox) });
  const items = (value.workspaces ?? []).filter((w) => w.mounted && w.directory).map((w) => ({
    sandbox,
    path: w.directory,
    branch: w.branch ?? null,
    base: w.base ?? null,
    activity: ACTIVITY[w.activity] ?? w.activity ?? null,
    changed: w.dirty ? w.dirty.inodes + w.dirty.entries > 0 : null,
    workspace: w.workspace,
    state: w.activity,
    dirty: w.dirty ?? null,
    headMovedSinceMount: w.headMovedSinceMount ?? null,
    pendingMerge: !!w.pendingMerge,
  }));
  return { ...pageOf(items, at), version: value.version };
}
