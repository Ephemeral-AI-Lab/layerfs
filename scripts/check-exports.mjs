import { execFile } from "node:child_process";
import { access, mkdtemp, mkdir, readFile, readdir, rm, writeFile } from "node:fs/promises";
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

async function filesBelow(directory, prefix = "") {
  const result = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const relative = prefix ? `${prefix}/${entry.name}` : entry.name;
    if (entry.isDirectory()) result.push(...await filesBelow(path.join(directory, entry.name), relative));
    else if (entry.isFile()) result.push(relative);
  }
  return result;
}
function sameFiles(left, right) { return JSON.stringify([...left].sort()) === JSON.stringify([...right].sort()); }

// This sentinel makes stale-output cleanup executable evidence instead of an
// assumption about the caller's workspace. The clean build must remove it.
const distDirectory = path.join(packageDirectory, "dist");
await mkdir(distDirectory, { recursive: true });
const staleSentinel = path.join(distDirectory, "__m0_stale_output_sentinel__.js");
await writeFile(staleSentinel, "throw new Error('stale output was packed');\n");
await rm(distDirectory, { recursive: true, force: true });
const tscCli = path.join(root, "node_modules", "typescript", "bin", "tsc");
await execute(process.execPath, [tscCli, "-p", path.join(packageDirectory, "tsconfig.json")], { cwd: root, windowsHide: true, maxBuffer: 8 * 1024 * 1024 });
try { await access(staleSentinel); throw new Error("clean export build retained its stale-output sentinel"); } catch (error) { if (error?.code !== "ENOENT") throw error; }

const sourceFiles = (await filesBelow(path.join(packageDirectory, "src"))).filter((name) => name.endsWith(".ts") && !name.endsWith(".d.ts"));
const expectedDistFiles = sourceFiles.flatMap((name) => { const stem = name.slice(0, -3); return [`${stem}.js`, `${stem}.js.map`, `${stem}.d.ts`, `${stem}.d.ts.map`]; }).sort();
const actualDistFiles = (await filesBelow(distDirectory)).sort();
if (!sameFiles(actualDistFiles, expectedDistFiles)) {
  const unexpected = actualDistFiles.filter((name) => !expectedDistFiles.includes(name));
  const missing = expectedDistFiles.filter((name) => !actualDistFiles.includes(name));
  throw new Error(`clean dist does not correspond exactly to src (unexpected: ${unexpected.join(", ") || "none"}; missing: ${missing.join(", ") || "none"})`);
}

const temporary = await mkdtemp(path.join(tmpdir(), "ephemeral-ai-fs-exports-"));
try {
  const packDirectory = path.join(temporary, "pack"); const consumer = path.join(temporary, "consumer");
  await mkdir(packDirectory); await mkdir(consumer);
  const shell = process.platform === "win32";
  const dryRun = await execute("pnpm", ["pack", "--dry-run", "--json"], { cwd: packageDirectory, windowsHide: true, shell, maxBuffer: 8 * 1024 * 1024 });
  const dryRunManifest = JSON.parse(dryRun.stdout); const packedFiles = dryRunManifest.files.map((entry) => entry.path.replaceAll("\\", "/")).sort();
  const approvedPackageFiles = ["package.json"];
  for (const asset of ["README.md", "LICENSE"]) { try { await access(path.join(packageDirectory, asset)); approvedPackageFiles.push(asset); } catch (error) { if (error?.code !== "ENOENT") throw error; } }
  const expectedPackedFiles = [...expectedDistFiles.map((name) => `dist/${name}`), ...approvedPackageFiles].sort();
  if (!sameFiles(packedFiles, expectedPackedFiles)) {
    const unexpected = packedFiles.filter((name) => !expectedPackedFiles.includes(name)); const missing = expectedPackedFiles.filter((name) => !packedFiles.includes(name));
    throw new Error(`packed file list differs from clean source artifact (unexpected: ${unexpected.join(", ") || "none"}; missing: ${missing.join(", ") || "none"})`);
  }
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
  console.log(`exports: clean ${actualDistFiles.length}-file dist matches source; stale sentinel removed; ${packedFiles.length}-file tarball passes runtime/type consumers and rejects internal deep imports`);
} finally {
  await rm(temporary, { recursive: true, force: true });
}
