# WP4-M M4.5 terminal report — independent-audit checkpoint

## F0 custody freeze — 2026-08-19

- **F0 PASS.** The accepted M4.5 checkpoint is frozen at clean commit
  `26f4f10122a16dd14474e93076c92f80876b798f`, parent
  `f3df30a80172131b74b5949a6a55234c962dac67`, and tree
  `0c9042da733d9ca0045a93fb69eb709f8d77ef09`.
- Exact patch definition
  `git diff --binary f3df30a80172131b74b5949a6a55234c962dac67 26f4f10122a16dd14474e93076c92f80876b798f`
  hashes to
  `8103959f462cb073293d42ae3944ad80171cb0e9509417fc08352288e960e7d3`.
  The historical v4 tracked-dirty diff
  `49d1734aae97d30cadc2d7224e6729e40c22eef91f625e8dbaf40ecd9061d281`
  remains separately named and is not substituted for the committed patch.
- V4 remains the active small-edit baseline: C0 `446.457042 ms` versus C1
  `8.540708 ms` (`-98.087003%`, 5/5), exact Q 2,222,803 bytes, terminal zero,
  release executable
  `7c395935457f99acfd3b02e08cdba976b971c61a407aeb65c243f2f26ddaf1a2`.
- V4 verifies 61/61 complete-manifest entries and 15/15 focused-manifest
  entries; v3 independently reverifies 171/171. The committed benchmark and
  controlling spec are byte-identical to the frozen v4 snapshots.
- A fresh independent recomputation from retained v4 raw JSONL/preflight is
  byte-identical to the retained independent JSON. All 12 rows are release,
  non-debug, one transaction, one COMMIT, committed publication, exact
  identity/Q/storage, and byte-identical database/authority/expectation arms.
- C0 complete-closure and C1 changed-spine are permanent regression controls.
- Dedicated report: `wp04-opt-milestone-f0.md`. Compact machine manifest:
  `target/wp4m-f0-m45-freeze-20260819-v1/f0-manifest.json`.
- No F0 implementation, build, benchmark, profile decision, promotion,
  production integration, or Phase 4 completion occurred. F1 is eligible only
  as a separate task. No F0 commit was created.

The M4.5 sections below remain preserved as the evidence being frozen.

## Final checkpoint disposition — 2026-08-19 v4 follow-up

- **PASS; ready for a separate F0 freeze.** The private exact-XOR same-count
  changed-spine implementation remains accepted and retained.
- The v3 terminal campaign/report below remain unchanged as the prior accepted
  baseline. The final checkpoint follow-up is
  `wp04-opt-milestone-4-5-v3-follow-up.md`, with new release evidence at
  `target/wp4m-m45-checkpoint-k64-20260819-v4/`.
- §13.5A now records the actual one-executable C0 complete-closure versus C1
  changed-spine comparison as a post-measurement clarification. The v3
  measured-spec hash and §13.3/§13.3A history remain preserved.
- `ChargedVec::from_exact_builder` rejects `len == declared` vectors whose
  separately allocated capacity exceeds the admitted capacity; the typed
  failure returns Q to zero.
- A source-free synthetic K64/F64 topology directly proves H=2 behavior:
  262,145 references, four changed leaves, five changed branches, 376 covered
  edges, 14 new/different edges, 34 C1 SQL queries versus 266,318 C0 queries,
  43,488-byte C1 qualification Q high-water, and terminal zero. Deep malformed
  cumulative summaries are rejected by both modes as typed `LengthMismatch`.
- All **98 tests** pass; warnings-denied clippy, format, diff check, and debug
  self-test pass.
- The release-path guard changed executable bytes, so the fresh official v4
  result is C0 `446.457042 ms` versus C1 `8.540708 ms`
  (`-98.087003%`, 5/5 wins). Campaign Q remains 2,222,803 bytes with terminal
  zero. RSS is -0.175% and peak footprint +0.129% by arm median, so the
  15-pair extension was not triggered.
