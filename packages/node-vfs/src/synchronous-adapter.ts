import type { FileStat } from "@ephemeralai/fs";
import type { NodeFileSession, NodeVfsProvider, OpenFileOptions } from "./index.js";

/**
 * Structural synchronous filesystem surface for host adapters such as FUSE.
 *
 * This deliberately has no dependency on a host filesystem library.  The
 * durable namespace, branch view, COW admission, and session semantics stay
 * in NodeVfsProvider; a host only supplies the object shape it already
 * consumes.
 */
export interface NodeVfsSynchronousFileSystem {
  existsSync(path: string): boolean;
  statSync(path: string): FileStat;
  lstatSync(path: string): FileStat;
  readdirSync(path: string): string[];
  readlinkSync(path: string): string;
  accessSync(path: string): void;
  readRangeSync(path: string, position: number, length: number): Uint8Array;
  readFileSync(path: string): Uint8Array;
  writeFileSync(path: string, bytes: Uint8Array, options?: { mode?: number }): void;
  createFileSync(path: string, options?: { mode?: number }): void;
  writeRangeSync(path: string, bytes: Uint8Array, position: number): number;
  truncateFileSync(path: string, size: number): void;
  mkdirSync(path: string, options?: { recursive?: boolean; mode?: number }): void;
  chmodSync(path: string, mode: number): void;
  linkSync(existingPath: string, newPath: string): void;
  symlinkSync(target: string, path: string): void;
  renameSync(oldPath: string, newPath: string): void;
  unlinkSync(path: string): void;
  rmdirSync(path: string): void;
}

/**
 * The portable core deliberately exposes numeric millisecond timestamps.
 * Host adapters such as FUSE use the conventional Date-shaped stat fields;
 * keep this conversion here so each host does not invent its own mapping.
 */
function hostStat(stat: FileStat): FileStat & {
  readonly mtime: Date;
  readonly atime: Date;
  readonly ctime: Date;
  readonly birthtime: Date;
} {
  // The portable stat contract stores the inode kind separately and keeps
  // mode as permission bits. POSIX hosts encode the kind in st_mode; without
  // these bits a FUSE kernel treats a directory root as a regular file and
  // rejects readdir/opendir with EIO.
  const typeMode =
    stat.type === "directory"
      ? 0o040000
      : stat.type === "symlink"
        ? 0o120000
        : 0o100000;
  return Object.freeze({
    ...stat,
    mode: typeMode | (stat.mode & 0o7777),
    mtime: new Date(stat.mtimeMs),
    atime: new Date(stat.mtimeMs),
    ctime: new Date(stat.ctimeMs),
    birthtime: new Date(stat.birthtimeMs),
  });
}

function closeSession(session: NodeFileSession): void {
  try {
    session.closeSync();
  } catch {
    session.abortSync();
  }
}

/** Adapt a branch-scoped Node VFS provider to a host's sync filesystem shape. */
export function createNodeVfsSynchronousFileSystem(
  provider: NodeVfsProvider,
): NodeVfsSynchronousFileSystem {
  const open = (path: string, options: OpenFileOptions = {}) =>
    provider.openFileSync(path, options);
  return {
    existsSync: (path) => provider.existsSync(path),
    statSync: (path) => hostStat(provider.statSync(path)),
    lstatSync: (path) => hostStat(provider.lstatSync(path)),
    readdirSync: (path) => provider.readdirSync(path),
    readlinkSync: (path) => provider.readlinkSync(path),
    accessSync: (path) => {
      provider.statSync(path);
    },
    readRangeSync: (path, position, length) =>
      provider.readRangeSync(path, position, length),
    readFileSync: (path) => {
      const stat = provider.statSync(path);
      return provider.readRangeSync(path, 0, stat.size);
    },
    writeFileSync: (path, bytes, options = {}) => {
      const session = open(path, {
        writable: true,
        create: true,
        truncate: true,
        ...(options.mode === undefined ? {} : { mode: options.mode }),
      });
      try {
        session.writeSync(bytes, 0);
        session.commitVisibleSync();
      } finally {
        closeSession(session);
      }
    },
    createFileSync: (path, options = {}) => {
      const session = open(path, {
        writable: true,
        create: true,
        exclusive: true,
        ...(options.mode === undefined ? {} : { mode: options.mode }),
      });
      try {
        session.commitVisibleSync();
      } finally {
        closeSession(session);
      }
    },
    writeRangeSync: (path, bytes, position) => {
      const session = open(path, { writable: true });
      try {
        const written = session.writeSync(bytes, position);
        session.commitVisibleSync();
        return written;
      } finally {
        closeSession(session);
      }
    },
    truncateFileSync: (path, size) => {
      const session = open(path, { writable: true });
      try {
        session.truncateSync(size);
        session.commitVisibleSync();
      } finally {
        closeSession(session);
      }
    },
    mkdirSync: (path, options = {}) => provider.mkdirSync(path, options),
    chmodSync: (path, mode) => provider.chmodSync(path, mode),
    linkSync: (existingPath, newPath) => provider.linkSync(existingPath, newPath),
    symlinkSync: (target, path) => provider.symlinkSync(target, path),
    renameSync: (oldPath, newPath) => provider.renameSync(oldPath, newPath),
    unlinkSync: (path) => provider.unlinkSync(path),
    rmdirSync: (path) => provider.rmdirSync(path),
  };
}
