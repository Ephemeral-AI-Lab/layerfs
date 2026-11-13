// A refusal as an MCP tool result: isError true, one text block holding the
// structured { code, message, detail }, and, for an uncertain outcome, a
// second block that says what to do next and that nothing was retried.

import { encodeResult, isUncertain } from "./values.js";

/** The fixed sentence of an uncertain result, checked by the tests. */
export const UNCERTAIN = "outcome uncertain: read state before acting again; nothing was retried.";

/** `{ content, isError }` for a LayerFSError. */
export function errorResult(error) {
  const body = {
    code: error?.code ?? "Failed",
    message: error?.message ?? String(error),
    detail: encodeResult(error?.detail ?? null),
  };
  const content = [{ type: "text", text: JSON.stringify(body) }];
  if (isUncertain(error)) content.push({ type: "text", text: UNCERTAIN });
  return { content, isError: true };
}
