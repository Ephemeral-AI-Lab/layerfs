// A stand-in native binding for trace checks: no Docker, Store or daemon.
// Store calls name the client's storePath; Workspace calls their Sandbox.
// Every call resolves like the real binding; a path "/missing" is refused.
// `sandboxList` is the project's running Sandboxes, `workspaceList` a
// Sandbox's daemon records; a Store path "/missing" names no project.
const commit = { id: "c0ffee", logical: 120, newCanonical: 40, newStored: 30, newMetadata: 8 };
const answer = (op, args) => {
  if (args.storePath === "/missing") return { refused: { code: "NoSuchProject", message: "no such project", detail: null } };
  if (args.path === "/missing") return { refused: { code: "NoSuchPath", message: "/missing does not exist", detail: null } };
  if (op === "workspace.mount") return { value: { kind: "Mounted", sandbox: "1", container: "layerfs-website-1", path: "/workspaces/w1", branch: args.branch, base: "b4se", baseLayer: "lay0", root: "r00t" } };
  if (op === "workspace.exec") return { value: { kind: "exit:3", exitCode: 3, stdout: "x".repeat(5000), stderr: "", truncated: false } };
  if (op === "backend.daemon.process.start") return { value: { kind: "running", process: "proc_1_ab", command: args.command } };
  if (op === "workspace.commit") return { value: { kind: "Committed", head: commit.id, commit } };
  if (op === "fs.rm") return { value: { kind: "UpToDate", head: "h3ad" } };
  if (op === "branch.merge") return { value: { kind: "Merged", branch: args.name, head: "m3rge", root: "r00t" } };
  if (op === "branch.publish") return { value: { kind: "Added", source: { branch: "feature", commit: "pub1" }, publishedLayer: { id: "lay1" } } };
  if (op === "branch.fork") return { value: { kind: "Forked", branch: args.name, base: "lay0", head: "not-a-publication" } };
  if (/^fs\.(write|edit|apply|mkdir|cp|mv)$/.test(op)) return { value: { commit } };
  if (op === "project.list_sandboxes") return { value: { kind: "ok", items: exports.sandboxList } };
  if (op === "backend.store.project.get_history") return { value: { kind: "ok", version: "h1", project: "website", branches: [], commits: [], more: {}, layers: [], workspaces: [] } };
  if (op === "sandbox.list_workspaces") return { value: { kind: "ok", version: "1", workspaces: exports.workspaceList } };
  if (op === "backend.store.project.get_storage") return { value: { dedupe: "2", compression: null } };
  return { value: { items: [], hasMore: false } };
};
// A traced call (args.observe) gets daemon telemetry like the real binding:
// flat timing nodes [name, parent, us, outcome, incomplete], per-call SQL work,
// and the Workspace's FUSE opcode counts on status and unmount.
const telemetry = (op) => [{
  timing: { nodes: [["commit", -1, 2500, "ok", false], ["capture", 0, 400, "ok", false], ["construct", 0, 1500, "ok", false], ["captured.namespace", 2, 1400, "ok", false]], omitted: 0 },
  sql: { writer: { statements: 3, fullscan_steps: 0 }, readers: { statements: 2, vm_steps: 40 } },
  ...(op === "backend.daemon.workspace.status" || op === "workspace.unmount" ? { fuse: { lookup: 5, getattr: 7, served: 12 } } : {}),
  ...(op === "workspace.commit" ? { storage: { pack_write_bytes: 4096 }, construction: { files_constructed: 1 } } : {}),
}];
exports.calls = [];
exports.observed = [];
exports.deploys = [];
exports.args = [];
exports.workspaceList = [];
exports.sandboxList = [{ sandbox: "1", name: "layerfs-website-1", image: "fake-image", workspaces: 0 }];
const traced = (op, args) => {
  exports.calls.push(op);
  exports.observed.push(args.observe ?? null);
  if (args.deploy) exports.deploys.push(args.deploy);
  exports.args.push(args);
  const plain = answer(op, args);
  return args.observe ? { ...plain, telemetry: telemetry(op) } : plain;
};
exports.stores = [];
// The in-process Store (#352 F-020): opened once per client, never traced.
exports.storeOpen = async (args) => { exports.stores.push(["open", args.storePath]); return { value: { clients: 1 } }; };
exports.storeClose = async (args) => { exports.stores.push(["close", args.storePath]); return { value: { clients: 0 } }; };
exports.call = async (op, args) => traced(op, args);
exports.backendCall = async (op, args) => traced(op, args);
