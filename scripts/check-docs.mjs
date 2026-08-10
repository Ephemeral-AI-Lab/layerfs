import { readFile, readdir } from "node:fs/promises";
import path from "node:path";

const root = path.resolve(import.meta.dirname, "..");
async function walk(directory) {
  const result = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const filename = path.join(directory, entry.name);
    if (entry.isDirectory() && entry.name !== "node_modules")
      result.push(...(await walk(filename)));
    else if (entry.isFile() && entry.name.endsWith(".md")) result.push(filename);
  }
  return result;
}

const missing = [];
for (const filename of await walk(root)) {
  const source = await readFile(filename, "utf8");
  for (const match of source.matchAll(/\[[^\]]+\]\(([^)#]+)(?:#[^)]+)?\)/g)) {
    const target = match[1];
    if (!target || /^(?:https?:|mailto:)/.test(target)) continue;
    try {
      await readFile(path.resolve(path.dirname(filename), decodeURIComponent(target)));
    } catch {
      missing.push(`${path.relative(root, filename)} -> ${target}`);
    }
  }
}
if (missing.length)
  throw new Error(`broken documentation links:\n${missing.join("\n")}`);
console.log("docs: local links valid");
