/** A refused call: `code` is stable (InvalidInput, NotFound, InUse, Exists,
 * Conflict, Unsupported, Unavailable, Unknown, Failed, …), `detail` is the
 * library's structured detail. */
export class LayerFSError extends Error {
  constructor(code, message, detail) {
    super(message);
    this.name = "LayerFSError";
    this.code = code;
    this.detail = detail ?? null;
  }
}
