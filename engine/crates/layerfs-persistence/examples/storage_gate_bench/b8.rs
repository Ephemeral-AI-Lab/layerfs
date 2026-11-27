//! B8 catalog scaling (and the B5 statement row): the real quarry locators
//! (`r758.lvar`, 1,693,320 rows with chained-varint ancestor lists) at 1x,
//! and a synthetic 10x copy, in bench-owned schemas, as resumable steps.
//!
//! **Copy rule.** Copy `c` (0..=9) of every `r758.lvar` row keeps role,
//! canonical length, group, record and the `ancestors` bytes verbatim, and
//! shifts its pack id by `c * 2^25` (above quarry's largest id 18,939,906).
//! Pack ids inside a copy therefore keep quarry's distribution and stride,
//! and because a list entry codes its pack relative to the previous entry
//! (entry 0 relative to the row's own pack), every copied list still names
//! the copy's own packs. Copy 0 keeps the real object ids, so the same
//! 4,096-root samples resolve identically at 1x and 10x; copies 1..9 get
//! fresh ids `sha256(int4send(c) || id)`.
//!
//! **Packs.** One `pack` row per distinct quarry pack per copy (38,677 per
//! copy). `directory` has the pack's real directory length: 24 header bytes
//! plus 4 per group in the whole-file lane (a pack holding role 1 records)
//! or 16 per group otherwise, groups = largest group number + 1 (this gives
//! quarry's measured 72.3 B mean, 2.80 MB total). Its bytes are filler. The
//! chain statement reads directories, never bodies, so `body` holds the
//! directory bytes only, and `length` is the real body length from
//! research758 `packs.tsv` when given (`--pack-lengths`), else the
//! directory length.
//!
//! **Statement under test.** Read from a file; `{schema}` is replaced by the
//! schema name. Parameter `$1` is the sample's ids as `bytea[]` and, when
//! the statement has a second parameter, `$2` their prefix keys as
//! `bigint[]` (first 8 id bytes, big-endian signed), the shape of today's
//! locate statement and of the product's chain statement 1
//! (`sql/postgresql/chain_roots.sql`, #758 phase 2). A statement whose one
//! parameter is `bigint[]` is chain statement 2
//! (`sql/postgresql/chain_packs.sql`): `$1` is then the member pack ids the
//! sample's ancestor lists name beyond its roots' own packs, found by
//! running the product's statement 1 (compiled in from the same file) and
//! decoding the lists, unmeasured, before the warm-up.
//!
//! **Measurement (declared warm server cache).** Per sample: one unmeasured
//! execution of the statement, then one `EXPLAIN (ANALYZE, BUFFERS, TIMING
//! OFF, SUMMARY ON, FORMAT JSON)` execution (the B5 baseline used ANALYZE,
//! TIMING OFF, SUMMARY ON). One sample per case.
//!
//! **B5 real-history samples (gate-tooling round, added rows).** `b8 samples
//! ... --commit-packs FILE` also inserts samples `early` and `late`: 4,096
//! `r758.lvar` rows each (first by `sha256(id)`, as `deep`) from the objects
//! of the first and the last 10 % of quarry's commits. `r758` holds no
//! commits, so the order comes from quarry's own Store (research758
//! `store.sqlite`, read only): FILE has one line per commit, `history_serial
//! created_ms root_pack_id`, in `history_serial` order (serials 1..4,514,
//! never decreasing in `created_ms`), where `root_pack_id` is the pack holding
//! the commit's root object. An object's commit is not recorded, so its pack
//! id is the proxy: `early` = rows with `pack_id <=` the largest root pack of
//! the first 10 % of commits (this includes Project Init's packs, which
//! precede commit 1); `late` = rows with `pack_id >` the smallest root pack of
//! the last 10 %. Pack ids are reserved in 4,096-id blocks per writer, so
//! root packs are not strictly monotonic in commit order (131 of 4,513 steps
//! decrease); taking the max / min over each tenth keeps every commit of the
//! tenth inside its bucket. `measure` then prints `b8_statement:` lines for
//! these kinds too, and one `b5_real_history:` line (late / early ratios).
//! The `random` and `deep` lines are unchanged.
use crate::monitor::{self, Monitor, TAG};
use postgres::types::{FromSql, Type};
use postgres::Client;

