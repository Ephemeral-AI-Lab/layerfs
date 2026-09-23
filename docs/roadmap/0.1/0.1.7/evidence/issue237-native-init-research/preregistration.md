# #237 native Init research preregistration

> **Status:** Research; informative and not a product contract.

Source control starts at `7df25f9790996cf83232782c7b35f7c26fcc3252` in the
isolated `codex/issue237-init-research` worktree. All trials below are labelled
diagnostics, not #231 gate samples. Each case and source identity is invoked at
most once. The #231 fixture profile, seed 1, daemon-host public operation and
15-second request/command limits stay fixed. `#236` owns the pending host-direct
SDK route; these trials cannot qualify it. A 100k native Init run is not registered
by the current runner, so it remains NOT_RUN here unless a separate prospective
contract and driver are completed.

## D1: same-route cold-source mechanism trial

- Control: unchanged main product, `namespace-10000`, one call and full verifier
  if a confirmed root returns.
- Treatment: one unmerged research diff in `history_bootstrap::prerequisites`:
  reuse the C1 metadata root for entries with identical kind, mode and mtime.
  The one C2 save, public command, file reader, FileView validation and C5
  publication remain in the same places. No changed worker or Store policy.
- Expected counts: 10,101 metadata lookups but two unique metadata builds on
  this fixture (regular file and directory), instead of 10,101 builds. This is
  source-derived and must be checked against the manifest and Store rows. C2
  representation and canonical root should be unchanged. If physical Store size,
  `sum(length(data))`, pack occupancy or full readback worsens, reject it.
- Source cache: after fixture seal/reuse check, use the existing Darwin
  `Residency` backend on **every** source file: invalidate data pages, then a
  separate nonfaulting whole-input `mincore` pass. Require zero resident pages
  and record the gap until caller timer. Both arms get this exact method. The
  runner itself still calls this `source-cache-uncontrolled-v1`; the research
  sidecar is independent and makes no gate PASS claim.
- Result: retain every control and treatment receipt, telemetry, Store and
  readback result. Compare public operation and named Service spans as one
  observation per arm, with CPU and Store bytes kept separate. A failed root or
  unequal cache state prevents a matched speedup claim, but its mechanism counts
  and failure remain evidence.

## D2: sparse-pack protection

The #229 stride1 archived/current figures are historical observations, not a
matched pair for this trial. Read its pack policy and calculate allocation from
the retained rows. A metadata memo must leave pack reservation behavior intact;
no claim of sparse-pack repair follows from a dense Init Store. A future pack
algorithm requires a separate sparse-history control/treatment and full readback.

## Attempt ledger

- `issue237-d1-control-edc290627-a`: script import failed before it created an
  output directory, fixture or product process (`runner` resolved to the legacy
  module, which has no `owned`). Zero performance samples. Fixed only the Python
  import order before the first D1 control sample.

## D3: labelled count-driven localization after D1 failure

D1 control and metadata treatment both returned `Unknown` with no C5 root, and
their Stores had only file/prerequisite save rows. Do not repeat either identity.
An instrumented derivative of the metadata prototype will emit coarse Service
and C1 phase milestones plus completed C2 save counters to retained stderr.
Take one new cold-source `namespace-10000` run, solely to locate the unfinished
work and its counts. Its elapsed time is diagnostic, not a D1 third sample.
Keep the instrumentation as a separate unmerged diff, then restore product
source; never promote this run to performance admission.

## D4: native-only redundant file-root reopen removal

D3 observed 10,000 files saved by 6.80 s and a subsequent ~2.01 s
prerequisite stage despite its C2 save inserting only 11 objects. The source
opens each of the 10,000 just-constructed roots in that stage. Freeze one
additional unmerged research diff: native import passes an internal boolean
to `build_namespace` so the prerequisite pass trusts only file roots that the
same call constructed, length-checked and acknowledged through C2. The
pathless `InitLayerStack` keeps its existing `FileView::open` validation. The
metadata memo remains in both D1 treatment and D4. Reuse the same 10k source
fixture with zero-residency whole-input preflight, the same public daemon
request, seed, worker count, deadlines, full verifier and Store policy. Take
one D4 performance observation. A successful root plus full readback and equal
physical/semantic Store metrics are required before any public time comparison;
if D4 still fails, retain it and report only mechanism localization.

