// Pi's tools name local paths; LayerFS names paths in a Branch. The session's
// working directory stands for the Branch root "/": a path maps by its place
// under that directory, and nothing outside it exists in the Branch.
import path from "node:path";

/** Path mapping for a session whose working directory is `root`. */
export function branchPaths(root) {
  /** An absolute local path as its Branch path. */
  const branch = (local) => {
    const rel = path.relative(root, local);
    if (rel === ".." || rel.startsWith(`..${path.sep}`) || path.isAbsolute(rel)) {
      throw new Error(`${local} is outside ${root}, the LayerFS Branch root "/"; name a path inside it`);
    }
    return `/${rel.split(path.sep).join("/")}`;
  };
  return {
    root,
    branch,
    /** A tool's path argument, relative to `cwd` or absolute, as its Branch path. Pi's tools drop a leading "@" the same way. */
    resolve: (input, cwd = root) => branch(path.resolve(cwd, input.startsWith("@") ? input.slice(1) : input)),
    /** A Branch path as the local path Pi's tools show. */
    local: (branchPath) => path.join(root, branchPath),
  };
}
