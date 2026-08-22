# Proposed G4 reconstruction and materialization contract

Status: **Round-1 proposal only — G4 remains UNSTARTED**
Authority: checkpoint `5c342f0ae24ecc69f2bfc03da1c05d1074fe956a`
Purpose: handoff to a separate preregistration/execution agent

This document does not authorize a build, candidate implementation, measured
G4 campaign, production integration, G5, or a commit. It fixes the questions,
row meanings, resource gates, and evidence rules that the later G4
preregistration must instantiate with exact source, binary, fixture, base,
methodology, and analyzer hashes.

## 1. Decision boundary

G4 must freeze two different scoreboards:

1. **authenticated logical reconstruction**: SQLite/CAS through mapping and
   canonical authentication to a logical byte sink or returned range; and
2. **native materialization**: authenticated logical or protected-seed bytes
   through a private native temporary file, data and metadata sync, atomic
   publication, directory sync, reconciliation, and cleanup.

The native scoreboard qualifies exactly **one regular file** at the engine/OS
boundary. It does not qualify directory-tree/workspace publication,
projection, VFS, SDK, or application integration. Those cells remain absent or
`NotApplicable`, never inferred from the one-file result.

The current `materialize-warm` and `materialize-fresh` benchmark operations are
logical hashing-sink reconstruction. They are not native-file rows. The G3-v13
clone/patch code is benchmark-private and is not production integration.

The campaign may qualify an exact benchmark/engine mechanism. It may not:

- silently change Canonical-v2 identities, mapping profile, schema, receipt,
  journal, synchronous, temp-store, mmap, or `cache_spill=2000` policy;
- treat fixture hashes or benchmark closure folds as product authority without
  an explicit equivalence decision;
- call a process restart, database reopen, or `F_NOCACHE` ordinary-path cold;
- promote VFS, SDK, application, daemon, persistent cache, or cross-process
  authority integration; or
- open G5 work.

## 2. Frozen inputs and preflight

The later preregistration must freeze before any row:

- branch, HEAD, status, tracked diff, untracked set, applicable instructions,
  and absence of another benchmark lock holder;
- the two distinct binary identities: retained frozen `M0-control` source set,
  executable, and static-closure hashes, plus the separately screened and
  frozen `M0-candidate` source set, executable, and static-closure hashes;
- debug-assertion status, methodology hashes, and independent analyzer hash;
- exact 1/10/100-MiB fixture manifest and source hashes;
- exact database, authority, and expectation hashes for every reusable base;
- OS/build, CPU topology, memory, filesystem/volume/device, SQLite, rusqlite,
  Rust, page size, and runtime PRAGMA observations;
- result-root nonexistence and fail-fast exclusive acquisition of the one
  repository-global lock path
  `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty/target/BENCHMARK_LOCK`; and
- a prospective chronology with no row reuse, deletion, replacement, or rerun.

`M0-control` is retained frozen G3-v13 input, not a rebuild or candidate. Its
sealed measurement identity is executable
`535bfa178a8a569ea43d9f1d23808775c2349a29f9cdacddae508391a6e5e61e`,
measured at HEAD `d79f0e0e2582d1bc491410224fec2b6cef7482e9` with the then-dirty frozen
four-file source set
`3a0330fc12cdc9b05b949a3f3f2b39f47e8d41d41234fffeedaa0ec65449058d`.
Those source bytes were committed later in clean controlling checkpoint
`5c342f0ae24ecc69f2bfc03da1c05d1074fe956a`; that later commit alone is not
the historical measurement identity.

`M0-candidate` must have a different frozen executable identity. Before the
30-row campaign, its disposable candidate build and focused 1/10-MiB
semantic/resource screen form one complete pre-implementation experiment.
That experiment must acquire the exact global lock before the build, start its
wrapper clock at acquisition, and finish build, preflight, screen, analysis,
cleanup, source restoration, and lock release within `120,000,000,000 ns`.
Only a passing screen may retain and freeze the exact candidate source-set,
executable, and static-closure hashes; the retained binary's bytes and storage
must be recorded. This screen contributes no row to the 30-row campaign.

The later measured-campaign wrapper reacquires the same global lock and starts
its clock immediately after acquisition. It uses the two already-frozen binary
identities and includes measured preflight, private base copies/preparation,
rows, row-level verification, primary analysis, independent analysis, cleanup,
storage/mode checks, payload manifest, and measured terminal verification. It
must end within `120,000,000,000 ns`. No build occurs inside this main campaign.

