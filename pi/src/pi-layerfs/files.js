// Pi's file tools on a LayerFS Branch: read, write, edit, ls, find and grep
// keep Pi's names, schemas and renderers and call `fs.*`. Reads see the
// Branch's latest Commit; each write or edit is one Commit. Where Pi's
// operations fit one SDK call per step (write, find) the tool runs on them;
// read, edit, ls and grep need calls Pi's operations cannot express (a line
// window, an edit at the head, a typed listing, a literal search), so they
// keep Pi's definition with an execute of their own.
import {
  createEditToolDefinition,
  createFindToolDefinition,
  createGrepToolDefinition,
  createLsToolDefinition,
  createReadToolDefinition,
  createWriteToolDefinition,
  DEFAULT_MAX_BYTES,
  formatSize,
  generateDiffString,
  generateUnifiedPatch,
  truncateHead,
  truncateLine,
} from "@earendil-works/pi-coding-agent";
import { Type } from "typebox";
import { isRefusal } from "./sdk.js";
import { text, upTo } from "./session.js";

const LS_LIMIT = 500;
const GREP_LIMIT = 100;
const UNLIMITED_LINES = { maxLines: Number.MAX_SAFE_INTEGER };

/** `lines` cut to Pi's byte limit, with the notices Pi's listing tools add. */
function listing(lines, notices, details) {
  const truncation = truncateHead(lines.join("\n"), UNLIMITED_LINES);
  if (truncation.truncated) {
    notices.push(`${formatSize(DEFAULT_MAX_BYTES)} limit reached`);
    details.truncation = truncation;
  }
  const output = truncation.content + (notices.length ? `\n\n[${notices.join(". ")}]` : "");
  return text(output, Object.keys(details).length ? details : undefined);
}

/**
 * @param {import("../../../sdk/js/src/index.js").LayerFS} lf
 * @param {string} branch
 * @param {ReturnType<import("./paths.js").branchPaths>} paths
 */
