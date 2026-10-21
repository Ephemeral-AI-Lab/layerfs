// Frames between the ported binding (binding.js) and the Store container's worker
// (worker.mjs): a 4-byte big-endian length, then a v8-serialized value.
import { deserialize, serialize } from "node:v8";

/** A value as one frame. */
export function frame(value) {
  const payload = serialize(value), head = Buffer.allocUnsafe(4);
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
      try { value = deserialize(bytes); } catch { return onBad(); }
      onValue(value);
    }
  });
}
