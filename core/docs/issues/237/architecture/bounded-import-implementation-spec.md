# #237: implementation spec for the native-import memory gap

> **Status: Proposal; target LayerFS 0.1.7; not a released contract.**
> Source reviewed at `00c677489f813d6157a72e245203e74123e732f6`.
> This is a prospective implementation and proof sequence, not a memory or
> speed result. It accompanies the [frontier](bounded-import-frontier.md)
> and [fresh-namespace](bounded-compact-fresh-namespace.md) proposals.

**Treatment A outcome, 2026-09-24:** the matched
[memory result](../c3-entry-lifetime-memory-result-20260924.md) failed its
8-MiB whole-call peak reduction gate: the candidate was 753,664 B higher.
The product treatment was reverted. Namespace remains the peak, so
Treatment B's file-job-frontier condition is not met. The remaining
fresh-builder design gate below is still open.

**Direct-inode outcome, 2026-09-24:** a separately frozen allocation
treatment avoided both prerequisite root vectors and reduced one-shot
whole-call peak RSS by **17.22 MiB**, above its 4-MiB gate; full reopened
readback passed. See the [result](../c3-prerequisite-inode-fusion-result-20260924.md).
It is kept. Namespace Save still sets the high-water, so Treatment B
remains unrun and the full bounded builder remains a separate design.

**Fresh C1 scratch outcome, 2026-09-24:** a later, separately frozen
prototype removed tree-wide validation copies in the fresh path, but its
[one-shot result](../c1-fresh-validation-memory-result-20260924.md)
reduced whole-call peak RSS by only **2.42 MiB**, below the 8-MiB gate.
It passed full readback and was reverted. Namespace still sets the peak;
the remaining bound needs input ownership and a builder-level design.

**Compact-record prototype outcome, 2026-09-24:** after the record-custody
[plan](native-record-fresh-builder-plan.md), an isolated native-only C1
builder passed exact-root and reopened-oracle checks. Its one-shot
[diagnostic](../native-record-fresh-builder-result-20260924.md) observed
10.47 MiB less process RSS but owned 8.32 MB of scratch; charging the
entire scratch reservation leaves a 2.53-MiB numeric reduction, below
the frozen 8-MiB target. The source path lengths differed, and scratch
cache was unqualified. The prototype was not adopted. The scan and job
planner remain whole-vector, so this is not a full bounded import.

## Review verdict

The measured maximum occurs during namespace construction, after the full
file-job list has been consumed. On the retained compact-Job 100k/500-MB
[diagnostic](../c3-job-metadata-memory-result-20260924.md), process peak RSS
was 76,480,512 B after scan, 105,857,024 B after the file loop and
144,703,488 B by namespace Save. A refillable job queue therefore has no
proven ability to lower the current *whole-call* maximum. The first change
should remove a representation that is live during that maximum.

The existing `PreparedEntry` vector is one such representation. Its vector
reserved 14,680,064 B and its names occupied another 705,000 B on this
source. [`build_namespace`](../../../../crates/layerfs-service/src/save/import/namespace.rs)
needs entries while preparing metadata, inode and directory inputs, but does
not need them after those owned inputs exist. Both
[`catalog` callers](../../../../crates/layerfs-service/src/save/catalog.rs)
already own a `Vec<PreparedEntry>`. Releasing it before the C1 tree build is
the smallest treatment with a direct causal path to the observed peak.

This remains an `O(N)` import. A true end-to-end bound is a different project:
Service currently sorts every directory's complete child list, and C1's
fresh path retains complete input slices, `initial_counts[N]`, binding and
reachability maps/sets, and a directory-root map
([scan](../../../../crates/layerfs-service/src/save/import/scan.rs),
[C1 build](../../../../crates/layerfs-content/src/filesystem/update.rs),
[validation](../../../../crates/layerfs-content/src/filesystem/validate.rs)).
Changing only one vector or queue must not be reported as a bounded import.

## Treatment A: release entries before C1

**Source scope.** In `core/crates/layerfs-service/src/save/import/namespace.rs`,
take `Vec<PreparedEntry>` by value. Update the two `build_namespace` calls in
`core/crates/layerfs-service/src/save/catalog.rs` to transfer ownership.
Keep the current count/range calculation, prerequisite Save, inode values,
directory updates, `FilesystemInput`, C1 `build_filesystem`, tree Save and C5
publication. Once the existing `directory_updates` call has produced its
owned result, explicitly drop the entry vector before `input.check()` and
the C1 build. No worker, cache, SQL, canonical format, writer-budget or
public API policy changes belong in this treatment. Add no new dependency.

**Mechanism claim.** At C1 entry, the `PreparedEntry` vector and its owned
names are no longer live. This is a lifetime claim; freed allocator pages
may be reused or retained, so the change does **not** promise a 15.4-MB RSS
reduction. The remaining serial, metadata-root, content-root and inode
vectors still occupy their current space.

