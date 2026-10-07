# Captured file normalization and indexed reference reduction

> Status: implementation checkpoint following local first parent
> `71a3a24b8f25fbb5cb5821c554c050be5e6a1182`. Captured product tree
> `d97312c619f0726ff863e05b9dbe450759cce0e1`; final committed verification
> follows this prepared checkpoint. S0/S7–S13 remain incomplete.

This continues the [full goal](IMPLEMENTATION-PLAN-S7-S13-20261007.md).
[Append-only receipts](checks/captured-reducer-20261007) retain exact commands,
source-scoped observations, failed attempts, repairs and unrun scope. There is
no performance treatment or new E1 sample; numerical/resource qualification
remains NOT_EVALUATED. Earlier eight Init failures, six history pairs and E01
consistency receipts retain their original identity and verdicts.

## Implemented behavior

[Captured file normalization](../../architecture/56-captured-file-normalization.md)
derives one exact Workspace/reader/operation binding, authenticated immutable
serial and cheap length, and retains one FileView classification. It accepts no
caller root/size. Only logical PathNotFound establishes base absence. Missing,
denied, malformed, identity and unavailable length facts end preparation.
Coalesced final Data/Zero changes and a separate trailing size edit are sealed
as numbered 25-byte records in the existing operation database. Inherited bytes
below authenticated EOF remain retained; inherited Gap/Window mask bytes above
it become zero. There is no chronological replay, edit Vec or second tree.

The consuming adapter shares one short-borrow record/failure owner with the
sequence, fallible source and backed editor. One forward cursor skips retained
gaps without rescanning its metadata prefix. One planned backwards transition
is allowed for comparison followed by construction; failure permits no reset.
Actual returned data/mask/raw-vector capacities are checked before retention or
decode. Continue must advance the opaque metadata cursor monotonically rather
than merely claim a row. No iteration or smaller total file/edit limit is added.
Original reader, operation, provider, record and accepted-consumer custody remain
with the actual owners; Drop releases none. Reads after logical close remain
separate from required scratch mutations, which retain original Closed refusal.

The additive [fallible source and borrowed-view API](../../architecture/54-run-aware-localized-file-edits.md)
uses the same Content driver as legacy byte memory/backed/Whole/Empty/streamed
paths. It validates policy-derived capacities and base length, propagates indexed
length errors before boundary effects and does not reopen/classify its root.
Legacy length-query counts and first-failure ordering remain. The generic view is
no authority/revocation/topology token; its caller retains the owning context.

[Indexed filesystem reduction/release](../../architecture/53-backed-filesystem-serial-state.md)
uses the same Row96 grammar and arithmetic/final derivation, separate fresh and
touched records, sealed seed membership, complete-key windows, FIFO/node/frame
records and coupled child decrement/touch/work/name-progress guards. Release may
introduce lower serials; final enumeration restarts from None. The old explicit
resident route remains. Exact rebuilt/base provenance selects the caller's
same-Save accepted reader, with no failed-read substitute. One Store Save producer
and the actual authorized provider/fences are still integration obligations.

Two inherited correctness issues are repaired on both routes: positive reference
count overflow now returns LengthOverflow rather than zero/wrap, with negative
clamping unchanged; final-row errors pass directly into the fallible inode merge
rather than normal EOF followed by delayed failure. Earlier accepted children
remain retained, with no later demand/acceptance or final root after that error.

The [non-destructive raw key cursor](../../architecture/49-indexed-operation-scratch.md)
uses the exact namespace/operation/file-scope/kind prefix and full BLOB32 exclusive
seek, ORDER BY key LIMIT 64. None includes zero. Older providers explicitly refuse
this capability; first-key consuming queues retain their own meaning. Owner jobs
charge/copy the bounded reply and retain original attempted/unattempted causes.
No new database, schema, dependency, canonical algorithm or provider/producer lane
is added. Names and context markers establish no full-root qualification.

## Verification and retained failures

Locked no-run precedes every test selection. Normal construction has
`LAYERFS_CONSTRUCTION_WORKERS=1`. Every test command has an explicit outer wall
ceiling at most 120 s; Docker also has child 110 s/kill-after 1 s. No hang occurred.
These are public functional checks from an uncontrolled cache, not numerical
speed/storage, cold, physical-I/O or aggregate-residency proofs.

[Distinct totals](checks/captured-reducer-20261007/functional-totals.json) are
408 host bodies and 373 Linux bodies: Content 287, Overlay 8, Workspace 50 and
Daemon 25 each, plus SDK 38 host/3 Linux. The other 35 SDK bodies are macOS-only.
After the terminal-size comparison lint rewrite, matching builds 26/28 and
[host 27](checks/captured-reducer-20261007/27-final-captured-bodies.json)/
[Linux 29](checks/captured-reducer-20261007/29-linux-final-captured-bodies.json)
recheck only the affected fourteen captured cases. Repeated bodies do not increase
the distinct totals. Unchanged component/family evidence retains its source pin.

