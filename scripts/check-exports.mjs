import { execFile } from "node:child_process";
import { mkdtemp, mkdir, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { promisify } from "node:util";

const execute = promisify(execFile);
const root = path.resolve(import.meta.dirname, "..");
const packageDirectory = path.join(root, "packages", "fs");
const expected = [".", "./integrations/node-vfs", "./integrations/replication", "./sqlite-driver"].sort();
const manifest = JSON.parse(await readFile(path.join(packageDirectory, "package.json"), "utf8"));
const actual = Object.keys(manifest.exports).sort();
if (JSON.stringify(actual) !== JSON.stringify(expected)) throw new Error(`core exports changed: ${actual.join(", ")}`);

const temporary = await mkdtemp(path.join(tmpdir(), "ephemeral-ai-fs-exports-"));
try {
  const packDirectory = path.join(temporary, "pack"); const consumer = path.join(temporary, "consumer");
  await mkdir(packDirectory); await mkdir(consumer);
  const shell = process.platform === "win32";
  await execute("pnpm", ["pack", "--pack-destination", packDirectory], { cwd: packageDirectory, windowsHide: true, shell, maxBuffer: 8 * 1024 * 1024 });
  const archives = (await readdir(packDirectory)).filter((name) => name.endsWith(".tgz"));
  if (archives.length !== 1) throw new Error(`expected one packed tarball, found ${archives.length}`);
  const archive = path.join(packDirectory, archives[0]);
  await writeFile(path.join(consumer, "package.json"), JSON.stringify({ private: true, type: "module" }));
  await execute("npm", ["install", "--ignore-scripts", "--no-audit", "--no-fund", "--package-lock=false", archive], { cwd: consumer, windowsHide: true, shell, maxBuffer: 8 * 1024 * 1024 });

  await writeFile(path.join(consumer, "runtime.mjs"), `
const root = await import("@ephemeralai/fs");
if (typeof root.EphemeralFS !== "function" || typeof root.FilesystemError !== "function") throw new Error("root exports are incomplete");
await import("@ephemeralai/fs/sqlite-driver");
await import("@ephemeralai/fs/integrations/replication");
const nodeVfs = await import("@ephemeralai/fs/integrations/node-vfs");
if (typeof nodeVfs.createNodeVfsBridge !== "function") throw new Error("Node VFS bridge export is incomplete");
const forbidden = [
  "cas/sha256", "cdc/fastcdc", "cow/pages", "manifests/codec",
  "sqlite/schema", "sqlite/content-repository", "sqlite/unit-of-work"
];
for (const suffix of forbidden) {
  try { await import(\`@ephemeralai/fs/\${suffix}\`); }
  catch (error) { if (error?.code === "ERR_PACKAGE_PATH_NOT_EXPORTED") continue; throw error; }
  throw new Error(\`deep import unexpectedly resolved: \${suffix}\`);
}
`);
  await execute(process.execPath, [path.join(consumer, "runtime.mjs")], { cwd: consumer, windowsHide: true, maxBuffer: 8 * 1024 * 1024 });

  await writeFile(path.join(consumer, "consumer.ts"), `
import { EphemeralFS, FilesystemError, type EphemeralFilesystem } from "@ephemeralai/fs";
import type { FilesystemSQLiteDriver } from "@ephemeralai/fs/sqlite-driver";
import type { ReplicationPlan } from "@ephemeralai/fs/integrations/replication";
import { createNodeVfsBridge, type NodeVfsFilesystemBridge } from "@ephemeralai/fs/integrations/node-vfs";
declare const driver: FilesystemSQLiteDriver;
const open: (options: Parameters<typeof EphemeralFS.open>[0]) => Promise<EphemeralFilesystem> = EphemeralFS.open;
const plan: ReplicationPlan = { pullMain: true };
const bridge: NodeVfsFilesystemBridge = createNodeVfsBridge({ database: driver });
void FilesystemError; void open; void plan; void bridge;
`);
  await writeFile(path.join(consumer, "tsconfig.json"), JSON.stringify({ compilerOptions: { strict: true, noEmit: true, target: "ES2022", module: "NodeNext", moduleResolution: "NodeNext", lib: ["ES2022", "ESNext.Disposable", "DOM", "DOM.Iterable"] }, files: ["consumer.ts"] }));
  const tsc = path.join(root, "node_modules", ".bin", "tsc");
  await execute(tsc, ["-p", path.join(consumer, "tsconfig.json")], { cwd: consumer, windowsHide: true, shell, maxBuffer: 8 * 1024 * 1024 });
  console.log("exports: packed tarball passes clean runtime/type consumers and rejects internal deep imports");
} finally {
  await rm(temporary, { recursive: true, force: true });
}