Workspace tests, Clippy, rustfmt, and source/static-closure manifesting are a
separate **non-measured static-validation phase** after the measured terminal.
They do not extend or rescue the 120-second measured campaign and contribute no
performance rows. Final G4 closure requires both the <=120-second measured
terminal and the separately timed static terminal. A static failure is still
`REVISE`; the split is timer attribution, not a weaker gate.

## 3. Cache, source, seed, and destination vocabulary

Every row must use exactly one value from each applicable column.

| Dimension | Allowed value | Meaning |
|---|---|---|
| process | `same-process` | Same process/connection as an explicitly recorded preceding operation |
| process | `fresh-process` | New process; says nothing about OS cache residency |
| source cache | `warm` | Exact operand was read by a recorded warm-up immediately before the measured row |
| source cache | `warm-or-unknown` | Ordinary path without a qualifying cache-residency control |
| source cache | `controlled-host-buffer-cold-approximation` | Exclusive host; all operands closed; successful `/usr/sbin/purge`; row starts immediately; device/controller cache remains `Unavailable` |
| source cache | `controlled-cold-unavailable` | Exclusive/purge preconditions failed or were not authorized; no cold latency claim |
| seed | `none` | No native seed exists or is consulted |
| seed | `same-open-protected` | G3-style read-only, no-follow, unlinked descriptor bound to current store/root/profile/epoch and operation authority |
| seed | `persistent-untrusted` | May be a lookup hint only; full authentication or rejection is mandatory |
| destination | `empty` | Target name absent before the operation |
| destination | `authenticated-prior` | Exact prior native content is covered by current operation authority |
| destination | `mutated-or-untrusted` | Complete authenticated fallback or typed rejection is required |
| route | `logical`, `first-full`, `seed-read`, `clone`, `clone-patch`, `complete-fallback`, `typed-rejection` | Exact executed mechanism |

`F_NOCACHE` is a distinct I/O policy. Apple documents it as turning caching off
for a descriptor; it does not prove eviction of pages already resident and may
not be used to label the ordinary cached route cold. The local `purge(8)`
manual describes a machine-wide approximation of initial-boot disk-buffer
conditions. Consequently, Round 1 cannot promise a controlled-cold row on a
shared interactive host. If the later exclusive-host preflight fails, the row
is retained with status `Unavailable`, not skipped or relabeled.

## 4. Compact row matrix

`S` is logical file size. A dot means no operation; `U` means a required
administrative `Unavailable` record when the cold preflight does not qualify.
One- and ten-MiB rows are single correctness/resource smokes. Each 100-MiB
`primary` in the exact chronology is also once-only. It freezes mechanism and
resource evidence under the hard wrapper budget but makes no distribution,
variance, or median claim. Any later repeated statistical campaign requires
separate authorization and cannot rewrite these rows.

### 4.1 Reconstruction scoreboard

| Row | State and boundary | 1 MiB | 10 MiB | 100 MiB | Primary result |
|---|---|:---:|:---:|:---:|---|
| R0 | accepted complete authenticated reconstruction, warm | smoke | smoke | primary | operation wall and MiB/s |
| R1 | same work, fresh process, source cache warm-or-unknown | . | . | primary | reopen/head separate from reconstruction |
| R2 | same work, controlled host-buffer-cold approximation | . | . | U/primary | classification plus wall; device-cache status remains unavailable |
| R3 | same-open protected-seed full read to a consuming sink | . | smoke | primary | read wall; seed construction/authentication reported separately and never hidden |
| R4 | authenticated returned range | . | . | protected 1-MiB probe | selected objects/bytes and returned-range wall |

R0 is the first measurement before any new optimization. It must preserve the
current complete root/transition/closure/output/ordered-occurrence evidence so
that later mechanism results have a valid control. A candidate that proposes a
different proof contract gets a separate diagnostic arm; it does not overwrite
R0.

### 4.2 Native-materialization scoreboard

| Row | State and boundary | 1 MiB | 10 MiB | 100 MiB | Primary result |
|---|---|:---:|:---:|:---:|---|
| M0-control | unchanged frozen G3 complete fallback, empty destination, warm source | smoke | smoke | diagnostic primary | current native baseline; per-object query shape and omitted proof outputs explicit; never promoted as M0 |
| M0-candidate | proof-preserving batched writer, empty destination, warm source | smoke | smoke | primary | accepted auth/folds + write + sync + publish |
| M1 | M0-candidate, empty destination, controlled host-buffer-cold approximation | . | . | U/primary | same boundary; cache limitations explicit |
| M2 | same-root protected-seed clone/no-op | . | smoke | primary | wall, clone calls, apparent/allocated change; no fabricated throughput |
| M3 | protected-seed one-byte same-size patch | . | . | primary | exact authenticated changed range and patch bytes |
| M4 | protected-seed same-size 1-MiB replacement | . | smoke | . | exact authenticated changed range and patch bytes |
| M5 | count-changing complete fallback | smoke | . | primary | complete authenticated fallback; no hidden preparation |
| M6 | invalid authority or external mutation | 2 focused | . | primary | zero unsafe reuse; complete fallback or typed rejection |
| M7 | symlink/wrong-kind, before-publication, and lost-ack faults | 3 focused | . | . | exact error precedence, old-or-new reconciliation, cleanup |

