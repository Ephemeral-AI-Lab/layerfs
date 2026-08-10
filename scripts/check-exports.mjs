import { execFile } from "node:child_process";
import {
  access,
  cp,
  mkdtemp,
  mkdir,
  readFile,
  readdir,
  rm,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { promisify } from "node:util";
import ts from "typescript";

const execute = promisify(execFile);
const root = path.resolve(import.meta.dirname, "..");
const packagesRoot = path.join(root, "packages");
const expectedCoreExports = [
  ".",
  "./integrations/node-vfs",
  "./integrations/replication",
  "./sqlite-driver",
].sort();
const executable = (name) => (process.platform === "win32" ? `${name}.cmd` : name);

async function filesBelow(directory, prefix = "") {
  const result = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const relative = prefix ? `${prefix}/${entry.name}` : entry.name;
    if (entry.isDirectory())
      result.push(...(await filesBelow(path.join(directory, entry.name), relative)));
    else if (entry.isFile()) result.push(relative);
  }
  return result;
}
async function filesBelowIfPresent(directory, prefix = "") {
  try {
    return await filesBelow(directory, prefix);
  } catch (error) {
    if (error?.code === "ENOENT") return [];
    throw error;
  }
}
function sameFiles(left, right) {
  return JSON.stringify([...left].sort()) === JSON.stringify([...right].sort());
}
function listDifference(left, right) {
  return left.filter((name) => !right.includes(name));
}

async function expectedDistFiles(packageDirectory) {
  const sourceFiles = (await filesBelow(path.join(packageDirectory, "src"))).filter(
    (name) =>
      (name.endsWith(".ts") || name.endsWith(".mts") || name.endsWith(".cts")) &&
      !name.endsWith(".d.ts"),
  );
  return sourceFiles
    .flatMap((name) => {
      const extension = path.posix.extname(name);
      const stem = name.slice(0, -extension.length);
      const outputExtension =
        extension === ".mts" ? ".mjs" : extension === ".cts" ? ".cjs" : ".js";
      const declarationExtension =
        extension === ".mts" ? ".d.mts" : extension === ".cts" ? ".d.cts" : ".d.ts";
      return [
        `${stem}${outputExtension}`,
        `${stem}${outputExtension}.map`,
        `${stem}${declarationExtension}`,
        `${stem}${declarationExtension}.map`,
      ];
    })
    .sort();
}

async function assertExactDist(packageName, packageDirectory) {
  const expected = await expectedDistFiles(packageDirectory);
  const actual = (
    await filesBelowIfPresent(path.join(packageDirectory, "dist"))
  ).sort();
  if (!sameFiles(actual, expected))
    throw new Error(
      `${packageName} clean dist differs from source (unexpected: ${listDifference(actual, expected).join(", ") || "none"}; missing: ${listDifference(expected, actual).join(", ") || "none"})`,
    );
  return { expected, actual };
}

function runtimeValueExportNames(entry, rootNames) {
  const program = ts.createProgram({
    rootNames,
    options: {
      target: ts.ScriptTarget.ES2022,
      module: ts.ModuleKind.NodeNext,
      moduleResolution: ts.ModuleResolutionKind.NodeNext,
      skipLibCheck: true,
      noEmit: true,
    },
  });
  const sourceFile = program.getSourceFile(path.resolve(entry));
  const checker = program.getTypeChecker();
  const moduleSymbol = sourceFile
    ? (checker.getSymbolAtLocation(sourceFile) ?? sourceFile.symbol)
    : undefined;
  if (!sourceFile || !moduleSymbol)
    throw new Error(`cannot inspect public runtime declarations: ${entry}`);
  return checker
    .getExportsOfModule(moduleSymbol)
    .filter((symbol) => {
      const target =
        symbol.flags & ts.SymbolFlags.Alias ? checker.getAliasedSymbol(symbol) : symbol;
      return Boolean(target.flags & ts.SymbolFlags.Value);
    })
    .map((symbol) => symbol.name)
    .sort();
}

