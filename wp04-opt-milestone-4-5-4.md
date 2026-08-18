# WP4-M M4.5-4 — exact live Q, SQL, changed-work, and structural evidence

- Verdict: **PASS for debug correctness and accounting gates**.
- Release performance: **NotRun**.
- Decision: retain the checked accounting implementation.  M4.5-5 may start
  only after its own preflight reconfirms this exact tree and no Cargo writer.
- Scope remains the private benchmark `Store` shadow.  This is not production
  `Engine` integration, promotion, profile selection, or a full-create claim.

## Independent-audit corrections incorporated

The rejected-M4 fixed envelope plus maximum-local-vector diagnostic was
removed.  The candidate now maintains one thread-local, private logical-Q
current sum for the single-threaded benchmark child and scoped move-only
charges for LayerFS-owned capacities.  Charge, high-water update, and
decharge use checked `u64`; `Drop` guarantees early-return/error cleanup, and
every emitted row calls `finish_q` and requires `q_current == 0`.

Tracked capacities on the governed same-middle paths include:

- canonical SQLite row vectors and generated canonical vectors;
- decoded object vectors and canonical-name storage;
- file-root, branch, leaf-reference, transition-page, and delta-operation
  vectors;
- simultaneously retained prior/replacement parent and child vectors during
  recursive descent;
- active prior/replacement ancestry stacks;
- generated bounded leaf-batch SQL strings;
- bounded source replacement, eager range-output, and publication receipt
  buffers; and
- independent expected-operation path storage passed into qualification.

The Q tracker deliberately excludes allocator metadata, rusqlite/SQLite
internals, SQLite page cache, mapped pages, filesystem cache, and kernel state.
Those remain separately `Unavailable`; no zero substitutes them.

SQL evidence now distinguishes statement-cache acquisition, query calls,
execute calls, rows returned, and rows changed.  Native SQLite prepare count
is explicitly `Unavailable`, because rusqlite's cache API does not expose the
complete native miss/prepare total.  The old `sql_preparations` label no longer
appears in new row or phase JSON.

The audit's higher-authority `w_bytes` and `d_bytes` fields were preserved
without redefinition.  The row now additionally reports:

```text
canonical_new_write_bytes
canonical_authenticated_nonnew_bytes
canonical_rewrite_bytes
covered_equal_edges
new_or_different_edges
fully_authenticated_new_objects
fully_authenticated_new_bytes
```

`canonical_authenticated_nonnew_bytes` is checked as authentication bytes
minus newly written canonical bytes.  It is not relabeled as D.

Campaign ingestion is now a structural top-level JSON parser.  It handles
nested objects, arrays, strings, and escapes; rejects duplicate/empty fields;
and cannot mistake nested/string lookalikes for top-level `status`, `warmup`,
or iteration values.  C0/C1 results are joined by exact size, operation, and
iteration before each signed paired effect and win is emitted.  Unpaired arm
medians are not used as causal effects.

## Fingerprints and custody

| Item | SHA-256 / value |
|---|---|
| branch | `codex/empty-worktree` |
| HEAD | `f3df30a80172131b74b5949a6a55234c962dac67` |
| cumulative tracked implementation diff | `640285dde7f3f5a84c0cb16a589b63020e5cb354acb0df3a7b3c257c018b44e0` |
| benchmark source | `976cba60408bc00e939f063add8bf427fc66857477158d3aea4eae2235eafe18` |
| engine manifest | `f2f17cf5d302dfeaab12c4b1d0b6af660c229cd737c773f3a5d417dcb2eb1242` |
| shared file-root decoder | `1e1803250fe91493c26844c35ed20c5979c2d27a85b7411799da6606ed5b5d03` |
| parity source | `2798b4973697e13deab8a45bfb1200118adc250d4568f6bac3b72450544ed47c` |
| debug executable | `871da6c46ff75d35ee45f253ddc9d717514c649eac021955d1c82a1cf9e3b4ce` |
| retained 100-MiB source | `63b3695b8c117b5bc39885e0df0dcd0af1d49e575482bab16577d84b4f40eff4` |
| retained fixture manifest | `8c64b5f49a10651e71fd52df3959cae22d291af4d95f47e43f7456308baad4ca` |
| independently prepared debug expectation | `81b2eaf5b5c0144fe945e2bd17228cee046e85ba021970eeeb0653cf8042a316` |

The current dirt remains preserved, including user-owned untracked notes that
appeared during the work.  None was overwritten or staged.  No file in
`/Users/yifanxu/Ephemeral-AI-Lab/layerfs` was modified, no commit was made,
and no release build occurred.

## Checked equations

Logical Q uses the exact allocation capacities supplied by `Vec::capacity`
or `String::capacity`:

```text
on charge(c):
  q_current' = checked_add(q_current, c)
  q_high_water' = max(q_high_water, q_current')

on scoped release(c):
  q_current' = checked_sub(q_current, c)

row hard gate:
  q_current = 0
```

