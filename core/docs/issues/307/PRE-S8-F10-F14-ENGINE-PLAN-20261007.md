# Finite engine service and original family receipts

> **Status:** F10 functionally closed after `916aa7d81`; F14 receipt foundation
> implemented, independent validation/phase observation next. Deterministic proof plus
> diagnostic observations under the owner acceptance decision. E04 stays closed.

Reuse the real Owner, six service classes, original Completion/JobSql receipts,
fixed default credits, existing E01 external JSON/counter encoders and existing
resource-observer helpers. Add an external `engine_finite` example with focused
`driver.rs`/`receipt.rs`, and `tests/finite_service.rs` importing that external
workflow. No production hook, new dependency or generic campaign runner.

Freeze4 Workspaces,6 concurrent outstanding logical callers/classes per Workspace,
32 finite waves. Every wave offers each class once in every Workspace before
waiting: Inode read, Publish, CapturedInodes, State, owned operation-record put,
AcquireBaseSource. Original publication reply acknowledgements and source releases
then finish before the next wave. Capture/operation ownership is established
explicitly before arrivals and released after them. Every class/Workspace must
complete all32 arrivals; no admission failure is retried. Hot-work variation and
parked-state behavior retain the existing unaffected source proofs; no latency
number is invented here. Native Bash concurrency remains S8.

Stream each original job once, including setup/dependencies/teardown, to a fresh
JSONL receipt with exact service-class/Workspace association and all observed SQL
families. Sum those actual families, payload/allocation counts and completions
against the owner's foreground aggregate; maintenance is separately observed.
Credits must return to zero; no original result is silently dropped. Queue wait
includes parking and SQL/service spans overlap, labeled accordingly. Logical close
and physical maintenance outcomes are distinct; do not infer final debt from row
activity. Reuse fixed-window encoders; never retain a workload-sized receipt list.

This proof is F10 plus the F14 family-receipt foundation. The following F14 step
registers the direct-engine successor in the existing evidence tooling, validates
these streams independently, and captures supported phase resources through the
existing external observer. Missing attribution stays explicit. Retired transport
subjects are withdrawn, not ported. New numerical latency/phase-memory acceptance
is owner-deferred; deterministic gates and unexplained growth/starvation remain.

Build first; host then Linux one selected proof per platform,100s stops, independent
proof<10s, source/binary/image/cache/profile pins, scoped final checks and exact LOC.
Global Store is out of scope for this independent engine slice; overlay uses its
unchanged schema16 MEMORY/OFF/EXCLUSIVE profile on native temporary storage.

## Exact terminal drain observation (before implementation)

Host02 completes every finite arrival but observes all4 namespaces still Queued
when the record-byte frontier is reached; Linux07 happens to observe all Gone.
Neither byte count nor delay is a portable terminal fence. Preserve both receipts.
Add a real operator diagnostic `OwnerWork.closed_namespaces`: increment only
when an acknowledged `reclaim_closed` step has `done=true`, never for a live
maintenance step or logical Close. Deepest changed files are
`overlay/owner.rs` (carry the closed/live origin of the existing maintenance
result) and `overlay/queue.rs` (publish the counter under the existing diagnostic
lock). No SQL, scheduling policy, wait, retry or work is added to a Store write.

The external driver waits on that memory-only original completion count for its
four owned namespaces, then makes one final read-only verification per route.
All must be Gone. This closes the full selected lifecycle without polling SQL or
waking the owner as a cleanup pump. Extend the new summary with the exact count;
old E01/E04 schemas/receipts remain unchanged. Rebuild and run the affected finite
proof once at this changed identity on host and Linux, retaining the prior partial
drain observations. This is product diagnostic capability, not a test hook.

## Final functional evidence

[Receipts](checks/pre-s8-finite-engine-20261007/)01/02 and06/07 retain original
host/Linux finite traces;04/08 are lossless compressed job streams with hashes
in05/09. The host's original physical close observations were Queued; Linux's
were Gone. Both supplied finite-service evidence, neither was relabeled.

The changed terminal diagnostic builds in10/15 and passes in11/16.14 pins exact
source/config/lock;12/17 retain the original complete job streams and13/18 their
hashes. Each platform completes all32 arrivals for each of6 classes in each of4
Workspaces:768 primary arrivals,256 dependent reply/source releases,20 setup and
16 teardown/verification jobs,1060 original jobs total. Peak queued jobs24,
peak credited bytes92320, final credited bytes/outstanding0 and receipt overruns0.
Exact SQL-family, payload and allocation sums equal the owner's foreground totals.
Automatic maintenance remains separate:144 rows and16384 operation-record bytes
removed, with4 acknowledged terminal namespace completions and all4 final Gone.

The counter remains0 through live maintenance and all logical Closes while original
captures are held. Release then permits terminal reclamation. Waiting reads only
memory diagnostics, issuing no SQL/status or cleanup wake. One final read per route
verifies the exact Gone outcome. Complete proof commands finish0.13s host/0.16s
Linux under100s outer stops; wall figures are diagnostic, not latency admission.

This selects asynchronous logical callers over the real engine, with a finite
wave schedule and default admission. It does not claim infinite-load fairness,
per-class millisecond limits, real concurrent Bash or FUSE readiness. Captured
readiness/frontier correctness retains the unchanged earlier owner proofs.

Host binary SHA256 `6c7453797264c83c579a00d94e594cc097959c11a4210d890024ff8c531ed652`;
Linux `2d76557478e3e751cd93261fc3d3c2e9faa83490cc043c95bb52f06aefec3bd7`.
Rust1.85.1/repository ARM64 flags, pinned image
`sha256:378b799ef43343fc64008b6a5ef456dd6bf0cb8f6ea4cec72e7b42dfc17d2cd6`.
Overlay schema16 MEMORY/OFF/EXCLUSIVE; native host temp or Linux `/tmp`, one owner.
Global Store and its Durable execution are not selected; Durable development
execution remains owner-deferred. Caches are functional natural state, no cold
or timing treatment. No original product unknown or failure occurred here.

Host raw streams and closed backing artifacts remain at their printed paths;
compressed exact copies are committed. Linux raw streams remain under ignored
`core/target/pre-s8-finite-engine-linux-v*.jsonl`; its successful temporary backing
leaves with the owned container. No unrelated container/process is touched.
The new example's ordinary executable/phase-observer run remains NOT_RUN here;
`--test --example --no-run` is compilation only, not execution of its main.
F14 must independently validate the schema and collect supported phase resources;
this checkpoint does not close those rows by counter consistency alone.

Final19–22 pass: daemon all-target Clippy `-D warnings`, format,706-file product
boundary and46 guard tests. Source and API docs reflect the new diagnostic.
Production LOC:165669→165673 (delta+4); core100252→100256,
active57378→57382; reference65417, excluded predecessor36325 and excluded
integration6549 unchanged. Exact first-parent/staged method is in23-loc.json.
This is a diagnostic capability plus external proof, not transport retirement.