const publishablePackages = [];
for (const entry of (await readdir(packagesRoot, { withFileTypes: true })).sort(
  (left, right) => left.name.localeCompare(right.name),
)) {
  if (!entry.isDirectory()) continue;
  const directory = path.join(packagesRoot, entry.name);
  const manifest = JSON.parse(
    await readFile(path.join(directory, "package.json"), "utf8"),
  );
  if (manifest.private === true || !manifest.exports) continue;
  publishablePackages.push({ directory, manifest });
}
const core = publishablePackages.find(
  (item) => item.manifest.name === "@ephemeralai/fs",
);
if (!core) throw new Error("missing publishable @ephemeralai/fs package");
const actualCoreExports = Object.keys(core.manifest.exports).sort();
if (!sameFiles(actualCoreExports, expectedCoreExports))
  throw new Error(`core exports changed: ${actualCoreExports.join(", ")}`);

const cleanMutationTemporary = await mkdtemp(
  path.join(tmpdir(), "ephemeral-ai-fs-clean-mutation-"),
);
try {
  const fixture = path.join(
    root,
    "tests",
    "fixtures",
    "package-build-bypasses",
    "sentinel-only",
  );
  const candidate = path.join(cleanMutationTemporary, "candidate");
  await cp(fixture, candidate, { recursive: true });
  const dist = path.join(candidate, "dist");
  await mkdir(dist, { recursive: true });
  await writeFile(path.join(dist, "stale.js"), "export const stale = true;\n");
  await writeFile(
    path.join(dist, "__m0_stale_output_sentinel__.js"),
    "export const sentinel = true;\n",
  );
  await rm(dist, { recursive: true, force: true });
  await execute(executable("npm"), ["run", "build", "--silent"], {
    cwd: candidate,
    windowsHide: true,
    shell: process.platform === "win32",
  });
  let rejected = false;
  try {
    await assertExactDist("sentinel-only mutation fixture", candidate);
  } catch {
    rejected = true;
  }
  if (!rejected)
    throw new Error("sentinel-only/no-op package build mutation was not rejected");
} finally {
  await rm(cleanMutationTemporary, { recursive: true, force: true });
}

let totalDistFiles = 0;
const packageArtifacts = new Map();
for (const packageInfo of publishablePackages) {
  const distDirectory = path.join(packageInfo.directory, "dist");
  await rm(distDirectory, { recursive: true, force: true });
  await execute(executable("pnpm"), ["--filter", packageInfo.manifest.name, "build"], {
    cwd: root,
    windowsHide: true,
    shell: process.platform === "win32",
    maxBuffer: 8 * 1024 * 1024,
  });
  const { expected: expectedDistFiles, actual: actualDistFiles } =
    await assertExactDist(packageInfo.manifest.name, packageInfo.directory);
  totalDistFiles += actualDistFiles.length;
  packageArtifacts.set(packageInfo.manifest.name, {
    expectedDistFiles,
    actualDistFiles,
  });
}

const declarationRoots = publishablePackages.flatMap((packageInfo) =>
  packageInfo.manifest.exports
    ? Object.values(packageInfo.manifest.exports).map((condition) =>
        path.resolve(packageInfo.directory, condition.types),
      )
    : [],
);
// No version 0.1 public value is declaration-only. Additions require a name and
// a reviewable reason rather than silently weakening runtime parity.
const runtimeValueExceptions = new Map();
const publicRuntimeValues = new Map();
for (const packageInfo of publishablePackages) {
  for (const [subpath, condition] of Object.entries(packageInfo.manifest.exports)) {
    const specifier =
      subpath === "."
        ? packageInfo.manifest.name
        : `${packageInfo.manifest.name}${subpath.slice(1)}`;
    const exceptions = runtimeValueExceptions.get(specifier) ?? [];
    publicRuntimeValues.set(
      specifier,
      runtimeValueExportNames(
        path.resolve(packageInfo.directory, condition.types),
        declarationRoots,
      ).filter((name) => !exceptions.some((item) => item.name === name)),
    );
  }
}

const runtimeFixtureDirectory = path.join(
  root,
  "tests",
  "fixtures",
  "runtime-export-bypasses",
  "missing-value",
);
const runtimeFixtureExpected = runtimeValueExportNames(
  path.join(runtimeFixtureDirectory, "index.d.ts"),
  [path.join(runtimeFixtureDirectory, "index.d.ts")],
);
const runtimeFixtureActual = Object.keys(
  await import(pathToFileURL(path.join(runtimeFixtureDirectory, "index.js")).href),
).sort();
if (sameFiles(runtimeFixtureExpected, runtimeFixtureActual))
  throw new Error("missing runtime value negative fixture was not rejected");

