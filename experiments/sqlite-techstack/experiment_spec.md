# SQLite technology-selection experiment specification

Status: diagnostic experiment only

Worktree: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-sqlite-techstack-experiment`

Branch: `experiment/sqlite-techstack`
Pinned base commit: `67e3d77df3509d6d219c6dae78c2a537a3831e27`

This experiment decides whether the next LayerFS storage implementation should
use SQLite directly or SQLite plus immutable carrier files. It does not change
the production LayerFS format, authorize a merge, close a milestone, qualify a
provider, or create L1.5.5 benchmark/custody evidence.

## 1. Decision to make

Compare three eligible candidates under the same data, durability boundary,
operation semantics, host, and measurement code:

| ID | Candidate | Object payload storage | Role |
|---|---|---|---|
| `T-SQL` | TypeScript + SQLite | SQLite BLOBs | Language/runtime control |
| `R-SQL` | Rust + SQLite | SQLite BLOBs | Primary simple candidate |
| `R-HYBRID` | Rust + SQLite catalog + immutable carriers | External carrier files addressed by SQLite rows | Conditional candidate |
| `R-FS` | Current Rust filesystem-locator engine | Existing carriers plus per-object filesystem locators | Historical anchor only; cannot win |

`R-HYBRID` is not built until the `R-SQL` measurements satisfy the trigger in
Section 12. `R-FS` receives at most one common-harness calibration run and must
not consume implementation or tuning time in this experiment.

## 2. Questions the experiment must answer

1. Does TypeScript add material cost once both implementations use the same
   SQLite schema, transaction boundaries, fixture, and operation semantics?
2. Can `R-SQL` meet the Create, read, edit, and recovery targets without an
   external carrier layer?
3. If not, is SQLite BLOB/WAL/checkpoint work the measured bottleneck, and does
   `R-HYBRID` remove at least 20% from a primary metric without weakening reads,
   edits, recovery, or bounded memory?
4. Which candidate is the smallest design that meets the requirements?

## 3. Non-goals

Do not add or measure:

- FUSE, macFUSE, OverlayFS, a mount, projection, or a native workspace driver;
- network, Cloudflare, a remote provider, multi-host coordination, or Docker;
- the production L1.5.5 locator/catalog format or a production migration;
- GC, compaction, physical deletion of unreachable immutable objects, or
  recursive native-directory deletion;
- a claim of OS-cache-cold behavior without verified cache eviction;
- cross-platform qualification; or
- a full LayerFS verification wall during the edit loop.

Logical directory `Remove` is in scope. Native materialization and physical
space reclamation are not.

## 4. Fixed host and filesystem

All result-bearing runs use the same machine and direct APFS path:

| Field | Frozen value |
|---|---|
| Host | MacBook Pro `Mac15,10` |
| CPU | Apple M3 Max, 14 cores (10 performance + 4 efficiency) |
| Memory | 36 GB |
| OS | macOS 26.4.1, build 25E253 |
| Kernel/architecture | Darwin 25.4.0, `arm64` |
| Filesystem | Internal APFS-backed SSD under `/System/Volumes/Data` |
| Rust | `rustc 1.96.0`, `cargo 1.96.0` |
| Node.js | `22.23.1` |
| SQLite | `3.51.0` |
| Power | AC power; record low-power mode and thermal state |

The result record must capture free space, source commit, diff hash, runtime
versions, and background-load policy again for every campaign. No other Cargo
build, benchmark, database maintenance, or storage experiment may run at the
same time.

Use these labels instead of the words `cold` and `warm`:

- `apfs_direct_fresh_namespace`;
- `apfs_direct_reopened_namespace`;
- `apfs_direct_same_process_repeat`; and
- `equal_incumbent`.

## 5. Shared semantic contract

Every eligible candidate must implement the same observable operation contract.
An implementation that omits a required check may be useful as a diagnostic
calibration, but it is ineligible for selection.

### 5.1 Common content rules

- Consume the source exactly once through the frozen FastCDC profile.
- Hash canonical object bytes with BLAKE3 in the same typed domains.
- Produce the same ordered chunk/object manifest and final logical digest.
- A repeated object ID is reusable only after exact canonical-byte equality.
- A same-ID/different-bytes occupant fails closed.
- Reads and edits are bounded streams; no source-sized or workspace-sized RAM
  staging is allowed.
- A logical edit or Remove publishes one new durable root generation.

The harness has two lanes:

1. `storage_only`: consume a precomputed canonical object batch. This isolates
   SQLite/carrier storage mechanics and excludes CDC/object construction.
2. `end_to_end`: consume source bytes, run CDC/object construction, store/reuse
   objects, publish the durable root, reopen it, and verify the logical digest.

Only `end_to_end` may be reported as Create.

### 5.2 Durable transaction boundary

Create/edit/Remove operation time starts at request entry and stops only after:

1. every required payload/object is durable under the candidate protocol;
2. the new root is atomically committed and visible;
3. the operation has returned a reopenable root identity; and
4. an immediate reopen verifies the committed root and logical digest.

`T-SQL` and `R-SQL` use one SQLite transaction for object rows, root metadata,
and the head-generation update. `R-HYBRID` writes and synchronizes a carrier,
installs it at its final relative path, then commits its catalog rows and root
in SQLite. A crash may leave an unreferenced carrier, but never a visible
partial root; recovery must identify and remove or quarantine that orphan.

Checkpoint time is measured separately. It is not silently removed from total
system cost, and no candidate receives a weaker durability setting.

### 5.3 Recovery contract

After a clean close or an injected process termination at each transaction
boundary, reopen must produce exactly one of:

- the previously committed root; or
- the complete newly committed root.

It must never expose a partial root, missing referenced payload, foreign bytes,
ambiguous object binding, or an unreported residue. Recovery failures remain
typed and explicit.

## 6. Frozen SQLite configuration

Apply these settings before schema creation where SQLite requires it:

```sql
PRAGMA journal_mode = WAL;
PRAGMA synchronous = FULL;
PRAGMA page_size = 4096;
PRAGMA mmap_size = 0;
PRAGMA cache_size = -65536;
PRAGMA wal_autocheckpoint = 0;
```

The 64 MiB SQLite page cache is part of candidate memory cost. Report WAL and
checkpoint bytes/time directly. Do not tune candidates independently until the
first paired campaign is complete.

## 7. Minimum candidate shape

Keep the implementations intentionally small. Both SQL candidates use the same
three logical tables: immutable objects, immutable roots/manifests, and a
single current-head row containing root ID plus generation. Use database
constraints for uniqueness and referential integrity.

`R-HYBRID` adds only immutable carrier metadata and replaces the object BLOB
with `(carrier_id, offset, length, checksum)`. It must use sequential bounded
carrier I/O. Do not add a general provider interface, background service,
cache hierarchy, or migration framework.

The TypeScript and Rust candidates must share fixture bytes, expected hashes,
SQL schema text, SQLite pragmas, and JSON result schema. Language-specific
wrappers may differ; semantics and timing boundaries may not.

## 8. Fixed fixtures

Generate each fixture deterministically and record its BLAKE3 digest:

| Fixture | Development size | Final size | Purpose |
|---|---:|---:|---|
| `random` | 10 MiB | 100 MiB | Low-dedup sequential payload |
| `duplicate_heavy` | 10 MiB | 100 MiB | Dedup/reuse behavior |
| `large_edit_base` | 10 MiB | 100 MiB | Start/middle/end and repeated edits |
| `many_files` | 100 × 100 KiB | 100 × 1 MiB | Metadata/object-count pressure |
| `directory` | 10,000 entries | 25,600 entries | Logical Remove/locality |

Use a fixed seed and generator version. The development loop uses 10 MiB. Run
100 MiB only for the selected candidate and, if necessary, one runner-up.

## 9. Workload matrix

| ID | Lane | Workload | Primary result |
|---|---|---|---|
| `CAL-APFS` | calibration | Durable sequential file write on direct APFS | MiB/s and sync time |
| `CAL-SQL` | calibration | One SQLite BLOB in one FULL transaction | MiB/s, WAL bytes, checkpoint time |
| `S1` | storage only | Store precomputed canonical random objects | MiB/s and write amplification |
| `S2` | storage only | Reuse the same precomputed objects | elapsed time and bytes written |
| `C1` | end to end | Fresh random Create | durable Create MiB/s |
| `C2` | end to end | Fresh duplicate-heavy Create | durable Create MiB/s and reuse |
| `C3` | end to end | Equal-content Create | elapsed time and new bytes |
| `R1` | end to end | Reopened full sequential read | MiB/s |
| `R2` | end to end | Same-process repeat full read | MiB/s |
| `R3` | end to end | 1,000 4 KiB random reads | p50/p95/p99 latency |
| `R4` | end to end | 1,000 64 KiB random reads | p50/p95/p99 latency |
| `X1` | end to end | Extract one file to a new direct-APFS file | MiB/s, including destination durability |
| `X2` | end to end | Extract `many_files` | files/s and MiB/s |
| `E1` | end to end | Three one-byte edits: start/middle/end | p50 and total durable latency |
| `E2` | end to end | 100 sequential one-byte edits | p50/p95 and total time |
| `E3` | end to end | 50 scattered edits | total time and bytes written |
| `W1` | end to end | Mixed workspace fresh/reopened | elapsed time |
| `D1` | end to end | Remove one directory entry at head/middle/tail | p50/p95 and bytes written |
| `REC1` | recovery | Clean close and reopen | exact root/digest |
| `REC2` | recovery | Injected termination at transaction boundaries | old-or-new atomicity |

The final campaign raises random-read operations to 10,000 and scattered edits
to 500 only after the 10 MiB decision campaign is stable.

## 10. Measurements

### 10.1 Headline metrics

- durable Create throughput and elapsed time;
- equal-content Create latency;
- reopened and same-process full-read throughput;
- 4 KiB and 64 KiB random-read p50/p95/p99;
- end-to-end small-edit p50/p95 and total time;
- extraction throughput;
- logical Remove latency;
- clean/injected recovery results;
- user and system CPU time;
- peak RSS;
- physical bytes written/read and write amplification;
- database, WAL, carrier, and filesystem namespace growth.

Throughput is `logical_mib / operation_seconds`. Peak RSS is an observed host
value, never the LayerFS logical memory ledger. An unavailable counter remains
`null`/`unavailable`, never numeric zero.

### 10.2 Per-operation counters

Every retained raw sample records:

```text
candidate, source_commit, source_diff_hash, run_id, host_id,
runtime_versions, fixture_id, fixture_digest, namespace_label, operation,
logical_bytes, elapsed_ns, throughput_mib_s, user_cpu_ns, system_cpu_ns,
peak_rss_bytes, objects_requested, objects_new, objects_reused,
database_bytes_before, database_bytes_after, wal_bytes_before, wal_bytes_after,
sqlite_prepare_calls, sqlite_step_calls, sqlite_rows_inserted,
sqlite_rows_read, sqlite_transactions, sqlite_checkpoints, checkpoint_ns,
filesystem_open_calls, filesystem_stat_calls, filesystem_link_calls,
filesystem_rename_calls, filesystem_unlink_calls, filesystem_sync_calls,
bytes_read, bytes_written, bytes_hashed, cdc_input_bytes, cdc_passes,
carrier_count, carrier_bytes_read, carrier_bytes_written, carrier_syncs,
final_logical_digest, reopen_verified, residue_count, terminal
```

Fields that do not apply to a candidate use `not_applicable`; fields that the
host cannot observe use `unavailable`.

## 11. Trial protocol

### 11.1 Fast edit loop

For one changed candidate:

1. run its smallest semantic self-check;
2. run one 10 MiB result-shaped case;
3. run its formatter/type/check command; and
4. append the result or rejection to `experiment.md`.

Do not run all candidates or the 100 MiB case after every edit.

### 11.2 Paired decision campaign

- Release/optimized builds only; compilation is excluded from operation time.
- One unrecorded warm-up, then five retained samples per candidate/workload.
- Rotate candidate order (`A/B`, then `B/A`) to reduce cache and thermal bias.
- Use a fresh namespace for every fresh-Create sample.
- Record median, p25, p75, minimum, and maximum. Small operations additionally
  report p50/p95/p99 over at least 100 measurements.
- Reject a sample if another build/benchmark ran concurrently, the fixture or
  source fingerprint changed, the host throttled, or correctness did not pass.

### 11.3 Final campaign

Run 100 MiB only after the 10 MiB decision is frozen. Use one warm-up and five
retained samples on fixed source fingerprints. No code changes are allowed
between the final candidate runs.

## 12. Decisions and stop/go rules

### 12.1 TypeScript versus Rust

Select `R-SQL` when it is within 10% of `T-SQL` on durable Create and does not
regress any primary read/edit metric by more than 10%. Prefer Rust on a tie
because it is the production LayerFS language and avoids a second runtime.

Select `T-SQL` only if it wins a primary metric by more than 10%, the result is
reproducible under the paired protocol, and the advantage survives inspection
of identical SQLite statement counts, transaction boundaries, and semantics.

### 12.2 Whether to build `R-HYBRID`

Build `R-HYBRID` only if both are true:

1. `R-SQL` misses the frozen target or SQLite BLOB/WAL/checkpoint work accounts
   for at least 25% of end-to-end durable Create/read time or amplification; and
2. the `CAL-SQL`/counter evidence predicts that external sequential carriers can
   remove the measured cost rather than merely move it.

Retain `R-HYBRID` only if it improves at least one primary metric by 20% or more
over `R-SQL`, regresses no other primary read/edit metric by more than 10%, uses
bounded memory, and passes the same recovery contract. Otherwise choose the
simpler `R-SQL` design.

### 12.3 Overall target

The performance target is at least 100 MiB/s for final 100 MiB durable Create.
It is a target, not a pass fabricated by changing the operation boundary. If no
candidate reaches it, select the smallest correct candidate and record the
remaining measured bottleneck before authorizing another optimization.

## 13. Invalid-result rules

A result is invalid if it:

- times only CDC, object staging, SQLite insertion, or carrier writing and calls
  that number Create;
- excludes required commit/sync/reopen verification from the primary boundary;
- changes fixture bytes, CDC profile, object IDs, digest, or durability;
- uses FUSE, a memory-only database, `synchronous != FULL`, or hidden fallback;
- reports reopened/same-process data as OS-cache-cold;
- substitutes logical memory accounting for RSS;
- leaves an unreported WAL, temp file, orphan carrier, partial root, or residue;
- changes source between paired runs without a new campaign; or
- reports a microbenchmark as an end-to-end result.

## 14. Execution checklist

- [ ] Record environment and fixture custody.
- [ ] Implement the common result schema and semantic self-check.
- [ ] Run `CAL-APFS` and `CAL-SQL`.
- [ ] Implement minimal `T-SQL`.
- [ ] Implement minimal `R-SQL` with identical schema/transactions.
- [ ] Run the paired 10 MiB matrix.
- [ ] Select the language/runtime candidate.
- [ ] Decide whether the `R-HYBRID` trigger is met.
- [ ] If triggered, implement and compare only `R-HYBRID`.
- [ ] Freeze one winner and, if needed, one runner-up.
- [ ] Run the 100 MiB final campaign.
- [ ] Record the decision and unresolved measured bottlenecks.
