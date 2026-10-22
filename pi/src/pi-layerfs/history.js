// The `layerfs` tool: the project's history around the session's Branch.
// status lists the Branches, log a Branch's Commits, diff and diff_commits
// the paths two Branches or two Commits differ in, merge joins two Branches
// into a new one, and publish makes main show a Branch. Each action is one
// SDK call or one paged listing.
import { Type } from "typebox";
import { MAX_PAGE, text, upTo } from "./session.js";

const LOG_LIMIT = 20;
const DIFF_LIMIT = 200;
const ID_CHARS = 12;

const short = (id) => (id ? id.slice(0, ID_CHARS) : "-");
const moreNote = (more, offset, count) => (more ? `\n[more: use offset=${offset + count}]` : "");

const parameters = Type.Object({
  action: Type.Union(["status", "log", "diff", "diff_commits", "merge", "publish"].map((name) => Type.Literal(name)), {
    description: "status: Branches and their heads. log: a Branch's Commits, newest first. diff: paths that differ between two Branches or main. diff_commits: paths that differ between two Commits. merge: join Branches a and b into a new Branch. publish: make main show a Branch",
  }),
  branch: Type.Optional(Type.String({ description: "log, publish: the Branch (default: this session's)" })),
  from: Type.Optional(Type.String({ description: "diff: a Branch or main to compare from. diff_commits: a Commit id from log" })),
  to: Type.Optional(Type.String({ description: "diff: a Branch or main to compare to (default: this session's Branch). diff_commits: a Commit id from log" })),
  a: Type.Optional(Type.String({ description: "merge: first Branch; its /.lww rules apply" })),
  b: Type.Optional(Type.String({ description: "merge: second Branch, or main" })),
  name: Type.Optional(Type.String({ description: "merge: name of the new Branch" })),
  message: Type.Optional(Type.String({ description: "merge: message of the merge Commit" })),
  limit: Type.Optional(Type.Number({ description: `log, diff, diff_commits: how many (default: ${LOG_LIMIT} Commits, ${DIFF_LIMIT} paths)` })),
  offset: Type.Optional(Type.Number({ description: "log, diff, diff_commits: how many to skip (default: 0)" })),
});

/** One page of a diff as `<kind> <path>` lines. */
async function changes(answer, from, to, offset) {
  const page = await answer;
  if (!page.items.length) return text(`no differences between ${from} and ${to}`);
  return text(page.items.map((d) => `${d.kind} ${d.path}`).join("\n") + moreNote(page.hasMore, offset, page.items.length));
}

/**
 * @param {import("../../../sdk/js/src/index.js").LayerFS} lf
 * @param {string} branch the session's Branch
 */
export function historyTool(lf, branch) {
  const actions = {
    async status() {
      const { items } = await upTo(Number.MAX_SAFE_INTEGER, (size, pos) => lf.project.list_branches(size, pos));
      const lines = items.map((b) =>
        b.name === "main"
          ? `main  Layer ${short(b.base)} (published; read-only)`
          : `${b.name}  head ${short(b.head)}  base ${short(b.base)}${b.name === branch ? "  (this session)" : ""}`,
      );
      return text(`project ${lf.projectName}\n${lines.join("\n")}`);
    },
    async log({ branch: name = branch, limit = LOG_LIMIT, offset = 0 }) {
      const page = await lf.branch.list_commits(name, Math.min(limit, MAX_PAGE), offset);
      if (!page.items.length) return text(`${name} has no Commits${offset ? ` after ${offset}` : ""}`);
      // full ids: diff_commits takes them back
      const lines = page.items.map((c) => `${c.id} ${c.via} ${c.message}`);
      return text(lines.join("\n") + moreNote(page.hasMore, offset, page.items.length));
    },
    diff: ({ from, to = branch, limit = DIFF_LIMIT, offset = 0 }) =>
      changes(lf.branch.diff_branches(from, to, Math.min(limit, MAX_PAGE), offset, { hunks: false }), from, to, offset),
    diff_commits: ({ from, to, limit = DIFF_LIMIT, offset = 0 }) =>
      changes(lf.branch.diff_commits(from, to, Math.min(limit, MAX_PAGE), offset, { hunks: false }), from, to, offset),
    async merge({ a, b, name, message }) {
      const m = await lf.branch.merge(a, b, name, message);
      const lww = m.lww.map((d) => `\n  ${d.path}: kept ${d.kept} (${d.rule})`).join("");
      return text(`Branch ${m.branch}, head ${short(m.head)}${m.fastForward ? " (fast-forward)" : ""}${lww ? `\nlast-writer-wins:${lww}` : ""}`);
    },
    async publish({ branch: name = branch }) {
      const p = await lf.branch.publish(name);
      const overwrote = p.overwrote?.length ? `; skipped Layers ${p.overwrote.map(short).join(", ")}` : "";
      return text(`${p.kind}: main shows ${name}${overwrote}`);
    },
  };
  return {
    name: "layerfs",
    label: "LayerFS",
    description:
      "LayerFS history of this project. status lists the Branches; log lists a Branch's Commits; diff lists the paths two Branches (or main) differ in, diff_commits those two Commits from log differ in; merge joins Branches a and b into a new Branch name (a and b stay as they are); publish makes main show a Branch's head.",
    promptSnippet: "LayerFS history: Branches, Commits, differences, merge and publish",
    parameters,
    execute: (_id, params) => actions[params.action](params),
  };
}