- V4 measured diff / release executable / complete manifest SHA-256:
  `efc18e05d85c0ecb7a7dc02dd72205d873ad173521848800614511a7f1a1f449` /
  `7c395935457f99acfd3b02e08cdba976b971c61a407aeb65c243f2f26ddaf1a2` /
  `1b1621735ad949abe4755e94dcd2487699af5502479dd99b707cc4d4a20e99c1`.
- Final tracked dirty diff SHA-256:
  `49d1734aae97d30cadc2d7224e6729e40c22eef91f625e8dbaf40ecd9061d281`.
- Qualification, promotion, profile selection/rejection, production
  integration, F0 source work, and later Phase 4 work remain not started.

The v3 terminal section below is preserved as the exact prior checkpoint. Its
“only accepting campaign” wording describes the state before this release-path
follow-up and does not invalidate or overwrite v3.

## Final terminal disposition — 2026-08-19 v3 repair

- **PASS** for the private K64/F64 exact-XOR same-count changed-spine
  milestone. The changed-spine implementation is retained.
- The complete final evidence is
  `wp04-opt-milestone-4-5-v3-terminal-benchmark.md`; the only accepting
  campaign is
  `target/wp4m-m45-repair-k64-20260819-v3-terminal/`.
- The prospective experiment authority is the new governing-spec §13.3A.
  The original §13.3 uniform-`0x5a` row remains verbatim but withdrawn:
  exact Phase-2 FastCDC proves it is count-changing (5,283 references).
- The amended `old_byte XOR 0x5a` edit has 5,284 references and independently
  frozen sequence/root/transition/closure identities. The terminal binary
  rejects any fixture or expectation mismatch.
- All correctness gates pass: **96 tests, 0 failed**; warnings-denied clippy,
  format check, diff check, and debug self-test pass. The final five-lane
  review found no P0/P1 authority, publication, CDC/COW, durability,
  logical-Q, accounting, or custody blocker.
- Final C0/C1 durable-edit medians are `440.023209 -> 9.134334 ms`
  (`-97.924124%`, 5/5 wins). Exact logical Q is 2,222,803 bytes in both arms
  and returns to zero on every row. The 20-pair adjudication found no
  repeatable RSS or peak-footprint regression.
- Measured implementation diff SHA-256:
  `e08558c030040216489365a76c0643fa83e3f49aec9425ac06b78bba4d86057d`.
  Release executable SHA-256:
  `f84e6b0f656e03ba3c537dbce08b085c3b52094a229b6df29593082e1d745ef1`.
  Complete retained-tree manifest SHA-256:
  `60887e2a4245fd3358f2242eac06b88e11051beacd3fc0bd0a2d7a7115f28cfd`.
- Qualification, promotion, profile selection/rejection, production
  integration, and later Phase 4 work remain false/not started. **F0 may
  begin only as a separate next task.**

The independent-audit FAIL/REVISE section and the earlier v2 PASS below are
preserved as historical checkpoints. Both are superseded for active status
by this post-repair v3 disposition; no old timing or Q value is reused.

## Historical disposition — 2026-08-19 independent audit

- **FAIL / REVISE.** The earlier repaired PASS below is withdrawn as an
  acceptance claim while the second independent-audit findings are repaired.
- The retained v2 XOR campaign is credible causal-direction evidence only. It
  was observed before the controlling experiment amendment, and its logical-Q
  and COMMIT-boundary evidence do not satisfy the governing hard gates.
- Qualification, promotion, profile selection, production integration, later
  Phase 4 work, and F0 remain false/not started. **F0 may not begin.**
- A final disposition requires the prospective governing-spec amendment,
  BEGIN-ownership repair, exact pre-admission Q proof, real COMMIT-boundary
  reconciliation proof, diagnostic-preserving publication outcome, all gates,
  one fresh retained v3 campaign, and a new read-only five-lane audit.

