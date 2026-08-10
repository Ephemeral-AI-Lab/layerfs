import { execFile } from "node:child_process";
import { access, mkdtemp, mkdir, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { promisify } from "node:util";

const execute = promisify(execFile);
const root = path.resolve(import.meta.dirname, "..");
const packagesRoot = path.join(root, "packages");
const expectedCoreExports = [".", "./integrations/node-vfs", "./integrations/replication", "./sqlite-driver"].sort();
const executable = (name) => process.platform === "win32" ? `${name}.cmd` : name;

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
function listDifference(left, right) { return left.filter((name) => !right.includes(name)); }

const publishablePackages = [];
for (const entry of (await readdir(packagesRoot, { withFileTypes: true })).sort((left, right) => left.name.localeCompare(right.name))) {
  if (!entry.isDirectory()) continue;
  const directory = path.join(packagesRoot, entry.name);
  const manifest = JSON.parse(await readFile(path.join(directory, "package.json"), "utf8"));
  if (manifest.private === true || !manifest.exports) continue;
  publishablePackages.push({ directory, manifest });
}
const core = publishablePackages.find((item) => item.manifest.name === "@ephemeralai/fs");
if (!core) throw new Error("missing publishable @ephemeralai/fs package");
const actualCoreExports = Object.keys(core.manifest.exports).sort();
if (!sameFiles(actualCoreExports, expectedCoreExports)) throw new Error(`core exports changed: ${actualCoreExports.join(", ")}`);

let totalDistFiles = 0;
const packageArtifacts = new Map();
for (const packageInfo of publishablePackages) {
  const distDirectory = path.join(packageInfo.directory, "dist");
  await mkdir(distDirectory, { recursive: true });
  const staleSentinel = path.join(distDirectory, "__m0_stale_output_sentinel__.js");
  await writeFile(staleSentinel, "throw new Error('stale output was packed');\n");
  await execute(executable("pnpm"), ["--filter", packageInfo.manifest.name, "build"], { cwd: root, windowsHide: true, shell: process.platform === "win32", maxBuffer: 8 * 1024 * 1024 });
  try { await access(staleSentinel); throw new Error(`${packageInfo.manifest.name} clean build retained its stale-output sentinel`); }
  catch (error) { if (error?.code !== "ENOENT") throw error; }

  const sourceFiles = (await filesBelow(path.join(packageInfo.directory, "src"))).filter((name) => (name.endsWith(".ts") || name.endsWith(".mts") || name.endsWith(".cts")) && !name.endsWith(".d.ts"));
  const expectedDistFiles = sourceFiles.flatMap((name) => {
    const extension = path.posix.extname(name);
    const stem = name.slice(0, -extension.length);
    const outputExtension = extension === ".mts" ? ".mjs" : extension === ".cts" ? ".cjs" : ".js";
    const declarationExtension = extension === ".mts" ? ".d.mts" : extension === ".cts" ? ".d.cts" : ".d.ts";
    return [`${stem}${outputExtension}`, `${stem}${outputExtension}.map`, `${stem}${declarationExtension}`, `${stem}${declarationExtension}.map`];
  }).sort();
  const actualDistFiles = (await filesBelow(distDirectory)).sort();
  if (!sameFiles(actualDistFiles, expectedDistFiles)) {
    throw new Error(`${packageInfo.manifest.name} clean dist differs from source (unexpected: ${listDifference(actualDistFiles, expectedDistFiles).join(", ") || "none"}; missing: ${listDifference(expectedDistFiles, actualDistFiles).join(", ") || "none"})`);
  }
  totalDistFiles += actualDistFiles.length;
  packageArtifacts.set(packageInfo.manifest.name, { expectedDistFiles, actualDistFiles });
}

await execute(process.execPath, [path.join(root, "scripts", "check-api-snapshots.mjs")], { cwd: root, windowsHide: true, maxBuffer: 8 * 1024 * 1024 });

const temporary = await mkdtemp(path.join(tmpdir(), "ephemeral-ai-fs-exports-"));
try {
  const packDirectory = path.join(temporary, "pack");
  const consumer = path.join(temporary, "consumer");
  await mkdir(packDirectory);
  await mkdir(consumer);
  let totalPackedFiles = 0;
  for (const packageInfo of publishablePackages) {
    const dryRun = await execute(executable("pnpm"), ["pack", "--dry-run", "--json"], { cwd: packageInfo.directory, windowsHide: true, shell: process.platform === "win32", maxBuffer: 8 * 1024 * 1024 });
    const dryRunManifest = JSON.parse(dryRun.stdout);
    const packedFiles = dryRunManifest.files.map((entry) => entry.path.replaceAll("\\", "/")).sort();
    const approvedPackageFiles = ["package.json"];
    for (const asset of ["README.md", "LICENSE"]) {
      try { await access(path.join(packageInfo.directory, asset)); approvedPackageFiles.push(asset); }
      catch (error) { if (error?.code !== "ENOENT") throw error; }
    }
    const expectedPackedFiles = [...packageArtifacts.get(packageInfo.manifest.name).expectedDistFiles.map((name) => `dist/${name}`), ...approvedPackageFiles].sort();
    if (!sameFiles(packedFiles, expectedPackedFiles)) {
      throw new Error(`${packageInfo.manifest.name} packed files differ from its clean source artifact (unexpected: ${listDifference(packedFiles, expectedPackedFiles).join(", ") || "none"}; missing: ${listDifference(expectedPackedFiles, packedFiles).join(", ") || "none"})`);
    }
    totalPackedFiles += packedFiles.length;
    await execute(executable("pnpm"), ["pack", "--pack-destination", packDirectory], { cwd: packageInfo.directory, windowsHide: true, shell: process.platform === "win32", maxBuffer: 8 * 1024 * 1024 });
  }
  const archives = (await readdir(packDirectory)).filter((name) => name.endsWith(".tgz")).map((name) => path.join(packDirectory, name));
  if (archives.length !== publishablePackages.length) throw new Error(`expected ${publishablePackages.length} packed tarballs, found ${archives.length}`);
  await writeFile(path.join(consumer, "package.json"), JSON.stringify({ private: true, type: "module" }));
  await execute(executable("npm"), ["install", "--ignore-scripts", "--no-audit", "--no-fund", "--package-lock=false", ...archives], { cwd: consumer, windowsHide: true, shell: process.platform === "win32", maxBuffer: 16 * 1024 * 1024 });

  await writeFile(path.join(consumer, "runtime.mjs"), `
const packageNames = ${JSON.stringify(publishablePackages.map((item) => item.manifest.name))};
for (const packageName of packageNames) {
  const loaded = await import(packageName);
  if (Object.keys(loaded).length === 0) throw new Error(\`packed package has no runtime exports: \${packageName}\`);
}
const root = await import("@ephemeralai/fs");
if (typeof root.EphemeralFS !== "function" || typeof root.FilesystemError !== "function") throw new Error("root exports are incomplete");
await import("@ephemeralai/fs/sqlite-driver");
await import("@ephemeralai/fs/integrations/replication");
const nodeVfs = await import("@ephemeralai/fs/integrations/node-vfs");
if (typeof nodeVfs.createNodeVfsBridge !== "function") throw new Error("Node VFS bridge export is incomplete");
const forbidden = [
  "cas/sha256", "cdc/fastcdc", "cow/pages", "patches/patches", "manifests/codec",
  "namespace/paths", "branches/types", "revisions/types", "operations/filesystem",
  "sqlite/schema", "sqlite/content-repository", "sqlite/unit-of-work", "resources/limits",
  "streams/types", "cache/content-cache", "maintenance/maintenance"
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
import { openNodeSqlite, type NodeSQLiteDriver } from "@ephemeralai/fs-sqlite-node";
import { CloudflareSQLiteDriver } from "@ephemeralai/fs-sqlite-cloudflare";
import { openNodeVfs, type NodeVfsHandle } from "@ephemeralai/fs-node-vfs";
import { REPLICATION_PROTOCOL_VERSION } from "@ephemeralai/fs-replication";
import { createRecordingFactory, type ConformanceAdapterFactory } from "@ephemeralai/fs-testkit";
declare const driver: FilesystemSQLiteDriver;
const open: (options: Parameters<typeof EphemeralFS.open>[0]) => Promise<EphemeralFilesystem> = EphemeralFS.open;
const plan: ReplicationPlan = { pullMain: true };
const bridge: NodeVfsFilesystemBridge = createNodeVfsBridge({ database: driver });
const nodeDriverFactory: typeof openNodeSqlite = openNodeSqlite;
const nodeDriver: NodeSQLiteDriver | undefined = undefined;
const cloudflareDriver: typeof CloudflareSQLiteDriver = CloudflareSQLiteDriver;
const nodeVfsFactory: typeof openNodeVfs = openNodeVfs;
const nodeVfsHandle: NodeVfsHandle | undefined = undefined;
const protocol: string = REPLICATION_PROTOCOL_VERSION;
const recorder: typeof createRecordingFactory = createRecordingFactory;
const adapterFactory: ConformanceAdapterFactory | undefined = undefined;
void FilesystemError; void open; void plan; void bridge; void nodeDriverFactory; void nodeDriver; void cloudflareDriver;
void nodeVfsFactory; void nodeVfsHandle; void protocol; void recorder; void adapterFactory;
`);
  await writeFile(path.join(consumer, "tsconfig.json"), JSON.stringify({ compilerOptions: { strict: true, noEmit: true, target: "ES2022", module: "NodeNext", moduleResolution: "NodeNext", lib: ["ES2022", "ESNext.Disposable", "DOM", "DOM.Iterable"] }, files: ["consumer.ts"] }));
  const tsc = process.platform === "win32" ? path.join(root, "node_modules", ".bin", "tsc.CMD") : path.join(root, "node_modules", ".bin", "tsc");
  await execute(tsc, ["-p", path.join(consumer, "tsconfig.json")], { cwd: root, windowsHide: true, shell: process.platform === "win32", maxBuffer: 8 * 1024 * 1024 });
  console.log(`exports: ${publishablePackages.length} clean packages (${totalDistFiles} dist files) match source/API snapshots; stale sentinels removed; ${publishablePackages.length} tarballs (${totalPackedFiles} files) pass clean runtime/type consumers and core deep-import denials`);
} finally {
  await rm(temporary, { recursive: true, force: true });
}
