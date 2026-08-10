import { readFile, readdir, realpath } from "node:fs/promises";
import path from "node:path";
import ts from "typescript";

const root = path.resolve(import.meta.dirname, "..");
const packageRoot = path.join(root, "packages");
const coreRoot = path.join(packageRoot, "fs", "src");
const fixtureRoot = path.join(root, "tests", "fixtures", "architecture-bypasses");
const violations = [];

const requiredCoreDirectories = new Set([
  "filesystem", "cas", "cdc", "cow", "patches", "manifests", "namespace", "branches",
  "revisions", "operations", "sqlite", "resources", "streams", "cache", "maintenance", "integrations",
]);
const sourceExtensions = new Set([".ts", ".tsx", ".mts", ".cts"]);

async function filesBelow(directory, extensions = sourceExtensions) {
  const output = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const filename = path.join(directory, entry.name);
    if (entry.isDirectory()) output.push(...await filesBelow(filename, extensions));
    else if (extensions.has(path.extname(entry.name))) output.push(filename);
  }
  return output;
}

function key(filename) {
  const value = path.resolve(filename);
  return process.platform === "win32" ? value.toLowerCase() : value;
}
function relative(filename) { return path.relative(root, filename).replaceAll("\\", "/"); }
function within(filename, directory) {
  const value = path.relative(directory, filename);
  return value === "" || (!value.startsWith(`..${path.sep}`) && value !== ".." && !path.isAbsolute(value));
}
function coreRelative(filename) { return path.relative(coreRoot, filename).replaceAll("\\", "/"); }
function coreArea(filename) {
  const value = coreRelative(filename);
  return value.includes("/") ? value.slice(0, value.indexOf("/")) : "(root)";
}
function packageName(specifier) {
  if (!specifier.startsWith("@ephemeralai/")) return undefined;
  return specifier.split("/").slice(0, 2).join("/");
}

/** Return every executable or type-level module edge represented by TypeScript syntax. */
function moduleReferences(sourceFile) {
  const references = [];
  const push = (kind, node, value, typeOnly = false) => {
    references.push({
      kind,
      specifier: value && ts.isStringLiteralLike(value) ? value.text : undefined,
      typeOnly,
      line: sourceFile.getLineAndCharacterOfPosition(node.getStart(sourceFile)).line + 1,
    });
  };
  const visit = (node) => {
    if (ts.isImportDeclaration(node)) {
      push("static-import", node, node.moduleSpecifier, Boolean(node.importClause?.isTypeOnly));
      return;
    }
    if (ts.isExportDeclaration(node) && node.moduleSpecifier) {
      push("static-export", node, node.moduleSpecifier, Boolean(node.isTypeOnly));
      return;
    }
    if (ts.isImportEqualsDeclaration(node) && ts.isExternalModuleReference(node.moduleReference)) {
      push("import-equals", node, node.moduleReference.expression, Boolean(node.isTypeOnly));
      return;
    }
    if (ts.isImportTypeNode(node)) {
      const argument = ts.isLiteralTypeNode(node.argument) ? node.argument.literal : undefined;
      push("import-type", node, argument, true);
    } else if (ts.isCallExpression(node) && node.expression.kind === ts.SyntaxKind.ImportKeyword) {
      push("dynamic-import", node, node.arguments[0]);
    } else if (ts.isCallExpression(node) && ts.isIdentifier(node.expression) && node.expression.text === "require") {
      push("require", node, node.arguments[0]);
    } else if (ts.isCallExpression(node) && ts.isPropertyAccessExpression(node.expression)
      && ts.isIdentifier(node.expression.expression) && node.expression.expression.text === "require"
      && node.expression.name.text === "resolve") {
      push("require-resolve", node, node.arguments[0]);
    }
    ts.forEachChild(node, visit);
  };
  visit(sourceFile);
  return references;
}

function parse(filename, source) {
  const extension = path.extname(filename);
  const kind = extension === ".tsx" ? ts.ScriptKind.TSX
    : extension === ".mts" ? ts.ScriptKind.TS
      : extension === ".cts" ? ts.ScriptKind.TS : ts.ScriptKind.TS;
  return ts.createSourceFile(filename, source, ts.ScriptTarget.Latest, true, kind);
}

