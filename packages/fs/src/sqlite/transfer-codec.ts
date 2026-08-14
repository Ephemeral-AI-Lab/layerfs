import { encodeUtf8 } from "../namespace/utf8.js";

/**
 * Frozen semantic fragment grammars for `efs-replication-v1` state-transfer
 * phases. These grammars are normative for this implementation and MUST NOT
 * change without new golden vectors and a protocol version bump.
 *
 * All integers are unsigned big-endian. `text` is uint32 byte length
 * followed by exactly that many well-formed UTF-8 bytes. `bytes` is uint32
 * byte length followed by exactly that many bytes. `optional` is 0x00, or
 * 0x01 followed by the encoded value. `digest32` is 32 raw bytes.
 */

export interface TransferInodeRow {
  readonly inodeId: string;
  readonly tombstone: boolean;
  readonly encoded: Uint8Array | null;
}

export interface TransferEntryRow {
  readonly parentInode: string;
  readonly nameSort: Uint8Array;
  readonly tombstone: boolean;
  readonly encoded: Uint8Array | null;
}

export interface TransferManifestRefRow {
  readonly inodeId: string;
  readonly manifestHash: Uint8Array;
}

export type TransferNamespaceRow =
  | ({ readonly kind: 1 } & TransferInodeRow)
  | ({ readonly kind: 2 } & TransferEntryRow)
  | ({ readonly kind: 3 } & TransferManifestRefRow);

export interface TransferRevisionFragment {
  readonly revisionId: string;
  readonly parentRevisionId: string | null;
  readonly created_at_ms: number;
  readonly writerId: string;
  readonly changeCount: number;
  readonly rows: readonly TransferNamespaceRow[];
}

export interface TransferCheckpointFragment {
  readonly revisionId: string;
  readonly rows: readonly TransferNamespaceRow[];
}

export interface TransferBranchChangeRow {
  readonly path: Uint8Array;
  /** 0 for a present entry, 1 for a tombstone. */
  readonly disposition: number;
  readonly expectedToken: number | null;
  readonly encoded: Uint8Array | null;
}

export interface TransferBranchOverlayRow {
  readonly inodeId: string;
  readonly expectedToken: number | null;
  readonly encoded: Uint8Array;
}

export interface TransferBranchPageRow {
  readonly inodeId: string;
  readonly pageIndex: number;
  readonly generation: number;
  readonly bytes: Uint8Array;
  readonly created_at_ms: number;
  readonly head: boolean;
}

export interface TransferBranchPatchRow {
  readonly inodeId: string;
  readonly sequence: number;
  readonly generation: number;
  readonly offset: number;
  readonly deleteLength: number;
  readonly insertLength: number;
  readonly segments: readonly Uint8Array[];
}

export interface TransferBranchExpectationRow {
  readonly inodeId: string;
  readonly expectedToken: number | null;
}

export interface TransferBranchManifestRefRow {
  readonly path: Uint8Array;
  readonly manifestHash: Uint8Array;
}

export type TransferBranchRow =
  | ({ readonly kind: 1 } & TransferBranchChangeRow)
  | ({ readonly kind: 2 } & TransferBranchOverlayRow)
  | ({ readonly kind: 3 } & TransferBranchPageRow)
  | ({ readonly kind: 4 } & TransferBranchPatchRow)
  | ({ readonly kind: 5 } & TransferBranchExpectationRow)
  | ({ readonly kind: 6 } & TransferBranchManifestRefRow);

export interface TransferBranchGenerationFragment {
  readonly branchId: string;
  readonly baseRevision: string;
  readonly generation: number;
  readonly generationDigest: Uint8Array;
  /**
   * The exact digest held by the destination before this generation.  A
   * destination may advance a lower generation only when both values match.
   */
  readonly previousGeneration: number | null;
  readonly previousGenerationDigest: Uint8Array | null;
  readonly state: number;
  readonly rows: readonly TransferBranchRow[];
}

export interface TransferGenesisRow {
  readonly inodeId: string;
  readonly tombstone: boolean;
  readonly encoded: Uint8Array | null;
}

