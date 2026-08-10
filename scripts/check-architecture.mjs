import { readFile, readdir } from "node:fs/promises";
import path from "node:path";
import ts from "typescript";

const root = path.resolve(import.meta.dirname, "..");
const packageRoot = path.join(root, "packages");
const coreRoot = path.join(packageRoot, "fs", "src");
const violations = [];

const requiredCoreDirectories = new Set([
  "filesystem", "cas", "cdc", "cow", "patches", "manifests", "namespace", "branches",
  "revisions", "operations", "sqlite", "resources", "streams", "cache", "maintenance", "integrations",
]);

async function filesBelow(directory, extensions = new Set([".ts"])) {
  const output = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const filename = path.join(directory, entry.name);
    if (entry.isDirectory()) output.push(...await filesBelow(filename, extensions));
    else if (extensions.has(path.extname(entry.name))) output.push(filename);
  }
  return output;
}

function relative(filename) { return path.relative(root, filename).replaceAll("\\", "/"); }
function coreArea(filename) {
  const value = path.relative(coreRoot, filename).replaceAll("\\", "/");
  return value.includes("/") ? value.slice(0, value.indexOf("/")) : "(root)";
}
function moduleReferences(sourceFile) {
  const references = [];
  for (const statement of sourceFile.statements) {
    if ((ts.isImportDeclaration(statement) || ts.isExportDeclaration(statement)) && statement.moduleSpecifier && ts.isStringLiteral(statement.moduleSpecifier)) {
      references.push({ specifier: statement.moduleSpecifier.text, typeOnly: Boolean(statement.importClause?.isTypeOnly || statement.isTypeOnly) });
    }
  }
  return references;
}
function resolveLocal(from, specifier) {
  if (!specifier.startsWith(".")) return undefined;
  const candidate = path.resolve(path.dirname(from), specifier);
  const choices = [candidate, candidate.replace(/\.js$/u, ".ts"), path.join(candidate, "index.ts")];
  return choices.find((choice) => sourceFiles.has(choice));
}
function packageName(specifier) {
  if (!specifier.startsWith("@ephemeralai/")) return undefined;
  return specifier.split("/").slice(0, 2).join("/");
}
function findCycles(graph, label) {
  const state = new Map(); const stack = [];
  const visit = (node) => {
    state.set(node, 1); stack.push(node);
    for (const next of graph.get(node) ?? []) {
      if (!state.has(next)) visit(next);
      else if (state.get(next) === 1) {
        const start = stack.lastIndexOf(next);
        violations.push(`${label} cycle: ${[...stack.slice(start), next].map((item) => label === "source" ? relative(item) : item).join(" -> ")}`);
      }
    }
    stack.pop(); state.set(node, 2);
  };
  for (const node of graph.keys()) if (!state.has(node)) visit(node);
}

