// A write's binary body. Any typed array, DataView or ArrayBuffer becomes a
// Buffer over the same memory, without a copy, so every route takes it: the
// binding reads any view as bytes, but the worker's encoding carries only
// Buffer, Uint8Array and ArrayBuffer. The binding copies the bytes when the
// call starts, after this client's Store is open, so the caller must not
// change them until the call settles. Memory another thread can change
// (SharedArrayBuffer) and a transferred (detached) ArrayBuffer are refused.
// Other values pass unchanged.
import { types } from "node:util";
import { LayerFSError } from "../errors.js";

const refused = (what) => new LayerFSError("InvalidInput", `write bytes ${what} are not accepted`, null);

/** `value` as a Buffer when it is binary, else `value` itself. */
export function body(value) {
  const view = ArrayBuffer.isView(value);
  if (!view && !types.isAnyArrayBuffer(value)) return value;
  const memory = view ? value.buffer : value;
  if (types.isSharedArrayBuffer(memory)) throw refused("in shared memory (SharedArrayBuffer)");
  if (memory.detached) throw refused("in a detached ArrayBuffer");
  if (Buffer.isBuffer(value)) return value;
  try {
    return view ? Buffer.from(memory, value.byteOffset, value.byteLength) : Buffer.from(memory);
  } catch {
    // Node 20 has no `detached` flag; a detached buffer or view throws here.
    throw refused("in a detached ArrayBuffer");
  }
}

/** One `fs.apply` change with its write body as `fs.write` sends it: binary
 * `content` travels as `bytes`. */
export function change(c) {
  if (c?.op !== "write") return c;
  if (c.content !== undefined && typeof c.content !== "string" && c.bytes === undefined) {
    const { content, ...rest } = c;
    return { ...rest, bytes: body(content) };
  }
  return c.bytes === undefined ? c : { ...c, bytes: body(c.bytes) };
}
