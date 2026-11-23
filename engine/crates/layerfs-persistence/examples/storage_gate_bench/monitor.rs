//! The bench's own observation connection: server statement counts and time
//! (`pg_stat_statements`), WAL, wire bytes, catalog sizes, row-update counts,
//! the resumable-step state table and the +1 ms netem arm.
//!
//! Every statement this connection sends carries the `/*gate-monitor*/` tag
//! and is excluded from the server counts, which therefore hold only the
//! Store's own statements (the bench server is dedicated to the bench).
use postgres::{Client, NoTls};
use std::process::Command;

pub const TAG: &str = "/*gate-monitor*/";

/// The container of the bench PostgreSQL server; `GATE_PG_CONTAINER`
/// overrides `lfs-bench-758-pg`.
pub fn container() -> String {
    std::env::var("GATE_PG_CONTAINER").unwrap_or_else(|_| "lfs-bench-758-pg".to_owned())
}

/// The server's message for a database error, else the driver's.
pub fn detail(e: &postgres::Error) -> String {
    e.as_db_error().map_or_else(
        || e.to_string(),
        |d| format!("{}: {}", d.code().code(), d.message()),
    )
}

pub struct Monitor {
    pub client: Client,
}

/// One class of server statements over a phase.
pub struct Class {
    pub class: String,
    pub calls: i64,
    pub exec_ms: f64,
    pub rows: i64,
    pub hit: i64,
    pub read: i64,
}

impl Monitor {
    pub fn connect(url: &str) -> Monitor {
        Monitor {
            client: Client::connect(url, NoTls).expect("monitor connection"),
        }
    }

    pub fn batch(&mut self, sql: &str) {
        self.client
            .batch_execute(&format!("{TAG} {sql}"))
            .unwrap_or_else(|e| panic!("{sql}: {}", detail(&e)));
    }

    pub fn i64(&mut self, sql: &str) -> i64 {
        self.client
            .query_one(&format!("{TAG} {sql}"), &[])
            .unwrap_or_else(|e| panic!("{sql}: {}", detail(&e)))
            .get(0)
    }

    pub fn schema_exists(&mut self, schema: &str) -> bool {
        self.client
            .query_one(
                &format!("{TAG} SELECT count(*) FROM pg_namespace WHERE nspname=$1"),
                &[&schema],
            )
            .unwrap()
            .get::<_, i64>(0)
            == 1
    }

    /// Clears `pg_stat_statements` before a phase.
    pub fn stat_reset(&mut self) {
        self.batch("SELECT pg_stat_statements_reset()");
    }

    /// Server statements since the last reset, by class. The class comes
    /// from the statement text, first match wins:
    /// `control` (BEGIN/COMMIT/ROLLBACK/SET/SAVEPOINT), `reserve`
    /// (`store_policy`), `lock` (advisory locks, the `store_lane` row lock),
    /// `publication` (INSERT/UPDATE/DELETE/COPY, `store_totals`), `locate`
    /// (`object_location`, which includes a chain statement), `pack_read`
    /// (reads of `pack` rows, directories or bodies), `meta` (bare
    /// `history_meta` incarnation checks; fenced reads that join it stay in
    /// `locate`/`pack_read`), `other`. In the synchronous driver each call
    /// is one round trip (a COPY two), plus one per cached prepare, which the
    /// client counter reports.
    pub fn stat_classes(&mut self) -> Vec<Class> {
        let rows = self
            .client
            .query(
                &format!(
                    "{TAG} SELECT c, sum(calls)::bigint, sum(total_exec_time)::float8, sum(rows)::bigint, \
                     sum(shared_blks_hit)::bigint, sum(shared_blks_read)::bigint FROM ( \
                     SELECT CASE \
                       WHEN query ~* '^\\s*(begin|commit|rollback|set|start|savepoint|release|end)\\M' THEN 'control' \
                       WHEN query ~* 'store_policy' THEN 'reserve' \
                       WHEN query ~* 'pg_advisory|store_lane' THEN 'lock' \
                       WHEN query ~* '\\m(insert|update|delete|copy)\\M' OR query ~* 'store_totals' THEN 'publication' \
                       WHEN query ~* 'object_location' THEN 'locate' \
                       WHEN query ~* '\\.pack\\M' THEN 'pack_read' \
                       WHEN query ~* 'history_meta' THEN 'meta' \
                       ELSE 'other' END AS c, calls, total_exec_time, rows, shared_blks_hit, shared_blks_read \
                     FROM pg_stat_statements WHERE dbid=(SELECT oid FROM pg_database WHERE datname=current_database()) \
                     AND query NOT LIKE '%gate-monitor%' AND query NOT LIKE '%gate_state%') s GROUP BY c ORDER BY c"
                ),
                &[],
            )
            .expect("pg_stat_statements");
        rows.iter()
            .map(|r| Class {
                class: r.get(0),
                calls: r.get(1),
                exec_ms: r.get(2),
                rows: r.get(3),
                hit: r.get(4),
                read: r.get(5),
            })
            .collect()
    }