**Proof and decision.** Freeze one comparison identity, point probes and
meaningful threshold before the product edit. Report vector capacity/live
ownership just before C1 and process current/peak RSS at file-loop end,
tree-input construction, C1 completion and public-call return. Require the
entry vector to be dead before C1 and use **at least 8 MiB less whole-call
peak RSS** as the local adoption threshold; report a miss plainly and do
not rerun the arm. The 8-MiB threshold is a prospective decision rule, not
a predicted saving. Compare against an exact-identity retained control if
available; otherwise preregister one distinct control/candidate diagnostic
before either run. The prior compact-Job receipt is contextual unless its
product, instrument and harness identities match the new comparison.

Require both the native import and manifest-init callers to preserve their
results. For the native case, use one real public `Client::init_project`
call and a separate reopened oracle for all 101,001 paths, portable
metadata, 500,000,000 bytes and file hashes. With identical initial Store
and History allocator state, stack and scope, compare C5 reservation
start/count, exact root and object IDs, and Store geometry. Preserve failure
and cleanup receipts. A raw speed observation stays diagnostic because source
metadata residency is unqualified; the registered #236 SDK Init selection
uses debug binaries and is separate from release memory diagnostics.

## Treatment B: file-job frontier, only if it becomes the peak

If Treatment A lowers the namespace maximum enough that scan/file work is
the remaining high-water, freeze a separate job-frontier treatment. A
candidate may declare, for example, at most 64 queued jobs and 1 MiB of
queued job/path capacity, charging four in-flight jobs separately. Keep four
constructors, the bounded `ImportBatch` handoff and one C2 Save owner. The
owner must drain outputs while admission waits, propagate the first failure,
close input on error and join workers without a full-queue deadlock. Completed
file roots must retain their original entry-index association.

This bounds **jobs**, not discovery: `read_dir(...).collect()` sorts an entire
directory, and the breadth-first queue can retain many directory paths. The
existing 4,097-directory [test](../../../../crates/layerfs-service/tests/history.rs)
expects success; an arbitrary new fanout refusal would change accepted
inputs. Starting workers during scan also moves source reads earlier than
today's scan-then-construct route. Decide and test those source-mutation
semantics before implementing a refillable queue, or retain the phase order
using declared operation-owned records. An old v0.1.6 8-MiB planning limit
is not evidence that Core's new queue is correct or faster.

## Gate for a truly bounded fresh namespace

Do not begin the larger C1 change until its input custody is specified.
The design must name where every path, parent/name binding, file root,
metadata root and serial ordinal lives between the file, prerequisite and
tree Saves. C5 currently reserves one contiguous range after the final
entry count; the present breadth-first, byte-sorted entry order determines
each serial. A second source pass needs count/order and mutation checks;
operation-owned backing needs a format, byte budget, residency policy and
cleanup. A payload-size-proportional spool or cgroup page-cache growth is
not excused by a smaller process heap.

C1 validates complete input before emitting its first tree object. Preserve
that failure boundary with a bounded validation pass and construction replay,
or make an explicit separate contract decision. A fresh-native-only builder
may use stronger tree-shape invariants than generic `FilesystemInput`, but
must check them itself. The generic `base: Some` Workspace update path and
the wider `base: None` input semantics, including file hardlinks, remain.
Reuse the existing iterator-based sorted directory and inode page engines;
preserve the special canonical empty-directory page. Bound the validation
maps, per-inode counts, directory-root association, out-of-order file-result
window, high-fanout sort and any external ordering records. Merely passing
an iterator to the current shared `run` is insufficient.

The larger candidate must prove exact Core root/object IDs under identical
initial C5 reservation state, complete reopened readback, dense and #229
sparse-history Store geometry, failure/cleanup behavior, high-fanout and
deep-tree inputs, and aggregate memory during an overlapping Workspace
Commit. Keep the current three sequential C2 Saves initially: the service
provider reads published roots, and combining Saves is a separate ownership
and multi-writer design question. Existing short SQLite transactions must
remain short while input or output is backpressured.

## Final verification and reporting

Run the covering Core locked tests, examples, warning-denying Clippy,
format check, product-boundary guard and guard self-tests once at the final
source identity, following `core/AGENTS.md`. Record production LOC before,
after and delta for every commit. For each prospectively frozen memory
treatment, retain one sample per arm, every failure, exact source/binary/
harness identity, declared cache state, current and peak RSS, Store and
scratch disk bytes, and page-cache domains. Do not use a warm source or
recently written backing page to credit a timed phase
([benchmark rules](../../../../../docs/general/benchmark_rules.md)).

Run a separate default-budget-two overlap check: an import holds one service
writer permit for its entire request; a high-level `Workspace::commit` sends
multiple file/metadata/History mutations that each acquire a permit only
for that remote call. Check progress, admission/refusal, aggregate RSS and
competing-call latency while C2 private Saves interleave short transactions.
This check proves concurrency behavior, not a speed claim. Report any unrun
gate and keep the architecture notes as proposals until product and evidence
support a narrower claim.
