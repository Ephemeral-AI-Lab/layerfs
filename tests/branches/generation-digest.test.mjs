import assert from "node:assert/strict";
import test from "node:test";

import { computeBranchGenerationDigest } from "../../packages/fs/dist/operations/generation-digest.js";

const digestBytes = (value) => new Uint8Array(32).fill(value);

test("efs-branch-generation-digest-v1 golden fixtures", () => {
  const empty = computeBranchGenerationDigest({
    filesystemId: "fs-empty",
    branchId: "branch-e\u0301",
    baseRevision: "0",
    generation: 0,
    namespace: [],
    nodes: [],
    expectations: [],
    immutableReferences: [],
  });
  assert.equal(
    empty,
    "f005f165fdcc6dc79735e1790f03a9311e2cb0b0833554ad46ab25130aec266d",
  );

  const nonempty = computeBranchGenerationDigest({
    filesystemId: "fs-nonempty",
    branchId: "branch-e\u0301",
    baseRevision: "42",
    generation: 7,
    namespace: [
      { path: "/z", disposition: "tombstone", inodeId: null },
      { path: "/a", disposition: "present", inodeId: "inode-file" },
    ],
    nodes: [
      {
        inodeId: "inode-link",
        kind: "symlink",
        mode: 0o777,
        birthtimeMs: 6,
        mtimeMs: 7,
        ctimeMs: 8,
        logicalSize: 0,
        manifestHash: null,
        pages: [],
        patches: [],
        symlinkTarget: "../target",
      },
      {
        inodeId: "inode-file",
        kind: "file",
        mode: 0o640,
        birthtimeMs: 1,
        mtimeMs: 2,
        ctimeMs: 3,
        logicalSize: 8193,
        manifestHash: digestBytes(0x11),
        pages: [
          { index: 1, bytes: Uint8Array.of(9, 8, 7) },
          { index: 0, bytes: Uint8Array.of(1, 2, 3, 4) },
        ],
        patches: [
          {
            order: 1,
            offset: 12,
            deleteLength: 3,
            insertManifestDigest: null,
          },
          {
            order: 0,
            offset: 2,
            deleteLength: 8,
            insertManifestDigest: digestBytes(0x22),
          },
        ],
        symlinkTarget: null,
      },
      {
        inodeId: "inode-dir",
        kind: "directory",
        mode: 0o755,
        birthtimeMs: 3,
        mtimeMs: 4,
        ctimeMs: 5,
        logicalSize: 0,
        manifestHash: null,
        pages: [],
        patches: [],
        symlinkTarget: null,
      },
    ],
    expectations: [
      {
        reason: "ancestor-changed",
        path: "/f",
        expectedRevision: null,
        expectedToken: null,
      },
      {
        reason: "subtree-changed",
        path: "/e",
        expectedRevision: "42",
        expectedToken: null,
      },
      {
        reason: "destination-changed",
        path: "/d",
        expectedRevision: null,
        expectedToken: "9",
      },
      {
        reason: "source-changed",
        path: "/c",
        expectedRevision: "41",
        expectedToken: "8",
      },
      {
        reason: "node-changed",
        path: "/b",
        expectedRevision: null,
        expectedToken: "7",
      },
      {
        reason: "entry-changed",
        path: "/a",
        expectedRevision: "40",
        expectedToken: null,
      },
    ],
    immutableReferences: [
      { kind: "manifest", digest: digestBytes(0x44) },
      { kind: "content", digest: digestBytes(0x33) },
    ],
  });
  assert.equal(
    nonempty,
    "89efe082029285feb5e9245b3cf8b459ef9eef1f873a77b0c40fafef20da2de5",
  );
});