    pub fn wal_lsn(&mut self) -> String {
        self.client
            .query_one(
                &format!("{TAG} SELECT pg_current_wal_insert_lsn()::text"),
                &[],
            )
            .unwrap()
            .get(0)
    }

    pub fn wal_bytes(&mut self, from: &str, to: &str) -> i64 {
        self.client
            .query_one(
                &format!("{TAG} SELECT pg_wal_lsn_diff($2::text::pg_lsn,$1::text::pg_lsn)::bigint"),
                &[&from, &to],
            )
            .unwrap()
            .get(0)
    }

    /// `(n_tup_upd, n_tup_del)` summed over `object_location` and `pack`.
    /// Statistics arrive asynchronously; the caller flushes first.
    pub fn tuple_updates(&mut self, schema: &str) -> (i64, i64) {
        let _ = self
            .client
            .batch_execute(&format!("{TAG} SELECT pg_stat_force_next_flush()"));
        let r = self
            .client
            .query_one(
                &format!(
                    "{TAG} SELECT coalesce(sum(n_tup_upd),0)::bigint, coalesce(sum(n_tup_del),0)::bigint \
                     FROM pg_stat_all_tables WHERE schemaname=$1 AND relname IN ('object_location','pack')"
                ),
                &[&schema],
            )
            .unwrap();
        (r.get(0), r.get(1))
    }

    /// Catalog figures of a Store schema.
    pub fn catalog(&mut self, schema: &str) -> Catalog {
        let column = |m: &mut Monitor, table: &str, column: &str| {
            m.client
                .query_one(
                    &format!(
                        "{TAG} SELECT count(*) FROM information_schema.columns \
                         WHERE table_schema=$1 AND table_name=$2 AND column_name=$3"
                    ),
                    &[&schema, &table, &column],
                )
                .unwrap()
                .get::<_, i64>(0)
                == 1
        };
        let ancestors = column(self, "object_location", "ancestors");
        let directory = column(self, "pack", "directory");
        let r = self
            .client
            .query_one(
                &format!(
                    "{TAG} SELECT pg_total_relation_size('{schema}.object_location')::bigint, \
                     (SELECT count(*) FROM {schema}.object_location), \
                     pg_total_relation_size('{schema}.pack')::bigint, \
                     (SELECT count(*) FROM {schema}.pack), \
                     (SELECT coalesce(sum(length),0)::bigint FROM {schema}.pack)"
                ),
                &[],
            )
            .unwrap();
        let (list_rows, list_bytes, list_max) = if ancestors {
            let r = self
                .client
                .query_one(
                    &format!(
                        "{TAG} SELECT count(*) FILTER (WHERE octet_length(ancestors)>0), \
                         coalesce(sum(octet_length(ancestors)),0)::bigint, \
                         coalesce(max(octet_length(ancestors)),0)::bigint FROM {schema}.object_location"
                    ),
                    &[],
                )
                .unwrap();
            (r.get(0), r.get(1), r.get(2))
        } else {
            (-1, -1, -1)
        };
        let directory_bytes = if directory {
            self.i64(&format!(
                "SELECT coalesce(sum(octet_length(directory)),0)::bigint FROM {schema}.pack"
            ))
        } else {
            -1
        };
        Catalog {
            location_bytes: r.get(0),
            locations: r.get(1),
            pack_relation_bytes: r.get(2),
            packs: r.get(3),
            pack_body_bytes: r.get(4),
            list_rows,
            list_bytes,
            list_max,
            directory_bytes,
        }
    }

    /// Median of `n` untimed-elsewhere `SELECT 1` round trips, in µs: the
    /// RTT of the arm (published port, or with netem added).
    pub fn rtt_us(&mut self, n: usize) -> f64 {
        let mut times: Vec<f64> = (0..n)
            .map(|_| {
                let t = std::time::Instant::now();
                self.client
                    .simple_query(&format!("{TAG} SELECT 1"))
                    .unwrap();
                t.elapsed().as_secs_f64() * 1e6
            })
            .collect();
        times.sort_by(|a, b| a.partial_cmp(b).unwrap());
        times[n / 2]
    }

    // ---- resumable-step state, in the bench-owned schema `gate_state` ----

    pub fn state_get(&mut self, store: &str, key: &str) -> Option<Vec<u8>> {
        self.batch("CREATE SCHEMA IF NOT EXISTS gate_state; CREATE TABLE IF NOT EXISTS gate_state.state (store text, key text, value bytea NOT NULL, PRIMARY KEY (store, key))");
        self.client
            .query_opt(
                &format!("{TAG} SELECT value FROM gate_state.state WHERE store=$1 AND key=$2"),
                &[&store, &key],
            )
            .unwrap()
            .map(|r| r.get(0))
    }

