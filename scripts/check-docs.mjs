import { readFile, readdir } from "node:fs/promises";
import path from "node:path";
import { documentationLinkErrors } from "./documentation-links.mjs";

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
  for (const error of await documentationLinkErrors(source, filename, { root }))
    missing.push(`${path.relative(root, filename)} -> ${error}`);
}
if (missing.length)
  throw new Error(`broken documentation links:\n${missing.join("\n")}`);
console.log("docs: local links valid");