const allowedPackages = new Map([
  ["@ephemeralai/fs", new Set()],
  ["@ephemeralai/fs-sqlite-node", new Set(["@ephemeralai/fs"])],
  ["@ephemeralai/fs-sqlite-cloudflare", new Set(["@ephemeralai/fs"])],
  ["@ephemeralai/fs-node-vfs", new Set(["@ephemeralai/fs", "@ephemeralai/fs-sqlite-node"])],
  ["@ephemeralai/fs-replication", new Set(["@ephemeralai/fs"])],
  ["@ephemeralai/fs-testkit", new Set(["@ephemeralai/fs"])],
]);

// Discover every workspace source first so relative imports can be resolved by
// real path and attributed to their actual package, including symlink escapes.
const packages = [];
const sourceByLogical = new Map();
const sourceByReal = new Map();
for (const entry of await readdir(packageRoot, { withFileTypes: true })) {
  if (!entry.isDirectory()) continue;
  const directory = path.join(packageRoot, entry.name);
  const manifest = JSON.parse(await readFile(path.join(directory, "package.json"), "utf8"));
  const sources = await filesBelow(path.join(directory, "src"));
  const info = { name: manifest.name, directory, manifest, sources: [] };
  packages.push(info);
  for (const logical of sources) {
    const real = await realpath(logical);
    const source = { logical: path.resolve(logical), real: path.resolve(real), package: info };
    info.sources.push(source);
    sourceByLogical.set(key(source.logical), source);
    sourceByReal.set(key(source.real), source);
  }
}

function localCandidates(from, specifier) {
  const base = path.resolve(path.dirname(from), specifier);
  const extension = path.extname(base).toLowerCase();
  if (extension === ".js") return [base.slice(0, -3) + ".ts", base.slice(0, -3) + ".tsx", base];
  if (extension === ".mjs") return [base.slice(0, -4) + ".mts", base];
  if (extension === ".cjs") return [base.slice(0, -4) + ".cts", base];
  if (sourceExtensions.has(extension)) return [base];
  return [base, `${base}.ts`, `${base}.tsx`, `${base}.mts`, `${base}.cts`, path.join(base, "index.ts"), path.join(base, "index.mts"), path.join(base, "index.cts")];
}
async function resolveLocal(from, specifier) {
  if (!specifier.startsWith(".")) return undefined;
  for (const candidate of localCandidates(from, specifier)) {
    const logical = sourceByLogical.get(key(candidate));
    if (logical) return logical;
    try {
      const real = await realpath(candidate);
      const resolved = sourceByReal.get(key(real));
      if (resolved) return resolved;
    } catch (error) {
      if (error?.code !== "ENOENT" && error?.code !== "ENOTDIR") throw error;
    }
  }
  return undefined;
}

function findCycles(graph, label, display = (value) => value) {
  const state = new Map();
  const stack = [];
  const visit = (node) => {
    state.set(node, 1);
    stack.push(node);
    for (const next of graph.get(node) ?? []) {
      if (!state.has(next)) visit(next);
      else if (state.get(next) === 1) {
        const start = stack.lastIndexOf(next);
        violations.push(`${label} cycle: ${[...stack.slice(start), next].map(display).join(" -> ")}`);
      }
    }
    stack.pop();
    state.set(node, 2);
  };
  for (const node of graph.keys()) if (!state.has(node)) visit(node);
}

const allowedAreas = new Map([
  ["(root)", new Set(["filesystem", "branches", "resources"])],
  ["filesystem", new Set(["filesystem", "operations", "sqlite", "resources", "cow"])],
  ["cas", new Set(["cas"])],
  ["cdc", new Set(["cdc", "resources"])],
  ["cow", new Set(["cow", "resources"])],
  ["patches", new Set(["patches", "resources"])],
  ["manifests", new Set(["manifests", "cas", "resources"])],
  ["namespace", new Set(["namespace", "filesystem", "resources"])],
  ["branches", new Set(["branches", "filesystem", "revisions"])],
  ["revisions", new Set(["revisions"])],
  ["operations", new Set(["operations", "cas", "cdc", "cow", "patches", "manifests", "namespace", "branches", "revisions", "resources", "streams", "cache", "filesystem"])],
  ["sqlite", new Set(["sqlite", "cas", "manifests", "namespace", "cow", "resources", "cache", "filesystem", "branches", "revisions", "operations"])],
  ["resources", new Set(["resources"])],
  ["streams", new Set(["streams", "resources"])],
  ["cache", new Set(["cache", "cas", "resources"])],
  ["maintenance", new Set(["maintenance", "operations", "filesystem"])],
  ["integrations", new Set(["integrations", "operations", "filesystem", "sqlite", "resources"])],
]);

