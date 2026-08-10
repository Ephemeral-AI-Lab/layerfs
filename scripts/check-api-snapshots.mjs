import { mkdir, readFile, readdir, rm, writeFile } from "node:fs/promises";
import path from "node:path";
import ts from "typescript";

const root = path.resolve(import.meta.dirname, "..");
const packagesRoot = path.join(root, "packages");
const update = process.argv.includes("--update");
const compilerOptions = {
  target: ts.ScriptTarget.ES2022,
  module: ts.ModuleKind.NodeNext,
  moduleResolution: ts.ModuleResolutionKind.NodeNext,
  lib: [
    "lib.es2022.d.ts",
    "lib.dom.d.ts",
    "lib.dom.iterable.d.ts",
    "lib.esnext.disposable.d.ts",
  ],
  skipLibCheck: true,
  noEmit: true,
};

async function filesBelow(directory) {
  const output = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const filename = path.join(directory, entry.name);
    if (entry.isDirectory()) output.push(...(await filesBelow(filename)));
    else output.push(filename);
  }
  return output;
}
function normalized(value) {
  return value.replaceAll("\r\n", "\n");
}
function relativeRoot(filename) {
  return path.relative(root, filename).replaceAll("\\", "/");
}
function within(filename, directory) {
  const value = path.relative(directory, path.resolve(filename));
  return (
    value === "" ||
    (!value.startsWith(`..${path.sep}`) && value !== ".." && !path.isAbsolute(value))
  );
}
function snapshotStem(subpath) {
  return subpath === "." ? "root" : subpath.slice(2).replaceAll("/", "-");
}
function symbolKinds(symbol) {
  const result = [];
  if (symbol.flags & ts.SymbolFlags.Value) result.push("value");
  if (symbol.flags & ts.SymbolFlags.Type) result.push("type");
  if (symbol.flags & ts.SymbolFlags.Namespace) result.push("namespace");
  return result;
}

function declarationModuleSpecifiers(sourceFile) {
  const result = new Set();
  const visit = (node) => {
    if (
      (ts.isImportDeclaration(node) || ts.isExportDeclaration(node)) &&
      node.moduleSpecifier &&
      ts.isStringLiteralLike(node.moduleSpecifier)
    )
      result.add(node.moduleSpecifier.text);
    else if (
      ts.isImportEqualsDeclaration(node) &&
      ts.isExternalModuleReference(node.moduleReference) &&
      ts.isStringLiteralLike(node.moduleReference.expression)
    )
      result.add(node.moduleReference.expression.text);
    else if (ts.isImportTypeNode(node) && ts.isLiteralTypeNode(node.argument)) {
      const literal = node.argument.literal;
      if (ts.isStringLiteralLike(literal)) result.add(literal.text);
    }
    ts.forEachChild(node, visit);
  };
  visit(sourceFile);
  return [...result];
}

function reachableDeclarationFiles(entry, program) {
  const result = new Set();
  const pending = [path.resolve(entry)];
  while (pending.length) {
    const filename = pending.pop();
    const sourceFile = program.getSourceFile(filename);
    if (!sourceFile || result.has(path.resolve(sourceFile.fileName))) continue;
    const resolvedFilename = path.resolve(sourceFile.fileName);
    result.add(resolvedFilename);
    for (const specifier of declarationModuleSpecifiers(sourceFile)) {
      const resolved = ts.resolveModuleName(
        specifier,
        resolvedFilename,
        compilerOptions,
        ts.sys,
      ).resolvedModule?.resolvedFileName;
      if (resolved && within(resolved, packagesRoot) && resolved.endsWith(".d.ts"))
        pending.push(path.resolve(resolved));
    }
  }
  return [...result].sort((left, right) =>
    relativeRoot(left).localeCompare(relativeRoot(right)),
  );
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
  const dist = path.join(directory, "dist");
  const declarations = (await filesBelow(dist)).filter((filename) =>
    filename.endsWith(".d.ts"),
  );
  publishablePackages.push({ directory, dist, manifest, declarations });
}

const declarationFiles = publishablePackages.flatMap((item) => item.declarations);
const program = ts.createProgram({
  rootNames: declarationFiles,
  options: compilerOptions,
});
const checker = program.getTypeChecker();
const printer = ts.createPrinter({
  newLine: ts.NewLineKind.LineFeed,
  removeComments: false,
});

