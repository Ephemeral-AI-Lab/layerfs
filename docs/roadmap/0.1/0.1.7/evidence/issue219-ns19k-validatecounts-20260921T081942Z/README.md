# #219 round 11 — `validate` is 17,777 authenticated inode reads at 12.5 µs each

Pre-registration: `pre-registration.md`. Row: `benchmark-results/issue219/ns19-K1-counts-20260921T082053Z`,
**PASS, 13/13 gates, 14/14 pins**, `source_commit` `31660e990`, clean. **No product line changed.**

## 1. The counts

| count | value | per unit |
| --- | ---: | ---: |
| `build_validate_ns` | **222.15 ms** | — |
| `validation_objects_read` | **17,777** | **12.50 µs each** |
| `validation_inode_demands` | 17,777 | 12.50 µs each |
| `validation_inode_pages_read` | 27,662 | 8.03 µs each |
| `validation_read_waves` | 27,657 | 8.03 µs each |
| `validation_directory_pages_read` | **0** | — |
| `validation_entries_examined` | **4,096** | 54.2 µs each |
| `build_objects_read` (all phases) | 310 | — |
| `build_base_records_read` | **6,066** | ~11 µs each of the 68 ms residual |

## 2. The phase is reads, and the prior said it would not be

The pre-registration's prior was that `objects_read` would be small — the batch's parents and
children are prefetched into one grouped demand before the loop — and that `entries_examined` would be
12,000–40,000, i.e. pure CPU over the effective-tree cycle walk. **Both halves of that prior are
refuted by the row**, and clause 1 of the refutation list fired:

- `entries_examined` is **4,096**, exactly `MAXIMUM_CYCLE_CHECK_ENTRIES` — the walk railed against
  its declared ceiling rather than running long. `directory_pages_read` is **0** in validation.
- `objects_read` is **17,777** for 10,100 bindings across 3 batches — **1.76 authenticated inode
  reads per binding** — and `inode_demands` equals it exactly, so essentially every logical demand
  cost a read.

**222.15 ms / 17,777 reads = 12.50 µs per authenticated read.** That figure is the finding, because
it is not a property of `validate` at all: the residual half of the build reads **6,066** base inode
records while deriving final counts, and at ~11 µs each those 6,066 reads are most of the 68.38 ms
residual. **The same per-read price appears in both halves, so the price belongs to the read path,
not to either caller.**

For scale: the object being read is a base inode record of a few hundred bytes, read through an
**in-memory `TreeStore`** (`PairProvider::new(&prefixes[index], &empty)`) — there is no disk, no
SQLite and no page cache in this span. 12.5 µs is the product's own decode-and-authenticate path.

## 3. What the round does not say

It does not say the reads are unnecessary. `read_waves` (27,657) exceeds `objects_read` (17,777), so
a large share of the waves return nothing and their fixed cost is unmeasured; and whether 1.76 reads
per binding is inherent (each batch re-reads the chain it was handed) or avoidable is a question for
a round that prices the read path itself. **This round prices the symptom; it does not name the
cure**, and that is stated rather than dressed up.

Two candidate directions, in the order the evidence supports them:

1. **The price per read (12.5 µs).** An in-memory authenticated read of a few-hundred-byte object
   should cost a map lookup, a hash and a decode; 12.5 µs is an order of magnitude above that, and
   the same price recurs in a different caller. A standalone diagnostic through the product's own
   read path on a known object settles whether 12.5 µs is a property of one read or of the row.
2. **The number of reads (1.76 per binding, plus 0.6 outside validation).** Fewer reads is a product
   algorithmic question (batch-local caching of the chain, or a demand that does not re-read what a
   previous batch already authenticated).

Nothing here is a v0.1.6 comparison and nothing is claimed about correctness: every pin, every work
counter and the root digest reproduce, and the row is PASS.

## 4. Window, stated rather than banked

K1 is uniformly slower than J1 (`span_build_ns` 323.86 vs 308.59, `operation_work_ns` 1677.00 vs
1590.39, CPU 1695.45 vs 1616.91) while `validate` itself is flat (222.15 vs 221.70). The counters are
the deliverable and they are work-neutral by construction — they read fields the product already
maintains. Harness suite: **120 passed / 3 failed**, the three pre-existing `registry_negative` cases.

Checks as run: harness suite `--no-fail-fast` **120 passed / 3 failed**. Not run: the product's
suites, which this round cannot affect, and any other harness case or lane.

Production LOC: **31377 -> 31377 (delta 0)**; the measurement harness is not product source. Method
`python3 tools/production_loc.py --root <tree>`, first parent `a2f049084` against the committed tree.
