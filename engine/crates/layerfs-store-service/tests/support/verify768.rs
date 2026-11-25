//! Independent #768 verification helpers (not the author's harness): fault
//! injection by server-side triggers, raw row counts, and amcheck, all on a
//! [`TestStore`](super::adversary::TestStore)'s private PostgreSQL schema.
//!
//! A trigger installed here fires once (a sequence, which a rollback does
//! not undo, gates it) and only inside a fused Commit's transaction: that
//! transaction alone sets the transaction-local `layerfs.attempt` token, so
//! every other write (the separate path, a fallback, a later Commit) passes.
use super::adversary::TestStore;

/// A table a fused Commit writes, the event a trigger watches there, and the
/// fused step that writes it (`backend/postgresql/history/fused.rs`).
#[derive(Clone, Copy, Debug)]
pub struct Target {
    pub table: &'static str,
    pub event: &'static str,
    pub step: Step,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Step {
    Packs,
    Rows,
    Hints,
    Commit,
    Lineage,
    Advance,
}

/// Every table the fused message writes, in message order.
pub const TARGETS: &[Target] = &[
    Target {
        table: "pack",
        event: "INSERT",
        step: Step::Packs,
    },
    Target {
        table: "object_location",
        event: "INSERT",
        step: Step::Rows,
    },
    Target {
        table: "inode_value_block",
        event: "INSERT",
        step: Step::Rows,
    },
    Target {
        table: "content_signature",
        event: "INSERT",
        step: Step::Rows,
    },
    Target {
        table: "store_totals",
        event: "UPDATE",
        step: Step::Rows,
    },
    Target {
        table: "store_policy",
        event: "UPDATE",
        step: Step::Rows,
    },
    Target {
        table: "binding_hint_block",
        event: "INSERT",
        step: Step::Hints,
    },
    Target {
        table: "history_meta",
        event: "UPDATE",
        step: Step::Commit,
    },
    Target {
        table: "\"commit\"",
        event: "INSERT",
        step: Step::Commit,
    },
    Target {
        table: "commit_lineage",
        event: "INSERT",
        step: Step::Lineage,
    },
    Target {
        table: "branch",
        event: "UPDATE",
        step: Step::Advance,
    },
];

fn schema(store: &TestStore) -> String {
    store
        .postgres
        .as_ref()
        .expect("a PostgreSQL Store")
        .1
        .clone()
}

/// Installs the one-shot trigger on `target`, raising SQLSTATE `code` the
/// first time a fused transaction touches a row there.
pub fn arm(store: &TestStore, target: Target, code: &str) {
    let s = schema(store);
    let mut admin = store.admin();
    admin
        .batch_execute(&format!(
            "CREATE SEQUENCE IF NOT EXISTS {s}.v768_fire; \
             ALTER SEQUENCE {s}.v768_fire RESTART; \
             CREATE OR REPLACE FUNCTION {s}.v768_fail() RETURNS trigger LANGUAGE plpgsql AS $f$ \
             BEGIN \
               IF coalesce(current_setting('layerfs.attempt', true), '') <> '' \
                  AND nextval('{s}.v768_fire') = 1 THEN \
                 RAISE EXCEPTION 'v768 injected abort on %', TG_TABLE_NAME USING ERRCODE = TG_ARGV[0]; \
               END IF; \
               IF TG_OP = 'DELETE' THEN RETURN OLD; END IF; \
               RETURN NEW; \
             END $f$; \
             DROP TRIGGER IF EXISTS v768_t ON {s}.{table}; \
             CREATE TRIGGER v768_t BEFORE {event} ON {s}.{table} FOR EACH ROW \
               EXECUTE FUNCTION {s}.v768_fail('{code}');",
            table = target.table,
            event = target.event,
        ))
        .unwrap();
}

/// Whether the armed trigger fired since it was armed.
pub fn fired(store: &TestStore) -> bool {
    let s = schema(store);
    store
        .admin()
        .query_one(&format!("SELECT is_called FROM {s}.v768_fire"), &[])
        .unwrap()
        .get(0)
}

/// Removes the trigger, its function and its sequence.
pub fn disarm(store: &TestStore, target: Target) {
    let s = schema(store);
    store
        .admin()
        .batch_execute(&format!(
            "DROP TRIGGER IF EXISTS v768_t ON {s}.{table}; \
             DROP FUNCTION IF EXISTS {s}.v768_fail(); \
             DROP SEQUENCE IF EXISTS {s}.v768_fire;",
            table = target.table
        ))
        .unwrap();
}

/// Everything a Commit could have written, read with admin SQL.
pub fn written(store: &TestStore) -> String {
    store
        .admin()
        .query_one(
            "SELECT (SELECT count(*) FROM \"commit\")::text || '/' || \
             (SELECT count(*) FROM commit_lineage)::text || '/' || \
             (SELECT count(*) FROM object_location)::text || '/' || \
             (SELECT count(*) FROM pack)::text || '/' || \
             (SELECT count(*) FROM inode_value_block)::text || '/' || \
             (SELECT count(*) FROM content_signature)::text || '/' || \
             (SELECT count(*) FROM binding_hint_block)::text || '/' || \
             (SELECT next_history_serial FROM history_meta WHERE id = 1)::text || '/' || \
             (SELECT canonical::text || ':' || stored::text || ':' || metadata::text \
              FROM store_totals WHERE id = 1) || '/' || \
             (SELECT string_agg(encode(branch_id, 'hex') || '=' || \
                coalesce(encode(head_commit_id, 'hex'), '-'), ',' ORDER BY branch_id) FROM branch)",
            &[],
        )
        .unwrap()
        .get(0)
}

/// amcheck over every B-tree index of the Store's schema (`bt_index_check`
/// with heap verification) and `verify_heapam` over every table: each
/// failure as text. Needs the `amcheck` extension in the test database.
pub fn amcheck(store: &TestStore) -> Vec<String> {
    let s = schema(store);
    let mut admin = store.admin();
    admin
        .batch_execute("CREATE EXTENSION IF NOT EXISTS amcheck WITH SCHEMA public")
        .unwrap();
    let mut failures = Vec::new();
    let indexes = admin
        .query(
            "SELECT c.oid::bigint, c.relname::text FROM pg_index i \
             JOIN pg_class c ON c.oid = i.indexrelid \
             JOIN pg_namespace n ON n.oid = c.relnamespace \
             JOIN pg_am a ON a.oid = c.relam \
             WHERE n.nspname = $1 AND a.amname = 'btree'",
            &[&s],
        )
        .unwrap();
    for row in &indexes {
        let (oid, name): (i64, String) = (row.get(0), row.get(1));
        if let Err(error) =
            admin.batch_execute(&format!("SELECT public.bt_index_check({oid}::oid, true)"))
        {
            failures.push(format!("index {name}: {error}"));
        }
    }
    let tables = admin
        .query(
            "SELECT c.oid::bigint, c.relname::text FROM pg_class c \
             JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE n.nspname = $1 AND c.relkind = 'r'",
            &[&s],
        )
        .unwrap();
    for row in &tables {
        let (oid, name): (i64, String) = (row.get(0), row.get(1));
        let corrupt: i64 = admin
            .query_one(
                &format!("SELECT count(*) FROM public.verify_heapam({oid}::oid)"),
                &[],
            )
            .unwrap()
            .get(0);
        if corrupt != 0 {
            failures.push(format!("heap {name}: {corrupt} corruption rows"));
        }
    }
    failures
}

/// Object ids referenced by the Store's locators more than once, and pack
/// ids or inode-value ordinal ranges used twice (duplicated allocation).
pub fn duplicate_allocations(store: &TestStore) -> Vec<String> {
    let mut admin = store.admin();
    let mut out = Vec::new();
    let checks = [
        (
            "duplicate object locators",
            "SELECT count(*) FROM (SELECT object_id FROM object_location \
             GROUP BY object_id HAVING count(*) > 1) d",
        ),
        (
            "overlapping inode value blocks",
            "SELECT count(*) FROM inode_value_block a JOIN inode_value_block b \
             ON a.first_ordinal < b.first_ordinal AND b.first_ordinal <= a.last_ordinal",
        ),
        (
            "locators into a missing pack",
            "SELECT count(*) FROM object_location o WHERE NOT EXISTS \
             (SELECT 1 FROM pack p WHERE p.pack_id = o.pack_id)",
        ),
        (
            "inode values past the allocator",
            "SELECT count(*) FROM inode_value_block, store_policy p \
             WHERE p.id = 1 AND last_ordinal >= p.next_ordinal",
        ),
        (
            "packs past the allocator",
            "SELECT count(*) FROM pack, store_policy p WHERE p.id = 1 AND pack_id >= p.next_pack_id",
        ),
    ];
    for (what, sql) in checks {
        let n: i64 = admin.query_one(sql, &[]).unwrap().get(0);
        if n != 0 {
            out.push(format!("{what}: {n}"));
        }
    }
    out
}
