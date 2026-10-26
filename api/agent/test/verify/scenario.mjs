// The real-Store scenario shared by the SDK and `layerfs` CLI checks (#428):
// every agent call, made once against a real Store, Docker and daemon.
// Disposable Store only. Each part is a list of steps run in order on a fresh
// project; each step is one call:
// - `sdk(lf, ctx)` makes it through the SDK and `argv(ctx)` is the same call
//   as a `layerfs` command line, with `input` its stdin; a `stream` step's
//   answer is the exit code and streamed output, not a result;
// - `check(result, ctx)` asserts what a correct answer is; a step with
//   `refused` expects a LayerFSError with that code instead;
// - `keep(result, ctx)` records what later steps name (commit ids, the
//   Workspace), so each run uses its own.
// calls_real.mjs runs a part through the SDK and checks it (correctness);
// api/cli/test/verify/cli_real.mjs runs it through the SDK and then the command
// and requires the same answers, step by step (consistency).
import assert from "node:assert/strict";
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { Writable } from "node:stream";

/** The project source both runs Init from. */
export function source(dir) {
  mkdirSync(join(dir, "src"), { recursive: true });
  writeFileSync(join(dir, "README.md"), "# Shop\n");
  writeFileSync(join(dir, "src", "app.js"), "const a = 1;\n// TODO: cart\nexport default a;\n");
  return dir;
}

const handle = (ctx) => {
  if (!ctx.ws) throw new Error("no Workspace: the mount failed");
  return `${ctx.ws.sandbox}:${ctx.ws.path}`;
};
/** workspace.exec with sinks, as `layerfs workspace exec --stream` runs it:
 * the exit code and the bytes each sink received, as text. */
async function streamed(lf, ws, command) {
  const sink = (chunks) => new Writable({ write(chunk, encoding, done) { chunks.push(Buffer.from(chunk)); done(); } });
  const [out, err] = [[], []];
  const result = await lf.workspace.exec(ws, command, undefined, { stdout: sink(out), stderr: sink(err) });
  return { exitCode: result.exitCode, stdout: Buffer.concat(out).toString(), stderr: Buffer.concat(err).toString() };
}
// over the binding's 1 MiB inline output, so stdout is spooled and paged
const BIG = 2 * 1024 * 1024 + 1;
const STREAM = `head -c ${BIG} /dev/zero | tr '\\0' a; echo no >&2; exit 4`;
const kind = (want) => (result) => assert.equal(result.kind, want);
const lastCommit = (key) => (result, ctx) => { ctx[key] = result.head; };
const init = {
  name: "init", init: true,
  argv: (ctx) => ["init", ctx.project, ctx.source],
  check: (r, ctx) => assert.deepEqual([r.kind, r.project], ["Initialized", ctx.project]),
};

