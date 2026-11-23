//! #758 release-gate benchmark rows B1-B4, B7 and B8 (spec `SPEC-758-2.0.md`
//! section 5), each one bounded command, longer rows as resumable steps.
//!
//! Build once (release, as the baselines were):
//!
//! ```text
//! cargo +1.85.1 build --locked -j 6 --release -p layerfs-persistence --example storage_gate_bench
//! BIN=target/release/examples/storage_gate_bench
//! U="host=127.0.0.1 port=55758 user=postgres dbname=postgres sslmode=disable"
//! export LAYERFS_CONSTRUCTION_WORKERS=1
//! ```
//!
//! Server: the dedicated bench container `lfs-bench-758-pg` (published port
//! 55758; `GATE_PG_CONTAINER` overrides the name used for wire bytes, guest
//! space and netem). Shipped defaults (`synchronous_commit = local`); `SYNC=off`
//! only for B6-style diagnostics. Add `--strict-hints` to every read row once
//! Phase 1's switch is wired (`common::strict`, marked HOOK); until then it
//! is refused as NOT_WIRED.
//!
//! | Row | Commands, in order | Key output |
//! |---|---|---|
//! | B1 | `$BIN b1 postgresql "$U" g0_b1 g0` | `write:` `catalog:` `cold_read:` (as the baselines), `cold_read_counters:`, `cold_read_bytes:` (model, ratios), `cold_read_server:` |
//! | B2 | `$BIN netem add`; `$BIN b1 postgresql "$U" g0_b2 g0+1ms`; `$BIN netem del` | as B1; `arm: rtt_select1_median_us` shows the delay |
//! | B3 | `$BIN reset "$U" g0_b3`; `$BIN b3 build "$U" g0_b3 --to 4`; `... --to 7`; `... --to 10`; `$BIN b3 read "$U" g0_b3` | `b3_save:` per version; `b3_cold_read:` (`locate_waves`, `round_trips`, `a8_budget_round_trips`), `b3_cold_read_bytes:` (`model_bytes`, ratios) |
//! | B4 | `$BIN b4 save "$U" g0_b3` (after B3) | `b4_shape:`, `b4_construct:`, `b4_save:` (`packs_added`, `shape_ok`, `base_fetch_round_trips`, `publication_calls`) |
//! | B7 | `$BIN reset "$U" g0_b7`; `b7 grow --to 100`; `b7 probe`; `b7 grow --to 400`; `b7 probe`; `b7 grow --to 1000`; `b7 grow --to 1600`; `b7 probe` (each `"$U" g0_b7`, probes with `--chain-sql FILE` once Phase 2 exists) | `b7_grow:`, `b7_catalog:`, `b7_cold_read:`, `b7_chain_statement:`, `b7_save:` |
//! | B8 | per size S in {`g0_b8_x1` (copy 0), `g0_b8_x10` (copies 0-9)}: `b8 create "$U" S --pack-lengths packs.tsv`; `b8 load "$U" S --copy C` per copy; `b8 index`; [`b8 sql "$U" S extra.sql`]; `b8 analyze`; `b8 samples`; `b8 measure "$U" S --sql STATEMENT --label L`; after both sizes, `b8 drop` each | `b8_catalog:` (`bytes_per_locator`), `b8_statement:` per sample (`exec_ms`, `buffers`, `plan_md5`, `plan`) |
//!
//! `b8 measure` with `storage_gate_bench/b8_locate.sql` runs today's locate
//! statement; the Phase 2 chain statement goes in a file of the same form
//! (`{schema}`, `$1` ids `bytea[]`, optional `$2` keys `bigint[]`). The
//! `g0_b8_x1` measurement is also the B5 row (real locators, product row).
//! `b8 space` prints the Docker guest's free bytes; `load` refuses a copy
//! that would leave under 20 GiB.
//!
//! **Counters.** `statements`/`round_trips`/`prepares` are the port's own
//! `SqlWork` (the baselines' counters). Server time and the per-class call
//! counts come from `pg_stat_statements`, reset before each phase, the
//! bench's own statements excluded (`monitor`). Wire bytes are the server
//! container's `eth0` counters read before and after, outside timed regions.
//! The needed-groups model is in `storage_gate_bench/model.rs`.
//!
//! **Cache state (declared).** As the baselines: each read row's reader is a
//! fresh `Storage` (empty locator and pack caches); the server's buffers are
//! warm (the Store was just written). B8 runs one unmeasured execution before
//! its one measured EXPLAIN. One sample per case; nothing is rerun for a
//! number. Size overrides (`VERSIONS`, `FILES_*`, `B3_MIB`, `B4_FILES`) exist
//! for smoke runs only; rows print `gate_size=false` when one is set.
//!
//! Every command fits the 2-minute limit at gate size except where the
//! how-to (`758-impl/phase0-bench-howto.md`) says otherwise; split B3/B7
//! steps further with smaller `--to` increments if a step approaches it.

#[path = "storage_gate_bench/b1.rs"]
mod b1;
#[path = "storage_gate_bench/b7.rs"]
mod b7;
#[path = "storage_gate_bench/b8.rs"]
mod b8;
#[path = "storage_gate_bench/common.rs"]
mod common;
#[path = "storage_gate_bench/large.rs"]
mod large;
#[path = "storage_gate_bench/model.rs"]
mod model;
#[path = "storage_gate_bench/monitor.rs"]
mod monitor;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let row = args.get(1).map(String::as_str).unwrap_or("");
    let step = args.get(2).map(String::as_str).unwrap_or("");
    match (row, step) {
        ("b1", _) => b1::run(&args),
        ("b3", "build") => large::b3_build(&args),
        ("b3", "read") => large::b3_read(&args),
        ("b4", "save") => large::b4_save(&args),
        ("b7", "grow") => b7::grow(&args),
        ("b7", "probe") => b7::probe(&args),
        ("b8", _) => b8::run(&args),
        ("netem", action) => std::process::exit(monitor::netem(action)),
        ("reset", _) => large::reset(&args),
        _ => {
            eprintln!(
                "usage: storage_gate_bench b1 (postgresql <conninfo> <schema> | sqlite <dir>) <label> [--strict-hints]\n\
                 \x20      storage_gate_bench b3 build <conninfo> <schema> --to N | b3 read <conninfo> <schema>\n\
                 \x20      storage_gate_bench b4 save <conninfo> <schema>\n\
                 \x20      storage_gate_bench b7 grow <conninfo> <schema> --to N | b7 probe <conninfo> <schema> [--chain-sql F]\n\
                 \x20      storage_gate_bench b8 space | create | load | index | sql | analyze | samples | measure | drop ...\n\
                 \x20      storage_gate_bench netem add|del|show | reset <conninfo> <schema>"
            );
            std::process::exit(2);
        }
    }
}
