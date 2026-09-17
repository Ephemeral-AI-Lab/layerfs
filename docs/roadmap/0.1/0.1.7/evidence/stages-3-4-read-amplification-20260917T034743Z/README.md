# Read amplification on the representation transition — receipt round

> **Round `stages-3-4-read-amplification-20260917T034743Z`** (host UTC
> `2026-09-17T03:48:05Z`). Collected for **#169 acceptance item 2** and for the
> verification registry row that has carried
> `read amplification on the representation transition | none | NOT_RUN - no case
> exists` since the batch was written. Receipts are append-only: nothing here
> rewrites an earlier round.

## 1. Identity

| Item | Value |
| --- | --- |
| Repository | `/Users/yifanxu/Ephemeral-AI-Lab/layerfs` |
| Commit | `841d9d2b1d811745026719096d6b736bfe3fe567` |
| Toolchain | `cargo 1.85.1 (d73d2caf9 2024-12-31)`, `rustc 1.85.1 (4eb161250 2025-03-15)` |
| Tool identity | `tool-identities.txt`; both executables archived by sha256 under `binary-archive/<sha256>/` |
| Raw logs | `c1-read-amplification.log`, `c2-read-amplification.log`, `fails-without-fix.log`, `b5-fails-without-fix.log`, `b5b4-fails-without-fix.log`, `b3b1-fails-without-fix.log`, `tests.log`, `clippy.log`, `fmt.log`, `checks.log` |
| Commands, exits and wall times | `commands.txt` |

**Reading `commands.txt`.** Four rows run two commands in one shell so that a
single control patch can be exercised on both sides; those rows report the
*shell's* exit and the individual exits are recorded inside the log as
`---- <step> exit: N ----`. A control row whose log holds exit 101 proved what it
was written to prove; the wrapper's 0 is not a pass.

**Why the archive exists.** The previous timing round recorded sha256 values for
`measure_pooled`/`measure_edits` that match nothing on disk, so its receipts are
no longer tied to any retained artifact. This round archives both executables by
the sha256 of the produced binary, gzipped, and `tool-identities.txt` states the
uncompressed hash, the archive hash, the length and the exact `--no-run` command
that rebuilds it.

## 2. The accounted quantity, defined before measuring

**A transition's physical read amplification is the canonical bytes the
operation acquires from its supplied provider, divided by the logical bytes the
transition produces.**

Every read this component performs goes through `AuthenticatedObjects`, so the
provider boundary is the one place that sees all of them across all three result
representations (whole file, chunked, empty). The instrument is a recording
provider in the test, not a product hook: the case matrix decodes the base's
mapping tree through the public codec to build a **coverage model** — which
logical range every payload object covers — and that model is what makes "a
discarded range is never read" checkable rather than asserted.

`EditCounters` is reported beside it. The round also pins where that instrument
is *absent* rather than zero, because the difference matters to any later reader
of these numbers:

* the whole-file assembly path and the empty-result path return
  `EditCounters::default()`, so every `small-to-large`, `large-to-small` and
  `to-empty` row below carries `nodes_read=0` because the path **publishes no
  counters**, not because it read nothing. The class is what charges those rows,
  and a dedicated case asserts the gap so it cannot be mistaken for a measured
  zero.
* on the Store side, `StoreReadCounters::canonical_bytes` counts
  dependency-chain reconstruction only, so a plain FULL acquisition contributes
  `0` to it. That corroborates the review's §6.2 "source/base/pack reads, scanned
  bytes: not counted for payload lanes", and it is why the byte quantity here is
  the provider census and not that field.

## 3. Case matrix and declared bounds

Every accepted cutoff (128 KiB, 256 KiB, 1 MiB) times four transitions on a
4 × cutoff base, plus one Store-backed case at the default cutoff. Declared
before collection:

