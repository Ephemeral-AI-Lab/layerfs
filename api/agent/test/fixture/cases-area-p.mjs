// P14 cases in the shared op/sent/result triple shape. `prepare` selects the
// stand-in's data; `sdk` makes the one binding call described by the triple.

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

export const CASES = [...branches, ...workspaces];