`M0-control` is measured **before** `M0-candidate` and remains the unchanged
frozen G3 complete fallback. It supplies the missing current native baseline
but is ineligible for promotion because it has a derived approximately
5,371-query S1-100 shape and omits accepted closure/occurrence outputs.
`M0-candidate` then uses the accepted batched authenticated traversal with a
bounded `Write` sink. It may not overwrite, replace, or retroactively relabel
the control. Both use private temp creation with no-follow descriptor-relative
operations, bounded writes, output verification, explicit sync policy, atomic
rename, directory sync, fresh old/new reconciliation on ambiguity, and residue
cleanup.

### 4.3 Exact one-shot chronology and wall budget

The main G4 acceptance campaign contains exactly **30 append-only JSON row
records**. Each primary is once-only; this freezes exact mechanism evidence and
does not claim a sampling distribution. The two cold slots become explicit
`Unavailable` administrative records when exclusive-host qualification fails.

| Slots | Rows |
|---:|---|
| 3 | R0 accepted warm logical reconstruction at 1/10/100 MiB |
| 1 | R1 fresh-process 100 MiB, source cache warm-or-unknown |
| 1 | R2 controlled-host-buffer-cold 100 MiB or `Unavailable` |
| 2 | R3 same-open protected-seed full read at 10/100 MiB |
| 1 | R4 protected 1-MiB returned range on S1-100 |
| 3 | M0-control unchanged G3 fallback at 1/10/100 MiB |
| 3 | M0-candidate batched writer at 1/10/100 MiB |
| 1 | M1 controlled-host-buffer-cold M0-candidate at 100 MiB or `Unavailable` |
| 2 | M2 clone/no-op at 10/100 MiB |
| 1 | M3 one-byte patch at 100 MiB |
| 1 | M4 1-MiB patch at 10 MiB |
| 2 | M5 count-change fallback at 1/100 MiB |
| 2 | M6 invalid-authority fallback at 1/100 MiB |
| 1 | M6 external-mutation fallback at 1 MiB |
| 3 | M7 symlink, before-publication, and lost-ack focused 1-MiB rows |
| 3 | adjacent protected 100-MiB create, edit, and reopen/head guards |
| **30** | **fixed total** |

The separate closure-product A/B (`G4-R1` in the decision matrix) is **not
stacked into these 30 rows**. It is a later G4-repair side experiment with its
own <=120-second full build/prep/measure/analyze/cleanup ceiling. Passing G4
acceptance rows are not rerun to accommodate it.

The measured-campaign allocation is prospective and terminal:

```text
T_campaign
  = T_lock_and_measured_preflight        <=  5 s
  + T_private_base_and_shared_prep       <= 50 s
  + T_30_row_dispatch_and_operations     <= 20 s
  + T_separate_exact_row_verification    <= 10 s
  + T_primary_and_independent_analysis   <= 10 s
  + T_cleanup_storage_and_modes          <=  5 s
  + T_payload_manifest_and_terminal      <= 10 s
  + T_reserve                            <= 10 s
  <= 120 s
```

Any bucket overrun is a campaign-design failure and stops before another row;
the ceiling is never increased. Preparation is shared only where exact
authority allows fresh isolated destinations/permits; its wall, CPU, RSS, Q,
seed/temp storage, and reuse graph are reported. The measured-operation sum is
also reported and targets `<20 s`, but it is not substituted for `T_campaign`.

## 5. Timer equations

No nested timer is added twice.

```text
reconstruction_total
  = open_or_reopen_if_in_scope
  + mapping_and_canonical_authentication
  + required_proof_and_output_folds
  + consuming_sink

first_native_total
  = preflight
  + authority_qualification
  + authenticated_reconstruction_into_private_temp
  + data_sync
  + metadata_apply
  + metadata_sync
  + atomic_rename
  + directory_sync
  + required_reconciliation
  + cleanup

protected_seed_total
  = preflight
  + authority_qualification
  + clone
  + selected_range_authentication_and_patch
  + data_sync
  + metadata_apply_and_sync
  + atomic_rename
  + directory_sync
  + required_reconciliation
  + cleanup
```

