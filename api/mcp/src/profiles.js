// Which calls each launch profile exposes. Pure: it reads the table entries it
// is given, imports nothing, and writes no call name of its own beyond the
// small exception lists below. `tools()` is the one generated tool list the
// server, the documentation table and the coverage column use.
//
// Assignment rule (each call lands in exactly one bucket, none excluded):
//   work   the families an agent works in (branch, commit, file, workspace)
//          plus the read-only project and Sandbox calls named below
//   admin  every other call: project create/remove, Sandbox create/close,
//          and the machine, store and backup families
// The `admin` launch profile exposes the union, so `admin` adds the
// administrative calls to the `work` set.

/** The profile names, in help order. `work` is the default. */
export const PROFILES = ["work", "admin"];

/** Families whose every call belongs to the `work` profile. */
const WORK_FAMILIES = new Set(["branch", "commit", "file", "workspace"]);

/** Read-only calls of families that are otherwise administration. */
const WORK_CALLS = new Set(["project.list", "project.get", "project.export", "sandbox.list", "sandbox.get"]);

/** Administrative calls of the project and sandbox families. */
const ADMIN_CALLS = new Set(["project.init", "project.remove", "sandbox.create", "sandbox.close"]);

/** Families whose every call belongs to the `admin` profile. */
const ADMIN_FAMILIES = new Set(["machine", "store", "backup"]);

/** Why a family's calls sit where they do; one line per family, in the doc. */
const WHY = {
  project: "read a project's record or export one (work); importing and removing a project are administration",
  branch: "work on Branches: list, read, fork, merge, publish and diff",
  commit: "read a Branch's Commit history",
  file: "read and change files in a Branch; each write is one explicit Commit",
  sandbox: "read a Sandbox and its Workspaces (work); starting and closing one is administration",
  workspace: "mount, run in, commit and unmount a Workspace explicitly; nothing is implicit",
  machine: "machines hold Stores and workers; creating, changing and removing them is administration",
  store: "create, connect, transfer and remove Stores; administration",
  backup: "upload and restore a Store on a bucket; administration",
};

const nameOf = (spec) => `${spec.family}.${spec.call}`;

/**
 * The profile bucket of one table entry: `"work"` or `"admin"`, or `null` when
 * this file has not classified the call (a new family or a renamed call).
 */
export function bucketOf(spec) {
  const name = nameOf(spec);
  if (WORK_CALLS.has(name)) return "work";
  if (WORK_FAMILIES.has(spec.family)) return "work";
  if (ADMIN_CALLS.has(name)) return "admin";
  if (ADMIN_FAMILIES.has(spec.family)) return "admin";
  return null;
}

/** Why this call is in its bucket: a line for the generated documentation table. */
export function whyOf(spec) {
  if (spec.family === "project" && spec.call === "init") return "import a host folder into a Store; administration";
  if (spec.family === "project" && spec.call === "remove") return "remove a project from its Store; administration";
  if (spec.family === "sandbox" && spec.call === "create") return "start a Sandbox process; administration";
  if (spec.family === "sandbox" && spec.call === "close") return "stop a Sandbox; administration";
  if (spec.family === "project" && spec.call === "export") return "export a Branch snapshot to a local folder; a folder scope refuses it";
  return WHY[spec.family] ?? "not classified";
}

/**
 * The table entries a launch profile exposes: `work` its own bucket, `admin`
 * both buckets. Throws when a call has no bucket, so a new call cannot appear
 * in a profile silently.
 */
export function tools(table, profile = "work") {
  if (!PROFILES.includes(profile)) throw new Error(`unknown profile "${profile}"; one of ${PROFILES.join(", ")}`);
  const out = [];
  for (const spec of table) {
    const bucket = bucketOf(spec);
    if (bucket === null) throw new Error(`${nameOf(spec)}: no profile bucket; add it to api/mcp/src/profiles.js`);
    if (profile === "admin" || bucket === "work") out.push(spec);
  }
  return out;
}

/** Everything that keeps the buckets equal to the table: `[]` when they are. */
export function check(table) {
  const problems = [];
  const seen = new Set();
  for (const spec of table) {
    const name = nameOf(spec);
    if (seen.has(name)) problems.push(`${name}: appears twice in the table`);
    seen.add(name);
    if (bucketOf(spec) === null) problems.push(`${name}: no profile bucket`);
  }
  for (const name of [...WORK_CALLS, ...ADMIN_CALLS]) {
    if (!seen.has(name)) problems.push(`${name}: a profile exception that the table does not have`);
  }
  return problems;
}

/** The generated Markdown rows of the profile table, one row per call in table order. */
export function renderRows(table) {
  return table.map((spec) => {
    const bucket = bucketOf(spec);
    return `| \`${nameOf(spec)}\` | ${bucket ?? "UNCLASSIFIED"} | ${bucket === "work" ? "work, admin" : "admin"} | ${whyOf(spec)} |`;
  });
}
