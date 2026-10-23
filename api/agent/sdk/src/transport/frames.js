// Frames between the worker client (worker_client.js) and a LayerFS home's
// worker (`@layerfs/runtime`'s worker/server.js): a 4-byte big-endian length,
// then a JSON value tagged "layerfs-worker" with its wire version,
// independent of the Node/V8 build. Both ends run the same SDK (the worker
// checks it), so the tag changes with the code. Every
// compound value is tagged so user objects cannot collide with binary markers.
const invalid = (message) => Object.assign(new Error(`LayerFS worker protocol: ${message}; restart the worker and its clients together`), { code: "ProtocolError" });
function pack(value) {
  if (value === undefined) return ["undefined"];
  if (typeof value === "bigint") return ["bigint", String(value)];
  if (typeof value === "number" && (!Number.isFinite(value) || Object.is(value, -0))) return ["number", Object.is(value, -0) ? "-0" : String(value)];
  if (value === null || typeof value !== "object") return value;
  if (Buffer.isBuffer(value)) return ["buffer", value.toString("base64")];
  if (value instanceof Uint8Array) return ["bytes", Buffer.from(value).toString("base64")];
  if (value instanceof ArrayBuffer) return ["arraybuffer", Buffer.from(value).toString("base64")];
  if (Array.isArray(value)) return ["array", value.map(pack)];
  if (Object.getPrototypeOf(value) !== Object.prototype && Object.getPrototypeOf(value) !== null) throw invalid("unsupported value type");
  return ["object", Object.entries(value).map(([key, item]) => [key, pack(item)])];
}
function unpack(value) {
  if (!Array.isArray(value)) {
    if (value !== null && typeof value === "object") throw invalid("untagged object");
    return value;
  }
  const [tag, data] = value;
  switch (tag) {
    case "undefined": if (value.length === 1) return undefined; break;
    case "bigint": if (typeof data === "string" && /^-?\d+$/.test(data)) return BigInt(data); break;
    case "number": if (["NaN", "Infinity", "-Infinity", "-0"].includes(data)) return Number(data); break;
    case "buffer": case "bytes": case "arraybuffer": {
      if (typeof data !== "string" || (data.length % 4 !== 0 || !/^[A-Za-z0-9+/]*={0,2}$/.test(data))) break;
      const bytes = Buffer.from(data, "base64");
      return tag === "buffer" ? bytes : tag === "bytes" ? Uint8Array.from(bytes) : Uint8Array.from(bytes).buffer;
    }
    case "array": if (Array.isArray(data)) return data.map(unpack); break;
    case "object":
      if (Array.isArray(data) && data.every((pair) => Array.isArray(pair) && pair.length === 2 && typeof pair[0] === "string")) return Object.fromEntries(data.map(([key, item]) => [key, unpack(item)]));
  }
  throw invalid("invalid value tag");
}
const TAG = "layerfs-worker";
export const encode = (value) => Buffer.from(JSON.stringify([TAG, 1, pack(value)]));
export function decode(bytes) {
  let value;
  try { value = JSON.parse(bytes.toString("utf8")); } catch { throw invalid("invalid JSON or legacy V8 frame"); }
  if (!Array.isArray(value) || value.length !== 3 || value[0] !== TAG || value[1] !== 1) throw invalid("unsupported wire version");
  return unpack(value[2]);
}

/** A value as one frame. */
export function frame(value) {
  const payload = encode(value), head = Buffer.allocUnsafe(4);
  head.writeUInt32BE(payload.length);
  return Buffer.concat([head, payload]);
}

/** Calls `onValue` with each value `socket` receives, in order; a frame that
 * does not decode calls `onBad`. `firstLimit` caps the first frame's length
 * (the unauthenticated secret frame on a worker); a longer one calls `onBad`
 * as soon as its length arrives. Later frames have no cap. */
export function readFrames(socket, onValue, onBad, { firstLimit = Infinity } = {}) {
  let parts = [], have = 0, need = null, failed = false, first = true;
  const take = (n) => {
    const all = parts.length === 1 ? parts[0] : Buffer.concat(parts, have);
    parts = all.length > n ? [all.subarray(n)] : [];
    have -= n;
    return all.subarray(0, n);
  };
  socket.on("data", (chunk) => {
    if (failed) return;
    parts.push(chunk);
    have += chunk.length;
    for (;;) {
      if (need === null && have >= 4) {
        need = take(4).readUInt32BE(0);
        if (first && need > firstLimit) { failed = true; parts = []; have = 0; return onBad(invalid(`a first frame of ${need} bytes is over ${firstLimit}`)); }
      }
      if (need === null || have < need) return;
      first = false;
      const bytes = take(need);
      need = null;
      let value;
      try { value = decode(bytes); } catch (error) { failed = true; parts = []; have = 0; return onBad(error); }
      onValue(value);
    }
  });
}
