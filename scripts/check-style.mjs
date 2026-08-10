import { readFile, readdir } from "node:fs/promises";
import path from "node:path";

const root = path.resolve(import.meta.dirname, "..");
const ignoredDirectories = new Set([".git", "node_modules", "dist", "api-snapshots"]);
const checkedExtensions = new Set([".ts", ".mts", ".cts", ".mjs", ".json", ".yml", ".yaml", ".md"]);
const violations = [];

async function walk(directory) {
  const output = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory() && ignoredDirectories.has(entry.name)) continue;
    const filename = path.join(directory, entry.name);
    if (entry.isDirectory()) output.push(...await walk(filename));
    else if (checkedExtensions.has(path.extname(entry.name))) output.push(filename);
  }
  return output;
}

const prettier = JSON.parse(await readFile(path.join(root, ".prettierrc.json"), "utf8"));
if (prettier.semi !== true || prettier.singleQuote !== false || prettier.proseWrap !== "always" || !Number.isSafeInteger(prettier.printWidth)) {
  violations.push(".prettierrc.json does not contain the approved shared formatting policy");
}
const base = JSON.parse(await readFile(path.join(root, "tsconfig.base.json"), "utf8"));
for (const option of ["strict", "noUncheckedIndexedAccess", "exactOptionalPropertyTypes", "useUnknownInCatchVariables", "verbatimModuleSyntax"]) {
  if (base.compilerOptions?.[option] !== true) violations.push(`tsconfig.base.json must enable ${option}`);
}

const files = await walk(root);
for (const filename of files) {
  const text = await readFile(filename, "utf8");
  const label = path.relative(root, filename).replaceAll("\\", "/");
  if (!text.endsWith("\n")) violations.push(`${label} has no final newline`);
  const lines = text.replaceAll("\r\n", "\n").split("\n");
  for (let index = 0; index < lines.length; index += 1) {
    if (/[\t]/u.test(lines[index])) violations.push(`${label}:${index + 1} contains a tab`);
    if (/[ \t]+$/u.test(lines[index])) violations.push(`${label}:${index + 1} has trailing whitespace`);
  }
  if (filename.endsWith(".json")) {
    try { JSON.parse(text); }
    catch (error) { violations.push(`${label} is invalid JSON: ${error instanceof Error ? error.message : String(error)}`); }
  }
}

if (violations.length) {
  console.error([...new Set(violations)].join("\n"));
  process.exitCode = 1;
} else {
  console.log(`style: shared strict TypeScript/format policy and ${files.length} source/config files pass whitespace, newline, and JSON lint`);
}
