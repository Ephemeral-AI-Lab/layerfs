// A mountless file and history flow in a throwaway home (Disposable, no
// Sandbox, no machine, nothing mounted): a Store, a project from a folder, a
// Branch, three writes (text, bytes, a batch), the history and a diff, then
// the Branch published to `main`. From the repository, after building the
// addon (`node api/scripts/build-native.mjs`):
//   node api/node/examples/file-history.mjs
// The temporary directory must be on ext4, xfs or btrfs (a Store needs SQLite
// locks); set TMPDIR to such a directory if /tmp is not. Installed, import
// from "layerfs" instead of "../src/index.js".
import { chmodSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { client, close } from "../src/index.js";

const root = mkdtempSync(join(tmpdir(), "layerfs-example-"));
chmodSync(root, 0o700);
try {
  const folder = join(root, "app");
  const home = join(root, "home");
  const lfs = client({ home });

  await lfs.store.create("shop");
  // the project is a copy of a host folder: make one with a README
  mkdirSync(folder, { mode: 0o700 });
  writeFileSync(join(folder, "README.md"), "# app\n");
  await lfs.project.init("shop/app", folder);

  await lfs.branch.fork("shop/app", "agent", "main");
  await lfs.file.write("shop/app", "agent", "/notes.md", "# notes\n", { message: "add notes" });
  await lfs.file.write("shop/app", "agent", "/logo.bin", Buffer.from([0, 1, 255]), { message: "add logo" });
  await lfs.file.apply("shop/app", "agent", [
    { op: "mkdir", path: "/src" },
    { op: "write", path: "/src/index.js", content: "export const answer = 42n;\n" },
  ], { message: "add source" });

  const history = await lfs.commit.list("shop/app", "agent");
  console.log("commits:", history.items.map((commit) => commit.message));
  console.log("read:", (await lfs.file.read("shop/app", "agent", "/src/index.js")).text.trim());
  const diff = await lfs.branch.diff("shop/app", "main", "agent", { paths_only: true });
  console.log("changed:", diff.items.map((change) => `${change.kind} ${change.path}`));

  const published = await lfs.branch.publish("shop/app", "agent");
  console.log("published:", published.kind);
  console.log("on main:", (await lfs.file.stat("shop/app", "main", "/notes.md")).size, "bytes in /notes.md");
} finally {
  close();
  rmSync(root, { recursive: true, force: true });
}
