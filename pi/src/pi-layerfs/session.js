// What the session's tools share: the Branch the session works on, paged
// listings, the text results tools return, and how LayerFS refusals read as
// tool errors.
import { createHash } from "node:crypto";
import { isRefusal } from "./sdk.js";

const BRANCH_PREFIX = "pi-";
const BRANCH_ID_LENGTH = 12;
/** The SDK's largest page. */
export const MAX_PAGE = 1000;

/**
 * The Branch of a session started without --layerfs-branch, derived from its
 * id so a resumed session finds its Branch again with no state of its own.
 * Pi's session ids are UUIDv7, whose leading digits are a clock reading that
 * sessions started within a minute share, so the name hashes the whole id.
 */
export const sessionBranch = (sessionId) =>
  BRANCH_PREFIX + createHash("sha256").update(sessionId).digest("hex").slice(0, BRANCH_ID_LENGTH);

/** Makes sure `branch` exists, forking it from main when it does not; NameTaken means it already does. */
export async function ensureBranch(lf, branch) {
  try {
    await lf.branch.fork(branch);
  } catch (error) {
    if (!isRefusal(error, "NameTaken")) throw error;
  }
}

/** Up to `limit` items of a paged listing, `page(pageSize, pagePos)`, and whether more remain. */
export async function upTo(limit, page) {
  const items = [];
  for (let pos = 0; ; ) {
    const answer = await page(Math.min(MAX_PAGE, limit - items.length), pos);
    items.push(...answer.items);
    if (!answer.hasMore || items.length >= limit) return { items, more: answer.hasMore };
    pos += answer.items.length;
  }
}

/** A tool result of one text, with `details` for Pi's renderers. */
export const text = (value, details = undefined) => ({ content: [{ type: "text", text: value }], details });

/**
 * An error as a tool's error text: a LayerFS refusal is the SDK's
 * `code: message`, then what its detail names; any other error its message.
 */
export function errorText(error) {
  const detail = isRefusal(error) ? error.detail : null;
  if (!detail) return error.message;
  if (detail.conflicts) return `${error.message}\nConflicting paths: ${detail.conflicts.map((c) => c.path).join(", ")}`;
  return `${error.message}\n${JSON.stringify(detail)}`;
}

/** A tool definition whose failures read as `errorText`. */
export function reporting(definition) {
  return {
    ...definition,
    async execute(...args) {
      try {
        return await definition.execute(...args);
      } catch (error) {
        throw new Error(errorText(error));
      }
    },
  };
}
