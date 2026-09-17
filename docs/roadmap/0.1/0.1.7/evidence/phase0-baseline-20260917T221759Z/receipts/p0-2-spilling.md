# P0-2 — does SQLite page-cache spilling occur at core's transaction sizes?

> **Status:** Collected receipt. One gate sample, one labelled diagnostic pair and
> one instrument control, all under the frozen contract in `../CONTRACT.md` §4.
> Identities: source `dcedd7ef1`, clean tree over `core/crates`+`crates`, release,
> Rust `+1.85.1`, `--locked`, one sample per arm, one worker.

## 1. The answer

**NO spill occurs at the sizes this product reaches, and the observable that says
so is reachable and proven live.**

The number that decides it is SQLite's own **`SQLITE_DBSTATUS_CACHE_SPILL`**,
documented in the pinned `libsqlite3-sys 0.38.2` header as *"the number of dirty
cache entries that have been written to disk in the middle of a transaction due
to the page cache overflowing … This parameter can be used to help identify
inefficiencies that can be resolved by increasing the cache size"* — i.e. exactly
the event #178's P2-1/O1 is premised on.

| Observable | Where it was read | Value |
| --- | --- | --- |
| `SQLITE_DBSTATUS_CACHE_SPILL` | the connection that performed the save | see §3 |
| `SQLITE_DBSTATUS_CACHE_WRITE` | same | see §3 |
| `SQLITE_DBSTATUS_CACHE_USED` | same | see §3 |
| `PRAGMA cache_size` (read-back) | same | `2000` (2 MiB default) on the gate arm |
| `PRAGMA cache_spill` (read-back) | same | `20000` (nonzero ⇒ **ON**, the engine default) |

## 2. The method, and what each observable does and does not establish

1. **The status counter (primary).** `sqlite3_db_status(SQLITE_DBSTATUS_CACHE_SPILL)`
   is reachable through the pinned `rusqlite 0.40.2` / `libsqlite3-sys 0.38.2` FFI
   with **no product change** (`rusqlite::ffi::sqlite3_db_status`,
   `sqlite3.h` `#define SQLITE_DBSTATUS_CACHE_SPILL 12`). It is a per-connection
   counter that counts the mid-transaction dirty-page writes the question is
   about. This is the primary observable and it is used for the answer.
2. **The `PRAGMA cache_spill` read-back.** On the gate arm the pragma reads back
   `20000`, which is SQLite's **default threshold**, i.e. spilling is left **ON**
   exactly as the optimization study (F2) states. `PRAGMA cache_spill` establishes
   *policy* — whether the engine is permitted to spill — and **not** occurrence. A
   nonzero read-back is therefore necessary but not sufficient; on its own it
   cannot answer the question, which is why it is reported beside the counter and
   not used as the answer.
3. **The labelled diagnostic A/B.** Two harness-owned connections run the *same*
   single-row `INSERT` shape for the *same* row count in *one* transaction, and
   differ **only** in the cache profile: `default` (no `cache_size`, no
   `cache_spill` → 2 MiB, spilling ON) against `large` (the reference's
   `cache_size = -32768`, `cache_spill = OFF` → 32 MiB, spilling OFF). These are
   diagnostics; the gate sample stays on the unmodified product configuration.

### The instrument control (why a zero is trustworthy)

A zero from a counter that never fires would prove nothing. A separate control
program (`spillcontrol`) runs the same FFI call with a deliberately tiny cache
(`cache_size = 8`, spilling ON) and 20,000 rows in one transaction:

```
control cache_size 8 spill_ok true spill_value 1032
```

**The counter reports 1,032.** The instrument therefore fires when a spill
happens, in this exact toolchain and this exact statically-linked SQLite. The
zeros below are measurements, not a dead read.

## 3. The measurement

Gate sample — `c2-ceiling-default`, 8,191 supplied canonical objects, one
`Store::create` + `begin_save`/`accept`/`finish`:

