# Provider-backed namespace Init: first measurement selection

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Prospective sample selection, 2026-10-06 (Asia/Singapore), issue #307.
> No acquisition-v1/v2 sample has run when this selection is written.

The owner asked whether to run the namespace benchmark and then directed:
"lift the time budget a bit to have smooth run". This selection uses the earlier
Init allowance: 30 seconds complete performance command and 19 seconds independent
verification, symmetrically in both arms. The eight acquisition-v1 identities keep
15/9.5-second limits and sample_count=0. New acquisition-v2 identities change only
the budgets and their registration marker; no workload, cache, verifier, relative
speed or allocation gate is relaxed. Build remains release/locked with its 30-second
bound; actual test invocations still have explicit wall ceilings at most 120 seconds.

## Scope and inherited contract

This is a direct cluster-one component comparison through the existing
[SQLite Init family](../../../../core/benchmark/fs-bench-pro/families/phase7_sqlite.py).
It is not the retained SDK `init_namespace` route, a native mount, or S7/S9 closure.
The general [benchmark rules](../../../general/benchmark_rules.md),
[measurement workflow](../../../general/agent-measurement-policy.md),
[core routing](../../../../core/benchmark/fs-bench-pro/AGENTS.md) and
[Step10 contract](../../../../core/docs/issues/302/SQLITE-STEP10-CONTRACT.md) govern.
The [earlier owner-closure report](../../../../core/docs/issues/302/SQLITE-OWNER-CLOSURE-RESULTS-20261005.md)
retains all of its original verdicts and describes the approved reference treatment.

A3 product source is `0d84badef97f468a7269f9991ab920a8f7077a83`; its receipt
snapshot is `e517ae72bcccbc2587585fc0f28f2930c2b1da36`. The budget/selection commit
adds no Rust product, schema, profile or algorithm change. Seal its exact resulting
commit/tree, relevant source and SQL, example/manifest/lock/root-config, release
binaries, native cold helper, harness/oracle and workload identities before each
arm. The completed A3 checks retain their scopes; this registration does not
re-execute or relabel them.

## Cases, arms and order

Initial execution is four matched pairs, in this order:

| Case | Files / directories / logical bytes | Profile | Arm order |
| --- | --- | --- | --- |
| `phase7-sqlite-init-100-acquisition-v2` | 100 / 1 / 5000000 | Durable | baseline, candidate |
| `phase7-sqlite-disposable-init-100-acquisition-v2` | 100 / 1 / 5000000 | Disposable | baseline, candidate |
| `phase7-sqlite-init-1000-acquisition-v2` | 1000 / 10 / 20000000 | Durable | baseline, candidate |
| `phase7-sqlite-disposable-init-1000-acquisition-v2` | 1000 / 10 / 20000000 | Disposable | baseline, candidate |

The same registry also contains 10000/100000-file cases under both profiles.
They remain visibly NOT_RUN in this first selection; do not claim a complete
four-tier family. Each arm is sampled once. No retry, best-of, replacement sample
or larger post-failure budget. Preserve build failures, cold-state ineligibility,
refusals, timeouts, verifier failures and unrun rows in fresh append-only outputs.

Candidate: public `layerfs_project::init` via its `benchmark_init` release example,
using the global Store's acquisition provider and opt-in Monolithic schema 4.
The measured driver creates/opens a fresh Store, performs real Init, completes
the selected profile, checkpoints/releases allocation as required and closes.
Successful root/LayerStack acknowledgement and checked closure are required.
No per-Init scratch database, file-run alternative or benchmark-only algorithm.

Reference: clean pinned `7edddbdb8e8512627aed0ed42533ef099d802384`, through the
existing `sqlite_reference_init` wrapper and its frozen public Service route.
The wrapper is the already-owned reference driver, not a replacement product.
Reference is Phase4.5 MEMORY/OFF with split storage/history; candidate Durable
is WAL/FULL/fullfsync and Disposable is MEMORY/OFF. This difference is inherited
and explicitly reported. The comparison is against that selected reference,
not an isolated SQLite-vs-files experiment or proof of improvement over the
discarded uncommitted SQLite prototype. Root equality between each pair is required.

