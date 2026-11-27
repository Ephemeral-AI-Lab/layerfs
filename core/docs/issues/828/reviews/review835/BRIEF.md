# Review 835: shared brief

This is planning only. You review a PLANNED source layout for LayerFS 2.0 (issue #828).

Do not touch the repository at /Users/yifanxu/Ephemeral-AI-Lab/layerfs, any worktree under
/Users/yifanxu/Ephemeral-AI-Lab/, or any git state. Write only your report file.

Scratchpad root: S=/private/tmp/claude-501/-Users-yifanxu-Ephemeral-AI-Lab-layerfs/641fd44c-e5a6-4baa-bdc0-04ce0d86438d/scratchpad

## Why this review

The plan was built against branch `layerfs_v2_0_0` at `1317ebf7ca`. Since then the branch
has gained two pieces of work:

- **#818** (PostgreSQL round trips, already merged). It added files in core storage `read/`,
  persistence `acquisition/` and `history/`, and store-service `history/`, `read/` and
  `write/`.
- **#835** (the #819 "host worker", about to merge). A long-lived worker process on the
  host serves CLI Store calls (reads, streamed writes, PostgreSQL held sessions), with its
  own admission, leases and maintenance fences. It also renamed the "native" machine to
  the **host** machine (`platforms/host/`) and the native Sandbox backend to the
  **process** backend (`backend/process/`).

The restructure will be done on this new base, so moving code is cheap now.

**The owner asks: does reaching a clearly better layout need MORE DESTRUCTIVE changes than
the plan makes?** Examples: a new crate or a crate merge, dissolving or regrouping whole
folders, moving a concern to a different crate, or renaming a concept everywhere. Propose
these boldly where they clearly improve the format. Don't propose churn for its own sake,
and say plainly where the plan is already right.

## Read first

- **S/layout-style.md**: the owner's binding style rules.
  - Shape:
    - leaf folders hold 3–12 files; mod.rs, lib.rs and main.rs don't count; 20 is the hard
      ceiling;
    - branch folders have at most 4 loose files;
    - depth is at most 4 levels below the crate dir;
    - 3 or more files that share a prefix become a folder;
    - no thin or catch-all folders.
  - Siblings: a group's members share one inner design. Non-members don't sit among the
    variants.
  - Naming:
    - a name must be predictable from the name alone;
    - never use a word that fits everything (rejected: calls/, call/, read/, surface/,
      snapshot, files, power.rs, entry.rs);
    - no collision with another LayerFS concept;
    - no folder repeats its crate name and no file repeats its folder name;
    - the same idea uses the same word everywhere.
  - Organization:
    - domain first, then role;
    - placement must be proven by `use` lines;
    - the entity crates (machine, store, sandbox, workspace) share one skeleton:
      status/, lifecycle/, record/, route/, backend/.
  - Exception: file-family call folders stay flat, one file per call, with no count limit.
- **§5 of that file lists SETTLED decisions.** Reopen one only with a concrete defect and
  evidence, and say explicitly that you are reopening it.
- **The plan with #835 placements:** S/sim835/site/layout.json, one JSON record per line:
  - l = layer
  - o = old path
  - n = new planned path
  - c = change
  - loc = production LOC
  - why = note

  Rows whose note says "#834/#835 new file" are my first-pass placements of the new files.
  Treat them as proposals to judge, not decisions. The mapping JSONs are in
  S/sim835/mapping-*.json.
- **New source (read-only copy of the merged tree):** S/v835tree/<old path>. Old paths for
  #835's renamed files use the new names (`platforms/host/`, `backend/process/`).
- **Production LOC per file in the new tree:** S/loc_835.txt (columns: path, scope, prod,
  physical).
- **Layer order:** core < engine < control < api. Never propose an upward dependency. Check
  dependencies in each crate's Cargo.toml in S/v835tree.

## How to judge

For every folder and file in your scope:

- Read its `//!` header and its `use` / `crate::` / `super::` lines.
- Grep S/v835tree for its callers.
- Ask:
  - Does the code match the folder's role?
  - Does a folder mix two roles?
  - Does one concern sit scattered over several crates or folders?
  - Do sibling variants share one design?
  - Is a name predictable, or does it collide, repeat or drift from the same concept
    elsewhere?
  - Are the counts, depth and loose-file limits met?

## Report (write to the path your task names)

1. A verdict: is more destructive change needed? Answer yes or no and say why, in a few lines.
2. Numbered findings ranked by value. Each finding gives:
   - what is wrong;
   - the evidence (file, header quote, use lines, callers);
   - the concrete fix, with exact planned → proposed paths;
   - its cost (files touched) beside its gain.
3. The proposed layout of every changed folder, as a fenced ```diff listing in the repo's
   AGENTS.md style:
   - column 1 is `+` for new, `-` for removed, or a space for stays;
   - one path per line, indented two spaces per level;
   - directories end in `/` and carry bracketed LOC subtotals;
   - every file carries its production LOC from S/loc_835.txt;
   - a file that doesn't exist yet shows `—` and `new`;
   - short annotations in parentheses.
4. What you checked and left alone, briefly.
5. A machine-readable list at the end, in a fenced ```json block:
   [{"from": "<planned new path>", "to": "<proposed path>", "op": "move|rename|split|merge|new", "why": "<one line>"}]
   - Use full paths starting with the layer (`control/crates/...`).
   - A folder rename may use folder paths without a file extension.
   - For a split, list the original with op "split" (to = where it lands), plus one op
     "new" entry per new file.

Final message: at most 250 words, giving your verdict, your top findings and the report path.
