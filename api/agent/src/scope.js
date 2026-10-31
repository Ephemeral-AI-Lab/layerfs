// P13 folder and Branch-namespace guards, shared by named methods and client.call.
import { LayerFSError } from "./errors.js";

const HISTORY_NAME = /^[a-z0-9](?:[a-z0-9_.-]{0,61}[a-z0-9])?$/;
const IDENTITY = /^[0-9a-f]{66}$/;
const HISTORY = new Set(["branch.list_commits", "branch.graph", "branch.changed_paths", "branch.diff_commits", "branch.diff_branches"]);
const PATHS = {
  "fs.list": ["path"], "fs.glob": ["path"], "fs.grep": ["path"],
  "fs.stat": ["path"], "fs.read": ["path"], "fs.blame": ["path"],
  "fs.write": ["path"], "fs.edit": ["path"], "fs.mkdir": ["path"],
  "fs.rm": ["path"], "fs.cp": ["from", "to"], "fs.mv": ["from", "to"],
};
const FILE_REFS = new Set(Object.keys(PATHS));
FILE_REFS.add("fs.apply");
FILE_REFS.add("fs.diff_files");

export function outOfScope(kind) {
  return new LayerFSError("InvalidInput", `out of scope: ${kind}`, null);
}

const plain = (value) => value !== null && typeof value === "object" && !Array.isArray(value);
const canonical = (path) => typeof path === "string" && (path === "/" || (path.startsWith("/") &&
  !path.split("/").slice(1).some((part) => !part || part === "." || part === "..")));

/** Validates and snapshots LayerFS.open's optional scope. */
export function checkScope(scope) {
  if (scope === undefined) return null;
  if (!plain(scope)) throw new LayerFSError("InvalidInput", "scope must be { folder, branch? }", null);
  for (const key of Object.keys(scope)) {
    if (key !== "folder" && key !== "branch") throw new LayerFSError("InvalidInput", `unknown scope option "${key}"`, null);
  }
  if (!canonical(scope.folder)) throw new LayerFSError("InvalidInput", "scope.folder must be a canonical project-root absolute path", null);
  if (scope.branch !== undefined && (typeof scope.branch !== "string" || !HISTORY_NAME.test(scope.branch))) {
    throw new LayerFSError("InvalidInput", "scope.branch must be a slash-free HistoryName namespace prefix", null);
  }
  return { folder: scope.folder, ...(scope.branch === undefined ? {} : { branch: scope.branch }) };
}

function inside(path, folder) {
  if (!canonical(path)) return false;
  return folder === "/" || path === folder || path.startsWith(`${folder}/`);
}

function branch(scope, name) {
  if (!scope.branch) throw outOfScope("branch");
  const composed = `${scope.branch}.${name}`;
  if (typeof name !== "string" || !HISTORY_NAME.test(name) || !HISTORY_NAME.test(composed)) throw outOfScope("branch");
  return composed;
}

function ref(scope, name) {
  return typeof name === "string" && IDENTITY.test(name) ? name : branch(scope, name);
}

/** One `fs.apply` change's paths: a move or copy names both sides. */
function changePaths(change) {
  return change?.op === "cp" || change?.op === "mv" ? [change.from, change.to] : [change?.path];
}

function pathsChecked(scope, method, args) {
  if (method === "fs.apply") {
    if (!Array.isArray(args.changes)) throw outOfScope("path");
    for (const change of args.changes) {
      for (const path of changePaths(change)) {
        if (!inside(path, scope.folder)) throw outOfScope("path");
      }
    }
  } else if (method === "fs.diff_files") {
    if (!inside(args.left?.path, scope.folder) || !inside(args.right?.path, scope.folder)) throw outOfScope("path");
  } else {
    for (const key of PATHS[method] ?? []) {
      if (!inside(args[key], scope.folder)) throw outOfScope("path");
    }
  }
}

/** One scoped raw operation's binding arguments, or throws before dispatch. */
export function scopeArgs(scope, method, args) {
  if (!scope) return args;
  if (HISTORY.has(method)) throw outOfScope("history");
  if (method === "events.subscribe") throw outOfScope("events");
  const given = { ...args };
  if (FILE_REFS.has(method)) pathsChecked(scope, method, given);
  if (FILE_REFS.has(method)) given.branch = ref(scope, given.branch);
  if (method === "fs.diff_files") {
    given.left = { ...given.left, commit: ref(scope, given.left?.commit), path: given.left?.path };
    given.right = { ...given.right, commit: ref(scope, given.right?.commit), path: given.right?.path };
  } else if (method === "branch.fork") {
    given.name = branch(scope, given.name);
    given.from = ref(scope, given.from);
  } else if (method === "branch.merge") {
    given.branch = branch(scope, given.branch);
    given.with = branch(scope, given.with);
    given.name = branch(scope, given.name);
  } else if (method === "branch.publish") {
    given.branch = branch(scope, given.branch);
  } else if (method === "project.list_branches") {
    if (!scope.branch) throw outOfScope("branch");
  } else if (method === "workspace.mount") {
    if (given.branch !== undefined && given.branch !== "main") throw outOfScope("branch");
    given.branch = branch(scope, "main");
  } else if (method === "workspace.exec") {
    if (given.cwd !== undefined && given.cwd !== scope.folder) throw outOfScope("path");
    given.cwd = scope.folder;
  }
  return given;
}

/** Branch records relative to the scope's namespace, for filtered paging. */
export function scopeBranches(scope, items) {
  const prefix = `${scope.branch}.`;
  return items.filter((item) => typeof item.name === "string" && item.name.startsWith(prefix))
    .map((item) => ({ ...item, name: item.name.slice(prefix.length) }));
}
