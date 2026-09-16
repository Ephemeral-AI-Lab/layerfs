# Handoff prompt: Radish architecture_overview (v0.1.7 study, issue #173)

> Working artifact: the handoff prompt for the study lead agent. Not a study
> document and not a product contract. The deliverable it commissions is
> `architecture_overview.md` in this folder.

---

You are the **lead agent** for one LayerFS v0.1.7 external study. You own the
research and its single deliverable, and you are expected to run the research
**with subagents** — briefing them, verifying their findings, and synthesizing
the document yourself. A repository this dense needs parallel readers for full
coverage; a single-pass read is not acceptable for this study.

## Your mission: one file

```
/Users/yifanxu/Ephemeral-AI-Lab/layerfs/docs/roadmap/0.1/0.1.7/study/radish/architecture_overview.md
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
Store format. Your study is the fourth external study feeding that refactor,
after Drive9 (#157), Cloudflare Computer (#158) and AgentFS (#159).

It is also, specifically, the **Durable Objects placement study** the v0.1.7
persistence-scope direction deferred ("Cloudflare Durable Objects is a future
placement study because its documented SQLite storage uses WAL" —
[v0.1.7 checklist](../../README.md)). LayerFS's Store runs SQLite at
`journal_mode=MEMORY`, `synchronous=OFF`, `locking_mode=EXCLUSIVE`, with WAL
rejected at preflight and the one-writer invariant owned by a POSIX file lock;
[`docs/roadmap/0.2/cloud-sqlite-vfs.md`](../../../../0.2/cloud-sqlite-vfs.md)
asks what has to change before that Store can leave the local disk, and a
Durable Object is one candidate answer. Radish is the fullest worked example
available of a SQLite-authoritative service on that substrate. Your "what
LayerFS can learn" section is where this placement question gets its
source-backed material — as recommendations to the v0.1.7 checklist, not
decisions.

The Cloudflare Computer study
([`../cloudflare-computer/architecture_overview.md`](../cloudflare-computer/architecture_overview.md))
already covers Durable Objects from the authority/projection angle (one
authoritative store, execution surfaces as projections of it, a sync protocol
to a sandbox). Radish is the complementary angle: a stateless wire protocol
(Redis) served directly from platform-managed storage, with the interesting
parts being what the platform owns (single-writer execution, durability,
failover, journal policy) and what the application still owns (authorization,
budgets, cross-store tiering consistency). Cross-reference the Computer study
where the two overlap; do not re-derive it.

## Your subject

Radish (upstream https://github.com/Dhravya/radish) — a Redis-compatible server
that runs inside a Cloudflare Durable Object. SQLite is the store, the Durable
Object is the process, and R2 is the cold tier. A TCP-to-WebSocket shim
(explicitly temporary) forwards raw RESP bytes; a Worker router authenticates
the upgrade, routes `?db=<name>` to `redis.getByName(name).fetch(request)`, and
exits the data path; every RESP frame is then handled by the DO's hibernation
`webSocketMessage` handler. One name is one instance — its own SQLite (10 GB
platform cap), lease and failover. Values beyond the SQLite budget tier to R2
through a hot/warm/cold state machine whose `meta` directory never leaves
SQLite. Correctness is claimed through differential testing against Redis
7.4.11, and the repo carries its own pre-repair self-review.

Status caveats that bind you:

- **Not production ready** (their own README). It runs, but "it has no
  production miles."
- **`REVIEW.md` (2026-09-14) reviews the tree *before* repairs**; the rewrite
  landed 2026-09-15, one commit after the review. Most review findings already
  have visible repairs in the current tree. Every finding you cite must be
  labelled with its repaired/unrepaired status at the snapshot — the review is
  evidence of the *process*, the code is evidence of the *state*.
- **The test suite never executes workerd.** Everything runs on a local
  bun:sqlite adapter (`test/sqlite-adapter.ts`). Real `transactionSync`, real
  attachments, real alarms, real R2 and the input/output gates are
  untested in-repo. Platform behavior is platform-claimed, not repo-proven;
  quote it as such.
- Cloudflare platform facts (WAL in DO SQLite, gate semantics, 16,384-byte
  attachments, 10 GB per DO, 2 MB row cap, 100 bound parameters, 100 KB
  statements) are reproduced in the repo from Cloudflare docs. You have no web
  access to verify them; cite them as repo-quoted claims.

Pinned local snapshot (the only source for this study):

```
/Users/yifanxu/Ephemeral-AI-Lab/study/radish @ git 98768e8
(2026-09-15, "Add a full-width source link mid-page")
```

## How to run the research

- **Phase 0 — ground yourself (you, alone, before spawning anyone).** Read the
  LayerFS grounding docs below, then skim the subject's `README.md`,
  `REVIEW.md`, and the top-level layout until you can write precise subagent
  briefs. Bad briefs produce useless reports; do not skip this.
- **Phase 1 — decompose into areas.** Use the suggested split below, adjusted
  to what Phase 0 reveals (merge, split, add areas). The split is a
  suggestion; full coverage is the requirement. Every area gets an owner.
- **Phase 2 — brief and spawn one subagent per area, in parallel.** Each brief
  must be fully self-contained — subagents see nothing of your context. Every
  brief carries: the subject repo path and pin; the area's questions; where to
  look (entry files, modules, tests); the citation format (`path:line` for
  code, `README#section` / `REVIEW.md#section` for docs); the read-only rules
  from the Hard rules section; the status caveats above; and the report format
  — discrete findings, each with its citation and a
  code-verified / doc-claimed / platform-claimed label, plus an explicit
  unknowns list. Subagents report in their reply and write no files.
