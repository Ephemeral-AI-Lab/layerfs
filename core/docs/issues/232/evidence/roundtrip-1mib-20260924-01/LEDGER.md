# #232 1 MiB Exec/FUSE round-trip ledger (append-only)

Each entry records counts, identities, the command, the arithmetic and every
non-passing line. `FAIL`, `INELIGIBLE` and `TARGET_MISS` are reported as
plainly as `PASS`. Wall times here are single retained observations; the ones
marked *diagnostic* are never promoted and never compared.

Scope for this ledger: the frozen crossing budget of
`overwrite-middle-4k-on-1mib-ops-1-exec-v2`, the levers R1, R2 and R3, and the
instruments D3 and D1. It does not touch the #232 baseline report, the
registry, the targets or the frozen v2 receipts.

---

## L1 — Instruments: projection byte totals and request-size histogram (D3)

Change: `ProjectionCounters` records request and returned byte totals for read
and write plus an 18-bucket power-of-two histogram of the request sizes; the
FUSE adapter records them at its data callbacks; the daemon maps them onto the
status wire; public `WorkspaceApi::status` returns them.

Counts before: the status route reported callback classes only. Counts after,
on the measured case: `read_request=0 read_returned=0 write_request=4096
write_returned=4096`; both histograms populated; the write lands in the
`2k-4k` bucket, which is the declared 4 KiB replacement.

**Verdict: worked.** The instrument answers the question it was built for, and
it answers a second one for free: the 1 MiB overwrite performs no base read at
all, so R3's term in this row is zero by construction.

Audit carried out with the same instrument, and the defect it found:

* the daemon compared the workspace's combined 36-row histogram against the 18
  declared buckets, failed closed, and the status route returned `Integrity` for
  **every** mounted workspace. Nothing in the crossing path depended on it, so
  it changed no number in this ledger; it is fixed, with an external test that
  covers the combined list, both directions, a short list and an unknown label.

## L2 — Instrument: the daemon's own log stream (D1)

Change: the harness attaches `docker logs --follow` for the sandbox's life and
retains it as `daemon.log` beside the sample.

Counts before: the daemon's `role=2` LFT1 records never reached the host — the
sandbox launch consumed the daemon's first output and later records went to a
container log nobody read. Counts after: the sample carries the daemon's own
operation records, including `daemon.exec_spawn` / `daemon.exec_output` for the
measured Exec.

**Verdict: worked**, and it is the instrument that diagnosed L4.

* Diagnostic limit: the stream starts when the sandbox appears and `--since`
  replays what the daemon already wrote, so no record is lost to the attach. It
  is a harness instrument, not a product change, and it is outside every timer.

## L3 — Lever R3: projection requests per declared block

Counts in this row: `read=0`, so no projection read request exists to collapse.
`write=1` for one declared 4 KiB replacement.

**Verdict: not testable here; no claim made.** R3 must be measured on a shift
case, where the frozen row already shows 7-8 projection requests for 4 declared
128 KiB blocks and `FUSE read == upstream ReadFile` 1:1.

## L4 — Lever R1: merge the content and metadata save

Change: `Operation::EditFileWithMetadata` carries the edit and the portable
fields it stamps under one save owner; the workspace uses it for every published
file and keeps the separate metadata request only for a fresh file.

Counts before: 4 upstream ops in the window. Counts after (diagnostic, four
identities): **2** — `EditFileWithMetadata ×1` and `HistoryCommand ×1`; the
driver's own `upstream_calls` counter reports 2 against the baseline row's 4.

Daemon side, from the retained log: one save, `service.begin_save` 1.9 /
`service.edit` 1.8 / `service.finish` 1.9 ms, building a content root and a
metadata root.

**Verdict: FAILED, withdrawn unshipped.** The reply never reached the client:
the daemon encoded and wrote the 90-byte `Response::Saved` with its metadata
root and the client never observed it, so the Commit waited out the five-second
silent-wait window and failed with an unknown outcome. Reproduced at four
identities, and independently of whether the metadata base named the published
sibling root or the content root. Nothing about the failure is a wall claim; it
is a broken fast path and was reverted rather than kept.

What the attempt still establishes: R1's real size is **one crossing**, plus the
second save's `begin_save` and `finish`. The handoff's 2.6 ms estimate assumed
the whole metadata operation's cost disappears; it does not, because the
metadata objects and their role validation move into the first save.

## L5 — Lever R2: the baseline refresh

Findings, no change made: `serial_original`
(`core/crates/layerfs-workspace/src/filesystem/original.rs:88`) runs the
`Inspect` only when the node's baseline is stale, and it exists to learn the
published content root that `EditFile` must name as its base. A node resolved
from the published base is already canonical and takes the cached branch.

**Verdict: not attempted; it is structural.** Removing the crossing changes
where the published base root comes from, which is a correctness question, and
the frozen contract requires the canonical result to be proved by the oracle
before anything else. Recorded as an open lever, not as a win.

## L6 — Final sample at the post-revert identity

One sample, `--verification skipped` during collection, separate `verify-edit`
afterwards: **verification PASS**, `full-file` coverage, observed digest equals
the declared one, canonical root differs from the pristine root.

Counts: `upstream_calls 4`; window ops `Inspect ×1, EditFile ×1,
UpdatePortableMetadata ×1, HistoryCommand ×1`; crossings 6 of a 4 floor.
`edit_commit_ns` 35.23 ms against a `g2_target_ms` of 5.63 —
**`TARGET_MISS` as recorded**; the row stays `INELIGIBLE` under the frozen cache
contract and is not a PASS. The target was not moved and this sample is not
pooled with G2.

## L7 — Stop reason

Stop condition: the crossing budget is not met. Four of the six levers the
handoff named were addressed (D3, D1, R1, R3) and one (R2) was characterised as
structural; of the three performance levers, none moved a crossing in this row.
The remaining named terms are `EditFile` at 6.29 ms and the sandbox share of
`edit` at 13.23 ms, neither with a measured cause, so the next step is more
instrumentation rather than another product change — see NOTE.md §9.

The budget was not met because R1, the one lever that does reduce this row's
crossings, cannot be delivered without first understanding why a written
response is not observed, and that question is outside the window this round
had. That is the stop reason, and it is reported as a miss.
