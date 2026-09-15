# Handoff prompt — v0.1.7 architecture exploration study (#156)

Copy the prompt below into the coordinating agent's task. Creating this document
has not started the study.

---

You are the coordinating agent for **[#156](https://github.com/Ephemeral-AI-Lab/layerfs/issues/156)**,
sub-issue of **[#155](https://github.com/Ephemeral-AI-Lab/layerfs/issues/155)**.

Your single deliverable is one document:
**`docs/roadmap/0.1/0.1.7/architecture-overview.md`**, written to the
owner-approved content spec in
[#156](https://github.com/Ephemeral-AI-Lab/layerfs/issues/156#issuecomment-5686317948),
in **ASCII-diagram-first** house style, explored by **read-only subagents**.

They gather evidence; you compose the diagrams, write the document, and own every
claim in it.

## Read before you touch anything

| file | what you need from it |
|---|---|
| [#156](https://github.com/Ephemeral-AI-Lab/layerfs/issues/156) and its content-spec comment | the contract: the 9 sections, the acceptance list, the LFCNT1 question |
| [`AGENTS.md`](../../../../AGENTS.md) | repo-wide rules; note which bind a source-reads-only study (tree cleanliness, no claims, no third-party edits) and which do not apply (measurement, budgets) |
| [`docs/general/documentation-policy.md`](../../../general/documentation-policy.md) | status banner, link rules, typed entity names, "one clear status" |
| [`docs/general/concepts.md`](../../../general/concepts.md) | the shipped vocabulary you must use exactly |
| [`README.md`](../../../../README.md#L28) | the product pitch — verbatim source of §1's north star |
| [`docs/roadmap/architecture.md`](../../architecture.md#L41) | the north star and its ownership boundaries |
| [`docs/research/vision/README.md`](../../../research/vision/README.md) | long-range vision, explicitly **non-binding** — cite it as intent, never as behavior |
| [`docs/roadmap/0.2/agent-branch-reconciliation/current-model.md`](../../0.2/agent-branch-reconciliation/current-model.md) | **diagram house style**: the ASCII entity graph, ownership boxes and flow charts to imitate |
| [`docs/roadmap/0.1/0.1.6/sandbox-host-connection-architecture.md`](<../0.1.6/sandbox-host-connection-architecture.md>) | **before/after ASCII diagrams** and an ownership-flow style to imitate |
| [`docs/roadmap/0.1/development.md`](<../development.md#L42>) | the crate map that is already stale (9 crates, no `layerfs-workspace-core`): correct it in prose, do not edit it |
| [`docs/roadmap/0.1/0.1.5/existing_architecture.md`](<../0.1.5/existing_architecture.md>) | the previous source-bound inventory — useful shape, real content, pinned to an older source; say what still holds and what does not |

Read applicable ancestor instructions before writing. Read the crate sources you
will describe; do not write about modules you have not opened.

## Hard rules

1. **Source reads only.** No builds, tests, benchmarks, `cargo build/test/run`,
   Docker, or code changes. Use scoped `rg`/`grep`/`read`; never scan `target/`,
   `benchmark-results/`, or the whole of `docs/roadmap/` (thousands of files).
2. **You create exactly one file:** `docs/roadmap/0.1/0.1.7/architecture-overview.md`.
   Do not edit any other document; record needed corrections as observations in §9.
3. **ASCII diagrams, not prose walls.** Every structural section carries at least
   one diagram — the crate dependency map (§4), the small-file vs large-file
   storage paths (§3), the core algorithm flows (§6), the runtime topology and
   what crosses each boundary (§7). Plain ASCII only: `+ - | > < v ^ \ /` and
   column-aligned labels; no Mermaid, no images, no Unicode box-drawing
   characters. A diagram is a claim: every arrow must be traceable to source.
4. **Every load-bearing claim carries a repository-relative source reference**
   (path, and line where useful) — or is explicitly marked `not-found` /
   `unknown`. An unwitnessed claim is worse than a gap.
5. **Subagent output is untrusted observation.** Verify each cited claim yourself
   before it enters the document. A citation you have not opened is not evidence.
6. **No design decisions, no scope, no estimates, no performance claims.** The
   document describes what exists. Observations that imply work go into a short
   "hand-off to the v0.1.7 checklist" list at the end of the study — they are
   recorded, not decided.
7. **Use typed entity names exactly as the source defines them** (`LayerStack`,
   `Layer`, `Branch`, `Commit`, `Workspace`, `ObjectId`, …). Do not invent names
   or import Git vocabulary.
8. **Pin the source.** Record the exact commit the study was written against
   (`git rev-parse HEAD`) and whether the tree was dirty. Every citation is read
   at that commit.
9. **Do not commit unless the owner asks.** Leave the new document in the working
   tree, untouched alongside an otherwise unchanged repository, and report its
   path and content hash.
10. **Report on #156 when the document is drafted**: path, section coverage, the
    unknowns, and every claim you could not verify. Do not close #156.

## Work plan

### Step 0 — orientation (you, no subagents)

Read the table above. Pin `HEAD`. Build your own one-page index of the crates and
their module trees (`crates/*/src`, one level deep) so you can judge subagent
reports against the tree rather than against prose. Note what a *good* ASCII
diagram looks like in the two style references; you will be composing these, so
decide the conventions now (owner box, arrow labels, one concept per diagram).

### Step 1 — breadth round: read-only exploration subagents, one slice each

Launch the slices below **in parallel**. Every subagent task must be self-contained
(subagents do not see this conversation) and must include: the repository path, the
slice scope, the questions, the return format, and the rule that it is read-only.
One subagent covers exactly one slice; never merge slices to save a launch.

| # | Slice | Scope to read | Questions it must answer |
|---|---|---|---|
| A | Canonical identity and authentication | `crates/layerfs-content/src/object/*` | How `ObjectId` and friends are computed; what is authenticated on every durable read; where a hash is *not* the identity |
| B | Large-file representation | `crates/layerfs-content/src/file/{cdc,extent,extent_codec,rope,content}.rs` | The CDC profile in force, boundary stability under an edit, how extents/ropes reference chunks, what bounds apply |
| C | Filesystem trees and reconcile | `crates/layerfs-content/src/tree/*`, `crates/layerfs-content/src/filesystem/*` | inode/directory/metadata roots, canonical root computation, compaction, three-root reconcile and its conflict classes |
| D | Store schema and transactions | `crates/layerfs-layerstack-store/src/{store,schema*,statements,staging}.rs`, `sql/` | Tables and their invariants; which SQL is static vs generated; transaction boundaries and what must stay outside them |
| E | Store physical representation and admission | `crates/layerfs-layerstack-store/src/objects/*` | Small-content vs large-content admission, delta selection and chains, pack/read paths, and **which small-file mechanism is live today** (the LFCNT1 read-only question) |
| F | Publication and history operations | `crates/layerfs-layerstack-store/src/{workspace,layerstack,branch,query}.rs` | Commit publication compare-and-swap, `Add` into the LayerStack, fork/query operations, what is visibility-last and why |
| G | Workspace lifecycle and candidate construction | `crates/layerfs-workspace/src/{lifecycle,session,changes,capture,registry}.rs` | Workspace states and lease, candidate construction and worker limits, capture/dirty frontier, what makes a Workspace disposable |
| H | Portable core and capture inputs | `crates/layerfs-workspace-core/src/*`, `crates/layerfs-workspace/src/{cow_tree,reconcile,projection,snapshot_input,live_backing,remote_commit}.rs` | What the portable core owns vs the host, how snapshot input and live backing feed capture, reconcile state lifetime |
| I | FUSE projection internals | `crates/layerfs-fuse/src/{adapter,filesystem,inode_table,handles,immutable_read_cache,local_spool,port,host_mount}.rs` | Inode/handle model, read cache and spool, what the mount guarantees, where FUSE behavior diverges from materialization |
| J | Sandbox-local ownership and wire path | `crates/layerfs-fuse/src/{live_owner,live_runtime,live_transport,live_wire,proxy_client,proxy_host,write_metrics}.rs`, `crates/layerfs-workspace/src/{daemon,docker,docker_engine,execution,output}.rs` | Which process owns mutable state, what is transferred and when, message shapes, cleanup on failure |
| K | Control plane and public contracts | `crates/layerfs-daemon/src/*`, `crates/layerfs-sdk/src/*`, `crates/layerfs-cli/src/*` | The public contract surface (SDK operations, CLI grammar, daemon protocol, capability authentication); what a caller can and cannot reach |
| L | Observation, telemetry and bounds | `crates/layerfs-monitor/src/*`, `*limits*.rs`, `objects/telemetry.rs`, `write_metrics.rs` | What is measured and recorded, where limits are declared and enforced, which numbers are product signals vs diagnostics |
| M | Documentation staleness audit | `docs/roadmap/0.1/development.md`, `docs/roadmap/0.1/0.1.5/existing_architecture.md`, `docs/general/concepts.md`, `docs/versioned/0.1.5/*` | Which document describes which source; what is stale, what is still true, what contradicts the tree |

Subagent task template (adapt per slice; keep it explicit):

```text
Read-only exploration. Repository: /Users/yifanxu/Ephemeral-AI-Lab/layerfs
Slice: <name>. Read only: <paths>. Do not modify anything; do not build or run
anything; do not scan target/, benchmark-results/, or all of docs/roadmap/.
Follow ownership flow, not single functions: read the callers and the module
that owns the state before answering.

Answer, with a source reference for each answer:
<the slice's questions>

Return exactly three blocks:

1. Table: claim | source path:line | status (verified|partial|not-found) |
   invariant it enforces | notes
2. One ASCII diagram (<= 30 lines) of this slice's core mechanism or ownership
   flow, in plain ASCII: boxes, arrows, aligned labels. Keep it small enough to
   be wrong in a visible way; label every box with its source path.
3. "Not covered / uncertain" — what you could not confirm, what you did not read,
   and the files you actually opened.

Prose beyond these three blocks is not wanted.
```

Ignore anything that comes back without citations or the opened-files list.

### Step 2 — merge, verify, compose

- Build §2's claim→mechanism table from the subagent rows; keep each row's status.
- Where two slices disagree (the live vs retained small-file path is the expected
  one), open the source yourself, decide from the call sites, and record the loser
  as retained/legacy with its path.
- Verify every row you intend to call load-bearing, plus every row in §3, by
  reading the cited code yourself. Downgrade anything you cannot reproduce.
- Compose the diagrams from the subagents' sketches into one consistent style
  (same box/arrow conventions throughout the document), then walk each arrow back
  to source. If an arrow cannot be traced, cut it or mark it uncertain.
- Keep a private list of dropped or downgraded rows; they become §9 material if
  they reveal a documentation gap.

### Step 3 — write the judgment sections yourself (never delegate)

- **§1 Product identity** and **§2's obligation list**: yours, from the pitch and
  the authority levels.
- **§8 Load-bearing read**: yours, against the stated criteria
  (invariant-carrying / contract surface / internal seam / movable plumbing /
  harness-only), one "what breaks if you touch it" line per item. This section is
  the point of the study; a subagent may supply evidence, never the ranking. A
  ranked table plus one summary diagram is the expected form.
- **§9 Staleness and unknowns**: from slice M plus everything you dropped.

### Step 4 — depth round: targeted follow-ups on what round 1 left soft

From the "not covered / uncertain" lists and your own downgrades, pick the **3–5
most load-bearing unresolved items** (for example: the live small-file path, what
crosses the sandbox boundary at Commit, which limits are enforced vs declared) and
launch one focused subagent per item, with a narrower scope and the same three-block
return format. Do not use this round to re-run a slice that already returned clean
evidence — use it to close real gaps, one at a time.

### Step 5 — adversarial coverage and citation check (two more subagents)

- **Coverage:** *"Here is the finished map and the slices already covered. Name the
  load-bearing mechanism, boundary or contract this map does not mention. Cite
  paths; do not repeat what the map already says."*
- **Citations:** *"Here are N of the document's highest-value claims with their
  citations. For each, open the cited source and report whether it supports the
  claim exactly, partly, or not at all."* Any "not at all" invalidates that claim
  until you re-verify it yourself.

Fold real findings back in and record what both checks found in §9, including when
they found nothing.

### Step 6 — acceptance self-check, then report

Walk #156's acceptance list item by item and write the result as part of your report:

- every §2 claim resolves to a path, or is marked not-found;
- §3 states which small-file path is live today;
- §8 uses stated criteria, not taste;
- §9 lists stale documents and unknowns explicitly;
- every structural section has at least one ASCII diagram whose arrows trace to
  source;
- one status banner
  (`Status: Research; informative and not a product contract.` or
  `Status: Current planning checklist; no release candidate exists.` — pick the
  one that matches how the owner will use it and say why you chose it);
- all local links resolve; source commit pinned.

Then post on #156: the path, a short summary of §8, the unknowns, the two check
results, and the hash of the document. Do not close the issue and do not claim
anything the document does not support.

## Stop conditions

Pause and ask the owner only if:

1. the spec asks for something the source cannot answer and the gap would change
   §8's ranking;
2. two authoritative sources contradict each other in a way you cannot settle by
   reading call sites (for example: a released manual versus current code);
3. the study would require running code to answer (it must not — record the
   question as open instead).

Park the item in §9, note it on #156, and continue with the sections that are not
blocked. Never fill the wait by expanding the study's scope.

## What "done" looks like

`docs/roadmap/0.1/0.1.7/architecture-overview.md` exists, in the owner-approved
outline, as an ASCII-diagram-first document where every box and arrow traces to
the pinned commit; §3 answers the small-file question with evidence; §8 gives a
ranked, criteria-based read of what matters for the migration; §9 is honest about
staleness, gaps and what the two checks found — and #156 carries the report, still
open.
