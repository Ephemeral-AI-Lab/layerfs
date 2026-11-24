//! #758 open risk 1 DIAGNOSTIC: per-byte cost of reading pack bodies from
//! PostgreSQL in the Rust client, by stage, against the same bytes read by
//! other clients and paths. Not a qualification; one sample per arm.
//!
//! ```text
//! cargo +1.85.1 build --locked -j 6 --release -p layerfs-persistence --example pg_client_profile
//! target/release/examples/pg_client_profile "<conninfo>" <store schema>
//! ```
//!
//! The pack set is every pack of `<store schema>` (a Store written by
//! `storage_gate_bench`, so real pack lengths and bodies), read in pack-id
//! order. Cache state (declared): the server's buffers are warmed by one
//! unmeasured COPY of the set; every arm then reads the same set once.
//!
//! Arms, all over the same connection string (macOS host, published port):
//! - `server`: server-only time, `EXPLAIN (ANALYZE, SERIALIZE BINARY)` of the
//!   batched statement (detoast and serialization, nothing sent);
//! - `copy_out`: `COPY (...) TO STDOUT (FORMAT binary)` through
//!   `postgres::Client::copy_out`, bytes counted and dropped: the driver's
//!   transport floor;
//! - `rows_borrow`: one prepared statement for the whole set, binary result,
//!   each body borrowed as `&[u8]` (no copy);
//! - `rows_vec`: the same, each body decoded as `Vec<u8>` (one copy), the
//!   product's decode;
//! - `per_pack_vec`: one statement per pack (the product's per-pack round
//!   trip shape), `Vec<u8>`;
//! - `text_hex`: the simple (text) protocol, bytea as hex text, decoded;
//! - `sha256`: SHA-256 over the bodies already in memory (the product's
//!   whole-pack authentication);
//! - `port_read_packs`: the product port, `PostgresStorage::read_packs` in
//!   batches within its 2 MiB budget (statement, decode, copy, SHA-256).
//!
//! The non-Rust comparisons (psql inside the server container over its Unix
//! socket and over TCP in its network namespace, and a Python stdlib client
//! on the host over the published port) are run by
//! `storage_gate_bench/pg_copy_probe.py`; see `758-impl/phase0-client-profile.md`.
use layerfs_persistence::{PostgresHandles, PostgresSelection};
use layerfs_storage::port::PackPersistence;
use postgres::{Client, NoTls, SimpleQueryMessage};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::time::Instant;

const BINDING: &[u8] = b"delta-hint-bench";
const CURSOR: [u8; 32] = [58; 32];
const BUDGET: usize = 2 * 1024 * 1024;

fn report(arm: &str, ms: f64, bytes: u64, statements: usize) {
    println!(
        "arm={arm} ms={ms:.2} bytes={bytes} statements={statements} ms_per_mb={:.3} mb_per_s={:.1}",
        ms / (bytes as f64 / 1e6),
        bytes as f64 / 1e6 / (ms / 1e3)
    );
}

