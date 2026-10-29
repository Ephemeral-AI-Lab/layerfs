// P1 cases only (branch.graph + branch.changed_paths). P14's branch/workspace
// matrices arrive with the P14 slice and merge into this file then.

const graphResult = (branch, pagePos) => ({
  branch: { name: branch, id: "branch1234567890ab", base: "base1234567890ab", head: "head1234567890ab", forkedFrom: "c0ffee" },
  forkPoint: { kind: "commit", id: "c0ffee" },
  commits: [{ id: "decaf", branch, parent: "parent1234567890", logical: 120, newCanonical: 40, newStored: 30, newMetadata: 8, published: true }],
  hasMore: true,
  nextPagePos: pagePos + 1,
});

const changedResult = (commit) => ({
  commit,
  parent: "parent1234567890",
  items: [{ path: "/changed.txt", kind: "Modified", type: "file", before: { type: "file", inode: 7 }, after: { type: "file", inode: 7 }, content: true, identity: false, metadata: false, hunks: null, hunksOmitted: false }],
  hasMore: false,
  nextPagePos: null,
});

export const P1_CASES = [
  {
    name: "P1 branch graph",
    argv: ["branch", "graph", "cart", "--page-size", "2", "--page-pos", "1"],
    sdk: (lf) => lf.branch.graph("cart", 2, 1),
    op: "branch.graph",
    sent: { branch: "cart", pageSize: 2, pagePos: 1 },
    result: graphResult("cart", 1),
  },
  {
    name: "P1 branch changed_paths",
    argv: ["branch", "changed_paths", "c0ffee", "--page-size", "5"],
    sdk: (lf) => lf.branch.changed_paths("c0ffee", 5),
    op: "branch.changed_paths",
    sent: { commit: "c0ffee", pageSize: 5, pagePos: 0 },
    result: changedResult("c0ffee"),
  },
];

export const CASES = [...P1_CASES];

