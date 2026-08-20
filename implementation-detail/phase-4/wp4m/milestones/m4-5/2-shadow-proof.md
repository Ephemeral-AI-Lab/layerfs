# WP4-M M4.5-2 — transaction-witnessed proof, independent oracle, and C0/C1 shadow

- Verdict: **PASS for debug correctness/activation shadow**.
- Release performance: **NotRun**.
- Decision: retain the changed-spine implementation and advance only to the
  durability/provenance milestone.  C1 release activation remains blocked on
  M4.5-3/4.
- Scope: private benchmark `Store` shadow, K64/F64 same-count edit only.  This
  is not production `Engine` integration, profile selection, full-create gain,
  qualification, promotion, or rejection.
- Labels remain `qualification=false`, `promotion=false`, and
  `rejection=false`.

## Independent-audit corrections incorporated

The independent audit found that the first draft bundled substrate changes
with the changed-spine algorithm and lacked an activation shadow.  M4.5 now
defines three explicit arms:

```text
A0 = frozen historical M3 executable/evidence
C0 = corrected M4.5 substrate + ordinary full pre-COMMIT closure
C1 = byte-identical C0 substrate + changed-spine qualification
```

`WP4M_M45_QUALIFICATION_MODE=full-closure|changed-spine` selects C0 or C1 in
the same private executable.  The database, oracle, edit, publication,
reconciliation, error, counter, and post-COMMIT paths are otherwise identical.
M4.5-5 must calibrate C0 A/A noise, use C0/C1 for the causal result, and report
A0/C0 substrate tax plus A0/C1 cumulative continuity separately.  No current
timing is attributed to the algorithm.

## Fingerprints and artifacts

| Item | SHA-256 / value |
|---|---|
| branch | `codex/empty-worktree` |
| HEAD | `f3df30a80172131b74b5949a6a55234c962dac67` |
| cumulative tracked implementation diff | `53d014809dd15d2f07b2861a96116cc9f9b9a6ed57dac1bcf56a12b43b6b0ecf` |
| benchmark source | `0456148b92e2f06b5e03da2cce632e036528cac4f0edbc7061adb0b52e8a8d75` |
| shared file-root decoder | `1e1803250fe91493c26844c35ed20c5979c2d27a85b7411799da6606ed5b5d03` |
| parity source | `2798b4973697e13deab8a45bfb1200118adc250d4568f6bac3b72450544ed47c` |
| debug executable | `e9030eab8b93b919614450c40c114bbc3202d26f025a13da4648bcd7b782c65e` |
| retained 100-MiB source | `63b3695b8c117b5bc39885e0df0dcd0af1d49e575482bab16577d84b4f40eff4` |
| retained fixture manifest | `8c64b5f49a10651e71fd52df3959cae22d291af4d95f47e43f7456308baad4ca` |
| independently prepared debug expectation file | `81b2eaf5b5c0144fe945e2bd17228cee046e85ba021970eeeb0653cf8042a316` |
| expectation file size | 76,585 bytes; inserted and removed byte strings are each bounded to at most 32 KiB |

The debug expectation artifact is
`target/wp4m-opt2-k64-20260818/db-K64-F64-104857600-same-middle-200.sqlite.expectations`.
It is correctness evidence, not a measured row.  Its separate oracle database
and authority sidecar were deleted after preparation; the measured prepared
database was closed before the disposable oracle image was created and was not
opened or mutated by oracle construction.

## Oracle custody and exact pre-COMMIT binding

Preparation uses a separate disposable SQLite database image.  It independently:

1. reconstructs the exact retained base from the retained source;
2. publishes and validates that base in the oracle image;
3. performs a full rebuild of the edited reference stream, not changed-spine
   COW;
4. fully validates source bytes, ordered CDC, transition, root, and closure;
5. writes only bounded immutable expectation data to the prepared manifest;
6. rolls back/removes the oracle image and sidecar; and
7. never copies oracle objects or authority into the measured database.

The measured child reads and checksum-validates the fixed manifest before
`BEGIN IMMEDIATE`, byte-compares the exact bounded removed and inserted bytes,
then retains only constant-size expected result IDs/closure during the edit.
Before COMMIT both C0 and C1 require the exact operation fields and the exact
expected root and transition.

The retained 100-MiB debug preparation reproduced the frozen values exactly:

```text
base source fingerprint   bb883eecf4ea85d80432953791dcc352243da94175e7503e2c476afe9bd0bab7
edited fingerprint        a4aaf02c293df75c63072af86264908183c6e213997cf677b63f75d8a9819e3e
edited references         5,284
edited CDC fingerprint    58b61bbd4f319ecb6011278ca42caf2b5d696e42b4655c054c48b3906d017b83
edit ordinal              2,642
edit offset               52,480,416
removed/inserted length   18,854 / 18,854 bytes
result root               cc8f31adc20eaa56b621744fe45f90f65fb9ac6177446d33b0052d7ebd404560
result transition         2686d6ffc512b38f64922073dcc191a1ff1c7eacedb1c73e0a72045bf7cf4a92
result closure            7b7142f5e203ae23efd46662efe576a182f8043c4323f487407bbb031b7cc2bb
```

After publication, the code still drops every handle, reopens independently,
performs a fresh full scrub, reconstructs the complete edited source, recomputes
the source and ordered CDC fingerprints, and verifies all ranges.  Post-COMMIT
verification is not used to excuse a pre-COMMIT oracle mismatch.

## Changed-spine implementation and complete summaries

C1 is reachable only with the transaction-owned permit from M4.5-1.  It:

- authenticates prior and replacement namespace roots;
- authenticates both file roots and compares mode, length, reference count,
  level, child count, candidate profile, and every cumulative descriptor;