const OFFSET: i64 = 1 << 25;
const COPY_GUEST_BYTES: u64 = 400_000_000;
const GUEST_FLOOR: u64 = 20 * (1 << 30);

struct RawText(String);
impl<'a> FromSql<'a> for RawText {
    fn from_sql(_: &Type, raw: &'a [u8]) -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        // jsonb's binary form is a version byte then text; json is text.
        let raw = if raw.first() == Some(&1) && !raw.starts_with(b"[") {
            &raw[1..]
        } else {
            raw
        };
        Ok(RawText(String::from_utf8(raw.to_vec())?))
    }
    fn accepts(_: &Type) -> bool {
        true
    }
}

/// The prefix key the product's index uses: the first 8 id bytes as a
/// big-endian signed integer.
pub fn key(id: &[u8]) -> i64 {
    i64::from_be_bytes(id[..8].try_into().unwrap())
}

pub struct Explain {
    pub exec_ms: f64,
    pub plan_ms: f64,
    pub hit: i64,
    pub read: i64,
    pub rows: i64,
    pub plan: String,
    pub plan_md5: String,
}

impl Explain {
    pub fn line(&self, ids: usize) -> String {
        format!(
            "ids={ids} exec_ms={:.3} plan_ms={:.3} shared_hit={} shared_read={} buffers={} rows={} plan_md5={} plan=[{}]",
            self.exec_ms,
            self.plan_ms,
            self.hit,
            self.read,
            self.hit + self.read,
            self.rows,
            self.plan_md5,
            self.plan
        )
    }
}

/// One warm-up execution, then one EXPLAIN ANALYZE of `sql` over `ids`.
pub fn explain(client: &mut Client, schema: &str, sql: &str, ids: &[Vec<u8>]) -> Explain {
    let sql = sql.replace("{schema}", schema);
    let sql = sql.trim().trim_end_matches(';');
    let keys: Vec<i64> = ids.iter().map(|i| key(i)).collect();
    let mut packs: Option<Vec<i64>> = None;
    let mut run = |client: &mut Client, text: &str| {
        let statement = client
            .prepare(text)
            .unwrap_or_else(|e| panic!("{text}: {e}"));
        match statement.params().len() {
            1 if statement.params()[0] == Type::INT8_ARRAY => {
                let packs = packs.get_or_insert_with(|| member_packs(client, schema, ids, &keys));
                client.query(&statement, &[&*packs])
            }
            1 => client.query(&statement, &[&ids]),
            2 => client.query(&statement, &[&ids, &keys]),
            n => panic!(
                "statement takes {n} parameters; the harness binds $1 ids and $2 keys, or $1 member pack ids"
            ),
        }
        .unwrap_or_else(|e| panic!("{text}: {e}"))
    };
    run(client, sql);
    let rows = run(
        client,
        &format!("EXPLAIN (ANALYZE, BUFFERS, TIMING OFF, SUMMARY ON, FORMAT JSON) {sql}"),
    );
    let json = rows[0].get::<_, RawText>(0).0;
    let r = client
        .query_one(
            &format!(
                "{TAG} SELECT (j->0->>'Execution Time')::float8, (j->0->>'Planning Time')::float8, \
                 (j->0->'Plan'->>'Shared Hit Blocks')::bigint, (j->0->'Plan'->>'Shared Read Blocks')::bigint, \
                 (j->0->'Plan'->>'Actual Rows')::bigint, \
                 (SELECT string_agg(concat_ws(':', n->>'Node Type', n->>'Relation Name', n->>'Index Name'), ',' ORDER BY o) \
                  FROM jsonb_path_query(j, 'strict $.** ? (exists (@.\"Node Type\"))') WITH ORDINALITY q(n,o)) \
                 FROM (SELECT $1::text::jsonb AS j) s"
            ),
            &[&json],
        )
        .expect("explain json");
    let plan: String = r.get(5);
    let plan_md5: String = client
        .query_one(&format!("{TAG} SELECT left(md5($1),12)"), &[&plan])
        .unwrap()
        .get(0);
    Explain {
        exec_ms: r.get(0),
        plan_ms: r.get(1),
        hit: r.get(2),
        read: r.get(3),
        rows: r.get(4),
        plan,
        plan_md5,
    }
}

/// The product's chain statement 1 (#758 phase 2).
const CHAIN_ROOTS: &str = include_str!("../../sql/postgresql/metadata/chain/roots.sql");