| Case | Base | Edit | Declared bound on acquired payload bytes |
| --- | --- | --- | --- |
| `small-to-large` | `cutoff/2` whole file | insert `cutoff` at 1 000 | the whole single-object base — nothing is discarded |
| `large-to-small` | `4 × cutoff` chunked | delete `[cutoff/4, 4·cutoff − cutoff/4)` | `final + 2 × MAX_CHUNK` (kept ranges + one straddling chunk per edge) |
| `to-empty` | `4 × cutoff` chunked | delete `0..4·cutoff` | `0` payload bytes, at most one object acquired (the file state) |
| `in-place` (control) | `4 × cutoff` chunked | overwrite 512 bytes at `2·cutoff` | `4 × MAX_CHUNK`, and under half the base |
| `store-large-to-small` | `16 × cutoff` chunked, saved to a real `Store` | same delete, base acquired through the Store | `final + 2 × MAX_CHUNK`; no pack holding only discarded payloads; both instruments agree |

Per case the run asserts: the result equals the byte model; a fresh construction
of it reaches the same root wherever the canonical bytes are a pure function of
the content (the frozen registry's own rule judges a *chunked* overwrite by the
byte model, so the control is not compared by root); no payload lying entirely
inside the discarded interval was acquired; the base was acquired through the
supplied provider and the transition never acquired one of its own results; the
declared bound holds; and mapping pages are charged against a per-page allowance
of the largest canonical page.

## 4. Measured rows (quoted from `c1-read-amplification.log`, exit 0, 1.969 s)

```text
MEASURED read-amplification case=small-to-large-131072 base_payload_bytes=65559 acquired_bytes=65559 acquired_payload_bytes=65559 acquired_node_bytes=0 acquired_objects=1 exclusive_discarded=0 final_bytes=196608 nodes_read=0 nodes_created=0 payloads_created=0 peak_deferred_bytes=0 amplification_milli=333
MEASURED read-amplification case=large-to-small-131072 base_payload_bytes=524897 acquired_bytes=78083 acquired_payload_bytes=75569 acquired_node_bytes=2514 acquired_objects=7 exclusive_discarded=24 final_bytes=65536 nodes_read=0 nodes_created=0 payloads_created=0 peak_deferred_bytes=0 amplification_milli=1191
MEASURED read-amplification case=to-empty-131072 base_payload_bytes=524897 acquired_bytes=106 acquired_payload_bytes=0 acquired_node_bytes=106 acquired_objects=1 exclusive_discarded=29 final_bytes=0 nodes_read=0 nodes_created=0 payloads_created=0 peak_deferred_bytes=0 amplification_milli=null
MEASURED read-amplification case=in-place-131072 base_payload_bytes=524897 acquired_bytes=20474 acquired_payload_bytes=17854 acquired_node_bytes=2620 acquired_objects=3 exclusive_discarded=0 final_bytes=524288 nodes_read=7 nodes_created=1 payloads_created=1 peak_deferred_bytes=1768 amplification_milli=39
MEASURED read-amplification case=small-to-large-262144 base_payload_bytes=131095 acquired_bytes=131095 acquired_payload_bytes=131095 acquired_node_bytes=0 acquired_objects=1 exclusive_discarded=0 final_bytes=393216 nodes_read=0 nodes_created=0 payloads_created=0 peak_deferred_bytes=0 amplification_milli=333
MEASURED read-amplification case=large-to-small-262144 base_payload_bytes=1049794 acquired_bytes=154351 acquired_payload_bytes=149517 acquired_node_bytes=4834 acquired_objects=11 exclusive_discarded=49 final_bytes=131072 nodes_read=0 nodes_created=0 payloads_created=0 peak_deferred_bytes=0 amplification_milli=1177
MEASURED read-amplification case=to-empty-262144 base_payload_bytes=1049794 acquired_bytes=106 acquired_payload_bytes=0 acquired_node_bytes=106 acquired_objects=1 exclusive_discarded=58 final_bytes=0 nodes_read=0 nodes_created=0 payloads_created=0 peak_deferred_bytes=0 amplification_milli=null
MEASURED read-amplification case=in-place-262144 base_payload_bytes=1049794 acquired_bytes=29988 acquired_payload_bytes=25048 acquired_node_bytes=4940 acquired_objects=3 exclusive_discarded=0 final_bytes=1048576 nodes_read=7 nodes_created=1 payloads_created=1 peak_deferred_bytes=2928 amplification_milli=28
MEASURED read-amplification case=small-to-large-1048576 base_payload_bytes=524311 acquired_bytes=524311 acquired_payload_bytes=524311 acquired_node_bytes=0 acquired_objects=1 exclusive_discarded=0 final_bytes=1572864 nodes_read=0 nodes_created=0 payloads_created=0 peak_deferred_bytes=0 amplification_milli=333
MEASURED read-amplification case=large-to-small-1048576 base_payload_bytes=4198882 acquired_bytes=556746 acquired_payload_bytes=547552 acquired_node_bytes=9194 acquired_objects=34 exclusive_discarded=188 final_bytes=524288 nodes_read=0 nodes_created=0 payloads_created=0 peak_deferred_bytes=0 amplification_milli=1061
MEASURED read-amplification case=to-empty-1048576 base_payload_bytes=4198882 acquired_bytes=106 acquired_payload_bytes=0 acquired_node_bytes=106 acquired_objects=1 exclusive_discarded=218 final_bytes=0 nodes_read=0 nodes_created=0 payloads_created=0 peak_deferred_bytes=0 amplification_milli=null
MEASURED read-amplification case=in-place-1048576 base_payload_bytes=4198882 acquired_bytes=30992 acquired_payload_bytes=16528 acquired_node_bytes=14464 acquired_objects=5 exclusive_discarded=0 final_bytes=4194304 nodes_read=10 nodes_created=3 payloads_created=1 peak_deferred_bytes=9424 amplification_milli=7
```