The PASS section below is retained verbatim as a superseded historical
checkpoint; it is not the active M4.5 disposition.

## Historical repaired disposition — superseded v2 checkpoint

- Final M4.5 disposition: **PASS** for the private K64/F64 same-count
  changed-spine milestone.
- Qualification: `false`.
- Promotion: `false`.
- Profile selection/rejection: `false`.
- Production integration and full-create gain: not claimed.
- F0: may begin only as a separate next work item; the repaired five-lane
  re-audit is PASS in `wp04-opt-milestone-4-5-independent-audit.md`.

The original implementing claim and the independent audit's **FAIL / REVISE**
record remain below unchanged as historical evidence. They are superseded by
the repaired source, exact operation, copied-base campaign, and report in
`wp04-opt-milestone-4-5-repair-benchmark.md`.

The critical correction is that exact FastCDC over the old uniform-`0x5a`
edited stream has 5,283 references. That operation is count-changing, so its
old 5,284-reference oracle and `431.490 -> 2.437 ms` result are inadmissible.
The repaired predeclared operation transforms the same 18,854-byte middle
chunk with `old_byte XOR 0x5a`; exact full scanning and local rejoin both
produce 5,284 references.

### Repaired custody and gates

| Item | Repaired value |
|---|---|
| HEAD | `f3df30a80172131b74b5949a6a55234c962dac67` |
| Measured implementation diff | `0c8d70bc6aa5944f40ead21ffefb335457df251f7df8351bef02c04acda0ac1e` |
| Measured benchmark source | `07df5f2b6124af8be4e8ad0f0213875108c0809d38e436a7c020ab83125188dc` |
| Release executable | `37643a4eb99a0ab8fcbeaa326ebb2ceada98a9716c9dbe677c6f4a53e7320d02` |
| Fixture / manifest | `63b3695b...eff4` / `8c64b5f4...d4ca` |
| Expectation, every pair | `70520375af87d5227e28775a59879067d3b942cd82eb3f2fd2e15bb942b169ff` |
| Raw / preflight / summary | `be708e3c...a8a6` / `e88bc7f2...9912` / `22da16ef...d887` |

All focused and workspace gates pass: **92 tests, 0 failed**, warnings-denied
clippy PASS, format PASS, and `git diff --check` PASS. Direct tests cover exact
edited CDC, complete witness closure, rollback invalidation, exact provenance,
all actual-COMMIT reconciliation outcomes, committed-result custody,
same-count length redistribution, bounded expectations, 1-GiB allocation
rejection, real-path summed Q, and parity.

### Repaired result

| Metric | C0 | C1 | Change / verdict |
|---|---:|---:|---|
| Durable edit median | 443.143 ms | 9.001 ms | -97.969%; 5/5 wins; PASS |
| Pre-COMMIT qualification | 434.819 ms | 0.297 ms | -99.932% |
| Exact CDC + mapping/COW | 6.191 ms | 6.211 ms | byte-identical substrate |
| SQLite COMMIT | 1.951 ms | 2.066 ms | diagnostic +5.893% |
| Same-open authority | 235.888 ms | 233.667 ms | separate linear phase |
| Post-COMMIT verification | 698.166 ms | 702.497 ms | separate linear phase |
| Same-open lifecycle | 1,134.436 ms | 710.947 ms | -37.329% |
| First-open lifecycle | 1,379.387 ms | 943.741 ms | -31.581% |
| CPU median | 1.810 s | 1.370 s | paired median -24.157%; PASS |
| Exact Q | 2,278,037 bytes | 2,278,037 bytes | every row terminal zero |
| Apparent / allocated endpoint | identical | identical | PASS |

Every one of the 12 arm images matched its pair's prepared database,
authority, and expectations hashes. The exact local CDC inspected 143,709
bytes and produced five changed chunks; C1 covered 123 equal immutable edges,
followed eight new/different edges, and fully authenticated five new chunk
objects / 103,363 canonical bytes. W/D, native prepares, sync/fsync, page-cache
bytes, peak journal/temp, and byte-level physical I/O remain explicitly
`Unavailable`.

