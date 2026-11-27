//! The frozen PostgreSQL schema-3 fixture (#804), restored into a fresh
//! database per test from its plain `pg_dump` (`psql` meta-commands
//! skipped, `COPY … FROM stdin` blocks sent with `COPY`), and dropped after.
use std::io::Write;

/// One restored fixture database; dropped with the value.
pub struct FixtureDb {
    pub admin: String,
    pub url: String,
    pub name: String,
}

/// The Store schema the fixture holds.
pub const SCHEMA: &str = "layerfs";

impl FixtureDb {
    /// Restores `postgresql/v3/store.sql` into database `lfs804_<tag>_<pid>`.
    pub fn restore(admin: &str, tag: &str) -> Self {
        let name = format!("lfs804_{tag}_{}", std::process::id());
        let mut client = postgres::Client::connect(admin, postgres::NoTls).unwrap();
        client
            .batch_execute(&format!("DROP DATABASE IF EXISTS {name} WITH (FORCE)"))
            .unwrap();
        client
            .batch_execute(&format!("CREATE DATABASE {name}"))
            .unwrap();
        let url = format!("{admin} dbname={name}");
        let mut db = postgres::Client::connect(&url, postgres::NoTls).unwrap();
        let dump = std::fs::read_to_string(
            super::super::upgrade::fixtures().join("postgresql/v3/store.sql"),
        )
        .unwrap();
        let mut pending = String::new();
        let mut lines = dump.lines();
        while let Some(line) = lines.next() {
            if line.starts_with('\\') {
                continue;
            }
            if line.starts_with("COPY ") && line.ends_with("FROM stdin;") {
                db.batch_execute(&std::mem::take(&mut pending)).unwrap();
                let mut writer = db.copy_in(line.trim_end_matches(';')).unwrap();
                for row in lines.by_ref() {
                    if row == "\\." {
                        break;
                    }
                    writer.write_all(row.as_bytes()).unwrap();
                    writer.write_all(b"\n").unwrap();
                }
                writer.finish().unwrap();
                continue;
            }
            pending.push_str(line);
            pending.push('\n');
        }
        db.batch_execute(&pending).unwrap();
        db.close().unwrap();
        Self {
            admin: admin.to_owned(),
            url,
            name,
        }
    }

    /// A client on the fixture database with `search_path` on the Store.
    pub fn client(&self) -> postgres::Client {
        let mut client = postgres::Client::connect(&self.url, postgres::NoTls).unwrap();
        client
            .batch_execute(&format!("SET search_path TO {SCHEMA}"))
            .unwrap();
        client
    }

    /// A digest of every row of every Store table and of the schema's
    /// columns, constraints and indexes: equal digests mean nothing changed.
    pub fn digest(&self) -> String {
        let mut client = self.client();
        let tables: Vec<String> = client
            .query(
                "SELECT c.relname::text FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace \
                 WHERE n.nspname=$1 AND c.relkind='r' ORDER BY 1",
                &[&SCHEMA],
            )
            .unwrap()
            .iter()
            .map(|r| r.get(0))
            .collect();
        let mut parts = Vec::new();
        for table in tables {
            let row = client
                .query_one(
                    &format!(
                        "SELECT coalesce(md5(string_agg(t::text, ',' ORDER BY t::text)),'-') FROM {SCHEMA}.\"{table}\" t"
                    ),
                    &[],
                )
                .unwrap();
            parts.push(format!("{table}:{}", row.get::<_, String>(0)));
        }
        let schema: String = client
            .query_one(
                "SELECT md5(coalesce((SELECT string_agg(c.relname||'.'||a.attname||':'||format_type(a.atttypid,a.atttypmod)||':'||a.attnum, ',' ORDER BY 1) \
                   FROM pg_attribute a JOIN pg_class c ON c.oid=a.attrelid JOIN pg_namespace n ON n.oid=c.relnamespace \
                   WHERE n.nspname=$1 AND a.attnum>0 AND NOT a.attisdropped),'') \
                 ||coalesce((SELECT string_agg(conname||pg_get_constraintdef(oid), ',' ORDER BY 1) FROM pg_constraint WHERE connamespace=$1::text::regnamespace),'') \
                 ||coalesce((SELECT string_agg(indexdef, ',' ORDER BY 1) FROM pg_indexes WHERE schemaname=$1),''))",
                &[&SCHEMA],
            )
            .unwrap()
            .get(0);
        parts.push(format!("schema:{schema}"));
        parts.join("\n")
    }
}

impl Drop for FixtureDb {
    fn drop(&mut self) {
        if let Ok(mut client) = postgres::Client::connect(&self.admin, postgres::NoTls) {
            let _ = client.batch_execute(&format!(
                "DROP DATABASE IF EXISTS {} WITH (FORCE)",
                self.name
            ));
        }
    }
}