`amplification_milli` is acquired bytes per 1 000 produced bytes; `null` is the
zero-length result, where the ratio is undefined and nothing is reported as zero
in its place.

Read as: an insertion acquires exactly its whole-file base (0.333); a deletion
that keeps an eighth of the file acquires 1.06–1.19 bytes per produced byte and
leaves **24 / 49 / 188** payload objects that lie entirely inside the discarded
range unread; a complete deletion acquires exactly one object — the 106-byte file
state — and no payload at all, leaving 29 / 58 / 218 discarded payloads unread;
and a localized 512-byte overwrite costs 0.007–0.039 bytes per produced byte of a
4 × cutoff base.

## 5. Store-charged row (quoted from `c2-read-amplification.log`, exit 0, 0.467 s)

```text
MEASURED read-amplification-store case=large-to-small base_payload_bytes=2099504 payload_acquired=84588 final_bytes=65536 exclusive_discarded=107 store_objects=8 store_packs_read=5 store_pages=5 store_edges=0 store_max_depth=0 store_canonical_bytes=0 store_ceiling=10 packs_in_store=10          packs_acquired=3 amplification_milli=1290
```

Read as: a 2 099 504-byte base held in a real Store, transitioned to a 65 536-byte
whole-file result, acquires 84 588 payload bytes across 3 of the Store's 10 packs;
107 payload objects that lie entirely inside the discarded range are never
acquired; no pack that holds *only* discarded payloads is touched; the Store
follows no dependency edge (`store_edges=0`, `store_max_depth=0`); and the two
instruments agree — the census served exactly the 8 objects C2 reports
(`store_objects=8`). `store_canonical_bytes=0` is the pinned finding of §2: that
field charges dependency reconstruction, not a plain FULL acquisition.

## 6. Controls — every new assertion has a run that fails without it

Each control patch was applied to the tree, checked to have applied (the
`control-patch-*.txt` files quote `git diff --stat`), run, and reverted; each
reverted file was compared byte-for-byte with the copy taken before the patch,
and the full suite was re-run afterwards (§7).