export interface TransferGenesisFragment {
  readonly filesystemId: string;
  readonly rootInode: string;
  readonly mainRevision: number;
  readonly rootMutationGeneration: number;
  readonly nextAllocationSequence: number;
  readonly cowPageBytes: number;
  readonly createdAtMs: number;
  readonly maxManifestEntries: number;
  readonly maxManifestDepth: number;
  readonly maxFileBytes: number;
  readonly writerProfile: string;
  readonly manifestFormat: string;
  readonly chunkerFormat: string;
  readonly fastCdcMinimum: number;
  readonly fastCdcAverage: number;
  readonly fastCdcMaximum: number;
  readonly rootInodeType: number;
  readonly rootMode: number;
  readonly rootBirthtimeMs: number;
  readonly rootMtimeMs: number;
  readonly rootCtimeMs: number;
  readonly rootToken: number;
  readonly rows: readonly TransferGenesisRow[];
}

export interface TransferActivationResult {
  readonly kind: 0 | 1;
  readonly revision: string;
  readonly branchId: string | null;
  readonly baseRevision: string | null;
  readonly generation: number;
  readonly generationDigest: Uint8Array | null;
  readonly state: 0 | 1 | 2;
  readonly authorityResult: TransferAuthorityResult | null;
}

export type TransferAuthorityResult =
  | {
      readonly kind: "publication";
      readonly operationId: string;
      readonly outcome: "merged" | "conflict";
      readonly resultDigest: Uint8Array;
    }
  | {
      readonly kind: "discard";
      readonly operationId: string | null;
      readonly resultDigest: Uint8Array;
    };

const encoder = new TextEncoder();

function bytes(value: Uint8Array): Uint8Array {
  return new Uint8Array(value);
}

function uint32(value: number, name: string): Uint8Array {
  if (!Number.isSafeInteger(value) || value < 0 || value > 0xffff_ffff)
    throw new RangeError(`${name} is outside the uint32 envelope`);
  const out = new Uint8Array(4);
  new DataView(out.buffer).setUint32(0, value, false);
  return out;
}

function uint64(value: number, name: string): Uint8Array {
  if (!Number.isSafeInteger(value) || value < 0)
    throw new RangeError(`${name} is outside the safe uint64 envelope`);
  const out = new Uint8Array(8);
  new DataView(out.buffer).setBigUint64(0, BigInt(value), false);
  return out;
}

function uint8(value: number): Uint8Array {
  return Uint8Array.of(value);
}

function digest32(value: Uint8Array): Uint8Array {
  if (value.byteLength !== 32) throw new RangeError("digest32 must contain 32 bytes");
  return bytes(value);
}

function text(value: string): Uint8Array {
  const encoded = encodeUtf8(value);
  const out = new Uint8Array(4 + encoded.byteLength);
  new DataView(out.buffer).setUint32(0, encoded.byteLength, false);
  out.set(encoded, 4);
  return out;
}

function byteValue(value: Uint8Array): Uint8Array {
  const out = new Uint8Array(4 + value.byteLength);
  new DataView(out.buffer).setUint32(0, value.byteLength, false);
  out.set(value, 4);
  return out;
}

function optional(encoded: Uint8Array | null): Uint8Array {
  if (encoded === null) return Uint8Array.of(0);
  const out = new Uint8Array(1 + encoded.byteLength);
  out[0] = 1;
  out.set(encoded, 1);
  return out;
}

function concat(parts: readonly Uint8Array[]): Uint8Array {
  let length = 0;
  for (const part of parts) length += part.byteLength;
  const out = new Uint8Array(length);
  let offset = 0;
  for (const part of parts) {
    out.set(part, offset);
    offset += part.byteLength;
  }
  return out;
}

function encodeNamespaceRow(row: TransferNamespaceRow): Uint8Array {
  if (row.kind === 1)
    return concat([
      uint8(1),
      text(row.inodeId),
      uint8(row.tombstone ? 1 : 0),
      byteValue(row.encoded ?? new Uint8Array(0)),
    ]);
  if (row.kind === 2)
    return concat([
      uint8(2),
      text(row.parentInode),
      byteValue(row.nameSort),
      uint8(row.tombstone ? 1 : 0),
      byteValue(row.encoded ?? new Uint8Array(0)),
    ]);
  return concat([uint8(3), text(row.inodeId), digest32(row.manifestHash)]);
}

