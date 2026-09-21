## Status: the pack format was the lever. −21.2 % on `operation_ns`, row PASS.

**Nothing here is a proposal — it is measured, and the receipts are in the tree.**

### The result

`pipeline-namespace-10000`, one sample per arm, fresh `--out`, single thread, same instrument on
both arms (`pipeline.span_*` / `pipeline.diag_*` are the diagnostic counters this round added):

| | A1 (baseline + instrument) | T1c (treatment) | movement |
| --- | ---: | ---: | ---: |
| `operation_ns` | 3524.0 ms | **2776.3 ms** | **−747.8 ms, −21.22 %** |
| CPU (user+system) | 3274.3 ms | **2533.8 ms** | −740.5 ms, −22.62 % |
| `complete_command_ns` | 4783.7 ms | 4051.4 ms | −15.31 % |
| `profile_commit_ns` | 1133.6 ms | 682.5 ms | 0.602x |
| `profile_sql_ns` | 780.6 ms | 498.7 ms | 0.639x |
| `profile_place_ns` | 78.6 ms | 19.0 ms | 0.242x |
| **pack bytes written** | — | **302,406,480 B** | vs **2,292,865,337 B** = **7.58x** |

Row: **PASS, 13/13 gates**, **14/14 pinned counters reproduced**, `digest:filesystem_root`
`1d6fba29…` unchanged. The store is **not** byte-identical (`d8cd2384…`, 336,400,384 B against
307,879,936 B, +9.26 %) because a pack row now allocates its lane's whole 256 KiB limit — declared
before the run, not after.

The write amplification the campaign measured at **7.5917x** is now **1.0013x**. The 0.13 % above
1.0 is one 24-byte control area per pack write plus one directory entry per group.

### What changed

The directory moved into a **region reserved at the lane's own width**, and a pack now **declares
its own assembled length** in its control area, so a pack row may be allocated with spare capacity
and an append writes only the bytes it adds through incremental BLOB I/O. `SCHEMA_VERSION` 8 → 9;
framing versions 1/2/4/6/7 → 9/10/11/12/13, with the old ones refused by the existing
"unsupported framing" path and old Stores refused at open before any pack is read.

Two findings that decided the design, both measured rather than argued:

- **A companion `used` column does not work.** SQLite rewrites the whole row for any `UPDATE`, so
  `UPDATE object_packs SET used = ?` on a 256 KiB row costs the rewrite the format exists to
  remove: **72.5 µs** against **11.1 µs** for a four-byte in-place BLOB write.
- **BLOB I/O alone changes nothing.** With the directory at the front, appending group *k* shifts
  bodies `0..k` by one entry width. Reserving the region is what makes an append local.

### The 31 % remainder, attributed (this round's second deliverable)

A1's remainder was 1257.6 ms of 3519.5 ms (35.7 %). By name: **365.0 ms** is the driver's own C1
construction inside the timer (caller work, charged to nothing); **202.8 ms** is `BEGIN IMMEDIATE`
plus the pack-watermark read, 17,378 times — which `SaveProfile`'s own documentation claims is
charged to `commit_ns` and which is charged **nowhere**; **120.0 ms** is `validate_candidates`
(75.7 ms of it one `SELECT` per row); **107.6 ms** is the per-wave locator query and presence seed;
**207.7 ms** is `offer` outside its charged buckets; **257.0 ms** is the finish-span drain.

Two of those are the next levers and neither is in the write path: `BEGIN IMMEDIATE` (5.8 %, and it
is the multi-writer cadence contract, so it is not free to remove) and `validate_candidates`
(3.4 %, one query per row where one paged query per seal would do). After the treatment the same
remainder is 1294.7 ms of a 2772.5 ms span — it did not grow, but its *share* did.

### Verification, and what was not run

`cargo test -p layerfs-storage` **33 binaries, 0 failures**; the **whole core workspace** 108
`test result: ok`, exit 0; `clippy` clean; `fmt --check` clean;
`core/tools/check_product_boundary.py` PASS. The invariants the handoff named as the wall —
`cas_reuse` (pack sharing) and `delta_payload` (intra-save delta candidacy) — are green, because
this change alters **what is written, never when**.

**Not run and not claimed:** the reference `crates/` workspace's tests, any other harness case or
lane, and any durability run — the connection profile is unchanged, so this is a format and cost
change, not a durability change. The harness's `registry_self_check` reports the same pre-existing
cardinality mismatch on every row here, including the campaign baseline.

### Where this sits against this issue's acceptance bar — read this before quoting the number

The bar in this issue's focus section names **`namespace-10000` in the `init_namespace` family**
(`layerstack_init_ns`, ≤ 578.245 ms, paired against the v0.1.6 reference arm). This round optimized
**`pipeline-namespace-10000`**, the v0.1.7 `pipeline.*` row that performs the same work (25,245
objects, 301,171,810 canonical bytes) through the C1→C2 handoff. **They are different cases in
different harnesses with different timers, so no number here is a claim about that bar.** The
−21.22 % is on `pipeline-namespace-10000`'s own `operation_ns`, against a control measured in this
same worktree with the same instrument.

What transfers is the mechanism, and it is the reason this was worth doing: 60 % of the pipeline row
was the Store's write path, and its 2,292,865,337 bytes of whole-pack rewrite are now 302,406,480
bytes of real append. Whether the `init_namespace` case carries the same share is **a measurement
this round did not take**, and the v0.1.6 pairing that bar requires has not been produced. The
`init_namespace` route is also a different construction path (multi-worker init is explicitly out
of scope here: every row in this round ran with one construction worker).

### Evidence

`docs/roadmap/0.1/0.1.7/evidence/issue219-ns19-format-20260921T054430Z/` — pre-registration,
`README.md`, `attribute.py` (re-derives every number above from the receipts), the seven raw
receipts, `stores.txt`, `loc.txt` and the test transcripts.

Production LOC: 30963 → 31247 (delta +284), combined 96380 → 96664, by
`python3 tools/production_loc.py`.

**Nothing is closed on this handoff.** The write path is fixed; the remainder is now the row.
