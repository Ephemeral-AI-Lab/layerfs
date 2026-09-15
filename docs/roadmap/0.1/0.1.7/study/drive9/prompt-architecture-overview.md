# Handoff prompt: Drive9 architecture_overview (v0.1.7 study, issue #157)

> Working artifact: the handoff prompt for the study lead agent. Not a study
> document and not a product contract. The deliverable it commissions is
> `architecture_overview.md` in this folder.

---

You are the **lead agent** for one LayerFS v0.1.7 external study. You own the
research and its single deliverable, and you are expected to run the research
**with subagents** — briefing them, verifying their findings, and synthesizing
the document yourself. A repository this size needs parallel readers for full
coverage; a single-pass read is not acceptable for this study.

## Your mission: one file

```
/Users/yifanxu/Ephemeral-AI-Lab/layerfs/docs/roadmap/0.1/0.1.7/study/drive9/architecture_overview.md
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
Computer, AgentFS) feeding that refactor; **Drive9 is the closest analogue to
LayerFS in the set, so this study goes the deepest.**

## Your subject

Drive9 — server-side workspace kernel for AI agents
(upstream https://github.com/mem9-ai/drive9, Go 1.25+, Apache-2.0). Its own
framing: "Sandboxes are disposable. Workspaces should not be." Local disks run
the process, Git stores the final result, Drive9 keeps the working state in
between. It gives agents a durable workspace that can be mounted from any
sandbox (FUSE), forked for parallel attempts, checkpointed, rolled back, and
committed with server-side conflict detection.

Pinned local snapshot (the only source for this study):

```
/Users/yifanxu/Ephemeral-AI-Lab/study/drive9 @ git cff6d29 (2026-09-05,
"feat(fuse): add S3 Express append-log WAL support (#883)")
```

## How to run the research

- **Phase 0 — ground yourself (you, alone, before spawning anyone).** Read the
  LayerFS grounding docs below, then skim Drive9's `README.md`,
  `docs/design-overview.md`, and the top-level layout until you can write
  precise subagent briefs. Bad briefs produce useless reports; do not skip
  this.
- **Phase 1 — decompose into areas.** Use the suggested split below, adjusted
  to what Phase 0 reveals (merge, split, add areas). The split is a
  suggestion; full coverage is the requirement. Every area gets an owner.
- **Phase 2 — brief and spawn one subagent per area, in parallel.** Each brief
  must be fully self-contained — subagents see nothing of your context. Every
  brief carries: the subject repo path and pin; the area's questions; where to
  look (entry files, packages, docs); the citation format (`path:line` for
  code, `doc#section` for docs); the read-only rules from the Hard rules
  section; and the report format — discrete findings, each with its citation,
  plus an explicit unknowns list. Subagents report in their reply and write no
  files.
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

1. **Server architecture and authority.** The server/client split: which
   process runs where (server, sandbox, host), where authority lives, the
   communication channel and protocol; how a workspace, a mount, a fork, a
   checkpoint and a commit are actually represented — data structures,
   tables, messages. Start: `cmd/drive9-server`, `pkg/server`,
   `docs/design-overview.md`, `docs/design/`, `docs/specs/`.
2. **Storage and journaling.** `pkg/journal`; the S3 Express append-log WAL
   (the snapshot's most recent feature — locate it, explain write and recovery
   semantics); `pkg/datastore` + `pkg/backend` — what backends exist, what
   each stores, on-disk layout.
3. **Mount lifecycle and FUSE.** `pkg/fuse`, `pkg/mountcontrol`,
   `pkg/mountpath`, `pkg/mountstate`, `pkg/mountsupervisor`: supervision,
   reconnection, failure handling; FUSE semantics; the findings of
   `docs/posix-compatibility-report.md`.
4. **Commit and conflict detection.** The commit path end-to-end; the
   server-side conflict detection algorithm — the exact rules; what happens
   when two attempts from the same base both want to commit; how
   fork/checkpoint/rollback interact with it.
5. **Content representation and Git integration.** `pkg/objectfs`,
   `pkg/treebuilder`, `pkg/gitcache`, `pkg/gitwsindex`.