export function fileTools(lf, branch, paths) {
  const root = paths.root;

  const read = {
    ...createReadToolDefinition(root),
    description:
      "Read a text file of the LayerFS Branch. One answer holds at most 2000 lines or 48 KiB; use offset/limit for large files and continue with the offset the answer names. Binary files and images are refused.",
    async execute(_id, { path, offset, limit }, _signal, _onUpdate, ctx) {
      const start = offset ?? 1;
      const r = await lf.fs.read(branch, paths.resolve(path, ctx?.cwd), start, limit === undefined ? undefined : start + limit - 1);
      let output = r.text;
      if (r.lineTruncated) output += `\n\n[Line ${r.endLine} is cut to its first 48 KiB; read the rest with bash.]`;
      if (r.nextLine) output += `\n\n[Showing lines ${r.startLine}-${r.endLine}. Use offset=${r.nextLine} to continue.]`;
      return { ...text(output), structuredContent: output };
    },
  };

  const write = createWriteToolDefinition(root, {
    operations: {
      // the parent folder is made in the write's own Commit
      mkdir: async () => {},
      async writeFile(local, content) {
        const target = paths.branch(local);
        const parent = target.slice(0, target.lastIndexOf("/"));
        const mkdir = parent ? [{ op: "mkdir", path: parent }] : [];
        await lf.fs.apply(branch, [...mkdir, { op: "write", path: target, content }], `write ${target}`);
      },
    },
  });

  const edit = {
    ...createEditToolDefinition(root),
    async execute(_id, { path, edits }, _signal, _onUpdate, ctx) {
      const target = paths.resolve(path, ctx?.cwd);
      // applied to the head at the moment of the Commit, so a change another
      // writer committed since this agent read the file is kept
      const changes = edits.map(({ oldText, newText }) => ({ op: "edit", path: target, oldText, newText }));
      const r = await lf.fs.apply(branch, changes, `edit ${target}`);
      const details = r.kind === "Committed" ? await editDiff(lf, r, target, path) : undefined;
      return text(`Successfully replaced ${edits.length} block(s) in ${path}.`, details);
    },
  };

  const ls = {
    ...createLsToolDefinition(root),
    async execute(_id, { path, limit }, _signal, _onUpdate, ctx) {
      const dir = paths.resolve(path || ".", ctx?.cwd);
      const max = limit ?? LS_LIMIT;
      const { items, more } = await upTo(max, (size, pos) => lf.fs.list(branch, dir, size, pos));
      if (!items.length) return text("(empty directory)");
      const notices = more ? [`${max} entries limit reached. Use limit=${max * 2} for more`] : [];
      return listing(items.map((e) => (e.type === "dir" ? `${e.name}/` : e.name)), notices, more ? { entryLimitReached: max } : {});
    },
  };

  const find = {
    ...createFindToolDefinition(root, {
      operations: {
        exists: (local) =>
          lf.fs.stat(branch, paths.branch(local)).then(
            () => true,
            (error) => {
              if (isRefusal(error, "NoSuchPath")) return false;
              throw error;
            },
          ),
        async glob(pattern, local, { limit }) {
          const { items } = await upTo(limit, (size, pos) => lf.fs.glob(branch, anyDepth(pattern), paths.branch(local), size, pos));
          return items.map(paths.local);
        },
      },
    }),
    description:
      "Search the LayerFS Branch for files by glob pattern. Returns matching files and folders relative to the search directory, ignored files and node_modules included. Output is truncated to 1000 results or 50KB.",
  };

  const grep = {
    ...createGrepToolDefinition(root),
    description: `Search the text files of the LayerFS Branch for a literal string, case-sensitively. Returns matching lines with file paths and line numbers, ignored files and node_modules included. Not a regular expression: for one, run grep -rnE in bash. Output is truncated to ${GREP_LIMIT} matches or 50KB.`,
    parameters: Type.Object({
      pattern: Type.String({ description: "Literal text to find; case-sensitive, not a regular expression" }),
      path: Type.Optional(Type.String({ description: "Directory or file to search (default: current directory)" })),
      limit: Type.Optional(Type.Number({ description: `Maximum number of matches to return (default: ${GREP_LIMIT})` })),
    }),
    async execute(_id, { pattern, path, limit }, _signal, _onUpdate, ctx) {
      const where = paths.resolve(path || ".", ctx?.cwd);
      const max = Math.max(1, limit ?? GREP_LIMIT);
      const { items, more } = await upTo(max, (size, pos) => lf.fs.grep(branch, pattern, where, size, pos));
      if (!items.length) return text("No matches found");
      let cut = false;
      const lines = items.map((m) => {
        const shown = truncateLine(m.text.replace(/\r?\n$/, ""));
        cut ||= shown.wasTruncated || m.lineTruncated;
        return `${relativeTo(where, m.path)}:${m.line}: ${shown.text}`;
      });
      const notices = [], details = {};
      if (more) {
        notices.push(`${max} matches limit reached. Use limit=${max * 2} for more, or refine pattern`);
        details.matchLimitReached = max;
      }
      if (cut) {
        notices.push("Some lines truncated. Use read tool to see full lines");
        details.linesTruncated = true;
      }
      return listing(lines, notices, details);
    },
  };

  return [read, write, edit, ls, find, grep];
}

/**
 * Pi's find (fd) matches a pattern with "/" anywhere below the search folder;
 * a LayerFS glob anchors such a pattern at the search folder, so it gets a
 * leading "**\/". A pattern without "/" matches at any depth in both.
 */
const anyDepth = (pattern) => (!pattern.includes("/") || pattern.startsWith("/") || pattern.startsWith("**/") ? pattern : `**/${pattern}`);

/** A match's path as Pi's grep shows it: relative to a searched folder, the file name for a searched file. */
const relativeTo = (where, path) => (path === where ? path.slice(path.lastIndexOf("/") + 1) : path.slice(where === "/" ? 1 : where.length + 1));

/**
 * The details Pi's edit renderer draws, from LayerFS's line difference of the
 * file between the Commit's parent and the Commit. The difference lists every
 * line of both sides, so it rebuilds both texts; one too large for an answer
 * leaves the details out.
 */
async function editDiff(lf, r, target, shown) {
  const d = await lf.fs.diff_files(r.commit.parent ?? r.commit.base, target, r.head, target);
  if (d.hunksOmitted) return undefined;
  const side = (other) => d.hunks.filter((h) => h.op !== other).map((h) => h.text).join("\n");
  const before = side("+"), after = side("-");
  return { ...generateDiffString(before, after), patch: generateUnifiedPatch(shown, before, after) };
}
