import { sha256 } from "../cas/sha256.js";
import { bytesToHex } from "../cas/bytes.js";
import { checkedAdd } from "../resources/safe-integers.js";
import { encodeManifestNode, encodeManifestRoot, type ManifestChild, type ManifestEntry, type ManifestInternal, type ManifestLeaf, type ManifestNode, type ManifestParameters } from "./codec.js";
import { advanceManifestGroupingState, isManifestGroupBoundary } from "./grouping.js";

export interface EncodedManifestNode { readonly hash: Uint8Array; readonly encoded: Uint8Array; readonly node: ManifestNode }
export interface BuiltManifest { readonly id: string; readonly rootHash: Uint8Array; readonly root: Uint8Array; readonly nodes: ReadonlyMap<string, EncodedManifestNode>; readonly entries: readonly ManifestEntry[] }

function contentDefinedGroups<T extends ManifestEntry | ManifestChild>(records: readonly T[], minimum: number, target: number, maximum: number): T[][] {
  if (records.length === 0) return [[]];
  const groups: T[][] = []; let group: T[] = []; let state = 0n;
  for (const record of records) {
    group.push(record);
    state = advanceManifestGroupingState(state, record);
    if (isManifestGroupBoundary(group.length, state, minimum, target, maximum)) { groups.push(group); group = []; state = 0n; }
  }
  if (group.length) groups.push(group);
  return groups;
}

function storeNode(node: ManifestNode, nodes: Map<string, EncodedManifestNode>): ManifestChild {
  const encoded = encodeManifestNode(node); const hash = sha256(encoded); const key = bytesToHex(hash);
  nodes.set(key, Object.freeze({ hash, encoded, node }));
  return Object.freeze({ hash, span: node.span, entryCount: node.entryCount });
}

export function buildManifestFromEntries(entries: readonly ManifestEntry[], parameters: ManifestParameters): BuiltManifest {
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
