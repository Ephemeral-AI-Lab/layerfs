// LayerFS 2.0.0 from Node: the mountless flow. Store, project, files, Branches,
// diff, merge, publish, history and refusals. No Sandbox, no Workspace, no
// machine.
//
// How it was run for this manual (see README.md in this folder for the output):
// on macOS with the worktree's debug addon (api/node/layerfs.node), from a
// scratch folder whose node_modules/@layerfs/node links to api/node:
//   TMPDIR=<scratch dir> node node-flow.mjs
// It makes its own Disposable home and project folder in a temporary directory
// and removes them at the end. On Linux the temporary directory must be on a
// filesystem with SQLite locks (ext4, xfs, btrfs).
import { chmodSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { LayerFSError, client, close } from "@layerfs/node";

const root = mkdtempSync(join(process.env.TMPDIR ?? tmpdir(), "layerfs-guide-"));
chmodSync(root, 0o700);
const say = (label, value) => console.log(`${label.padEnd(28)}${value}`);

async function refused(label, call) {
  try {
    await call();
    say(label, "NOT REFUSED (unexpected)");
  } catch (e) {
    if (!(e instanceof LayerFSError)) throw e;
    say(label, JSON.stringify({ error: e.error, message: e.message, fix: e.fix }));
  }
}

try {
  // the project is a copy of a host folder
  const folder = join(root, "site");
  mkdirSync(join(folder, "docs"), { recursive: true, mode: 0o700 });
  writeFileSync(join(folder, "README.md"), "# site\n");
  writeFileSync(join(folder, "docs", "a.txt"), "one\ntwo\n");

  const P = "shop/site";
  const lfs = client({ root });

  await lfs.store.create("shop");
  say("store.get", JSON.stringify(await lfs.store.get("shop")));
  const events = [];
  await lfs.project.init(P, folder, { on_progress: (p) => events.push(p.phase) });
  say("project.init phases", events.join(" "));

  // mountless file work on a Branch (main changes only through publish)
  await lfs.branch.fork(P, "agent", "main");
  await lfs.file.write(P, "agent", "/notes.md", "# notes\n", { message: "add notes" });
  say("file.read", JSON.stringify((await lfs.file.read(P, "agent", "/notes.md")).text));
  const listing = await lfs.file.list(P, "agent", "/");
  say("file.list /", listing.items.map((e) => `${e.path}(${e.type})`).join(" "));
  await lfs.file.edit(P, "agent", "/docs/a.txt", { old_string: "two", new_string: "three", message: "edit a" });
  say("file.read after edit", JSON.stringify((await lfs.file.read(P, "agent", "/docs/a.txt")).text));

  // two Branches change different files, then merge into a new Branch
  await lfs.branch.fork(P, "left", "main");
  await lfs.branch.fork(P, "right", "main");
  await lfs.file.edit(P, "left", "/docs/a.txt", { old_string: "one", new_string: "uno", message: "left edits a" });
  await lfs.file.write(P, "right", "/right.txt", "from right\n", { message: "right adds a file" });
  const merged = await lfs.branch.merge(P, "left", "right", { into: "merged", message: "merge left and right" });
  say("branch.merge", (await lfs.branch.get(P, "merged")).commit === merged.commit ? "merged is at the merge Commit" : "?");
  const diff = await lfs.commit.diff(P, "merged", { from: "main" });
  say("commit.diff main..merged", diff.items.map((c) => `${c.change} ${c.path}`).join(", "));
  say("file.read merged", JSON.stringify((await lfs.file.read(P, "merged", "/docs/a.txt")).text));

  // history, and main's next head
  const history = await lfs.commit.list(P, "merged");
  say("commit.list merged", history.items.map((c) => c.message).join(" | "));
  const branches = await lfs.branch.list(P);
  say("branch.list", branches.items.map((b) => b.branch).join(" "));
  await lfs.branch.publish(P, "merged");
  const page = await lfs.commit.list(P, "main", { limit: 1 });
  say("commit.list main --limit 1", JSON.stringify({ source_branch: page.items[0].source_branch, more: Boolean(page.next_cursor) }));

  // refusals: LayerFSError with error, message and fix
  await refused("missing Branch", () => lfs.file.read(P, "nope", "/README.md"));
  await refused("write to main", () => lfs.file.write(P, "main", "/x.txt", "x\n"));
  await refused("unknown option", () => lfs.file.read(P, "agent", "/README.md", { colour: true }));
  const scoped = client({ root, scope: { project: P, folder: "/docs", branch: "ns" } });
  await refused("scope: other call", () => scoped.store.list());
  await refused("scope: outside folder", () => scoped.file.read(P, "main", "/README.md"));
} finally {
  close();
  rmSync(root, { recursive: true, force: true });
}
