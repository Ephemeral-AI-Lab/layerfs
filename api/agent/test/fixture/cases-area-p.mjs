// Area P SDK/CLI cases in the shared op/sent/result triple shape. `prepare`
// selects the stand-in's data; `sdk` makes the one binding call in the triple.

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
  nextCursor: null,
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
  {
    name: "P1 branch changed_paths cursor",
    argv: ["branch", "changed_paths", "c0ffee", "--page-size", "5", "--cursor", "/changed.txt"],
    sdk: (lf) => lf.branch.changed_paths("c0ffee", 5, 0, { cursor: "/changed.txt" }),
    op: "branch.changed_paths",
    sent: { commit: "c0ffee", pageSize: 5, pagePos: 0, cursor: "/changed.txt" },
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

export const P8_REFS = {
  commit: `12${"a".repeat(64)}`,
  layer: `32${"b".repeat(64)}`,
  unknownCommit: `12${"c".repeat(64)}`,
  unknownLayer: `32${"d".repeat(64)}`,
  badTag: `99${"e".repeat(64)}`,
  nonhex: "z".repeat(66),
};

const p8Text = (ref) => ref === P8_REFS.commit ? "v1\n" : "v2\n";
const p8Header = (ref, path) => ({ kind: "ok", snapshot: ref.slice(2, 14), root: `${ref === P8_REFS.commit ? "aa" : "bb"}`.repeat(33), path });
const p8Items = (path) => {
  const names = path === "/dir" ? [["inside.txt", "file"]] : [["binary.bin", "file"], ["dir", "dir"], ["file.txt", "file"], ["link", "symlink"], ["same.txt", "file"]];
  return names.map(([name, type], index) => ({ name, path: `${path === "/" ? "" : path}/${name}`, type, inode: index + 2 }));
};
const p8List = (ref, path, pageSize, pagePos) => {
  const items = p8Items(path).slice(pagePos, pagePos + pageSize);
  const hasMore = pagePos + items.length < p8Items(path).length;
  return { ...p8Header(ref, path), items, hasMore, nextPagePos: hasMore ? pagePos + items.length : null };
};
const p8Read = (ref, path, binary) => {
  const content = path === "/binary.bin" ? Buffer.from([0, 255, 1, 10]) : Buffer.from(path === "/same.txt" ? "same\n" : p8Text(ref));
  return { ...p8Header(ref, path), ...(binary ? { bytes: { base64: content.toString("base64") } } : { text: content.toString("utf8") }), startLine: 1, endLine: 1, lineTruncated: false, truncated: false, nextLine: null, size: content.length };
};
const p8ListCase = (ref, path, pageSize = 1, pagePos = 0) => ({
  name: `P8 list ${ref.slice(2, 14)} ${path}`,
  argv: ["fs", "list", ref, path, "--page-size", String(pageSize), "--page-pos", String(pagePos)],
  sdk: (lf) => lf.fs.list(ref, path, pageSize, pagePos),
  op: "fs.list",
  sent: { branch: ref, path, pageSize, pagePos },
  result: p8List(ref, path, pageSize, pagePos),
});
const p8StatCase = (ref) => ({
  name: `P8 stat ${ref.slice(2, 14)}`,
  argv: ["fs", "stat", ref, "/file.txt"],
  sdk: (lf) => lf.fs.stat(ref, "/file.txt"),
  op: "fs.stat",
  sent: { branch: ref, path: "/file.txt" },
  result: { ...p8Header(ref, "/file.txt"), type: "file", inode: 2, size: Buffer.byteLength(p8Text(ref)), mode: 420 },
});
const p8ReadCase = (name, ref, path, options = {}) => ({
  name: `P8 ${name}`,
  argv: ["fs", "read", ref, path, "--start-line", "1", "--end-line", "1", ...(options.binary ? ["--binary"] : [])],
  sdk: (lf) => lf.fs.read(ref, path, 1, 1, options),
  op: "fs.read",
  sent: { branch: ref, path, startLine: 1, endLine: 1, binary: options.binary ?? false },
  result: p8Read(ref, path, options.binary ?? false),
});
const p8Refusal = (name, ref, path, code, message, detail = null, options = {}) => ({
  name: `P8 ${name}`,
  argv: ["fs", "read", ref, path],
  sdk: (lf) => lf.fs.read(ref, path, undefined, undefined, options),
  op: "fs.read",
  sent: { branch: ref, path, binary: false },
  refused: { code, message: `${code}: ${message}`, detail },
});

export const P8_CASES = [
  p8ListCase(P8_REFS.commit, "/"),
  p8ListCase(P8_REFS.commit, "/dir"),
  p8ListCase(P8_REFS.layer, "/"),
  p8ListCase(P8_REFS.layer, "/dir"),
  p8StatCase(P8_REFS.commit),
  p8StatCase(P8_REFS.layer),
  p8ReadCase("read Commit text", P8_REFS.commit, "/file.txt"),
  p8ReadCase("read Layer text", P8_REFS.layer, "/file.txt"),
  p8ReadCase("read Commit binary", P8_REFS.commit, "/binary.bin", { binary: true }),
  p8ReadCase("read text as binary", P8_REFS.commit, "/file.txt", { binary: true }),
  p8Refusal("binary=false refuses binary", P8_REFS.commit, "/binary.bin", "Binary", "text contains NUL or invalid UTF-8", null, { binary: false }),
  p8Refusal("omitted binary refuses binary", P8_REFS.commit, "/binary.bin", "Binary", "text contains NUL or invalid UTF-8"),
  p8Refusal("unknown Branch", "p8-unknown-branch", "/file.txt", "NoSuchBranch", "Branch p8-unknown-branch does not exist"),
  p8Refusal("bad identity tag", P8_REFS.badTag, "/file.txt", "InvalidInput", `\`${P8_REFS.badTag}\` is not a Commit or Layer identity`),
  p8Refusal("66-nonhex is a Branch", P8_REFS.nonhex, "/file.txt", "NoSuchBranch", `Branch ${P8_REFS.nonhex} does not exist`),
  p8Refusal("unknown Commit", P8_REFS.unknownCommit, "/file.txt", "NoSuchCommit", `Commit ${P8_REFS.unknownCommit} does not exist`),
  p8Refusal("unknown Layer", P8_REFS.unknownLayer, "/file.txt", "NotFound", `Layer ${P8_REFS.unknownLayer} does not exist`),
  p8Refusal("missing path at Commit", P8_REFS.commit, "/missing", "NoSuchPath", `/missing does not exist at ${P8_REFS.commit.slice(2, 14)}`, { path: "/missing" }),
  p8Refusal("directory read", P8_REFS.commit, "/dir", "IsDirectory", "expected a regular file"),
  p8Refusal("symlink read", P8_REFS.commit, "/link", "NotFile", "expected a regular file"),
  p8Refusal("invalid path", P8_REFS.commit, "bad", "InvalidInput", "expected an absolute path without empty, dot or parent components"),
];

export const CASES = [...branches, ...workspaces, ...P1_CASES, ...P7_CASES, ...P8_CASES];
