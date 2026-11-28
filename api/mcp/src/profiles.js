// Which calls each launch profile exposes. Pure: it reads the table entries it
// is given, imports nothing, and names no call to classify it. `tools()` is the
// one generated tool list the server, the documentation table and the coverage
// column use.
//
// Every call says its own profile in its table entry (`form.profile`, call
// surface 2.0): `agent` is the `work` bucket and `full` the `admin` bucket. The
// `admin` launch profile exposes the union of both buckets, so `admin` adds the
// administrative calls to the `work` set. An entry without a profile has no
// bucket, which `tools()` and `check()` refuse.

/** The profile names, in help order. `work` is the default. */
export const PROFILES = ["work", "admin"];

/** What a family's calls are for; one line per family, in the doc. */
const WHY = {
  home: "set the home up, read its state and call log, and check it",
  project: "import, list, read, count, export, delete or check a project, or hide a path in it",
  branch: "list, read, fork, merge, publish and remove Branches",
  commit: "read a Branch's Commit history and compare Commits",
  file: "read and change files in a Branch; each write is one explicit Commit",
  sandbox: "start, read, remove, inspect and check a Sandbox",
  workspace: "mount, run in, cancel, commit and unmount a Workspace explicitly; nothing is implicit",
  machine: "machines hold Stores and workers: read, start, stop, create, change, inspect and check them",
  store: "create, connect, attach, move, compact, upgrade, check and remove Stores",
  backup: "connect buckets, back up and restore a Store, and check a bucket",
};

/** Lines that say more than the family's line, by call. */
const CALL_WHY = {
  "project.init": "import a host folder into a Store",
  "project.delete": "retire a project in its Store and remove its home entry; a folder scope refuses it",
  "project.hide_path": "hide one absolute path from the project's whole history; a folder scope refuses it",
  "project.export": "export a Branch snapshot to a local folder under a launch-time --export-root; a folder scope refuses it",
  "branch.remove": "retire one Branch in its Store; a folder scope refuses it",
  "store.compact": "reclaim one Store's dead bytes; a folder scope refuses it",
  "store.upgrade": "bring an older-format Store to the latest format, in place; Ctrl-C cancels it before COMMIT",
  "store.repair_mount_holders": "the operator's explicit repair of the admissions that hold one Branch: list, preview, settle, release or audit",
  "home.move": "move this home into another folder on the same filesystem; client folders stay",
  "sandbox.create": "start a Sandbox, and with --mount mount a Branch in it",
  "sandbox.remove": "stop a Sandbox and remove it",
};

const nameOf = (spec) => `${spec.family}.${spec.call}`;

/**
 * The profile bucket of one table entry, from its own `form.profile`:
 * `"work"` (`agent`) or `"admin"` (`full`), or `null` when the entry names no
 * profile (a call this file cannot place).
 */
export function bucketOf(spec) {
  if (spec.form?.profile === "agent") return "work";
  if (spec.form?.profile === "full") return "admin";
  return null;
}

/** Why this call is in its bucket: a line for the generated documentation table. */
export function whyOf(spec) {
  const what = CALL_WHY[nameOf(spec)] ?? WHY[spec.family] ?? "not classified";
  const bucket = bucketOf(spec);
  if (bucket === "work") return `${what}; the \`agent\` profile`;
  if (bucket === "admin") return `${what}; the \`full\` profile (administration)`;
  return what;
}

/**
 * The table entries a launch profile exposes: `work` its own bucket, `admin`
 * the `work` and `admin` buckets. Throws when a call has no bucket, so a new
 * call cannot appear in a profile silently.
 */
export function tools(table, profile = "work") {
  if (!PROFILES.includes(profile)) throw new Error(`unknown profile "${profile}"; one of ${PROFILES.join(", ")}`);
  const out = [];
  for (const spec of table) {
    const bucket = bucketOf(spec);
    if (bucket === null) throw new Error(`${nameOf(spec)}: no profile bucket; its table entry names no form.profile`);
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
  for (const name of Object.keys(CALL_WHY)) {
    if (!seen.has(name)) problems.push(`${name}: a profile note for a call the table does not have`);
  }
  return problems;
}

/** The generated Markdown rows of the profile table, one row per call in table order. */
export function renderRows(table) {
  return table.map((spec) => {
    const bucket = bucketOf(spec);
    const listed = bucket === "work" ? "work, admin" : bucket === "admin" ? "admin" : "none";
    return `| \`${nameOf(spec)}\` | ${bucket ?? "UNCLASSIFIED"} | ${listed} | ${whyOf(spec)} |`;
  });
}