## D5: reducer work counters after D4 failed

D4 returned no C5 root and its verifier did not run. Do not repeat that source.
For one new labelled diagnostic identity, add only milestone output around C1
directory effects, directory value insertion and each 1,024 remaining inode
values. At each milestone read the reducer's existing counters: rows touched,
spilled, run reads/writes, runs and merges. Run the same cold-source 10k public
request once. This diagnoses whether tiered ordering lookups are superlinear;
the timer is not another D4 performance sample and no gate admission follows.

## D6: per-tier proven-absence interval

D5 found 9,011,202 run-row reads after 10k directory effects and 13,869,215
before the value loop finished, against 20,201 and 25,422 touched rows at
those points. The source's `RunStore::find` restarts a tier scan whenever the
next ascending serial is below the row that overshot a previous miss. Freeze a
single C1 change on top of D4: retain one half-open proven-absent serial
interval per immutable run tier, skip only that tier for a request inside the
gap, and invalidate it with the tier's existing scan reset. No page-size,
ordering quota, worker, buffer, pack, SQL or public API change. An external
two-tier regression must show the newer sparse run's gap still falls through
to an older matching row while row reads stay bounded, including backward and
upper-bound queries. Then take one cold-source native 10k operation with full
verifier and report its root, resources and Store geometry. D5 is an
instrumented diagnostic, not a matched time control; a completion after D6
proves feasibility but no historical speedup factor.

## D7: full-oracle metadata memo and corrected cold preflight boundary

D6 returned a confirmed 10k C5 root in 7.362191042 s; the 5 s full verifier
timed out with no child result. Keep that `INCOMPLETE` receipt. The independent
verifier currently calls `read_portable` for each path, although this fixture's
file entries share a metadata root and its directories share another. Freeze a
one-entry `(metadata_root, kind)` cache in each verifier worker and the
directory walker. Every path still checks kind/mode/mtime, and every file's
full contents still stream into SHA-256. The 5 s watchdog and four verifier
workers stay unchanged. Expect fewer repeated metadata read waves, but reject
the candidate on any missing path, byte, metadata or SHA-256 check.

The research cold wrapper also moves acquisition immediately after prepared
fixture validation, before the runner starts its complete-command clock; it
re-hashes every source file against the sealed manifest before invalidating
pages and takes a separate whole-input nonfaulting residency pass. Record a
≤1 s preflight-to-timer gap. These edits change verifier/build and research
harness identity; take one new 10k public operation and one full verifier at
that identity, retain any timeout or failure, and do not use D6 as a passing
proof. This remains a debug-build research row because the current Core runner
does not yet follow #231's frozen release-build requirement. SQLite page size
stays 4096.

## D8: restore the frozen release binary profile

D7's enhanced cold preflight found zero resident source pages and the native
10k call returned a root in 7.456454042 s, but its full verifier timed out
at 5.005929958 s. Both product and verifier binaries were still `target/debug`.
The #231 frozen build contract requires `cargo ... --locked --release`. Change
only the Core runner's build command, binary path and exact-reuse profile key to
that release profile, and add bounded progress counts to the external verifier
so a future timeout has a named stage/file count. Keep the same public operation,
source, 4096-byte SQLite pages, workers, 15 s command and 5 s verifier limits.
Attempt one release build; if its complete invocation exceeds 30 s, preserve
`BUILD_SLOW` and run no gate sample. If it passes, take one fresh cold-source
10k operation and full verifier. Compare debug/release timings only as build
profile observations, never as a product algorithm speedup, and retain D7's
timeout. The official runner still labels source cache uncontrolled; the
research sidecar is not a retrospectively approved admission contract.

## D9: release fast lane with immediate residency recheck