Seed creation, complete seed verification, permit minting, target construction,
fixture reads, candidate compilation, and expected-output construction must be
reported as separately timed preparation. They may be outside the operation
timer but never outside the campaign wrapper or omitted from the row.

The retained G3 100-MiB one-byte row demonstrates why: its qualified operation
was 3.414166 ms, while the full child was 4.24 s real / 3.23 s user / 0.91 s
system, with a 100-MiB seed, a 100-MiB candidate temp, and a 100-MiB exact
verification read outside the operation timer. Every seed/cache result must
therefore expose separate equations for (a) fill/qualification, (b) qualified
hit, and (c) maintenance revalidation/eviction/repair/rebuild. Maintenance is
not free background work; record foreground and maintenance CPU, RSS/Q,
storage, and wall independently.

## 6. Direct counters and resource equations

Every successful and failing row records deltas for:

- wall, user CPU, system CPU, CPU percentage/core span, voluntary/involuntary
  switches, maximum RSS, and instructions/cycles only where directly supported;
- exact logical `Q` equation, high-water, current/terminal value, DFS frames,
  decoded state, canonical buffers, output buffers, worker queues, and the
  maximum simultaneous overlap;
- source/logical/canonical bytes read, authenticated, hashed by domain,
  decoded, returned, reconstructed, written, cloned, patched, compared, and
  discarded;
- roots, mappings, leaves, references, objects, closure occurrences, range
  objects, queries, rows, statement-cache acquisitions, row BLOBs, borrowed
  BLOB bytes, incremental BLOB opens/reads, and filesystem operation wrappers;
- file/dir opens, no-follow checks, `read`/`pread`, `write`/`pwrite`, clone,
  preallocation/truncation, chmod/metadata, data sync, metadata sync, rename,
  directory sync, reconciliation, unlink, and residue counts;
- SQLite cache used snapshots, hits, misses, writes, spills, page size, derived
  pager bytes (explicitly not physical I/O), and all runtime PRAGMAs;
- pre/peak/post logical, apparent, and allocated DB, journal, authority, seed,
  temp, destination, and total-store bytes; and
- exact publication outcome, first error, cleanup error, reconciliation result,
  old-or-new result, receipt/permit consumption, and terminal residue.

`Q`, RSS, allocation blocks, SQLite pager bytes, block-operation counts, and
wall time are never converted into physical bytes. Unsupported VFS sync or
physical-device observations are `Unavailable` with a reason.

The resource gates are:

```text
application maximum RSS <= 20 MiB
terminal Q = 0
full-file application buffer = 0 bytes
all queues and buffers have frozen finite capacities
new mandatory persistent metadata <= 5% of canonical durable truth
per-revision full native duplicate = 0
optional cache capacity, eviction, corruption recovery, and allocated bytes = explicit
```

## 7. Performance objectives and protection gates

The 100-MiB objectives are prospective hypotheses:

| Path | Acceptance | Stretch |
|---|---:|---:|
| Warm complete authenticated reconstruction | `<=333 ms` / `>=300 MiB/s` | `<=300 ms` / `>=333 MiB/s` |
| Fresh-process reconstruction | `<=400 ms` / `>=250 MiB/s` | `<=350 ms` / `>=286 MiB/s` |
| Controlled host-buffer-cold reconstruction | `<=400 ms` / `>=250 MiB/s` | `<=333 ms` / `>=300 MiB/s` |
| Same-open trusted-seed full read | `<=50 ms` / `>=2,000 MiB/s` | `<=35 ms` / `>=2,857 MiB/s` |
| First full native, warm source | `<=400 ms` / `>=250 MiB/s` | `<=333 ms` / `>=300 MiB/s` |
| First full native, controlled host-buffer-cold | `<=500 ms` / `>=200 MiB/s` | `<=400 ms` / `>=250 MiB/s` |
| Protected-seed same-root clone | `<=10 ms` | `<=5 ms` |
| One-byte incremental native update | `<=10 ms` | `<=5 ms` |
| 1-MiB incremental native update | `<=20 ms` | `<=10 ms` |

Absolute targets do not replace adjacent protection. The later preregistration
must run or bind an exact adjacent control and normally reject more than 5%
degradation in durable create, same-count and count-changing edits, 1-MiB
range, reopen/head, and retained G3 qualified routes. Contextual 5% ceilings
from current retained values are:

