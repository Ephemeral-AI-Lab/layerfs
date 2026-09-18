# P1-2 receipt — pool the read connection and decode arena per operation

> **Status:** Item receipt. Written once, from [`after/`](after/) (commit
> `9ec299f13`) under [`../../CONTRACT.md`](../../CONTRACT.md). The **before** arm is
> [`../v3/after/`](../v3/after/), collected on `aefcd95a5`, whose product tree is
> byte-identical to this item's parent `1109fa853` (the difference between the two
> commits is the V3 round's own evidence files). No row is straddled: both arms use
> the same counter (`StoreReadCounters.opens`, added by V3) and the same vehicles.

## 1. The item

One read wave paid two fixed costs before reading anything — a connection with the
declared pragma profile and a decode arena — and both were per **wave**. P1-2's
Big-O target is that an operation's fixed cost is `O(1)` rather than `O(waves)`,
with the visibility ceiling still captured per wave (required semantics), and the
decode arena held once (1 MiB × 1) instead of transiently per wave.

## 2. The change (one commit)

| # | Site | Statement |
| --- | --- | --- |
| 1 | `cas/read.rs` | new `ReadSession { connection, workspace }`: opened once, reused by every wave of one operation; `read()` re-reads `schema::retained_pack_ceiling` **per wave** |
| 2 | `cas/provider.rs` | `StoreProvider` holds `RefCell<Option<ReadSession>>`; its first wave opens the session and reports `opens: 1`, later waves reuse it and report `opens: 0`; `connection_opens()` still sums them |
| 3 | `cas/read.rs`, `cas/provider.rs`, `cas/store.rs` | the wave's **declared demand bound** survives the pooling: `check_read_demand` moves into `cas/read.rs`, runs inside `ReadSession::read`, and the provider calls it *before* the session exists — a demand over `READ_OBJECT_LIMIT` is refused without opening anything, exactly as on the Store's own path |

The session lives in the **provider**, not in `Store`: the Store stays shared and
`Sync`, the provider is `!Sync` by construction. `Store::read_batch` is unchanged —
it is the wave-level API and still opens its own connection per call.

## 3. Before → after on the frozen set

Counter: `readback connection opens` (`measure_edits`, the V3 print). Same
workload, same vehicle, same tree shape, one sample each.

| Row | workload | before (v3 arm) | after (this arm) | predicted |
| --- | --- | ---: | ---: | --- |
| **D21** | `edits.pipeline.chunked` | **3** | **1** | O(waves) → O(1) ✔ |
| **D22** | `edits.pipeline.small-to-large` | **3** | **1** | ✔ |
| **D23** | `edits.pipeline.large-to-small` | **3** | **1** | ✔ |
| **D24** | `edits.pipeline.batch` | **3** | **1** | ✔ |
| D20 | `edits.pipeline.small` | 1 | 1 | single wave — cannot discriminate |
| D15–D19 | `edits.c2.*` | 1 | 1 | single wave — cannot discriminate |

**Every other counter on the frozen set is identical**: a line-by-line diff of all
29 D-rows between the two arms, with timing figures and paths removed, differs in
exactly **16 lines — the four `3 → 1` readings above** (4 rows × 4 diff lines). No
root, object, page, row, spill, run or byte moved; the readbacks are byte-for-byte
identical (`edits.pipeline.chunked`: 262,144 bytes verified against the independent
model, root `ca6c30a4355c63c3…`).

The four discriminating rows are one shape at four edit tuples, not four
independent confirmations — said plainly.

## 4. The item's tests

| Test | What it pins |
| --- | --- |
| `cas_roundtrip::a_pooled_session_opens_one_connection_for_every_wave` | three waves through one provider: per-wave `opens` = `[1, 0, 0]`, `connection_opens() == 1`; a direct `Store::read_batch` still reports `1` |
| `cas_roundtrip::a_pooled_session_sees_a_save_that_completed_between_its_waves` | the ceiling **advances** across a save that lands between two waves and the second wave reads the new object — the guard against pooling the ceiling |
| `cas_roundtrip::an_oversized_provider_demand_is_refused_before_the_session_opens` | an over-ceiling demand is refused with `CapacityExceeded`/`storage.read_objects` and `connection_opens()` stays **0**; a served demand then reports `opens: 1` |

