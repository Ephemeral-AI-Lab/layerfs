# Handoff prompt: AgentFS architecture_overview (v0.1.7 study, issue #159)

> Working artifact: the handoff prompt for the study lead agent. Not a study
> document and not a product contract. The deliverable it commissions is
> `architecture_overview.md` in this folder.

---

You are the **lead agent** for one LayerFS v0.1.7 external study. You own the
research and its single deliverable, and you are expected to run the research
**with subagents** — briefing them, verifying their findings, and synthesizing
the document yourself. Parallel readers give this study full coverage; a
single-pass read is not acceptable.

## Your mission: one file

```
/Users/yifanxu/Ephemeral-AI-Lab/layerfs/docs/roadmap/0.1/0.1.7/study/agentfs/architecture_overview.md
```

Everything else is read-only. Work from the pinned local snapshot only.

## Why this study exists

LayerFS (Rust; repo at `/Users/yifanxu/Ephemeral-AI-Lab/layerfs`) is a workspace
filesystem for AI agents. Its implemented 0.1 model:

- One local Store (`layerfs-layerstack-store`): a single SQLite file plus a
  canonical-object namespace — LayerStacks (named linear sequences of immutable
  Layers), Layers (immutable published snapshots), Branches (named writable
  lines based on a Layer), Commits (immutable Branch snapshots), and globally
  deduplicated content-addressed objects.
- Workspaces (`layerfs-workspace`): ephemeral writable views of one exact
  Branch head, with no database of their own. Lifecycle: Create (pin snapshot,
  prepare projection) → Exec/Shell (run a process in the projection) → Commit
  (capture final state, publish a Commit, compare-and-swap on the Branch head)
  → End (never commits implicitly).
- Supporting crates: `layerfs-content` (object/tree model, CDC chunking,
  extent trees, canonical identity), `layerfs-fuse` (projection into
  sandboxes/containers, live proxy transport, local spool), `layerfs-daemon`
  (container execution + FUSE control), `layerfs-workspace-core` (shared
  Workspace contracts), `layerfs-cli`/`layerfs-sdk` (surfaces),
  `layerfs-monitor` (telemetry), `layerfs-materialization`
  (projection/capture port).

The target 0.2 model (not yet implemented): LayerStack = main, the globally
integrated checkpoint history; Branch = a rapidly iterating node or pod shared
by cooperating agents; Workspace = one isolated tool-call attempt — with
concurrent Workspaces per Branch, automatic reconciliation, and resumable
structured conflict work.

