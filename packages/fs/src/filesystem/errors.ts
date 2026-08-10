export type FilesystemErrorCode = "EINVAL" | "ENOENT" | "ENOTDIR" | "EISDIR" | "EEXIST" | "ENOTEMPTY" | "ELOOP" | "EPERM" | "EROFS" | "EBADF" | "EAGAIN" | "EBUSY" | "EFBIG" | "ENOSPC" | "ECORRUPT" | "ESCHEMA" | "EIO";

export class FilesystemError extends Error {
  readonly name = "FilesystemError" as const; readonly code: FilesystemErrorCode; readonly syscall?: string; readonly path?: string; readonly destination?: string;
  constructor(code: FilesystemErrorCode, message: string, options: { syscall?: string; path?: string; destination?: string; cause?: unknown } = {}) {
    super(message, options.cause === undefined ? undefined : { cause: options.cause }); this.code = code;
    if (options.syscall !== undefined) this.syscall = options.syscall; if (options.path !== undefined) this.path = options.path; if (options.destination !== undefined) this.destination = options.destination;
  }
}

export function fsError(code: FilesystemErrorCode, syscall: string, path: string | undefined, detail: string, cause?: unknown): FilesystemError {
  return new FilesystemError(code, `${code}: ${detail}${path ? `, ${syscall} '${path}'` : `, ${syscall}`}`, { syscall, ...(path === undefined ? {} : { path }), ...(cause === undefined ? {} : { cause }) });
}

export function mapStorageError(error: unknown, syscall: string, path?: string): never {
  if (error instanceof FilesystemError) throw error;
  const message = error instanceof Error ? error.message : String(error);
  for (const code of ["ESCHEMA", "ECORRUPT", "EROFS", "ENOSPC", "EBUSY"] as const) if (message.includes(code)) throw fsError(code, syscall, path, message, error);
  if (/busy|locked/i.test(message)) throw fsError("EBUSY", syscall, path, message, error);
  if (error instanceof RangeError && /managed resident|quota|capacity/i.test(message)) throw fsError("ENOSPC", syscall, path, message, error);
  if (/limit|too large|range/i.test(message)) throw fsError("EFBIG", syscall, path, message, error);
  if (error instanceof RangeError || error instanceof TypeError) throw fsError("EINVAL", syscall, path, message, error);
  throw fsError("EIO", syscall, path, message, error);
}

export function abortError(): DOMException { return new DOMException("The operation was aborted", "AbortError"); }