6. **Interfaces.** CLI verbs (`cmd/drive9`), the server API surface, and the
   five SDKs (`clients/drive9-{js,kotlin,py,rs,swift}`) — shape and
   consistency across languages.
7. **Tenancy, leadership, secondary surfaces.** `pkg/tenant`, `pkg/tenantctx`,
   `pkg/leader` at mechanism depth; `semantic`, `embedding`, `webdav`, `vault`,
   `encrypt`, `migration` at component-map depth only (what they are for, how
   they attach) — no deep dives.

### LayerFS grounding reads (Phase 0, all read-only, in the LayerFS repo)

1. `docs/general/concepts.md` — the durable + Workspace model, in full
2. `docs/roadmap/0.2/README.md` — the 0.2 problem statement and target topology
3. `docs/roadmap/0.2/agent-branch-reconciliation/README.md` — the 0.1 vs 0.2 gap
4. `docs/roadmap/0.1/0.1.7/README.md` — this release's goal and boundary
5. `docs/roadmap/0.2/cloud-sqlite-vfs.md` — optional but useful: LayerFS's open
   question about a cloud Store; Drive9's server-side authority is a live
   answer to a version of that question

## The document you must produce

Start the file with the repo documentation-policy banner:

```
> **Status:** Research; informative and not a product contract.
```

Then record the pin (upstream URL, snapshot path, commit, date you wrote it)
and the issue reference (#157). Required sections:

1. **The system in one paragraph** — what it is, the problem it solves, its own
   framing of that problem.
2. **Component map** — annotated tree of the repository; one line per major
   component on what it owns; the language/runtime stack and deployment shape
   (what runs on a server, what runs in the sandbox, what runs on the host).
3. **Authority and data flow** — where state lives, who may write it, the
   write path and read path end-to-end, the mount/sync lifecycle, and the
   concurrency model (what is single-threaded, what is parallel, what
   coordinates, how failures and reconnects are handled).
4. **Implementation details** — the verified findings from areas 1-5 and 7,
   organized by mechanism, with code references.
5. **Interfaces** — the verified findings from area 6.
6. **What LayerFS can learn — the point of the study.** Structure it as:
   (a) a concept-mapping table — their concept ↔ closest LayerFS concept,
   including "no equivalent" rows in both directions; (b) observations with
   citations; (c) for each observation a verdict — adopt / avoid /
   borrow-with-changes / not-applicable — with reasoning grounded in LayerFS's
   constraints (local Store today, 0.2 topology next, patch-release
   stability). Pay particular attention to: authority placement (server-side
   kernel vs LayerFS's local Store), fork/attempt representation vs
   Branch/Workspace, commit conflict handling vs compare-and-swap on the
   Branch head, journaling/WAL vs LayerFS's capture path, mount supervision
   vs `layerfs-fuse`'s live proxy lifecycle, and multi-tenancy (LayerFS has
   none today — is that a gap or out of scope?). These are recommendations
   handed to the v0.1.7 checklist, not decisions.
7. **Unknowns** — what the research could not verify and why.
8. **Source index** — the files and docs actually consulted.

Depth target: this is the architecture overview of the deepest of the three
studies — typically 300-600 lines. Prefer precise mechanism descriptions over
feature lists.

## Hard rules (they bind you and every subagent you spawn — restate them in
every brief)

- Read-only everywhere. Exactly one file gets written, by you alone: the
  `architecture_overview.md` named above. Subagents write nothing. No other
  file creations or edits, no commits, no git operations in the LayerFS repo.
  Never modify anything under `/Users/yifanxu/Ephemeral-AI-Lab/study/`.
- The pinned local snapshot is the only source for the subject. No fetching
  upstream repositories, no web browsing for the subject; the upstream URL is
  for citation only.
- No builds, tests, benchmarks, or execution of studied code. No LayerFS
  builds or benchmarks either. (The repo-root `AGENTS.md` governs agents in
  this repo; these rules restate the parts that bind you.)
- Citations: every load-bearing claim carries `path:line` (code) or
  `doc#section` (docs). Unverifiable claims go to Unknowns — never guessed.
- Performance numbers may be quoted only from Drive9's own material, with
  citations; this study measures nothing and never presents a vendor claim as
  an independently measured fact.
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
