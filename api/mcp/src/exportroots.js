// The export allowlist. `project.export` writes to a path on this server's own
// host, and the client chooses that path, so the destination must lie inside a
// directory the operator named at launch with `--export-root`. With no root
// configured the call is refused: the allowlist is empty, not open.
//
// This is the one check the server makes that the library does not. The
// library still applies its own destination rules (absolute path, no `..`,
// not inside the Store home, absent or empty) and the folder scope; this runs
// first and only decides whether the place is one the operator allowed.
//
// The check is made before the call. It does not protect against another local
// process that can rewrite directories under a root while an export runs.

import { realpathSync, statSync } from "node:fs";
import { basename, dirname, isAbsolute, join, relative, sep } from "node:path";

/**
 * The real paths of the `--export-root` values. Throws an Error whose message
 * is the usage error: a root must be an absolute path to an existing directory
 * other than the filesystem root.
 */
export function resolveRoots(given) {
  const roots = [];
  for (const root of given) {
    if (root === "") throw new Error("--export-root needs a directory");
    if (!isAbsolute(root)) throw new Error(`--export-root must be an absolute path, got "${root}"`);
    let real;
    try {
      real = realpathSync(root);
    } catch {
      throw new Error(`--export-root "${root}" does not exist`);
    }
    if (!statSync(real).isDirectory()) throw new Error(`--export-root "${root}" is not a directory`);
    if (dirname(real) === real) throw new Error("--export-root must not be the filesystem root");
    if (!roots.includes(real)) roots.push(real);
  }
  return roots;
}

/** True when `path` is `root` or lies beneath it (a sibling such as `<root>-x` does not). */
function within(root, path) {
  const rel = relative(root, path);
  return rel === "" || (rel !== ".." && !rel.startsWith(`..${sep}`) && !isAbsolute(rel));
}

/**
 * Why `destination` may not be exported to, or `null` when it may. A refusal
 * means nothing was attempted. The destination's parent is resolved through
 * symlinks first, so a link inside a root that points out of it does not pass.
 */
export function exportRefusal(destination, roots) {
  if (roots.length === 0) {
    return "project.export: no export root is configured on this server; the operator must start it with --export-root <dir> to allow exports";
  }
  const outside = `project.export: the destination is outside the export roots this server allows (${roots.join(", ")})`;
  if (typeof destination !== "string" || !isAbsolute(destination)) return outside;
  if (destination.split(sep).includes("..")) return outside;
  let parent;
  try {
    parent = realpathSync(dirname(destination));
  } catch {
    // the library refuses a missing parent in its own words; here it is simply not allowed
    return outside;
  }
  const resolved = join(parent, basename(destination));
  return roots.some((root) => within(root, resolved)) ? null : outside;
}
