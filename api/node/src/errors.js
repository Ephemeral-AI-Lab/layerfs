/** A refused call. `code` (also `error`) is one of the thirteen public codes:
 * NotFound, Exists, InUse, Busy, Unresolved, Conflict, Dirty, Capacity,
 * InvalidInput, Unsupported, Unavailable, Failed, Unknown. `fix` is one
 * runnable shell line, or null. The fields the next call needs (a
 * Conflict's `commit`, a Busy's `op`, a Dirty's `paths`) are properties of
 * the error. `detail` is the library's structured detail, present when the
 * call asked for the long form (`{ detail: true }`), else null. */
export class LayerFSError extends Error {
  constructor(code, message, detail) {
    super(message);
    this.name = "LayerFSError";
    this.code = code;
    this.error = code;
    this.fix = null;
    this.detail = detail ?? null;
  }

  /** The error of a refusal as the library answers it:
   * `{ error, message, fix?, detail?, ...fields }`. */
  static from(refused) {
    const { error, message, fix, detail, ...fields } = refused;
    const made = new LayerFSError(error, message, detail);
    made.fix = fix ?? null;
    for (const [key, value] of Object.entries(fields)) {
      // a field never replaces what an Error already has (name, stack, code)
      if (!(key in made)) made[key] = value;
    }
    return made;
  }
}