The repaired complexity is
`O(X_b + X_c + K + F*H)` mutation,
`O(K + F*H + A_delta + V_delta + H^2)` qualification, and
`O(H + K + F + bounded buffers)` resident memory. Same-open authority, fresh
scrub, reconstruction, and first-open lifecycle remain linear; `+1` remains
suffix-linear and was not run.

Repaired artifacts are under
`target/wp4m-m45-repair-k64-20260818-v2/`. The old artifacts under
`target/wp4m-m45-k64-20260818/` remain retained and invalid for acceptance.

> Historical independent-audit disposition (2026-08-18): **FAIL / REVISE**. The terminal
> implementation and M4.5-5 campaign are not accepted. See
> `wp04-opt-milestone-4-5-independent-audit.md`. The original checkpoint below
> is retained as the implementing task's frozen claim and evidence record; it
> is not current acceptance authority and was superseded by the repaired
> disposition above.

## Terminal decision

- Implementation decision: **RETAIN**.
- Private same-count changed-spine mechanism: **PASS**.
- Production integration: **not claimed**.
- Qualification: **false**.
- Promotion: **false**.
- Rejection: **false**.
- Full-create gain: **not claimed**.
- Profile selection: **not run and not claimed**.
- Next action: **stop for independent read-only audit**.

M4.5 repaired the rejected-M4 authority, publication, oracle, durability,
failure-provenance, logical-Q, SQL-label, and causal-attribution defects.  The
retained 100-MiB K64/F64 same-middle experiment isolates C0 full closure from
C1 changed-spine qualification in one byte-identical executable.  C1 reduced
median durable edit latency from 431.490 ms to 2.437 ms (−99.435%) with 5/5
wins, while correctness, exact Q, CPU, storage, and the predeclared 20-pair
RSS/peak procedure passed.

This candidate `Store` remains a private benchmark shadow.  SQLite remains
authoritative.  No append-only/pack carrier, new production database/WAL,
worker, async path, pool, VFS, public profile selector, or source-sized state
was added.  No file in `/Users/yifanxu/Ephemeral-AI-Lab/layerfs` was modified,
and no commit was created.

## Governance and terminal custody

| Item | SHA-256 / value |
|---|---|
| governed worktree | `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty` |
| branch | `codex/empty-worktree` |
| HEAD | `f3df30a80172131b74b5949a6a55234c962dac67` |
| terminal tracked diff | `b001f0088234b2bd03300890d4195ea0c28d167e6e677df32144fe24f13490a3` |
| terminal benchmark source | `dbe0106c43665b81dfcf6d4604ffc807ad010dccf23a5a9a527dca1679a71656` |
| terminal debug executable | `558ac042274d93b6a2eb10969be890a63820f4a5973eed36acf9cbac38ca122f` |
| measured release-build diff | `640285dde7f3f5a84c0cb16a589b63020e5cb354acb0df3a7b3c257c018b44e0` |
| measured release-build benchmark source | `976cba60408bc00e939f063add8bf427fc66857477158d3aea4eae2235eafe18` |
| measured C0/C1 release executable | `f0ba1c2423161cc2f79a0e7378408141eecfed30d4e65aceab3c8c667e5570af` |
| frozen historical A0/M3 executable | `ff4f7206acbdff06bf9052550b3841e989f3cab603b509f9482c3d40b949213c` |
| rejected-M4 executable | `310d63e95a0d5dcbeedd537370c7d875cc0a2d57735e87b6254721de5a9043ad` |
| retained source fixture | `63b3695b8c117b5bc39885e0df0dcd0af1d49e575482bab16577d84b4f40eff4` |
| retained fixture manifest | `8c64b5f49a10651e71fd52df3959cae22d291af4d95f47e43f7456308baad4ca` |
| current oracle expectation | `81b2eaf5b5c0144fe945e2bd17228cee046e85ba021970eeeb0653cf8042a316` |

