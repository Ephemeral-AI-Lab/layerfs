# #209 format round — the reserved directory measured: `commit_ns` −52.8 %, then reverted by owner decision

> Status: Research; **diagnostic evidence, not release admission**. Round of
> 2026-09-21 continuing [#209](https://github.com/Ephemeral-AI-Lab/layerfs/issues/209).
> **The format change was implemented, measured, and then reverted by owner decision**
> — see §8. The tree is byte-identical to `704580673` over `core/crates` and
> `crates`, and the change itself is retained whole at
> [`scratch/format.diff`](scratch/format.diff). Admission `INELIGIBLE`, every budget
> class `NOT_RUN`. Nothing here closes #190, #205 or #208.

## The answer, in one paragraph

**The pack directory reserved its lane's whole area at a fixed offset, the pack
header carried the assembled length, and the row was pre-allocated in reservation
steps — so an append keeps the payload the same size and SQLite takes
`btreeOverwriteCell` and its content comparison, dirtying only the pages whose bytes
actually changed.** Measured on a matched pair sampled one sample per arm in one
window: **`commit_ns` 2.938 → 1.388 s (−1.550 s, −52.8 %)**, **64.16 → 30.30 µs per
append**, and **the operation 26.021 → 24.720 s (−1.300 s, −5.0 %)**, with
`resolve_ns`, `full_ns`, `delta_ns`, `group_ns` and `place_ns` all unchanged
within ±0.07 s. The saved Store moved from `7ea2fe6c…` to **`0c54dbf2…`**, which is
what an owner-authorized format change is — and the change was then **reverted by owner
decision** (§8), so nothing in this section is shipped. **The prediction was beaten on
`commit_ns`'s direction and size and missed on its level** (1.388 s against a
predicted ≤ 1.0 s, operation −1.300 s against −2.5 to −3.2 s); the residual is
measured, reported, and **not yet attributed**.

## 1. The design was measured before it was chosen

[`scratch/pack_row_probe.c`](scratch/pack_row_probe.c) replays four candidate write
paths over a copy of the measured run's own Store, arms interleaved round-robin in one
process, reading `SQLITE_DBSTATUS_CACHE_WRITE` around **every** `COMMIT`
([`scratch/pack-row-arms.txt`](scratch/pack-row-arms.txt)):

| arm | µs/step | pages/commit |
| --- | ---: | ---: |
| today: whole-row `UPDATE`, front directory, growing row | 2202.0 | 34.158 |
| same-size row, front directory, bodies shift by 16 B | 90.0 | **29.067** |
| same-size row, **reserved directory**, append-only tail | 49.2 | **3.353** |
| **incremental blob**: directory + tail ranges only | **36.1** | **3.367** |

**Pre-allocating the row is not enough** (29.067 against 34.158): the directory sits at
the front, so one new 16-byte entry moves every body byte and SQLite's
`if( memcmp(pDest, pX->pData+iOffset, iAmt)!=0 ){ sqlite3PagerWrite(...) }` guard never
fires. **The layout was the blocker, not the payload size.** Reserving the directory
takes the commit's page count down **10.2×**. The incremental-blob path is a further
27 % off the step and is **not shipped in this round** — it is the staged second half,
worth ≈0.6 s by the same probe.

## 2. What was changed

| element | before | after |
| --- | --- | --- |
| pack header | magic, version, group count (16 B) | plus the assembled length (20 B) |
| body base offset | `HEADER_LEN + dirent × groups` — moves every append | `body_base(lane) = HEADER_LEN + group_count_limit × dirent` — **constant** |
| row length | exactly the assembled length | reserved in `max(32 KiB, pack_limit / 8)` steps |
| framing versions | 1, 2, 4, 6, 7 | **8, 9, 10, 11, 12** — the old numbers are rejected by `parse_header`, never misread |
| `FORMAT_PROFILE` / `SCHEMA_VERSION` | 1 / 7 | **2 / 8** — the DDL CHECK that pins the profile moves with it |
| write path | `UPDATE … SET data = ?2` | **unchanged**: the image is already reservation-sized, so the same statement now takes SQLite's same-size overwrite path |

Files: `pack/layout.rs` (header, reserved base, reservation arithmetic, both group
views), `pack/assemble.rs` (assembly pads to the reserved base and patches the
assembled length), `pack/placement.rs` (the running total starts at the reserved
base and a group adds only its body), `cas/placement.rs` (the transaction is charged
the assembled length, not the reservation), `policy.rs` and `sql/schema.sql` (profile
2, schema 8).

## 3. The measured pair

