# Current integrated C1+C2 10k hot-path diagnostic

## Preregistration (before instrumentation or timed run)

This is one count-driven **DIAGNOSTIC** of the current integrated C1 direct
builder and C2 bounded grouping at source `f26865747`, not a treatment arm or
admission sample. It uses the public `namespace-10000` native Init operation,
one fresh Store, four Init construction workers, one C2 owner, the existing
512-object / 4-MiB wave bounds, 128-KiB whole-file cutoff, 4-KiB SQLite pages,
seed 1, unchanged limits, and full verification **SKIPPED**. Output is fresh and
append-only. The driver command is:

```sh
python3 docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/cold_diagnostic.py --case namespace-10000 --independent-source-copy --fixed-operation-identity --out NEW_ABSOLUTE_OUTPUT
```

The H3 independent byte copy reuses the sealed master only outside the timed
operation. Full payload hash/invalidation and immediate nonfaulting whole-input
residency recheck must show zero resident source pages, or the row is
`INELIGIBLE`. Source metadata residency remains unqualified: elapsed time is
exploratory and cannot be pooled with a fully cold result. No OS cache utility
is used. No second sample will be taken for this instrumented identity.

Temporary isolated instrumentation emits one aggregate line after the file
Save and one after the file pipeline: `SaveOutcome` counts and existing seven
disjoint profile buckets plus nested diagnostic regions, object sends and
worker construction / blocked-send wall sums, receiver idle and owner accept
wall. Bounded queue flushes are counted by reason (lane change, bytes, rows,
dependency, direct placement, wave end). One terminal summary per Save avoids
per-object logging. Worker sums overlap each other and owner work; nested
profile regions overlap their parent buckets. Report exact counts without
adding overlapping durations. Preserve raw stderr, timing, receipt, CPU/RSS,
Store geometry, instrument patch, source/harness hashes, and every failure.
Rank the next optimization from this current profile, not earlier D11/D12
profiles. No product algorithm change is part of this diagnostic.

## Result

One run completed on 2026-09-23. Its original output is retained at
`benchmark-results/fs-bench-pro/issue237-integrated-hot-profile-20260923-a/` in
this isolated worktree; the compact [receipt](evidence/integrated-hot-profile/receipt.json),
[telemetry](evidence/integrated-hot-profile/telemetry.lft1), [cold checks](evidence/integrated-hot-profile/cold-launch.json),
[Store geometry](evidence/integrated-hot-profile/store-geometry.json), and
[instrumentation diff](evidence/integrated-hot-profile/instrumentation.diff.gz)
are copied here. The build and command identities are in
[run.json](evidence/integrated-hot-profile/run.json) and
[build.json](evidence/integrated-hot-profile/build.json). The instrumented
source remained a dirty, temporary worktree diff; it is not product code.

| Observation | One diagnostic result |
| --- | ---: |
| Public operation / full command | 1,352,215,375 / 2,283,417,042 ns |
| Input / throughput calculated from public time | 300,000,000 B / 221.858 decimal MB/s |
| `history.import_files` / scan / named finish-save child | 1,170,553,792 / 46,905,875 / 3,654,833 ns |
| Service caller CPU, sampled local monitor | 960,667,000 user + 853,074,000 system ns; shared process window |
| Service sampled maximum RSS | 59,588,608 B; boundary coverage false, about 104 ms maximum sample gap |
| Store apparent / allocated | 334,176,256 / 335,609,856 B |
| SQLite page size / page count | 4,096 B / 81,586 |
| Pack capacity / assembled / spare | 331,350,016 / 305,977,812 / 25,372,204 B |
| Objects / packs, all saves | 24,683 / 1,264 |

Both source checks found **0 of 27,503 pages resident** across 10,000 files and
300,000,000 B; the immediate recheck ended 6,533,542 ns before the public
timer. The source was an `independent-byte-copy-v1` of the sealed master and
the fixed operation identity was used. The runner nevertheless labels the row
`source-cache-uncontrolled-v1`, `DIAGNOSTIC`, `admission_eligible=false`:
metadata residency is unmeasured. Full verification was `SKIPPED`; telemetry,
cleanup and the performance command passed. No fully cold speedup or readback
claim follows. The retained Store has SHA-256
`4c0810f84173b309b51d6efc329c6fb23685d7e680ee340b530c2f4f5b020e31`;
its public root is
`e7850f75a65309568e3454f7ab962aa16952d0c02218a55f42f3e2fd44ddf673`.

The file Save inserted **24,364**, reused **198**, created **1,259 packs**,
appended **1,089** times, issued **1,405** object-row INSERT statements and
committed **80** times. Its aggregate queue actually drained **1,347** times:
1,153 because another group would exceed the 256-KiB encoded-body cap, 118
on lane changes, one on the row cap, and 75 at wave end. Dependency and direct
placement caused zero queued drains. Those drains placed 7,716 groups, 24,322
rows and 300,630,888 encoded-body bytes; the remaining 42 inserted rows used
the direct path. The byte-bound count is close to
`ceil(300,630,888 / 262,144) = 1,147`; eliminating every lane switch could
remove at most 118 of 1,347 drains and would not necessarily remove a pack
write. The prerequisite Save had no queued drain; the tree Save had one wave-end
drain (six groups, 280 rows, 65,451 body bytes).