function encodeBranchRow(row: TransferBranchRow): Uint8Array {
  if (row.kind === 1)
    return concat([
      uint8(1),
      uint8(row.disposition),
      byteValue(row.path),
      uint8(row.expectedToken === null ? 0 : 1),
      ...(row.expectedToken === null
        ? []
        : [uint64(row.expectedToken, "expected token")]),
      uint8(row.encoded === null ? 0 : 1),
      ...(row.encoded === null ? [] : [byteValue(row.encoded)]),
    ]);
  if (row.kind === 2)
    return concat([
      uint8(2),
      text(row.inodeId),
      uint8(row.expectedToken === null ? 0 : 1),
      ...(row.expectedToken === null
        ? []
        : [uint64(row.expectedToken, "expected token")]),
      byteValue(row.encoded),
    ]);
  if (row.kind === 3)
    return concat([
      uint8(3),
      text(row.inodeId),
      uint64(row.pageIndex, "page index"),
      uint64(row.generation, "page generation"),
      byteValue(row.bytes),
      uint64(row.created_at_ms, "page creation time"),
      uint8(row.head ? 1 : 0),
    ]);
  if (row.kind === 4)
    return concat([
      uint8(4),
      text(row.inodeId),
      uint64(row.sequence, "patch sequence"),
      uint64(row.generation, "patch generation"),
      uint64(row.offset, "patch offset"),
      uint64(row.deleteLength, "patch delete length"),
      uint64(row.insertLength, "patch insert length"),
      uint32(row.segments.length, "patch segment count"),
      ...row.segments.map((segment) => byteValue(segment)),
    ]);
  if (row.kind === 5)
    return concat([
      uint8(5),
      text(row.inodeId),
      uint8(row.expectedToken === null ? 0 : 1),
      ...(row.expectedToken === null
        ? []
        : [uint64(row.expectedToken, "expected token")]),
    ]);
  return concat([uint8(6), byteValue(row.path), digest32(row.manifestHash)]);
}

export function encodeRevisionFragment(
  fragment: TransferRevisionFragment,
): Uint8Array {
  return concat([
    uint8(1),
    text(fragment.revisionId),
    optional(
      fragment.parentRevisionId === null ? null : text(fragment.parentRevisionId),
    ),
    uint64(fragment.created_at_ms, "revision creation time"),
    text(fragment.writerId),
    uint64(fragment.changeCount, "revision change count"),
    uint32(fragment.rows.length, "revision row count"),
    ...fragment.rows.map((row) => encodeNamespaceRow(row)),
  ]);
}

export function encodeCheckpointFragment(
  fragment: TransferCheckpointFragment,
): Uint8Array {
  return concat([
    uint8(1),
    text(fragment.revisionId),
    uint32(fragment.rows.length, "checkpoint row count"),
    ...fragment.rows.map((row) => encodeNamespaceRow(row)),
  ]);
}

export function encodeBranchGenerationFragment(
  fragment: TransferBranchGenerationFragment,
): Uint8Array {
  if ((fragment.previousGeneration === null) !== (fragment.previousGenerationDigest === null))
    throw new RangeError("branch predecessor generation and digest must be present together");
  return concat([
    uint8(1),
    text(fragment.branchId),
    text(fragment.baseRevision),
    uint64(fragment.generation, "branch generation"),
    digest32(fragment.generationDigest),
    optional(
      fragment.previousGeneration === null
        ? null
        : uint64(fragment.previousGeneration, "branch predecessor generation"),
    ),
    optional(
      fragment.previousGenerationDigest === null
        ? null
        : digest32(fragment.previousGenerationDigest),
    ),
    uint8(fragment.state),
    uint32(fragment.rows.length, "branch row count"),
    ...fragment.rows.map((row) => encodeBranchRow(row)),
  ]);
}

export function encodeGenesisFragment(fragment: TransferGenesisFragment): Uint8Array {
  return concat([
    uint8(1),
    text(fragment.filesystemId),
    text(fragment.rootInode),
    uint64(fragment.mainRevision, "genesis main revision"),
    uint64(fragment.rootMutationGeneration, "genesis root generation"),
    uint64(fragment.nextAllocationSequence, "genesis allocation sequence"),
    uint32(fragment.cowPageBytes, "genesis page size"),
    uint64(fragment.createdAtMs, "genesis creation time"),
    uint32(fragment.maxManifestEntries, "genesis manifest entries"),
    uint32(fragment.maxManifestDepth, "genesis manifest depth"),
    uint64(fragment.maxFileBytes, "genesis max file bytes"),
    text(fragment.writerProfile),
    text(fragment.manifestFormat),
    text(fragment.chunkerFormat),
    uint32(fragment.fastCdcMinimum, "genesis fastcdc minimum"),
    uint32(fragment.fastCdcAverage, "genesis fastcdc average"),
    uint32(fragment.fastCdcMaximum, "genesis fastcdc maximum"),
    uint8(fragment.rootInodeType),
    uint32(fragment.rootMode, "genesis root mode"),
    uint64(fragment.rootBirthtimeMs, "genesis root birthtime"),
    uint64(fragment.rootMtimeMs, "genesis root mtime"),
    uint64(fragment.rootCtimeMs, "genesis root ctime"),
    uint64(fragment.rootToken, "genesis root token"),
    uint32(fragment.rows.length, "genesis row count"),
    ...fragment.rows.map((row) =>
      concat([
        text(row.inodeId),
        uint8(row.tombstone ? 1 : 0),
        byteValue(row.encoded ?? new Uint8Array(0)),
      ]),
    ),
  ]);
}

