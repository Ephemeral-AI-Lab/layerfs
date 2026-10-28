// The one stand-in native binding for the SDK and `layerfs` CLI tests: no
// Docker, Store or daemon. Store calls name the client's storePath; Workspace
// calls their Sandbox. Every call resolves like the real binding, with small
// fixed answers.
// - Recording: in-process tests read the exports `calls`, `observed`,
//   `deploys`, `args` and `stores`. A CLI test runs the SDK in a subprocess,
//   so each call is also appended to the JSON-lines file CLI_FAKE_LOG as
//   [op, args] (bytes as { base64 }); the Store opens as "store.open"/"close".
// - Refusals: a path "/missing" is refused NoSuchPath and a Store path
//   "/missing" NoSuchProject; each op named in CLI_FAKE_REFUSE
//   (comma-separated) is refused with code CLI_FAKE_CODE (default Conflict);
//   `initRefusal`, when set, is what project.init refuses; a policy depth over
//   255 is refused as the addon refuses it.
// - Answers: `sandboxList` is the project's running Sandboxes,
//   `workspaceList` a Sandbox's daemon records. An exec command "exit <n>"
//   ends with exit code n, stdout "out:<command>" and stderr "err"; any other
//   command ends with code 3 and 5000 bytes of stdout.
const { appendFileSync } = require("node:fs");

const plain = (value) => {
  if (Buffer.isBuffer(value) || value instanceof Uint8Array) return { base64: Buffer.from(value).toString("base64") };
  if (Array.isArray(value)) return value.map(plain);
  if (value && typeof value === "object") return Object.fromEntries(Object.entries(value).map(([k, v]) => [k, plain(v)]));
  return value;
};
const log = (op, args) => {
  if (process.env.CLI_FAKE_LOG) appendFileSync(process.env.CLI_FAKE_LOG, `${JSON.stringify([op, plain(args)])}\n`);
};

const commit = { id: "c0ffee", logical: 120, newCanonical: 40, newStored: 30, newMetadata: 8 };
const committed = { kind: "Committed", head: commit.id, commit };
const refusal = (code, message, detail = null) => ({ refused: { code, message, detail } });

function answer(op, args) {
  const refuse = (process.env.CLI_FAKE_REFUSE || "").split(",").filter(Boolean);
  if (refuse.includes(op)) return refusal(process.env.CLI_FAKE_CODE || "Conflict", `${op} refused by the fake`, { op });
  if (args.storePath === "/missing") return refusal("NoSuchProject", "no such project");
  if (args.path === "/missing") return refusal("NoSuchPath", "/missing does not exist");
  switch (op) {
    case "project.init":
      if (exports.initRefusal) return { refused: exports.initRefusal };
      if (args.policy?.chunkDepth > 255) return refusal("InvalidInput", "policy.chunkDepth is out of range");
      return { value: { kind: "Initialized", project: args.name, layer: "lay0", root: "r00t", store: args.storePath } };
    case "project.list_branches": return { value: { items: [
      { name: "main", base: "layer0000000000aa", head: null },
      { name: "cart", base: "base1234567890ab", head: "head1234567890ab" },
    ], hasMore: false, nextPagePos: null } };
    case "project.list_sandboxes": return { value: { kind: "ok", items: exports.sandboxList } };
    case "branch.fork": return { value: { kind: "Forked", branch: args.name, base: "lay0", head: "not-a-publication" } };
    case "branch.merge": return { value: { kind: "Merged", branch: args.name, head: "m3rge", root: "r00t" } };
    case "branch.publish": return { value: { kind: "Added", source: { branch: "feature", commit: "pub1" }, publishedLayer: { id: "lay1" } } };
    case "fs.read": return { value: args.startLine === 1 ? { text: "a\n", nextLine: 2 } : { text: "b\n", nextLine: null } };
    case "fs.stat": return { value: { type: "file", size: 5, mode: 420 } };
    case "fs.rm": return { value: { kind: "UpToDate", head: "h3ad" } };
    case "workspace.mount": return { value: { kind: "Mounted", sandbox: args.sandbox, container: `layerfs-website-${args.sandbox}`, path: "/workspaces/w1", image: "fake-image", branch: args.branch, base: "b4se", baseLayer: "lay0", root: "r00t" } };
    case "workspace.exec": {
      const exit = /^exit (\d+)$/.exec(args.command);
      if (!exit) return { value: { kind: "exit:3", exitCode: 3, stdout: "x".repeat(5000), stderr: "", truncated: false } };
      const code = Number(exit[1]);
      return { value: { kind: `exit:${code}`, exitCode: code, stdout: `out:${args.command}`, stderr: "err", truncated: false } };
    }
    case "workspace.commit": return { value: committed };
    case "workspace.unmount": return { value: args.force ? { kind: "ForceUnmounted", abort: "Written", detach: "Detached", cleanup: "Queued" } : { kind: "Unmounted" } };
    case "backend.daemon.process.start": return { value: { kind: "running", process: "proc_1_ab", command: args.command } };
    case "backend.store.project.get_history": return { value: exports.history ?? { kind: "ok", version: "h1", project: "website", branches: [], commits: [], more: {}, layers: [], workspaces: [] } };
    case "backend.store.project.get_storage": return { value: { dedupe: "2", compression: null } };
    case "sandbox.create": return { value: { sandbox: "2", name: "layerfs-website-2", image: args.deploy?.image } };
    case "sandbox.list_workspaces": return { value: { kind: "ok", version: "1", workspaces: exports.workspaceList } };
    case "sandbox.close": return { value: { kind: "Closed", sandbox: args.sandbox } };
    case "runtime.sandbox.reclaim": return { value: { kind: "ok", reclaimed: exports.reclaimed ?? [] } };
    default:
      if (/^fs\.(write|edit|apply|mkdir|cp|mv)$/.test(op)) return { value: committed };
      return { value: { items: [], hasMore: false, nextPagePos: null } };
  }
}
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
  log(op, args);
  exports.calls.push(op);
  exports.observed.push(args.observe ?? null);
  if (args.deploy) exports.deploys.push(args.deploy);
  exports.args.push(args);
  const result = answer(op, args);
  return args.observe ? { ...result, telemetry: telemetry(op) } : result;
};
exports.stores = [];
// The in-process Store (#352 F-020): opened once per client, never traced.
exports.storeOpen = async (args) => { log("store.open", args); exports.stores.push(["open", args.storePath]); return { value: { clients: 1 } }; };
exports.storeClose = async (args) => { log("store.close", args); exports.stores.push(["close", args.storePath]); return { value: { clients: 0 } }; };
exports.call = async (op, args) => traced(op, args);
exports.backendCall = async (op, args) => traced(op, args);
