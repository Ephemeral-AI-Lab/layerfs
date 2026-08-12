import {
  bytesToHex,
  copyBytes,
  equalBytes,
  intrinsicByteLength,
} from "../cas/bytes.js";
import { sha256, type HashFunction } from "../cas/sha256.js";
import type { ContentCache } from "../cache/content-cache.js";
import {
  decodeManifestNode,
  decodeManifestRoot,
  validateSupportedManifestParameters,
  type ManifestChild,
  type ManifestNode,
  type ManifestParameters,
} from "../manifests/codec.js";
import { validateCanonicalManifestNode } from "../manifests/cursor.js";
import {
  maxPersistedContentObjectBytes,
  type StorageLimits,
} from "../resources/limits.js";
import { checkedAdd } from "../resources/safe-integers.js";
import { encodeUtf8 } from "../namespace/utf8.js";
import type { FilesystemSQLiteTransaction, SqliteRow, SqliteValue } from "./driver.js";
import { ContentRepository } from "./content-repository.js";
import {
  applyChargedMetadata,
  CHARGED_ROW_BYTES,
  UsageRepository,
} from "./usage-repository.js";

export interface SQLiteManifestTreePathNode {
  readonly hash: Uint8Array;
  readonly path: readonly number[];
  readonly offset: number;
  readonly finalAtLevel: boolean;
  readonly node: ManifestNode;
  readonly selectedChildIndex?: number;
}

export interface SQLiteManifestTreePath {
  readonly manifestHash: Uint8Array;
  readonly parameters: ManifestParameters;
  readonly fileSize: number;
  readonly entryCount: number;
  readonly nodesRead: number;
  readonly nodes: readonly SQLiteManifestTreePathNode[];
  readonly leafOffset: number;
  readonly entryIndex: number;
  readonly entryOffset: number;
}

export interface ManifestSubtreeSummary {
  readonly objectCount: number;
  readonly objectBytes: number;
  readonly nodeCount: number;
  readonly nodeBytes: number;
  readonly membershipCount: number;
  readonly closureFold: Uint8Array;
  readonly chainDigest: Uint8Array;
  readonly objectBloom: Uint8Array;
  readonly nodeBloom: Uint8Array;
  readonly objectMembers: Uint8Array;
  readonly nodeMembers: Uint8Array;
}

/** Metadata produced while source-authenticated reused claims are registered. */
export interface ReusedSubtreeCacheMetadata {
  readonly nodeHash: Uint8Array;
  readonly sourceManifestHash: Uint8Array;
  readonly sourcePath: Uint8Array;
  readonly span: number;
  readonly entryCount: number;
  readonly validatedNonfinalLeafDelta: number | null;
  readonly validatedFinalLeafDelta: number | null;
  readonly summaryUsable: boolean;
  readonly summary?: ManifestSubtreeSummary;
}

export interface ReusedSubtreeCertificateState {
  readonly chainDigest: Uint8Array;
  readonly chainFold: Uint8Array;
  readonly objectCount: number;
  readonly objectBytes: number;
  readonly nodeCount: number;
  readonly nodeBytes: number;
  readonly membershipCount: number;
}

export const SUMMARY_BLOOM_BYTES = 1024;
const SUMMARY_CHAIN_SEED = sha256(encodeUtf8("efs-subtree-chain-v1"));

export function bloomAdd(bloom: Uint8Array, hash: Uint8Array): void {
  for (let index = 0; index < 4; index += 1) {
    const bit =
      ((hash[index * 2]! << 8) | hash[index * 2 + 1]!) % (bloom.byteLength * 8);
    const byteIndex = bit >>> 3;
    bloom[byteIndex] = (bloom[byteIndex] ?? 0) | (1 << (bit & 7));
  }
}

export function bloomMayOverlap(left: Uint8Array, right: Uint8Array): boolean {
  if (left.byteLength !== right.byteLength)
    throw new Error("ECORRUPT: subtree bloom size mismatch");
  for (let index = 0; index < left.byteLength; index += 1)
    if ((left[index]! & right[index]!) !== 0) return true;
  return false;
}

function memberHashKeys(encoded: Uint8Array): string[] {
  if (intrinsicByteLength(encoded) % 32 !== 0)
    throw new Error("ECORRUPT: subtree member hash list is misaligned");
  const keys: string[] = [];
  for (let offset = 0; offset < encoded.byteLength; offset += 32)
    keys.push(bytesToHex(encoded.subarray(offset, offset + 32)));
  return keys;
}

function memberHashMap(
  encoded: Uint8Array,
): Map<string, { readonly hash: Uint8Array }> {
  const members = new Map<string, { readonly hash: Uint8Array }>();
  if (intrinsicByteLength(encoded) % 32 !== 0)
    throw new Error("ECORRUPT: subtree member hash list is misaligned");
  for (let offset = 0; offset < encoded.byteLength; offset += 32) {
    const hash = copyBytes(encoded.subarray(offset, offset + 32));
    members.set(bytesToHex(hash), Object.freeze({ hash }));
  }
  return members;
}

function mergeBloom(target: Uint8Array, source: Uint8Array): void {
  if (target.byteLength !== source.byteLength)
    throw new Error("ECORRUPT: subtree bloom size mismatch");
  for (let index = 0; index < target.byteLength; index += 1)
    target[index] = target[index]! | source[index]!;
}

function foldHash(fold: Uint8Array, hash: Uint8Array): Uint8Array {
  const out = copyBytes(fold);
  for (let index = 0; index < 32; index += 1) out[index]! ^= hash[index]!;
  return out;
}

function extendSummaryChain(
  previous: Uint8Array,
  kind: 0 | 1,
  hash: Uint8Array,
  size: number,
): Uint8Array {
  const encoded = new Uint8Array(49);
  const view = new DataView(encoded.buffer);
  encoded[0] = kind;
  encoded.set(hash, 1);
  view.setBigUint64(33, BigInt(size), true);
  return sha256(new Uint8Array([...previous, ...encoded]));
}

function extendSummaryCertificateChain(
  previous: Uint8Array,
  sequence: number,
  summary: ManifestSubtreeSummary,
): Uint8Array {
  const encoded = new Uint8Array(49);
  const view = new DataView(encoded.buffer);
  encoded[0] = 3;
  encoded.set(summary.chainDigest, 1);
  view.setBigUint64(33, BigInt(sequence), true);
  view.setBigUint64(41, BigInt(summary.membershipCount), true);
  return sha256(new Uint8Array([...previous, ...encoded]));
}

interface SummaryRow extends SqliteRow {
  node_hash?: Uint8Array;
  object_count: number;
  object_bytes: number;
  node_count: number;
  node_bytes: number;
  membership_count: number;
  closure_fold: Uint8Array;
  chain_digest: Uint8Array;
  object_bloom: Uint8Array;
  node_bloom: Uint8Array;
  object_members: Uint8Array;
  node_members: Uint8Array;
}

