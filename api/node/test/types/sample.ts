// Type-checks the generated index.d.ts; it is never run. From a directory
// outside the repository with `typescript` and `@types/node` installed:
//   tsc --noEmit --strict --module nodenext --moduleResolution nodenext --target es2022 \
//     --typeRoots <that dir>/node_modules/@types api/node/test/types/sample.ts
import layerfs, { client, doctor, close, table, LayerFSError, type Result, type FileReadOptions } from "../../src/index.js";

const lfs = client({ home: "/tmp/home", scope: { project: "shop/api", folder: "/web", branch: "agent" } });

async function main(): Promise<void> {
  await layerfs.store.create("shop");
  const progress: unknown[] = [];
  await layerfs.project.init("shop/api", "/src/api", { timeout: 5, on_progress: (p) => progress.push(p) });
  const page: Result = await lfs.project.list();
  await lfs.project.list("shop", { page_size: 10, page_pos: 2n });
  await lfs.branch.fork("shop/api", "agent", "main", "abc");
  await lfs.branch.fork("shop/api", "agent", "main", null);
  const options: FileReadOptions = { start_line: 2n, end_line: 5 };
  await lfs.file.read("shop/api", "main", "/README.md", null, options);
  await lfs.file.read("shop/api", "main", "/README.md", "commit-id", { start_line: 1 });
  // the options may follow the required positionals directly
  await lfs.file.read("shop/api", "main", "/README.md", options);
  await lfs.file.write("shop/api", "agent", "/a.bin", Buffer.from([0, 1, 255]), { message: "bytes", mode: 0o644 });
  await lfs.file.write("shop/api", "agent", "/a.txt", "text");
  await lfs.file.apply("shop/api", "agent", [{ op: "write", path: "/a", content: new Uint8Array([0, 255]) }], { message: "m" });
  await lfs.workspace.exec("ws", "ls", { stream: true, stdout: (bytes) => process.stdout.write(bytes), stderr: (bytes) => void bytes });
  await lfs.machine.create("vm", "lima", { cpus: 2, memory: "8G", disk: 8589934592n });
  await lfs.backup.create("shop", "bucket", "file", { on_progress: () => {} });
  const report = await doctor({ home: "/tmp/home" });
  const worst: string | undefined = report.findings[0]?.severity;
  const closed: number = close();
  const first = table[0];
  void [page, worst, closed, first.family, progress];
  try {
    await lfs.store.list();
  } catch (error) {
    if (error instanceof LayerFSError) void [error.code, error.message, error.detail];
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
