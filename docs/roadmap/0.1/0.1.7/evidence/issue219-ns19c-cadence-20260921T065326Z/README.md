# #219 round 3 — the step is the wave: `operation_ns` −23.92 %

Pre-registration: [`../issue219-ns19b-remainder-20260921T063653Z/pre-registration-T3.md`](../issue219-ns19b-remainder-20260921T063653Z/pre-registration-T3.md),
written **before** the run, including the pin consequence declared before the number existed.
Control: **D2b** (`ns19-D2b-instrument-20260921T064257Z`) — the committed product code
(`a5c54df16`) with this round's instrument and no treatment, measured 20 minutes earlier on the
same machine. Receipts: `receipts/`.

## 1. The result

| instrument | D2b (control) | T3c | movement |
| --- | ---: | ---: | ---: |
| `operation_ns` | 2714.3 ms | **2065.0 ms** | **−649.3 ms, −23.92 %** |
| CPU (user+system) | 2666.9 ms | 1822.6 ms | **−844.3 ms, −31.66 %** |
| `complete_command_ns` | 4001.6 ms | 3316.2 ms | −685.5 ms, −17.13 % |
| `accept_span_ns` | 2710.3 ms | 2061.7 ms | −648.6 ms |
| `diag_begin_ns` | 257.5 ms | **10.8 ms** | −246.7 ms |
| `profile_commit_ns` | 742.4 ms | 384.8 ms | −357.7 ms |
| `profile_sql_ns` | 509.4 ms | 361.1 ms | −148.3 ms |
| `profile_total_ns` | 1553.0 ms | 1023.9 ms | −529.0 ms |
| `pipeline.commits` | 17378 | **800** | the declared pin change |

Row **PASS, 13/13 gates, 14 of 14 pinned counters reproduced** after the one declared re-pin, and
`digest:filesystem_root` `1d6fba29…` unchanged. **Every work counter is byte-identical to the
control**: `inserted` 25245, `statements` 16595, `packs_created` 1268, `pack_appends` 15534,
`presence_queries` 398, `full_records` 25241, `prefix_records` 4, `content_bytes` 301171810,
`reused` 0, `batches` 3, `bindings` 10100, `metadata_objects` 382, `objects_emitted` 67,
`chain_objects` 382, `largest_batch_bindings` 4096. The store's sha256 is unchanged from round 1's
treatment (`d8cd2384…`), because this treatment writes the same bytes in fewer transactions.

**The prediction was beaten on `begin_ns` and missed on `commit_ns`'s composition.**
`diag_begin_ns` fell exactly as predicted (257.5 -> 10.8 ms against a predicted <= 40 ms, from
17,378 `BEGIN IMMEDIATE` to 982: ~600 waves plus 382 ordinal reservations). `commit_ns` fell
**357.7 ms against a predicted 100-190 ms**, and `sql_ns` fell **148.3 ms**, which the registration
did not predict at all. The mechanism is the one the registration under-weighted: an append's
*body* pages are new, but the pack's **control page and the `objects`-table pages are re-dirtied by
every seal**, so a transaction that spans a wave journals and writes them once instead of once per
seal. The registration's own rule applies - a fall larger than predicted must be explained - and
this is the explanation, measured: the deduped pages are the small, repeatedly-touched ones, not the
bodies.

## 2. The one semantic difference the suite found, and how it is preserved

`metadata_pool_index::a_failed_save_cannot_change_the_published_candidate_set` pins that **an
aborted save's ordinal reservations are never reused**. Under per-seal commits the reservation was
durable on its own, because the seal before it had already committed. Under a wave transaction it
would have rolled back with the wave, and the next save would have reused the ordinals - the test
failed with `left: [(1, 8), (9, 8)]` against `right: [(1, 8), (17, 8)]`.

It is **preserved, not waived**: the reservation is committed as **its own step**
(`commit_reservation`), which closes the wave's transaction and lets the next write reopen it -
exactly the boundary every seal used to draw. It is the only behavioural difference the 33 test
binaries found, and the fix is a six-line function with the reason in it.

