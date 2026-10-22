// Frames between the ported binding and the Store container's processes
// (binding.js, worker.mjs, run.mjs): a 4-byte big-endian length, then the value
// as JSON, with Buffers carried as base64.
//
// JSON rather than `v8.serialize`: the two sides are different Node builds, and
// v8's wire format starts with a version byte. A Host whose V8 is newer than the
// image's writes 0x10 where the image's `deserialize` accepts only 0x0f, so the
// frame never decodes — the worker drops the connection and the Host sees a
// closed socket rather than an error. Buffers are the only value these calls
// carry that JSON cannot express (file contents and exec output pages), so one
// base64 marker covers them.

/** A value as text `parseValue` reads back, Buffer included. */
export const stringifyValue = (value) => JSON.stringify(value, (_key, v) => (
  v && v.type === "Buffer" && Array.isArray(v.data) ? { $buffer: Buffer.from(v.data).toString("base64") } : v));

/** `stringifyValue`'s inverse. */
export const parseValue = (text) => JSON.parse(text, (_key, v) => (
  v && typeof v.$buffer === "string" ? Buffer.from(v.$buffer, "base64") : v));

/** A value as one frame. */
export function frame(value) {
  const payload = Buffer.from(stringifyValue(value), "utf8"), head = Buffer.allocUnsafe(4);
  head.writeUInt32BE(payload.length);
  return Buffer.concat([head, payload]);
}

/** Calls `onValue` with each value `socket` receives, in order; a frame that does not decode calls `onBad`. */
export function readFrames(socket, onValue, onBad) {
  let parts = [], have = 0, need = null;
  const take = (n) => {
    const all = parts.length === 1 ? parts[0] : Buffer.concat(parts, have);
    parts = all.length > n ? [all.subarray(n)] : [];
    have -= n;
    return all.subarray(0, n);
  };
  socket.on("data", (chunk) => {
    parts.push(chunk);
    have += chunk.length;
    for (;;) {
      if (need === null && have >= 4) need = take(4).readUInt32BE(0);
      if (need === null || have < need) return;
      const bytes = take(need);
      need = null;
      let value;
      try { value = parseValue(bytes.toString("utf8")); } catch { return onBad(); }
      onValue(value);
    }
  });
}
