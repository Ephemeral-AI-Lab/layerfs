import { sha256 } from "../../packages/fs/dist/cas/sha256.js";
import {
  encodeManifestNode,
  encodeManifestRoot,
} from "../../packages/fs/dist/manifests/codec.js";

export const RELEASED_FIXTURE_FILE = "fixture-file";
export const RELEASED_FIXTURE_INODE = "fixture-file-inode";
export const RELEASED_FIXTURE_BRANCH = "fixture-branch";
export const RELEASED_FIXTURE_BYTES = Uint8Array.of(0x45, 0x46, 0x53, 0x36);

function hex(bytes) {
  return Buffer.from(bytes).toString("hex");
}

export function seedReleasedSchemaData(tx) {
  const encoder = new TextEncoder();
  const nameSort = encoder.encode(RELEASED_FIXTURE_FILE);
  const objectHash = sha256(RELEASED_FIXTURE_BYTES);
  const node = encodeManifestNode({
    kind: "leaf",
    span: RELEASED_FIXTURE_BYTES.byteLength,
    entryCount: 1,
    entries: [{ hash: objectHash, length: RELEASED_FIXTURE_BYTES.byteLength }],
  });
  const nodeHash = sha256(node);
  const root = encodeManifestRoot({
    parameters: { minimum: 1, average: 2, maximum: 4 },
    fileSize: RELEASED_FIXTURE_BYTES.byteLength,
    entryCount: 1,
    rootNodeHash: nodeHash,
  });
  const rootHash = sha256(root);
  const rootRevision = encoder.encode(
    JSON.stringify({
      id: "root",
      type: 1,
      mode: 493,
      birthtime_ms: 1,
      mtime_ms: 1,
      ctime_ms: 1,
      nlink: 1,
      size: null,
      manifest_hash: null,
      symlink_target: null,
      token: 0,
    }),
  );
  const fileRevision = encoder.encode(
    JSON.stringify({
      id: RELEASED_FIXTURE_INODE,
      type: 0,
      mode: 420,
      birthtime_ms: 2,
      mtime_ms: 2,
      ctime_ms: 2,
      nlink: 1,
      size: RELEASED_FIXTURE_BYTES.byteLength,
      manifest_hash: hex(rootHash),
      symlink_target: null,
      token: 1,
    }),
  );
  const entryRevision = encoder.encode(
    JSON.stringify({
      parent_inode: "root",
      name_sort: hex(nameSort),
      name: RELEASED_FIXTURE_FILE,
      inode_id: RELEASED_FIXTURE_INODE,
      token: 1,
    }),
  );

  tx.run("INSERT INTO efs_revisions VALUES(0,NULL,1,'bootstrap',1)");
  tx.run("INSERT INTO efs_revisions VALUES(1,0,2,'released-fixture',2)");
  tx.run("INSERT INTO efs_cas_objects VALUES(?,?,?,1)", [
    objectHash,
    RELEASED_FIXTURE_BYTES.byteLength,
    RELEASED_FIXTURE_BYTES,
  ]);
  tx.run("INSERT INTO efs_manifest_nodes VALUES(?,0,4,1,?,2)", [nodeHash, node]);
  tx.run("INSERT INTO efs_manifest_roots VALUES(?,?,4,1,1,2,4,?,3)", [
    rootHash,
    nodeHash,
    root,
  ]);
  tx.run("INSERT INTO efs_inodes VALUES('root',1,493,1,1,1,1,NULL,NULL,NULL,0)");
  tx.run("INSERT INTO efs_inodes VALUES(?,0,420,2,2,2,1,?,?,NULL,1)", [
    RELEASED_FIXTURE_INODE,
    RELEASED_FIXTURE_BYTES.byteLength,
    rootHash,
  ]);
  tx.run("INSERT INTO efs_entries VALUES('root',?,?,?,1)", [
    nameSort,
    RELEASED_FIXTURE_FILE,
    RELEASED_FIXTURE_INODE,
  ]);
  tx.run("INSERT INTO efs_inode_revisions VALUES(0,'root',0,?)", [rootRevision]);
  tx.run("INSERT INTO efs_inode_revisions VALUES(1,?,0,?)", [
    RELEASED_FIXTURE_INODE,
    fileRevision,
  ]);
  tx.run("INSERT INTO efs_entry_revisions VALUES(1,'root',?,0,?)", [
    nameSort,
    entryRevision,
  ]);
  tx.run("INSERT INTO efs_revision_manifest_roots VALUES(1,?,?)", [
    RELEASED_FIXTURE_INODE,
    rootHash,
  ]);
  tx.run("INSERT INTO efs_branch_ids VALUES(?,2)", [RELEASED_FIXTURE_BRANCH]);
  tx.run("INSERT INTO efs_branches VALUES(?,1,0,0,2,NULL)", [RELEASED_FIXTURE_BRANCH]);
  return Object.freeze({ nodeBytes: node.byteLength, rootBytes: root.byteLength });
}
