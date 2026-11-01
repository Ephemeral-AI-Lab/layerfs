// Values across the addon: JSON text, bytes as { "base64": … } both ways.
const isPlain = (value) =>
  value !== null && typeof value === "object" && !Array.isArray(value) && !ArrayBuffer.isView(value);

/** A JavaScript value as the library's JSON text. */
export function encode(value) {
  return JSON.stringify(value, (_key, item) => {
    if (item instanceof Uint8Array) return { base64: Buffer.from(item).toString("base64") };
    if (item && item.type === "Buffer" && Array.isArray(item.data)) {
      return { base64: Buffer.from(item.data).toString("base64") };
    }
    if (typeof item === "bigint") return Number(item);
    return item;
  });
}

/** The library's JSON text as a JavaScript value; `{ base64 }` becomes a Buffer. */
export function decode(text) {
  return JSON.parse(text, (_key, item) => {
    if (isPlain(item) && Object.keys(item).length === 1 && typeof item.base64 === "string") {
      return Buffer.from(item.base64, "base64");
    }
    return item;
  });
}

export { isPlain };
