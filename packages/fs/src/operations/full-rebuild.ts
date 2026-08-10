import { bytesToHex, copyBytes, intrinsicByteRange } from "../cas/bytes.js";
import { manifestIdFromHash, sha256 } from "../cas/sha256.js";
import {
  findFastCdcBoundary,
  type FastCdcConfiguration,
  validateSupportedFastCdcConfiguration,
} from "../cdc/fastcdc.js";
import {
  buildManifestFromEntries,
  type EncodedManifestNode,
  type ManifestBuildRecord,
  type ManifestBuildWorkspace,
} from "../manifests/builder.js";
import type { ManifestEntry } from "../manifests/codec.js";
import { MAX_CONTENT_OBJECT_BYTES } from "../resources/limits.js";

export interface DiagnosticBuiltManifest {
  readonly id: string;
  readonly rootHash: Uint8Array;
  readonly root: Uint8Array;
  readonly nodes: ReadonlyMap<string, EncodedManifestNode>;
  readonly entries: readonly ManifestEntry[];
}
export const MAX_DIAGNOSTIC_MANIFEST_ENTRIES = 16_384;
export const MAX_DIAGNOSTIC_CONTENT_BYTES = MAX_CONTENT_OBJECT_BYTES;

class BoundedDiagnosticWorkspace implements ManifestBuildWorkspace {
  readonly nodes = new Map<string, EncodedManifestNode>();
  readonly levels = new Map<number, ManifestBuildRecord[]>();
  writeNode(record: {
    readonly level: number;
    readonly index: number;
    readonly child: ManifestBuildRecord["child"];
    readonly value: EncodedManifestNode;
  }): void {
    if (this.nodes.size >= MAX_DIAGNOSTIC_MANIFEST_ENTRIES * 2)
      throw new RangeError("diagnostic manifest node limit exceeded");
    this.nodes.set(bytesToHex(record.value.hash), record.value);
    const level = this.levels.get(record.level) ?? [];
    level.push({ index: record.index, child: record.child });
    this.levels.set(record.level, level);
  }
  readLevel(
    level: number,
    afterIndex: number,
    limit: number,
  ): readonly ManifestBuildRecord[] {
    return (this.levels.get(level) ?? []).slice(afterIndex + 1, afterIndex + 1 + limit);
  }
}

/** A deliberately capped diagnostic fixture builder; storage-scale paths use a durable ManifestBuildWorkspace. */
export function buildManifest(
  bytes: Uint8Array,
  parameters: FastCdcConfiguration,
): DiagnosticBuiltManifest & { readonly objects: ReadonlyMap<string, Uint8Array> } {
  bytes = intrinsicByteRange(bytes);
  parameters = Object.freeze({
    minimum: parameters.minimum,
    average: parameters.average,
    maximum: parameters.maximum,
  });
  validateSupportedFastCdcConfiguration(parameters);
  if (bytes.byteLength > MAX_DIAGNOSTIC_CONTENT_BYTES)
    throw new RangeError(
      `diagnostic manifest input exceeds ${MAX_DIAGNOSTIC_CONTENT_BYTES} bytes`,
    );
  const objects = new Map<string, Uint8Array>();
  const entries: ManifestEntry[] = [];
  for (let offset = 0; offset < bytes.byteLength;) {
    if (entries.length >= MAX_DIAGNOSTIC_MANIFEST_ENTRIES)
      throw new RangeError(
        "diagnostic manifest entry limit exceeded; use a durable streaming workspace",
      );
    const boundary = findFastCdcBoundary(bytes, offset, parameters);
    const object = copyBytes(bytes, offset, boundary);
    const hash = sha256(object);
    objects.set(bytesToHex(hash), object);
    entries.push(Object.freeze({ hash, length: object.byteLength }));
    offset = boundary;
  }
  const workspace = new BoundedDiagnosticWorkspace();
  const built = buildManifestFromEntries(entries, parameters, workspace);
  return Object.freeze({
    id: manifestIdFromHash(built.rootHash),
    rootHash: built.rootHash,
    root: built.root,
    nodes: workspace.nodes,
    entries: Object.freeze(entries),
    objects,
  });
}