let checkedSymbols = 0;
let checkedSubpaths = 0;
for (const packageInfo of publishablePackages) {
  const snapshotDirectory = path.join(packageInfo.directory, "api-snapshots");
  await mkdir(snapshotDirectory, { recursive: true });
  const expectedSnapshotNames = new Set();
  const publicEntries = Object.entries(packageInfo.manifest.exports)
    .map(([subpath, condition]) => ({
      subpath,
      entry: path.resolve(packageInfo.directory, condition.types),
    }))
    .sort((left, right) => left.subpath.localeCompare(right.subpath));
  for (const publicEntry of publicEntries) {
    const sourceFile = program.getSourceFile(publicEntry.entry);
    if (!sourceFile)
      throw new Error(
        `public declaration entry was not built: ${relativeRoot(publicEntry.entry)}`,
      );
    const moduleSymbol = checker.getSymbolAtLocation(sourceFile) ?? sourceFile.symbol;
    if (!moduleSymbol)
      throw new Error(
        `public declaration entry has no module symbol: ${relativeRoot(publicEntry.entry)}`,
      );
    const exported = checker
      .getExportsOfModule(moduleSymbol)
      .sort((left, right) => left.name.localeCompare(right.name));
    const report = [];
    const declarations = [
      "/* Generated public API declaration snapshot. Update only with: pnpm api:update */",
      `/* package: ${packageInfo.manifest.name}; subpath: ${publicEntry.subpath}; entry: ${relativeRoot(publicEntry.entry)} */`,
      "",
    ];
    for (const exportedSymbol of exported) {
      const target =
        exportedSymbol.flags & ts.SymbolFlags.Alias
          ? checker.getAliasedSymbol(exportedSymbol)
          : exportedSymbol;
      const workspaceDeclarations = (target.declarations ?? []).filter((declaration) =>
        within(declaration.getSourceFile().fileName, packagesRoot),
      );
      const declarationReport = workspaceDeclarations
        .map((declaration) => ({
          file: relativeRoot(declaration.getSourceFile().fileName),
          kind: ts.SyntaxKind[declaration.kind],
        }))
        .sort(
          (left, right) =>
            left.file.localeCompare(right.file) || left.kind.localeCompare(right.kind),
        );
      const kinds = symbolKinds(target);
      report.push({
        name: exportedSymbol.name,
        kinds,
        declarations: declarationReport,
      });
      declarations.push(
        `/* export: ${exportedSymbol.name}; kinds: ${kinds.join(",") || "unknown"} */`,
      );
      if (!workspaceDeclarations.length) {
        declarations.push("/* declaration supplied outside this workspace */", "");
        continue;
      }
      for (const declaration of workspaceDeclarations) {
        declarations.push(
          `/* source: ${relativeRoot(declaration.getSourceFile().fileName)} */`,
        );
        declarations.push(
          printer
            .printNode(
              ts.EmitHint.Unspecified,
              declaration,
              declaration.getSourceFile(),
            )
            .trim(),
        );
      }
      declarations.push("");
    }
    checkedSymbols += report.length;
    checkedSubpaths += 1;
    const stem = snapshotStem(publicEntry.subpath);
    const expectedSymbols = `${JSON.stringify({ package: packageInfo.manifest.name, subpath: publicEntry.subpath, entry: relativeRoot(publicEntry.entry), symbols: report }, null, 2)}\n`;
    const expectedDeclarations = `${declarations.join("\n").trimEnd()}\n`;
    const rollupFiles = reachableDeclarationFiles(publicEntry.entry, program);
    const expectedRollup = `${[
      "/* Generated reachable public declaration rollup. Update only with: pnpm api:update */",
      `/* package: ${packageInfo.manifest.name}; subpath: ${publicEntry.subpath}; entry: ${relativeRoot(publicEntry.entry)} */`,
      ...(await Promise.all(
        rollupFiles.map(async (filename) =>
          [
            "",
            `/* ===== ${relativeRoot(filename)} ===== */`,
            normalized(await readFile(filename, "utf8"))
              .replace(/^\/\/# sourceMappingURL=.*$/gmu, "")
              .trimEnd(),
          ].join("\n"),
        ),
      )),
    ]
      .join("\n")
      .trimEnd()}\n`;
    for (const [suffix, expected] of [
      ["symbols.json", expectedSymbols],
      ["d.ts", expectedDeclarations],
      ["rollup.d.ts", expectedRollup],
    ]) {
      const snapshotName = `${stem}.${suffix}`;
      expectedSnapshotNames.add(snapshotName);
      const filename = path.join(snapshotDirectory, snapshotName);
      if (update) {
        await writeFile(filename, expected);
        continue;
      }
      let actual;
      try {
        actual = normalized(await readFile(filename, "utf8"));
      } catch (error) {
        if (error?.code === "ENOENT")
          throw new Error(
            `missing public API snapshot ${relativeRoot(filename)}; run pnpm api:update and review it`,
            { cause: error },
          );
        throw error;
      }
      if (actual !== expected)
        throw new Error(
          `public API snapshot changed: ${relativeRoot(filename)}; run pnpm api:update and review the diff`,
        );
    }
  }
  const actualSnapshotNames = (await readdir(snapshotDirectory)).filter(
    (name) =>
      name.endsWith(".symbols.json") ||
      name.endsWith(".d.ts") ||
      name.endsWith(".rollup.d.ts"),
  );
  for (const actual of actualSnapshotNames) {
    if (expectedSnapshotNames.has(actual)) continue;
    const filename = path.join(snapshotDirectory, actual);
    if (update) await rm(filename);
    else
      throw new Error(
        `stale public API snapshot ${relativeRoot(filename)}; run pnpm api:update and review the deletion`,
      );
  }
}

const reachableFixtureDirectory = path.join(
  root,
  "tests",
  "fixtures",
  "api-snapshot-bypasses",
  "private-reachable",
);
const reachableFixtureFiles = ["before.d.ts", "after.d.ts"].map((name) =>
  path.join(reachableFixtureDirectory, name),
);
const reachableFixtureProgram = ts.createProgram({
  rootNames: reachableFixtureFiles,
  options: compilerOptions,
});
const reachableFixtureRollups = await Promise.all(
  reachableFixtureFiles.map(async (entry) =>
    (
      await Promise.all(
        reachableDeclarationFiles(entry, reachableFixtureProgram).map((filename) =>
          readFile(filename, "utf8"),
        ),
      )
    ).join("\n"),
  ),
);
if (
  !reachableFixtureRollups.every((value) =>
    value.includes("interface HiddenPublicOption"),
  ) ||
  reachableFixtureRollups[0] === reachableFixtureRollups[1]
)
  throw new Error("reachable private public-type mutation fixture was not detected");

console.log(
  `api snapshots: ${publishablePackages.length} publishable packages, ${checkedSubpaths} public subpaths, and ${checkedSymbols} exported symbols match committed symbol/.d.ts reports`,
);