- **Phase 3 — verify and reconcile (you).** Spot-check several citations from
  every subagent report against the source yourself, and personally settle
  the code-verified vs doc-claimed question for every mechanism that reaches
  the document. When two reports disagree, re-read the source and decide; do
  not average. What you cannot verify goes into Unknowns, not into the
  document.
- **Phase 4 — write (you, the single writer).** Synthesize the verified
  findings into the skeleton below. The "what LayerFS can learn" section is
  your own synthesis from the LayerFS grounding plus the area findings — do
  not delegate it whole.

If your environment provides no way to spawn subagents, run the areas
yourself in the same order and say so in your final report.

### Suggested research areas (subagent briefs)

1. **Request path and deployment.** `src/worker.ts` (auth, name validation,
   the one-line data plane), `shim/tcp-shim.ts` + `shim/README.md` (byte
   shovelling, backpressure, why it exists and when it dies), `alchemy.run.ts`
   and the alchemy wiring (Worker + DO namespace + `ColdTier` R2 bucket, no
   wrangler). What each hop does and does not do; where auth lives; what is
   temporary.
2. **Durable Object lifecycle and concurrency.** `src/do.ts`, `src/session.ts`:
   the upgrade path, hibernation (`webSocketMessage`/`webSocketClose`),
   attachments and the `conn_spill` table, alarms (`sweep → reap → relieve →
   rearm`), per-connection ordering, what the DO runtime serializes, what
   "input gate" and "output gate" mean for this design (as platform-claimed
   semantics).
3. **SQLite schema and command-layer storage.** `src/schema.ts`, `src/store.ts`,
   `src/commands/`: the `meta` directory and per-type value tables; raw-BLOB
   keys; fractional list indices; nullable cardinality; server-side SCAN
   cursors; lazy expiry; the fifteen mutation-invalidation triggers;
   `transactionSync` and its three call sites; failure-ordered
   non-transactional writes; how per-type handlers stay synchronous.
4. **Tier engine and R2 cold tier.** `src/tier/` (engine, planner, codec,
   bucket): hot/warm/cold semantics; 8/6 GiB watermarks; the pure-function
   planner (free-before-paid, scoring, hard exclusions); promote with
   re-read-and-byte-compare; transactional demote; fault-in before dispatch
   and the EXEC union rule; the R2 bucket port and `k/<hex>` naming; orphan
   collection and its call sites. Also: when eviction is *triggered* (alarm
   arming), and what the trigger dependency implies.
