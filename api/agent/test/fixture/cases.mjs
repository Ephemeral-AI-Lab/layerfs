// The SDK call cases shared by the SDK and `layerfs` CLI tests, over the
// stand-in binding (fake-native.cjs). Each case is one call:
// - `sdk(lf, LayerFS)` makes it through the SDK (`lf` is an open client of
//   project "site"; Init uses `LayerFS`), `argv` is the same call as a
//   `layerfs` command line, `input` its stdin;
// - `op` is the binding call it makes and `sent` a subset of the arguments
//   it must send; `refuse` names ops the stand-in refuses (CLI_FAKE_REFUSE);
// - `result` is what the SDK returns, bytes as { base64 }, or `refused` the
//   LayerFSError it throws ({ code, message, detail }); a refusal the SDK
//   makes itself sends nothing (`op` null).
// agent/test/calls.mjs checks the SDK against `sent` and `result`
// (correctness). api/cli/test/cli.test.mjs runs each case through both and
// checks that the CLI sends and prints exactly what the SDK sends and
// returns (consistency).
const WS = { sandbox: "1", path: "/workspaces/w1" };
const HANDLE = "1:/workspaces/w1";
const commit = { id: "c0ffee", logical: 120, newCanonical: 40, newStored: 30, newMetadata: 8 };
const committed = { kind: "Committed", head: "c0ffee", commit };
const page = { items: [], hasMore: false, nextPagePos: null };

/** `value` with bytes as { base64 }, as the stand-in logs them and the CLI prints them. */
export function plain(value) {
  if (Buffer.isBuffer(value) || value instanceof Uint8Array) return { base64: Buffer.from(value).toString("base64") };
  if (Array.isArray(value)) return value.map(plain);
  if (value && typeof value === "object") return Object.fromEntries(Object.entries(value).map(([k, v]) => [k, plain(v)]));
  return value;
}

/** True when every field of `want` is in `have`, recursively. */
export function contains(have, want) {
  if (want === null || typeof want !== "object") return Object.is(have, want);
  if (have === null || typeof have !== "object") return false;
  return Object.entries(want).every(([k, v]) => contains(have[k], v));
}

/** The [op, args] a call made: without LayerFS.open's one-Branch probe. */
export const made = (calls) => calls.filter(([op, args]) => !op.startsWith("store.") && !(op === "project.list_branches" && args.pageSize === 1));

/** The fixed Store path and project the cases run against. */
export const ENV = {
  LAYERFS_HOME: "/private/cases-home", LAYERFS_DOCKER: "/fake.sock", LAYERFS_IMAGE: "fake-image",
  LAYERFS_VOLUME: "fake-volume", LAYERFS_DAEMON: "/fake/daemon", LAYERFS_STORE_PATH: "/fake/store.sqlite",
  LAYERFS_PROJECT: "site",
};