/// The packs the ancestor lists of `ids` name that are not the roots' own:
/// chain statement 2's `$1`. Lists use the D1 entry form
/// `varint(zigzag(pack - previous)) || group || varint(record) ||
/// varint(length - 1)`, `previous` starting at the row's own pack.
fn member_packs(client: &mut Client, schema: &str, ids: &[Vec<u8>], keys: &[i64]) -> Vec<i64> {
    let sql = CHAIN_ROOTS.replace("{schema}", schema);
    let rows = client
        .query(sql.trim().trim_end_matches(';'), &[&ids, &keys])
        .unwrap_or_else(|e| panic!("chain statement 1: {e}"));
    let (mut own, mut named) = (
        std::collections::BTreeSet::new(),
        std::collections::BTreeSet::new(),
    );
    for row in &rows {
        let pack: i64 = row.get(6);
        own.insert(pack);
        if row.get::<_, i32>(0) != 0 {
            continue;
        }
        let Some(list) = row.get::<_, Option<Vec<u8>>>(7) else {
            continue;
        };
        let (mut at, mut previous) = (0, pack);
        let varint = |at: &mut usize| {
            let (mut value, mut shift) = (0u64, 0);
            loop {
                let byte = list[*at];
                *at += 1;
                value |= u64::from(byte & 0x7f) << shift;
                shift += 7;
                if byte < 0x80 {
                    return value;
                }
            }
        };
        while at < list.len() {
            let zigzag = varint(&mut at);
            previous += ((zigzag >> 1) as i64) ^ -((zigzag & 1) as i64);
            named.insert(previous);
            at += 1;
            varint(&mut at);
            varint(&mut at);
        }
    }
    named.difference(&own).copied().collect()
}