The deliberate overlap test holds a parent file-child vector, canonical
buffer, and active-ID stack simultaneously and asserts:

```text
Q = parent.capacity * sizeof(FileChild)
  + canonical.capacity * sizeof(u8)
  + stack.capacity * sizeof(ObjectId)
```

It then releases child buffers, verifies the parent remains charged, releases
the parent, and separately forces an error while an output buffer is live.
Both success and error paths finish at exact `q_current = 0`.

Per-row accounting checks include:

```text
canonical_bytes_authenticated
  = canonical_new_write_bytes
  + canonical_authenticated_nonnew_bytes

sql_calls = sql_query_calls + sql_execute_calls
commits <= transactions
borrowed_row_blob_reads <= row_blob_reads
borrowed_row_blob_bytes <= canonical_bytes_authenticated
fully_authenticated_new_objects <= objects_authenticated
fully_authenticated_new_bytes <= canonical_bytes_authenticated
```

The direct generated-object SQL test observes exactly one cache acquisition,
one execute call, one changed row, zero query calls, and zero returned rows.
The structural row test freezes the independent example equation
`41 authenticated = 11 new + 30 authenticated-nonnew`, preserves synthetic
`W=101` and `D=202`, and separately records nine rewrite bytes, 127 covered
edges, and four new/different edges.

The actual one-change changed-spine fixture still reports:

```text
prior spine objects authenticated       4
replacement spine objects authenticated 4
covered equal edges                    127
new/different edges                      4
fully authenticated new chunks           1
COMMITs before qualification completion   0
q_current after success                   0
q_current after exact missing-object fail 0
```

The C0 then C1 shadow test compares logical/apparent/allocated database,
journal, and authority-sidecar endpoints immediately before and after both
read-only qualification arms and requires the complete physical snapshot to
be identical.  BLOB counters are observed zeros because neither arm invokes
SQLite's incremental BLOB API.  Native prepares, SQLite page-cache bytes,
sync/fsync calls, process/host physical I/O, and peak temp/journal usage are
`Unavailable`, never reported as observed zero.

## Commands and debug evidence

```text
cargo test -p layerfs-engine --bin phase4_create_edit_benchmark
  -> 18 passed; 0 failed

cargo test -p layerfs-core
  -> 44 passed; 0 failed

cargo test -p layerfs-engine --test phase4_engine_parity
  -> 12 passed; 0 failed

cargo test -p layerfs-engine --bin phase4_create_edit_benchmark \
  witnessed_changed_spine_authenticates_all_differences_before_commit \
  -- --nocapture
  -> 1 passed; 0 failed

cargo build -p layerfs-engine --bin phase4_create_edit_benchmark
  -> PASS (debug only)

target/debug/phase4_create_edit_benchmark --self-test \
  /tmp/layerfs-m45-m4-final.TqTUlM
  -> PASS; root=f1cfdd7fdd658506caf39ace169dd83f1a95fbb25aafcbb7f9b72864f9d2e42a;
     objects=20; auth_bytes=1,054,836

cargo fmt --all -- --check
git diff --check
  -> PASS
```

The temporary self-test input, SQLite image, and authority sidecar were
removed.  No release row, paired delta, CPU, RSS, peak footprint, or release
storage observation exists yet; all are **NotRun**.

## Before/after path and bounds

Before M4.5-4, Q was:

```text
33,604,696 fixed bytes + maximum one locally estimated semantic allocation
```

That equation could not represent overlapping parent/child canonical,
decoded, stack, SQL, source, and output capacities.

After M4.5-4, every instrumented owned buffer uses the same charge/decharge
mechanism, so recursive parent charges remain live while child charges are
added.  The declared bounds remain:

```text
same-count mutation          O(Xb + Xc + K + F*H)
C1 qualification             O(K + F*H + A_delta + V_delta + H^2)
resident candidate memory    O(H + K + bounded pages/chunks/SQL/output)
C0 / first authority / scrub linear in complete closure
fresh reconstruction         linear in source plus closure
+1                           suffix-linear
```

The ancestry scan remains deliberately bounded and quadratic in height; no
visited map or source-sized state was added.  Q measures capacity high-water,
not cumulative W/D work and not OS/SQLite memory.

## Defects and retain/revise/revert decision

The audit correctly found that the rejected-M4 Q and SQL names were not
evidence: simultaneous live allocations were not summed, early-return balance
was not proved, and cache acquisitions were mislabeled as native prepares.
The campaign summary also used string search and unpaired medians.  These
defects are now corrected in shared private paths with direct adversarial
tests.

Decision: **retain** M4.5-4.  All C0/C1 correctness, authority, atomicity,
identity, exact-Q, SQL-label, equation, structural-JSON, and read-only storage
gates required before timing now pass in debug.  Release performance remains
**NotRun** and M4.5-5 has not started.
