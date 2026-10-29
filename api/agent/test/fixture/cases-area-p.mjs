// P14 branch/workspace paging matrices + P1 graph/changed_paths cases, in the
// shared op/sent/result triple shape. `prepare` selects the stand-in's data;
// `sdk` makes the one binding call described by the triple.

const branchNames = (count) => [
  ...Array.from({ length: count - 1 }, (_, n) => `branch-${String(n).padStart(3, "0")}`),
  "main",
];

export const BUDGET_BRANCH_NAMES = [
  ...Array.from({ length: 129 }, (_, n) => `b${String(n).padStart(3, "0")}-${"x".repeat(58)}`),
  "main",
];

const branchValue = (name) => ({ name, base: "base1234567890ab", head: null });

function branchPage(names, pageSize, pagePos, replyBudget = null) {
  const items = [];
  let spent = 0;
  let hasMore = false;
  for (const name of names.slice(pagePos)) {
    const cost = Buffer.byteLength(name) + 512;
    if (items.length === pageSize || (replyBudget !== null && spent + cost > replyBudget)) {
      hasMore = true;
      break;
    }
    spent += cost;
    items.push(branchValue(name));
  }
  return { items, hasMore, nextPagePos: hasMore ? pagePos + items.length : null };
}

function branchCase(names, pageSize, pagePos, replyBudget = null) {
  const label = replyBudget === null ? `count=${names.length}` : "reply-budget";
  return {
    name: `P14 list_branches ${label} pageSize=${pageSize} pagePos=${pagePos}`,
    prepare(native) {
      native.branchList = names;
      native.branchReplyBudget = replyBudget;
    },
    sdk: (lf) => lf.project.list_branches(pageSize, pagePos),
    op: "project.list_branches",
    sent: { pageSize, pagePos },
    result: branchPage(names, pageSize, pagePos, replyBudget),
  };
}

const workspaceRecords = (count) => Array.from({ length: count }, (_, n) => ({
  workspace: `w${n}`,
  branch: `branch-${n}`,
  base: `base-${n}`,
  activity: "idle",
  mounted: true,
  directory: `/workspaces/w${n}`,
  dirty: { inodes: 0, entries: 0 },
  headMovedSinceMount: false,
  pendingMerge: false,
}));

function workspacePage(records, pageSize, pagePos) {
  const items = records.map((w) => ({
    sandbox: "1",
    path: w.directory,
    branch: w.branch,
    base: w.base,
    activity: "Idle",
    changed: false,
    workspace: w.workspace,
    state: w.activity,
    dirty: w.dirty,
    headMovedSinceMount: w.headMovedSinceMount,
    pendingMerge: w.pendingMerge,
  }));
  const end = pagePos + pageSize;
  return {
    items: items.slice(pagePos, end),
    hasMore: end < items.length,
    nextPagePos: end < items.length ? end : null,
    version: "1",
  };
}

function workspaceCase(records, pageSize, pagePos) {
  return {
    name: `P14 list_workspaces count=${records.length} pageSize=${pageSize} pagePos=${pagePos}`,
    prepare(native) {
      native.workspaceList = records;
    },
    sdk: (lf) => lf.sandbox.list_workspaces("1", pageSize, pagePos),
    op: "sandbox.list_workspaces",
    sent: { sandbox: "1" },
    result: workspacePage(records, pageSize, pagePos),
  };
}

const branches = [];
for (const count of [1, 2, 3, 64, 65, 66, 130]) {
  const names = branchNames(count);
  for (const pageSize of [1, 2, 5, 50]) {
    for (const pagePos of [0, 1, count - 1, count, count + 10]) {
      branches.push(branchCase(names, pageSize, pagePos));
    }
  }
}
const firstBudgetPage = branchPage(BUDGET_BRANCH_NAMES, 1000, 0, 48 * 1024);
branches.push(branchCase(BUDGET_BRANCH_NAMES, 1000, 0, 48 * 1024));
branches.push(branchCase(BUDGET_BRANCH_NAMES, 1000, firstBudgetPage.items.length, 48 * 1024));

const workspaces = [];
for (const count of [0, 64, 65, 129]) {
  const records = workspaceRecords(count);
  for (const pageSize of [1, 2, 50]) {
    for (const pagePos of [0, 63, 64, 65, count, count + 5]) {
      workspaces.push(workspaceCase(records, pageSize, pagePos));
    }
  }
}

// P1 cases (branch.graph + branch.changed_paths), merged in with the P14 slice.

const graphResult = (branch, pagePos) => ({
  branch: { name: branch, id: "branch1234567890ab", base: "base1234567890ab", head: "head1234567890ab", forkedFrom: "c0ffee" },
  forkPoint: { kind: "commit", id: "c0ffee" },
  commits: [{ id: "decaf", branch, parent: "parent1234567890", author: null, labels: [], logical: 120, newCanonical: 40, newStored: 30, newMetadata: 8, published: true }],
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

const WS = { sandbox: "1", path: "/workspaces/w1" };
const attributedResult = (author, labels) => ({
  kind: "Committed",
  head: "c0ffee",
  commit: { id: "c0ffee", author, labels, logical: 120, newCanonical: 40, newStored: 30, newMetadata: 8 },
});

export const P7_CASES = [
  {
    name: "P7 workspace commit author and labels",
    argv: ["workspace", "commit", "1:/workspaces/w1", "-m", "attributed", "--author", "agent-p7", "--labels", '["call-7","call-7","last"]'],
    sdk: (lf) => lf.workspace.commit(WS, "attributed", false, { author: "agent-p7", labels: ["call-7", "call-7", "last"] }),
    op: "workspace.commit",
    sent: { ...WS, message: "attributed", force: false, author: "agent-p7", labels: ["call-7", "call-7", "last"] },
    result: attributedResult("agent-p7", ["call-7", "call-7", "last"]),
  },
  {
    name: "P7 workspace commit empty labels",
    argv: ["workspace", "commit", "1:/workspaces/w1", "--author", "agent-empty", "--labels", "[]"],
    sdk: (lf) => lf.workspace.commit(WS, undefined, false, { author: "agent-empty", labels: [] }),
    op: "workspace.commit",
    sent: { ...WS, force: false, author: "agent-empty", labels: [] },
    result: attributedResult("agent-empty", []),
  },
];

export const CASES = [...branches, ...workspaces, ...P1_CASES, ...P7_CASES];
