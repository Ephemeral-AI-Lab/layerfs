// Which calls each launch profile exposes. Pure: it reads the table entries it
// is given, imports nothing, and writes no call name of its own beyond the
// small exception lists below. `tools()` is the one generated tool list the
// server, the documentation table and the coverage column use.
//
// A migrated call (call surface 2.0) says its own profile in its table entry
// (`form.profile`): `agent` is the `work` bucket and `full` the `admin`
// bucket, and no list here names it. Every other call follows the
// assignment rule (each call lands in exactly one bucket):
//   work   the families an agent works in (branch, commit, file, workspace)
//          plus the read-only project calls named below
//   admin  every other call an agent may make: project create/delete/
//          remove_file, Branch remove, and the home, machine, store and
//          backup families
//   ui     the monitoring calls a UI polls (layerfs-project#178 D2, #164): in no
//          profile, so no agent is offered them
//   none   the doctor family: like `layerfs doctor` itself, which is no
//          tool, its repair call is a person's, in neither profile
// The `admin` launch profile exposes the union of `work` and `admin`, so
// `admin` adds the administrative calls to the `work` set.

/** The profile names, in help order. `work` is the default. */
export const PROFILES = ["work", "admin"];

/** Families whose every call belongs to the `work` profile. */
const WORK_FAMILIES = new Set(["branch", "commit", "file", "workspace"]);

/** Read-only calls of families that are otherwise administration. */
const WORK_CALLS = new Set(["project.list", "project.get", "project.stats", "project.export"]);

/** Administrative calls outside the admin families. */
const ADMIN_CALLS = new Set(["project.init", "project.delete", "project.remove_file", "branch.remove"]);

/** Monitoring calls for a UI, which no profile lists (layerfs-project#178 D2, #164). */
const UI_CALLS = new Set(["machine.stats", "machine.files", "machine.log"]);

/** Families whose every call belongs to the `admin` profile. */
const ADMIN_FAMILIES = new Set(["home", "machine", "store", "backup"]);

/** Families that are no MCP tool in any profile. */
const NO_TOOL_FAMILIES = new Set(["doctor"]);

/** Why a family's calls sit where they do; one line per family, in the doc. */
const WHY = {
  home: "set the home up, read its state and what this build would use with it, and read or clear its call log; administration",
  project: "read a project's record or its entry counts, or export one (work); importing, deleting or hiding a path in a project is administration",
  branch: "work on Branches: list, read, fork, merge, publish and diff",
  commit: "read a Branch's Commit history",
  file: "read and change files in a Branch; each write is one explicit Commit",
  sandbox: "start a Sandbox (work); read, remove and inspect one (administration)",
  workspace: "mount, run in, commit and unmount a Workspace explicitly; nothing is implicit",
  machine: "machines hold Stores and workers; creating, changing and removing them is administration",
  store: "create, connect, transfer and remove Stores; administration",
  backup: "upload and restore a Store on a bucket; administration",
  doctor: "remove a leftover the doctor's report listed; like `layerfs doctor` itself, a person's repair and no MCP tool",
};

const nameOf = (spec) => `${spec.family}.${spec.call}`;

/**
 * The profile bucket of one table entry: `"work"`, `"admin"`, `"ui"` or
 * `"none"` (the last two in no profile), or `null` when this file has not
 * classified the call (a new family or a renamed call).
 */
export function bucketOf(spec) {
  const name = nameOf(spec);
  // a migrated entry classifies itself
  if (spec.form?.profile === "agent") return "work";
  if (spec.form?.profile === "full") return "admin";
  // The admin exception list is checked before the work families: a retired
  // entity in a work family (`branch.remove`) is still administration (§10),
  // and a monitoring call a UI polls is offered to no agent.
  if (UI_CALLS.has(name)) return "ui";
  if (ADMIN_CALLS.has(name)) return "admin";
  if (WORK_CALLS.has(name)) return "work";
  if (WORK_FAMILIES.has(spec.family)) return "work";
  if (ADMIN_FAMILIES.has(spec.family)) return "admin";
  if (NO_TOOL_FAMILIES.has(spec.family)) return "none";
  return null;
}

/** Why this call is in its bucket: a line for the generated documentation table. */
export function whyOf(spec) {
  if (UI_CALLS.has(nameOf(spec))) return "a monitoring read a UI polls (a machine's stats, disk space, files or worker log); offered to no agent";
  if (spec.family === "project" && spec.call === "init") return "import a host folder into a Store; administration";
  if (spec.family === "project" && spec.call === "delete") return "retire a project in its Store and remove its home entry; administration; a folder scope refuses it";
  if (spec.family === "project" && spec.call === "remove_file") return "hide one absolute path from every node at or below its marker frontier; administration; a folder scope refuses it";
  if (spec.family === "branch" && spec.call === "remove") return "retire one Branch in its Store; administration; a folder scope refuses it";
  if (spec.family === "store" && spec.call === "compact") return "reclaim one Store's dead bytes; administration; a folder scope refuses it";
  if (spec.family === "sandbox" && spec.call === "create") return "start a Sandbox, and with --mount mount a Branch in it; work";
  if (spec.family === "sandbox" && spec.call === "remove") return "stop a Sandbox and remove it; administration";
  if (spec.family === "project" && spec.call === "export") return "export a Branch snapshot to a local folder under a launch-time --export-root; a folder scope refuses it";
  return WHY[spec.family] ?? "not classified";
}

/**
 * The table entries a launch profile exposes: `work` its own bucket, `admin`
 * the `work` and `admin` buckets; a `ui` or `none` call is in neither. Throws
 * when a call has no bucket, so a new call cannot appear in a profile silently.
 */
export function tools(table, profile = "work") {
  if (!PROFILES.includes(profile)) throw new Error(`unknown profile "${profile}"; one of ${PROFILES.join(", ")}`);
  const out = [];
  for (const spec of table) {
    const bucket = bucketOf(spec);
    if (bucket === null) throw new Error(`${nameOf(spec)}: no profile bucket; add it to api/mcp/src/profiles.js`);
    if (bucket === "ui" || bucket === "none") continue;
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
  for (const name of [...WORK_CALLS, ...ADMIN_CALLS, ...UI_CALLS]) {
    if (!seen.has(name)) problems.push(`${name}: a profile exception that the table does not have`);
  }
  return problems;
}

/** The generated Markdown rows of the profile table, one row per call in table order. */
export function renderRows(table) {
  return table.map((spec) => {
    const bucket = bucketOf(spec);
    const listed = bucket === "work" ? "work, admin" : bucket === "ui" || bucket === "none" ? "none" : "admin";
    return `| \`${nameOf(spec)}\` | ${bucket ?? "UNCLASSIFIED"} | ${listed} | ${whyOf(spec)} |`;
  });
}
