// A stand-in native binding for the layerfs CLI tests: no Docker, Store or
// daemon. Each binding call is appended to the JSON-lines file CLI_FAKE_LOG
// as [op, args] (bytes as { base64 }), so a test sees exactly what the SDK
// sent. Answers are fixed and small; a path "/missing" is refused
// NoSuchPath, and each op named in CLI_FAKE_REFUSE (comma-separated) is
// refused with code CLI_FAKE_CODE (default Conflict). An exec command
// "exit <n>" ends with exit code n.
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
const committed = { kind: "Committed", head: "c0ffee", commit };

function answer(op, args) {
  const refuse = (process.env.CLI_FAKE_REFUSE || "").split(",").filter(Boolean);
  if (refuse.includes(op)) return { refused: { code: process.env.CLI_FAKE_CODE || "Conflict", message: `${op} refused by the fake`, detail: { op } } };
  if (args.path === "/missing") return { refused: { code: "NoSuchPath", message: "/missing does not exist", detail: null } };
  switch (op) {
    case "project.init": return { value: { kind: "Initialized", project: args.name, head: "lay0" } };
    case "project.list_branches": return { value: { items: [
      { name: "main", base: "layer0000000000aa", head: null },
      { name: "cart", base: "base1234567890ab", head: "head1234567890ab" },
    ], hasMore: false, nextPagePos: null } };
    case "project.list_sandboxes": return { value: { kind: "ok", items: [{ sandbox: "1", name: "layerfs-site-1", image: "fake-image", workspaces: 2 }] } };
    case "branch.fork": return { value: { kind: "Forked", branch: args.name, base: "lay0", head: "h3ad0000000000ff" } };
    case "branch.merge": return { value: { kind: "Merged", branch: args.name, head: "m3rge" } };
    case "branch.publish": return { value: { kind: "Added", publishedLayer: { id: "lay1" } } };
    case "fs.read": return { value: args.startLine === 1 ? { text: "a\n", nextLine: 2 } : { text: "b\n", nextLine: null } };
    case "fs.stat": return { value: { type: "file", size: 5, mode: 420 } };
    case "workspace.mount": return { value: { kind: "Mounted", sandbox: args.sandbox, path: "/workspaces/w1", image: "fake-image", branch: args.branch } };
    case "workspace.exec": {
      const code = Number(/^exit (\d+)$/.exec(args.command)?.[1] ?? 0);
      return { value: { kind: `exit:${code}`, exitCode: code, stdout: `out:${args.command}`, stderr: "err", message: args.message ?? null, truncated: false } };
    }
    case "workspace.commit": return { value: committed };
    case "workspace.unmount": return { value: { kind: "Unmounted" } };
    case "sandbox.create": return { value: { sandbox: "2", name: "layerfs-site-2", image: args.deploy?.image } };
    case "sandbox.list_workspaces": return { value: { kind: "ok", version: "1", workspaces: [] } };
    case "sandbox.close": return { value: { kind: "Closed", sandbox: args.sandbox } };
    default:
      if (/^fs\.(write|edit|apply|mkdir|rm|cp|mv)$/.test(op)) return { value: committed };
      return { value: { items: [], hasMore: false, nextPagePos: null } };
  }
}

exports.call = async (op, args) => { log(op, args); return answer(op, args); };
exports.backendCall = exports.call;
exports.storeOpen = async (args) => { log("store.open", args); return { value: { clients: 1 } }; };
exports.storeClose = async (args) => { log("store.close", args); return { value: { clients: 0 } }; };