| Protected operation | Retained | Contextual 5% ceiling |
|---|---:|---:|
| durable full create | 308.884052 ms | 324.328255 ms |
| same-count middle edit | 6.960791 ms | 7.308831 ms |
| `+1` early | 5.108458 ms | 5.363881 ms |
| `+1` middle | 4.576000 ms | 4.804800 ms |
| returned 1-MiB range | 2.279209 ms | 2.393169 ms |
| reopen/head | 2.088334 ms | 2.192751 ms |
| G3 100-MiB one-byte mechanism screen | 3.414166 ms | 3.584874 ms |

Up to 10% is reviewable only after an independently measured at-least-2x read
or first-materialization gain with bounded resources. No correctness,
authority, exact-error, crash, cleanup, durability, or terminal-Q regression
is reviewable.

## 8. Retain, kill, and unavailable rules

A mechanism is retainable only if all exact semantic, counter, timer, custody,
resource, storage, cleanup, and independent-analysis gates pass and its primary
row meets the relevant objective without violating a protected operation.

Kill immediately on any of:

- identity/root/transition/closure/output/occurrence mismatch;
- unauthenticated output before the applicable authority boundary;
- reused ordinary user-editable destination or persistent seed as byte
  authority;
- more than one publication COMMIT where the existing contract requires one,
  changed `FULL+DELETE`, or missing native data/metadata/directory durability;
- unbounded/full-file memory, RSS over 20 MiB, terminal `Q != 0`, residue, or
  unexplained storage growth;
- cold-state relabeling, hidden setup, fabricated clone throughput, or inferred
  physical I/O;
- protected-operation breach; or
- wrapper wall over 120 seconds. A timeout rejects the campaign design; it
  does not increase the ceiling.

An unavailable observation cannot fail a mechanism unless the observation is a
preregistered qualification requirement. It must remain explicitly
`Unavailable`; it cannot be replaced by zero or a proxy.

## 9. Evidence, independent analysis, and append-only repair

The result root is append-only from the first row start. Each row is one JSON
object written once, immediately hashed, and added to a chronology. Raw rows,
stderr/stdout, source custody, method custody, operands, environment, storage,
cleanup, and timing remain immutable after creation.

Primary and independent analyzers must be separately hash-frozen and must parse
the raw JSONL independently. They must recompute sizes, exact row values, timer sums,
counter deltas, Q/storage equations, performance/protection gates, cleanup,
chronology, and row-set completeness. Exact agreement is terminal.

If a documentation or analysis defect is found after rows exist:

1. do not edit, delete, replace, or rerun a row;
2. append a repair record naming the old artifact hash, defect, scope, and new
   artifact hash;
3. independently verify that raw rows and operands are unchanged;
4. issue a new versioned manifest/terminal that includes the prior terminal;
   and
5. treat any defect that changes row meaning, custody, one-variable scope, or a
   performance gate as `REVISE`, requiring a new separately authorized
   campaign rather than repair.

The terminal closes only when the payload manifest has no missing, extra, or
mismatched entry; cleanup and modes pass; the benchmark lock is released; and
the wrapper wall remains within budget.

## 10. G4 exit and non-integration statement

G4 may exit only with:

- both scoreboards frozen with honest unavailable cells;
- a disposition for complete reconstruction, first native materialization,
  protected-seed full read, clone/no-op, one-byte, 1-MiB, count-change,
  invalid-authority/mutation fallback, and faults;
- protected create/edit/range/reopen evidence;
- bounded CPU/RSS/Q/storage and exact durability/cleanup evidence; and
- complete primary/independent/manifest/terminal closure under 120 seconds.

Even a G4 PASS qualifies only the exact benchmark/engine mechanism. Persistent
cross-process seed authority, a native cache service, format/profile migration,
VFS/projection, SDK/application integration, concurrency/endurance/history,
and final Phase-4 closure remain separately authorized future work.

## Primary external semantics used by this proposal

- SQLite incremental BLOB handles and expiry rules:
  <https://www.sqlite.org/c3ref/blob_open.html>
- SQLite per-connection cache hit/miss/write/spill meanings:
  <https://www.sqlite.org/c3ref/c_dbstatus_options.html>
- rusqlite incremental/positional BLOB API:
  <https://docs.rs/rusqlite/0.40.2/rusqlite/blob/index.html>
- Apple `clonefile`/`fclonefileat` behavior and same-volume limitation:
  <https://developer.apple.com/library/archive/documentation/FileManagement/Conceptual/APFS_Guide/ToolsandAPIs/ToolsandAPIs.html>
- Apple `F_NOCACHE` and `F_FULLFSYNC` descriptor semantics:
  <https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/fcntl.2.html>