5. **RESP protocol and the compatibility contract.** `src/resp.ts`,
   `src/dtoa.ts`, `src/commands/spec.ts`, `src/commands/index.ts`,
   `test/differential.ts`: incremental decoding, inline commands, RESP2/RESP3
   encoding differences, Grisu2, software binary128; the 124-spec registry plus
   12 connection-scoped commands; key extraction; deliberate deviations and
   their pinning tests; the differential harness (seeds, relaxations, the
   final full-key sweep).
6. **Review and limits audit.** `REVIEW.md` end-to-end, cross-checked against
   the current tree: each of the 27 numbered findings labelled
   repaired/unrepaired, and classified as Redis-semantics vs
   Durable-Object-platform vs implementation defect. Also the README's Limits,
   Not-implemented, Tiering and Durability sections against the code, and the
   platform-limit enforcement sites (1 MiB values, 100 parameters, 100 KB
   statements, attachment budgets).
7. **Claims and numbers audit.** The README's quantitative claims vs the tree:
   command-count arithmetic (124 + 12 = 136, but distinct names = 134 because
   QUIT/RESET sit in both tables), the 624-test count, the 7,980-line claim,
   the differential-campaign and measurement claims that have no in-tree
   receipts, and any doc statement the code contradicts or only approximates
   (for a start: the HSCAN/SSCAN/ZSCAN expired-cursor asymmetry, the "one
   enforcement point" expiry claim, the eviction trigger's TTL dependency,
   unwired orphan collection, write-recency rather than read-recency in the
   planner). This area also cross-checks the others.

### What an initial pass already established (orientation only — re-verify
before use; treat as briefs for Phase 3 spot-checks, not as settled fact)

An initial four-agent source-reading pass (2026-09-17) recorded, among other
things: the shim/worker/DO split and the worker's one-line data plane; the 12
KiB attachment threshold with `conn_spill`; `transactionSync` used exactly
three times; the tier engine's watermarks, exclusions and byte-compare
promotion; eviction running only from the alarm handler, which is armed only
while some key has a TTL — so a keyspace with no expiring keys never evicts;
`collectOrphans` having no production call site; `touched_at` recording
writes and fault-ins but not reads; all tests running on the local
`FakeSqlStorage` adapter with no workerd execution; and the review-repair
temporal frame above. Verify whatever you use.

### LayerFS grounding reads (Phase 0, all read-only, in the LayerFS repo)

1. `docs/general/concepts.md` — the durable + Workspace model, in full
2. `docs/roadmap/0.2/README.md` — the 0.2 problem statement and target topology
3. `docs/roadmap/0.2/agent-branch-reconciliation/README.md` — the 0.1 vs 0.2 gap
4. `docs/roadmap/0.1/0.1.7/README.md` — this release's goal, boundary and the
   persistence-scope direction that defers this study
5. `docs/roadmap/0.2/cloud-sqlite-vfs.md` — LayerFS's own cloud question: the
   `MEMORY`/`OFF`/`EXCLUSIVE` constraint, the D1 placement options, decision
   D2 on WAL, the append-only `object_packs` region, the staged roadmap, the
   invariants and the open questions. **The central grounding document for
   this study.**
6. `docs/roadmap/0.1/0.1.7/study/cloudflare-computer/architecture_overview.md` —
   the complementary Durable Object study; cross-reference, do not re-derive
7. `docs/roadmap/0.1/0.1.7/component-decoupling/admission-and-persistence.md` —
   the C2 save/persistence design whose seams a placement would target

## The document you must produce

Start the file with the repo documentation-policy banner:

```
> **Status:** Research; informative and not a product contract.
```