The release benchmark was built exactly once.  The later final clippy gate
reported eight style-only lints: one unnecessary `as_deref_mut`, one complex
return type, two internal argument-count warnings, one auto-deref, two
if/else style warnings, and one `write!` newline warning.  The terminal source
contains only the lint-equivalent alias/allow/rewrites needed to make
warnings-denied clippy pass.  No release rebuild or performance rerun followed.
Both the exact measured source/diff hashes and the terminal source/diff hashes
are therefore preserved rather than falsely presented as one state.

The worktree was dirty before M4.5 and remains dirty.  Existing and newly
appearing user-owned untracked reports/notes were preserved, not staged,
deleted, or overwritten.

## M4.5-0 — frozen evidence and corrected rejected-M4 rationale

M4.5-0 made no source edit.  It froze:

- retained M3 diff/executable/report/raw evidence;
- rejected-M4 diff/executable/report/raw rows;
- the exact retained 104,857,600-byte source and manifest; and
- the edited fingerprint, ordered CDC, root, transition, and closure.

The corrected rationale is:

1. rejected M4 had a semantic authority defect independent of performance;
2. its Store-open/cross-reopen receipt could not authorize changed-spine
   skipping in a later writer snapshot;
3. its prepublication proof was incomplete and did not bind complete mode and
   exact expected result;
4. its old Q was maximum-local plus a fixed envelope, not exact live Q;
5. `sql_preparations` did not prove native prepares; and
6. its mixed five-pair RSS result is preserved as inconclusive/noisy rather
   than causal proof under the new adjudication.

Frozen evidence report: `wp04-opt-milestone-4-5-0.md`.

## M4.5-1 — transaction-owned same-open authority

The first draft violated the independent audit because its witness was tied
only to one Store-open identity and could predate `BEGIN`.  The retained
sequence is now:

```text
BEGIN IMMEDIATE
  -> read and authenticate the exact complete prior visible head
     in that writer transaction/snapshot
  -> full same-open scrub and authority establishment
  -> reread and compare the exact complete head
  -> issue one transaction-owned witness
  -> consume it exactly once
  -> prepare/qualify the changed result
  -> stage one exact complete head
  -> dispatch one COMMIT
```

The private move-only witness binds:

```text
Store open identity
store instance ID
validation authority ID
integrity epoch
mapping profile
generation
root
transition
216-byte receipt
authority serial
active writer-transaction identity
single issuance and single consumption
```

Reopen, tuple mismatch, mutation/authority-serial change, reuse, absent writer
transaction, missing closure, and unresolved durability all invalidate it.
Persisted receipts never authorize cross-reopen skipping.  The receipt codec
and shared authenticated walkers are reused; no serializable witness or public
authority API exists.

Initial authority establishment is a full linear scrub and is reported
separately from edit latency.  Official-five medians were 239.172 ms C0 and
237.256 ms C1.

Detailed report: `wp04-opt-milestone-4-5-1.md`.

## M4.5-2 — complete changed-spine proof and independent oracle

C1 can run only after consuming the transaction witness.  Equal edges may be
covered only when the exact authenticated parent descriptor and child
`ObjectId` agree.  Every different mapping edge is recursively authenticated;
every new chunk is fully loaded, authenticated, length-checked, and matched to
its raw `ChunkId`.

Complete summaries check:

```text
mode
total raw length
reference count
level/minimal height
child count
candidate K/F profile and fullness
every cumulative_end
final partial leaf/branch rules
active-cycle/depth bounds
exact EOF
```

The shared file-root decoder now exposes mode, and same-count COW re-encodes
the original mode rather than hardcoding zero.