[Host Clippy 24](checks/captured-reducer-20261007/24-host-clippy-comparison.json)
and [Linux Clippy 25](checks/captured-reducer-20261007/25-linux-clippy.json) pass
all targets of the five affected packages. The
[native proof 31](checks/captured-reducer-20261007/native-proof-summary.json)
passes both persistence profiles in 4,293,307,833 ns with fixed Docker 40 s,
host 45 s and outer 120 s limits. The
[new consumer binary](checks/captured-reducer-20261007/consumer-binary.json)
is 72,916,528 bytes, SHA-256
`17a6a58c48b4acabb078734089d470b1d4646b065c72a4bacffc5b7c4e014aa9`.
Source removal, original RemoteRefusal, thirteen native paths/seven regular names
(six regular inodes), permissions/raw symlinks/hardlink identity, idle reclamation,
Gone cleanup and zero live messages/transport credits are checked. This does not
exercise Commit, qualify numerical performance, or replace full-root/P14 evidence.
[Format 32](checks/captured-reducer-20261007/32-format-test-wrapping.json),
[source guard 33](checks/captured-reducer-20261007/33-boundary.json) over 736
production Rust/SQL files and [tool tests 34](checks/captured-reducer-20261007/34-boundary-selftests.json)
(41 bodies) pass. [Changed-source caps](checks/captured-reducer-20261007/source-line-caps-final.json)
cover 42 files, maximum production 807 and thin entry 61 physical lines.
The retained initial format failure 32 is repaired by wrapping one external
assertion; the AST/product behavior is unchanged and earlier test evidence
retains its source scope. An empty source guard is not semantic qualification. Local-link verification
passes across 18 documents, 302 local links and 13 anchors.

Failure 07 is one external EditCounters export-path error, corrected to the
existing file-module API without product change. Failure 12 is 24 diagnostics from
six external Timing closures returning Attempt instead of Result; outer Ok and
outer unwrap preserve the inner result/custody, with no Timing/API change.
Failure 16 retains Daemon 3 and captured 13 passes and one fixture failure: a second
Capture after reader release correctly refuses CaptureInFlight. Both files were
already in the same snapshot. The repaired body constructs both under one retained
reader/operation with disjoint file scopes, then releases shared custody once.
Only that failed body is rerun after matching build 17; unchanged passes are reused.
No product, cache, operation deadline or workload-byte change hides the failure.
Failure 23 is Clippy comparison_chain at the terminal-size branch; a narrow
Less/Greater/Equal match preserves delete/insert arithmetic, equal no edit and
original push errors. Receipts 24 and 27–29 confirm the affected source repair.

Independent source review found and repaired the inherited overflow/error-to-EOF
cases and the new adapter's inherited EOF, actual capacity and continuation
boundaries. Fourteen real-Owner bodies include new/growth/no-op, overlapping
writes/shrink/regrow/cross-cell, >4 GiB hole, 129 edits, install/current-binding rebind,
close, context/missing-record corruption, original denied/missing/identity/length/
reader/guard/consumer causes and malformed bounded-unit negatives. Their byte and
localized canonical oracles are independent; existing chunked results are not
claimed equal to a complete fresh scan with different CDC boundaries.

## Remaining goal and source size

Captured directory header/count/name points and typed namespace values remain
next work. Validator grouped/addition/alias/cycle state and whole-base walks,
complete immutable-root/changed-parent qualification, same-Save production,
Stage/history/install/unknown disposition and native application/kernel acceptance
remain. The direct-input fresh-child-under-a-released-rebuilt-parent refusal is
retained explicitly; normalized final namespace state still needs classification.
rows_touched is resident insertion/reload or indexed raw creation; peak_depth is
release-frame occupancy including sibling seeds, not namespace path depth or heap.

[Exact production LOC](checks/captured-reducer-20261007/production-loc-source.json)
compares first parent with the prepared staged product: **167676 → 170041
(delta +2365)**. Core is 102259 → 104624 (+2365); reference remains 65417.
[Manifest classification](checks/captured-reducer-20261007/production-loc-classification.json)
records active core 59094 → 61459 (+2365), excluded predecessor 40321 and excluded
integration 2844 unchanged. Counter `tools/production_loc.py`, SHA-256
`c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb`,
counts nonblank/non-comment first-party Rust/runtime SQL from `git archive TREE
crates core/crates`; tests, inline test-only code, tools, docs and third-party
code are excluded identically. Final staged/committed scoped-tree equality and
counts are confirmed before/after the commit. This is source growth for additive
construction/normalization behavior; no relocation, duplication or retirement is
credited as algorithmic simplification. There is no push, release, early reference
retirement or milestone completion.
The full goal continues after the component checkpoint. The immediate next
work reconciles and closes named S7/S9 exit requirements. Independent engine
E1–E4 evidence precedes the exact native request/open/lookup/reply dependency
on S8; that dependency remains explicit. Later milestone work retains its owning
acceptance criteria and is advanced only when required by a named S7/S9 exit.
