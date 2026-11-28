// Type-checks the generated index.d.ts; it is never run. From a directory
// outside the repository with `typescript` and `@types/node` installed:
//   tsc --noEmit --strict --module nodenext --moduleResolution nodenext --target es2022 \
//     --typeRoots <that dir>/node_modules/@types api/node/test/drift/types-sample.ts
import layerfs, { client, close, table, LayerFSError, type Result, type FileReadOptions } from "../../src/index.js";

const lfs = client({ root: "/tmp/home", scope: { project: "shop/api", folder: "/web", branch: "agent" } });

async function main(): Promise<void> {
  await layerfs.store.create("shop");
  const progress: unknown[] = [];
  await layerfs.project.init("shop/api", "/src/api", { timeout: 5, on_progress: (p) => progress.push(p) });
  const page: Result = await lfs.project.list();
  await lfs.project.list({ store: "shop", limit: 10, cursor: "b3A6MQ" });
  await lfs.branch.fork("shop/api", "agent", "main", { at: "32ab9e01" });
  await lfs.branch.fork("shop/api", "agent", null);
  // an optional positional may be left off before the options
  await lfs.branch.fork("shop/api", "agent", { at: "32ab9e01" });
  await lfs.branch.merge("shop/api", "main", "agent", { into: "merged", message: "merge" });
  await lfs.commit.list("shop/api", "agent", { at: "32ab9e01", limit: 50, cursor: "b3A6MQ" });
  await lfs.commit.get("shop/api", "32ab9e01", { branch: "main", detail: true });
  await lfs.commit.diff("shop/api", "agent", { from: "main", path: "/README.md", with: ["patch"] });
  await layerfs.home.get({ with: ["platforms", "tree"] });
  await lfs.store.get("shop", { with: ["live", "stats"], log_tag: "run-1" });
  await lfs.file.history("shop/api", "agent", "/README.md", { at: "saved-id", limit: 10 });
  const options: FileReadOptions = { line: 2n, limit: 5 };
  await lfs.file.read("shop/api", "main", "/README.md", { at: "commit-id", line: 1 });
  // the options may follow the required positionals directly
  await lfs.file.read("shop/api", "main", "/README.md", options);
  await lfs.file.write("shop/api", "agent", "/a.bin", Buffer.from([0, 1, 255]), { message: "bytes", mode: "0644" });
  await lfs.file.write("shop/api", "agent", "/a.txt", "text");
  await lfs.file.apply("shop/api", "agent", [{ op: "write", path: "/a", content: new Uint8Array([0, 255]) }], { message: "m" });
  await lfs.workspace.exec("shop:dev", "ls", { stream: true, stdout: (bytes) => process.stdout.write(bytes), stderr: (bytes) => void bytes });
  await lfs.machine.create("vm", "lima", { cpus: 2, memory: "8G", disk: 8589934592n });
  await lfs.machine.images("lima1");
  await lfs.sandbox.create("shop/api", "dev", {
    machine: "lima1",
    image: "postgres:17-bookworm",
    on_progress: (p) => progress.push(p),
  });
  await lfs.machine.load_image("lima1", `sha256:${"a".repeat(64)}`, { on_progress: (p) => progress.push(p) });
  await lfs.machine.prune("lima1", null, { unused: true });
  await lfs.backup.create("shop", "bucket", { file: "file", on_progress: () => {} });
  const exported: Result = await lfs.project.export("shop/api", "/exports/api", {
    branch: "main", on_progress: (p) => progress.push(p),
  });
  await lfs.project.export("shop/api", "/exports/api-default");
  const cancelled: Result = await lfs.workspace.cancel("shop:dev", { exec: "exec-0123456789abcdef" });
  const closed: number = close();
  const first = table[0];
  void [page, closed, first.family, progress, exported, cancelled];
  try {
    await lfs.store.list();
  } catch (error) {
    if (error instanceof LayerFSError) void [error.error, error.code, error.message, error.fix, error.detail];
  }
}
void main;

// each of these must be refused by the types
// @ts-expect-error unknown option
void lfs.store.get("shop", { bogus: 1 });
// @ts-expect-error an unknown option in the slot after the required positionals
void lfs.file.read("shop/api", "main", "/README.md", { bogus: 1 });
// @ts-expect-error a required positional is missing
void lfs.file.read("shop/api", "main");
// @ts-expect-error a positional of the wrong type
void lfs.file.read("shop/api", "main", 5);
// @ts-expect-error a function option takes a function
void lfs.project.init("shop/api", "/src", { on_progress: 1 });
// @ts-expect-error store.list takes no positional
void lfs.store.list("x");
// @ts-expect-error no such family
void lfs.nothing;
// @ts-expect-error a size is a number or text
void lfs.machine.create("vm", "lima", { memory: true });
// @ts-expect-error a scope needs its project and folder
void client({ scope: { folder: "/web" } });
// @ts-expect-error export has no commit option
void lfs.project.export("shop/api", "/exports/api", { commit: "main" });
// @ts-expect-error a cancel's exec id is text
void lfs.workspace.cancel("shop:dev", { exec: 1 });
// @ts-expect-error a section this call does not have
void lfs.store.get("shop", { with: ["patch"] });
// @ts-expect-error the 1.x call is gone (commit.list)
void lfs.branch.commits("shop/api", "agent");