fn arg(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn guard(schema: &str) {
    if ["r758", "public", "gate_state"].contains(&schema) || schema.starts_with("pg_") {
        panic!("refusing to use schema {schema}");
    }
}

fn marked(m: &mut Monitor, schema: &str) -> bool {
    m.i64(&format!(
        "SELECT count(*) FROM pg_tables WHERE schemaname='{schema}' AND tablename='b8_marker'"
    )) == 1
}

pub fn run(args: &[String]) {
    let step = args.get(2).map(String::as_str).unwrap_or("");
    if step == "space" {
        println!(
            "b8 space: guest_free_bytes={:?}",
            monitor::guest_free_bytes()
        );
        return;
    }
    let (url, schema) = (&args[3], args[4].as_str());
    guard(schema);
    let mut m = Monitor::connect(url);
    match step {
        "create" => create(&mut m, schema, arg(args, "--pack-lengths")),
        "load" => load(
            &mut m,
            schema,
            arg(args, "--copy").expect("--copy C").parse().unwrap(),
        ),
        "index" => {
            assert!(marked(&mut m, schema), "not a B8 schema");
            m.batch(&format!(
                "SET maintenance_work_mem='1GB'; CREATE INDEX object_location_prefix ON {schema}.object_location \
                 (((('x'||encode(substring(object_id FROM 1 FOR 8),'hex'))::bit(64)::bigint)))"
            ));
            println!("b8 index {schema}: object_location_prefix built");
        }
        "analyze" => {
            assert!(marked(&mut m, schema), "not a B8 schema");
            // VACUUM cannot run inside a multi-statement batch.
            m.batch(&format!("VACUUM (ANALYZE) {schema}.object_location"));
            m.batch(&format!("VACUUM (ANALYZE) {schema}.pack"));
            println!("b8 analyze {schema}: done");
        }
        "sql" => {
            // HOOK(#758 phase 2): extra 2.0 DDL (indexes, early-freeze
            // reloptions) the product's schema adds, `{schema}` substituted.
            assert!(marked(&mut m, schema), "not a B8 schema");
            let text = std::fs::read_to_string(&args[5]).unwrap();
            m.batch(&text.replace("{schema}", schema));
            println!("b8 sql {schema}: applied {}", args[5]);
        }
        "samples" => samples(&mut m, schema, arg(args, "--commit-packs")),
        "measure" => measure(&mut m, schema, args),
        "drop" => {
            assert!(marked(&mut m, schema), "not a B8 schema: refusing to drop");
            m.batch(&format!("DROP SCHEMA {schema} CASCADE"));
            println!(
                "b8 drop {schema}: dropped; guest_free_bytes={:?}",
                monitor::guest_free_bytes()
            );
        }
        _ => panic!("b8 space | create | load | index | analyze | sql | samples | measure | drop"),
    }
}

fn create(m: &mut Monitor, schema: &str, lengths: Option<String>) {
    m.batch(&format!(
        "CREATE SCHEMA {schema}; \
         CREATE TABLE {schema}.b8_marker (copy int PRIMARY KEY, rows bigint NOT NULL, packs bigint NOT NULL); \
         CREATE TABLE {schema}.object_location (pack_id bigint NOT NULL, canonical_length integer NOT NULL, \
           role smallint NOT NULL, group_number smallint NOT NULL, record_number smallint NOT NULL, \
           object_id bytea NOT NULL, ancestors bytea); \
         ALTER TABLE {schema}.object_location ALTER COLUMN ancestors SET STORAGE PLAIN; \
         CREATE TABLE {schema}.pack (pack_id bigint PRIMARY KEY, domain bigint NOT NULL, digest bytea NOT NULL, \
           length bigint NOT NULL, body bytea NOT NULL, directory bytea NOT NULL); \
         ALTER TABLE {schema}.pack ALTER COLUMN directory SET STORAGE PLAIN, ALTER COLUMN body SET STORAGE EXTERNAL; \
         CREATE TABLE {schema}.b8_src_pack (pack_id bigint PRIMARY KEY, length bigint, directory_length int NOT NULL); \
         INSERT INTO {schema}.b8_src_pack SELECT pack_id, NULL, \
           24 + (CASE WHEN bool_or(role=1) THEN 4 ELSE 16 END)*(max(group_number)+1) FROM r758.lvar GROUP BY pack_id"
    ));
    if let Some(path) = lengths {
        let text = std::fs::read_to_string(&path).unwrap();
        let (mut ids, mut lens) = (Vec::new(), Vec::new());
        for line in text.lines() {
            let mut f = line.split_whitespace();
            ids.push(f.next().unwrap().parse::<i64>().unwrap());
            lens.push(f.next().unwrap().parse::<i64>().unwrap());
        }
        m.client
            .execute(
                &format!(
                    "{TAG} UPDATE {schema}.b8_src_pack s SET length=u.l FROM unnest($1::bigint[],$2::bigint[]) u(p,l) WHERE s.pack_id=u.p"
                ),
                &[&ids, &lens],
            )
            .unwrap();
    }
    m.batch(&format!(
        "UPDATE {schema}.b8_src_pack SET length=directory_length WHERE length IS NULL OR length < directory_length"
    ));
    let r = m
        .client
        .query_one(
            &format!(
                "{TAG} SELECT count(*), avg(directory_length)::float8, sum(directory_length)::bigint, sum(length)::bigint FROM {schema}.b8_src_pack"
            ),
            &[],
        )
        .unwrap();
    println!(
        "b8 create {schema}: source_packs={} directory_mean={:.1} directory_total={} body_length_total={}",
        r.get::<_, i64>(0),
        r.get::<_, f64>(1),
        r.get::<_, i64>(2),
        r.get::<_, i64>(3)
    );
}

fn load(m: &mut Monitor, schema: &str, copy: i32) {
    assert!(marked(m, schema), "not a B8 schema: run create first");
    assert!((0..=9).contains(&copy), "copy 0..=9");
    if m.i64(&format!(
        "SELECT count(*) FROM {schema}.b8_marker WHERE copy={copy}"
    )) == 1
    {
        panic!("copy {copy} is already loaded in {schema}");
    }
    let free = monitor::guest_free_bytes().expect("Docker guest free space");
    if free < GUEST_FLOOR + COPY_GUEST_BYTES {
        panic!("Docker guest has {free} bytes free; a copy needs about {COPY_GUEST_BYTES} above the 20 GiB floor");
    }
    let off = OFFSET * i64::from(copy);
    let started = std::time::Instant::now();
    m.batch(&format!(
        "BEGIN; SET LOCAL work_mem='512MB'; \
         INSERT INTO {schema}.object_location SELECT pack_id+{off}, canonical_length, role, group_number, record_number, \
           CASE WHEN {copy}=0 THEN object_id ELSE sha256(int4send({copy})||object_id) END, ancestors \
           FROM r758.lvar ORDER BY pack_id, group_number, record_number; \
         INSERT INTO {schema}.pack SELECT p, CASE WHEN length>262144 THEN 1 ELSE 0 END, sha256(int8send(p)), length, d, d FROM ( \
           SELECT pack_id+{off} AS p, length, substring(decode(repeat(encode(sha512(int8send(pack_id+{off})),'hex'), \
             directory_length/64+1),'hex') FROM 1 FOR directory_length) AS d FROM {schema}.b8_src_pack) x; \
         INSERT INTO {schema}.b8_marker SELECT {copy}, (SELECT count(*) FROM r758.lvar), (SELECT count(*) FROM {schema}.b8_src_pack); \
         COMMIT"
    ));
    println!(
        "b8 load {schema}: copy={copy} wall_s={:.1} guest_free_bytes_before={free} after={:?}",
        started.elapsed().as_secs_f64(),
        monitor::guest_free_bytes()
    );
}

/// The number of chained-varint entries in an ancestor list.
fn entries(list: &[u8]) -> usize {
    let (mut at, mut n) = (0, 0);
    let varint = |at: &mut usize| {
        while list[*at] & 0x80 != 0 {
            *at += 1;
        }
        *at += 1;
    };
    while at < list.len() {
        varint(&mut at); // zigzag pack delta
        at += 1; // group
        varint(&mut at); // record
        varint(&mut at); // canonical length - 1
        n += 1;
    }
    n
}

fn samples(m: &mut Monitor, schema: &str, commit_packs: Option<String>) {
    assert!(marked(m, schema), "not a B8 schema");
    m.batch(&format!(
        "CREATE TABLE {schema}.b8_sample (kind text, n int, object_id bytea NOT NULL, PRIMARY KEY (kind,n)); \
         INSERT INTO {schema}.b8_sample SELECT 'random', row_number() OVER (), i FROM r758.roots4096"
    ));
    let rows = m
        .client
        .query(
            &format!(
                "{TAG} SELECT object_id, ancestors FROM r758.lvar WHERE octet_length(ancestors) >= 56 ORDER BY sha256(object_id)"
            ),
            &[],
        )
        .unwrap();
    let mut deep: Vec<Vec<u8>> = Vec::new();
    let (mut candidates, mut depth_sum) = (0usize, 0usize);
    for r in &rows {
        let depth = entries(r.get::<_, &[u8]>(1));
        if depth >= 14 {
            candidates += 1;
            if deep.len() < 4096 {
                depth_sum += depth;
                deep.push(r.get::<_, Vec<u8>>(0));
            }
        }
    }
    m.client
        .execute(
            &format!(
                "{TAG} INSERT INTO {schema}.b8_sample SELECT 'deep', n, i FROM unnest($1::bytea[]) WITH ORDINALITY u(i,n)"
            ),
            &[&deep],
        )
        .unwrap();
    println!(
        "b8 samples {schema}: random=4096 (r758.roots4096) deep={} of {candidates} rows at depth>=14 (first by sha256(id)) deep_mean_depth={:.1}",
        deep.len(),
        depth_sum as f64 / deep.len().max(1) as f64
    );
    if let Some(path) = commit_packs {
        history_samples(m, schema, &path);
    }
}

/// The B5 real-history samples `early` and `late` (see the module doc).
fn history_samples(m: &mut Monitor, schema: &str, path: &str) {
    let text = std::fs::read_to_string(path).unwrap();
    let mut commits: Vec<(i64, i64, i64)> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let f: Vec<i64> = l.split_whitespace().map(|v| v.parse().unwrap()).collect();
            assert_eq!(
                f.len(),
                3,
                "commit-packs line: serial created_ms root_pack_id"
            );
            (f[0], f[1], f[2])
        })
        .collect();
    commits.sort();
    let n = commits.len();
    assert!(n >= 10, "need at least 10 commits");
    let tenth = n.div_ceil(10);
    let early_max = commits[..tenth].iter().map(|c| c.2).max().unwrap();
    let late_min = commits[n - tenth..].iter().map(|c| c.2).min().unwrap();
    let decreases = commits.windows(2).filter(|w| w[1].2 < w[0].2).count();
    for (kind, cond) in [
        ("early", format!("pack_id <= {early_max}")),
        ("late", format!("pack_id > {late_min}")),
    ] {
        let rows = m
            .client
            .query(
                &format!(
                    "{TAG} SELECT object_id, coalesce(ancestors, ''::bytea) FROM r758.lvar WHERE {cond} ORDER BY sha256(object_id)"
                ),
                &[],
            )
            .unwrap();
        let candidates = rows.len();
        let picked: Vec<&postgres::Row> = rows.iter().take(4096).collect();
        let depth_sum: usize = picked.iter().map(|r| entries(r.get::<_, &[u8]>(1))).sum();
        let full = picked
            .iter()
            .filter(|r| r.get::<_, &[u8]>(1).is_empty())
            .count();
        let ids: Vec<Vec<u8>> = picked.iter().map(|r| r.get::<_, Vec<u8>>(0)).collect();
        m.client
            .execute(
                &format!(
                    "{TAG} INSERT INTO {schema}.b8_sample SELECT $1, n, i FROM unnest($2::bytea[]) WITH ORDINALITY u(i,n)"
                ),
                &[&kind, &ids],
            )
            .unwrap();
        println!(
            "b8 samples {schema}: {kind}={} of {candidates} rows ({cond}; first by sha256(id)) mean_depth={:.2} full_rows={full}",
            ids.len(),
            depth_sum as f64 / ids.len().max(1) as f64
        );
    }
    println!(
        "b8 samples {schema}: commits={n} tenth={tenth} early_commits=1..{tenth} late_commits={}..{n} early_max_root_pack={early_max} late_min_root_pack={late_min} root_pack_decreases={decreases} proxy=pack_id (commit-packs {path})",
        n - tenth + 1
    );
}