const restrictedCoreEdges = new Map([
  ["filesystem->sqlite", new Set([
    "filesystem/ephemeral-fs.ts->sqlite/operations-storage.ts",
    "filesystem/types.ts->sqlite/driver.ts",
  ])],
  ["integrations->sqlite", new Set([
    "integrations/node-vfs.ts->sqlite/driver.ts",
    "integrations/node-vfs.ts->sqlite/operations-storage.ts",
  ])],
  ["sqlite->operations", new Set([
    "sqlite/operations-storage.ts->operations/storage-ports.ts",
  ])],
]);

function coreDirectionReason(fromArea, toArea, fromRelative, toRelative) {
  if (!allowedAreas.get(fromArea)?.has(toArea)) return `violates core direction ${fromArea} -> ${toArea}`;
  const restriction = restrictedCoreEdges.get(`${fromArea}->${toArea}`);
  if (restriction && !restriction.has(`${fromRelative}->${toRelative}`)) return `uses an unapproved ${fromArea} -> ${toArea} composition edge`;
  return undefined;
}

const corePackage = packages.find((item) => item.name === "@ephemeralai/fs");
if (!corePackage) throw new Error("missing @ephemeralai/fs package");
const coreFiles = corePackage.sources.filter((source) => within(source.logical, coreRoot));
const populatedAreas = new Set(coreFiles.map((source) => coreArea(source.logical)).filter((area) => area !== "(root)"));
for (const required of requiredCoreDirectories) if (!populatedAreas.has(required)) violations.push(`missing required core directory: ${required}`);
for (const actual of populatedAreas) if (!requiredCoreDirectories.has(actual)) violations.push(`unapproved core directory: ${actual}`);
for (const source of coreFiles.filter((item) => coreArea(item.logical) === "(root)" && path.basename(item.logical) !== "index.ts")) violations.push(`unapproved root source file: ${relative(source.logical)}`);

