# WP4-M fixed-radix compact evidence contract

- Status: authorized fast-lane acceptance contract
- Date: 2026-08-20
- Scope: private K64/F64 evidence only; `qualification=false` and
  `promotion=false`

This contract closes the fixed-radix measurement question without recreating
the deleted multi-profile campaign. It accepts suffix-linear count-changing
edits as an explicit product limitation. It does not claim that the former
forced-`+1` 5% gate passed.

## Evidence set

The required JSONL contains exactly 24 capture rows:

```text
write at 1/10/100 MiB:                         3 arms
edit-same/+1-early/+1-middle at 100 MiB only: 3 arms
6 arms * (1 warmup + 3 measured):             24 rows
row_kind: warmup(sample_index=0), measured(sample_index=1..3)
```

CP-0003 proves that the 10-MiB same-middle fixture changes the CDC count from
531 to 530. It is therefore not a same-count arm. All edit rows at 1 or 10 MiB
are inadmissible and must be rejected rather than relabeled.

Exactly three additional `roundtrip-check` write rows are required: one per
size, `sample_index=null`, and `validation_scope=complete-roundtrip`. They are
correctness checks and never enter medians or the 24-row count. The complete
routine evidence set is therefore exactly 27 rows: 6 warmups, 18 measured
capture rows, and 3 roundtrips.

The runner must emit `runner_wall_ceiling_seconds=120` and
`runner_command_ceiling_seconds=60` on every row, enforce the smaller of the
per-command ceiling and remaining campaign wall, and fail without publishing a
closure result if the 120-second ceiling expires. A 512-MiB run is only an
occasional separately labeled scale check; it is not part of these 27 rows and
cannot close or reopen WP4-M.

Every row has schema `wp4m-fixed-radix-acceptance-row-v1`, purpose
`fixed_radix_acceptance`, milestone `WP4-M-FIXED-RADIX`, candidate `K64-F64`,
and profile ID
`cbf5709c59629c812a6ed3e9ea94a9226deab71547d2ab6c0fca596ccfe357e9`.
The public operation maps respectively to engine operation `full`,
`same-middle`, `plus1-early`, or `plus1-middle`.

## Compact custody manifest

No database is retained. The JSONL itself carries the small custody manifest:

- one lowercase SHA-256 `executable_sha256` and `runner_sha256` for the whole
  evidence set;
- one `source_fingerprint` and `(fixture,fixture_sha256)` per size;
- one pre-edit database, authority, and expectations SHA-256 tuple per
  size/edit operation; and
- exact `root_id`, `transition_id`, and `ordered_closure_digest` per
  size/operation.

All four rows in an arm must agree on the applicable custody and result
identities. Every roundtrip result must equal its same-size write
identity. The final report records the raw JSONL SHA-256 plus the executable
and runner hashes. Temporary sources, bases, databases, and fixture manifests
may be deleted after JSONL and analysis custody is sealed.

The deleted 216-row/65-GiB campaign is not an input, baseline, or custody
dependency. Its historical summaries cannot satisfy any check here.

## Hard row checks

Every campaign and roundtrip row must be `PASS` and preserve:

```text
qualification=false
promotion=false
transactions=1
commits=1
commit_dispatches=1
commit_returns=1
commit_return_successes=1
commit_return_errors=0
q_current=0
commit_timer_equation_matches=true
durable_phase_sum_matches=true
actual_cdc_references=expected_cdc_references
```

Publish and complete-lifecycle timers must be positive. Warmups are validated
but excluded from statistics. Each measured arm reports the three-value median,
minimum, maximum, and spread for `capture_publish_wall_ns` and
`complete_lifecycle_total_wall_ns`. Full-write analysis reports exact 1-to-10,
10-to-100, and 1-to-100-MiB ratios for those medians and mapping bytes. Edit
medians, suffix counters, and alarms are reported only for the proven 100-MiB
fixture.

## Fixed-radix suffix model

Every forced-`+1` row carries:

```json
{
  "kind": "ordinal-fixed-radix-suffix-linear-v1",
  "old_references": 5284,
  "insertion_ordinal": 2642,
  "rewritten_references": 2642,
  "rewritten_raw_bytes": 52377184,
  "authenticated_objects": 86,
  "rewritten_pages": 42,
  "rewritten_branches": 2,
  "rewritten_mapping_bytes": 185915
}
```

The numbers above illustrate the retained 100-MiB middle fixture. The analyzer
requires the embedded model to equal the engine's exact `suffix_references`,
`suffix_bytes`, `suffix_objects`, `pages`, `branches`, and
`mapping_bytes_rewritten` counters in every row. It independently checks:

```text
early insertion ordinal  = 0
middle insertion ordinal = floor(old_references / 2)
source suffix references = old_references - insertion_ordinal
rebuilt occurrences       = source suffix references + inserted reference
changed leaves/branches   = exact K64/F64 ordinal regrouping
mapping objects           = changed leaves + changed branches + file root
```

`authenticated_objects` is an engine traversal counter; it is not relabeled as
the number of new mapping objects.

The old local 5% forced-`+1` ratio remains reported as a nonbinding alarm. A
high ratio does not invalidate this evidence because this contract explicitly
accepts suffix-linear count-changing work.

## Approved 100-GiB middle-insert budget

Both analyzers contain and emit this small immutable policy constant:

```json
{
  "approved": true,
  "old_reference_count": 5410816,
  "insertion_ordinal": 2705408,
  "rebuilt_reference_occurrences": 2705409,
  "changed_leaves": 42273,
  "changed_branches": 673,
  "mapping_objects": 42947,
  "canonical_mapping_bytes": 186891342,
  "latency_projection": false
}
```

These are exact analytical work limits, not measured 100-GiB latency and not a
file-size admission limit. The accepted limitation is Theta(suffix reference
occurrences) for count-changing edits. Same-count COW remains path-local.

## Independent analysis

Run both stdlib-only analyzers and require semantic agreement:

```sh
python3 implementation-detail/phase-4/test/analyze-phase4-fixed-radix.py RAW.jsonl > python-analysis.json
ruby implementation-detail/phase-4/test/analyze-phase4-fixed-radix.rb RAW.jsonl > ruby-analysis.json
```

Each emits one compact deterministic JSON object with `status=PASS|FAIL` and
sorted failure reasons. `PASS` means the evidence is complete and internally
consistent under this contract; it is not a throughput qualification or format
promotion.

DIR256K remains the frozen directory default only because directory comparison
evidence is unavailable. This is an explicit unmeasured fallback, not a
measured win. Selected-only directory goldens belong to WP4-P.
