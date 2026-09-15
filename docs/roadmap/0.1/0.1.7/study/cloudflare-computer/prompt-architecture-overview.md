# Handoff prompt: Cloudflare Computer architecture_overview (v0.1.7 study, issue #158)

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
/Users/yifanxu/Ephemeral-AI-Lab/layerfs/docs/roadmap/0.1/0.1.7/study/cloudflare-computer/architecture_overview.md
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

Cloudflare Computer (upstream https://github.com/cloudflare/computer) — a
virtual filesystem that lives inside a Durable Object. The Durable Object holds
the authoritative state in SQLite and exposes one pluggable execution surface
through `workspace.runtime`. Three backends ship today:

- **Container** — projects the SQLite state into a sandbox container as a real
  FUSE mount; a sandbox-side daemon (`computerd`) mounts the state as a
  filesystem and syncs changes back over a capnweb RPC channel. Full Linux
  userland, real binaries, real network.
- **Isolate shell** — runs just-bash in a Dynamic Worker, reaching the
  authoritative Workspace over Workers RPC (no second store, no sync round
  trip).
- **Isolate JavaScript** — runs an ECMAScript module in a fresh Dynamic Worker
  with structured input/results, durable relative imports, Workspace-backed
  `node:fs/promises`, and trusted `ws:git` and `ws:artifacts` modules.

Backends register under stable IDs behind a single
`workspace.runtime.exec(source, { backend })` entry point and connect lazily; a
Workspace can also be constructed with no backend at all — the filesystem on
its own.

Status caveat (their own README): **preview only**, APIs unstable, and the
spec under `docs/` is forward-looking — "read it for intent, not as description
of the code today." For every mechanism described, the code at this snapshot
must be checked before the document claims it exists; spec-only statements are
labelled as intent.

Pinned local snapshot (the only source for this study):

```
/Users/yifanxu/Ephemeral-AI-Lab/study/cloudflare-computer @ git 64c462b
(2026-08-26, "bench: account for spawned computerd tasks")
```

## How to run the research

- **Phase 0 — ground yourself (you, alone, before spawning anyone).** Read the
  LayerFS grounding docs below, then skim the subject's `README.md`,
  `docs/README.md`, and the top-level layout until you can write precise
  subagent briefs. Bad briefs produce useless reports; do not skip this.
- **Phase 1 — decompose into areas.** Use the suggested split below, adjusted
  to what Phase 0 reveals (merge, split, add areas). The split is a
  suggestion; full coverage is the requirement. Every area gets an owner.
- **Phase 2 — brief and spawn one subagent per area, in parallel.** Each brief
  must be fully self-contained — subagents see nothing of your context. Every
  brief carries: the subject repo path and pin; the area's questions; where to
  look (entry files, packages, spec docs); the citation format (`path:line`
  for code, `doc#section` for docs); the read-only rules from the Hard rules
  section; the verify-against-code caveat; and the report format — discrete
  findings, each with its citation and an implemented-vs-spec-only flag, plus
  an explicit unknowns list. Subagents report in their reply and write no
  files.
- **Phase 3 — verify and reconcile (you).** Spot-check several citations from
  every subagent report against the source yourself, and personally settle the
  implemented-vs-spec question for every mechanism that reaches the document.
  When two reports disagree, re-read the source and decide; do not average.
  What you cannot verify goes into Unknowns, not into the document.
- **Phase 4 — write (you, the single writer).** Synthesize the verified
  findings into the skeleton below. The "what LayerFS can learn" section is
  your own synthesis from the LayerFS grounding plus the area findings — do
  not delegate it whole.

If your environment provides no way to spawn subagents, run the areas
yourself in the same order and say so in your final report.

### Suggested research areas (subagent briefs)

1. **Durable Object authority and schema.** What state the DO holds
   (`docs/01_vfs.md`, `docs/03_filesystem_schema.md`,
   `docs/10_project_layout.md`, `packages/dofs`), how requests reach it, and
   its single-DO concurrency constraints — what serializes, hibernation
   behavior, input gates if any.
2. **Sync protocol and mount.** `docs/02_sync_protocol.md`,
   `docs/06_mount_interface.md`, `docs/08_capnweb_interface.md`,
   `packages/computerd`, `packages/rpc`: the message flow between sandbox and
   DO, consistency model, failure and reconnect handling, how FUSE state in
   the container maps to DO state.
3. **Runtime and backend registry.** `docs/05_runtime_interface.md`,
   `packages/computer/src/backend.ts`, `src/backends`, `src/client.ts`: how
   backends register under stable IDs, lazy connection, the single
   `workspace.runtime.exec(source, { backend })` entry point and its wire
   format.
4. **Container backend.** `packages/computerd` + `examples/container`: what
   computerd does inside the sandbox, the FUSE mount mechanics, what "full
   Linux userland" actually relies on.
5. **Isolate backends.** `docs/12_worker_backend.md`,
   `docs/16_code_execution.md`, `docs/17_isolate_javascript.md`,
   `examples/worker-shell`, `examples/worker-javascript`: the just-bash shell
   path; the JavaScript isolate path; Workspace-backed `node:fs/promises`;
   `ws:git` and `ws:artifacts` as seen from isolate code.
6. **Auxiliary interfaces.** `docs/09_tool_interface.md`,
   `docs/13_git_interface.md`, `docs/14_assets_interface.md`,
   `docs/15_artifacts_interface.md`, and the observe surface in
   `packages/computer/src/observe*`.
7. **Lifecycle and spec-vs-code audit.** `docs/11_lifecycle.md`,
   `docs/18_runtime_migration.md`, `docs/19_performance.md`: creation,
   hibernation/reconnect, teardown; performance claims quoted only with
   citations. This area also cross-checks the others: which mechanisms are
   implemented in the code at this snapshot, and which are spec-ahead-of-code.

### LayerFS grounding reads (Phase 0, all read-only, in the LayerFS repo)

1. `docs/general/concepts.md` — the durable + Workspace model, in full
2. `docs/roadmap/0.2/README.md` — the 0.2 problem statement and target topology
3. `docs/roadmap/0.2/agent-branch-reconciliation/README.md` — the 0.1 vs 0.2 gap
4. `docs/roadmap/0.1/0.1.7/README.md` — this release's goal and boundary
5. `docs/roadmap/0.2/cloud-sqlite-vfs.md` — LayerFS's own open question about
   putting the Store in the cloud behind a SQLite VFS; directly relevant to
   this study's authority-placement lessons

## The document you must produce

Start the file with the repo documentation-policy banner:

```
> **Status:** Research; informative and not a product contract.
```

Then record the pin (upstream URL, snapshot path, commit, date you wrote it)
and the issue reference (#158). Required sections:

1. **The system in one paragraph** — what it is, the problem it solves, its own
   framing of that problem.
2. **Component map** — annotated tree of the repository; one line per package
   and per major module on what it owns; the language/runtime stack (Workers,
   Durable Objects, container, Dynamic Worker) and deployment shape.
3. **Authority and data flow** — where state lives (the DO and its SQLite),
   who may write it, the write path and read path end-to-end for each backend
   (container with sync, isolates over RPC), the mount lifecycle, and the
   concurrency model (single-DO constraints, what serializes, what is
   lazy/deferred).
4. **Implementation details** — the verified findings from areas 1-5 and 7,
   organized by mechanism, with code references, each carrying its
   implemented-vs-spec-only status.
5. **Interfaces** — the verified findings from areas 3-6: the exec entry
   point; the `fs` and `runtime` surfaces of `@cloudflare/computer`; `ws:git`
   and `ws:artifacts`; the tool interface.
6. **What LayerFS can learn — the point of the study.** Structure it as:
   (a) a concept-mapping table — their concept ↔ closest LayerFS concept,
   including "no equivalent" rows in both directions; (b) observations with
   citations; (c) for each observation a verdict — adopt / avoid /
   borrow-with-changes / not-applicable — with reasoning grounded in LayerFS's
   constraints (local Store today, 0.2 topology next, patch-release
   stability). Pay particular attention to: their strict authority/projection
   split (one authoritative store, execution surfaces as projections of it)
   versus LayerFS's Store/Workspace boundary — exactly the boundary v0.1.7
   reworks; pluggable execution backends behind one entry point with stable
   IDs versus `layerfs-daemon`'s container execution; the sync-protocol design
   versus `layerfs-fuse`'s live proxy transport and local spool; single-DO
   concurrency versus LayerFS's SQLite Store (EXCLUSIVE locking — see the
   `cloud-sqlite-vfs.md` grounding read); and whether their numbered-spec
   documentation style is worth borrowing for LayerFS. These are
   recommendations handed to the v0.1.7 checklist, not decisions.
7. **Unknowns** — what the research could not verify and why, including
   spec-ahead-of-code items that could not be settled.
8. **Source index** — the files and docs actually consulted.

Depth target: typically 300-500 lines. Prefer precise mechanism descriptions
over feature lists.

## Hard rules (they bind you and every subagent you spawn — restate them in
every brief)

- Read-only everywhere. Exactly one file gets written, by you alone: the
  `architecture_overview.md` named above. Subagents write nothing. No other
  file creations or edits, no commits, no git operations in the LayerFS repo.
  Never modify anything under `/Users/yifanxu/Ephemeral-AI-Lab/study/`.
- The pinned local snapshot is the only source for the subject. No fetching
  upstream repositories, no web browsing for the subject; the upstream URL is
  for citation only.
- The preview caveat applies to everyone: verify against code; spec statements
  are labelled as intent.
- No builds, tests, benchmarks, or execution of studied code. No LayerFS
  builds or benchmarks either. (The repo-root `AGENTS.md` governs agents in
  this repo; these rules restate the parts that bind you.)
- Citations: every load-bearing claim carries `path:line` (code) or
  `doc#section` (docs). Unverifiable claims go to Unknowns — never guessed.
- Performance numbers may be quoted only from Cloudflare's own material, with
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