const coreFiles = await filesBelow(coreRoot);
const sourceFiles = new Set(coreFiles.map((filename) => path.resolve(filename)));
const populatedAreas = new Set(coreFiles.map(coreArea).filter((area) => area !== "(root)"));
for (const missing of requiredCoreDirectories.difference(populatedAreas)) violations.push(`missing required core directory: ${missing}`);
for (const unexpected of populatedAreas.difference(requiredCoreDirectories)) violations.push(`unapproved core directory: ${unexpected}`);
for (const filename of coreFiles.filter((item) => coreArea(item) === "(root)" && path.basename(item) !== "index.ts")) violations.push(`unapproved root source file: ${relative(filename)}`);

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
  ["operations", new Set(requiredCoreDirectories)],
  ["sqlite", new Set(["sqlite", "cas", "manifests", "namespace", "cow", "resources", "cache", "filesystem", "branches", "revisions"])],
  ["resources", new Set(["resources", "sqlite"])],
  ["streams", new Set(["streams", "resources", "sqlite"])],
  ["cache", new Set(["cache", "cas", "resources"])],
  ["maintenance", new Set(["maintenance", "operations", "filesystem"])],
  ["integrations", new Set(["integrations", "operations", "filesystem"])],
]);
const pureMechanisms = new Set(["cas", "cdc", "cow", "patches", "manifests", "namespace", "branches", "revisions"]);
const graph = new Map(coreFiles.map((filename) => [path.resolve(filename), new Set()]));
for (const filename of coreFiles) {
  const source = await readFile(filename, "utf8");
  const parsed = ts.createSourceFile(filename, source, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS);
  const fromArea = coreArea(filename); const composed = new Set();
  for (const { specifier } of moduleReferences(parsed)) {
    const target = resolveLocal(filename, specifier);
    if (specifier.startsWith(".") && !target) violations.push(`${relative(filename)} has unresolved local import ${specifier}`);
    if (!target) continue;
    graph.get(path.resolve(filename)).add(target);
    const toArea = coreArea(target);
    if (!allowedAreas.get(fromArea)?.has(toArea)) violations.push(`${relative(filename)} violates core direction ${fromArea} -> ${toArea}`);
    if (pureMechanisms.has(toArea) && toArea !== fromArea) composed.add(toArea);
  }
  if (fromArea !== "operations" && fromArea !== "sqlite" && composed.size > 1) violations.push(`${relative(filename)} cross-composes ${[...composed].sort().join(" + ")}; composition belongs in operations`);
  const sqlOwner = fromArea === "sqlite";
  const inspectSql = (node) => {
    if (!sqlOwner && (ts.isStringLiteralLike(node) || ts.isNoSubstitutionTemplateLiteral(node))) {
      const value = node.text.trimStart();
      if (/^(?:SELECT|INSERT|UPDATE|DELETE|CREATE|DROP|ALTER|PRAGMA|REPLACE|WITH|VACUUM|ATTACH|DETACH|BEGIN|COMMIT|ROLLBACK|SAVEPOINT|RELEASE)\b/iu.test(value)) violations.push(`${relative(filename)}:${parsed.getLineAndCharacterOfPosition(node.getStart()).line + 1} contains SQL outside sqlite ownership`);
    }
    ts.forEachChild(node, inspectSql);
  };
  inspectSql(parsed);
}
findCycles(graph, "source");

const allowedPackages = new Map([
  ["@ephemeralai/fs", new Set()],
  ["@ephemeralai/fs-sqlite-node", new Set(["@ephemeralai/fs"])],
  ["@ephemeralai/fs-sqlite-cloudflare", new Set(["@ephemeralai/fs"])],
  ["@ephemeralai/fs-node-vfs", new Set(["@ephemeralai/fs", "@ephemeralai/fs-sqlite-node"])],
  ["@ephemeralai/fs-replication", new Set(["@ephemeralai/fs"])],
  ["@ephemeralai/fs-testkit", new Set(["@ephemeralai/fs"])],
]);
const packageGraph = new Map();
for (const entry of await readdir(packageRoot, { withFileTypes: true })) {
  if (!entry.isDirectory()) continue;
  const directory = path.join(packageRoot, entry.name);
  const manifest = JSON.parse(await readFile(path.join(directory, "package.json"), "utf8"));
  const expected = allowedPackages.get(manifest.name); const edges = new Set(); packageGraph.set(manifest.name, edges);
  if (!expected) violations.push(`unexpected package ${manifest.name}`);
  const declared = { ...(manifest.dependencies ?? {}), ...(manifest.devDependencies ?? {}), ...(manifest.peerDependencies ?? {}) };
  for (const dependency of Object.keys(declared).filter((name) => name.startsWith("@ephemeralai/"))) {
    edges.add(dependency);
    if (!expected?.has(dependency)) violations.push(`${manifest.name} must not declare dependency ${dependency}`);
  }
  const packageSources = await filesBelow(path.join(directory, "src"), new Set([".ts", ".mts", ".cts"]));
  for (const filename of packageSources) {
    const parsed = ts.createSourceFile(filename, await readFile(filename, "utf8"), ts.ScriptTarget.Latest, true, ts.ScriptKind.TS);
    for (const { specifier } of moduleReferences(parsed)) {
      const dependency = packageName(specifier);
      if (!dependency || dependency === manifest.name) continue;
      edges.add(dependency);
      if (!expected?.has(dependency)) violations.push(`${relative(filename)} imports forbidden workspace package ${dependency}`);
      if (!(dependency in declared)) violations.push(`${relative(filename)} imports undeclared workspace package ${dependency}`);
    }
  }
}
findCycles(packageGraph, "package");

if (violations.length) {
  console.error([...new Set(violations)].join("\n"));
  process.exitCode = 1;
} else {
  console.log(`architecture: ${coreFiles.length} source files resolved; exact layout, directions, cycles, composition, SQL ownership, and package graph valid`);
}
