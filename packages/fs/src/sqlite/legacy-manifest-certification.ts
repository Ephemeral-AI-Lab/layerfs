import { decodeManifestNode, decodeManifestRoot } from "../manifests/codec.js";
import { validateManifestTree } from "../manifests/cursor.js";
import type { FilesystemSQLiteTransaction, SqliteRow } from "./driver.js";

const MAX_ATOMIC_LEGACY_MANIFEST_VISITS = 100_000;
const MAX_ATOMIC_LEGACY_MANIFEST_MS = 5_000;

export interface LegacyManifestLimits {
  readonly maxManifestEntries: number;
  readonly maxManifestDepth: number;
  readonly maxFileBytes: number;
  readonly maxContentObjectBytes: number;
}

/**
 * Authenticate the bounded legacy manifest set while the v4 migration owns its
 * exclusive transaction. Keeping this SQLite adapter separate prevents schema
 * migration orchestration from composing the COW and manifest transformations.
 */
export function certifyLegacyManifests(
  tx: FilesystemSQLiteTransaction,
  manifest: LegacyManifestLimits,
): void {
  let after: Uint8Array = Uint8Array.of(0);
  let visited = 0;
  const deadline = performance.now() + MAX_ATOMIC_LEGACY_MANIFEST_MS;
  const checkBounds = (): void => {
    if (performance.now() > deadline)
      throw new Error("ESCHEMA: legacy manifest validation exceeds time cap");
    if (visited > MAX_ATOMIC_LEGACY_MANIFEST_VISITS)
      throw new Error("ESCHEMA: legacy manifest validation exceeds row cap");
  };
  while (true) {
    checkBounds();
    const roots = tx.all<{ hash: Uint8Array; encoded: Uint8Array } & SqliteRow>(
      "SELECT hash,encoded FROM efs_manifest_roots WHERE hash>? ORDER BY hash LIMIT 256",
      [after],
      { maxRows: 256, maxBytes: 256 * 256 },
    );
    for (const root of roots) {
      const decodedRoot = decodeManifestRoot(root.encoded, root.hash);
      if (
        decodedRoot.entryCount > manifest.maxManifestEntries ||
        decodedRoot.fileSize > manifest.maxFileBytes ||
        decodedRoot.parameters.maximum > manifest.maxContentObjectBytes
      )
        throw new Error("ESCHEMA: legacy manifest exceeds requested storage limits");
      visited += 1;
      checkBounds();
      let leafDepth: number | undefined;
      validateManifestTree(
        root.encoded,
        {
          get(hash) {
            visited += 1;
            checkBounds();
            return tx.all<{ encoded: Uint8Array } & SqliteRow>(
              "SELECT encoded FROM efs_manifest_nodes WHERE hash=?",
              [hash],
              { maxRows: 1, maxBytes: 32 * 1024 },
            )[0]?.encoded;
          },
        },
        root.hash,
        manifest.maxManifestDepth,
      );
      // validateManifestTree proves balance; one authenticated left path obtains
      // the durable absolute leaf depth without exposing content bytes.
      visited += 1;
      checkBounds();
      let hash: Uint8Array = tx.all<{ root_node_hash: Uint8Array } & SqliteRow>(
        "SELECT root_node_hash FROM efs_manifest_roots WHERE hash=?",
        [root.hash],
        { maxRows: 1, maxBytes: 128 },
      )[0]!.root_node_hash;
      for (let depth = 1; depth <= manifest.maxManifestDepth; depth += 1) {
        visited += 1;
        checkBounds();
        const row = tx.all<{ kind: number; encoded: Uint8Array } & SqliteRow>(
          "SELECT kind,encoded FROM efs_manifest_nodes WHERE hash=?",
          [hash],
          { maxRows: 1, maxBytes: 32 * 1024 },
        )[0];
        if (!row) throw new Error("ECORRUPT: legacy manifest node is missing");
        const decoded = decodeManifestNode(row.encoded, hash);
        if (decoded.kind === "leaf") {
          leafDepth = depth;
          break;
        }
        hash = decoded.children[0]!.hash;
      }
      if (leafDepth === undefined)
        throw new Error("ECORRUPT: legacy manifest depth exceeds configured limit");
      tx.run(
        "INSERT INTO efs_manifest_validations(manifest_hash,tree_depth) VALUES(?,?)",
        [root.hash, leafDepth],
      );
      after = root.hash;
    }
    if (roots.length < 256) break;
  }
}