export const CASES = [
  { name: "init", argv: ["init", "site", "/fake/src", "--timeout", "5000"], sdk: async (lf, LayerFS) => (await LayerFS.init("site", "/fake/src", 5000)).created,
    op: "project.init", sent: { name: "site", path: "/fake/src", timeoutMs: 5000 },
    result: { kind: "Initialized", project: "site", layer: "lay0", root: "r00t", store: "/fake/store.sqlite" } },
  { name: "project list_branches", argv: ["project", "list_branches", "--page-size", "5", "--page-pos", "10"], sdk: (lf) => lf.project.list_branches(5, 10),
    op: "project.list_branches", sent: { pageSize: 5, pagePos: 10 }, result: { items: [{ name: "main", base: "layer0000000000aa", head: null }, { name: "cart", base: "base1234567890ab", head: "head1234567890ab" }], hasMore: false, nextPagePos: null } },
  { name: "project list_sandboxes", argv: ["project", "list_sandboxes"], sdk: (lf) => lf.project.list_sandboxes(),
    op: "project.list_sandboxes", sent: { project: "site" }, result: { items: [{ sandbox: "1", name: "layerfs-website-1", image: "fake-image", workspaces: 0 }], hasMore: false, nextPagePos: null } },
  { name: "branch fork", argv: ["branch", "fork", "cart"], sdk: (lf) => lf.branch.fork("cart"),
    op: "branch.fork", sent: { name: "cart", from: "main" }, result: { kind: "Forked", branch: "cart", base: "lay0", head: "not-a-publication" } },
  { name: "branch list_commits", argv: ["branch", "list_commits", "cart"], sdk: (lf) => lf.branch.list_commits("cart"),
    op: "branch.list_commits", sent: { branch: "cart", pageSize: 50, pagePos: 0 }, result: page },
  { name: "branch diff_commits", argv: ["branch", "diff_commits", "c1", "c2", "--no-hunks", "--cursor", "/a"], sdk: (lf) => lf.branch.diff_commits("c1", "c2", undefined, undefined, { cursor: "/a", hunks: false }),
    op: "branch.diff_commits", sent: { from: "c1", to: "c2", hunks: false, cursor: "/a" }, result: page },
  { name: "branch diff_branches", argv: ["branch", "diff_branches", "a", "b"], sdk: (lf) => lf.branch.diff_branches("a", "b"),
    op: "branch.diff_branches", sent: { from: "a", to: "b", pageSize: 50 }, result: page },
  { name: "branch diff_branches paths only", argv: ["branch", "diff_branches", "main", "cart", "--page-size", "200", "--no-hunks"], sdk: (lf) => lf.branch.diff_branches("main", "cart", 200, undefined, { hunks: false }),
    op: "branch.diff_branches", sent: { from: "main", to: "cart", pageSize: 200, hunks: false }, result: page },
  { name: "branch merge", argv: ["branch", "merge", "a", "b", "ab", "-m", "merge it", "--units", '["dist/**"]'], sdk: (lf) => lf.branch.merge("a", "b", "ab", "merge it", ["dist/**"]),
    op: "branch.merge", sent: { branch: "a", with: "b", name: "ab", message: "merge it", units: ["dist/**"] }, result: { kind: "Merged", branch: "ab", head: "m3rge", root: "r00t" } },
  { name: "branch publish", argv: ["branch", "publish", "cart", "c0ffee"], sdk: (lf) => lf.branch.publish("cart", "c0ffee"),
    op: "branch.publish", sent: { branch: "cart", commit: "c0ffee" }, result: { kind: "Added", source: { branch: "feature", commit: "pub1" }, publishedLayer: { id: "lay1" } } },
  { name: "branch publish head", argv: ["branch", "publish", "cart"], sdk: (lf) => lf.branch.publish("cart"),
    op: "branch.publish", sent: { branch: "cart" }, result: { kind: "Added", source: { branch: "feature", commit: "pub1" }, publishedLayer: { id: "lay1" } } },
  { name: "fs read", argv: ["fs", "read", "main", "/a.txt", "--start-line=3", "--end-line", "9"], sdk: (lf) => lf.fs.read("main", "/a.txt", 3, 9),
    op: "fs.read", sent: { branch: "main", path: "/a.txt", startLine: 3, endLine: 9 }, result: { text: "b\n", nextLine: null } },
  { name: "fs list", argv: ["fs", "list", "main"], sdk: (lf) => lf.fs.list("main"),
    op: "fs.list", sent: { branch: "main", path: "/", pageSize: 50, pagePos: 0 }, result: page },
  { name: "fs stat", argv: ["fs", "stat", "main", "/a"], sdk: (lf) => lf.fs.stat("main", "/a"),
    op: "fs.stat", sent: { branch: "main", path: "/a" }, result: { type: "file", size: 5, mode: 420 } },
  { name: "fs grep", argv: ["fs", "grep", "main", "TODO", "/src"], sdk: (lf) => lf.fs.grep("main", "TODO", "/src"),
    op: "fs.grep", sent: { branch: "main", pattern: "TODO", path: "/src" }, result: page },
  { name: "fs glob", argv: ["fs", "glob", "main", "**/*.js"], sdk: (lf) => lf.fs.glob("main", "**/*.js"),
    op: "fs.glob", sent: { branch: "main", pattern: "**/*.js", path: "/" }, result: page },
  { name: "fs diff_files", argv: ["fs", "diff_files", "main", "/a", "cart", "/b"], sdk: (lf) => lf.fs.diff_files("main", "/a", "cart", "/b"),
    op: "fs.diff_files", sent: { left: { commit: "main", path: "/a" }, right: { commit: "cart", path: "/b" } }, result: page },
  { name: "fs blame", argv: ["fs", "blame", "main", "/a", "--from-line", "4"], sdk: (lf) => lf.fs.blame("main", "/a", 4),
    op: "fs.blame", sent: { branch: "main", path: "/a", fromLine: 4 }, result: page },
  { name: "fs write", argv: ["fs", "write", "cart", "/h.bin", "-m", "bytes"], input: Buffer.from([0, 255, 1, 2]), sdk: (lf) => lf.fs.write("cart", "/h.bin", Buffer.from([0, 255, 1, 2]), "bytes"),
    op: "fs.write", sent: { branch: "cart", path: "/h.bin", bytes: { base64: "AP8BAg==" }, message: "bytes" }, result: committed },
  { name: "fs edit", argv: ["fs", "edit", "cart", "/a", "--old", "x", "--new", "y"], sdk: (lf) => lf.fs.edit("cart", "/a", "x", "y"),
    op: "fs.edit", sent: { branch: "cart", path: "/a", oldText: "x", newText: "y" }, result: committed },
  { name: "fs apply", argv: ["fs", "apply", "cart", "-m", "two"], input: '[{"op":"write","path":"/a","content":"hi"}]', sdk: (lf) => lf.fs.apply("cart", [{ op: "write", path: "/a", content: "hi" }], "two"),
    op: "fs.apply", sent: { branch: "cart", changes: [{ op: "write", path: "/a", content: "hi" }], message: "two" }, result: committed },
  { name: "fs mkdir", argv: ["fs", "mkdir", "cart", "/d"], sdk: (lf) => lf.fs.mkdir("cart", "/d"),
    op: "fs.mkdir", sent: { branch: "cart", path: "/d", recursive: true }, result: committed },
  { name: "fs write with a mode", argv: ["fs", "write", "cart", "/run.sh", "--mode", "755"], input: Buffer.from("#!/bin/sh\n"), sdk: (lf) => lf.fs.write("cart", "/run.sh", Buffer.from("#!/bin/sh\n"), { mode: 0o755 }),
    op: "fs.write", sent: { branch: "cart", path: "/run.sh", bytes: { base64: "IyEvYmluL3NoCg==" }, mode: 0o755 }, result: committed },
  { name: "fs mkdir with a mode and message", argv: ["fs", "mkdir", "cart", "/d", "--mode", "0o750", "-m", "private"], sdk: (lf) => lf.fs.mkdir("cart", "/d", { message: "private", mode: 0o750 }),
    op: "fs.mkdir", sent: { branch: "cart", path: "/d", recursive: true, mode: 0o750, message: "private" }, result: committed },
  { name: "fs rm", argv: ["fs", "rm", "cart", "/d"], sdk: (lf) => lf.fs.rm("cart", "/d"),
    op: "fs.rm", sent: { branch: "cart", path: "/d" }, result: { kind: "UpToDate", head: "h3ad" } },
  { name: "fs cp", argv: ["fs", "cp", "cart", "/a", "/b"], sdk: (lf) => lf.fs.cp("cart", "/a", "/b"),
    op: "fs.cp", sent: { branch: "cart", from: "/a", to: "/b" }, result: committed },
  { name: "fs mv", argv: ["fs", "mv", "cart", "/a", "/b", "-m", "move"], sdk: (lf) => lf.fs.mv("cart", "/a", "/b", "move"),
    op: "fs.mv", sent: { branch: "cart", from: "/a", to: "/b", message: "move" }, result: committed },
  { name: "workspace mount", argv: ["workspace", "mount", "1", "cart"], sdk: (lf) => lf.workspace.mount("1", "cart"),
    op: "workspace.mount", sent: { sandbox: "1", branch: "cart" }, result: { sandbox: "1", path: "/workspaces/w1", image: "fake-image" } },
  { name: "workspace exec", argv: ["workspace", "exec", HANDLE, "exit 7", "-m", "step"], sdk: (lf) => lf.workspace.exec(WS, "exit 7", "step"),
    op: "workspace.exec", sent: { ...WS, command: "exit 7", message: "step" }, result: { kind: "exit:7", exitCode: 7, stdout: "out:exit 7", stderr: "err", truncated: false } },
  { name: "workspace commit", argv: ["workspace", "commit", HANDLE, "--force"], sdk: (lf) => lf.workspace.commit(WS, undefined, true),
    op: "workspace.commit", sent: { ...WS, force: true }, result: committed },
  { name: "workspace unmount", argv: ["workspace", "unmount", HANDLE, "--discard"], sdk: (lf) => lf.workspace.unmount(WS, true),
    op: "workspace.unmount", sent: { ...WS, discard: true }, result: { kind: "Unmounted" } },
  { name: "workspace unmount (force)", argv: ["workspace", "unmount", HANDLE, "--force"], sdk: (lf) => lf.workspace.unmount(WS, false, true),
    op: "workspace.unmount", sent: { ...WS, force: true }, result: { kind: "ForceUnmounted", abort: "Written", detach: "Detached", cleanup: "Queued" } },
  { name: "sandbox create", argv: ["sandbox", "create", "--image", "sha256:abc"], sdk: (lf) => lf.sandbox.create({ image: "sha256:abc" }),
    op: "sandbox.create", sent: { deploy: { image: "sha256:abc" } }, result: { sandbox: "2", image: "sha256:abc" } },
  { name: "sandbox list_workspaces", argv: ["sandbox", "list_workspaces", "2"], sdk: (lf) => lf.sandbox.list_workspaces("2"),
    op: "sandbox.list_workspaces", sent: { sandbox: "2" }, result: { ...page, version: "1" } },
  { name: "sandbox close", argv: ["sandbox", "close", "2"], sdk: (lf) => lf.sandbox.close("2"),
    op: "sandbox.close", sent: { sandbox: "2" }, result: { kind: "Closed", sandbox: "2" } },
  // the first window, and refusals: the binding's, then one the SDK makes itself
  { name: "fs read (first window)", argv: ["fs", "read", "main", "/a"], sdk: (lf) => lf.fs.read("main", "/a"),
    op: "fs.read", sent: { branch: "main", path: "/a", startLine: 1 }, result: { text: "a\n", nextLine: 2 } },
  { name: "fs read (missing)", argv: ["fs", "read", "main", "/missing"], sdk: (lf) => lf.fs.read("main", "/missing"),
    op: "fs.read", sent: { path: "/missing" }, refused: { code: "NoSuchPath", message: "NoSuchPath: /missing does not exist", detail: null } },
  { name: "branch merge (refused)", argv: ["branch", "merge", "a", "b", "ab"], sdk: (lf) => lf.branch.merge("a", "b", "ab"), refuse: ["branch.merge"],
    op: "branch.merge", sent: { branch: "a", with: "b", name: "ab" }, refused: { code: "Conflict", message: "Conflict: branch.merge refused by the fake", detail: { op: "branch.merge" } } },
  { name: "workspace mount (Sandbox 0)", argv: ["workspace", "mount", "0", "cart"], sdk: (lf) => lf.workspace.mount("0", "cart"),
    op: null, refused: { code: "InvalidInput", message: 'InvalidInput: workspace.mount(sandbox, branch) takes a Sandbox number such as "1" (from sandbox.create() or project.list_sandboxes()) and a Branch name, and no options', detail: null } },
];