Preparation creates a disposable, separately authorized SQLite oracle image,
performs a full rebuild, validates source/ordered CDC/root/transition/closure,
closes/removes that image, and writes only bounded immutable expectations.
The measured child receives the fixed operation, offset, exact bounded
inserted/removed bytes, prior/after file IDs, root, transition, and closure.
It cannot learn expected values from the measured result or warm/mutate the
measured database during oracle construction.

Before activation, C0 ordinary full closure and C1 incremental proof were run
against the same prepared result and required to agree for:

| Case | C0 | C1 |
|---|---|---|
| valid one-change | accept | accept |
| exact missing new object | reject same `MissingObject(ObjectId)` | reject same `MissingObject(ObjectId)` |
| multiple changed children | accept | accept |
| final partial leaf | accept | accept |
| malformed cumulative summary | reject | reject |
| forged nonzero mode | reject | reject |

The retained one-change fixture proves four prior and four replacement spine
objects, 127 covered equal edges, four new/different edges, and one fully
authenticated 18,867-byte new chunk.  The multi-change/final-partial fixture
proves six/six spine objects, seven different edges, and two new chunks.  No
failing result reached COMMIT.

Detailed report: `wp04-opt-milestone-4-5-2.md`.

## M4.5-3 — complete-head publication and durability reconciliation

Publication compares the exact prior tuple:

```text
(generation, root, transition, validation_receipt)
```

Genesis is INSERT-only.  Update predicates on every complete-prior-head field
and must affect exactly one row, closing ABA.  Requested generation is checked
`prior + 1`, and the requested receipt binds the complete new head.

An actual SQLite COMMIT error is forced through SQLite's native commit hook.
Every actual COMMIT error uses a fresh independent
`SQLITE_OPEN_READ_ONLY` connection, performs no DDL, validates authority and
the complete receipt-backed head, and classifies:

| Observation | Result |
|---|---|
| exact requested complete head | requested-visible / success |
| exact prior complete head | prior-visible / original COMMIT error |
| another valid complete head | different-head / publication conflict |
| unreadable or invalid authority/head | unknown / ambiguous durability |

First error, cleanup-first error, reconciliation, and dominant result remain
separate.  Reconciliation cannot mint witness authority.  All fallible
counter/capacity precomputation occurs before COMMIT dispatch; the ordinary
known-success path has no fallible post-COMMIT instrumentation that can relabel
success.  Typed object reads preserve the exact missing `ObjectId`.

Detailed report: `wp04-opt-milestone-4-5-3.md`.

## M4.5-4 — exact Q, SQL, W/D, and structural parsing

The old fixed-envelope/max-local Q was deleted.  Scoped capacity charges use:

```text
charge(c):
  q_current'    = checked_add(q_current, c)
  q_high_water' = max(q_high_water, q_current')

release(c):
  q_current'    = checked_sub(q_current, c)

row gate:
  q_current = 0
```

Canonical, decoded, file-page/reference, transition/delta, SQL-string,
ancestry-stack, output, replacement, receipt, and expected-operation
capacities are summed while simultaneously live.  A deliberate overlap test
holds parent page, child canonical, and ancestry allocations together and
asserts their exact capacity sum.  A forced error proves scoped decharge back
to zero.  Every current release row reports `Q=48,133` bytes and final
`q_current=0`.

SQL now separately reports:

```text
statement-cache acquisitions
query calls
execute calls
rows returned
rows changed
row and incremental-BLOB counters
transactions and COMMITs
native SQLite prepares = Unavailable
```

The higher-authority historical W/D fields remain unchanged.  Separately
named canonical new-write, authenticated-nonnew, rewrite, and edge counters
avoid redefining W/D.  Per-row equations include:

```text
canonical authenticated
  = canonical new write + canonical authenticated nonnew

sql calls = query calls + execute calls
commits <= transactions
borrowed row reads/bytes <= authenticated row reads/bytes
fully authenticated new objects/bytes <= total authenticated objects/bytes
q_current = 0
```

