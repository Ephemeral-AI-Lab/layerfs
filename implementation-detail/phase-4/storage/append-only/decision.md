# Phase 4A decision record

Status: **OPEN / NO-GO for Phase 4B promotion**

Date: 2026-08-17

## Decision

Keep the Phase 4A Rust + SQLite engine as the reference implementation. The
append-only carrier is an exploratory, engine-private Phase 4B candidate only;
its public types remain explicitly marked as not authorized for production
promotion. No Phase 4A-to-Phase 4B A/B decision has been made because the
required same-source, same-profile, same-APFS comparison is not present.

## Evidence reviewed

The historical SQLite experiment at
`/Users/yifanxu/Ephemeral-AI-Lab/layerfs-sqlite-techstack-experiment` records:

- durable Rust + SQLite create time growing from 6.812 ms for 1 MiB to 459.173
  ms for 100 MiB, a 67.4x increase for a 100x size increase;
- a reuse/edit run with 6,399 reused objects out of 6,400 requested still
  issuing 19,207 logical statement calls, reading 19,203 rows, and inserting
  6,403 rows;
- timing that mixed source processing with the SQLite commit path, so it did
  not isolate pure durable commit cost.

Those facts justify measuring the append-only hypothesis. They do not prove
that SQLite BLOB, journal, pager, or statement work is the dominant material
cost for this repository's current Phase 4A workload. The candidate therefore
must not replace Phase 4A until a clean A/B ledger proves that point.

## Candidate evidence available

The corrected benchmark is a file-streamed single-pass diagnostic. The exact
run is recorded in `PHASE_4B_BENCHMARK_REPORT.jsonl` and was executed with:

```text
cargo run --release -q -p layerfs-engine --bin phase4b_benchmark -- /tmp/layerfs-phase4b-final-100m.log 100
```

The run measured CDC, callback, encoding, harness hashing, engine put,
carrier append/flush, index lookup/page work, marker capture digest, commit
publication, sync, reopen scan, and reopen reads separately. It reports a
single-run wall time and explicitly reports CPU, RSS/PSS, medians, spread,
actual syscall counts, cold APFS state, and SQLite pager/VFS/journal fields as
unavailable or not applicable where they were not measured.

The selected 100 MiB diagnostic consumed exactly 104,857,600 input bytes and
produced 106,327,544 carrier bytes: 1,469,944 bytes overhead, a
carrier/input ratio of `1.014018478394` (1.401847839% overhead). Its source
fingerprint is
`4ba6dba1d1f4383b6c3f484411a7aa74039749579e9225051abec1669e8b8faf`. The
command was the release-profile invocation shown above; the source was read
from a generated file in one bounded pass, and the OS/APFS cache state was
`warm_or_unknown_not_cold_apfs`. The working tree was uncommitted, so the
candidate build is identified by the base commit and source-file hash manifest
recorded below rather than by a release version.

Candidate build identity for this handoff:

```text
base_commit=f71d53b5c5ca42fc7a45010816d2a3093580f5d3
candidate_source_hash_manifest=recorded in PHASE_4B_BENCHMARK_REPORT.jsonl
build_profile=release
```

This one run does not qualify CPU/RSS, median/spread, cold-APFS behavior,
Phase 4A A/B, or P4B-C1. It must not be compared as a speed claim with the
historical Rust + SQLite or memory measurements.

There is an additional workload-fairness blocker. The current benchmark
streams CDC chunks into the carrier but commits a root containing only the
empty-directory object and a fixed benchmark delta. It therefore measures
scanner/admission and carrier behavior, not the same logical source graph as
the historical SQLite experiment. Its generated LCG source is also not the
historical xorshift fixture, and its wall timing includes source generation
while the historical timing did not. The current rows are consequently
excluded from Phase 4A comparison.

## Scope decisions

- `fs2 = 0.4.3` is already present in the lockfile and is used only for the
  nonblocking advisory carrier lock. The standard library has no portable
  advisory file-lock API. Contention maps to typed `CarrierBusy`; there are no
  retries, workers, or hidden queues.
- The current public append-only open is writer-capable and takes the exclusive
  lock even for inspection. A separate read-only open path is deferred;
  P4B-C1 (one writer with bounded readers) is **NOT QUALIFIED**.
- The fixed 256-bucket root plus immutable collision pages is retained instead
  of the proposed page/B-tree design for this smallest candidate. The full
  object ID is already available, object IDs are immutable, the root is a
  fixed 2 KiB frame, and the design avoids split/parent-version state and a
  second publication protocol. Traversal is bounded and observable. A B-tree
  requires a measured index-I/O win before it is justified.
- The carrier has one 64 KiB `BufWriter`, tracked physical/persisted ends, one
  append stream, and no source-sized or full-index staging. The benchmark's
  262,157-byte peak is a declared buffer-capacity upper bound, not RSS.
- A marker binds the format/profile, capture range and digest, all referenced
  index/root/delta frames, and the recursive authenticated object closure,
  with every locator bounded by the selected marker's visible end.

The measured follow-up remains bounded and exploratory. The next measured
targets are a receipt/pass reuse tied to an immutable locator and visible
generation, followed only if justified by counters by a page-local packed
collision index. Neither is implemented here: ingest still records 55,240
index-page reads for 5,363 lookups, and reopen still rereads 427,887,475
carrier bytes for a 106,327,544-byte carrier (4.024239x). These are follow-up
work items, not permission to add an unbounded map or claim speed.

## Exit criteria for a future decision

Do not promote until a new record contains the Phase 4A baseline and Phase 4B
rows on the same clean source fingerprints, at least three measured iterations
with median/spread, durability-equivalent work, direct SQLite statement/BLOB/
journal/pager fields or an explicit measurement limitation, and a decision on
P4B-C1. Before that A/B, both engines must consume one pre-generated source
file and fingerprint, publish a complete source-referencing root/member graph,
and perform equivalent full logical reopen verification with source generation
outside both timed boundaries. Until then, this record remains NO-GO.
