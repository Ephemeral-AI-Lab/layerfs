import { bytesToHex } from "../cas/bytes.js";
import { IncrementalSha256, sha256 } from "../cas/sha256.js";
import { DEFAULT_FASTCDC } from "../cdc/fastcdc.js";
import { encodeUtf8 } from "../namespace/utf8.js";
import { buildManifest } from "../operations/full-rebuild.js";

function concat(parts: readonly Uint8Array[]): Uint8Array {
  const length = parts.reduce((total, part) => total + part.byteLength, 0);
  if (!Number.isSafeInteger(length)) throw new RangeError("digest row is too large");
  const result = new Uint8Array(length);
  let offset = 0;
  for (const part of parts) {
    result.set(part, offset);
    offset += part.byteLength;
  }
  return result;
}

function u32(value: number): Uint8Array {
  if (!Number.isSafeInteger(value) || value < 0 || value > 0xffff_ffff)
    throw new RangeError("digest uint32 is outside its canonical range");
  const result = new Uint8Array(4);
  new DataView(result.buffer).setUint32(0, value, false);
  return result;
}

function u64(value: number): Uint8Array {
  if (!Number.isSafeInteger(value) || value < 0)
    throw new RangeError("digest uint64 is outside its canonical range");
  const result = new Uint8Array(8);
  new DataView(result.buffer).setBigUint64(0, BigInt(value), false);
  return result;
}

function text(value: string): Uint8Array {
  const bytes = encodeUtf8(value);
  return concat([u32(bytes.byteLength), bytes]);
}

function optional(value: Uint8Array | null): Uint8Array {
  return value === null ? Uint8Array.of(0) : concat([Uint8Array.of(1), value]);
}

function compareBytes(left: Uint8Array, right: Uint8Array): number {
  const length = Math.min(left.byteLength, right.byteLength);
  for (let index = 0; index < length; index += 1) {
    const difference = left[index]! - right[index]!;
    if (difference !== 0) return difference;
  }
  return left.byteLength - right.byteLength;
}

function root(domain: string, rows: readonly Uint8Array[]): Uint8Array {
  const digest = new IncrementalSha256().update(encodeUtf8(`${domain}\0`));
  digest.update(u64(rows.length));
  for (const row of rows) {
    digest.update(u32(row.byteLength));
    digest.update(row);
  }
  return digest.digest();
}

export interface BranchGenerationPage {
  readonly index: number;
  readonly bytes: Uint8Array;
}

export interface BranchGenerationPatch {
  readonly order: number;
  readonly offset: number;
  readonly deleteLength: number;
  /** Canonical manifest digest of inserted immutable content, when present. */
  readonly insertManifestDigest: Uint8Array | null;
}

export interface BranchGenerationNode {
  readonly inodeId: string;
  readonly kind: "file" | "directory" | "symlink";
  readonly mode: number;
  readonly birthtimeMs: number;
  readonly mtimeMs: number;
  readonly ctimeMs: number;
  readonly logicalSize: number;
  readonly manifestHash: Uint8Array | null;
  readonly pages: readonly BranchGenerationPage[];
  readonly patches: readonly BranchGenerationPatch[];
  readonly symlinkTarget: string | null;
}

export interface BranchGenerationExpectation {
  readonly reason:
    | "entry-changed"
    | "node-changed"
    | "source-changed"
    | "destination-changed"
    | "subtree-changed"
    | "ancestor-changed";
  readonly path: string;
  readonly expectedRevision: string | null;
  readonly expectedToken: string | null;
}

export interface BranchGenerationSnapshot {
  readonly filesystemId: string;
  readonly branchId: string;
  readonly baseRevision: string;
  readonly generation: number;
  readonly namespace: readonly {
    readonly path: string;
    readonly disposition: "present" | "tombstone";
    readonly inodeId: string | null;
  }[];
  readonly nodes: readonly BranchGenerationNode[];
  readonly expectations: readonly BranchGenerationExpectation[];
  readonly immutableReferences: readonly {
    readonly kind: "content" | "manifest";
    readonly digest: Uint8Array;
  }[];
}