| Control | Patch | Raw failure line |
| --- | --- | --- |
| A — the localized read | `assemble_inner` reads the whole base before the plan, as a rebuild would | C1 exit 101: `large -> small at 131072 acquired 600466 payload bytes for a 65536 byte result` (against 78 083 with the fix); C2 exit 101: `a payload entirely inside the discarded range was acquired from the Store` |
| B — the coverage checks (B5) | both checks removed from `file/mapping/read.rs` | exit 101; `a_state_that_overstates_its_mapping_is_refused_not_partly_served` and `a_state_that_contradicts_its_root_page_is_refused_before_the_walk` both FAILED |
| C — the slice grammar bound (B5) | `ExtentSlice::new` back to the overflow-only check | exit 101: `an out-of-grammar slice produced length mismatch: recorded 262144, actual 245780` — the forged page decoded and failed much later, which is the defect |
| D — the ordinal ceiling (B4) | the truncating `as u32` cast restored | exit 101: `the space is exhausted: 0` — the guard returned ordinal **0** instead of the ceiling refusal |
| E — cleanup paging (B3) | the pack pass back to one statement | exit 101: `300 pack rows of 1 KiB were deleted in 0 page(s)` |
| F — the pending-value guard (B1) | `PENDING_VALUES_LIMIT` lowered to 4 | exit 101: `a full leaf is admitted: Integrity("metadata pending values")` — the guard is live, and at its real value of 100 the full leaf is admitted |

Control A is the direct negative control for this round's central claim, and it
is also the case's *bound* control: the whole-base re-read exceeds the declared
`final + 2 × MAX_CHUNK` bound before it ever reaches the discard assertion.

Controls A/C/D/E/F are patches that **do not** restore any reviewed behaviour:
D reinstates the truncation the review found (F-14), E the unbounded statement
(F-12), C the decode-time gap (F-15), F a bound of 4 that no product path uses.
B restores the pre-fix read path. None of them is a candidate fix.

Controls not produced, and why: **B2's assembly equivalence** (the two assembly
paths must produce identical bytes) is a refactor guard, not a defect oracle — its
assertion compares two implementations, so there is no single-line revert that
fails it, and the honest control is the equivalence itself. **B8's codec family**
drives code that was not changed, so it has no "without the fix" run: its control
is that the entry points previously had *no* direct test at all. **B6 and B9** are
bounds and work reductions without a failing assertion; their evidence is the
compiled invariant and the measured instrument cross-check in §5.

## 7. Checks run for this round

| Check | Exit | Wall | Log |
| --- | ---: | ---: | --- |
| `cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked` | 0 | 131.987 s | `tests.log` — **290 passed, 0 failed, 41 targets** |
| `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --workspace --locked --all-targets -- -D warnings` | 0 | 1.640 s | `clippy.log` |
| `cargo +1.96.0 fmt --manifest-path core/Cargo.toml --all --check` | 0 | 0.651 s | `fmt.log` |
| `python3 core/tools/check_product_boundary.py` | 0 | (in 6.223 s) | `checks.log` — `PASS: scanned 75 production Rust/SQL files` |
| `python3 -m unittest discover -s core/tools -p 'test_*.py'` | 0 | | `checks.log` — 5 tests, OK |
| `python3 -m unittest discover -s tools -p 'test_production_loc.py'` | 0 | | `checks.log` — 17 tests, OK |
| `python3 tools/production_loc.py` | 0 | | `checks.log` — core **11 160** / 75 files, reference 65 417, combined 76 577 |
| `git diff --check` | 0 | | `checks.log` |

The 131.987 s figure is the whole correctness suite, not a measured selection:
the 15 s / 25 s budget rules in `docs/general/benchmark_rules.md` govern a
*performance* selection's complete command, and no row of this round is a
performance selection.

## 8. What this round does not measure

* **It is not a v0.1.6 comparison.** There is no reference arm here, so nothing in
  this round says the candidate is existing-or-better at anything. G13/G15 stay
  unmeasured and this round does not upgrade them.
* **It is not a release-profile run.** Every row is a debug-profile test with an
  in-process fixture; no timing is claimed from it.
* **It does not bound the payload lane's *internal* read counters.** C2's
  `StoreReadCounters.canonical_bytes` is 0 for these reads by construction (§2);
  the bytes are charged by the provider census instead, and a future product-side
  counter would need its own receipt.
* **It does not cover a cold cache.** The fixture is built in the test, so its
  objects are in the page cache and in memory; the same rows under
  `shared/cold.py` would be `INELIGIBLE`, and no cache claim is made.
