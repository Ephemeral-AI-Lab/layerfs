# H11 retained-control preregistration v3

Status: **FROZEN BEFORE THE V3 SCREEN OR GATE**. One unretained revision-1 child was used only to validate the v3 parser/report/Q-terminal protocol before this freeze. V1 and v2 source, evidence, and result roots remain byte-for-byte historical custody. V2 remains `H11_REVISE_EXACT_BLOCKER`.

## Scope and unchanged mechanism

V3 repairs H11 evidence authority only. The LayerFS CAS + FastCDC + K64/F64 + SQLite algorithm, canonical bytes, schema, receipt, transaction/COMMIT shape, durability profile, materializer, fixture, expected-root manifest, and balanced schedule are unchanged. The only shared-source change is instrumentation that splits `Store::open` into preflight and SQLite-open/profile wall buckets without changing either path.

The v1 operation log remains preserved but is omitted from v3 execution authority and the v3 method manifest because v1/v2 never passed it to the child. V3 does not invent a redundant parser or claim that hash custody is execution.

## Accepted audit repairs

Three independent G5-0 lanes agreed on these repairs:

- use a v3-only whole-harness RAII Q ledger, separate from the retained product operation ledger;
- precharge the exact manifest String capacity and exact `1,001 * size_of::<H11Expected>()` vector capacity;
- precharge history timings as exact `capacity * size_of::<u128>()`;
- charge every reachability ID before insertion under the frozen logical rule `size_of::<ObjectId>() + 4 * size_of::<usize>() = 64 bytes`, covering the ID plus B-tree node/link overhead;
- route stored/reachable canonical bytes through requested-ObjectId validation before counting;
- charge decoded reachability vectors, temporary digest strings, and the final report;
- count the report first, reserve exactly once, combine its live capacity with persistent harness state, print it, drop it and every other owner, then emit a second terminal marker read from the actual zero ledger;
- emit selected historical `revision/root/transition/output_digest` tuples and have both analyzers compare them independently with the frozen expected manifest;
- split reopen into preflight, SQLite open/profile, H11 cache profile, and head lookup; retain the emitted SQLite counters but label them `partial-logical` because preflight and direct PRAGMA SQL are not fully instrumented;
- retain the lock descriptor, bind token/device/inode, verify ownership before release, seal the owned inode into a release attestation, and fsync both namespaces;
- fsync every referenced artifact and result directory, then perform a final independent read-only manifest verification.

Rejected: product-ledger charges that would break per-operation zero; a guessed PASS from RSS; treating `PRAGMA integrity_check` as semantic authentication; parsing the unused operation log solely to preserve a stale claim; weakening the 20-second/RSS/Q gates; changing schema/profile/format/WAL/retry/GC.

## Exact whole-harness Q model

The v3 logical ledger is independent of allocator/RSS reporting.

```text
manifest parse overlap
  = exact manifest String capacity
  + 1,001 * size_of::<H11Expected>()

persistent sample state
  = exact expected-vector capacity
  + exact history-timing capacity
  + any live historical tuple/report capacity

reachability
  = 64 bytes per admitted unique ObjectId
  + charged decoded traversal state

whole_harness_q_high_water
  = max over every instant(
      harness_q_current
      + current retained product operation q
    )

terminal q
  = exactly 0 after report, tuples, history, expectations, and all charges drop
```

The screen/gate fails if the child report and its separately emitted terminal marker disagree, the marker is missing, Q is nonzero, or any already-allocated benchmark vector is adopted without prior charge.

## Schedule and gates

Dry-run executes zero rows and verifies branch, HEAD, four controlling hashes, tracked diff, explicit untracked sources, release executable, fixture, expectations, schedule, result-root absence, and lock absence.

Screen: one worst-history `N=1,000/sample=1` child, complete lock-to-terminal wall strictly below 20 seconds. It must pass identity, historical tuples, whole Q/terminal marker, RSS `<=20,971,520`, buffer `<=1,048,576`, reachability, cleanup, custody, and owner-bound lock release.

After the screen, source is frozen and workspace tests, clippy `-D warnings`, fmt, Python compilation, diff/whitespace, and release custody run once.

Gate: exact balanced eight-child schedule
`(1,1),(10,1),(100,1),(1000,1),(1000,2),(100,2),(10,2),(1,2)`.
Complete wall through fsynced terminal verification is strictly below 20 seconds. Primary may reuse the historical analyzer as a control; the independent v3 analyzer is standalone and imports no prior analyzer.

## Hard expected mechanism values

Current-live graph is exactly `58 objects / 1,051,574 canonical bytes / 2,255 mapping bytes`. Unique-revision slopes are exactly `6 objects / 23,030 canonical bytes / 2,255 mapping bytes`; logical/apparent SQLite growth is `24,858.9069069 bytes/revision`. The prior RSS maximum is `14,057,472`; the unchanged ceiling is `20,971,520`. Operation work classes, roots, transitions, digests, transactions/COMMITs, storage, descriptor/temp/work-root cleanup, lock custody, analyzer agreement, and terminal Q are hard.

Latency uses the unchanged dual rule:

```text
candidate_sum * 100 > control_sum * 105
AND candidate_sum - control_sum >= 2,000,000 ns
```

First edit compares the non-genesis N=10 class with N=1,000. All other operations compare N=1 with N=1,000. No latency materiality rule waives any hard gate.

## Outcomes

A complete screen or gate failure is preserved as `REVISE`; source/method repair requires a new versioned attempt. A qualifying gate returns `H11_PASS_G5_C_GATE_READY` only after exact analyzer agreement, terminal manifest verification, owner-bound lock release, and final read-only verification.