v0.1.7 (issue #155) is the architecture refactor migration: move the internal
architecture onto the shape the 0.2 model assumes while preserving public
SDK/CLI behavior, daemon protocol, canonical bytes and identities, and the
Store format. Your study is one of three external studies (Drive9, Cloudflare
Computer, AgentFS) feeding that refactor.

## Your subject

AgentFS (upstream https://github.com/tursodatabase/agentfs; Turso; Rust core +
multi-language SDKs; MIT; beta) — "the filesystem for agents": the entire agent
runtime — files, state, history — lives in a single SQLite database file. Its
three stated properties map directly onto questions LayerFS cares about:
**auditability** (every file operation, tool call and state change recorded,
queryable with SQL), **reproducibility** (snapshot = copy the database; restore
it to reproduce exact execution states), and **portability** (one file moves
between machines).

Pinned local snapshot (the only source for this study):

```
/Users/yifanxu/Ephemeral-AI-Lab/study/agentfs — not a git checkout; pinned by
release 0.6.4 (CHANGELOG.md head, 2026-03-25; matches sdk/rust and cli crate
versions)
```

Because the snapshot is not a git checkout, cite by file path plus SPEC/MANUAL
section, and record the 0.6.4 pin in the document header.

## How to run the research

- **Phase 0 — ground yourself (you, alone, before spawning anyone).** Read the
  LayerFS grounding docs below, then skim the subject's `README.md` and
  `SPEC.md` and the top-level layout until you can write precise subagent
  briefs. Bad briefs produce useless reports; do not skip this.
- **Phase 1 — decompose into areas.** Use the suggested split below, adjusted
  to what Phase 0 reveals (merge, split, add areas). The split is a
  suggestion; full coverage is the requirement. Every area gets an owner.
- **Phase 2 — brief and spawn one subagent per area, in parallel.** Each brief
  must be fully self-contained — subagents see nothing of your context. Every
  brief carries: the subject repo path and release pin; the area's questions;
  where to look (SPEC sections, SDK modules, CLI sources); the citation format
  (`path:line` for code, `doc#section` for SPEC/MANUAL); the read-only rules
  from the Hard rules section; and the report format — discrete findings, each
  with its citation, plus an explicit unknowns list. Subagents report in their
  reply and write no files.
- **Phase 3 — verify and reconcile (you).** Spot-check several citations from
  every subagent report against the source yourself. When two reports
  disagree, re-read the source and decide; do not average. What you cannot
  verify goes into Unknowns, not into the document.
- **Phase 4 — write (you, the single writer).** Synthesize the verified
  findings into the skeleton below. The "what LayerFS can learn" section is
  your own synthesis from the LayerFS grounding plus the area findings — do
  not delegate it whole.

If your environment provides no way to spawn subagents, run the areas
yourself in the same order and say so in your final report.

### Suggested research areas (subagent briefs)

1. **Store model and SQLite schema.** `SPEC.md` plus
   `sdk/rust/src/schema.rs`: the tables and their roles — how file content,
   metadata, history/audit events, tool-call records and kv state coexist in
   one database file.
2. **Access patterns, durability, concurrency.** `sdk/rust/src/connection_pool.rs`,
   `kvstore.rs`, `sdk/rust/src/filesystem/`: connection handling, transaction
   patterns, the SQLite pragmas in use, what a concurrent reader/writer
   actually sees, crash behavior.
3. **Audit trail and tool-call recording.** `sdk/rust/src/toolcalls.rs` plus
   the SPEC's event/history sections: what is recorded per operation, and how
   history is queried.
4. **Snapshot, restore, portability.** What copying the database file captures
   and misses; the restore flows; the portability claims in the README
   checked against the code.
5. **Mount layer.** `cli/` plus `MANUAL.md`: the CLI verbs; FUSE on Linux and
   NFS on macOS — how each is implemented, why NFS on macOS, the trade-offs
   (latency, caching, semantics).
6. **SDK surfaces.** `sdk/{typescript,python,rust,go}`: the method surface,
   consistency across the four languages, the error model, and what the Rust
   core exposes that the others do not.
7. **The sandbox component and the agent story.** `sandbox/`: what it is and
   how it relates to the SDKs and CLI; how a "filesystem for agents" differs
   from a filesystem for applications in their design (README motivation +
   SPEC).

### LayerFS grounding reads (Phase 0, all read-only, in the LayerFS repo)

1. `docs/general/concepts.md` — the durable + Workspace model, in full
2. `docs/roadmap/0.2/README.md` — the 0.2 problem statement and target topology
3. `docs/roadmap/0.2/agent-branch-reconciliation/README.md` — the 0.1 vs 0.2 gap
4. `docs/roadmap/0.1/0.1.7/README.md` — this release's goal and boundary
5. `docs/roadmap/0.2/cloud-sqlite-vfs.md` — LayerFS's own open question about
   putting the Store in the cloud behind a SQLite VFS, including the Store's
   current SQLite pragmas; directly relevant to comparing durability and
   concurrency choices here

## The document you must produce

Start the file with the repo documentation-policy banner:

```
> **Status:** Research; informative and not a product contract.
```

Then record the pin (upstream URL, snapshot path, release 0.6.4, date you
wrote it) and the issue reference (#159). Required sections:

1. **The system in one paragraph** — what it is, the problem it solves, its own
   framing of that problem.
2. **Component map** — annotated tree of the repository; one line per major
   component on what it owns; the language/runtime stack and deployment shape
   (what runs where when an agent uses it).
3. **Authority and data flow** — where state lives, who may write it, the
   write path and read path end-to-end, the mount lifecycle, and the
   concurrency model (what serializes, what is safe concurrently, how the
   audit trail is produced on each path).
4. **Implementation details** — the verified findings from areas 1-5 and 7,
   organized by mechanism, with code references.
5. **Interfaces** — the verified findings from areas 5-6: CLI, the four SDKs,
   mount UX, and what the SPEC fixes versus what each binding adds.
6. **What LayerFS can learn — the point of the study.** Structure it as:
   (a) a concept-mapping table — their concept ↔ closest LayerFS concept,
   including "no equivalent" rows in both directions; (b) observations with
   citations; (c) for each observation a verdict — adopt / avoid /
   borrow-with-changes / not-applicable — with reasoning grounded in LayerFS's
   constraints (local Store today, 0.2 topology next, patch-release
   stability). Pay particular attention to: auditability of every operation
   versus `layerfs-monitor`/Store telemetry (what LayerFS records today and
   could); snapshot-as-copy reproducibility versus LayerStack/Layer immutable
   checkpoints; single-file portability versus the LayerFS Store layout (one
   SQLite file plus a canonical-object namespace); SDK-across-languages
   versus `layerfs-sdk` (Rust today); FUSE/NFS mount trade-offs versus
   `layerfs-fuse`; and their SQLite durability/concurrency pragmas versus
   LayerFS's (see the `cloud-sqlite-vfs.md` grounding read). These are
   recommendations handed to the v0.1.7 checklist, not decisions.
7. **Unknowns** — what the research could not verify and why.
8. **Source index** — the files and docs actually consulted.

Depth target: typically 250-450 lines. Prefer precise mechanism descriptions
over feature lists.

## Hard rules (they bind you and every subagent you spawn — restate them in
every brief)

- Read-only everywhere. Exactly one file gets written, by you alone: the
  `architecture_overview.md` named above. Subagents write nothing. No other
  file creations or edits, no commits, no git operations in the LayerFS repo.
  Never modify anything under `/Users/yifanxu/Ephemeral-AI-Lab/study/`.
- The pinned local snapshot is the only source for the subject. No fetching
  upstream repositories, no web browsing for the subject (the Turso
  announcement blog is off-limits as a source); the upstream URL is for
  citation only.
- No builds, tests, benchmarks, or execution of studied code. No LayerFS
  builds or benchmarks either. (The repo-root `AGENTS.md` governs agents in
  this repo; these rules restate the parts that bind you.)
- Citations: every load-bearing claim carries `path:line` (code) or
  `doc#section` (SPEC/MANUAL). Unverifiable claims go to Unknowns — never
  guessed.
- Performance numbers may be quoted only from AgentFS/Turso's own material,
  with citations; this study measures nothing and never presents a vendor
  claim as an independently measured fact.
- Tone: describe the design directly and neutrally; no marketing language in
  either direction. English, matching the LayerFS docs style; use typed entity
  names exactly as the source defines them.

## When you finish

Reply with:

- the absolute path of the file you wrote and the pin you recorded;
- a research log: the areas you ran, how many subagents you spawned, what you
  changed in the suggested split, and how many citations you spot-checked;
- a 5-10 bullet summary of the strongest "what LayerFS can learn" findings;
- the list of unknowns.

If you could not complete a required section, say so explicitly instead of
padding it.