export function encodeActivationResult(result: TransferActivationResult): Uint8Array {
  const authority = result.authorityResult
    ? result.authorityResult.kind === "publication"
      ? concat([
          uint8(1),
          text(result.authorityResult.operationId),
          uint8(result.authorityResult.outcome === "merged" ? 0 : 1),
          digest32(result.authorityResult.resultDigest),
        ])
      : concat([
          uint8(2),
          optional(
            result.authorityResult.operationId === null
              ? null
              : text(result.authorityResult.operationId),
          ),
          digest32(result.authorityResult.resultDigest),
        ])
    : null;
  return concat([
    uint8(1),
    uint8(result.kind),
    text(result.revision),
    optional(result.branchId === null ? null : text(result.branchId)),
    optional(result.baseRevision === null ? null : text(result.baseRevision)),
    uint64(result.generation, "activation generation"),
    optional(
      result.generationDigest === null ? null : digest32(result.generationDigest),
    ),
    uint8(result.state),
    optional(authority),
  ]);
}

export function decodeActivationResult(value: Uint8Array): TransferActivationResult {
  const view = new Decoder(value);
  const version = view.uint8("activation version");
  if (version !== 1) throw new RangeError("activation version is not canonical");
  const kind = view.uint8("activation kind") as 0 | 1;
  const revision = view.text("activation revision");
  const branchId = view.optional(() => view.text("activation branch id"));
  const baseRevision = view.optional(() => view.text("activation base revision"));
  const generation = view.uint64("activation generation");
  const generationDigest = view.optional(() => view.digest("activation generation"));
  const state = view.uint8("activation state") as 0 | 1 | 2;
  if (state > 2) throw new RangeError("activation state is not canonical");
  const authorityTag = view.optional(() => view.uint8("authority result tag"));
  let authorityResult: TransferAuthorityResult | null = null;
  if (authorityTag === 1) {
    const operationId = view.text("publication operation id");
    const outcome = view.uint8("publication outcome");
    if (outcome > 1) throw new RangeError("publication outcome is not canonical");
    const resultDigest = view.digest("publication result digest");
    authorityResult = {
      kind: "publication",
      operationId,
      outcome: outcome === 0 ? "merged" : "conflict",
      resultDigest,
    };
  } else if (authorityTag === 2) {
    const operationId = view.optional(() => view.text("discard operation id"));
    const resultDigest = view.digest("discard result digest");
    authorityResult = { kind: "discard", operationId, resultDigest };
  } else if (authorityTag !== null) {
    throw new RangeError("authority result tag is not canonical");
  }
  if (view.remaining() !== 0)
    throw new RangeError("activation payload has trailing bytes");
  return {
    kind,
    revision,
    branchId,
    baseRevision,
    generation,
    generationDigest,
    state,
    authorityResult,
  };
}

class Decoder {
  readonly #value: Uint8Array;
  #offset = 0;
  constructor(value: Uint8Array) {
    this.#value = value;
  }
  remaining(): number {
    return this.#value.byteLength - this.#offset;
  }
  #take(length: number, name: string): Uint8Array {
    if (length < 0 || this.#offset + length > this.#value.byteLength)
      throw new RangeError(`truncated ${name}`);
    const out = this.#value.subarray(this.#offset, this.#offset + length);
    this.#offset += length;
    return out;
  }
  uint8(name: string): number {
    return this.#take(1, name)[0]!;
  }
  uint32(name: string): number {
    const bytes = this.#take(4, name);
    return new DataView(bytes.buffer, bytes.byteOffset, 4).getUint32(0, false);
  }
  uint64(name: string): number {
    const bytes = this.#take(8, name);
    const view = new DataView(bytes.buffer, bytes.byteOffset, 8);
    const value = view.getBigUint64(0, false);
    if (value > BigInt(Number.MAX_SAFE_INTEGER))
      throw new RangeError(`${name} exceeds the safe integer envelope`);
    return Number(value);
  }
  digest(name: string): Uint8Array {
    return this.#take(32, name);
  }
  bytes(name: string): Uint8Array {
    const length = this.uint32(name);
    return this.#take(length, `${name} bytes`);
  }
  bytesOrNull(name: string): Uint8Array | null {
    const length = this.uint32(name);
    if (length === 0) return null;
    return this.#take(length, `${name} bytes`);
  }
  text(name: string): string {
    const bytes = this.bytes(name);
    try {
      return new TextDecoder("utf-8", { fatal: true }).decode(bytes);
    } catch {
      throw new RangeError(`${name} is not well-formed UTF-8`);
    }
  }
  optional<T>(read: () => T): T | null {
    const tag = this.uint8("optional tag");
    if (tag === 0) return null;
    if (tag !== 1) throw new RangeError("optional tag is not canonical");
    return read();
  }
}