## 3. The declared pin change, and what it costs

`tests/golden/expected.tsv` pinned `pipeline-namespace-10000 counter:pipeline.commits 17378`.
This treatment acknowledges 800 commits, so the row **fails `g1.o3-pinned-counters`** unless that
one line moves. It was declared in the pre-registration **before the run**, with the reason, and no
other pin moved.

Three rows were taken and all three are reported:

| row | what it is | result |
| --- | --- | --- |
| `ns19-T3-wave-20260921T065156Z` | the treatment, before the re-pin | **FAIL**, 12/13, `pipeline.commits 17378 -> 800`; `operation_ns` 2113.2 ms |
| `ns19-T3b-repin-20260921T065303Z` | the same, after editing the table but **with `--no-build`** | **FAIL**, identical message: the golden table is compiled into the harness binary, so the gate read the old pin. A build-discipline note, kept because it is a real trap: editing a compiled-in expectation and re-running with `--no-build` measures the old gate. |
| `ns19-T3c-pinned-20260921T065326Z` | rebuilt, the row this round lands | **PASS**, 13/13, 14/14 |

**What the re-pin costs, stated plainly.** The pin was a regression gate for the row's cadence: a
future accidental return to per-seal commits would have failed it. It no longer will. The cadence is
now guarded by the *evidence* rather than by the constant - `diag_begin_ns` reads 10.8 ms on this
tree and ~250 ms on a per-seal tree - and by the T3 rows themselves. The other thirteen pins keep
their full force, and they are the ones that say the *work* did not change.

## 4. What the cadence change means for the second writer

A write transaction now spans a wave, so another writer waits for a wave instead of a seal: ~3.5 ms
against ~0.12 ms on this row. The contract's own words are satisfied rather than bent - "a write
transaction never outlives the step that opened it under the arbitration lock, so every step commits
before that lock is released; batching stays inside a step" - because the step is now the wave, and
the wave is the bounded unit the caller already offers. The prior art's pair probe tolerated a p99
of **6.7-7.1 ms**, so a 3.5 ms step is inside the width that probe already accepted. The two-thread
`multi_writer::…threads…` case is green: both writers still complete, with the same bytes.

**Not measured:** a pair-latency probe on this tree. The step width is quoted from the row's own
wave span (`span_content_ns` 1542.3 ms over ~600 waves), not from a paired measurement, and that is
a different instrument from the one the 6.7-7.1 ms p99 came from.

## 5. Cumulative, against the clean tree

| row | `operation_ns` | CPU | what it is |
| --- | ---: | ---: | --- |
| A0 | 3490.3 ms | 3389.0 ms | this worktree, unmodified tree |
| T1c | 2776.3 ms | 2533.8 ms | round 1: the reserved directory and the in-place append |
| T3c | **2065.0 ms** | **1822.6 ms** | round 3: the wave-scoped transaction |

**−40.8 % on `operation_ns` and −46.2 % on CPU against A0**, with the filesystem root, all work
counters and the store's bytes unchanged throughout. The campaign's own baseline row
(`ns17-squadA-packcounters-…`, 3351.0 ms) is −38.4 %.

## 6. Checks as run on this tree

`cargo test --locked --manifest-path core/Cargo.toml -p layerfs-storage` **33 binaries, 0 failed**;
the whole core workspace `--no-fail-fast` **108 `test result: ok`, 0 failed, exit 0**;
`clippy --all-targets` clean; `fmt --check` clean; `core/tools/check_product_boundary.py` **PASS over
194 files**. **Not run:** the reference `crates/` workspace's tests, any other harness case or lane,
and any durability run - the connection profile is unchanged, so this remains a format-and-cadence
change and not a durability change.

Production LOC: **31265 -> 31318 (delta +53)**; combined 96682 -> 96735. Method
`python3 tools/production_loc.py --root <tree>`, first parent `c83546ad4` against the committed tree.