function contentState(node: BranchGenerationNode): Uint8Array {
  if (node.kind === "directory")
    return sha256(encodeUtf8("efs-branch-directory-state-v1\0"));
  if (node.kind === "symlink")
    return sha256(
      concat([
        encodeUtf8("efs-branch-symlink-state-v1\0"),
        text(node.symlinkTarget ?? ""),
      ]),
    );
  const pages = [...node.pages].sort((left, right) => left.index - right.index);
  const patches = [...node.patches].sort((left, right) => left.order - right.order);
  const hash = new IncrementalSha256()
    .update(encodeUtf8("efs-branch-file-state-v1\0"))
    .update(u64(node.logicalSize))
    .update(optional(node.manifestHash))
    .update(u64(pages.length));
  for (const page of pages)
    hash
      .update(u64(page.index))
      .update(u32(page.bytes.byteLength))
      .update(sha256(page.bytes));
  hash.update(u64(patches.length));
  for (const patch of patches)
    hash
      .update(u64(patch.order))
      .update(u64(patch.offset))
      .update(u64(patch.deleteLength))
      .update(optional(patch.insertManifestDigest));
  return hash.digest();
}

/** Compute canonical `efs-branch-generation-digest-v1` without host encodings. */
export function computeBranchGenerationDigest(
  snapshot: BranchGenerationSnapshot,
): string {
  const namespaceRows = snapshot.namespace
    .map((row) => ({ row, order: encodeUtf8(row.path) }))
    .sort((left, right) => compareBytes(left.order, right.order))
    .map(({ row }) =>
      concat([
        text(row.path),
        Uint8Array.of(row.disposition === "present" ? 1 : 2),
        optional(row.inodeId === null ? null : text(row.inodeId)),
      ]),
    );
  const nodeRows = snapshot.nodes
    .map((node) => ({ node, order: encodeUtf8(node.inodeId) }))
    .sort((left, right) => compareBytes(left.order, right.order))
    .map(({ node }) =>
      concat([
        text(node.inodeId),
        Uint8Array.of(node.kind === "file" ? 1 : node.kind === "directory" ? 2 : 3),
        u32(node.mode),
        u64(node.birthtimeMs),
        u64(node.mtimeMs),
        u64(node.ctimeMs),
        u64(node.logicalSize),
        contentState(node),
      ]),
    );
  const reason = {
    "entry-changed": 1,
    "node-changed": 2,
    "source-changed": 3,
    "destination-changed": 4,
    "subtree-changed": 5,
    "ancestor-changed": 6,
  } as const;
  const expectationRows = snapshot.expectations
    .map((row) =>
      concat([
        Uint8Array.of(reason[row.reason]),
        text(row.path),
        optional(row.expectedRevision === null ? null : text(row.expectedRevision)),
        optional(row.expectedToken === null ? null : text(row.expectedToken)),
      ]),
    )
    .sort(compareBytes);
  const referenceRows = snapshot.immutableReferences
    .map((row) => {
      if (row.digest.byteLength !== 32)
        throw new RangeError("immutable reference digest must contain 32 bytes");
      return concat([Uint8Array.of(row.kind === "content" ? 1 : 2), row.digest]);
    })
    .sort(compareBytes);
  const digest = new IncrementalSha256()
    .update(encodeUtf8("efs-branch-generation-digest-v1\0"))
    .update(text(snapshot.filesystemId))
    .update(text(snapshot.branchId))
    .update(text(snapshot.baseRevision))
    .update(u64(snapshot.generation))
    .update(root("efs-branch-namespace-root-v1", namespaceRows))
    .update(root("efs-branch-node-root-v1", nodeRows))
    .update(root("efs-branch-expectation-root-v1", expectationRows))
    .update(root("efs-branch-reference-root-v1", referenceRows))
    .digest();
  return bytesToHex(digest);
}

/** Build the canonical manifest digest named by a structural-patch record. */
export function branchPatchInsertDigest(
  segments: readonly Uint8Array[],
): Uint8Array | null {
  if (segments.length === 0) return null;
  const bytes = concat(segments);
  return bytes.byteLength === 0 ? null : buildManifest(bytes, DEFAULT_FASTCDC).rootHash;
}
