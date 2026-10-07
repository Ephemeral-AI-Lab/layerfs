# S7 operation accounting: per-job SQL families and completion ownership

> Status: implemented source checkpoint with scoped functional proofs on host
> and Linux. S7 and S9 remain unchecked; no E1 sample, numerical gate or
> resource qualification follows. E04 is not reopened or rerun.

Parent `490c3ab3a71823268e476e6affdcfbf0c6f9fef0`. This is checkpoint 4 of the
[implementation-owner plan](S7-S9-IMPLEMENTATION-OWNER-20261007.md) and the
first change of the operation-accounting package. Product source changes are
confined to `core/crates/layerfs-daemon/src`. The same commit applies the
[owner's Disposable-only direction](checks/e04-disposal-20261007/63-owner-disposable-development.json)
to the mixed-profile external Rust tests. Source description is in
[daemon owner](../../architecture/21-daemon-owner.md#s7-completion-ownership-and-family-receipts-307)
and [operation cost observations](../../architecture/36-operation-cost-observations.md).

## What changed

| Requirement | Before | Now |
| --- | --- | --- |
| Per-original-job statement families (E2) | `JobWork.sql` was one statement total | `JobSql`: one exact row per family with observed work in that job's exclusive turns, including parked readiness turns and failures; `total()` and `expanded()` derive from those rows |
| Owner aggregate before completion visibility (E2) | Readiness-failure paths sent the result before the foreground SQL delta was published | One connection delta per turn feeds the job receipt and the foreground aggregate, and the aggregate is published first on every path |
| Failure accounting (E2) | A capture whose state read failed after an unready check was not counted in `completed` | Every terminal outcome other than a stop cancellation is counted |
| Receipt, queue and held-result storage (E2/E4) | Charge `Command + JobWork + 512 + input + reply`; the job, a per-job channel and its envelope were larger than that charge, and queue capacity was uncharged | Charge is the largest of three non-coexisting stages plus the cell; queues are pointer deques in capacity fixed at startup and subtracted from job bytes |
| Queue-only occupancy (E2 gap) | Unavailable; credits include caller-held results | `queued`/`peak_queued` job counts. Queue-only bytes remain unavailable |
| Exceptional outcomes | Unattempted results boxed a second command copy beside the envelope | The queued box is released first; the unattempted stage is charged explicitly |

Unchanged: 8 MiB total, 64 KiB lifecycle reserve, 16 namespaces, two lifecycle
slots per namespace, the 512-byte bookkeeping allowance, every declared reply
size, the SQL algorithms and schema, `Pending`/`Completion` signatures and the
`Unattempted { cause, command }` result. No dependency was added.

## Why the full array did not fit, and the correction

A lifecycle slot has 65,536 / 32 = 2,048 bytes. One `StatementWork` row is 136
bytes; all fourteen families are 1,904 bytes, which cannot share a slot with a
320-byte command and the retained 768 bytes of bookkeeping and reply. Seven
rows are 952 bytes and still do not fit while the command, the job receipt and
a second receipt copy in a per-job channel are all held at once.

The correction is ownership, not a larger number. A job's storage is exactly
one of: the boxed queued job; the boxed outcome allocated when the job
finishes; or an unattempted outcome returning the original boxed command and
cause. The queued box is released before the outcome is allocated, receipt rows
are an exact-size slice replaced without overlap, and the per-job channel is
replaced by one typed cell. The job is charged for the largest stage.

Compiler layouts, `aarch64-apple-darwin`, rustc 1.85.1, locked test profile.
Public types were read with `size_of`; the three private ones follow from the
measured charges and reconcile with the measured live bytes below.

| Type | Bytes | Type | Bytes |
| --- | --- | --- | --- |
| `Command` | 320 | `StatementWork` | 136 |
| `Result<Response, OwnerError>` | 232 | `DatabaseWork` (14 families) | 1,904 |
| `OwnerError` | 40 | `PayloadWork` / `AllocationWork` | 56 / 48 |
| queued `Job` (private) | 568 | `JobWork` | 152 |
| `Outcome` (private) | 384 | completion cell with its counters (private) | 96 |

```text
charge = cell + 512 + max(reply, 232) + max(queued, held, unattempted)

lifecycle, default reply 256, no input, never parks:
  queued      = 568
  held        = (384 - 232) + 7 x 136                = 1,104
  unattempted = (384 - 232) + 320 + 40               =   512
  charge      = 96 + 512 + 256 + 1,104               = 1,968
  32 slots    = 62,976 <= 65,536                      (2,560 spare)

other classes, default reply, no input:
  queued      = 568 + 14 x 136                        = 2,472
  held        = (384 - 232) + 14 x 136                = 2,056
  unattempted = (384 - 232) + 14 x 136 + 320 + 40     = 2,416
  charge      = 96 + 512 + 256 + 2,472                = 3,336
```

The inline result value is part of the reply bytes, as the existing reply
sizes already assumed; a declared reply below 232 bytes is raised to 232 so it
is never undercounted. The previous lifecycle charge was 320 + 264 + 768 =
1,352; the new one is larger and now covers what is actually held.

The fixed scheduler state (the shared owner state, 16 lanes and their pointer
queues) is 18,776 bytes on the host and 18,760 on Linux. It is allocated once,
reported as `OwnerWork::scheduler_bytes`, and removed from the bytes jobs can
use: ordinary classes may hold `bytes - scheduler - reserve`, lifecycle jobs
`bytes - scheduler`. Startup refuses a configuration that cannot hold every
lifecycle slot at the real charge or that leaves no ordinary bytes.

### The seven-family lifecycle bound

Seven is a maintained source invariant, not an observation limit. Every
lifecycle transaction executes the pre-BEGIN freelist read (`Startup`), `Begin`
and exactly one of `Commit` or `Rollback`; a failed COMMIT quarantines without
a rollback attempt. Lifecycle jobs never reach the capture/install readiness
check, so they never park. The domain families reachable from lifecycle
commands in `layerfs-overlay/src/lifetime` and their shared helpers are at most
four per command. `ResolveFailed` and `ReleaseClosedCapture` reach exactly
seven (`Startup`, `Begin`, `Commit`, `Workspace`, `Capture`, `Lease`,
`Reclaim`); the public sweep below observes that maximum.

The bound limits the admitted charge only. Rows are allocated for the families
actually observed. If a lifecycle job ever observes an eighth family its row is
retained, the excess is added to the ledger and to that job's credit at
publication, and `receipt_overruns`/`receipt_overrun_bytes` report it. Nothing
is truncated, and no family value is derived from a class or command name. An
Overlay change that adds a family to a lifecycle command must update this
bound and its reservation arithmetic; the public sweep asserts zero overruns.

## Proofs

All selections are functional checks with uncontrolled cache, built first with
locked Rust 1.85.1 and the repository ARM64 build inputs, each run once under
an explicit wall stop at most 110 s (host) or 100 s (Linux image
`sha256:378b799ef43343fc64008b6a5ef456dd6bf0cb8f6ea4cec72e7b42dfc17d2cd6`).
Receipts are in [checks/s7-completion-ownership-20261007](checks/s7-completion-ownership-20261007/00-index.json).
The Daemon owner uses its own overlay profile; no global Store profile is
involved except the Upstream bodies, which select Disposable.

| Claim | Public proof | Result |
| --- | --- | --- |
| Family receipts equal the foreground family deltas | `completion_ownership::original_family_receipts_equal_the_foreground_aggregate_on_every_path`: 41 original jobs over success, a rolled-back failure, parked captures and close; SQL family arrays, payload and allocation reconciled after each completion and after parked sequences | PASS host and Linux; 288 foreground attempts, 3 parked turns |
| Lifecycle family bound and zero overruns | Same body: 35 lifecycle jobs covering 29 distinct lifecycle commands (the `DatabaseWork` diagnostic is not swept) | PASS; maximum 7, `receipt_overruns = 0` |
| All 32 lifecycle slots under ordinary saturation, unchanged caps | `completion_ownership::every_configured_lifecycle_slot_is_admitted_while_ordinary_credit_is_saturated`: 133 held ordinary results fill ordinary bytes to within one default charge, then 32 lifecycle results are admitted, served and held together; a 33rd in one namespace is refused | PASS host and Linux; lifecycle charge 1,968, ordinary 3,336, peak 8,367,251 <= 8,388,608 - 18,776 |
| Live heap never exceeds its credit | `completion_storage`: counting allocator in a one-test binary; held lifecycle 624 B/job live vs 1,968 credited, parked capture 944 vs 3,336, unattempted after stop 1,128 (host) / 1,152 (Linux) vs 3,336 | PASS host and Linux; live bytes equal the layout sums above |
| One-shot delivery, blocked waiter wake, receiver loss, stop | `completion_ownership::a_completion_is_handed_out_once_and_wakes_its_blocked_waiter` plus the existing owner bodies | PASS host and Linux |
| Existing Daemon behavior | 13 existing Daemon test binaries and the two Workspace binaries which drive the daemon | PASS: 73 host bodies in 15 binaries; 40 Linux bodies in 12 binaries with one ignored |
| Static checks | Formatting, Clippy `-D warnings` on all core targets, product boundary guard (738 production files) | PASS |

The heap figures count requested allocation sizes. Allocator rounding is the
declared allowance, and nothing here measures SQLite, pager, kernel or process
residency. The worst lifecycle held stage is 96 + 384 + 952 = 1,432 requested
bytes inside its 1,968-byte charge.

## Disposable-only development selection

Seventeen two-profile loops in eleven external Rust test files (Daemon 1,
Persistence 9, Project 3, SDK 4) now iterate a `DEVELOPMENT_PROFILES` constant
containing only Disposable, with the deferral stated beside it. No product
source or test-only branch changed, and Durable support is untouched. The
sixteen converted Persistence, Project and SDK bodies were selected once each
on Disposable: fifteen pass, and `an_unsupported_platform_…` has no macOS body
and is NOT_RUN here. The converted Daemon body is the ignored explicit Docker
proof and was not run. Test function names keep their historical "both
profiles" wording; their receipts state the actual profile.

Not changed, and not selected in this checkpoint: tests that rely on the Store's
default Durable profile, the Durable-named Project examples, and the Python
harness families and campaigns that take a profile argument. They must select
Disposable or be skipped before their next selection. Outstanding Durable
execution is **NOT_RUN — deferred by owner for Disposable-only development**.

## Retained failures in this checkpoint

| Receipt | Outcome |
| --- | --- |
| `01-completion-ownership-first-run.stdout` | Two of three new bodies FAILED. Both were test-design errors: held lifecycle results had filled the namespace's two slots, and a second capture in a workspace that still retained its first capture was correctly refused `CaptureInFlight` instead of parking. The saturation body passed. Not hangs: both failed inside their own 20 s bounded waits |
| `02-waiter-diagnostic.stdout` | Named diagnostic of the second failure with the observed counters; FAILED as expected before the test correction |
| `09-shell-invocation-failed.stdout` | A zsh loop variable named `path` replaced `PATH`; no test binary executed. Not a product or test result |

No product behavior was changed in response to these; `03` onward use the
corrected tests, and `10` (host) and `07` (Linux) are the final-identity runs.

## Scope limits and remaining operation-accounting work

- The retained E01/E04 serializers and the Python validator keep their
  historical statement-total schema, including `statement_family_status:
  UNAVAILABLE` in those streams. A family-carrying successor schema, its
  validator and tests are not written; no diagnostic was collected at this
  source.
- Of E04's eleven unavailable dimensions, per-original-job statement families
  are now implemented and reconciled in the owner API, and queue-only job
  occupancy is observable. Queue-only bytes, provider facts/IDs, whole-operation
  copies, statement-fingerprint/bind/EXPLAIN correlation, indexed visited rows,
  exact eligible debt, isolated waits, phase residency, physical I/O and cache
  enforcement remain unavailable.
- SQL spans still overlap service time and queue wait still includes parking.
- `device_capacity` has no host body and one ignored explicit-device body on
  Linux; it was not run. `upstream_docker` is an ignored explicit Docker proof
  and was not run.
- No Durable selection, measurement, E1 sample or Init/history rerun was made.

## Production LOC

`tools/production_loc.py`, SHA256
`c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb`, on exact
`git archive` exports of the first parent and the staged tree (`core/crates`
and `crates`); see [the comparison receipt](checks/s7-completion-ownership-20261007/13-production-loc-comparison.json).

| Scope | Before | After | Delta |
| --- | --- | --- | --- |
| Combined | 170,108 | 170,293 | +185 |
| Core | 104,691 | 104,876 | +185 |
| Active core | 61,526 | 61,711 | +185 |
| `layerfs-daemon` | 2,650 | 2,835 | +185 |
| Root reference | 65,417 | 65,417 | 0 |
| Excluded predecessors / integration | 40,321 / 2,844 | 40,321 / 2,844 | 0 |
| Content | 18,613 | 18,613 | 0 |

The growth is new product behavior: the family receipt, the typed completion
cell, explicit stage charging and the fixed lane table. Nothing was relocated
or retired.
