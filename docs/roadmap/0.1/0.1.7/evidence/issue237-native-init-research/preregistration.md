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