fn measure(m: &mut Monitor, schema: &str, args: &[String]) {
    let path = arg(args, "--sql").expect("--sql <statement file>");
    let label = arg(args, "--label").unwrap_or_else(|| schema.to_owned());
    let sql = std::fs::read_to_string(&path).unwrap();
    let r = m
        .client
        .query_one(
            &format!(
                "{TAG} SELECT (SELECT count(*) FROM {s}.object_location), pg_table_size('{s}.object_location')::bigint, \
                 pg_indexes_size('{s}.object_location')::bigint, (SELECT count(*) FROM {s}.pack), \
                 (SELECT sum(octet_length(directory))::bigint FROM {s}.pack), \
                 (SELECT string_agg(copy::text, ',' ORDER BY copy) FROM {s}.b8_marker)",
                s = schema
            ),
            &[],
        )
        .unwrap();
    let (rows, heap, index): (i64, i64, i64) = (r.get(0), r.get(1), r.get(2));
    println!(
        "{label} b8_catalog: copies=[{}] rows={rows} heap_bytes={heap} index_bytes={index} bytes_per_locator={:.1} packs={} directory_bytes={} statement={path}",
        r.get::<_, String>(5),
        (heap + index) as f64 / rows as f64,
        r.get::<_, i64>(3),
        r.get::<_, i64>(4)
    );
    let mut history: Vec<(&str, Explain)> = Vec::new();
    for kind in ["random", "deep", "early", "late"] {
        let ids: Vec<Vec<u8>> = m
            .client
            .query(
                &format!("{TAG} SELECT object_id FROM {schema}.b8_sample WHERE kind=$1 ORDER BY n"),
                &[&kind],
            )
            .unwrap()
            .iter()
            .map(|r| r.get(0))
            .collect();
        if ids.is_empty() && matches!(kind, "early" | "late") {
            continue; // the B5 real-history samples exist only with --commit-packs
        }
        let e = explain(&mut m.client, schema, &sql, &ids);
        println!("{label} b8_statement: sample={kind} {}", e.line(ids.len()));
        if matches!(kind, "early" | "late") {
            history.push((kind, e));
        }
    }
    if let [(_, early), (_, late)] = history.as_slice() {
        println!(
            "{label} b5_real_history: early_exec_ms={:.3} late_exec_ms={:.3} late_over_early_exec={:.3} early_buffers={} late_buffers={} late_over_early_buffers={:.3} early_rows={} late_rows={} plans_equal={}",
            early.exec_ms,
            late.exec_ms,
            late.exec_ms / early.exec_ms,
            early.hit + early.read,
            late.hit + late.read,
            (late.hit + late.read) as f64 / (early.hit + early.read) as f64,
            early.rows,
            late.rows,
            early.plan_md5 == late.plan_md5
        );
    } else {
        println!("{label} b5_real_history: NOT_RUN (no early/late samples; run b8 samples with --commit-packs)");
    }
}
