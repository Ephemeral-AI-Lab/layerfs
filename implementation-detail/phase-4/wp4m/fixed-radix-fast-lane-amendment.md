# WP4-M fixed-radix fast-lane amendment

- Status: active specification; CP-0006 evidence PASS; WP4-M complete
- Effective: 2026-08-20
- Scope: WP4-M finalization and WP4-P eligibility only

## 1. Decision

The project accepts fixed ordinal file-reference grouping as an explicit
product tradeoff. K64/F64 is policy-selected for the compact WP4-M fast lane,
subject to the correctness, exact-model, resource, and evidence gates below.
Count-changing insertion or deletion remains suffix-linear; this amendment
makes no logarithmic or path-local claim for that operation.

DIR256K is retained through the unavailable-evidence fallback in section 6.
Neither choice has compatibility authority until WP4-P deletes the alternatives,
regenerates selected-only independent goldens, and completes its audits.

This is a new specification effective on the date above. It does not relabel,
repair, or promote evidence collected under an earlier contract.

## 2. Precedence and unchanged contracts

For work performed on or after the effective date, this amendment supersedes
only these earlier WP4-M requirements:

- a forced `+1` edit exceeding 5% of unchanged full-capture wall is no longer
  an automatic fixed-radix rejection;
- another three-profile file-ranking campaign is not required before WP4-P;
  and
- another three-ceiling directory-ranking campaign is not required before
  WP4-P when directory comparative evidence is unavailable as section 6
  defines.

The 5% forced-`+1` ratio remains a mandatory diagnostic. All other semantic,
authentication, atomic-publication, durability, error, timer-boundary, and
bounded-memory requirements remain in force.

This amendment changes no:

- canonical object bytes, mapping grammar, profile ID, root, delta, or
  `ObjectId` derivation;
- metadata field, SQLite schema, visible-head tuple, transaction boundary, or
  durability setting;
- CDC profile, raw `ChunkId`, reconstruction, range, closure, or receipt rule;
  or
- file-size admission, checked-arithmetic, object-size, logical-depth, or
  resource limit.

Candidate selectors and losing constants remain private and temporary. Their
deletion and the final selected-only golden regeneration belong to WP4-P.

## 3. Historical campaign disposition

The earlier 216-row, 252-database profile campaign remains a terminal `NO-GO`
under the contract that governed it. In particular, its forced-`+1` ratios of
61.997% through 71.417% failed the then-binding 5% rejection gate. The campaign
did not complete its required manifest, seal, external attestation, or final
audit.

The former approximately 65-GiB campaign root, including its raw rows,
databases, ledgers, analyzers, and custody inputs, is no longer present. The
remaining summaries are historical directional evidence only. They are not
recovered custody and must not be used as promotion-bearing input under this
amendment.

## 4. Declared formula-only 100-GiB analytical suffix bound

For the frozen retained-density model of a 100-GiB K64/F64 file with
`N = 5,410,816` references, the approved middle `+1` insertion budget is:

| Quantity | Exact model value and maximum accepted value |
|---|---:|
| rewritten reference occurrences | 2,705,409 |
| changed leaves | 42,273 |
| changed branches | 673 |
| new mapping objects, including the root | 42,947 |
| canonical mapping bytes | 186,891,342 |

The checked fixed-radix equations must reproduce all five values exactly. A
different value is a model failure even when it is below the numeric ceiling.
Exceeding any ceiling blocks WP4-M finalization under this amendment and
requires a separately approved format or policy specification.

The early `+1` insertion remains the known whole-suffix diagnostic, not the
approved middle-insert budget:

| Quantity | Exact early-insert diagnostic |
|---|---:|
| rewritten reference occurrences | 5,410,817 |
| changed leaves | 84,545 |
| changed branches | 1,343 |
| new mapping objects, including the root | 85,889 |
| canonical mapping bytes | 373,777,332 |

Both projections are work equations, not wall-time extrapolations. No
projected 100-GiB latency is an acceptance claim.

## 5. Compact K64/F64 evidence required to finalize WP4-M

Run only K64/F64 against deterministic 1-MiB, 10-MiB, and retained 100-MiB
fixtures. The six capture arms are exactly:

1. 1-MiB full-write capture and publication;
2. 10-MiB full-write capture and publication;
3. 100-MiB full-write capture and publication;
4. 100-MiB same-count middle edit;
5. 100-MiB forced `+1` early edit; and
6. 100-MiB forced `+1` middle edit.