/** Store calls: projects, Branches, files and history; no Sandbox. */
export const STORE = [
  init,
  { name: "project list_branches", sdk: (lf) => lf.project.list_branches(), argv: () => ["project", "list_branches"],
    check: (r) => assert.deepEqual(r.items.map((b) => b.name), ["main"]) },
  { name: "branch fork", sdk: (lf) => lf.branch.fork("cart"), argv: () => ["branch", "fork", "cart"],
    check: (r) => assert.deepEqual([r.kind, r.branch], ["Forked", "cart"]) },
  { name: "fs write", sdk: (lf) => lf.fs.write("cart", "/notes.txt", "one\ntwo\n", "notes"), argv: () => ["fs", "write", "cart", "/notes.txt", "-m", "notes"], input: "one\ntwo\n",
    check: kind("Committed"), keep: lastCommit("first") },
  { name: "fs read", sdk: (lf) => lf.fs.read("cart", "/notes.txt"), argv: () => ["fs", "read", "cart", "/notes.txt"],
    check: (r) => assert.equal(r.text, "one\ntwo\n") },
  { name: "fs list", sdk: (lf) => lf.fs.list("cart"), argv: () => ["fs", "list", "cart"],
    check: (r) => assert.deepEqual(r.items.map((e) => e.name).sort(), ["README.md", "notes.txt", "src"]) },
  { name: "fs stat", sdk: (lf) => lf.fs.stat("cart", "/README.md"), argv: () => ["fs", "stat", "cart", "/README.md"],
    check: (r) => assert.equal(r.size, 7) },
  { name: "fs grep", sdk: (lf) => lf.fs.grep("cart", "TODO"), argv: () => ["fs", "grep", "cart", "TODO"],
    check: (r) => assert.deepEqual(r.items.map((h) => [h.path, h.line]), [["/src/app.js", 2]]) },
  { name: "fs glob", sdk: (lf) => lf.fs.glob("cart", "**/*.js"), argv: () => ["fs", "glob", "cart", "**/*.js"],
    check: (r) => assert.deepEqual(r.items, ["/src/app.js"]) },
  { name: "fs edit", sdk: (lf) => lf.fs.edit("cart", "/notes.txt", "two", "deux", "edit"), argv: () => ["fs", "edit", "cart", "/notes.txt", "--old", "two", "--new", "deux", "-m", "edit"],
    check: kind("Committed") },
  { name: "fs apply", sdk: (lf) => lf.fs.apply("cart", [{ op: "write", path: "/a.txt", content: "a\n" }, { op: "write", path: "/b.txt", content: "b\n" }], "apply"),
    argv: () => ["fs", "apply", "cart", "-m", "apply"], input: JSON.stringify([{ op: "write", path: "/a.txt", content: "a\n" }, { op: "write", path: "/b.txt", content: "b\n" }]),
    check: kind("Committed") },
  { name: "fs mkdir", sdk: (lf) => lf.fs.mkdir("cart", "/docs", "mkdir"), argv: () => ["fs", "mkdir", "cart", "/docs", "-m", "mkdir"], check: kind("Committed") },
  { name: "fs cp", sdk: (lf) => lf.fs.cp("cart", "/README.md", "/docs/README.md", "cp"), argv: () => ["fs", "cp", "cart", "/README.md", "/docs/README.md", "-m", "cp"], check: kind("Committed") },
  { name: "fs mv", sdk: (lf) => lf.fs.mv("cart", "/a.txt", "/c.txt", "mv"), argv: () => ["fs", "mv", "cart", "/a.txt", "/c.txt", "-m", "mv"], check: kind("Committed") },
  { name: "fs rm", sdk: (lf) => lf.fs.rm("cart", "/b.txt", "rm"), argv: () => ["fs", "rm", "cart", "/b.txt", "-m", "rm"], check: kind("Committed"), keep: lastCommit("last") },
  { name: "fs read (after the edits)", sdk: (lf) => lf.fs.read("cart", "/notes.txt"), argv: () => ["fs", "read", "cart", "/notes.txt"],
    check: (r) => assert.equal(r.text, "one\ndeux\n") },
  { name: "fs diff_files", sdk: (lf) => lf.fs.diff_files("main", "/README.md", "cart", "/docs/README.md"), argv: () => ["fs", "diff_files", "main", "/README.md", "cart", "/docs/README.md"],
    check: (r) => assert.deepEqual([r.kind, r.contentEqual, r.hunks], ["Equal", true, []], "a copy has no differences") },
  { name: "fs blame", sdk: (lf) => lf.fs.blame("cart", "/notes.txt"), argv: () => ["fs", "blame", "cart", "/notes.txt"],
    check: (r) => assert.deepEqual(r.lines.map((l) => [l.n, l.text, l.message]), [[1, "one", "notes"], [2, "deux", "edit"]]) },
  { name: "branch list_commits", sdk: (lf) => lf.branch.list_commits("cart"), argv: () => ["branch", "list_commits", "cart"],
    check: (r) => assert.deepEqual(r.items.map((c) => c.message).slice(0, 7), ["rm", "mv", "cp", "mkdir", "apply", "edit", "notes"]) },
  { name: "branch diff_branches", sdk: (lf) => lf.branch.diff_branches("main", "cart"), argv: () => ["branch", "diff_branches", "main", "cart"],
    check: (r) => assert.deepEqual(r.items.map((d) => d.path).sort(), ["/c.txt", "/docs", "/docs/README.md", "/notes.txt"]) },
  { name: "branch diff_commits", sdk: (lf, ctx) => lf.branch.diff_commits(ctx.first, ctx.last), argv: (ctx) => ["branch", "diff_commits", ctx.first, ctx.last],
    check: (r) => assert.deepEqual(r.items.map((d) => d.path).sort(), ["/c.txt", "/docs", "/docs/README.md", "/notes.txt"]) },
  { name: "branch fork (side)", sdk: (lf) => lf.branch.fork("side"), argv: () => ["branch", "fork", "side"], check: kind("Forked") },
  { name: "fs write (side)", sdk: (lf) => lf.fs.write("side", "/side.txt", "side\n", "side"), argv: () => ["fs", "write", "side", "/side.txt", "-m", "side"], input: "side\n", check: kind("Committed") },
  { name: "branch merge", sdk: (lf) => lf.branch.merge("cart", "side", "both", "merge"), argv: () => ["branch", "merge", "cart", "side", "both", "-m", "merge"],
    check: (r) => assert.deepEqual([r.kind, r.branch], ["Merged", "both"]) },
  { name: "fs list (merged)", sdk: (lf) => lf.fs.list("both"), argv: () => ["fs", "list", "both"],
    check: (r) => assert.deepEqual(r.items.map((e) => e.name).sort(), ["README.md", "c.txt", "docs", "notes.txt", "side.txt", "src"]) },
  { name: "branch publish", sdk: (lf, ctx) => lf.branch.publish("cart", ctx.last), argv: (ctx) => ["branch", "publish", "cart", ctx.last],
    check: kind("Added") },
  { name: "project list_sandboxes", sdk: (lf) => lf.project.list_sandboxes(), argv: () => ["project", "list_sandboxes"],
    check: (r) => assert.deepEqual(r.items, []) },
  { name: "fs read (missing)", sdk: (lf) => lf.fs.read("cart", "/nope"), argv: () => ["fs", "read", "cart", "/nope"], refused: "NoSuchPath" },
];

