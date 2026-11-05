//! The logged tables and the bundle's own tables: both are part of the bundle
//! format.

/// Bundle format written and read by this module.
pub const BUNDLE_FORMAT: i64 = 1;

/// One logged table: its stable code, name and primary key.
pub(super) struct Logged {
    pub(super) code: i64,
    pub(super) table: &'static str,
    pub(super) keys: &'static [&'static str],
    /// Rows are never updated or deleted (published packs).
    pub(super) immutable: bool,
}

/// Logged tables in code order. Codes are part of the bundle format and of
/// `change_log.sql`; code 0 is the sequence marker.
pub(super) const LOGGED: [Logged; 19] = [
    Logged {
        code: 1,
        table: "pack",
        keys: &["pack_id"],
        immutable: true,
    },
    Logged {
        code: 2,
        table: "pack_unit",
        keys: &["unit_id"],
        immutable: true,
    },
    Logged {
        code: 3,
        table: "object_location",
        keys: &["object_id"],
        immutable: false,
    },
    Logged {
        code: 4,
        table: "inode_value",
        keys: &["ordinal"],
        immutable: false,
    },
    Logged {
        code: 5,
        table: "content_signature",
        keys: &["slot"],
        immutable: false,
    },
    Logged {
        code: 6,
        table: "store_policy",
        keys: &["id"],
        immutable: false,
    },
    Logged {
        code: 7,
        table: "store_totals",
        keys: &["id"],
        immutable: false,
    },
    Logged {
        code: 8,
        table: "history_meta",
        keys: &["id"],
        immutable: false,
    },
    Logged {
        code: 9,
        table: "layer_stack",
        keys: &["layer_stack_id"],
        immutable: false,
    },
    Logged {
        code: 10,
        table: "layer",
        keys: &["layer_id"],
        immutable: false,
    },
    Logged {
        code: 11,
        table: "commit",
        keys: &["commit_id"],
        immutable: false,
    },
    Logged {
        code: 12,
        table: "branch",
        keys: &["branch_id"],
        immutable: false,
    },
    Logged {
        code: 13,
        table: "scope_allocator",
        keys: &["scope_id"],
        immutable: false,
    },
    Logged {
        code: 14,
        table: "binding_hint",
        keys: &["child", "parent", "name"],
        immutable: false,
    },
    Logged {
        code: 15,
        table: "layer_lineage",
        keys: &["layer_id"],
        immutable: false,
    },
    Logged {
        code: 16,
        table: "commit_lineage",
        keys: &["commit_id"],
        immutable: false,
    },
    Logged {
        code: 17,
        table: "absorbed",
        keys: &["run", "depth"],
        immutable: false,
    },
    Logged {
        code: 18,
        table: "reach",
        keys: &["key", "x"],
        immutable: false,
    },
    Logged {
        code: 19,
        table: "file_digest",
        keys: &["digest", "length"],
        immutable: false,
    },
];

/// `store_totals`' code. Inserting Commits and Layers adjusts the replica's
/// totals through its own triggers, so the bundled totals are installed last.
pub(super) const TOTALS: i64 = 7;

/// The bundle's own tables, next to copies of the logged tables.
pub(super) const BUNDLE_SCHEMA: &str = "
CREATE TABLE bundle_meta (
 id INTEGER PRIMARY KEY CHECK(id=1),
 format INTEGER NOT NULL,
 catalog_id BLOB NOT NULL,
 before_state BLOB NOT NULL CHECK(length(before_state)=32),
 before_seq INTEGER NOT NULL CHECK(before_seq>=0),
 after_state BLOB NOT NULL CHECK(length(after_state)=32),
 after_seq INTEGER NOT NULL CHECK(after_seq>before_seq),
 rows INTEGER NOT NULL CHECK(rows>=0),
 deletes INTEGER NOT NULL CHECK(deletes>=0),
 digest BLOB NOT NULL CHECK(length(digest)=32)
) STRICT;
CREATE TABLE bundle_delete (
 tbl INTEGER NOT NULL CHECK(tbl BETWEEN 1 AND 19),
 k1 ANY NOT NULL,
 k2 ANY,
 k3 ANY
) STRICT;
";

pub(super) fn quoted(name: &str) -> String {
    format!("\"{name}\"")
}

/// `k1,...,kn` for a table's key width.
pub(super) fn key_columns(width: usize) -> String {
    (1..=width)
        .map(|i| format!("k{i}"))
        .collect::<Vec<_>>()
        .join(",")
}