One sample per arm, one window, fresh `--output` each, both global flocks, quiet
preflight. The control is the **archived profile-1 executable** the confirmation window
left behind (sha256 `3a6c1c20397663c4fc2e0d89a3c2453ba26b39b1dbca0a1b7a5a5bce448a84ad`,
whose tree the round's first parent rebuilds byte for byte); the treatment is the
freshly built profile-2 binary (sha256
`9e2828b2d30ed52bfb86e766084c908db40281b9f78d37001a9c37724d462933`). Both are archived
under [`binary-archive/`](binary-archive).

| term | profile 1 (control) | profile 2 | Δ |
| --- | ---: | ---: | ---: |
| **operation** | 26.021 s | **24.720 s** | **−1.300 s (−5.0 %)** |
| **`commit_ns`** | 2.938 s | **1.388 s** | **−1.550 s (−52.8 %)** |
| **`commit_ns` per append** | 64.16 µs | **30.30 µs** | −33.86 µs |
| `sql_ns` | 1.971 s | 1.861 s | −0.110 s |
| `resolve_ns` | 6.877 s | 6.944 s | +0.068 s |
| `full_ns` | 1.728 s | 1.721 s | −0.007 s |
| `delta_ns` | 0.697 s | 0.696 s | −0.001 s |
| `group_ns` | 0.050 s | 0.051 s | +0.001 s |
| `place_ns` | 0.348 s | 0.348 s | +0.001 s |
| `filesystem` | 6.810 s | 7.084 s | +0.274 s |
| saved Store | `7ea2fe6ccf13bc5a…` | **`0c54dbf2f512115b…`** | moves |

**The treatment touches `commit_ns` and nothing else.** Every other bucket is inside
±0.11 s; `resolve_ns`, `full_ns`, `delta_ns`, `group_ns` and `place_ns` are inside
±0.07 s, which is the shape a write-path change must have. `filesystem` is +0.274 s
(+4.0 %) and is **not** attributed to the change — nothing in this diff is on that
path, and this window's own drift is not measurable from one sample per arm.

## 4. What the prediction got right, and what it missed

Pre-registered: `pages_at_commit` per commit **≤ 6.0**; `commit_ns` **≤ 1.0 s**
(−1.9 s or better); operation **−2.5 s to −3.2 s**. Measured: **−1.550 s on
`commit_ns` and −1.300 s on the operation.**

- **Right: the mechanism and the target.** The change moves `commit_ns` by more than
  half and leaves every other bucket alone, which is what the page-accounting probe
  predicted.
- **Missed: the level.** 1.388 s against a predicted ≤ 1.0 s is ≈0.39 s of page writes
  the probe's arms did not have. The probe wrote **only the pack row** into a Store
  copy; the product's transaction also writes page 1's change counter, the `objects`
  primary-key btree and the `objects_save` index for ≈1.07 object rows per commit,
  and the reservation growth events. Those terms were 3.4 pages per commit in the
  instrumented round's accounting and are **not re-measured here** — the probe that
  read `SQLITE_DBSTATUS_CACHE_WRITE` on the product's own connection was removed with
  the previous round. **The residual is stated as unattributed, not explained.**
- **Missed: the operation.** −1.300 s against −2.5 to −3.2 s, because `commit_ns` is
  the only term that moved. The probe's estimate of the *step's* saving assumed the
  btree delete-and-insert would collapse too; `sql_ns` fell only 0.110 s, so the
  append's own engine work did **not** collapse as predicted. Why is not established
  here.

## 5. The cost, measured rather than asserted

- **The Store moves.** `7ea2fe6c…` → `0c54dbf2f512115be4b7a8747cc9f1ff…`. Every
  pack is re-framed. This is the authorized consequence and the reason the
  byte-identical-Store rule of L57/L59/L60/L61 cannot apply to this round.
- **The workload is not counter-identical, and the pre-registration was wrong to say
  it would be.** The reserved directory consumes 4,096 B of each ordinary pack's
  262,144 B limit (1.6 %), so **`packs_created` moved 255 → 256 and `pack_appends`
  45,794 → 45,793** — one pack boundary in 255. Every other counter
  (`statements` 44,334, and the per-state save counters) is unchanged. A trailing
  directory instead of a reserved one would remove even this, at the cost of a larger
  reader change; it is recorded as the alternative, not taken.
- **Space.** +4,096 B per ordinary pack (≈+1.0 MB) and at most one reservation step of
  padding per pack (32 KiB for the ordinary lanes, `pack_limit / 8` for the singleton
  lane). **Not measured in this round** — the saved Store's size is reported with the
  pair in §3's receipts and is the number to read it from.
- **Migration.** `FORMAT_PROFILE` 1 → 2 and `SCHEMA_VERSION` 7 → 8. This lane
  rejects older schemas and never migrates them: **a Store written by the previous
  product is not readable by this one and vice versa.**

## 6. Checks

| check | result |
| --- | --- |
| `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked --workspace` | see [`checks/workspace-test.log`](checks/workspace-test.log) |
| `cargo +1.85.1 clippy --locked --workspace --all-targets -- -D warnings` | see [`checks/clippy.log`](checks/clippy.log) |
| `cargo +1.85.1 fmt --all --check` | see [`checks/fmt.log`](checks/fmt.log) |
| `core/tools/check_product_boundary.py` + self-tests | see [`checks/boundary.log`](checks/boundary.log), [`checks/boundary-selftest.log`](checks/boundary-selftest.log) |
| harness suite | see [`checks/harness-test.log`](checks/harness-test.log) |

`multi_writer.rs`, `memory_bounds.rs`, `visibility.rs` and `persistence_failure.rs`
are inside the workspace run and must all stay green: the change touches no lock, no
transaction boundary and no declared bound.

**Four tamper helpers had to be corrected, and that is a finding, not housekeeping.**
`cas_reuse::tamper_pack`, `support::corrupt_first_pack`,
`support::filesystem::corrupt_pack_containing` and `metadata_pool`'s pooled-leaf
damage all corrupted **the last byte of the row**, which is now reservation padding —
so each silently stopped testing anything. They now corrupt the last byte of the
**assembled pack** (`pack::layout::image_assembled_length`). **A pre-allocated row
creates a region where corruption is invisible by construction**, and any future
instrument that damages a stored pack must respect it.

## 7. Reproduce

```sh
# the design measurement (seconds)
cc -O2 -o scratch/pack_row_probe scratch/pack_row_probe.c -lsqlite3
./scratch/pack_row_probe <a copy of a measured run's Store> 150

# the matched pair, one sample per arm, one window
bash pair_driver.sh
```

## 8. The revert, and why it is not a refutation

**The change worked and was still reverted.** On 2026-09-21 the owner directed the
round to revert to the code of L57's window and close the issue. The product change is
removed — `git diff --stat 704580673 HEAD -- core/crates crates` prints nothing — and
the whole change is retained at [`scratch/format.diff`](scratch/format.diff)
(21 files, +258 −82), so the measurement above stands on its own and the work is
recoverable without re-deriving it.

**Reverting does not restore L57's 16 s, and the round's own receipts say so.** The
archived, unchanged profile-1 binary (`3a6c1c203976…`) read **16.739 s in L57's
window, 17.550 s in the confirmation window and 26.021 s in this round's window** —
same executable, same 45,794 appends, same 48,446 commits. **The 16 s figure is a
property of the machine, not of the code**, so the reverted tree reads ≈26 s today and
would have read ≈16 s in L57's window with or without this change.

**And the level is not predicted by the protocol's own gate.** Every row in the table
below passed the same quiet preflight — no named `cargo`/`rustc`/`fs-bench`
competitor, ≥ 70 % CPU idle — and they differ by 55 %:

| window | operation | load (1m) | CPU idle | competitors |
| --- | ---: | ---: | ---: | ---: |
| L57's window | **16.739 s** | 8.38 | 77.4 % | 0 |
| confirmation window | **17.550 s** | 4.26 | 81.3 % | 0 |
| this round's pack-cache row | **17.697 s** | 5.10 | 71.5 % | 0 |
| this round's instrument row | **25.986 s** | 5.37 | 82.3 % | 0 |
| this round's format control | **26.021 s** | 5.42 | 82.3 % | 0 |
| this round's format treatment | **24.720 s** | 5.88 | 80.1 % | 0 |

The fastest window carried the **highest** load and the slowest a middling one, so load
average is not the driver and the gate that admits a window does not discriminate
between a 17 s window and a 26 s window. That is recorded as a measurement-protocol
finding in its own right: **any future round that wants an absolute bar must re-derive
it in-window, and no preflight reading currently available will tell it whether the
window is a fast one.**

**What the revert costs, stated plainly.** The largest `commit_ns` movement this lane
has ever measured — **−1.550 s (−52.8 %)** against a same-window control, with
`resolve_ns`, `full_ns`, `delta_ns`, `group_ns` and `place_ns` all inside ±0.07 s
and every check green — is **not shipped**. The measurement is retained; the change is
not.

Production LOC: **25403 → 25403 (delta 0)**. The change was reverted before commit, so
the product total is unchanged by construction; `tools/production_loc.py` reports core
25403, reference 65417, combined 90820.

