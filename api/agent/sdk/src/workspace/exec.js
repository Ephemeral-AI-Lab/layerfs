// workspace.exec: one command in the Workspace.
//
// Output of any size arrives complete. The binding keeps each stream inline
// up to 1 MiB; a result whose streams fit is returned exactly as the binding
// gives it. A larger stream is spooled by the binding to an unlinked host
// file and read back here a page at a time into one reused 1 MiB window,
// then the spool is released. As text it is decoded as UTF-8 (invalid bytes
// become U+FFFD), so it is bounded by the engine's longest string (2^29-24
// UTF-16 units in 64-bit V8). With `{ stdout, stderr }` sinks (Node Writable
// streams) the named streams' raw bytes are written to them instead, each
// page once the sink's callback reports the last one written, so the call
// holds one page of output however long it is. A sink must not keep a chunk
// after its write callback (file, socket and hash streams do not); the sinks
// are not ended, and the result's field for each is null. The result then
// also carries `stdoutBytes` and `stderrBytes`. Page reads are not traced:
// the call's trace line covers the command's run.
import { invoke, invokeClient, outcome, transportOf, workspaceArgs, checkedMessage } from "../client.js";
import { LayerFSError } from "../errors.js";

const PAGE = 1 << 20;

export async function exec(client, ws, command, message, sinks) {
  const to = checkedSinks(sinks);
  const value = await invokeClient(client, "workspace.exec", { ...workspaceArgs(ws), command, message: checkedMessage(message), ...(to ? { spool: true } : {}) });
  if (!to && value.output === undefined) return value;
  const { output, ...result } = value;
  result.stdoutBytes ??= Buffer.byteLength(result.stdout ?? "");
  result.stderrBytes ??= Buffer.byteLength(result.stderr ?? "");
  let window = null;
  try {
    for (const name of ["stdout", "stderr"]) {
      const sink = to?.[name];
      if (result[name] !== null) {
        if (sink && result[name]) await put(sink, Buffer.from(result[name]));
        if (sink) result[name] = null;
        continue;
      }
      window ??= Buffer.allocUnsafe(PAGE);
      const decoder = sink ? null : new TextDecoder();
      const total = result[`${name}Bytes`];
      let text = "";
      for (let offset = 0, end = false; !end; ) {
        const page = window.subarray(0, Math.min(PAGE, Math.max(1, total - offset)));
        const read = outcome(await transportOf(client).execOutput({ output, stream: name, offset }, page));
        if (!read.end && read.length === 0) throw new LayerFSError("Failed", `${name} page at ${offset} was empty`, null);
        offset += read.length;
        end = read.end;
        if (sink) await put(sink, page.subarray(0, read.length));
        else text += decoder.decode(page.subarray(0, read.length), { stream: !end });
      }
      if (!sink) result[name] = text;
    }
  } finally {
    if (output !== undefined) await invoke("workspace.exec_release", { output }, {}, transportOf(client));
  }
  return result;
}

function checkedSinks(sinks) {
  if (sinks === undefined) return null;
  const ok = (sink) => sink === undefined || (sink && typeof sink.write === "function" && typeof sink.once === "function");
  if (!sinks || typeof sinks !== "object" || !ok(sinks.stdout) || !ok(sinks.stderr)) {
    throw new LayerFSError("InvalidInput", "sinks must be { stdout?, stderr? } Writable streams", null);
  }
  return sinks.stdout || sinks.stderr ? sinks : null;
}

/** One write, finished: the sink's callback has run, so `bytes` may be reused. */
function put(sink, bytes) {
  return new Promise((resolve, reject) => sink.write(bytes, (error) => (error ? reject(error) : resolve())));
}
