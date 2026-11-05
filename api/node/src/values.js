// Values across the addon: JSON text, bytes as { "base64": … } both ways,
// whole numbers exactly.
//
// Encoding never loses a value silently. A `bigint` is written as its own
// digits. A `number` that is NaN, ±Infinity or an integer beyond
// ±(2^53 - 1) is refused, as are a Map, a Set, an ArrayBuffer, a typed array
// other than Uint8Array (a Buffer is one), a DataView, a function, a symbol
// and text with a lone surrogate, all with `LayerFSError("InvalidInput")`
// before any call. Decoding returns a whole number as a `number` when it is
// a safe integer and as a `bigint`, exactly, when it is not. (`JSON.rawJSON`
// and the `context.source` of `JSON.parse`'s reviver need Node 22.)
import { LayerFSError } from "./errors.js";

const isPlain = (value) =>
  value !== null && typeof value === "object" && !Array.isArray(value) && !ArrayBuffer.isView(value);

// The library reads a one-key map { "base64": text } as bytes when the text
// is base64 (base64 letters, then any `=`); `decode` reads it the same way.
const BASE64 = /^[A-Za-z0-9+/]*=*$/;
const WHOLE = /^-?\d+$/;

function refuse(message) {
  return new LayerFSError("InvalidInput", message, null);
}

/** A JavaScript value as the library's JSON text. */
export function encode(value) {
  try {
    return JSON.stringify(value, function replace(key, item) {
      // `this[key]` is the value before any toJSON (a Buffer has one)
      const original = this[key];
      const at = key === "" ? "the value" : `"${key}"`;
      if (!key.isWellFormed()) throw refuse(`${at}: a name with a lone surrogate cannot be sent`);
      if (typeof original === "bigint") return JSON.rawJSON(original.toString());
      if (original instanceof Uint8Array) return { base64: Buffer.from(original).toString("base64") };
      if (typeof item === "number" && (!Number.isFinite(item) || (Number.isInteger(item) && !Number.isSafeInteger(item)))) {
        throw refuse(`${at}: ${item} is not a safe integer; send a bigint for a whole number beyond ±(2^53 - 1)`);
      }
      if (typeof item === "string" && !item.isWellFormed()) throw refuse(`${at}: text with a lone surrogate cannot be sent`);
      if (
        original instanceof Map || original instanceof Set || original instanceof ArrayBuffer ||
        (typeof SharedArrayBuffer === "function" && original instanceof SharedArrayBuffer) ||
        ArrayBuffer.isView(original) || typeof original === "function" || typeof original === "symbol"
      ) {
        throw refuse(`${at}: ${original?.constructor?.name ?? typeof original} cannot be sent; use plain objects, arrays, strings, numbers, bigints and Uint8Array`);
      }
      return item;
    });
  } catch (error) {
    if (error instanceof LayerFSError) throw error;
    throw refuse(`the value cannot be sent: ${error.message}`);
  }
}

/** The library's JSON text as a JavaScript value: a whole number is a
 * `number` when it is a safe integer, else a `bigint`; `{ base64 }` becomes
 * a Buffer. */
export function decode(text) {
  return JSON.parse(text, (_key, item, context) => {
    if (typeof item === "number" && !Number.isSafeInteger(item) && WHOLE.test(context.source)) {
      return BigInt(context.source);
    }
    if (isPlain(item) && Object.keys(item).length === 1 && typeof item.base64 === "string" && BASE64.test(item.base64)) {
      return Buffer.from(item.base64, "base64");
    }
    return item;
  });
}

export { isPlain };
