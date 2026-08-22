# G3-v2 pre-execution revision

Disposition: **REVISE_BEFORE_MEASUREMENT**

No v2 build, measured row, analyzer campaign child, or finalizer ran. At
classification time both
`target/phase4-g3-incremental-materialization-20260822-v2` and its `.lock` were
absent. Every previously frozen v2 file remains byte-for-byte unchanged.

## Exact custody defect

The v2 runner created honest source copies, but its later verification and both
analyzers/finalizer did not independently require every custody record to bind
the exact copy path
`source-custody-v2/<repository-relative-source-path>`. They also failed to
jointly prove that the path was relative and traversal-free, resolved inside
the source-custody root, was distinct from the original, had
`copy_size_bytes == size_bytes`, and retained the exact source hash in a `0400`
regular non-symlink copy.

A forged or corrupted custody JSON could therefore redirect verification to an
outside or original path, omit `copy_path`, or misstate copy size without every
independent consumer rejecting it. No campaign consumed the defective protocol.
v3 preserves the same Attempt-B candidate and nine-row schedule but makes the
source-copy proof exact in the runner, primary analyzer, independent analyzer,
and finalizer, with synthetic missing/escape/wrong-size mutations.

## Frozen v2 hashes

| File | SHA-256 |
|---|---|
| `PROSPECTIVE-G3-INCREMENTAL-MATERIALIZATION-v2.md` | `851b05a40be4fe662c0ed095762c160c2f62bf4b4a4ae2f4120f92b3531dd51a` |
| `COUNTER-DICTIONARY-v2.md` | `e517b1c5da8b225ec4aa70c616a08b04843f2d8eea745b97dfe3c2cea8d796ba` |
| `run_g3_v2.py` | `7c2a959deb4e71cdf004c8c1aefc14a8f48059e42bbfb8cc9b226a4eee27e527` |
| `analyze_g3_v2.py` | `f3e3587e367ae2407e529fc0fac635ebd855fb2d69cac247e93275ae8cf58919` |
| `recompute_g3_v2.py` | `12874b918a05d9762c75b6dac91c94618d75cddece0409f3988664330a95483d` |
| `finalize_g3_v2.py` | `1816b3997a56787dc77b18e37017bff6a0932a8256059ad7cd581435c604266d` |
| `DRY-RUN-v2.json` | `09c65d1baac092add92b419bfd504c430e63e58494d756000d2060a69cbe5608` |
| source-set digest | `c6095dac5864e53592f193fb130f8dbc676b37cdd9af5734337cd9071f366fa3` |
| methodology-set digest | `b60d97d5fe561ec14c6e220454556f9e26b91736ac7adb5f674e537570fe4464` |

The later product-source change is not retroactively attributed to v2. v3
freezes the final current source bytes only after all v3 method checks pass.