await execute(
  process.execPath,
  [path.join(root, "scripts", "check-api-snapshots.mjs")],
  { cwd: root, windowsHide: true, maxBuffer: 8 * 1024 * 1024 },
);

const temporary = await mkdtemp(path.join(tmpdir(), "ephemeral-ai-fs-exports-"));
try {
  const packDirectory = path.join(temporary, "pack");
  const consumer = path.join(temporary, "consumer");
  await mkdir(packDirectory);
  await mkdir(consumer);
  let totalPackedFiles = 0;
  const archiveByPackage = new Map();
  for (const packageInfo of publishablePackages) {
    const dryRun = await execute(executable("pnpm"), ["pack", "--dry-run", "--json"], {
      cwd: packageInfo.directory,
      windowsHide: true,
      shell: process.platform === "win32",
      maxBuffer: 8 * 1024 * 1024,
    });
    const dryRunManifest = JSON.parse(dryRun.stdout);
    const packedFiles = dryRunManifest.files
      .map((entry) => entry.path.replaceAll("\\", "/"))
      .sort();
    const approvedPackageFiles = ["package.json"];
    for (const asset of ["README.md", "LICENSE"]) {
      try {
        await access(path.join(packageInfo.directory, asset));
        approvedPackageFiles.push(asset);
      } catch (error) {
        if (error?.code !== "ENOENT") throw error;
      }
    }
    const expectedPackedFiles = [
      ...packageArtifacts
        .get(packageInfo.manifest.name)
        .expectedDistFiles.map((name) => `dist/${name}`),
      ...approvedPackageFiles,
    ].sort();
    if (!sameFiles(packedFiles, expectedPackedFiles)) {
      throw new Error(
        `${packageInfo.manifest.name} packed files differ from its clean source artifact (unexpected: ${listDifference(packedFiles, expectedPackedFiles).join(", ") || "none"}; missing: ${listDifference(expectedPackedFiles, packedFiles).join(", ") || "none"})`,
      );
    }
    totalPackedFiles += packedFiles.length;
    const archivesBefore = new Set(await readdir(packDirectory));
    await execute(executable("pnpm"), ["pack", "--pack-destination", packDirectory], {
      cwd: packageInfo.directory,
      windowsHide: true,
      shell: process.platform === "win32",
      maxBuffer: 8 * 1024 * 1024,
    });
    const createdArchives = (await readdir(packDirectory)).filter(
      (name) => name.endsWith(".tgz") && !archivesBefore.has(name),
    );
    if (createdArchives.length !== 1)
      throw new Error(
        `${packageInfo.manifest.name} pack created ${createdArchives.length} archives`,
      );
    archiveByPackage.set(
      packageInfo.manifest.name,
      path.join(packDirectory, createdArchives[0]),
    );
  }
  const archives = (await readdir(packDirectory))
    .filter((name) => name.endsWith(".tgz"))
    .map((name) => path.join(packDirectory, name));
  if (archives.length !== publishablePackages.length)
    throw new Error(
      `expected ${publishablePackages.length} packed tarballs, found ${archives.length}`,
    );
  const packageByName = new Map(
    publishablePackages.map((packageInfo) => [packageInfo.manifest.name, packageInfo]),
  );
  const localDependencyClosure = (packageName) => {
    const result = new Set();
    const visit = (name) => {
      if (result.has(name)) return;
      result.add(name);
      const manifest = packageByName.get(name)?.manifest;
      for (const dependency of [
        ...Object.keys(manifest?.dependencies ?? {}),
        ...Object.keys(manifest?.peerDependencies ?? {}),
      ])
        if (packageByName.has(dependency)) visit(dependency);
    };
    visit(packageName);
    return [...result].sort();
  };
  for (const packageInfo of publishablePackages) {
    const isolated = path.join(
      temporary,
      `isolated-${packageInfo.manifest.name.replaceAll(/[^a-z0-9]+/giu, "-")}`,
    );
    await mkdir(isolated);
    await writeFile(
      path.join(isolated, "package.json"),
      JSON.stringify({ private: true, type: "module" }),
    );
    const closure = localDependencyClosure(packageInfo.manifest.name);
    await execute(
      executable("npm"),
      [
        "install",
        "--ignore-scripts",
        "--no-audit",
        "--no-fund",
        "--package-lock=false",
        ...closure.map((name) => archiveByPackage.get(name)),
      ],
      {
        cwd: isolated,
        windowsHide: true,
        shell: process.platform === "win32",
        maxBuffer: 16 * 1024 * 1024,
      },
    );
    const publicSpecifiers = [...publicRuntimeValues.keys()].filter(
      (specifier) =>
        specifier === packageInfo.manifest.name ||
        specifier.startsWith(`${packageInfo.manifest.name}/`),
    );
    await writeFile(
      path.join(isolated, "runtime.mjs"),
      `
const expected = new Map(${JSON.stringify(publicSpecifiers.map((specifier) => [specifier, publicRuntimeValues.get(specifier)]))});
for (const [specifier, names] of expected) {
  const actual = Object.keys(await import(specifier)).sort();
  if (JSON.stringify(actual) !== JSON.stringify(names))
    throw new Error(\`runtime value exports differ for \${specifier}: expected \${names}; actual \${actual}\`);
}
`,
    );
    await execute(process.execPath, [path.join(isolated, "runtime.mjs")], {
      cwd: isolated,
      windowsHide: true,
      maxBuffer: 8 * 1024 * 1024,
    });
    await writeFile(
      path.join(isolated, "consumer.ts"),
      publicSpecifiers
        .map(
          (specifier, index) =>
            `import * as public${index} from ${JSON.stringify(specifier)}; void public${index};`,
        )
        .join("\n"),
    );
    await writeFile(
      path.join(isolated, "tsconfig.json"),
      JSON.stringify({
        compilerOptions: {
          strict: true,
          noEmit: true,
          target: "ES2022",
          module: "NodeNext",
          moduleResolution: "NodeNext",
          lib: ["ES2022", "ESNext.Disposable", "DOM", "DOM.Iterable"],
        },
        files: ["consumer.ts"],
      }),
    );
    const tsc =
      process.platform === "win32"
        ? path.join(root, "node_modules", ".bin", "tsc.CMD")
        : path.join(root, "node_modules", ".bin", "tsc");
    await execute(tsc, ["-p", path.join(isolated, "tsconfig.json")], {
      cwd: root,
      windowsHide: true,
      shell: process.platform === "win32",
      maxBuffer: 8 * 1024 * 1024,
    });
  }
  await writeFile(
    path.join(consumer, "package.json"),
    JSON.stringify({ private: true, type: "module" }),
  );
  await execute(
    executable("npm"),
    [
      "install",
      "--ignore-scripts",
      "--no-audit",
      "--no-fund",
      "--package-lock=false",
      ...archives,
    ],
    {
      cwd: consumer,
      windowsHide: true,
      shell: process.platform === "win32",
      maxBuffer: 16 * 1024 * 1024,
    },
  );

  await writeFile(
    path.join(consumer, "runtime.mjs"),
    `
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
`,
  );
  await execute(process.execPath, [path.join(consumer, "runtime.mjs")], {
    cwd: consumer,
    windowsHide: true,
    maxBuffer: 8 * 1024 * 1024,
  });

  await writeFile(
    path.join(consumer, "consumer.ts"),
    `
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
`,
  );
  await writeFile(
    path.join(consumer, "tsconfig.json"),
    JSON.stringify({
      compilerOptions: {
        strict: true,
        noEmit: true,
        target: "ES2022",
        module: "NodeNext",
        moduleResolution: "NodeNext",
        lib: ["ES2022", "ESNext.Disposable", "DOM", "DOM.Iterable"],
      },
      files: ["consumer.ts"],
    }),
  );
  const tsc =
    process.platform === "win32"
      ? path.join(root, "node_modules", ".bin", "tsc.CMD")
      : path.join(root, "node_modules", ".bin", "tsc");
  await execute(tsc, ["-p", path.join(consumer, "tsconfig.json")], {
    cwd: root,
    windowsHide: true,
    shell: process.platform === "win32",
    maxBuffer: 8 * 1024 * 1024,
  });
  console.log(
    `exports: ${publishablePackages.length} gate-cleaned packages (${totalDistFiles} dist files) match source/reachable API snapshots; sentinel-only builds rejected; ${publishablePackages.length} tarballs (${totalPackedFiles} files) pass isolated declared-closure runtime/type parity and core deep-import denials`,
  );
} finally {
  await rm(temporary, { recursive: true, force: true });
}