const graph = new Map(coreFiles.map((source) => [source.real, new Set()]));
const transformationAreas = new Set(["cdc", "cow", "patches", "manifests"]);
for (const sourceInfo of coreFiles) {
  const sourceText = await readFile(sourceInfo.logical, "utf8");
  const parsed = parse(sourceInfo.logical, sourceText);
  const fromArea = coreArea(sourceInfo.logical);
  const composed = new Set();
  for (const reference of moduleReferences(parsed)) {
    if (!reference.specifier) {
      violations.push(`${relative(sourceInfo.logical)}:${reference.line} has non-literal ${reference.kind}; the import graph cannot prove its target`);
      continue;
    }
    if (!reference.specifier.startsWith(".")) continue;
    const target = await resolveLocal(sourceInfo.logical, reference.specifier);
    if (!target) {
      violations.push(`${relative(sourceInfo.logical)}:${reference.line} has unresolved local ${reference.kind} ${reference.specifier}`);
      continue;
    }
    if (target.package.name !== "@ephemeralai/fs" || !within(target.logical, coreRoot)) {
      violations.push(`${relative(sourceInfo.logical)}:${reference.line} escapes the core source tree to ${relative(target.logical)}`);
      continue;
    }
    graph.get(sourceInfo.real).add(target.real);
    const toArea = coreArea(target.logical);
    const reason = coreDirectionReason(fromArea, toArea, coreRelative(sourceInfo.logical), coreRelative(target.logical));
    if (reason) violations.push(`${relative(sourceInfo.logical)}:${reference.line} ${reason}`);
    if (transformationAreas.has(toArea) && toArea !== fromArea) composed.add(toArea);
  }
  if (fromArea !== "operations" && composed.size > 1) violations.push(`${relative(sourceInfo.logical)} cross-composes ${[...composed].sort().join(" + ")}; transformation composition belongs in operations`);

  const sqlOwner = fromArea === "sqlite";
  const inspectSql = (node) => {
    if (!sqlOwner && (ts.isStringLiteralLike(node) || ts.isNoSubstitutionTemplateLiteral(node))) {
      const value = node.text.trimStart();
      if (/^(?:SELECT|INSERT|UPDATE|DELETE|CREATE|DROP|ALTER|PRAGMA|REPLACE|WITH|VACUUM|ATTACH|DETACH|BEGIN|COMMIT|ROLLBACK|SAVEPOINT|RELEASE)(?:\s|;|\()/iu.test(value)) {
        violations.push(`${relative(sourceInfo.logical)}:${parsed.getLineAndCharacterOfPosition(node.getStart()).line + 1} contains SQL outside sqlite ownership`);
      }
    }
    ts.forEachChild(node, inspectSql);
  };
  inspectSql(parsed);
}
findCycles(graph, "source", (filename) => relative(filename));

const packageGraph = new Map();
for (const info of packages) {
  const expected = allowedPackages.get(info.name);
  const edges = new Set();
  packageGraph.set(info.name, edges);
  if (!expected) violations.push(`unexpected package ${info.name}`);
  const declared = { ...(info.manifest.dependencies ?? {}), ...(info.manifest.devDependencies ?? {}), ...(info.manifest.peerDependencies ?? {}) };
  const recordDependency = (dependency, label) => {
    if (dependency === info.name) return;
    edges.add(dependency);
    if (!expected?.has(dependency)) violations.push(`${label} imports forbidden workspace package ${dependency}`);
    if (!Object.hasOwn(declared, dependency)) violations.push(`${label} imports undeclared workspace package ${dependency}`);
  };
  for (const dependency of Object.keys(declared).filter((name) => name.startsWith("@ephemeralai/"))) {
    edges.add(dependency);
    if (!expected?.has(dependency)) violations.push(`${info.name} must not declare dependency ${dependency}`);
  }
  for (const sourceInfo of info.sources) {
    const parsed = parse(sourceInfo.logical, await readFile(sourceInfo.logical, "utf8"));
    for (const reference of moduleReferences(parsed)) {
      if (!reference.specifier) {
        violations.push(`${relative(sourceInfo.logical)}:${reference.line} has non-literal ${reference.kind}; the package graph cannot prove its target`);
        continue;
      }
      const bareDependency = packageName(reference.specifier);
      if (bareDependency) {
        recordDependency(bareDependency, `${relative(sourceInfo.logical)}:${reference.line}`);
        continue;
      }
      if (!reference.specifier.startsWith(".")) continue;
      const target = await resolveLocal(sourceInfo.logical, reference.specifier);
      if (!target) {
        violations.push(`${relative(sourceInfo.logical)}:${reference.line} has unresolved package-local ${reference.kind} ${reference.specifier}`);
        continue;
      }
      if (target.package.name !== info.name) recordDependency(target.package.name, `${relative(sourceInfo.logical)}:${reference.line} relative realpath escape`);
    }
  }
}
findCycles(packageGraph, "package");

// These deliberately forbidden files prove each syntax/realpath bypass is
// observed by the same parser, resolver, and policy used for production code.
const fixtureCases = [
  { file: "operations/dynamic-import.ts", kind: "dynamic-import", policy: "core" },
  { file: "operations/import-equals.cts", kind: "import-equals", policy: "core" },
  { file: "operations/require.cts", kind: "require", policy: "core" },
  { file: "fs/cross-package-relative.ts", kind: "static-import", policy: "package" },
];
for (const fixture of fixtureCases) {
  const filename = path.join(fixtureRoot, ...fixture.file.split("/"));
  const parsed = parse(filename, await readFile(filename, "utf8"));
  const reference = moduleReferences(parsed).find((item) => item.kind === fixture.kind);
  const target = reference?.specifier ? await resolveLocal(filename, reference.specifier) : undefined;
  let rejected = false;
  if (target && fixture.policy === "core" && target.package.name === "@ephemeralai/fs") {
    rejected = Boolean(coreDirectionReason("operations", coreArea(target.logical), "operations/negative-fixture.ts", coreRelative(target.logical)));
  } else if (target && fixture.policy === "package") {
    rejected = target.package.name !== "@ephemeralai/fs" && !allowedPackages.get("@ephemeralai/fs")?.has(target.package.name);
  }
  if (!reference || !target || !rejected) violations.push(`negative architecture fixture was not detected and rejected: ${fixture.file}`);
}

if (violations.length) {
  console.error([...new Set(violations)].join("\n"));
  process.exitCode = 1;
} else {
  console.log(`architecture: ${coreFiles.length} core files; full TypeScript module graph, realpath package graph, exact ports/directions, cycles, composition, SQL ownership, and ${fixtureCases.length} bypass fixtures valid`);
}
