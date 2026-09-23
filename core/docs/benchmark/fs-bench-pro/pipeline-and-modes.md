# Pipeline and execution modes

> **Status:** Proposed v0.1.7 migration contract. Case membership and numeric
> gates are not frozen; no benchmark has been run under this proposal.

## Pipeline to preserve

Keep the useful fs-bench-pro run sequence and its evidence boundaries:

1. Pin source tree, product, compilation/dependency, binary/image, harness, and
   workload identities. Reuse builds only by matching seals.
2. Resolve one registered family/case and its seed or repetition. Reject
   ambiguous selectors before setup.
3. Acquire or validate one prepared master outside the timed operation. Record
   cache hit/miss and preparation cost.
4. Give the sample an independent workspace/Store state: `fresh` for
   initialization/fresh-output cases, `clone` for post-initialization cases.
5. Start the selected runtime, establish the declared process and cache state,
   then begin the product timer at the registered operation boundary.
6. Run one full operation. Ingest the product's native telemetry into one
   retained raw event file and a parsed receipt; record external
   process/container resources in their own scopes.
7. Verify separately against an independent expected result using the pinned
   identity. Keep operation, verification, cleanup, and complete-command status
   separate.
8. Append all evidence to a fresh output path. After validating the retained
   telemetry file, remove only redundant telemetry capture/operational files;
   record that cleanup in the final receipt and manifest. Preserve failures
   and ineligible/incomplete/unrun rows; derive
   summaries without rewriting retained raw receipts.

Setup reuse never removes work from the measured operation. Operation time,
setup time, runtime/connection startup, verification time, cleanup time, and
complete invocation wall remain distinct. A `--perf-fast` equivalent is one
complete registered workload, not a reduced workload or best-of run.

## Prerequisite pilot gates