interface AuthenticatedNodePath {
  readonly hash: Uint8Array;
  readonly node: ManifestNode;
  readonly depth: number;
  readonly treeDepth: number;
  readonly finalAtLevel: boolean;
}

const authenticatedPathCaches = new WeakMap<
  FilesystemSQLiteTransaction,
  Map<string, AuthenticatedNodePath>
>();
/**
 * Summary rows preloaded while constructing fresh local nodes. Repository
 * instances are short-lived per call, but the transaction is shared; retain
 * this bounded handoff so reused-claim registration does not immediately
 * reread the same immutable summaries.
 */
const transactionSummaryCaches = new WeakMap<
  FilesystemSQLiteTransaction,
  Map<string, ManifestSubtreeSummary | undefined>
>();

function encodeMemberHashes(
  members: ReadonlyMap<string, { readonly hash: Uint8Array }>,
): Uint8Array {
  const output = new Uint8Array(members.size * 32);
  let offset = 0;
  for (const member of members.values()) {
    output.set(member.hash, offset);
    offset += 32;
  }
  return output;
}

function decodeSubtreeSummaryRow(row: SummaryRow): ManifestSubtreeSummary {
  if (
    !Number.isSafeInteger(row.object_count) ||
    !Number.isSafeInteger(row.object_bytes) ||
    !Number.isSafeInteger(row.node_count) ||
    !Number.isSafeInteger(row.node_bytes) ||
    !Number.isSafeInteger(row.membership_count) ||
    row.node_count < 0 ||
    row.membership_count < 0 ||
    row.object_count + row.node_count !== row.membership_count ||
    row.object_count > Number.MAX_SAFE_INTEGER / 32 ||
    row.node_count > Number.MAX_SAFE_INTEGER / 32 ||
    intrinsicByteLength(row.closure_fold) !== 32 ||
    intrinsicByteLength(row.chain_digest) !== 32 ||
    intrinsicByteLength(row.object_bloom) !== SUMMARY_BLOOM_BYTES ||
    intrinsicByteLength(row.node_bloom) !== SUMMARY_BLOOM_BYTES ||
    intrinsicByteLength(row.object_members) !== row.object_count * 32 ||
    intrinsicByteLength(row.node_members) !== row.node_count * 32
  )
    throw new Error("ECORRUPT: invalid manifest subtree summary");
  return Object.freeze({
    objectCount: row.object_count,
    objectBytes: row.object_bytes,
    nodeCount: row.node_count,
    nodeBytes: row.node_bytes,
    membershipCount: row.membership_count,
    closureFold: copyBytes(row.closure_fold),
    chainDigest: copyBytes(row.chain_digest),
    objectBloom: copyBytes(row.object_bloom),
    nodeBloom: copyBytes(row.node_bloom),
    objectMembers: copyBytes(row.object_members),
    nodeMembers: copyBytes(row.node_members),
  });
}

interface LeaseOwnerRow extends SqliteRow {
  owner_nonce: Uint8Array;
  state: number;
  sealed: number;
}

function snapshotNode(node: ManifestNode): ManifestNode {
  return node.kind === "leaf"
    ? Object.freeze({
        kind: "leaf" as const,
        span: node.span,
        entryCount: node.entryCount,
        entries: Object.freeze(
          node.entries.map((entry) =>
            Object.freeze({ hash: copyBytes(entry.hash), length: entry.length }),
          ),
        ),
      })
    : Object.freeze({
        kind: "internal" as const,
        span: node.span,
        entryCount: node.entryCount,
        children: Object.freeze(
          node.children.map((child) =>
            Object.freeze({
              hash: copyBytes(child.hash),
              span: child.span,
              entryCount: child.entryCount,
            }),
          ),
        ),
      });
}

export class ManifestTreeRepository {
  readonly #tx: FilesystemSQLiteTransaction;
  readonly #limits: StorageLimits;
  readonly #content: ContentRepository;

  constructor(
    tx: FilesystemSQLiteTransaction,
    limits: StorageLimits,
    cache?: ContentCache,
    hashBytes: HashFunction = sha256,
  ) {
    this.#tx = tx;
    this.#limits = limits;
    this.#content = new ContentRepository(tx, limits, cache, hashBytes);
  }

