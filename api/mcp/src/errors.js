// A refusal as an MCP tool result: isError true, one text block holding the
// refusal as every front end answers it, { error, message, fix } and the
// fields the next call needs (detail when the call asked for the long form),
// and, for an uncertain outcome, a second block that says what to do next and
// that nothing was retried.

import { encodeResult, isUncertain } from "./values.js";

/** The fixed sentence of an uncertain result, checked by the tests. */
export const UNCERTAIN = "outcome uncertain: read state before acting again; nothing was retried.";

/** What a LayerFSError has of its own; every other own property is a field the next call needs. */
const OWN = new Set(["name", "code", "error", "message", "fix", "detail", "stack", "cause"]);

/** `{ content, isError }` for a LayerFSError. */
export function errorResult(error) {
  const body = {
    error: error?.code ?? "Failed",
    message: error?.message ?? String(error),
  };
  if (typeof error?.fix === "string") body.fix = error.fix;
  if (error !== null && typeof error === "object") {
    for (const [key, value] of Object.entries(error)) {
      if (!OWN.has(key) && value !== undefined) body[key] = encodeResult(value);
    }
  }
  if (error?.detail !== undefined && error?.detail !== null) body.detail = encodeResult(error.detail);
  const content = [{ type: "text", text: JSON.stringify(body) }];
  if (isUncertain(error)) content.push({ type: "text", text: UNCERTAIN });
  return { content, isError: true };
}