D8's release build passed in 17.543971458 s, but its cold preflight became
stale during process startup; the wrapper refused the call with zero samples.
Keep that `NOT_RUN` receipt. Preserve full source hash/eviction outside the
complete-command clock, then add a separate **nonfaulting whole-input mincore
check immediately before the caller timer**. It checks every file's size/mtime
and counts resident pages without priming any payload. The command clock will
conservatively include this check; report its wall separately. Require zero
resident pages and a ≤1 s recheck-to-timer gap. Use the exact release binaries
sealed by D8, same product/fixture/worker/PageSize=4096 and unchanged 15 s
limits. Take one fresh `namespace-10000` sample with verifier **SKIPPED** for
fast iteration. Do not count verifier cost in performance, do not promote this
diagnostic to #231 admission, and retain any failed row.

## D10: one release-profile 100k native Init diagnostic

D9's release fast lane completed the 10k public Init in 1.590847333 s;
full-oracle work is deliberately omitted for performance exploration. The
official #231 runner hard-skips 100k, so a research-only driver will call its
existing `_case` once under the same worktree lock, then retain the normal
four-case report/manifest with the 100k receipt marked diagnostic. The product
command stays the same `HistoryCommand::ImportNativeDirectory`, seed 1, 15 s
deadline, four construction workers, one C2 owner, 4 KiB SQLite pages, and
fresh Store. The source is the declared 100,000 files/1,000 data directories,
500,000,000 total bytes (two 100 MB anchors inside the total), generated once
under the worktree's prepared root. Reuse the full hash/evict plus immediate
nonfaulting whole-input mincore contract; require zero resident pages and
record all 100,000 files, pages and launch gap. The research recheck is inside
the command wall and its cost is reported separately, not hidden.

Run **one** release 100k performance sample at this exact identity, with
verification SKIPPED as the fast-lane default. Preserve any deadline,
command, telemetry, cleanup or resource miss. The 2.7 s historical cold target
remains a target, not an automatic PASS, and the official #231 first-pass
registry remains unchanged. No 100k admission or full-readback claim follows
from this diagnostic.

## D11: release 10k file-ingest count diagnostic

The owner now prioritizes 10k and a possible 700 MB/s clean-source result;
100k work stops after D10's retained failure. D9's 1.590847333 s public call
contains 1.249613667 s in `history.import_files`, which exceeds the entire
0.428571429 s target. Take one new labelled release diagnostic identity on the
same 10k public route/fixture/cold-data contract with verifier SKIPPED. The
only source difference is temporary count/time instrumentation in
`import_native.rs`: aggregate worker jobs/logical bytes and construct wall,
object-channel send count/wall, receiver wait wall, C2 owner `accept` count/wall,
and one completed `SaveOutcome` counter/profile line. These are nested and
concurrent spans; never add their times as disjoint CPU or claim an operation
speedup from this instrumented run. Keep four workers, channel capacity eight,
Store policy and 4096-byte SQLite pages unchanged. Retain the instrumented
diff and raw receipt, then restore product source. A candidate algorithm is
registered only after these counts locate avoidable work.

## D12: release 10k collision-validation detail diagnostic

D11 retained the completed Save's seven disjoint buckets and the owner span,
but omitted its already collected `DiagProfile` fields. On a new diagnostic
identity, retain D11's count-only Service instrument and print the existing
`DiagProfile` once after `save.finish`, especially `validate_ns` and its nested
`collision_query_ns`. Change no algorithm, query, page size, worker count,
channel capacity, timeout, fixture or public operation. Run one release-profile
10k public Init with a fresh Store, full source payload rehash/invalidation and
immediate whole-input nonfaulting residency recheck. Require zero resident
source pages on both checks; verification is SKIPPED. The official harness's
cache label remains uncontrolled, so the result is a diagnostic, never an
admission PASS. Retain the raw receipt even on failure, record the instrument
diff, and restore product source afterward.

Decision: if `collision_query_ns` is below 50 ms, reject a batched-candidate
lookup treatment for this 10k iteration. If it exceeds 50 ms, the query is
large enough to investigate, but nested `validate_ns` and one measurement do
not themselves prove a speedup. D11 and D12 are different instrumented source
identities and are not compared as a treatment pair. SQLite stays at 4096-byte
pages.