export interface TransferActivationRequest {
  readonly kind: 0 | 1 | 2;
  readonly expectedRevision: number;
  readonly expectedRootMutationGeneration: number;
  readonly expectedNextAllocationSequence: number;
  readonly expectedRootInode: string;
  readonly expectedRevisionCount: number;
  readonly expectedStateRows: number;
  readonly expectedClosureRoots: number;
  readonly expectedClosureNodes: number;
  readonly expectedClosureObjects: number;
  readonly expectedClosureObjectBytes: number;
  readonly checkpoint: boolean;
  readonly branchId: string | null;
  readonly baseRevision: string | null;
  readonly generation: number | null;
  readonly generationDigest: Uint8Array | null;
  readonly terminalState: 0 | 1 | 2;
  readonly terminalResultOperationId: string | null;
  readonly terminalResultBytes: Uint8Array | null;
  readonly genesis: TransferGenesisFragment | null;
}

export function encodeActivationRequest(
  request: TransferActivationRequest,
): Uint8Array {
  return concat([
    uint8(1),
    uint8(request.kind),
    uint64(request.expectedRevision, "activation revision"),
    uint64(request.expectedRootMutationGeneration, "activation root generation"),
    uint64(request.expectedNextAllocationSequence, "activation allocation sequence"),
    text(request.expectedRootInode),
    uint64(request.expectedRevisionCount, "activation revision count"),
    uint64(request.expectedStateRows, "activation state rows"),
    uint64(request.expectedClosureRoots, "activation closure roots"),
    uint64(request.expectedClosureNodes, "activation closure nodes"),
    uint64(request.expectedClosureObjects, "activation closure objects"),
    uint64(request.expectedClosureObjectBytes, "activation closure object bytes"),
    uint8(request.checkpoint ? 1 : 0),
    optional(request.branchId === null ? null : text(request.branchId)),
    optional(request.baseRevision === null ? null : text(request.baseRevision)),
    optional(request.generation === null ? null : uint64(request.generation, "activation generation")),
    optional(
      request.generationDigest === null ? null : digest32(request.generationDigest),
    ),
    uint8(request.terminalState),
    optional(
      request.terminalResultOperationId === null
        ? null
        : text(request.terminalResultOperationId),
    ),
    optional(
      request.terminalResultBytes === null
        ? null
        : byteValue(request.terminalResultBytes),
    ),
    optional(
      request.genesis === null ? null : byteValue(encodeGenesisFragment(request.genesis)),
    ),
  ]);
}

