import { FASTCDC_GEAR_V1, fastCdcChunks, type FastCdcConfiguration } from "../cdc/fastcdc.js";
import { sha256 } from "../cas/sha256.js";
import { bytesToHex, checkedAdd } from "../utils/bytes.js";
import { encodeManifestNode, encodeManifestRoot, type ManifestChild, type ManifestEntry, type ManifestInternal, type ManifestLeaf, type ManifestNode } from "./codec.js";

export interface EncodedManifestNode { readonly hash: Uint8Array; readonly encoded: Uint8Array; readonly node: ManifestNode }
export interface BuiltManifest { readonly id: string; readonly rootHash: Uint8Array; readonly root: Uint8Array; readonly nodes: ReadonlyMap<string, EncodedManifestNode>; readonly entries: readonly ManifestEntry[] }

function recordBytes(entry: ManifestEntry | ManifestChild): Uint8Array {
  if ("length" in entry) {
    const bytes = new Uint8Array(36); bytes.set(entry.hash); new DataView(bytes.buffer).setUint32(32, entry.length, true); return bytes;
  }
  const bytes = new Uint8Array(48); const view = new DataView(bytes.buffer); bytes.set(entry.hash); view.setBigUint64(32, BigInt(entry.span), true); view.setBigUint64(40, BigInt(entry.entryCount), true); return bytes;
}

function contentDefinedGroups<T extends ManifestEntry | ManifestChild>(records: readonly T[], minimum: number, target: number, maximum: number): T[][] {
  if (records.length === 0) return [[]];
  const groups: T[][] = []; let group: T[] = []; let state = 0n;
  for (const record of records) {
    group.push(record);
    for (const byte of recordBytes(record)) state = ((state << 1n) + BigInt(FASTCDC_GEAR_V1[byte]!)) & 0xffff_ffff_ffff_ffffn;
    if (group.length >= maximum || (group.length >= minimum && (state & BigInt(target - 1)) === 0n)) { groups.push(group); group = []; state = 0n; }
  }
  if (group.length) groups.push(group);
  return groups;
}

function storeNode(node: ManifestNode, nodes: Map<string, EncodedManifestNode>): ManifestChild {
  const encoded = encodeManifestNode(node); const hash = sha256(encoded); const key = bytesToHex(hash);
  nodes.set(key, Object.freeze({ hash, encoded, node }));
  return Object.freeze({ hash, span: node.span, entryCount: node.entryCount });
}

export function buildManifestFromEntries(entries: readonly ManifestEntry[], parameters: FastCdcConfiguration): BuiltManifest {
  const nodes = new Map<string, EncodedManifestNode>();
  let current: ManifestChild[] = contentDefinedGroups(entries, 64, 128, 256).map((group) => {
    const span = group.reduce((sum, entry) => checkedAdd(sum, entry.length), 0);
    const leaf: ManifestLeaf = { kind: "leaf", span, entryCount: group.length, entries: group };
    return storeNode(leaf, nodes);
  });
  while (current.length > 1) {
    current = contentDefinedGroups(current, 32, 64, 128).map((group) => {
      const internal: ManifestInternal = { kind: "internal", span: group.reduce((sum, child) => checkedAdd(sum, child.span), 0), entryCount: group.reduce((sum, child) => checkedAdd(sum, child.entryCount), 0), children: group };
      return storeNode(internal, nodes);
    });
  }
  const rootNode = current[0]!;
  const fileSize = entries.reduce((sum, entry) => checkedAdd(sum, entry.length), 0);
  const root = encodeManifestRoot({ parameters, fileSize, entryCount: entries.length, rootNodeHash: rootNode.hash });
  const rootHash = sha256(root);
  return Object.freeze({ id: bytesToHex(rootHash), rootHash, root, nodes, entries: Object.freeze([...entries]) });
}

export function buildManifest(bytes: Uint8Array, parameters: FastCdcConfiguration): BuiltManifest & { readonly objects: ReadonlyMap<string, Uint8Array> } {
  const objects = new Map<string, Uint8Array>();
  const entries = fastCdcChunks(bytes, parameters).map(({ offset, length }) => {
    const object = bytes.slice(offset, offset + length); const hash = sha256(object); objects.set(bytesToHex(hash), object); return Object.freeze({ hash, length });
  });
  return Object.freeze({ ...buildManifestFromEntries(entries, parameters), objects });
}