The 1-MiB and 10-MiB arms are write/roundtrip scaling smokes only. CP-0003
shows that the 10-MiB middle workflow changes the reference count from 531 to
530, so this specification makes no same-count or other edit-classification
claim at 10 MiB. The retained CP-0004 supplies the prior workflow baseline;
the routine lane does not relabel either checkpoint.

Use one untimed warmup and three measured invocations per row. Only the declared
capture or edit publication work enters these medians:

```text
6 capture arms * (1 warmup + 3 measured) = 24 invocations
```

Then run one separately labeled full-write complete-roundtrip check per size.
Each check performs capture/publication, closes and reconstructs a fresh engine,
authenticates the complete required closure, streams and fingerprints the full
file, and verifies exact ranges. These three checks are correctness evidence
outside every acceptance median:

```text
24 capture invocations + 3 complete-roundtrip checks = 27 total invocations
```

The externally measured routine-package wall starts immediately before the
first invocation is dispatched and ends only after the 27th invocation
returns. The complete package must finish in at most 120 seconds. Binary build,
fixture generation, and manifest preflight occur before this package wall; no
scheduled row or correctness check may be omitted to meet it.

Every invocation is isolated, uses the existing timer boundaries, and emits:

```text
qualification=false
purpose=fixed_radix_acceptance
milestone=WP4-M-FIXED-RADIX
candidate=K64-F64
promotion=false
```

The three complete-roundtrip checks additionally emit:

```text
row_kind=roundtrip-check
validation_scope=complete-roundtrip
throughput_measurement_admissible=false
```

Warmup and measured rows may perform final semantic checks after their measured
boundary, but that verification time is not folded into the fast-lane medians.

The compact evidence passes only when:

- all canonical identities, closure, reopen, reconstruction, range, edit,
  one-transaction/one-COMMIT, prior-head failure, and typed-error checks pass;
- every scheduled arm's topology and rewrite counter agrees exactly with the
  fixed-radix equations for that row's authenticated CDC sequence;
- the measured 1-to-10-to-100-MiB full-write scaling is reported without a
  logarithmic claim;
- the 5% forced-`+1` ratios are reported as diagnostics and do not decide the
  result;
- logical `Q` remains within the existing bound, returns exactly to zero, and
  is independent of source or rewritten-suffix size; cumulative `W` and `D`
  remain checked telemetry rather than resident allocation;
- unavailable RSS, APFS, physical-I/O, or cache observations are labeled
  `Unavailable`, never replaced by zero or logical bytes; and
- section 4's exact 100-GiB middle and early equations are reproduced, with
  the middle row within every declared analytical ceiling; and
- all 27 invocations return successfully within the hard 120-second package
  wall.

The retained 512-MiB fixture is occasional scale evidence only. It is outside
the routine lane, outside WP4-M closure, and never required to make WP4-P
eligible. Any later 512-MiB run is separately labeled and cannot replace a
routine row.

The retained 500.000-ms and 333.333-ms full-capture values and all edit wall
times remain credibility diagnostics. They are not WP4-M fast-lane blockers
and cannot support a 200/300-MiB/s product claim.

Passing this compact evidence is sufficient to mark WP4-M complete and make
WP4-P eligible. It is a validation of the policy-selected default, not a new
multi-profile ranking.

## 6. DIR256K unavailable-evidence fallback

The deleted historical campaign cannot provide promotion-bearing directory
comparisons. Record the DIR64K, DIR256K, and DIR1M comparative campaign evidence
as `Unavailable(custody_lost)`.

WP4-M therefore retains DIR256K by the predeclared unavailable-evidence
fallback. This is a policy fallback, not a claim that DIR256K measured faster
than the alternatives. No new wide-directory performance campaign is required
to finalize WP4-M.

Directory canonical correctness, bounds, strict ordering, greedy partitioning,
lookup, replacement, insertion, authentication, and typed malformed-input
requirements remain unchanged. WP4-P must delete DIR64K and DIR1M selectors and
fixtures, regenerate independent DIR256K success and malformed goldens, and
complete the selected-only audit.

## 7. Claims and handoff boundary

The promoted documentation and implementation must state:

```text
same-count local edit: path-local under the existing K64/F64 contract
count-changing edit:   suffix-linear O(Z), worst case Theta(N)
```

No receipt, batching change, benchmark result, or 100-GiB analytical budget
changes that complexity. A future history-independent/prolly mapping requires
a separately approved canonical-format specification and is not required by
this fast lane.

WP4-P begins only after the 27-invocation package passes, its compact report and
hashes are retained, and an independent read-only audit agrees. WP4-P, not this
amendment, owns deletion of losing profiles, selected-only golden identities,
the final specification fingerprint, and compatibility promotion.