Then record the pin (upstream URL, snapshot path, commit, date you wrote it)
and the issue reference (#173). Required sections:

1. **The system in one paragraph** — what it is, the problem it solves, its own
   framing of that problem (escaping *operations*, not licenses).
2. **Component map** — annotated tree of the repository; one line per module on
   what it owns; the language/runtime stack (TypeScript, Bun, Cloudflare
   Workers, Durable Objects, R2, alchemy) and deployment shape.
3. **Authority and data flow** — where state lives (the per-DO SQLite and the
   R2 cold tier), who may write it, the write and read paths end-to-end, the
   mount/ingress lifecycle (shim → router → DO), and the concurrency model
   (what the DO runtime serializes, hibernation, alarms, what the application
   chains itself). **Must include a `text`-fenced request-path diagram** in the
   style of the subject's own README diagram, and a **hibernation/attachment
   lifecycle diagram**.
4. **Implementation details** — the verified findings from areas 2-4 and 6,
   organized by mechanism: schema and triggers; transactions and
   failure-ordering; the tier engine (with a **hot/warm/cold state-machine
   diagram**, including promote's byte-compare and the fault-in path); session
   state and budgets; enforced platform limits. Each mechanism carries its
   code-verified / doc-claimed / platform-claimed label.
5. **Interfaces** — the RESP2/RESP3 surface; the dispatch registry and
   connection-scoped commands; the compatibility contract with its deliberate
   deviations; the differential-testing methodology.
6. **What LayerFS can learn — the point of the study.** Structure it as:
   (a) a concept-mapping table — their concept ↔ closest LayerFS concept,
   including "no equivalent" rows in both directions; (b) observations with
   citations; (c) for each observation a verdict — adopt / avoid /
   borrow-with-changes / not-applicable — with reasoning grounded in LayerFS's
   constraints (local Store today, the 0.2 topology next, patch-release
   stability, the no-retry owner direction); (d) a **LayerFS
   Durable-Object-placement concept-map diagram** (`text`-fenced) showing how
   the Store / object packs / Workspaces / daemon protocol would — or would
   not — map onto DOs, R2, gates and the ingress shape. Pay particular
   attention to: one name = one instance vs one LayerStack per named DO as the
   server-enforced single-writer (cloud-sqlite-vfs.md invariant 2 and Stage 3's
   "branch leases become server-enforced"); DO SQLite's WAL and untouchable
   journal policy vs LayerFS's `MEMORY`/`OFF`/`EXCLUSIVE` preflight rejection
   (decision D2 — can the Store run as-is, or does the DO hold a different
   representation?); the tier engine vs the append-only `object_packs` region
   and D1-C replication, including the retraction contract (open questions 1,
   6, 7) and promote's byte-compare vs LayerFS's authenticated exact reads;
   the output gate vs "required encoding finishes before public success" and
   Radish's explicit "EXEC rolls back on infrastructure failure" deviation vs
   LayerFS's one-attempt rule; DO SQL limits (2 MB rows, 100 parameters,
   100 KB statements, 10 GB) vs LayerFS pack sizes and paged `IN (...)` batch
   reads; fetch/WebSocket-only ingress vs the daemon protocol and the shim
   pattern; per-name authorization gaps vs multi-tenant LayerFS; and what
   Radish's honesty artifacts (enforced-limits table, deviation-pinning tests,
   pre-repair REVIEW.md) suggest for LayerFS documentation. These are
   recommendations handed to the v0.1.7 checklist, not decisions.
7. **Unknowns** — what the research could not verify and why, including
   platform-claimed behavior with no in-repo proof, and README claims without
   in-tree receipts.
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
- The status caveats apply to everyone: verify against code; REVIEW.md
  findings carry repaired/unrepaired labels; platform facts are quoted as
  repo-claimed; nothing executes workerd.
- No builds, tests, benchmarks, or execution of studied code. No LayerFS
  builds or benchmarks either. (The repo-root `AGENTS.md` governs agents in
  this repo; these rules restate the parts that bind you.)
- Citations: every load-bearing claim carries `path:line` (code) or
  `README#section` / `REVIEW.md#section` (docs). Unverifiable claims go to
  Unknowns — never guessed.
- Performance numbers may be quoted only from the project's own material, with
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
