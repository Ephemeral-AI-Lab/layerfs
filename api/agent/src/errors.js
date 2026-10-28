// Typed refusals. `code` is stable and alone decides; `message` is for people.

/** A refused LayerFS call. */
export class LayerFSError extends Error {
  /**
   * @param {string} code stable refusal code, for example "Conflict"
   * @param {string} message
   * @param {unknown} detail structured detail, for example conflicting files
   */
  constructor(code, message, detail) {
    super(`${code}: ${message}`);
    this.name = "LayerFSError";
    this.code = code;
    this.detail = detail ?? null;
  }
}

/** Codes a caller is expected to handle. */
export const CODES = Object.freeze([
  "Conflict", "HeadMoved", "Dirty", "Busy", "NoSuchPath", "NoSuchBranch", "NoSuchCommit",
  "NoSuchProject", "NoCommit", "NoMergeBase", "IsDirectory", "NotDirectory", "NotFile", "Binary",
  "NotText", "NoMatch", "AmbiguousMatch",
  "Exists", "NotEmpty", "NameTaken", "ReservedName", "ReadOnly", "MountsActive", "TooLarge",
  "InvalidInput", "NotMounted", "NoSuchSandbox", "NoSuchProcess", "Capacity", "NotComparable",
]);

/** True when `error` is a LayerFS refusal with `code`. */
export function isRefusal(error, code) {
  return error instanceof LayerFSError && (code === undefined || error.code === code);
}
