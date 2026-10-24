// One binding call made in Linux by the worker (server.js) or by Init's own
// process (init.js). Copied from the dsh Host's port/call.mjs (#428). A
// spooled exec's output is read here and returned whole, then released, so
// no spool is left for the caller to read.
const PAGE = 1 << 20;

/** The Store a call names, as the binding's storeOpen and storeClose take it. */
export const storeOf = (args) => ({ project: args.project, home: args.home, storePath: args.storePath });

/** Makes the call once: its answer, `{ value }` or `{ refused }`. */
export async function made(binding, fn, op, args) {
  const answer = await binding[fn](op, args);
  return fn === "call" && op === "workspace.exec" && answer.value?.output !== undefined ? whole(binding, answer, args.spool) : answer;
}

/** A spooled exec's streams as text (bytes for sinks, `spool`), released after. */
async function whole(binding, answer, spool) {
  const { output, ...value } = answer.value;
  const page = Buffer.allocUnsafe(PAGE);
  try {
    for (const name of ["stdout", "stderr"]) {
      if (value[name] !== null) continue;
      const parts = [];
      for (let offset = 0, end = false; !end; ) {
        const read = await binding.execOutput({ output, stream: name, offset }, page);
        if (read.refused) return read;
        if (!read.value.end && read.value.length === 0) return { refused: { code: "Failed", message: `${name} page at ${offset} was empty`, detail: null } };
        parts.push(Buffer.from(page.subarray(0, read.value.length)));
        offset += read.value.length;
        end = read.value.end;
      }
      const bytes = Buffer.concat(parts);
      value[`${name}Bytes`] = bytes.length;
      value[name] = spool ? bytes : bytes.toString("utf8");
    }
  } finally {
    await binding.call("workspace.exec_release", { output });
  }
  return { ...answer, value };
}