  pathAtOffset(manifestHash: Uint8Array, offset: number): SQLiteManifestTreePath {
    if (intrinsicByteLength(manifestHash) !== 32)
      throw new RangeError("manifest hash must contain exactly 32 bytes");
    manifestHash = copyBytes(manifestHash);
    if (!Number.isSafeInteger(offset) || offset < 0)
      throw new RangeError("manifest tree offset must be a nonnegative safe integer");
    const validatedDepth = this.#content.validatedManifestDepth(manifestHash);
    if (validatedDepth === undefined)
      throw new Error("ECORRUPT: manifest lacks a durable validation certificate");
    const root = this.#content.withManifestRoot(manifestHash, (rootBytes) =>
      decodeManifestRoot(rootBytes, manifestHash),
    );
    if (!root) throw new Error("ECORRUPT: missing manifest root");
    validateSupportedManifestParameters(root.parameters);
    if (root.parameters.maximum > maxPersistedContentObjectBytes(this.#limits))
      throw new RangeError(
        "manifest FastCDC maximum exceeds the durable object transaction envelope",
      );
    if (offset > root.fileSize)
      throw new RangeError("manifest tree offset is outside the file");
    const selectedOffset =
      root.fileSize === 0 ? 0 : Math.min(offset, root.fileSize - 1);
    const nodes: SQLiteManifestTreePathNode[] = [];
    let path: number[] = [];
    let nodeOffset = 0;
    let remaining = selectedOffset;
    let finalAtLevel = true;
    let expected: ManifestChild | undefined;
    let hash = copyBytes(root.rootNodeHash);
    for (let depth = 1; ; depth += 1) {
      if (depth > this.#limits.maxManifestDepth)
        throw new Error("ECORRUPT: manifest depth exceeds configured maximum");
      const decoded = this.#content.withManifestNode(hash, (encoded) =>
        decodeManifestNode(encoded, hash),
      );
      if (!decoded) throw new Error("ECORRUPT: missing manifest node");
      if (
        expected &&
        (decoded.span !== expected.span || decoded.entryCount !== expected.entryCount)
      )
        throw new Error("ECORRUPT: manifest child totals mismatch");
      validateCanonicalManifestNode(
        decoded,
        root.parameters,
        finalAtLevel,
        depth === 1,
      );
      if (depth === 1) {
        if (
          decoded.span !== root.fileSize ||
          decoded.entryCount !== root.entryCount ||
          (root.fileSize === 0) !== (root.entryCount === 0)
        )
          throw new Error("ECORRUPT: manifest root totals mismatch");
      }
      if (decoded.kind === "leaf") {
        if (depth !== validatedDepth)
          throw new Error("ECORRUPT: manifest validation depth mismatch");
        let entryOffset = nodeOffset;
        let entryIndex = 0;
        if (root.fileSize !== 0) {
          let relative = remaining;
          entryIndex = -1;
          for (let index = 0; index < decoded.entries.length; index += 1) {
            const entry = decoded.entries[index]!;
            if (relative < entry.length) {
              entryIndex = index;
              break;
            }
            relative -= entry.length;
            entryOffset = checkedAdd(entryOffset, entry.length);
          }
          if (entryIndex < 0)
            throw new Error("ECORRUPT: leaf span does not contain requested offset");
        }
        nodes.push(
          Object.freeze({
            hash: copyBytes(hash),
            path: Object.freeze([...path]),
            offset: nodeOffset,
            finalAtLevel,
            node: snapshotNode(decoded),
          }),
        );
        return Object.freeze({
          manifestHash: copyBytes(manifestHash),
          parameters: Object.freeze({ ...root.parameters }),
          fileSize: root.fileSize,
          entryCount: root.entryCount,
          nodesRead: nodes.length,
          nodes: Object.freeze(nodes),
          leafOffset: nodeOffset,
          entryIndex,
          entryOffset,
        });
      }
      let childOffset = nodeOffset;
      let selected = -1;
      for (let index = 0; index < decoded.children.length; index += 1) {
        const child = decoded.children[index]!;
        if (remaining < child.span) {
          selected = index;
          break;
        }
        remaining -= child.span;
        childOffset = checkedAdd(childOffset, child.span);
      }
      if (selected < 0)
        throw new Error("ECORRUPT: internal span does not contain requested offset");
      nodes.push(
        Object.freeze({
          hash: copyBytes(hash),
          path: Object.freeze([...path]),
          offset: nodeOffset,
          finalAtLevel,
          node: snapshotNode(decoded),
          selectedChildIndex: selected,
        }),
      );
      expected = decoded.children[selected]!;
      hash = copyBytes(expected.hash);
      finalAtLevel = finalAtLevel && selected === decoded.children.length - 1;
      nodeOffset = childOffset;
      path = [...path, selected];
    }
  }

  protectSourceManifest(
    leaseId: string,
    ownerNonce: Uint8Array,
    manifestHash: Uint8Array,
  ): void {
    this.#activeOwner(leaseId, ownerNonce);
    if (!this.#content.withManifestRoot(manifestHash, () => true))
      throw new Error("ECORRUPT: source manifest root is missing");
    const inserted = this.#tx.run(
      "INSERT OR IGNORE INTO efs_lease_manifests(lease_id,manifest_hash) VALUES(?,?)",
      [leaseId, manifestHash],
    );
    if (inserted.changes)
      applyChargedMetadata(
        this.#tx,
        this.#limits,
        CHARGED_ROW_BYTES,
        "path-copy source root link",
      );
  }

  #cachedSubtreeSummary(nodeHash: Uint8Array): ManifestSubtreeSummary | undefined {
    const key = bytesToHex(nodeHash);
    const cached = transactionSummaryCaches.get(this.#tx);
    if (cached?.has(key)) return cached.get(key);
    const row = this.#tx.all<SummaryRow>(
      "SELECT object_count,object_bytes,node_count,node_bytes,membership_count,closure_fold,chain_digest,object_bloom,node_bloom,object_members,node_members FROM efs_manifest_subtree_summaries WHERE node_hash=?",
      [nodeHash],
      { maxRows: 1, maxBytes: this.#limits.maxFinalTransactionBytes },
    )[0];
    const summary = row ? decodeSubtreeSummaryRow(row) : undefined;
    let summaries = cached;
    if (!summaries) {
      summaries = new Map();
      transactionSummaryCaches.set(this.#tx, summaries);
    }
    summaries.set(key, summary);
    return summary;
  }

  /**
   * Preload immutable summaries for a trusted local-rebuild batch. Missing
   * rows are cached as undefined so registration retains the generic safe
   * no-summary path without repeating one SELECT per claim.
   */
  preloadSubtreeSummaries(nodeHashes: readonly Uint8Array[]): void {
    const unique = [
      ...new Map(nodeHashes.map((hash) => [bytesToHex(hash), hash])).values(),
    ];
    if (unique.length === 0) return;
    if (unique.some((hash) => intrinsicByteLength(hash) !== 32))
      throw new RangeError("manifest summary hash must contain exactly 32 bytes");
    let summaries = transactionSummaryCaches.get(this.#tx);
    if (!summaries) {
      summaries = new Map();
      transactionSummaryCaches.set(this.#tx, summaries);
    }
    const uncached = unique.filter((hash) => !summaries!.has(bytesToHex(hash)));
    const batchSize = Math.max(1, Math.min(this.#limits.maxQueryBatchSize, 8));
    for (let start = 0; start < uncached.length; start += batchSize) {
      const batch = uncached.slice(start, start + batchSize);
      const placeholders = batch.map(() => "?").join(",");
      const rows = this.#tx.all<SummaryRow>(
        `SELECT node_hash,object_count,object_bytes,node_count,node_bytes,membership_count,closure_fold,chain_digest,object_bloom,node_bloom,object_members,node_members FROM efs_manifest_subtree_summaries WHERE node_hash IN (${placeholders})`,
        batch,
        { maxRows: batch.length + 1, maxBytes: this.#limits.maxFinalTransactionBytes },
      );
      const byHash = new Map(
        rows.map((row) => [bytesToHex(row.node_hash!), decodeSubtreeSummaryRow(row)]),
      );
      for (const hash of batch)
        summaries.set(bytesToHex(hash), byHash.get(bytesToHex(hash)));
    }
  }

  /** Trusted local-edit source-link handoff using already authenticated root bytes. */
  protectTrustedSourceManifest(
    leaseId: string,
    ownerNonce: Uint8Array,
    manifestHash: Uint8Array,
    rootBytes: Uint8Array,
  ): void {
    // decodeManifestRoot verifies the supplied bytes against the authenticated
    // source hash; the durable INSERT below remains the protection boundary.
    decodeManifestRoot(rootBytes, manifestHash);
    const inserted = this.#tx.run(
      "INSERT OR IGNORE INTO efs_lease_manifests(lease_id,manifest_hash) VALUES(?,?)",
      [leaseId, manifestHash],
    );
    if (inserted.changes)
      applyChargedMetadata(
        this.#tx,
        this.#limits,
        CHARGED_ROW_BYTES,
        "path-copy source root link",
      );
    // The lease owner was inserted by the same trusted persistence step. Keep
    // the parameter in the contract to make accidental cross-lease use hard.
    if (intrinsicByteLength(ownerNonce) !== 16)
      throw new RangeError("staging owner nonce must contain 16 bytes");
  }

  /**
   * Records bounded, authenticated summaries for newly written manifest nodes.
   * The input is normally one manifest level at a time, but the recursive
   * lookup also handles a local-rebuild spine whose parents precede children.
   * A summary is optional: if exact member sets would exceed the existing
   * transaction envelope, or if sibling subtrees share a member hash, the row
   * is omitted and reconciliation retains its full-walk path.
   */
  recordSubtreeSummaries(
    nodes: readonly { readonly hash: Uint8Array; readonly encoded: Uint8Array }[],
  ): void {
    if (nodes.length === 0) return;
    const byHash = new Map<
      string,
      {
        readonly hash: Uint8Array;
        readonly encoded: Uint8Array;
        readonly node: ManifestNode;
      }
    >();
    for (const item of nodes) {
      if (intrinsicByteLength(item.hash) !== 32)
        throw new RangeError("manifest summary node hash must contain 32 bytes");
      const encoded = copyBytes(item.encoded);
      const hash = copyBytes(item.hash);
      const node = decodeManifestNode(encoded, hash);
      byHash.set(bytesToHex(hash), Object.freeze({ hash, encoded, node }));
    }
    // A local rebuild writes a short spine whose new nodes commonly point at
    // several already-durable children. Fetch those immutable summaries in
    // bounded batches so summary construction does not reopen one SQLite read
    // for every child edge. A missing row is retained in the preloaded set and
    // therefore falls through to the existing safe no-summary path.
    const referenced = new Map<string, Uint8Array>();
    for (const value of byHash.values()) {
      if (value.node.kind !== "internal") continue;
      for (const child of value.node.children)
        if (!byHash.has(bytesToHex(child.hash)))
          referenced.set(bytesToHex(child.hash), copyBytes(child.hash));
    }
    const preloadedSummaryKeys = new Set<string>();
    const preloadedSummaries = new Map<string, ManifestSubtreeSummary | undefined>();
    const preloadedNodeSizes = new Map<string, number>();
    const referencedValues = [...referenced.values()];
    const lookupBatchSize = Math.max(1, Math.min(this.#limits.maxQueryBatchSize, 8));
    for (let start = 0; start < referencedValues.length; start += lookupBatchSize) {
      const batch = referencedValues.slice(start, start + lookupBatchSize);
      const placeholders = batch.map(() => "?").join(",");
      const rows = this.#tx.all<
        SummaryRow & {
          readonly node_size: number | null;
          readonly object_count: number | null;
          readonly object_bytes: number | null;
          readonly node_count: number | null;
          readonly node_bytes: number | null;
          readonly membership_count: number | null;
          readonly closure_fold: Uint8Array | null;
          readonly chain_digest: Uint8Array | null;
          readonly object_bloom: Uint8Array | null;
          readonly node_bloom: Uint8Array | null;
          readonly object_members: Uint8Array | null;
          readonly node_members: Uint8Array | null;
        }
      >(
        `SELECT n.hash node_hash,length(n.encoded) node_size,s.object_count,s.object_bytes,s.node_count,s.node_bytes,s.membership_count,s.closure_fold,s.chain_digest,s.object_bloom,s.node_bloom,s.object_members,s.node_members FROM efs_manifest_nodes n LEFT JOIN efs_manifest_subtree_summaries s ON s.node_hash=n.hash WHERE n.hash IN (${placeholders})`,
        batch,
        {
          maxRows: batch.length + 1,
          maxBytes: this.#limits.maxFinalTransactionBytes,
        },
      );
      for (const hash of batch) preloadedSummaryKeys.add(bytesToHex(hash));
      for (const row of rows) {
        if (!row.node_hash) continue;
        const key = bytesToHex(row.node_hash);
        if (row.node_size !== null) preloadedNodeSizes.set(key, row.node_size);
        if (
          row.object_count !== null &&
          row.object_bytes !== null &&
          row.node_count !== null &&
          row.node_bytes !== null &&
          row.membership_count !== null &&
          row.closure_fold !== null &&
          row.chain_digest !== null &&
          row.object_bloom !== null &&
          row.node_bloom !== null &&
          row.object_members !== null &&
          row.node_members !== null
        )
          preloadedSummaries.set(key, decodeSubtreeSummaryRow(row as SummaryRow));
      }
    }
    let transactionSummaries = transactionSummaryCaches.get(this.#tx);
    if (!transactionSummaries) {
      transactionSummaries = new Map();
      transactionSummaryCaches.set(this.#tx, transactionSummaries);
    }
    for (const key of preloadedSummaryKeys)
      transactionSummaries.set(key, preloadedSummaries.get(key));
    const memo = new Map<string, ManifestSubtreeSummary | null>();
    const pendingSummaries = new Map<
      string,
      { readonly hash: Uint8Array; readonly summary: ManifestSubtreeSummary }
    >();
    const maxSummaryBytes = Math.max(
      0,
      this.#limits.maxFinalTransactionBytes - SUMMARY_BLOOM_BYTES * 2 - 512,
    );
    const summaryFor = (hash: Uint8Array): ManifestSubtreeSummary | undefined => {
      const key = bytesToHex(hash);
      const memoized = memo.get(key);
      if (memoized !== undefined) return memoized ?? undefined;
      const cached = preloadedSummaryKeys.has(key)
        ? preloadedSummaries.get(key)
        : this.#cachedSubtreeSummary(hash);
      if (cached) {
        memo.set(key, cached);
        return cached;
      }
      const value = byHash.get(key);
      if (!value) {
        memo.set(key, null);
        return undefined;
      }
      const objectMembers = new Map<string, { readonly hash: Uint8Array }>();
      const nodeMembers = new Map<string, { readonly hash: Uint8Array }>();
      let objectBytes = 0;
      let nodeBytes = 0;
      let closureFold: Uint8Array = new Uint8Array(32);
      let chainDigest: Uint8Array = copyBytes(SUMMARY_CHAIN_SEED);
      let sequence = 0;
      const objectBloom = new Uint8Array(SUMMARY_BLOOM_BYTES);
      const nodeBloom = new Uint8Array(SUMMARY_BLOOM_BYTES);
      const addMember = (
        target: Map<string, { readonly hash: Uint8Array }>,
        memberHash: Uint8Array,
      ): "added" | "duplicate" | "too-large" => {
        const memberKey = bytesToHex(memberHash);
        if (target.has(memberKey)) return "duplicate";
        const memberBytes =
          checkedAdd(
            checkedAdd(objectMembers.size, nodeMembers.size, "summary member count"),
            1,
            "summary member count",
          ) * 32;
        if (memberBytes > maxSummaryBytes) return "too-large";
        target.set(memberKey, Object.freeze({ hash: copyBytes(memberHash) }));
        return "added";
      };
      const addFold = (hashValue: Uint8Array): void => {
        closureFold = foldHash(closureFold, hashValue);
      };
      if (value.node.kind === "leaf") {
        for (const entry of value.node.entries) {
          if (objectMembers.has(bytesToHex(entry.hash))) continue;
          if (addMember(objectMembers, entry.hash) !== "added") {
            memo.set(key, null);
            return undefined;
          }
          objectBytes = checkedAdd(objectBytes, entry.length, "summary object bytes");
          bloomAdd(objectBloom, entry.hash);
          addFold(entry.hash);
          chainDigest = extendSummaryChain(chainDigest, 0, entry.hash, entry.length);
          sequence = checkedAdd(sequence, 1, "summary sequence");
        }
      } else {
        for (const child of value.node.children) {
          const childKey = bytesToHex(child.hash);
          const childSummary = summaryFor(child.hash);
          if (!childSummary) {
            memo.set(key, null);
            return undefined;
          }
          // A canonical DAG may reference the same child more than once. Its
          // closure is counted once, so duplicate edges reuse the first
          // child's authenticated summary instead of making the whole parent
          // summary unusable.
          if (nodeMembers.has(childKey)) continue;
          if (addMember(nodeMembers, child.hash) !== "added") {
            memo.set(key, null);
            return undefined;
          }
          const childValue = byHash.get(childKey);
          const childSize = childValue
            ? intrinsicByteLength(childValue.encoded)
            : (preloadedNodeSizes.get(childKey) ??
              this.#content.withManifestNode(child.hash, (encoded) =>
                intrinsicByteLength(encoded),
              ));
          if (childSize === undefined) {
            memo.set(key, null);
            return undefined;
          }
          nodeBytes = checkedAdd(nodeBytes, childSize, "summary node bytes");
          bloomAdd(nodeBloom, child.hash);
          addFold(child.hash);
          chainDigest = extendSummaryChain(chainDigest, 1, child.hash, childSize);
          sequence = checkedAdd(sequence, 1, "summary sequence");
          const childObjects = memberHashMap(childSummary.objectMembers);
          const childNodes = memberHashMap(childSummary.nodeMembers);
          for (const member of childObjects.values()) {
            if (addMember(objectMembers, member.hash) !== "added") {
              memo.set(key, null);
              return undefined;
            }
          }
          for (const member of childNodes.values()) {
            if (addMember(nodeMembers, member.hash) !== "added") {
              memo.set(key, null);
              return undefined;
            }
          }
          objectBytes = checkedAdd(
            objectBytes,
            childSummary.objectBytes,
            "summary object bytes",
          );
          nodeBytes = checkedAdd(
            nodeBytes,
            childSummary.nodeBytes,
            "summary node bytes",
          );
          mergeBloom(objectBloom, childSummary.objectBloom);
          mergeBloom(nodeBloom, childSummary.nodeBloom);
          closureFold = foldHash(closureFold, childSummary.closureFold);
          chainDigest = extendSummaryCertificateChain(
            chainDigest,
            sequence,
            childSummary,
          );
          sequence = checkedAdd(
            sequence,
            childSummary.membershipCount,
            "summary sequence",
          );
        }
        const summary = Object.freeze({
          objectCount: objectMembers.size,
          objectBytes,
          nodeCount: nodeMembers.size,
          nodeBytes,
          membershipCount: objectMembers.size + nodeMembers.size,
          closureFold,
          chainDigest,
          objectBloom,
          nodeBloom,
          objectMembers: encodeMemberHashes(objectMembers),
          nodeMembers: encodeMemberHashes(nodeMembers),
        });
        memo.set(key, summary);
        pendingSummaries.set(key, Object.freeze({ hash: copyBytes(hash), summary }));
        return summary;
      }
      for (const member of objectMembers.values()) bloomAdd(objectBloom, member.hash);
      const summary = Object.freeze({
        objectCount: objectMembers.size,
        objectBytes,
        nodeCount: 0,
        nodeBytes: 0,
        membershipCount: objectMembers.size,
        closureFold,
        chainDigest,
        objectBloom,
        nodeBloom: new Uint8Array(SUMMARY_BLOOM_BYTES),
        objectMembers: encodeMemberHashes(objectMembers),
        nodeMembers: new Uint8Array(0),
      });
      memo.set(key, summary);
      pendingSummaries.set(key, Object.freeze({ hash: copyBytes(hash), summary }));
      return summary;
    };
    for (const node of byHash.values()) summaryFor(node.hash);
    this.#insertSubtreeSummaries(pendingSummaries);
  }

  #insertSubtreeSummaries(
    summaries: ReadonlyMap<
      string,
      { readonly hash: Uint8Array; readonly summary: ManifestSubtreeSummary }
    >,
  ): void {
    const values = [...summaries.entries()];
    for (
      let start = 0;
      start < values.length;
      start += this.#limits.maxQueryBatchSize
    ) {
      const batch = values.slice(start, start + this.#limits.maxQueryBatchSize);
      if (!batch.length) continue;
      const placeholders = batch.map(() => "(?,?,?,?,?,?,?,?,?,?,?,?)").join(",");
      const bindings: SqliteValue[] = [];
      for (const [, value] of batch) {
        bindings.push(
          value.hash,
          value.summary.objectCount,
          value.summary.objectBytes,
          value.summary.nodeCount,
          value.summary.nodeBytes,
          value.summary.membershipCount,
          value.summary.closureFold,
          value.summary.chainDigest,
          value.summary.objectBloom,
          value.summary.nodeBloom,
          value.summary.objectMembers,
          value.summary.nodeMembers,
        );
      }
      const inserted = this.#tx.run(
        `INSERT OR IGNORE INTO efs_manifest_subtree_summaries(node_hash,object_count,object_bytes,node_count,node_bytes,membership_count,closure_fold,chain_digest,object_bloom,node_bloom,object_members,node_members) VALUES ${placeholders}`,
        bindings,
      );
      if (inserted.changes)
        applyChargedMetadata(
          this.#tx,
          this.#limits,
          inserted.changes * CHARGED_ROW_BYTES,
          "manifest subtree summaries",
        );
    }
  }

  registerReusedSubtrees(
    leaseId: string,
    ownerNonce: Uint8Array,
    sourceManifestHash: Uint8Array,
    claims: readonly {
      readonly sourcePath: readonly number[];
      readonly nodeHash: Uint8Array;
      readonly span: number;
      readonly entryCount: number;
    }[],
    options: {
      readonly knownObjectHashes?: readonly Uint8Array[];
      readonly knownNodeHashes?: readonly Uint8Array[];
      /** The same transaction already called protectSourceManifest. */
      readonly sourceManifestProtected?: boolean;
      /** Disable summary aggregation when overlap state cannot span batches. */
      readonly allowSummaries?: boolean;
      readonly certificateState?: ReusedSubtreeCertificateState;
      readonly deferCertificateWrite?: boolean;
      readonly certificatePatch?: { value?: ReusedSubtreeCertificateState };
      readonly authenticatedClaims?: readonly {
        readonly sourcePath: readonly number[];
        readonly nodeHash: Uint8Array;
        readonly span: number;
        readonly entryCount: number;
        readonly sourceFinalAtLevel: boolean;
        readonly sourceLeafDelta: number;
      }[];
    } = {},
  ): readonly ReusedSubtreeCacheMetadata[] {
    // The local durable path sets sourceManifestProtected only immediately
    // after protectSourceManifest() authenticated this owner in the same
    // transaction. Generic callers retain the active-owner query.
    if (!options.sourceManifestProtected) this.#activeOwner(leaseId, ownerNonce);
    if (claims.length > this.#limits.maxManifestDepth * 128)
      throw new RangeError("reused subtree claim batch exceeds bounded tree fanout");
    const linked = options.sourceManifestProtected
      ? 1
      : this.#tx.all(
          "SELECT 1 present FROM efs_lease_manifests WHERE lease_id=? AND manifest_hash=?",
          [leaseId, sourceManifestHash],
          { maxRows: 1, maxBytes: 128 },
        ).length;
    if (!linked) throw new Error("ECORRUPT: reused subtree lacks its source root link");
    const seenObjectHashes = new Set<string>();
    const seenNodeHashes = new Set<string>();
    const summariesByHash = new Map<string, ManifestSubtreeSummary>();
    const authenticatedClaims = new Map(
      (options.authenticatedClaims ?? []).map((claim) => [
        bytesToHex(claim.nodeHash),
        claim,
      ]),
    );
    if (
      options.authenticatedClaims &&
      authenticatedClaims.size !== options.authenticatedClaims.length
    )
      throw new Error("ECORRUPT: duplicate authenticated reused-subtree proof");
    const allClaimsAuthenticated =
      options.authenticatedClaims !== undefined &&
      claims.every((claim) => authenticatedClaims.has(bytesToHex(claim.nodeHash)));
    let authenticatedTreeDepth: number | undefined;
    if (allClaimsAuthenticated) {
      const depths = new Set(
        claims.map((claim) => {
          const proof = authenticatedClaims.get(bytesToHex(claim.nodeHash))!;
          return proof.sourcePath.length + 1 + proof.sourceLeafDelta;
        }),
      );
      if (depths.size !== 1)
        throw new Error("ECORRUPT: authenticated reused-subtree depths disagree");
      const suppliedDepth = [...depths][0];
      if (
        suppliedDepth === undefined ||
        !Number.isSafeInteger(suppliedDepth) ||
        suppliedDepth < 1 ||
        suppliedDepth > this.#limits.maxManifestDepth
      )
        throw new Error("ECORRUPT: authenticated reused-subtree depth is invalid");
      authenticatedTreeDepth = suppliedDepth;
    } else {
      const sourceRoot = this.#content.withManifestRoot(sourceManifestHash, (encoded) =>
        decodeManifestRoot(encoded, sourceManifestHash),
      );
      if (!sourceRoot) throw new Error("ECORRUPT: source manifest root is missing");
      validateSupportedManifestParameters(sourceRoot.parameters);
    }
    const distinctClaimHashes = [
      ...new Map(
        claims.map((claim) => [bytesToHex(claim.nodeHash), claim.nodeHash]),
      ).values(),
    ];
    const summaryLookupBatchSize = Math.max(
      1,
      Math.min(this.#limits.maxQueryBatchSize, 8),
    );
    const transactionSummaries = transactionSummaryCaches.get(this.#tx);
    for (
      let start = 0;
      start < distinctClaimHashes.length;
      start += summaryLookupBatchSize
    ) {
      const batch = distinctClaimHashes.slice(start, start + summaryLookupBatchSize);
      const missing: Uint8Array[] = [];
      for (const hash of batch) {
        const key = bytesToHex(hash);
        if (transactionSummaries?.has(key)) {
          const summary = transactionSummaries.get(key);
          if (summary) summariesByHash.set(key, summary);
        } else missing.push(hash);
      }
      if (missing.length) {
        const placeholders = missing.map(() => "?").join(",");
        const rows = this.#tx.all<SummaryRow>(
          `SELECT node_hash,object_count,object_bytes,node_count,node_bytes,membership_count,closure_fold,chain_digest,object_bloom,node_bloom,object_members,node_members FROM efs_manifest_subtree_summaries WHERE node_hash IN (${placeholders})`,
          missing,
          {
            maxRows: missing.length + 1,
            maxBytes: this.#limits.maxFinalTransactionBytes,
          },
        );
        const loaded = new Map(
          rows
            .filter((row) => row.node_hash)
            .map((row) => [bytesToHex(row.node_hash!), decodeSubtreeSummaryRow(row)]),
        );
        for (const hash of missing) {
          const key = bytesToHex(hash);
          const summary = loaded.get(key);
          transactionSummaries?.set(key, summary);
          if (summary) summariesByHash.set(key, summary);
        }
      }
    }
    // Do not materialize the complete staged closure here. Large local
    // rebuilds can have more staged rows than the transaction result budget;
    // individual claim checks and bounded reconciliation already validate
    // membership without this unbounded bookkeeping query.
    for (const hash of options.knownObjectHashes ?? []) {
      if (intrinsicByteLength(hash) !== 32)
        throw new RangeError("known reused-subtree object hash must be 32 bytes");
      seenObjectHashes.add(bytesToHex(hash));
    }
    for (const hash of options.knownNodeHashes ?? []) {
      if (intrinsicByteLength(hash) !== 32)
        throw new RangeError("known reused-subtree node hash must be 32 bytes");
      seenNodeHashes.add(bytesToHex(hash));
    }
    let summaryObjectCount = 0;
    let summaryObjectBytes = 0;
    let summaryNodeCount = 0;
    let summaryNodeBytes = 0;
    let summaryMembershipCount = 0;
    let summaryFold = new Uint8Array(32);
    const summaryChains: ManifestSubtreeSummary[] = [];
    const parents = new Map<
      string,
      {
        readonly path: readonly number[];
        readonly authenticated: ReturnType<
          ManifestTreeRepository["authenticateNodePath"]
        >;
      }
    >();
    let insertedRows = 0;
    const pendingClaims: Array<{
      readonly nodeHash: Uint8Array;
      readonly sourcePath: readonly number[];
      readonly span: number;
      readonly entryCount: number;
      readonly nonFinalLeafDelta: number | null;
      readonly finalLeafDelta: number | null;
      readonly summaryUsable: boolean;
    }> = [];
    for (const claim of claims) {
      const sourcePath = claim.sourcePath;
      if (
        sourcePath.length > this.#limits.maxManifestDepth ||
        sourcePath.some(
          (index) => !Number.isSafeInteger(index) || index < 0 || index > 255,
        )
      )
        throw new RangeError("reused subtree path is outside configured bounds");
      let authenticatedDepth: number;
      let sourceTreeDepth: number;
      let sourceFinalAtLevel: boolean;
      const suppliedProof = authenticatedClaims.get(bytesToHex(claim.nodeHash));
      if (suppliedProof) {
        const suppliedTreeDepth =
          suppliedProof.sourcePath.length + 1 + suppliedProof.sourceLeafDelta;
        if (
          suppliedProof.sourcePath.length !== sourcePath.length ||
          suppliedProof.sourcePath.some(
            (value, index) => value !== sourcePath[index],
          ) ||
          !equalBytes(suppliedProof.nodeHash, claim.nodeHash) ||
          suppliedProof.span !== claim.span ||
          suppliedProof.entryCount !== claim.entryCount ||
          !Number.isSafeInteger(suppliedProof.sourceLeafDelta) ||
          suppliedProof.sourceLeafDelta < 0 ||
          (authenticatedTreeDepth !== undefined &&
            suppliedTreeDepth !== authenticatedTreeDepth)
        )
          throw new Error("ECORRUPT: supplied reused-subtree proof disagrees");
        authenticatedDepth = sourcePath.length + 1;
        sourceTreeDepth = suppliedTreeDepth;
        sourceFinalAtLevel = suppliedProof.sourceFinalAtLevel;
      } else if (sourcePath.length === 0) {
        const authenticated = this.authenticateNodePath(sourceManifestHash, []);
        if (
          !equalBytes(authenticated.hash, claim.nodeHash) ||
          authenticated.node.span !== claim.span ||
          authenticated.node.entryCount !== claim.entryCount
        )
          throw new Error("ECORRUPT: reused root node is not source-authenticated");
        authenticatedDepth = authenticated.depth;
        sourceTreeDepth = authenticated.treeDepth;
        sourceFinalAtLevel = true;
      } else {
        const parentPath = sourcePath.slice(0, -1);
        const key = parentPath.join("/");
        let parent = parents.get(key);
        if (!parent) {
          parent = Object.freeze({
            path: Object.freeze([...parentPath]),
            authenticated: this.authenticateNodePath(sourceManifestHash, parentPath),
          });
          parents.set(key, parent);
        }
        if (parent.authenticated.node.kind !== "internal")
          throw new Error("ECORRUPT: reused subtree parent is not internal");
        const childIndex = sourcePath.at(-1)!;
        const child = parent.authenticated.node.children[childIndex];
        if (
          !child ||
          !equalBytes(child.hash, claim.nodeHash) ||
          child.span !== claim.span ||
          child.entryCount !== claim.entryCount
        )
          throw new Error("ECORRUPT: reused subtree claim is not source-authenticated");
        authenticatedDepth = checkedAdd(parent.authenticated.depth, 1);
        sourceTreeDepth = parent.authenticated.treeDepth;
        sourceFinalAtLevel =
          parent.authenticated.finalAtLevel &&
          childIndex === parent.authenticated.node.children.length - 1;
      }
      // The local-rebuild caller appends this exact batch immediately before
      // registering it in the same transaction. Its already-authenticated
      // source state makes the extra membership probe redundant; generic
      // callers retain the probe as a defense against misordered staging.
      if (options.knownObjectHashes === undefined) {
        const staged = this.#tx.all(
          "SELECT 1 present FROM efs_lease_staged_manifests WHERE lease_id=? AND kind=1 AND manifest_hash=?",
          [leaseId, claim.nodeHash],
          { maxRows: 1, maxBytes: 128 },
        ).length;
        if (!staged)
          throw new Error("ECORRUPT: reused subtree lacks staged membership");
      }
      const leafDelta = sourceTreeDepth - authenticatedDepth;
      if (leafDelta < 0)
        throw new Error("ECORRUPT: reused subtree depth exceeds source certificate");
      // A summary is produced when the source node is written, outside this
      // bounded edit's closure walk. If it is absent, leave the claim
      // count-only and let bounded reconciliation retain its safe full walk.
      const usableSummary = summariesByHash.get(bytesToHex(claim.nodeHash));
      const summaryObjectHashes = usableSummary
        ? memberHashKeys(usableSummary.objectMembers)
        : [];
      const summaryNodeHashes = usableSummary
        ? memberHashKeys(usableSummary.nodeMembers)
        : [];
      const summaryUsable =
        options.allowSummaries !== false &&
        usableSummary !== undefined &&
        !summaryObjectHashes.some((hash) => seenObjectHashes.has(hash)) &&
        !summaryNodeHashes.some((hash) => seenNodeHashes.has(hash));
      if (summaryUsable) {
        for (const hash of summaryObjectHashes) seenObjectHashes.add(hash);
        for (const hash of summaryNodeHashes) seenNodeHashes.add(hash);
        summaryObjectCount = checkedAdd(
          summaryObjectCount,
          usableSummary.objectCount,
          "summary object count",
        );
        summaryObjectBytes = checkedAdd(
          summaryObjectBytes,
          usableSummary.objectBytes,
          "summary object bytes",
        );
        summaryNodeCount = checkedAdd(
          summaryNodeCount,
          usableSummary.nodeCount,
          "summary node count",
        );
        summaryNodeBytes = checkedAdd(
          summaryNodeBytes,
          usableSummary.nodeBytes,
          "summary node bytes",
        );
        summaryMembershipCount = checkedAdd(
          summaryMembershipCount,
          usableSummary.membershipCount,
          "summary membership count",
        );
        summaryFold.set(foldHash(summaryFold, usableSummary.closureFold));
        summaryChains.push(usableSummary);
      }
      pendingClaims.push(
        Object.freeze({
          nodeHash: copyBytes(claim.nodeHash),
          sourcePath: Object.freeze([...sourcePath]),
          span: claim.span,
          entryCount: claim.entryCount,
          nonFinalLeafDelta: sourceFinalAtLevel ? null : leafDelta,
          finalLeafDelta: sourceFinalAtLevel ? leafDelta : null,
          summaryUsable,
        }),
      );
    }
    for (
      let start = 0;
      start < pendingClaims.length;
      start += this.#limits.maxQueryBatchSize
    ) {
      const batch = pendingClaims.slice(start, start + this.#limits.maxQueryBatchSize);
      insertedRows += this.#tx.run(
        `INSERT OR IGNORE INTO efs_staging_reused_subtrees(lease_id,node_hash,source_manifest_hash,source_path,span,entry_count,validated_nonfinal_leaf_delta,validated_final_leaf_delta,summary_usable) VALUES ${batch
          .map(() => "(?,?,?,?,?,?,?,?,?)")
          .join(",")}`,
        batch.flatMap((claim) => [
          leaseId,
          claim.nodeHash,
          sourceManifestHash,
          Uint8Array.from(claim.sourcePath),
          claim.span,
          claim.entryCount,
          claim.nonFinalLeafDelta,
          claim.finalLeafDelta,
          claim.summaryUsable ? 1 : 0,
        ]),
      ).changes;
    }
    if (summaryMembershipCount) {
      if (options.deferCertificateWrite && !options.certificateState)
        throw new Error("ECORRUPT: deferred certificate update lacks its state");
      const row = options.certificateState
        ? {
            chain_digest: copyBytes(options.certificateState.chainDigest),
            chain_fold: copyBytes(options.certificateState.chainFold),
            object_count: options.certificateState.objectCount,
            object_bytes: options.certificateState.objectBytes,
            node_count: options.certificateState.nodeCount,
            node_bytes: options.certificateState.nodeBytes,
            membership_count: options.certificateState.membershipCount,
          }
        : this.#tx.all<
            {
              chain_digest: Uint8Array;
              chain_fold: Uint8Array;
              object_count: number;
              object_bytes: number;
              node_count: number;
              node_bytes: number;
              membership_count: number;
            } & SqliteRow
          >(
            "SELECT chain_digest,chain_fold,object_count,object_bytes,node_count,node_bytes,membership_count FROM efs_staging_certificates WHERE lease_id=? AND sealed=0",
            [leaseId],
            { maxRows: 1, maxBytes: 4096 },
          )[0];
      if (!row) throw new Error("ECORRUPT: missing open staging certificate");
      let chain = copyBytes(row.chain_digest);
      let sequence = row.membership_count;
      for (const summary of summaryChains) {
        chain = extendSummaryCertificateChain(chain, sequence, summary);
        sequence = checkedAdd(
          sequence,
          summary.membershipCount,
          "summary certificate sequence",
        );
      }
      const certificatePatch = Object.freeze({
        chainDigest: chain,
        chainFold: foldHash(row.chain_fold, summaryFold),
        objectCount: checkedAdd(
          row.object_count,
          summaryObjectCount,
          "summary certificate object count",
        ),
        objectBytes: checkedAdd(
          row.object_bytes,
          summaryObjectBytes,
          "summary certificate object bytes",
        ),
        nodeCount: checkedAdd(
          row.node_count,
          summaryNodeCount,
          "summary certificate node count",
        ),
        nodeBytes: checkedAdd(
          row.node_bytes,
          summaryNodeBytes,
          "summary certificate node bytes",
        ),
        membershipCount: sequence,
      });
      if (options.deferCertificateWrite) {
        if (!options.certificatePatch)
          throw new Error("ECORRUPT: deferred certificate update lacks its patch sink");
        options.certificatePatch.value = certificatePatch;
      } else {
        this.#tx.run(
          "UPDATE efs_staging_certificates SET chain_digest=?,chain_fold=?,object_count=?,object_bytes=?,node_count=?,node_bytes=?,membership_count=?,next_sequence=? WHERE lease_id=? AND sealed=0",
          [
            certificatePatch.chainDigest,
            certificatePatch.chainFold,
            certificatePatch.objectCount,
            certificatePatch.objectBytes,
            certificatePatch.nodeCount,
            certificatePatch.nodeBytes,
            certificatePatch.membershipCount,
            certificatePatch.membershipCount,
            leaseId,
          ],
        );
      }
    }
    if (insertedRows)
      applyChargedMetadata(
        this.#tx,
        this.#limits,
        insertedRows * CHARGED_ROW_BYTES,
        "source-authenticated reused subtree",
      );
    return Object.freeze(
      pendingClaims.map((claim) => {
        const summary = summariesByHash.get(bytesToHex(claim.nodeHash));
        return Object.freeze({
          nodeHash: copyBytes(claim.nodeHash),
          sourceManifestHash: copyBytes(sourceManifestHash),
          sourcePath: Uint8Array.from(claim.sourcePath),
          span: claim.span,
          entryCount: claim.entryCount,
          validatedNonfinalLeafDelta: claim.nonFinalLeafDelta,
          validatedFinalLeafDelta: claim.finalLeafDelta,
          summaryUsable: claim.summaryUsable,
          ...(summary ? { summary } : {}),
        });
      }),
    );
  }

  authenticateNodePath(
    manifestHash: Uint8Array,
    sourcePath: readonly number[],
  ): AuthenticatedNodePath {
    if (intrinsicByteLength(manifestHash) !== 32)
      throw new RangeError("manifest hash must contain exactly 32 bytes");
    manifestHash = copyBytes(manifestHash);
    const cacheKey = `${bytesToHex(manifestHash)}:${sourcePath.join("/")}`;
    let pathCache = authenticatedPathCaches.get(this.#tx);
    if (!pathCache) {
      pathCache = new Map();
      authenticatedPathCaches.set(this.#tx, pathCache);
    }
    const cached = pathCache.get(cacheKey);
    if (cached) return cached;
    const treeDepth = this.#content.validatedManifestDepth(manifestHash);
    if (treeDepth === undefined)
      throw new Error("ECORRUPT: source manifest lacks a validation certificate");
    const root = this.#content.withManifestRoot(manifestHash, (rootBytes) =>
      decodeManifestRoot(rootBytes, manifestHash),
    );
    if (!root) throw new Error("ECORRUPT: missing source manifest root");
    validateSupportedManifestParameters(root.parameters);
    if (root.parameters.maximum > maxPersistedContentObjectBytes(this.#limits))
      throw new RangeError(
        "source manifest FastCDC maximum exceeds the durable object transaction envelope",
      );
    let hash = copyBytes(root.rootNodeHash);
    let finalAtLevel = true;
    let expected: ManifestChild | undefined;
    for (let depth = 1; ; depth += 1) {
      if (depth > this.#limits.maxManifestDepth)
        throw new Error("ECORRUPT: reused subtree path exceeds manifest depth");
      const node = this.#content.withManifestNode(hash, (encoded) =>
        decodeManifestNode(encoded, hash),
      );
      if (!node) throw new Error("ECORRUPT: missing reused manifest node");
      if (
        expected &&
        (node.span !== expected.span || node.entryCount !== expected.entryCount)
      )
        throw new Error("ECORRUPT: reused subtree child totals mismatch");
      validateCanonicalManifestNode(node, root.parameters, finalAtLevel, depth === 1);
      if (
        depth === 1 &&
        (node.span !== root.fileSize ||
          node.entryCount !== root.entryCount ||
          (root.fileSize === 0) !== (root.entryCount === 0))
      )
        throw new Error("ECORRUPT: reused source root totals mismatch");
      if (depth - 1 === sourcePath.length) {
        const authenticated = Object.freeze({
          hash: copyBytes(hash),
          node: snapshotNode(node),
          depth,
          treeDepth,
          finalAtLevel,
        });
        pathCache.set(cacheKey, authenticated);
        return authenticated;
      }
      if (node.kind !== "internal")
        throw new Error("ECORRUPT: reused subtree path continues below a leaf");
      const index = sourcePath[depth - 1]!;
      if (index >= node.children.length)
        throw new Error("ECORRUPT: reused subtree path child is absent");
      expected = node.children[index]!;
      hash = copyBytes(expected.hash);
      finalAtLevel = finalAtLevel && index === node.children.length - 1;
    }
  }

  #activeOwner(leaseId: string, ownerNonce: Uint8Array): void {
    const owner = this.#tx.all<LeaseOwnerRow>(
      "SELECT l.owner_nonce,l.state,c.sealed FROM efs_leases l JOIN efs_staging_certificates c ON c.lease_id=l.id WHERE l.id=?",
      [leaseId],
      { maxRows: 1, maxBytes: 256 },
    )[0];
    if (
      !owner ||
      !equalBytes(owner.owner_nonce, ownerNonce) ||
      owner.state !== 0 ||
      owner.sealed !== 0
    )
      throw new Error("ECORRUPT: path-copy staging owner is not active");
  }
}