[#230](https://github.com/Ephemeral-AI-Lab/layerfs/issues/230) first needs the
[shared substrate #235](https://github.com/Ephemeral-AI-Lab/layerfs/issues/235),
then migrates three prerequisite family clusters before the remaining suite:

1. [#231](https://github.com/Ephemeral-AI-Lab/layerfs/issues/231): first
   measure 100, 1,000 and 10,000 native-import files once each; retain the
   100,000 tier as `NOT_RUN` under the original four-tier final gate. The
   bounded `InitLayerStack` cannot substitute; see the
   [first-pass spec](issue-231/SPEC.md) and [route gap](#initialization-equivalence-gap).
2. [#232](https://github.com/Ephemeral-AI-Lab/layerfs/issues/232): all 56 active
   cases in `edit_length_preserving`, `edit_length_changing`, and
   `edit_canonical_chunk_count`. They require the public SDK edit route into the
   live Workspace, not a POSIX write or direct C1/C2 edit. The five capped-v1
   duplicates remain historical optional rows.
3. [#233](https://github.com/Ephemeral-AI-Lab/layerfs/issues/233): all 20
   `tiny_file_churn` cases plus the single `local_snapshot` lifecycle. The
   latter creates 25,000 one-byte files, edits and restores 256 selected files,
   and makes three Commits within **one** sample. These families distinguish
   high payload volume from high metadata cardinality.

Freeze each pilot family's **complete** membership, public operation, fixture,
seed, timer, cache contract, oracle, numeric gates, resource scope and receipt
schema in a committed specification before implementing or collecting it.
Develop with one selected case at a time, but complete the entire cluster at
   final source identity: exactly one eligible performance sample per case,
separate identity-matched verification, checked cleanup, and every failure or
`INELIGIBLE`/`INCOMPLETE`/`NOT_RUN` row retained. None of those nonpassing
statuses clears a required case. No remaining-family migration implementation
or collection begins until **all three** child issues pass.

Shared two-mode harness plumbing may be built only as the pilots require it.
`storage-direct` is a separate C1-to-C2 operation claim, not a replacement for
any of these public workflow pilots. Later work may register
`mixed_load_bearing`, `branch_development`, `multi_workspace_development`, and
`dedup_branch_history` under their own frozen contracts. Preserve all old
receipts and optional-case dispositions; do not infer a passing row from a
smaller or differently routed workload.

## Four macro timing scopes

One registered case at one source identity produces **one performance sample**. Its caller
monotonic interval is the headline operation time. The four macro scopes below
name work within that operation or a separately registered comparison; their
micro spans are observations **inside the same sample**, not extra samples.
`M1` through `M4` are headings in this benchmark report, not names emitted by
`layerfs-telemetry` or four required JSON fields. Keep each timer's actual
product label in the raw receipt. The scopes share code but are not four
additive timing terms.

```text
M1 PUBLIC WORKFLOW: one caller interval, start -> promised acknowledgement
  #232 SDK edit + explicit Commit, or #233 FUSE mutations + explicit Commit
  micro: edit/mutate | capture/prepare | file/metadata/tree saves | reconcile
    |
    +-- M2 DELIVERY: daemon -> bridge/network -> host Service.handle -> result
          micro: daemon local call | Service local call | connection/framing
            |
            +-- M3 C1/C2 ENGINE: construct/edit -> SaveHandoff -> Store SQLite
            |     micro: content probe/chunk/encode/read | C2 begin/read/finish
            |
            `-- M4 C5 HISTORY: stage -> Commit/Branch publication
                  micro: reservation | stage_changes | commit_staged

  #231 native Init/import instead begins with source-directory scan/read,
  then C1/C2 construction/save and C5 identity/LayerStack publication.

M2's matched experiment belongs to #193, with one sample per route/arm:
  direct:  caller ----------------------> SAME Service.handle -> C1/C2 -> SQLite
  forward: caller -> daemon -> network -> SAME Service.handle -> C1/C2 -> SQLite

M3's separate component experiment belongs to Stage 6 / storage-direct:
  host caller -> public C1 APIs -> C2 SaveHandoff/Store -> host Store SQLite
```

| Macro | Recorded now | Micro timing still needed for the proposed breakdown |
| --- | --- | --- |
| M1 public workflow | Workspace has named Stage/Commit **status** phases, not durations. The #230 caller timer does not exist yet. | One caller root at the frozen public boundary; bounded substeps for SDK edit or FUSE mutation, capture/prepare, Commit and reconcile. Do not create one timer node per file or callback. |
| M2 delivery | Daemon and Service emit separate local `LFT1` operation roots. In the Workspace route, the daemon root includes connection and call. | Caller route interval under #193, and named connection/framing/input/result spans only if required for attribution. The existing roots do not isolate socket time. |
| M3 C1/C2 | Service `begin_save`, `construct`/`edit`, `read` and `finish` scopes plus C1 content children exist. Stage 6 has its own component timer. | `service.construct`/`service.edit` include C2 `SaveHandoff::accept`; never label them pure C1. C2 accept has no per-object node. Add a bounded aggregate span only if the frozen claim needs that split. |
| M4 C5 | History calls run within the Service operation root; outcome is known. | Coarse spans for Init reservation/publication, `stage_changes` and `commit_staged`. SQL validation, insertion, Branch CAS and stage removal stay grouped until evidence justifies finer spans. |

The first #230 runner takes **one raw run per case**. It has no control arm,
candidate arm, paired scheduler or speedup ratio. A later source-pinned run
can inform an algorithm investigation, but one observation does not establish
a statistical improvement. #193's same-handler `direct`/`forward` treatment
is a separate experiment comparing complete delivery routes, not an isolated
socket cost or a FUSE Commit. M4 remains inside the public Init/Commit timer;
it is not a separately approved optimization campaign. Cross-macro rows
provide context, not a numerical decomposition of end-to-end time.

Within one process, timer nodes are inclusive. A parent already contains its
children; add only disjoint sibling work if their boundaries actually justify
it. Daemon and Service clocks are independent, so never subtract their roots
or peaks to invent transit time. A case such as `local_snapshot` has many file
saves and three Commits **inside its one sample**; retain their counts and
bounded micro observations, while the registered caller interval remains the
one raw performance result. The C1/C2 implementation is shared across the
views, but a Workspace Commit may make many saves while `storage-direct` names
one registered operation. M1 minus M3 is not daemon overhead.

Pin inputs, independent writable Stores, cache contracts, telemetry settings
and resource limits for every run. C5's
[`stage_changes`](../../../crates/layerfs-history/src/sqlite/staging.rs)
validates and records frozen context; [`commit_staged`](../../../crates/layerfs-history/src/sqlite/commit.rs)
validates it again, inserts or verifies a Commit, compare-and-swaps the Branch
head and removes the stage in one transaction. Investigate a C5-specific
optimization only if eligible measurements show material time, contention or
history-scaling cost.

## Mode boundaries

### `daemon-host`

Exercise the actual v0.1.7 daemon-to-host operation route selected by the
implemented runtime. Start the real binaries and dependencies from their pinned
artifacts. The caller's timer includes each operation step assigned to the
end-to-end contract, including transport and terminal result consumption when
those are part of that case. Report daemon, host service/storage, and caller
process measurements separately. Do not add overlapping spans or use unrelated
host/daemon timestamps to reconstruct caller wall time.

The exact placement and operation boundary must be written into each registered
case before implementation. Do not assume the older FUSE container layout still
matches v0.1.7. A missing production route is `NOT_RUN`, not an emulated pass.

### `storage-direct`

Exercise the public C1-to-C2 storage pipeline locally, omitting daemon and
transport work. Its current closest implementation is the frozen Stage 6
`pipeline.*` path: C1 construction/edit, C2 save, and its acknowledgement. That
path does **not** include Workspace Commit. Keep its Stage 6 identity and
structural-complexity claim unchanged; a new migration case needs a new identity
and contract. Main does not expose a public Workspace-to-Store Commit bypass, so
a true direct Workspace Commit is `NOT_RUN` unless the product adds that public
route or the frozen target is explicitly the C1-to-C2 operation.

For whichever public route is registered, start its timer before the first
required operation work and finish at its documented acknowledgement. Prepare
the input fixture before the timer only when the product contract also receives
a prepared input; never move canonical product work into setup.

This mode is useful for storage-path attribution and an absolute product gate.
It is not evidence for the end-to-end mode. The two rows can share an input
identity and independent oracle, but keep mode, operation boundary, setup,
resource scope, and status distinct. Do not invent a direct comparison when the
two routes perform different required work.

## Initialization equivalence gap

The v0.1.6 public Init accepts a native directory and pays for scanning names,
reading file bytes, constructing objects, and admitting them to the Store. Its
Init-specific parallel workers feed the same object-admission machinery used by
Workspace Commit. The current v0.1.7 `InitLayerStack` is a smaller history
bootstrap: regular-file roots must already be saved, and one bounded manifest
describes the tree. Sharing the C1-to-C2 save interface does not make these
end-to-end operations equivalent. See the [v0.1.6 source](https://github.com/Ephemeral-AI-Lab/layerfs/blob/v0.1.6/crates/layerfs-layerstack-store/src/layerstack.rs)
and the [v0.1.7 bootstrap contract](../../architecture/proposal/fuse-workspace-snapshot-overlay/03-commit-integration.md).

```text
v0.1.6 public native Init

  native directory (100 to 100,000 files)
       |
       v
  scan names + read bytes -> construct files/tree (Init workers)
       |
       v
  shared object admission -> Store SQLite objects + LayerStack metadata

v0.1.7 current InitLayerStack

  native directory --X--> no public native bulk importer

  file roots saved by earlier operations
       |
       v
  one pathless manifest (at most 128 entries including root; 32 KiB metadata)
       |
       v
  C1 prerequisites -> C2 save -> C1 filesystem tree -> C2 save
       |
       v
  C5 LayerStack record in the separately configured history catalog
```

The pathless bootstrap once had a **128-entry test cap**. That arbitrary cap is
lifted. Its legacy request still has a finite metadata frame, a 16-bit count and
16-bit parent indices, so it remains a small pre-saved-root operation. The
public native-directory import uses an internal wider index and no default
file or entry-count cap. Its source scan and reads occur inside one request.

```text
removing only the old 128-entry check
       |
       +--> request metadata still capped at 32 KiB
       +--> encoded entry count still uses 16 bits for pathless bootstrap
       +--> no streaming / multi-batch Init or native directory scan
       `--> cannot make pathless bootstrap equivalent to native import
```

Even the smallest entry occupies at least 24 encoded bytes: 100,000 entries
would need at least 2.4 MB before names or file roots. A replacement public
import needs bounded streaming or batching, source reads inside its declared
timer, complete final-tree construction, and its own cache and verification
contract. Preserve the Init multi-worker exception; do not relabel a sequence of
pre-saved file roots or mounted creates as the v0.1.6 Init case. Record inherited
`init_namespace` selections as `NOT_RUN` for the missing v0.1.7 operation until
an equivalent public path and frozen case contract exist.

## Proposed layout

The [#231 first-pass spec](issue-231/SPEC.md) chooses a smaller Python
orchestrator around the existing production binaries. It does not add a
second Cargo workspace or a benchmark Rust driver unless the new public Init
operation proves one necessary. Add adapters only with their first real case.

```text
core/benchmark/fs-bench-pro/
  AGENTS.md                  first-pass agent rules
  runner.py                  list/run/verify/report; one-case lazy setup
  families/
    init_namespace.py        case declarations, public Init, full verifier child
  shared/
    identity.py              source/build/image/fixture seals
    fixture.py               case-scoped immutable source acquisition
    telemetry.py             LFT1 parser and compact raw retention
    evidence.py              receipt, manifest and owned cleanup
  tests/
    test_init_namespace.py   case/fixture/oracle refusal checks
    test_substrate.py        identity/isolation/telemetry/receipt checks

core/docs/benchmark/fs-bench-pro/
  README.md
  pipeline-and-modes.md
  parameters-and-telemetry.md
  telemetry-ingestion-and-retention.md
  preparation-and-cache.md
  issue-231/SPEC.md          first-pass cases, CLI, speed and LOC plan

benchmark-results/fs-bench-pro/              gitignored run output
  prepared/<compatibility-digest>/           closed master + manifest
  <run_id>/<mode>/<family>/<case>/
    perf.jsonl                               raw selection rows
    telemetry.lft1                          exact native LFT1 lines when selected
    verification.json                        separate proof result
    receipt.json                             identity, parsed telemetry, diagnostics and statuses
    report.txt                               derived human-readable summary
  <run_id>/manifest.json                    hashes of all retained run evidence
```

The temporary stderr capture and benchmark-owned native telemetry segments
are removed only after the retained event file is validated. Record that
cleanup in the final receipt and evidence manifest. A failed ingestion
preserves the original stderr as failure evidence. See the
[parse and recycle rules](telemetry-ingestion-and-retention.md).

Use the core product lock/target for changed production binaries and a
worktree-local run lock only; different worktrees build and run concurrently.
Pin the full immutable Docker image ID and avoid legacy mutable tag/prune
collisions. Reuse focused fixture, phase, isolation, cache and receipt helpers
where their behavior matches this contract. Do not retain two competing
implementations of the same fixture or receipt pipeline.

## Readiness before implementation or collection

- #179 is closed with a bounded Workspace/FUSE route and small real-daemon
  scenario. It can exercise the substrate, but native-directory Init and
  load/performance qualification remain #231 work.
- #179's functional route already crosses the real daemon, host Service, and
  C1/C2, but its passing small-project receipt uses telemetry off. For each
  `daemon-host` case, verify the selected telemetry and deployment behavior
  against #192's contract. Unfinished #192 work outside that registered case
  is not a blanket gate. Align with #193's forward route where the measured
  boundary is the same.
- Freeze each selected family's complete registry, case identities, fixtures,
  timer formulas, resource scopes, cache contract, expected results, gates, and
  receipt schema before that family's benchmark code or collection, as required
  by the root measurement rules. Keep remaining families unimplemented until
  all three pilot issues pass.
- Keep Stage 6 C1/C2 measurements under their frozen contract. Preserve only
  compatible recipes and helpers; do not change old IDs, claims, or receipts.

Drafting is not evidence. Until these gates are satisfied, relevant selections
are `NOT_RUN` and their gates are `NOT_FROZEN`.
