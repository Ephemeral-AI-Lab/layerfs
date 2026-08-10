import { copyBytes, equalBytes } from "../cas/bytes.js";
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
import type { FilesystemSQLiteTransaction, SqliteRow } from "./driver.js";
import { ContentRepository } from "./content-repository.js";
import { CHARGED_ROW_BYTES, UsageRepository } from "./usage-repository.js";

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
  ) {
    this.#tx = tx;
    this.#limits = limits;
    this.#content = new ContentRepository(tx, limits, cache);
  }

  pathAtOffset(manifestHash: Uint8Array, offset: number): SQLiteManifestTreePath {
    manifestHash = copyBytes(manifestHash);
    if (!Number.isSafeInteger(offset) || offset < 0)
      throw new RangeError("manifest tree offset must be a nonnegative safe integer");
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
      new UsageRepository(this.#tx, this.#limits).apply(
        { charged_metadata_bytes: CHARGED_ROW_BYTES },
        "path-copy source root link",
      );
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
  ): void {
    this.#activeOwner(leaseId, ownerNonce);
    if (claims.length > this.#limits.maxManifestDepth * 128)
      throw new RangeError("reused subtree claim batch exceeds bounded tree fanout");
    const linked = this.#tx.all(
      "SELECT 1 present FROM efs_lease_manifests WHERE lease_id=? AND manifest_hash=?",
      [leaseId, sourceManifestHash],
      { maxRows: 1, maxBytes: 128 },
    ).length;
    if (!linked) throw new Error("ECORRUPT: reused subtree lacks its source root link");
    const parents = new Map<
      string,
      { readonly path: readonly number[]; readonly node: ManifestNode }
    >();
    let insertedRows = 0;
    for (const claim of claims) {
      const sourcePath = claim.sourcePath;
      if (
        !sourcePath.length ||
        sourcePath.length > this.#limits.maxManifestDepth ||
        sourcePath.some(
          (index) => !Number.isSafeInteger(index) || index < 0 || index > 255,
        )
      )
        throw new RangeError("reused subtree path is outside configured bounds");
      const parentPath = sourcePath.slice(0, -1);
      const key = parentPath.join("/");
      let parent = parents.get(key);
      if (!parent) {
        parent = Object.freeze({
          path: Object.freeze([...parentPath]),
          node: this.authenticateNodePath(sourceManifestHash, parentPath).node,
        });
        parents.set(key, parent);
      }
      if (parent.node.kind !== "internal")
        throw new Error("ECORRUPT: reused subtree parent is not internal");
      const child = parent.node.children[sourcePath.at(-1)!];
      if (
        !child ||
        !equalBytes(child.hash, claim.nodeHash) ||
        child.span !== claim.span ||
        child.entryCount !== claim.entryCount
      )
        throw new Error("ECORRUPT: reused subtree claim is not source-authenticated");
      const staged = this.#tx.all(
        "SELECT 1 present FROM efs_lease_staged_manifests WHERE lease_id=? AND kind=1 AND manifest_hash=?",
        [leaseId, claim.nodeHash],
        { maxRows: 1, maxBytes: 128 },
      ).length;
      if (!staged) throw new Error("ECORRUPT: reused subtree lacks staged membership");
      insertedRows += this.#tx.run(
        "INSERT OR IGNORE INTO efs_staging_reused_subtrees(lease_id,node_hash,source_manifest_hash,source_path,span,entry_count) VALUES(?,?,?,?,?,?)",
        [
          leaseId,
          claim.nodeHash,
          sourceManifestHash,
          Uint8Array.from(sourcePath),
          claim.span,
          claim.entryCount,
        ],
      ).changes;
    }
    if (insertedRows)
      new UsageRepository(this.#tx, this.#limits).apply(
        { charged_metadata_bytes: insertedRows * CHARGED_ROW_BYTES },
        "source-authenticated reused subtree",
      );
  }

  authenticateNodePath(
    manifestHash: Uint8Array,
    sourcePath: readonly number[],
  ): { readonly hash: Uint8Array; readonly node: ManifestNode } {
    manifestHash = copyBytes(manifestHash);
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
      if (depth - 1 === sourcePath.length)
        return Object.freeze({ hash: copyBytes(hash), node: snapshotNode(node) });
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
