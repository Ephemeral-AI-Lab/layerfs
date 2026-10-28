// A stand-in binding for workspace.exec output checks: no Docker or daemon.
// `exports.outputs[command]` is `{ stdout, stderr }` Buffers; a stream over
// INLINE bytes (or any non-empty stream with `spool`) is spooled and paged
// like the real binding (engine/crates/layerfs-node/src/output.rs) into the caller's
// page. `exports.failPage` refuses the next page read.
const INLINE = 1 << 20;
const PAGE = 1 << 20;
exports.outputs = {};
exports.spooled = new Map();
exports.calls = [];
exports.failPage = false;
let next = 0;
const refused = (code, message) => ({ refused: { code, message, detail: null } });
exports.call = async (op, args) => {
  exports.calls.push([op, { ...args }]);
  if (op === "workspace.exec") {
    const out = exports.outputs[args.command];
    const fields = [["kind", "exit:0"], ["exitCode", 0]];
    const spooled = {};
    for (const name of ["stdout", "stderr"]) {
      const bytes = out[name];
      if (bytes.length > (args.spool ? 0 : INLINE)) { spooled[name] = bytes; fields.push([name, null]); }
      else fields.push([name, bytes.toString("utf8")]);
    }
    fields.push(["message", args.message ?? null], ["truncated", false]);
    if (Object.keys(spooled).length) {
      const id = `out_${++next}`;
      exports.spooled.set(id, spooled);
      fields.push(["output", id], ["stdoutBytes", out.stdout.length], ["stderrBytes", out.stderr.length]);
    }
    return { value: Object.fromEntries(fields) };
  }
  // LayerFS.open's project lookup
  if (op === "project.list_branches") return { value: { items: [], hasMore: false, nextPagePos: null } };
  if (op === "workspace.exec_release") return { value: { kind: "ok", released: exports.spooled.delete(args.output) } };
  return refused("UnknownMethod", op);
};
// Fills the caller's page like the real binding's execOutput.
exports.execOutput = async (args, page) => {
  exports.calls.push(["execOutput", { ...args, page: page.length }]);
  if (exports.failPage) { exports.failPage = false; return refused("Failed", "page read failed"); }
  const bytes = exports.spooled.get(args.output)?.[args.stream];
  if (!bytes) return refused("InvalidInput", `no spooled ${args.stream} in ${args.output}`);
  if (!(page.length >= 1 && page.length <= PAGE)) return refused("InvalidInput", "page");
  const length = bytes.copy(page, 0, args.offset, args.offset + page.length);
  return { value: { kind: "ok", length, end: args.offset + length === bytes.length } };
};
exports.backendCall = exports.call;
exports.storeOpen = async () => ({ value: { clients: 1 } });
exports.storeClose = async () => ({ value: { clients: 0 } });