The receiver called `accept` **24,562** times, exactly the inserted-plus-reused
count, and spent **725,295,485 ns** there. It spent **441,699,032 ns** in
34,562 receive waits; those two nonoverlapping receiver actions explain
1,166,994,517 ns of the 1,170,553,792 ns file span, leaving 3,559,275 ns for
its other work. Across four producers, 10,000 `construct_file` calls accumulated
**4,501,064,088 ns**; **2,701,273,425 ns** of that was object-send time,
leaving **1,799,790,663 ns** across all producers for file open/read, C1
construction and checks. The 10,000 `Done` sends took another **177,614,599
ns**. Producer sums overlap one another and the receiver; send time includes
backpressure from the owner and is not removable channel overhead by itself.

The existing seven disjoint Save buckets totaled **464,116,180 ns**:
COMMIT **232,459,671**, SQL **139,615,750**, FULL encoding **69,895,968**,
group framing **11,837,409**, placement **10,307,382**, with resolution and
delta zero. The existing whole-`accept` profile was **723,680,135 ns**.
There is about **260 ms of owner work not assigned to a seven-bucket category**;
the category total also includes small begin/finish work outside receiver
`accept`, so this subtraction is a localization hint, not an exact exclusive
time bucket. Nested diagnostic regions include wave membership/presence
**64,237,454 ns**, candidate validation **44,291,996 ns** (of which collision
queries **42,744,487 ns**), pack writes **79,151,712 ns**, and object INSERTs
**66,017,582 ns**. These overlap their parent regions and must not be added to
the seven-bucket total. The Save's final drop took **62,652,458 ns**, almost
entirely connection release (**62,641,125 ns**); it occurs after the named
finish-save child timer but inside the public operation.

## Ranked next work from this current profile

1. **Treat COMMIT count and transaction cost separately.** COMMIT accounts for
   232.46 ms of this Save profile, 32.1% of receiver accept. If 90% fewer
   commits removed 90% of this time with no side effect, the optimistic public
   time would still be 1.143 s, about 262.5 MB/s. The separate matched
   [bounded-wave experiment](bounded-wave-experiment.md) made
   **seven** commits but raised aggregate COMMIT time and slowed public Init.
   That measured result rejects a simple count-based speed prediction; any new
   transaction treatment must also reduce pager work, lock hold and memory.
2. **Locate the owner remainder before changing per-object logic.** About 260
   ms is not charged to the seven disjoint buckets. Existing nested spans already
   name 64.24 ms wave lookup and 44.29 ms validation, but the scopes overlap.
   A targeted count/time probe of membership map construction, per-object
   availability checks and group assembly can separate what is scalable from
   unavoidable codec/SQL work. Do not infer the remainder's cause from the old
   D11/D12 profile.
3. **Reduce producer gaps without warming the source.** Receiver idle time is
   441.70 ms while four producers have 1.800 s aggregate non-send construction
   work. Instrument source reads and C1 construction inside that aggregate before
   selecting a treatment; faster C2 may increase receiver idle, so this saving
   cannot be added to the COMMIT bound. Keep the 128-KiB cutoff and cold payload
   contract.
4. **Inspect the 62.64 ms connection release cost in the public operation.** It
   is larger than all lane-switch queue drains can be assumed to save. Any fix
   must genuinely reduce teardown work inside the public call, rather than move
   it past the timer. The current profile does not explain why close costs this
   much.

The operation must reach **578,257,517 ns** for 518.8 decimal MB/s on this
300-MB fixture, **773,957,858 ns** below this diagnostic. The commit target
alone cannot close that gap. The 700-MB/s target requires 428,571,429 ns;
neither target is claimed here.

Identity custody: preregistration commit
`a9f8b7de4bd5c7451b52a1037dfac1ffce450ced`, product seal
`25293a2fc42e0101bbb9165fc495f9d742587f486e0d5d413cdcd282a81deaf9`,
harness seal `6d9a3e2eb2a1eea1f3f0d63f0df15949d6f3bab103657a824b0a04d34482eff8`,
fixture manifest SHA-256 `c1d7937c9f90d3585558e6d20b35183a737530d386667d08badc3121a6b3d55e`,
H3 driver SHA-256 `767292e7b5bfc9a06291239b471247a79474b91ce2ca0db8fc9092807e2c09a4`,
instrumentation diff SHA-256
`0c1519f56d6cfff4dcb8f961f63bed7c8debda7c668e4d191bba72efeb841942`.
The release build was recorded as first-use, `PASS`, 106,333,792 ns after an
untimed private-target dependency copy and a 13.5-second compile check; no
build overlapped the timed phase.
