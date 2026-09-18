# V1 receipt — `filesystem_timing_c1` prints `counters.validation`

> **Status:** Vehicle receipt (prerequisite V1 of the Phase 1 plan §1). No product,
> test or tool source is touched: an **example** gains one printed line. Written
> once, from [`after/`](after/) (commit `2b5e27e65`, the V1 commit) under
> [`../../CONTRACT.md`](../../CONTRACT.md). V1 has no #178 checkbox of its own —
> its deliverable is the observable P1-4 will be measured against, and it is
> reported on #178 as a prerequisite.

## 1. The gap it closes

Phase 0 recorded `ValidationWork` as **`NOT_EXPOSED` in every frozen vehicle**
(`p0-3-counter-baseline.md` §1, §7), which left P1-4 — "batch validate's lookups
+ memo pages across the three walks", whose whole acceptance is a movement in
`validation.objects_read`/`entries_examined` — without a before anchor. The
counters themselves were never missing: `FilesystemResult.counters.validation` is
public (`update.rs:52`, `ValidationWork` at `validate.rs:30`) and the example
already holds the result; it simply printed five of the six counter groups and
not this one.

## 2. The change

One additive `text.push_str` in
`core/crates/layerfs-content/examples/filesystem_timing_c1.rs`, after the
`references:` line:

```text
validation: objects <n> waves <n> inode_demands <n> inode_pages <n> directory_pages <n> entries_examined <n>
```

`ValidationWork`'s six fields, in declaration order. Under
[`../../CONTRACT.md`](../../CONTRACT.md) §2.1 a vehicle extension may only **add**
printed lines: the vehicle keeps its name, its cases, its parameters and every
field it printed before.

## 3. The arm and its artifact identity

`after/` was collected with the corrected driver (correction 2 in
[`../../ROUND-README.md`](../../ROUND-README.md)): `after/artifacts.txt` records
`filesystem_timing_c1` sha256 `39f5288d8a9ed01f…` and the probe client
`b0866eb1a9060618…`. Product tree: `core/crates` clean at `2b5e27e65`;
`LAYERFS_CONSTRUCTION_WORKERS=1`; release; `+1.85.1`; `--locked`; one sample per
workload. The full frozen set (D1–D29 + X1/X2) was collected, 33 commands, every
exit code in [`after/commands.tsv`](after/commands.tsv).

**The before value for the six new fields is `NOT_EXPOSED`** (Phase 0's recorded
gap) — there is no earlier number to compare against, and none is invented. What
*is* comparable is every field the vehicle printed before, and that comparison is
§5.

## 4. The baseline V1 creates (D1–D6, this tree)

| Row | `objects_read` | `read_waves` | `inode_demands` | `inode_pages_read` | `directory_pages_read` | `entries_examined` |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `c1.empty` (D1) | 0 | 0 | 0 | 0 | 0 | 0 |
| `c1.directory-update` (D2) | 21 | 42 | 21 | 42 | 0 | 0 |
| `c1.inode-update` (D3) | 0 | 0 | 0 | 0 | 0 | 0 |
| `c1.hardlink-move` (D4) | 6 | 6 | 6 | 6 | 0 | 0 |
| `c1.subtree-remove` (D5) | 0 | 0 | 0 | 0 | 0 | 0 |
| `c1.attributes` (D6) | 0 | 0 | 0 | 0 | 0 | 0 |

The two non-zero rows are the cases that resolve bindings through the base's
parent records: `directory-update` charges 21 inode demands in **42** waves
(≈2 waves per demand), `hardlink-move` 6 demands in 6 waves. `entries_examined`
is 0 on all six: none of the six frozen cases renames a directory, so the
cycle-check walk the field measures is never entered — P1-4's memo/anchor rows are
these six, and its movement will show in `objects_read`/`read_waves`/
`inode_demands`. This is the honest shape of the anchor: **three of the six fields
are exercised by the frozen set, and `entries_examined` is not** — P1-4's
bit-identical requirement for `entries_examined` is therefore trivially satisfied
on D1–D6 and must be checked on the suite's own directory-rename fixtures instead.

## 5. Additive-only, checked rather than asserted

The `c1-rebaseline/after` arm (C1's tree, same six cases, same vehicle *before*
this line existed) and this arm differ by exactly one printed line plus elapsed
figures. Command:

```sh
for i in 1 2 3 4 5 6; do
  diff <(sed -n '/--- stdout ---/,/--- stderr ---/p' ../c1-rebaseline/after/logs/D$i-*.log \
         | grep -vE '^(validation:|elapsed_ns |phase |report_nodes)') \
       <(sed -n '/--- stdout ---/,/--- stderr ---/p' after/logs/D$i-*.log \
         | grep -vE '^(validation:|elapsed_ns |phase |report_nodes)')
done
```

Exit 0 with empty output for all six rows: every field the vehicle printed before
— `root`, `inode_table`, `directories`, `objects read … emitted`, `directories:
pages …`, `inodes: pages …`, `references: …`, the `attribute*` lines,
`prepared_objects` — is byte-identical, so D1–D6 rows collected before V1 remain
comparable field by field. The only other differences in the raw diff are
`elapsed_ns`/`phase … elapsed_ns`, which are diagnostic (§8).

## 6. The eight checks (exit codes)

| # | Command | Exit |
| --- | --- | ---: |
| 1 | `python3 core/tools/check_product_boundary.py` | 0 (116 files) |
| 2 | `python3 -m unittest discover -s core/tools -p 'test_*.py'` | 0 (6 OK) |
| 3 | `python3 -m unittest discover -s tools -p 'test_production_loc.py'` | 0 (17 OK) |
| 4 | `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check` | 0 |
| 5 | `cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked --no-fail-fast` | 0 (436 passed, 0 failed) |
| 6 | `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --workspace --locked --all-targets -- -D warnings` | 0 |
| 7 | `python3 tools/production_loc.py --files` | 0 |
| 8 | `git diff --check` | 0 |

Sealed-oracle parity set: unchanged by V1 (no test file is touched), and re-run
green on the C1 tree in [`../c1-rebaseline/verify-c1.md`](../c1-rebaseline/verify-c1.md)
§1 (34/34).

## 7. Production LOC

`V1 | 0` was the plan's estimate; the actual is **0**.
`core` **18,793 → 18,793 (delta 0)**; `crates/` reference 65,417 → 65,417;
combined 84,210 → 84,210. Method: `tools/production_loc.py` over the first parent
(`7447f87d9`, via `git archive`) and the V1 tree; `examples/` is outside the
production scope, so the equality is structural, not a coincidence.

## 8. Determinism (labelled diagnostics)

`X1` (`order.forced64` repeat) and `X2` (`c1.subtree-remove` repeat) are
bit-identical to their gate samples on every work counter; `elapsed_ns` moved
(163.5 ms → 161.0 ms on D26; 310 µs → 296 µs on X2) and is diagnostic-only. No
V1 acceptance line is gated on elapsed.

## 9. Acceptance

- [x] The vehicle keeps its name, cases, parameters and previously printed fields
- [x] The new observable is printed from the public counters, with no product change
- [x] The full frozen set collected on the committed tree; exit codes and wall times recorded
- [x] `ValidationWork`'s six fields baselined for D1–D6; `NOT_EXPOSED` replaced by a number, not an estimate
- [x] Additive-only proven by diff against the pre-V1 arm (§5)
- [x] Eight checks green; parity untouched; LOC delta 0; no box ticked by this round
