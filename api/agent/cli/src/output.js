// What layerfs prints: a result as one line of JSON (or readable text with
// --text), and a refusal on stderr as {"code", "message", "detail"}. Bytes
// print as { base64 } and big integers as strings, so every result is JSON.

/** `value` with bytes as { base64 } and bigints as strings. */
export function plain(value) {
  if (typeof value === "bigint") return String(value);
  if (value instanceof Uint8Array) return { base64: Buffer.from(value.buffer, value.byteOffset, value.byteLength).toString("base64") };
  if (value instanceof ArrayBuffer) return { base64: Buffer.from(value).toString("base64") };
  if (Array.isArray(value)) return value.map(plain);
  if (value && typeof value === "object") return Object.fromEntries(Object.entries(value).map(([k, v]) => [k, plain(v)]));
  return value;
}

const scalar = (v) => (v === null || v === undefined ? "-" : String(v));
const nested = (v) => v && typeof v === "object" && (Array.isArray(v) ? v.length > 0 : Object.keys(v).length > 0);

function lines(value, pad, out) {
  if (Array.isArray(value)) {
    for (const item of value) {
      if (nested(item)) { out.push(`${pad}-`); lines(item, `${pad}  `, out); }
      else out.push(`${pad}- ${scalar(item)}`);
    }
    return;
  }
  for (const [key, v] of Object.entries(value)) {
    if (typeof v === "string" && v.includes("\n")) {
      out.push(`${pad}${key}: |`);
      for (const line of v.replace(/\n$/, "").split("\n")) out.push(`${pad}  ${line}`);
    } else if (nested(v)) {
      out.push(`${pad}${key}:`);
      lines(v, `${pad}  `, out);
    } else out.push(`${pad}${key}: ${Array.isArray(v) ? "(none)" : v && typeof v === "object" ? "{}" : scalar(v)}`);
  }
}

/** A result as it is printed. */
export function render(value, text) {
  const v = plain(value);
  if (!text) return `${JSON.stringify(v ?? null)}\n`;
  if (!nested(v)) return `${scalar(v)}\n`;
  const out = [];
  lines(v, "", out);
  return `${out.join("\n")}\n`;
}

/** A failed call as it is printed on stderr. */
export function refusal(error, text) {
  const code = typeof error?.code === "string" ? error.code : null;
  const raw = String(error?.message ?? error);
  const message = code && raw.startsWith(`${code}: `) ? raw.slice(code.length + 2) : raw;
  if (text) return `${code ? `${code}: ` : "layerfs: "}${message}\n`;
  return `${JSON.stringify({ code, message, detail: plain(error?.detail ?? null) })}\n`;
}