fn hex(text: &str) -> Vec<u8> {
    let raw = text.strip_prefix("\\x").expect("hex bytea").as_bytes();
    let nibble = |c: u8| match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        _ => panic!("hex digit"),
    };
    raw.chunks(2)
        .map(|p| (nibble(p[0]) << 4) | nibble(p[1]))
        .collect()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (url, schema) = (&args[1], &args[2]);
    let mut c = Client::connect(url, NoTls).unwrap();
    let packs: Vec<(i64, i64)> = c
        .query(
            &format!("SELECT pack_id, length FROM {schema}.pack ORDER BY pack_id"),
            &[],
        )
        .unwrap()
        .iter()
        .map(|r| (r.get(0), r.get(1)))
        .collect();
    let ids: Vec<i64> = packs.iter().map(|p| p.0).collect();
    let total: i64 = packs.iter().map(|p| p.1).sum();
    let list = ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",");
    let copy = format!(
        "COPY (SELECT body FROM {schema}.pack WHERE pack_id IN ({list}) ORDER BY pack_id) TO STDOUT (FORMAT binary)"
    );
    println!(
        "set: schema={schema} packs={} body_bytes={total} mean_pack={:.0}",
        packs.len(),
        total as f64 / packs.len() as f64
    );

    // Warm the server's buffers (unmeasured).
    let mut sink = Vec::new();
    c.copy_out(copy.as_str())
        .unwrap()
        .read_to_end(&mut sink)
        .unwrap();

    // server
    let plan = c
        .query(
            &format!("EXPLAIN (ANALYZE, SERIALIZE BINARY, TIMING OFF, SUMMARY ON, FORMAT TEXT) SELECT body FROM {schema}.pack WHERE pack_id = ANY($1) ORDER BY pack_id"),
            &[&ids],
        )
        .unwrap();
    for row in &plan {
        let line: String = row.get(0);
        if line.contains("Execution Time") || line.contains("Serialization") {
            println!("arm=server {}", line.trim());
        }
    }

    // copy_out
    let t = Instant::now();
    let mut reader = c.copy_out(copy.as_str()).unwrap();
    let mut buffer = vec![0u8; 1 << 20];
    let mut wire = 0u64;
    loop {
        let n = reader.read(&mut buffer).unwrap();
        if n == 0 {
            break;
        }
        wire += n as u64;
    }
    drop(reader);
    report("copy_out", t.elapsed().as_secs_f64() * 1e3, wire, 1);

    let batched = c
        .prepare(&format!(
            "SELECT body FROM {schema}.pack WHERE pack_id = ANY($1) ORDER BY pack_id"
        ))
        .unwrap();
    // rows_borrow
    let t = Instant::now();
    let rows = c.query(&batched, &[&ids]).unwrap();
    let mut n = 0u64;
    for row in &rows {
        n += row.get::<_, &[u8]>(0).len() as u64;
    }
    report("rows_borrow", t.elapsed().as_secs_f64() * 1e3, n, 1);
    drop(rows);

    // rows_vec
    let t = Instant::now();
    let rows = c.query(&batched, &[&ids]).unwrap();
    let bodies: Vec<Vec<u8>> = rows.iter().map(|r| r.get::<_, Vec<u8>>(0)).collect();
    let n: u64 = bodies.iter().map(|b| b.len() as u64).sum();
    report("rows_vec", t.elapsed().as_secs_f64() * 1e3, n, 1);
    drop(rows);

    // per_pack_vec
    let one = c
        .prepare(&format!(
            "SELECT body FROM {schema}.pack WHERE pack_id = $1"
        ))
        .unwrap();
    let t = Instant::now();
    let mut n = 0u64;
    for id in &ids {
        let row = c.query_one(&one, &[id]).unwrap();
        n += row.get::<_, Vec<u8>>(0).len() as u64;
    }
    report(
        "per_pack_vec",
        t.elapsed().as_secs_f64() * 1e3,
        n,
        ids.len(),
    );

    // text_hex
    let t = Instant::now();
    let messages = c
        .simple_query(&format!(
            "SELECT body FROM {schema}.pack WHERE pack_id IN ({list}) ORDER BY pack_id"
        ))
        .unwrap();
    let received = t.elapsed().as_secs_f64() * 1e3;
    let mut n = 0u64;
    for m in &messages {
        if let SimpleQueryMessage::Row(r) = m {
            n += hex(r.get(0).unwrap()).len() as u64;
        }
    }
    let decoded = t.elapsed().as_secs_f64() * 1e3;
    println!("arm=text_hex received_ms={received:.2} (hex text, 2x bytes on the wire)");
    report("text_hex", decoded, n, 1);
    drop(messages);

    // sha256
    let t = Instant::now();
    let mut acc = 0u8;
    for b in &bodies {
        acc ^= Sha256::digest(b)[0];
    }
    report("sha256", t.elapsed().as_secs_f64() * 1e3, n, 0);
    std::hint::black_box(acc);
    drop(bodies);

    // port_read_packs
    let handles =
        PostgresHandles::open_read_only(&PostgresSelection::new(url, schema), BINDING, CURSOR)
            .unwrap();
    let port = handles.storage.clone();
    let mut batches: Vec<Vec<i64>> = vec![Vec::new()];
    let mut held = 0usize;
    for (id, length) in &packs {
        if held + *length as usize > BUDGET && !batches.last().unwrap().is_empty() {
            batches.push(Vec::new());
            held = 0;
        }
        held += *length as usize;
        batches.last_mut().unwrap().push(*id);
    }
    let w0 = port.diagnostics().unwrap();
    let t = Instant::now();
    let mut out = Vec::new();
    let mut n = 0u64;
    for batch in &batches {
        port.read_packs(batch, &mut out).unwrap();
        n += out.iter().map(|p| p.body().len() as u64).sum::<u64>();
    }
    let ms = t.elapsed().as_secs_f64() * 1e3;
    let w1 = port.diagnostics().unwrap();
    report(
        "port_read_packs",
        ms,
        n,
        (w1.statements - w0.statements) as usize,
    );
}