    pub fn state_set(&mut self, store: &str, key: &str, value: &[u8]) {
        self.state_get(store, key);
        self.client
            .execute(
                &format!(
                    "{TAG} INSERT INTO gate_state.state VALUES ($1,$2,$3) \
                     ON CONFLICT (store,key) DO UPDATE SET value=excluded.value"
                ),
                &[&store, &key, &value],
            )
            .unwrap();
    }

    pub fn state_u64(&mut self, store: &str, key: &str) -> Option<u64> {
        self.state_get(store, key)
            .map(|v| u64::from_le_bytes(v.try_into().expect("u64 state")))
    }

    pub fn state_set_u64(&mut self, store: &str, key: &str, value: u64) {
        self.state_set(store, key, &value.to_le_bytes());
    }

    pub fn state_clear(&mut self, store: &str) {
        self.state_get(store, "");
        self.client
            .execute(
                &format!("{TAG} DELETE FROM gate_state.state WHERE store=$1"),
                &[&store],
            )
            .unwrap();
    }
}

pub struct Catalog {
    pub location_bytes: i64,
    pub locations: i64,
    pub pack_relation_bytes: i64,
    pub packs: i64,
    pub pack_body_bytes: i64,
    /// Rows with a non-empty `ancestors` list, or -1 without the column.
    pub list_rows: i64,
    pub list_bytes: i64,
    pub list_max: i64,
    /// Sum of `pack.directory`, or -1 without the column.
    pub directory_bytes: i64,
}

impl Catalog {
    pub fn line(&self) -> String {
        format!(
            "object_location_bytes={} rows={} bytes_per_locator={:.1} pack_relation_bytes={} packs={} pack_body_bytes={} \
             list_rows={} list_bytes={} list_bytes_per_delta_row={} list_max={} directory_bytes={}",
            self.location_bytes,
            self.locations,
            self.location_bytes as f64 / self.locations.max(1) as f64,
            self.pack_relation_bytes,
            self.packs,
            self.pack_body_bytes,
            self.list_rows,
            self.list_bytes,
            if self.list_rows > 0 {
                format!("{:.1}", self.list_bytes as f64 / self.list_rows as f64)
            } else {
                "NA".to_owned()
            },
            self.list_max,
            self.directory_bytes,
        )
    }
}

/// Server-side `(tx, rx)` byte counters of the container's `eth0`: bytes the
/// server sent and received, protocol framing included. Read with `docker
/// exec` outside every timed region. `None` when the container is not
/// reachable (the row then prints `NA`).
pub fn wire_bytes() -> Option<(u64, u64)> {
    let out = Command::new("docker")
        .args([
            "exec",
            &container(),
            "cat",
            "/sys/class/net/eth0/statistics/tx_bytes",
            "/sys/class/net/eth0/statistics/rx_bytes",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8(out.stdout).ok()?;
    let mut lines = text.lines().map(|l| l.trim().parse::<u64>());
    Some((lines.next()?.ok()?, lines.next()?.ok()?))
}

/// `tx` and `rx` deltas between two [`wire_bytes`] readings, or `NA`.
pub fn wire_delta(before: Option<(u64, u64)>, after: Option<(u64, u64)>) -> (String, String) {
    match (before, after) {
        (Some(b), Some(a)) => ((a.0 - b.0).to_string(), (a.1 - b.1).to_string()),
        _ => ("NA".to_owned(), "NA".to_owned()),
    }
}

/// Docker guest free bytes on the server's data volume.
pub fn guest_free_bytes() -> Option<u64> {
    let out = Command::new("docker")
        .args([
            "exec",
            &container(),
            "df",
            "-B1",
            "--output=avail",
            "/var/lib/postgresql/data",
        ])
        .output()
        .ok()?;
    String::from_utf8(out.stdout)
        .ok()?
        .lines()
        .nth(1)?
        .trim()
        .parse()
        .ok()
}

/// The +1 ms arm exactly as the baselines applied it: a NET_ADMIN sidecar in
/// the server container's network namespace adds `netem delay 1ms` to the
/// server's `eth0` egress (server-to-client direction only), and deletes it
/// after the arm. `action` is `add`, `del` or `show`.
pub fn netem(action: &str) -> i32 {
    let tc = match action {
        "add" => "tc qdisc add dev eth0 root netem delay 1ms && tc qdisc show dev eth0",
        "del" => "tc qdisc del dev eth0 root && tc qdisc show dev eth0",
        "show" => "tc qdisc show dev eth0",
        _ => {
            eprintln!("netem: add | del | show");
            return 2;
        }
    };
    let status = Command::new("docker")
        .args([
            "run",
            "--rm",
            "--net",
            &format!("container:{}", container()),
            "--cap-add",
            "NET_ADMIN",
            "alpine:3",
            "sh",
            "-c",
            &format!("apk add --no-cache -q iproute2-tc >/dev/null 2>&1 && {tc}"),
        ])
        .status()
        .expect("docker");
    status.code().unwrap_or(1)
}