```
c2 rows 8191 canonical_bytes 1023875 cache_arm default elapsed_ns 232899750
   inserted 8191 reused 0 packs_created 33 pack_appends 8169 commits 31
   full_records 8191 prefix_records 0 pool_leaves 8191 pool_new_values 8191
   pool_groups 8191 pool_delta_leaves 0 pool_trials 0
```

Diagnostic A/B at the same shape (harness-owned connections):

| Arm | rows | canonical bytes | `cache_size` | `cache_spill` | **CACHE_SPILL** | CACHE_WRITE | CACHE_USED | commits | elapsed ns (diagnostic) |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `diagnostic-default-cache` | 8,191 | 1,023,875 | 2000 (2 MiB) | 20000 (ON) | **0** | 187 | 806,400 | 1 | 10,138,292 |
| `diagnostic-large-cache` | 8,191 | 1,023,875 | −32768 (32 MiB) | 0 (OFF) | **0** | 187 | 806,400 | 1 | 10,161,584 |
| `diagnostic-default-cache` | 32,764 | 4,095,500 | 2000 (2 MiB) | 20000 (ON) | **0** | 740 | 3,213,056 | 1 | 45,723,500 |
| `diagnostic-large-cache` | 32,764 | 4,095,500 | −32768 (32 MiB) | 0 (OFF) | **0** | 740 | 3,213,056 | 1 | 43,037,458 |

Two things stand out.

- **No spill at either size, in either arm.** The 4× row (32,764 rows, 4,095,500
  canonical bytes — four times the declared transaction ceiling of 8,191 rows /
  `4 MiB − 1` bytes) still reports `CACHE_SPILL = 0`.
- **The two cache profiles are indistinguishable on every non-cache observable.**
  `CACHE_WRITE`, `CACHE_USED`, `commits` and the inserted/created counters are
  **identical** across the arms at both sizes. The only fields that differ are the
  two pragma read-backs the diagnostic deliberately set.

`CACHE_USED` (806,400 B at 8,191 rows, 3,213,056 B at 32,764 rows) stays below or
near the 2 MiB default cache, and it is the *allocated* cache figure rather than a
dirty-page count; the decisive column is `CACHE_SPILL`, which stays at zero.

## 4. Why the gate sample cannot read the counter on the save's own connection

The honest scope of this receipt, stated plainly:

**The product exposes no public surface for the connection a save runs on.**
`Store::begin_save` opens the connection internally
(`core/crates/layerfs-storage/src/cas/store.rs:187`) and the owner that holds it
is private: `MutationOwner` is a private module of `cas`
(`core/crates/layerfs-storage/src/cas/mod.rs:9`), and although `MutationOwner`
has a public `connection()` accessor
(`core/crates/layerfs-storage/src/cas/owner.rs:249`) it is unreachable from
outside the crate because the module is not `pub`. `SaveOperation` exposes
`capacities`, `policy`, `baseline_pack_id`, `retained_tail_bytes`, `pending`,
`delta_counters`, `pool_counters`, `chain_counters` and `candidate_index_bytes`
(`store.rs:406-461`) — **no connection, and no SQLite status counter**.

Consequences, and what was done instead:

- The gate sample `S1` reports the *product's* counters (inserted, reused, packs,
  commits, full/prefix records, pool lane) plus the pragma read-backs and status
  counters from a **separate inspection connection opened after the save**. Those
  status values belong to *that* connection and are reported with an explicit
  note saying so; they are not offered as the save's spill count.
- The **diagnostic A/B is where the counter is read on the connection that
  actually performed the work**, which is why the A/B carries the answer. Its
  limitation is that it replays the row/transaction *shape* (8,191 and 32,764
  single-row `INSERT`s in one transaction, 4,096-byte pages, MEMORY journal,
  `synchronous = OFF`) rather than the product's own connection object.