Campaign ingestion structurally parses top-level JSON, handles nested
objects/arrays/escaped strings, rejects duplicate fields, and joins exact
comparison/iteration pairs before calculating effects.  It no longer uses
string search or attributes unpaired medians to an algorithm.

Logical Q excludes allocator metadata, rusqlite/SQLite internals, page cache,
filesystem cache, and kernel state.  Those are separately Observed or
Unavailable.

Detailed report: `wp04-opt-milestone-4-5-4.md`.

## M4.5-5 — release evidence and causal result

### Raw custody

All authoritative raw artifacts live under
`target/wp4m-m45-k64-20260818/`:

| Artifact | SHA-256 |
|---|---|
| `wp4m-m45.raw.jsonl` — all 78 verbatim rows | `f6f1e698b7e50272cb993897c6ecff0c53fa4ba6bbd72c742f99de513f6e6165` |
| `wp4m-m45.macos-time.txt` | `673bb4e5da4dc8b955744b15c696136350d4e1345c2254c47e328c09c387ef49` |
| `wp4m-m45.commands.txt` | `13b30deda2063f04df2a6fc5683afa5f61da5aebafe1fee4f0e76633be4c0a3a` |
| `wp4m-m45.preflight.tsv` — 78 prepared bases | `6168b8be546b25a504321340bafd6e0b9659aa79bd5c55da9b6ef2ab069634a3` |
| `wp4m-m45.summary.json` | `8ff8ad020e348904c3c89a539b1c299dfa6b718e87860ce9ecd8f0d14a84cce3` |

There are eight warmup rows, 40 initial measured rows, and 30 predeclared
RSS-extension rows.  All are `PASS`; all current rows have one transaction,
one COMMIT, exact timer equations, exact result identities, and zero terminal
Q.

### Causal separation

| Comparison | Median edit A → B | Delta | B wins | Classification |
|---|---:|---:|---:|---|
| C0a → C0b A/A | 438.358 → 436.975 ms | −0.316%; median absolute pair noise 0.883% | 4/5 | noise calibration |
| A0 → C0 | 441.740 → 439.255 ms | −0.562% | 3/5 | wall inconclusive/within noise; corrected substrate CPU tax separate |
| **C0 → C1** | **431.490 → 2.437 ms** | **−99.435%** | **5/5** | **causal mechanism PASS** |
| A0 → C1 | 440.879 → 2.738 ms | −99.379% | 5/5 | cumulative continuity only |

The five official C0/C1 edit deltas are:

```text
-437,059,000; -428,921,667; -429,137,500; -440,792,791; -425,233,125 ns
-99.212%; -99.477%; -99.455%; -99.431%; -99.430%
```

This exceeds the `>=5%`/`>=4-of-5` gate and calibrated noise.  It is edit
latency, never 100-MiB throughput.

The causal counter difference is exactly 5,362 fewer cache acquisitions,
queries, returned rows, and row-BLOB reads in C1.  Writes, rewrites, executes,
changed rows, transactions, COMMITs, Q, and endpoint logical storage are
invariant.  C1 records the expected 127 covered edges, four different edges,
and one fully authenticated new chunk.

### Resource classification

| Resource | Result | Classification |
|---|---|---|
| CPU | 1.360 s → 0.940 s median (−30.882%), 5/5 | PASS |
| exact Q | 48,133 → 48,133 bytes; all final current 0 | PASS |
| logical/apparent store | 109,297,696 bytes both arms | PASS |
| allocated growth | 16,777,216-byte median both; 19 ties, one C1 improvement | PASS |
| one transaction/COMMIT | exact in every row | PASS |
| process block operations | observed zero through macOS `time -l` | Observed |
| instructions/cycles/RSS/peak | captured per child | Observed |
| native SQLite prepares | no direct complete instrumentation | Unavailable |
| SQLite/page/filesystem cache | no direct instrumentation | Unavailable |
| sync/fsync counts | no direct instrumentation | Unavailable |
| byte-level physical I/O | no direct instrumentation | Unavailable |
| peak temp/journal | no direct instrumentation | Unavailable |
| journal endpoint length | zero in every current row | Observed |

