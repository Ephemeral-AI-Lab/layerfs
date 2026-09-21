# #219 round 13 — memoised absence: `validate` 228.42 → 4.88 ms, the row −207.5 ms

Pre-registration: `pre-registration.md` (written before the edit and before the run). Row:
`benchmark-results/issue219/ns19-N1-absence-20260921T083530Z`, **PASS, 13/13 gates, 14/14 pinned
counters**, `source_commit` `35aaf6f0b`, clean. Landed as `35aaf6f0b`.

## 1. The movement

| instrument | M1 (control) | N1 | movement |
| --- | ---: | ---: | ---: |
| **`operation_work_ns` (the row's formula)** | 1681.30 ms | **1473.83 ms** | **−207.5 ms, −12.34 %** |
| **CPU user+system** | 1683.80 ms | **1495.48 ms** | **−188.3 ms, −11.2 %** |
| `span_build_ns` | 318.72 ms | **90.61 ms** | −228.1 |
| **`build_validate_ns`** | 228.42 ms | **4.88 ms** | **−223.5 ms** |
| `validation_inode_pages_read` | 27,662 | **226** | **−27,436 (−99.2 %)** |
| `validation_read_waves` | 27,657 | 221 | −27,436 |
| `validation_pages_bindings` | 13,718 | **0** | −13,718 |
| `validation_pages_cycles` | 13,718 | **0** | −13,718 |
| `validation_pages_prefetch` | 10 | 10 | 0 |
| `validation_pages_allocation` | 216 | 216 | 0 |
| `validation_entries_examined` | 4,096 | **4,096** | 0 |
| `phases.operation_ns` | 1723.9 ms | 1551.0 ms | −172.9 |
| complete command | 2447.6 ms | 2376.2 ms | −71.4 |

**Cumulative against this worktree's clean tree** (A0: ≤ 3487.3 ms of work, 3388.9 ms CPU):
`operation_work_ns` **≤ −57.7 %**, CPU **−55.9 %**; against D4b's landed 1821.0 ms it is −19.1 %, and
against round 8's 1656.2 ms −11.0 %. **Boundary-matched** (`+ construct_ns 447.67 + construct_noise_ns
113.11`) the row is **2034.6 ms** of work, against the reference's same-shape rows at 1789.5 / 2116.7 /
2195.3 ms of CPU — inside their band, and the gap that L69 recorded as 1.4–24 % behind is now closed to
the fast end of it.

## 2. What was done

`ValidationState` remembers absence as well as presence. An absent memo hit charges **one demand**,
exactly as the descent it replaces charges one, and what falls is the physical read. The prefetch
records absence for every serial the grouped demand already answered `None` for, which is what stops
the binding loop and the cycle walk buying those answers a second and third time.

The memo is sound because the base is immutable for its lifetime: one `check` call binds one base root
through `FilesystemTopology::load`, every site reads that same `InodeTable`, and a verdict is all this
phase produces.

## 3. The two sub-predictions that were missed

Both are reported as missed, and neither is repaired by re-running.

- **`read_waves` was predicted ≤ 60 and is 221.** The prediction priced the grouped prefetch and forgot
  the allocator precondition, which reads 216 pages in its own waves (64 serials per wave over
  `new_inodes`). 221 waves for 226 pages is one page per wave, which is that site plus the prefetch.
- **`inode_demands` was registered as "unchanged at 17,777" and is 17,975 — so refutation clause 2
  fired.** The +198 is explainable and is a charge-semantics detail, not a read: a serial that falls
  off a branch never reaches a leaf, so the old path charged **no** demand for it, while an absent memo
  hit charges one. 198 such serials. The clause existed to prove the treatment removed reads rather
  than accounting, and on that question the row is unambiguous — 27,436 pages and 27,436 waves
  removed for a 198-demand change — but the clause fired and is recorded as fired. Restoring
  bit-identity would mean storing what the replaced descent charged alongside the absence, which is
  available if the owner wants it and is **not** done here.

**Every pin held**: `commits` 284, `inserted` 25245, `pack_bytes_written` 302,406,480, `statements`
16595, `batches` 3, `bindings` 10100, and `digest:filesystem_root` unchanged. `fs_build.validation_entries`
— pinned at 4,096 for four other rows — is untouched at 4,096, which is why this treatment did not
need a re-pin.

One movement is left unexplained and is **not** claimed: `diag_commit_total_ns` rose 331.2 → 416.4 ms
while every other store-side term fell (`offer` −49.8, `write_pack` −19.3, `insert` −11.5, `full`
−11.0), so `span_content_ns` ended 20.7 ms higher. The commit is page-write bound and this lane has
measured it swinging with the window before; this round attributes none of it.

Checks as run: `cargo test -p layerfs-content` **262 passed / 0 failed**; the whole core workspace
`--no-fail-fast` **621 passed / 0 failed**; `clippy --all-targets` clean; `fmt --check` clean;
`core/tools/check_product_boundary.py` PASS. Not run: the harness's own suite for this commit (it does
not cover product source), the reference `crates/` workspace, and any other harness case or lane.

Production LOC: **31409 -> 31426 (delta +17)**. Method `python3 tools/production_loc.py --root <tree>`,
first parent `9736ce633` against the committed tree.