- authenticates both prior/replacement mapping nodes on every different path;
- treats an edge as covered only when its exact `ObjectId` and complete parent
  descriptor agree;
- recursively follows every different mapping edge;
- fully loads/authenticates every new chunk and checks exact raw length and raw
  `ChunkId`;
- validates fixed K/F fullness, final partial groups, minimal height, checked
  lengths, cumulative ends, active cycles, roles, and exact EOF through shared
  decoders/validators; and
- accepts no result until the independently prepared operation/root/transition
  binding passes.

Active-cycle membership uses bounded ancestry vectors, so its declared
qualification bound includes the honest `H^2` scan term.  No visited map,
cache, source vector, new schema, new receipt, worker, pool, WAL, append-only
path, or pack was added.

## C0/C1 activation-shadow evidence

The explicit debug shadow runs ordinary full qualification and incremental
qualification against the same transaction/result and requires equal
accept/reject classification.

| Case | C0 full closure | C1 changed spine | Evidence |
|---|---|---|---|
| valid one-change | accept | accept | exact source/CDC/root/transition/closure |
| missing new chunk | reject exact ID | reject exact ID | `CandidateError::MissingObject(replacement_id)` |
| multiple changed children | accept | accept | changes cross both root branches |
| final partial leaf | accept | accept | second change is in the four-reference final leaf |
| malformed cumulative summary | reject | reject | full and incremental validators both fail before COMMIT |
| forged mode | reject | reject | C1 complete-summary comparison plus C0 expected-root oracle |

For the retained one-change topology, C1 observes exactly:

```text
prior spine objects authenticated       4
replacement spine objects authenticated 4
receipt-covered equal edges            127
new/different edges                      4
fully authenticated new chunks           1
```

The multi-change/final-partial test observes six prior and six replacement
spine objects, seven different edges, and two fully authenticated new chunks.
All cases assert zero COMMITs and the byte-identical prior complete head after
failure.

## Commands and correctness gate

```text
cargo test -p layerfs-engine --bin phase4_create_edit_benchmark \
  tests::witnessed_changed_spine_authenticates_all_differences_before_commit \
  -- --exact --nocapture
cargo test -p layerfs-engine --bin phase4_create_edit_benchmark \
  tests::witnessed_spine_handles_multiple_children_final_partial_leaf_and_mode \
  -- --exact --nocapture
cargo test -p layerfs-engine --bin phase4_create_edit_benchmark \
  tests::full_and_incremental_shadow_both_reject_malformed_summary \
  -- --exact --nocapture
cargo test -p layerfs-engine --bin phase4_create_edit_benchmark
  -> 14 passed; 0 failed
cargo test -p layerfs-core
  -> 44 passed; 0 failed
cargo test -p layerfs-engine --test phase4_engine_parity
  -> 12 passed; 0 failed
cargo build -p layerfs-engine --bin phase4_create_edit_benchmark
  -> PASS
target/debug/phase4_create_edit_benchmark --self-test <mktemp directory>
  -> PASS; root=f1cfdd7fdd658506caf39ace169dd83f1a95fbb25aafcbb7f9b72864f9d2e42a;
     objects=20; auth_bytes=1,054,836
target/debug/phase4_create_edit_benchmark --prepare-row \
  target/wp4m-opt2-k64-20260818 K64-F64 104857600 same-middle 200
  -> PASS; exact frozen result identities reproduced
cargo fmt --all -- --check
git diff --check
  -> PASS
```

## Work/resource equations and classifications

The changed-spine edge equation for the one-change fixture is:

```text
4 different edges
  = namespace -> file root
  + file root -> changed branch
  + branch -> changed leaf
  + leaf -> new chunk

127 covered equal edges
  = 1 equal root child + 63 equal branch children + 63 equal leaf references
```

The two-change equation is:

```text
7 different edges
  = 1 namespace edge + 2 root edges + 2 branch edges + 2 chunk edges
```

Release CPU, RSS, peak footprint, paired wall deltas/wins, physical I/O,
sync/fsync, journal/temp high-water, and endpoint storage comparisons are
**NotRun**.  No release build occurred.  The current Q output remains the old
max-local/fixed-envelope diagnostic and is not accepted as exact; M4.5-4 must
replace it with checked summed live charge/decharge returning to zero.  SQL
prepare/acquisition/query/execute counters are likewise not yet acceptance
evidence and remain an M4.5-4 blocker.  W/D retain their higher-authority
definitions; this milestone does not redefine them.

## Before/after algorithm and memory bounds

Before corrected M4.5:

```text
same-count mutation O(Xb + Xc + K + F*H)
pre-COMMIT qualification Theta(full closure)
```

After C1:

```text
same-count mutation      O(Xb + Xc + K + F*H)
changed-spine qualification
                         O(K + F*H + A_delta + V_delta + H^2)
resident semantic shape  O(H + K + bounded pages/chunks/SQL/output)
```

C0 intentionally remains full-closure linear.  Initial transaction authority,
fresh scrub, reconstruction, and complete first-open lifecycle remain linear.
`+1` remains suffix-linear.  These are algorithm bounds, not release timing or
production claims.

## Defects and retain/revise/revert decision

M4.5-2 closes the original cross-reopen authority, incomplete mode summary,
unbound pre-COMMIT result, oracle self-comparison, and missing-ID defects for
the changed/new SQLite read path.  The independent audit still blocks release
on direct actual-COMMIT-error reconciliation tests and exact provenance
(M4.5-3), and exact Q/SQL/W/D/structural-JSON gates (M4.5-4).

Decision: **retain** C0/C1 and the separate-image oracle.  Advance to
M4.5-3 only; release performance remains **NotRun**.