/** Sandbox and Workspace calls: a Sandbox, one Workspace, Exec, Commit and close. */
export const WORKSPACE = [
  init,
  { name: "branch fork", sdk: (lf) => lf.branch.fork("lane"), argv: () => ["branch", "fork", "lane"], check: kind("Forked") },
  { name: "sandbox create", sdk: (lf) => lf.sandbox.create(), argv: () => ["sandbox", "create"],
    check: (r) => assert.deepEqual(r, { sandbox: "1", image: process.env.LAYERFS_IMAGE }), keep: (r, ctx) => { ctx.sandbox = r.sandbox; } },
  { name: "project list_sandboxes", sdk: (lf) => lf.project.list_sandboxes(), argv: () => ["project", "list_sandboxes"],
    check: (r) => assert.deepEqual(r.items.map((s) => [s.sandbox, s.workspaces]), [["1", 0]]) },
  { name: "workspace mount", sdk: (lf) => lf.workspace.mount("1", "lane"), argv: () => ["workspace", "mount", "1", "lane"],
    check: (r) => assert.deepEqual([r.sandbox, r.image], ["1", process.env.LAYERFS_IMAGE]),
    keep: (r, ctx) => { ctx.ws = { sandbox: r.sandbox, path: r.path }; } },
  { name: "workspace exec", sdk: (lf, ctx) => lf.workspace.exec(ctx.ws, "cat README.md && echo two > out.txt", "step 1"),
    argv: (ctx) => ["workspace", "exec", handle(ctx), "cat README.md && echo two > out.txt", "-m", "step 1"],
    check: (r) => assert.deepEqual([r.exitCode, r.stdout, r.stderr], [0, "# Shop\n", ""]) },
  { name: "workspace exec (exit 3)", sdk: (lf, ctx) => lf.workspace.exec(ctx.ws, "echo no >&2; exit 3"), argv: (ctx) => ["workspace", "exec", handle(ctx), "echo no >&2; exit 3"],
    check: (r) => assert.deepEqual([r.exitCode, r.stdout, r.stderr], [3, "", "no\n"]) },
  { name: "workspace exec --stream", stream: true, sdk: (lf, ctx) => streamed(lf, ctx.ws, STREAM), argv: (ctx) => ["workspace", "exec", handle(ctx), STREAM, "--stream"],
    check: (r) => assert.deepEqual([r.exitCode, r.stdout.length, /^a*$/.test(r.stdout), r.stderr], [4, BIG, true, "no\n"]) },
  { name: "sandbox list_workspaces", sdk: (lf) => lf.sandbox.list_workspaces("1"), argv: () => ["sandbox", "list_workspaces", "1"],
    check: (r, ctx) => assert.deepEqual(r.items.map((w) => [w.path, w.branch, w.changed]), [[ctx.ws.path, "lane", true]]) },
  { name: "workspace commit", sdk: (lf, ctx) => lf.workspace.commit(ctx.ws, "step 1"), argv: (ctx) => ["workspace", "commit", handle(ctx), "-m", "step 1"],
    check: kind("Committed") },
  { name: "fs read (committed)", sdk: (lf) => lf.fs.read("lane", "/out.txt"), argv: () => ["fs", "read", "lane", "/out.txt"],
    check: (r) => assert.equal(r.text, "two\n") },
  { name: "workspace unmount", sdk: (lf, ctx) => lf.workspace.unmount(ctx.ws), argv: (ctx) => ["workspace", "unmount", handle(ctx)],
    check: kind("Unmounted"), keep: (r, ctx) => { ctx.ws = null; } },
  { name: "sandbox close", sdk: (lf) => lf.sandbox.close("1"), argv: () => ["sandbox", "close", "1"],
    check: (r) => assert.deepEqual(r, { kind: "Closed", sandbox: "1" }), keep: (r, ctx) => { ctx.closed = true; } },
  { name: "project list_sandboxes (closed)", sdk: (lf) => lf.project.list_sandboxes(), argv: () => ["project", "list_sandboxes"],
    check: (r) => assert.deepEqual(r.items, []) },
  { name: "workspace mount (closed Sandbox)", sdk: (lf) => lf.workspace.mount("1", "lane"), argv: () => ["workspace", "mount", "1", "lane"], refused: "NoSuchSandbox" },
];

