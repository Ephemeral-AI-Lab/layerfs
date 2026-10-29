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
    // #506 Store-first: Stores, their projects, remotes and transfers
    case "runtime.store.create": return { value: { kind: "Created", store: args.store, storePath: args.storePath, bytes: 4096, sqlite: "3.50.0" } };
    case "runtime.store.add_project": return { value: { kind: "Initialized", project: args.name, store: args.store, layer: "lay0", root: "r00t", storePath: args.storePath } };
    case "runtime.store.list_projects": return { value: exports.storeProjects ?? [] };
    case "runtime.store.remove_project": return { value: { stack: "5e", packsKept: true } };
    case "runtime.store.forget": return { value: { removed: true } };
    case "runtime.store.push": case "runtime.store.publish": case "runtime.store.send": return { value: { kind: { "runtime.store.push": "Pushed", "runtime.store.publish": "Published", "runtime.store.send": "Sent" }[op], store: args.store, object: `${args.file}/store.sqlite`, pieces: 0, transfer: { remote: args.remote, file: args.file, direction: "push", at: 1, sha256: "ab".repeat(32), bytes: 8192, heads: "11".repeat(32) } } };
    case "runtime.store.pull": return { value: { kind: "Pulled", store: args.store, replaced: true, added: exports.pullAdded ?? [], missing: [], transfer: { remote: args.remote, file: args.file, direction: "pull", at: 2, sha256: "cd".repeat(32), bytes: 8192, heads: "22".repeat(32) } } };
    case "runtime.store.load": return { value: { kind: "Loaded", store: args.store, projects: ["docs", "web"], transfer: { remote: args.remote, file: args.file, direction: "pull", at: 3, sha256: "ab".repeat(32), bytes: 8192, heads: null } } };
    case "runtime.store.unlink": return { value: { removed: true } };
    case "runtime.store.import_postgresql": return { value: { sourceBytes: 4096, kind: "Imported", store: args.target, schema: args.postgresql.schema, tables: [], packBytes: 512 } };
    // #506 E1-E9: local stats, copy checks, listing, providers, PostgreSQL status, job progress
    case "runtime.store.stat": return { value: args.stores.map((s) => exports.storeStat?.[s.store] ?? { store: s.store, bytes: { file: 4096, wal: 512 }, open: false, heads: "11".repeat(32) }) };
    case "runtime.store.check": return { value: { store: args.store, remote: args.remote, file: args.file, present: true, bytes: 8192, sha256: "ab".repeat(32), heads: "11".repeat(32), modified: 1000, checkedAt: 2000 } };
    case "runtime.remote.list_stores": return { value: [{ file: "work-snap", bytes: 8192, sha256: "cd".repeat(32), heads: null, modified: 1000 }, { file: "work", bytes: 8192, sha256: "ab".repeat(32), heads: "11".repeat(32), modified: 1000 }] };
    case "runtime.remote.providers": return { value: [{ id: "minio", label: "MinIO", available: true, reason: null, fields: [{ name: "endpoint", label: "Endpoint", kind: "url", required: true }] }] };
    case "runtime.postgresql.status": return { value: { reachable: true, serverVersion: "17.6", latencyMs: 1, latencyUs: 900, schemas: Object.fromEntries(args.schemas.map((name) => [name, 4096])) } };
    case "runtime.sync.progress": return { value: exports.progress?.[args.job] ?? null };
    case "runtime.store.attach_postgresql": return { value: { kind: "Attached", store: args.store, schema: args.postgresql.schema, synchronousCommit: args.postgresql.synchronousCommit, projects: exports.storeProjects ?? [] } };
    case "runtime.remote.list": return { value: exports.remoteList ?? [] };
    case "runtime.remote.add": return { value: { ...args.remote, credentials: args.remote.credentials ?? "env:LAYERFS_REMOTE" } };
    case "runtime.remote.remove": return { value: { removed: args.name } };
    // #510: live read-only views of a remote Store file
    case "runtime.remote.open_view": return { value: { view: "v1", storePath: `remote:${args.remote}/${args.file}#v1`, remote: args.remote, file: args.file, bytes: 8192, etag: '"e1"', sha256: "ab".repeat(32), heads: "11".repeat(32), modified: 1000, projects: ["docs", "web"] } };
    case "runtime.remote.close_view": return { value: { closed: true } };
    case "runtime.remote.view_stats": return { value: { requests: 3, bytes: 2 << 20, fileBytes: 8192, hits: 10, misses: 2, fetches: 2, fetched: 2 << 20, resident: 2 << 20, capacity: 64 << 20 } };
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
// `hold`, when set, is a promise a push or load waits on before it answers, so a test can sample it running
exports.backendCall = async (op, args) => {
  if ((op === "runtime.store.push" || op === "runtime.store.load") && exports.hold) await exports.hold;
  return traced(op, args);
};