## Timing, cache, resources and oracle

Performance includes native cold preconditioning/attestation through checked
driver completion, profile finalization and close; the driver reports its complete
product clock and nested Init clock separately. Build/source preparation are outside
the performance timer and have separate receipts. Never subtract bootstrap,
checkpoint, cleanup or close to present a cheaper complete operation.

Use the existing seed-1 fixture definitions, exact class/byte distribution and
mtime/source manifest from `families/init_namespace.py`. Reuse qualifying closed
prepared masters through their manifest validation, using an independent ordinary
byte copy if transferring to the measurement checkout. Fresh writable Store/output
per Init; no mutated sample or APFS clone credited as cold. The native cold helper
invalidates and attests all source regular-file content pages before each arm.
Any nonzero/unavailable residency makes numeric evidence INELIGIBLE. Filesystem
metadata residency is unobserved and must remain labeled. Do not prime input via
oracle checks after the cold attestation or credit earlier writes/setup warmth.

The independent `verify_namespace` release example reopens Store/history and checks
every path, inode kind and directory metadata, plus full metadata/content for its
declared deterministic sample. Report sampled files/bytes separately; this is not
a full-content oracle or a whole-root S9 readiness proof. Verification has a separate
19-second wall cap and does not enter product speed arithmetic. It supplies
functional correctness; do not treat its wall as cold-read throughput.

Inherited joint gate: supported cold attestation, successful operation and independent
proof, root equality, checked cleanup, both arms under their absolute caps,
`10*candidate_product_ns <= 11*reference_product_ns`, and candidate final allocated
main/WAL/SHM no greater than matched reference's final total allocation. Report each
axis separately and retain a relative-speed FAIL even if the absolute caps pass.
Changing a budget cannot waive the 1.10× speed or storage requirement.

Record requested and read-back persistence settings; SQL counts are attribution,
not physical page or synchronization counts. Report actual end allocation with
main/WAL/SHM breakdown. It is not a disk peak. Darwin wait4 CPU/RSS covers each
driver's lifetime, not phase-only residency; report it as such and make no whole-
system bounded-memory claim. Physical I/O, peak journal, caches and sustained
service/debt acceptance remain S7 E2–E4 work. All four unrelated containers remain
running and are declared interference rather than interrupted.

## Execution, custody and reporting

Use a clean managed measurement checkout of the committed candidate so the primary
checkout's two unrelated untracked notes remain untouched. Targets, locks, output,
prepared inputs and the owned baseline stay worktree-local. Reuse closed qualifying
compilation inputs/builds with exact provenance; no concurrent measurement/build
in the same checkout. Export `LAYERFS_CONSTRUCTION_WORKERS=1`, retaining only the
supported Namespace Init four constructors and root ARM64 build flags.

The sole runner remains `core/benchmark/fs-bench-pro/runner.py run --case …` with
`--arm baseline|candidate`, a fresh owned `--out`, and the pinned owned
`--baseline-root` for the reference. Build/reference/cold/measurement/verifier
receipts retain their actual commands and all outcomes. A subprocess timeout is
stopped and recorded; no failed original operation is replayed. Read the report
template before every invocation. Setup/bootstrap failures are not samples but
remain retained; diagnose before any repair, and never overwrite their output.

Save a campaign ledger under the primary issue-307 checks/report area and preserve
raw outputs in the measurement checkout, with an independent byte copy of closed
receipts for review. The completed source report must state exact identities,
cache scope, operation/profile differences, all four pairs and all unrun larger
tiers. S7/S9 remain unchecked. No R1–R4 work, push/release/deployment, broad legacy
sweep or aggregate pre-push wrapper is authorized by this measurement selection.