export const PARTS = { store: STORE, workspace: WORKSPACE };

/** Checks one step's answer (its result, or the refusal it expects) and
 * keeps what later steps name. A wrong answer is returned as text with the
 * answer itself, so a run reports every wrong step, not only the first. */
export function verify(step, answer, ctx) {
  const got = () => (answer.error ? `${answer.error.code} ${answer.error.message}` : JSON.stringify(answer.result)?.slice(0, 2000));
  try {
    if (step.refused) {
      assert.equal(answer.error?.code, step.refused);
      return null;
    }
    assert.equal(answer.error, undefined);
    step.check(answer.result, ctx);
    step.keep?.(answer.result, ctx);
    return null;
  } catch (error) {
    return `${step.name}: ${error.generatedMessage ? "wrong answer" : error.message}\n  answer: ${got()}`;
  }
}

/** One part through the SDK in this process: each step's answer, and the
 * wrong ones (verify). */
export async function throughSdk(steps, LayerFS, ctx, plain = (v) => v) {
  const answers = [];
  const wrong = [];
  try {
    for (const step of steps) {
      let answer;
      try {
        answer = { result: plain(step.init ? (ctx.lf = await LayerFS.init(ctx.project, ctx.source)).created : await step.sdk(ctx.lf, ctx)) };
      } catch (error) {
        answer = { error };
      }
      const failure = verify(step, answer, ctx);
      if (failure) wrong.push(failure);
      answers.push(answer);
    }
    return { answers, wrong };
  } finally {
    // one attempt each, reported, never retried
    if (ctx.ws) console.log("left mounted:", await ctx.lf.workspace.unmount(ctx.ws, true).then(() => "unmounted", (e) => e.code));
    if (ctx.sandbox && !ctx.closed) console.log("left running:", await ctx.lf.sandbox.close(ctx.sandbox).then(() => "closed", (e) => e.code));
    await ctx.lf?.close();
  }
}