**Pre-item failure:** on the parent tree there is no session (the provider opens a
connection per wave and reports `opens: 1` every time), so the first test's
`[1, 0, 0]` cannot hold; the second test would pass on the parent tree, because a
per-wave connection re-reads the ceiling anyway — it is a **regression guard for the
new design**, not a falsification of the old one. Stated rather than presented as
two failing-on-parent tests.

**Why the third test exists.** Writing the error-path probe found that the pooled
path had **lost the wave's demand bound**: `READ_OBJECT_LIMIT` was checked at
`Store::read_batch`'s entry point and nowhere else, and `read_objects` never
enforced it — so `read_wave` reached `read_objects` unchecked and opened a
connection for a demand it should have refused. The check moved into `cas/read.rs`
and now runs on both doors, with the provider's own call before the open. It is the
same variable as the rest of the item (the session must not lose a bound the wave
had), which is why it is in this commit rather than a follow-up.

V3's `a_read_wave_reports_the_connection_it_opened` was written with a **bound**
(`1..=2` after two waves) precisely so this item could pool without re-pinning a
test; it still passes unchanged.

## 5. The eight checks (exit codes)

| # | Command | Exit |
| --- | --- | ---: |
| 1 | `python3 core/tools/check_product_boundary.py` | 0 (116 files) |
| 2 | `python3 -m unittest discover -s core/tools -p 'test_*.py'` | 0 (6 OK) |
| 3 | `python3 -m unittest discover -s tools -p 'test_production_loc.py'` | 0 (17 OK) |
| 4 | `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check` | 0 |
| 5 | `cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked --no-fail-fast` | 0 (**440 passed, 0 failed**) |
| 6 | `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --workspace --locked --all-targets -- -D warnings` | 0 |
| 7 | `python3 tools/production_loc.py --files` | 0 |
| 8 | `git diff --check` | 0 |

Sealed-oracle parity set: unchanged (`git diff --stat 1109fa853..d2c6dfb7f --
'*tests*'` is `cas_roundtrip.rs` only, not one of the seven targets) and green —
re-run on the C1 tree in [`../c1-rebaseline/verify-c1.md`](../c1-rebaseline/verify-c1.md) §1.

## 5b. The retained pre-fix arm

The round was first collected on the tree **before** the demand-bound restoration
(commit `d2c6dfb7f`, later amended). That arm is retained, not deleted, at
[`after-pre-demand-fix/`](after-pre-demand-fix/): every counter in it is identical
to the final arm (diff of all 29 rows: **0 lines**), because no frozen row exceeds
the read ceiling — the bound fires only on the error path the new test probes. The
gate sample is [`after/`](after/), collected on the final commit `9ec299f13`.

## 6. Determinism

`X5` (repeat of D21, the primary row) is bit-identical to its gate sample: same
root, same 262,144 readback bytes, same `opens: 1`. Its first attempt **failed**
(exit 1: the vehicle refuses a pre-existing `--output` directory — the driver's own
convention, which creates the directory inside the vehicle); the failed attempt and
its log are retained beside the successful one in `after/logs/`, and the failure is
a driver-invocation error, not a product result. `X1`/`X2` are the standard labelled
repeats. Every `elapsed_ns` in the round is diagnostic.

## 7. Production LOC

`P1-2 | +70..+130` was the plan's estimate; the actual is **+57**
(`cas/provider.rs` 73 → 101, `cas/read.rs` 74 → 112, `cas/store.rs` 387 → 378 —
the private demand check moved out of it).
`core` **18,805 → 18,862 (delta +57)**; `crates/` reference 65,417 → 65,417;
combined 84,222 → 84,279. Method: `tools/production_loc.py` over the first parent
(`1109fa853`, via `git archive`) and the staged tree.

## 8. Acceptance

- [x] The counter moved in the predicted direction on the frozen set (3 → 1 on the four discriminating rows)
- [x] No other counter moved (16 diff lines, all four readings)
- [x] Roots and readback bytes identical; parity set untouched
- [x] The item's own test exists and the ceiling semantics have their own guard
- [x] Architecture doc (`05-storage.md` §6.11) updated in the same commit
- [x] Eight checks green; LOC disclosed; determinism repeat labelled