export function decodeActivationRequest(
  value: Uint8Array,
): TransferActivationRequest {
  const view = new Decoder(value);
  const version = view.uint8("activation version");
  if (version !== 1) throw new RangeError("activation version is not canonical");
  const kind = view.uint8("activation kind") as 0 | 1 | 2;
  if (kind > 2) throw new RangeError("activation kind is not canonical");
  const expectedRevision = view.uint64("activation revision");
  const expectedRootMutationGeneration = view.uint64("activation root generation");
  const expectedNextAllocationSequence = view.uint64("activation allocation sequence");
  const expectedRootInode = view.text("activation root inode");
  const expectedRevisionCount = view.uint64("activation revision count");
  const expectedStateRows = view.uint64("activation state rows");
  const expectedClosureRoots = view.uint64("activation closure roots");
  const expectedClosureNodes = view.uint64("activation closure nodes");
  const expectedClosureObjects = view.uint64("activation closure objects");
  const expectedClosureObjectBytes = view.uint64("activation closure object bytes");
  const checkpointByte = view.uint8("activation checkpoint");
  if (checkpointByte > 1) throw new RangeError("activation checkpoint is not canonical");
  const branchId = view.optional(() => view.text("activation branch id"));
  const baseRevision = view.optional(() => view.text("activation base revision"));
  const generation = view.optional(() => view.uint64("activation generation"));
  const generationDigest = view.optional(() => view.digest("activation generation"));
  const terminalState = view.uint8("activation terminal state") as 0 | 1 | 2;
  if (terminalState > 2) throw new RangeError("activation terminal state is not canonical");
  const terminalResultOperationId = view.optional(() =>
    view.text("activation terminal operation id"),
  );
  const terminalResultBytes = view.optional(() => view.bytes("activation terminal result"));
  const genesisBytes = view.optional(() => view.bytes("activation genesis"));
  if (view.remaining() !== 0)
    throw new RangeError("activation request has trailing bytes");
  return {
    kind,
    expectedRevision,
    expectedRootMutationGeneration,
    expectedNextAllocationSequence,
    expectedRootInode,
    expectedRevisionCount,
    expectedStateRows,
    expectedClosureRoots,
    expectedClosureNodes,
    expectedClosureObjects,
    expectedClosureObjectBytes,
    checkpoint: checkpointByte === 1,
    branchId,
    baseRevision,
    generation,
    generationDigest,
    terminalState,
    terminalResultOperationId,
    terminalResultBytes,
    genesis: genesisBytes === null ? null : decodeGenesisFragment(genesisBytes),
  };
}

function decodeGenesisFragment(bytes: Uint8Array): TransferGenesisFragment {
  const view = new Decoder(bytes);
  const version = view.uint8("genesis version");
  if (version !== 1) throw new RangeError("genesis version is not canonical");
  const filesystemId = view.text("genesis filesystem id");
  const rootInode = view.text("genesis root inode");
  const mainRevision = view.uint64("genesis main revision");
  const rootMutationGeneration = view.uint64("genesis root generation");
  const nextAllocationSequence = view.uint64("genesis allocation sequence");
  const cowPageBytes = view.uint32("genesis page size");
  const createdAtMs = view.uint64("genesis creation time");
  const maxManifestEntries = view.uint32("genesis manifest entries");
  const maxManifestDepth = view.uint32("genesis manifest depth");
  const maxFileBytes = view.uint64("genesis max file bytes");
  const writerProfile = view.text("genesis writer profile");
  const manifestFormat = view.text("genesis manifest format");
  const chunkerFormat = view.text("genesis chunker format");
  const fastCdcMinimum = view.uint32("genesis fastcdc minimum");
  const fastCdcAverage = view.uint32("genesis fastcdc average");
  const fastCdcMaximum = view.uint32("genesis fastcdc maximum");
  const rootInodeType = view.uint8("genesis root type");
  const rootMode = view.uint32("genesis root mode");
  const rootBirthtimeMs = view.uint64("genesis root birthtime");
  const rootMtimeMs = view.uint64("genesis root mtime");
  const rootCtimeMs = view.uint64("genesis root ctime");
  const rootToken = view.uint64("genesis root token");
  const rowCount = view.uint32("genesis row count");
  if (rowCount > 256) throw new RangeError("genesis row count exceeds the envelope");
  const rows: TransferGenesisRow[] = [];
  for (let index = 0; index < rowCount; index += 1) {
    const inodeId = view.text("genesis row inode");
    const tombstoneByte = view.uint8("genesis row tombstone");
    if (tombstoneByte > 1) throw new RangeError("genesis row tombstone is not canonical");
    const encoded = view.bytesOrNull("genesis row encoded");
    rows.push({ inodeId, tombstone: tombstoneByte === 1, encoded });
  }
  if (view.remaining() !== 0)
    throw new RangeError("genesis fragment has trailing bytes");
  return {
    filesystemId,
    rootInode,
    mainRevision,
    rootMutationGeneration,
    nextAllocationSequence,
    cowPageBytes,
    createdAtMs,
    maxManifestEntries,
    maxManifestDepth,
    maxFileBytes,
    writerProfile,
    manifestFormat,
    chunkerFormat,
    fastCdcMinimum,
    fastCdcAverage,
    fastCdcMaximum,
    rootInodeType,
    rootMode,
    rootBirthtimeMs,
    rootMtimeMs,
    rootCtimeMs,
    rootToken,
    rows,
  };
}

export const TRANSFER_FRAGMENT_VERSIONS = Object.freeze({
  revision: 1,
  checkpoint: 1,
  branchGeneration: 1,
  genesis: 1,
  activationResult: 1,
  activationRequest: 1,
} as const);