The first five arm medians triggered the RSS extension.  Across 20 pairs:

| Measure | C0 median | C1 median | Paired median | >5% regression pairs | Verdict |
|---|---:|---:|---:|---:|---|
| RSS | 17,727,488 | 18,055,168 | +131,072 bytes / +0.699% | 5/20 | no repeatable regression |
| peak footprint | 11,862,400 | 12,239,244 | +163,840 bytes / +1.271% | 6/20 | no repeatable regression |

Repeatable failure required paired median above 5% and at least 16/20 pairs
above 5%.  Neither holds.  The original noisy five remain in raw evidence.

Detailed phase medians/min/max/spreads, every official-five phase delta,
counter equations, A0/C0 CPU tax, and physical classifications are in
`wp04-opt-milestone-4-5-5.md` and the hashed structural summary.

## Complexity and bounded-memory claims

Only these claims are retained:

```text
same-count mutation
  O(Xb + Xc + K + F*H)

C1 qualification
  O(K + F*H + A_delta + V_delta + H^2)

resident candidate memory
  O(H + K + bounded pages/chunks/SQL/output)

C0 full closure
  linear in the complete reachable closure

first same-open authority establishment
  linear in the complete reachable closure

fresh scrub/reconstruction
  linear in closure/source bytes

+1/count-changing edit
  suffix-linear and NotRun in M4.5
```

The H² term is the deliberately bounded ancestry scan.  No visited map,
source-sized vector, background worker, cache, or new persistent carrier was
introduced.  Complete lifecycle remains linear; C1 does not make it
logarithmic.

## Final validation

Commands run against the terminal source:

```text
cargo metadata --offline --no-deps --format-version 1
  -> PASS

cargo test --workspace --offline --all-targets
  -> 83 tests passed; 0 failed
     layerfs-core: 44
     layerfs-engine lib: 4
     private benchmark: 18
     Memory/SQLite parity: 12
     layerfs-eval: 5
     remaining all-target crates: 0 tests, PASS

cargo clippy --workspace --offline --all-targets -- -D warnings
  -> PASS

cargo fmt --all -- --check
  -> PASS

git diff --check
  -> PASS

cargo build --offline -p layerfs-engine \
  --bin phase4_create_edit_benchmark
  -> PASS (debug only)

target/debug/phase4_create_edit_benchmark --self-test \
  /tmp/layerfs-m45-terminal-selftest.8tC07P
  -> PASS; root=f1cfdd7fdd658506caf39ace169dd83f1a95fbb25aafcbb7f9b72864f9d2e42a;
     objects=20; auth_bytes=1,054,836
```

The temporary terminal self-test input, database, and authority sidecar were
removed.  No release build or benchmark followed the frozen M4.5 campaign.

## Defect disposition and audit boundary

The independent audit identified real P0 defects in the first M4.5 draft:

- Store-open authority could predate the writer snapshot;
- publication compared an incomplete head and allowed ABA risk;
- nonzero file-root mode was not preserved end-to-end;
- real COMMIT failures lacked fresh independent reconciliation;
- expected results could be coupled to measured preparation;
- changed-spine lacked a full-closure activation shadow;
- C0 substrate cost was bundled with C1 algorithm effect;
- Q/SQL/W/D/JSON evidence was mislabeled or incomplete; and
- the original five-row RSS interpretation was not procedurally sufficient.

All are corrected in the retained private candidate and directly tested.  No
known correctness, authority, atomicity, identity, exact-Q, or storage hard
gate remains failing.  The final clippy-only source/executable custody split
is explicitly disclosed for independent verification; it is not hidden by a
second release build.

Decision: **retain**, stop here, and request independent read-only audit.  Do
not infer production integration, full-create gain, profile selection,
qualification, promotion, 512-MiB behavior, `+1` improvement, or M5 readiness
from this result.
