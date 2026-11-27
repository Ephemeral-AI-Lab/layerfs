# Engine review 4 — shared brief

You are reviewing a PLANNED source layout (planning only). Never touch the repository at
/Users/yifanxu/Ephemeral-AI-Lab/layerfs or any git state. Write only your report file.

Scratchpad root S=/private/tmp/claude-501/-Users-yifanxu-Ephemeral-AI-Lab-layerfs/641fd44c-e5a6-4baa-bdc0-04ce0d86438d/scratchpad

Review BOTH organization (grouping, placement) AND naming, recursively: every folder and
every file in your scope, at every depth, plus the crate's tests/ and examples/ layout.

## Read first
- S/layout-style.md — the owner's binding style rules. Shape: leaf folders 3–12 files
  (mod.rs/lib.rs/main.rs not counted; 20 hard ceiling), branch folders ≤4 loose files,
  max depth 4 below the crate dir, 3+ files sharing a prefix become a folder, no thin or
  catch-all folders. Siblings: no long row of unrelated subfolders; siblings of one group
  share one inner design and differ only where explainable in one line; non-members don't
  sit among variants. Naming: predictable from the name alone; never a word that fits
  everything (rejected: calls/, call/, read/, surface/, monitor/, power.rs, support_rows.rs,
  entry.rs); no collision with another LayerFS concept or another folder; no folder repeating
  its crate name, no file repeating its folder name; call files named after the call; short
  nouns; singular for a role, plural for a set; same idea, same word everywhere (across
  crates and layers). Organization: domain first, then role; placement proven by `use` lines.
- §5 of that file lists SETTLED decisions. Do not reopen one unless you find a concrete
  defect with evidence, and then say explicitly that you are reopening it.
- The plan: S/nav/site/layout.json (one JSON record per line: l=layer, o=old path,
  n=new path, c=change, loc=production LOC, why=note). Select rows whose `n` starts with
  your crate path. Exceptions already granted: S/nav/site/exceptions.json. Old source,
  read-only archive of branch layerfs_v2_0_0 at 1317ebf7ca: S/v2tree/<old path>
  (e.g. S/v2tree/engine/crates/layerfs-persistence/src/...). Production LOC per file:
  S/prod_loc_files.txt. Planned ASCII trees: S/nav/trees/tree-{core,engine,control,api}.txt
  and tree-*-folders.txt — use them for cross-crate and cross-layer word consistency.
- Layer order core < engine < control < api. For dependencies between engine crates read
  each crate's Cargo.toml in the archive.

## How to judge
For each folder and file read its `//!` header and its `use`/`crate::`/`super::` lines in the
old source, and grep the archive for who uses it. Ask: does the code match the folder's role?
Does a folder mix two roles? Do sibling variants share one design? Is a name predictable,
colliding, repeating, vague, inconsistent with the same concept elsewhere? Is anything in the
wrong crate (only with `use`-line proof; never propose an upward dependency)? Are counts,
depth and loose-file limits met? Only propose a change where it is clearly better; a big
churn for a small gain is not worth it. If something is fine, say so briefly.

## Report (write to the path your task names)
1. Short verdict per crate.
2. Numbered findings ranked by value. Each: what, evidence (file, header quote, use lines,
   callers), the concrete fix (move/rename/split/merge with exact planned → proposed paths).
3. The proposed layout of every changed folder as a fenced ```diff listing per the repo
   AGENTS.md style: column 1 `+` new / `-` removed / space stays; one path per line;
   two-space indent per level; directories end in `/` with bracketed LOC subtotals ("partial"
   noted when partly shown); every file with production LOC; `—` + `new` for new files;
   short parenthesized annotations.
4. What you checked and left alone, and why (brief).
5. A machine-readable list at the end, in a fenced ```json block:
   [{"from": "<planned new path>", "to": "<proposed path>", "op": "move|rename|split|merge|new",
     "why": "<one line>"}]
   Use full paths starting `engine/crates/...`. For a split, list the original as op "split"
   (to = where the original lands) plus one op "new" entry per new file.

Final message: ≤250 words: top findings and the report path.