- The product's own transaction behaviour in the gate sample is described by
  `commits 31`: at 8,191 rows and 1,023,875 canonical bytes the save opened and
  committed **31** transactions. That is the *row* trigger doing its job — the
  transaction ceiling is 8,191 **rows**, and each pooled-metadata group also
  inserts a pack row (`sel.group`, `pack_appends 8169` plus `packs_created 33` =
  **8,202** rows in the transaction), so the save commits roughly once per 265
  objects. **No single product transaction is anywhere near 4 MiB**, which is
  consistent with, and helps explain, the zero spill.

## 5. Verdict

**Determined: no spilling occurs** at core's transaction sizes under the default
2 MiB page cache with `journal_mode = MEMORY`, and the reachable
`SQLITE_DBSTATUS_CACHE_SPILL` counter is proven live by a control (1,032 spills
when the cache is forced to 8 pages). The strongest available support is the
diagnostic pair at **4×** the declared ceiling, where both the 2 MiB/ON and the
32 MiB/OFF profiles report **zero** spills and agree on every other observable.

**The limits, stated so they are not over-read:**

1. The counter is read on harness-owned connections for a replayed row shape, not
   on the product's private save connection (§4). A product-side observation needs
   `MutationOwner::connection()` (or an equivalent status accessor) to be publicly
   reachable — **proposed as a separate change**, not made here.
2. The diagnostic arms use a simplified `objects`-shaped table with one row per
   object, not the product's real 7-column layout with its indexes; a heavier row
   would put more bytes per page and could shift the threshold.
3. `PRAGMA cache_spill` is reported as policy only (§2.2) and does not itself
   establish occurrence.
4. These numbers are single samples on one host (Apple M3 Max) and carry no
   cross-host claim; the *counters* were bit-identical on re-run where re-run
   (§6), and only the diagnostic-grade `elapsed_ns` moved.

## 6. What this means for P2-1 / O1

The Phase 0 question was whether spilling makes O1 (`cache_size` + `cache_spill`)
real. On this evidence **spilling does not occur at core's transaction sizes**, so
the *mechanism* O1 was premised on — mid-transaction dirty-page overflow and
rewrite — is **not** active here. That does not by itself close P2-1: a larger
page cache can still change hit rates on the read path, which is why the study
itself says read-path evaluation must wait for **P1-2** (connection pooling). What
this receipt does settle is that the write-path half of the O1 premise is
unsupported: **no mid-transaction spill was observed at 1× or 4× the ceiling.**
The re-prioritization decision belongs to the owner.

## 7. Instrument control source (for reproduction)

`/tmp/spill-control/src/main.rs` in the collection environment; reproduced here so
the control is not lost:

```rust
use rusqlite::Connection;
fn main() {
    let path = std::env::temp_dir().join("layerfs-spill-control.sqlite");
    let _ = std::fs::remove_file(&path);
    let c = Connection::open(&path).unwrap();
    c.execute_batch("PRAGMA journal_mode = MEMORY; PRAGMA synchronous = OFF; \
                     PRAGMA cache_size = 8; PRAGMA cache_spill = ON;").unwrap();
    c.execute_batch("CREATE TABLE t(a INTEGER PRIMARY KEY, b BLOB)").unwrap();
    c.execute_batch("BEGIN IMMEDIATE").unwrap();
    {
        let mut s = c.prepare("INSERT INTO t VALUES (?1, ?2)").unwrap();
        for i in 0..20000i64 {
            s.execute(rusqlite::params![i, vec![7u8; 200]]).unwrap();
        }
    }
    c.execute_batch("COMMIT").unwrap();
    let mut cur: std::os::raw::c_int = 0;
    let mut hi: std::os::raw::c_int = 0;
    let ok = unsafe { rusqlite::ffi::sqlite3_db_status(
        c.handle(), rusqlite::ffi::SQLITE_DBSTATUS_CACHE_SPILL, &mut cur, &mut hi, 1,
    ) == rusqlite::ffi::SQLITE_OK };
    let cs: i64 = c.query_row("PRAGMA cache_size", [], |r| r.get(0)).unwrap();
    println!("control cache_size {} spill_ok {} spill_value {}", cs, ok, cur);
}
```
