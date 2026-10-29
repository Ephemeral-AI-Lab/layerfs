//! Copies a SQLite Store into a new PostgreSQL schema (#506 A4).
//!
//! Init stays SQLite only, so a PostgreSQL Store gets its rows from an Init
//! or a pulled Store. The copy reads one SQLite snapshot and writes one
//! PostgreSQL transaction: the schema exists with every row, or not at all.
//! Rows move in bounded pages (`ROWS` rows, or `PACK_BYTES` of pack bodies)
//! by `COPY BINARY`. Pack bodies are read through the SQLite engine's own
//! pack read, so every SQLite pack layout imports. Ids, roots, Commits and
//! inode serials are copied unchanged; the engine-scoped schema version and
//! recorded schema become PostgreSQL's.
use super::{
    schema,
    session::{connection::Session, transaction::Transaction},
};
use crate::backend::{
    records::BackendError, sqlite::session::rows as sqlite_rows,
    sqlite::session::transaction::Transaction as SqliteTransaction, Mechanics,
};
use layerfs_storage::port::{PersistedPack, PersistenceError};
use postgres::types::{ToSql, Type};

const ROWS: usize = 4096;
const PACK_BYTES: usize = 64 << 20;

/// Rows copied into a new PostgreSQL Store, per table.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ImportReceipt {
    /// `(table, rows)` in copy order.
    pub tables: Vec<(&'static str, u64)>,
    /// Pack body bytes copied.
    pub pack_bytes: u64,
}

/// Column kinds of one copied table.
#[derive(Clone, Copy)]
enum Kind {
    Integer,
    Bytes,
    Text,
}
use Kind::{Bytes as B, Integer as I, Text as T};

/// One copied table: SQLite and PostgreSQL share its name and columns.
struct Table {
    name: &'static str,
    columns: &'static [(&'static str, Kind)],
    order: &'static str,
}

/// Every logical table after `pack`, in an order the immediate foreign keys
/// accept (history references are deferred to COMMIT).
const TABLES: &[Table] = &[
    Table {
        name: "object_location",
        columns: &[
            ("object_id", B),
            ("role", I),
            ("canonical_length", I),
            ("pack_id", I),
            ("group_number", I),
            ("record_number", I),
        ],
        order: "object_id",
    },
    Table {
        name: "inode_value",
        columns: &[("ordinal", I), ("value", B)],
        order: "ordinal",
    },
    Table {
        name: "content_signature",
        columns: &[
            ("slot", I),
            ("stamp", I),
            ("object_id", B),
            ("signature", B),
        ],
        order: "slot",
    },
    Table {
        name: "store_policy",
        columns: &[
            ("id", I),
            ("schema_version", I),
            ("format_profile", I),
            ("small_file_threshold_bytes", I),
            ("whole_file_delta_max_depth", I),
            ("chunk_delta_max_depth", I),
            ("metadata_delta_max_depth", I),
            ("next_pack_id", I),
            ("next_ordinal", I),
        ],
        order: "id",
    },
    Table {
        name: "history_meta",
        columns: &[
            ("id", I),
            ("catalog_id", B),
            ("catalog_incarnation", I),
            ("binding_key", B),
            ("identity_format", I),
            ("schema_version", I),
            ("schema_source", T),
            ("schema_definition", T),
        ],
        order: "id",
    },
    Table {
        name: "layer_stack",
        columns: &[
            ("layer_stack_id", B),
            ("name", T),
            ("scope_id", B),
            ("profile_id", B),
            ("head_layer_id", B),
        ],
        order: "layer_stack_id",
    },
    Table {
        name: "layer",
        columns: &[
            ("layer_id", B),
            ("layer_stack_id", B),
            ("parent_layer_id", B),
            ("root_id", B),
            ("source_branch_id", B),
            ("logical", I),
            ("overwrote", B),
            ("source_commit_id", B),
        ],
        order: "layer_id",
    },
    Table {
        name: "commit",
        columns: &[
            ("commit_id", B),
            ("layer_stack_id", B),
            ("root_id", B),
            ("parent_commit_id", B),
            ("base_layer_id", B),
            ("second_parent_id", B),
            ("branch_id", B),
            ("via", I),
            ("message", T),
            ("merged_from_id", B),
            ("created_ms", I),
            ("logical", I),
            ("new_canonical", I),
            ("new_stored", I),
            ("new_metadata", I),
            ("lww", B),
            ("displaced_id", B),
        ],
        order: "commit_id",
    },
    Table {
        name: "branch",
        columns: &[
            ("branch_id", B),
            ("layer_stack_id", B),
            ("name", T),
            ("base_layer_id", B),
            ("forked_from_id", B),
            ("head_commit_id", B),
        ],
        order: "branch_id",
    },
    Table {
        name: "scope_allocator",
        columns: &[("scope_id", B), ("highwater", I), ("authority_id", B)],
        order: "scope_id",
    },
    Table {
        name: "binding_hint",
        columns: &[("child", I), ("parent", I), ("name", B)],
        order: "child,parent,name",
    },
    Table {
        name: "layer_lineage",
        columns: &[
            ("layer_id", B),
            ("layer_stack_id", B),
            ("height", I),
            ("overwrote_count", I),
        ],
        order: "layer_id",
    },
    Table {
        name: "commit_lineage",
        columns: &[
            ("commit_id", B),
            ("run", B),
            ("depth", I),
            ("newest", I),
            ("merged", B),
        ],
        order: "commit_id",
    },
    Table {
        name: "absorbed",
        columns: &[("run", B), ("depth", I), ("height", I)],
        order: "run,depth",
    },
    Table {
        name: "reach",
        columns: &[("key", B), ("x", I), ("height", I)],
        order: "key,x",
    },
    Table {
        name: "file_digest",
        columns: &[("digest", B), ("length", I), ("root", B)],
        order: "digest,length",
    },
];

/// Totals are written last: inserting Layers and Commits runs the logical-size trigger.
const TOTALS: &[&str] = &[
    "logical",
    "canonical",
    "stored",
    "metadata",
    "init_entries",
    "init_files",
    "init_bytes",
    "init_canonical",
    "init_stored",
    "init_metadata",
    "init_millis",
];

enum Cell {
    Integer(Option<i64>),
    Bytes(Option<Vec<u8>>),
    Text(Option<String>),
}
impl Cell {
    fn value(&self) -> &(dyn ToSql + Sync) {
        match self {
            Cell::Integer(v) => v,
            Cell::Bytes(v) => v,
            Cell::Text(v) => v,
        }
    }
}

/// Copies the Store read by `source` into the new schema of `target`.
pub(crate) fn import(
    source: &SqliteTransaction<'_>,
    target: &Session,
) -> Result<ImportReceipt, PersistenceError> {
    target.run_as(
        super::session::transaction::Mode::Stage,
        |tx| -> Result<ImportReceipt, PersistenceError> {
            let name = &target.settings.schema;
            tx.batch(&format!(
                "CREATE SCHEMA {name}; SET LOCAL search_path TO {name}; {}",
                schema::SCHEMA
            ))?;
            let mut receipt = ImportReceipt::default();
            receipt.pack_bytes = packs(source, tx, &mut receipt)?;
            for table in TABLES {
                let rows = copy(source, tx, table)?;
                receipt.tables.push((table.name, rows));
            }
            totals(source, tx)?;
            schema::record(tx)?;
            Ok(receipt)
        },
    )
}

fn sqlite<T>(result: rusqlite::Result<T>) -> Result<T, BackendError> {
    result.map_err(sqlite_rows::error)
}

fn packs(
    source: &SqliteTransaction<'_>,
    tx: &Transaction<'_>,
    receipt: &mut ImportReceipt,
) -> Result<u64, PersistenceError> {
    let mut ids = sqlite(
        source
            .connection
            .prepare("SELECT pack_id FROM pack ORDER BY pack_id"),
    )?;
    let mut ids = sqlite(ids.query([]))?;
    let (mut count, mut bytes) = (0u64, 0u64);
    let mut page: Vec<PersistedPack> = Vec::new();
    let mut held = 0usize;
    let flush = |page: &mut Vec<PersistedPack>| -> Result<(), PersistenceError> {
        if page.is_empty() {
            return Ok(());
        }
        tx.copy_in(
            "COPY pack (pack_id,domain,digest,length,body) FROM STDIN (FORMAT binary)",
            &[Type::INT8, Type::INT8, Type::BYTEA, Type::INT8, Type::BYTEA],
            |writer| {
                for pack in page.iter() {
                    let info = pack.info();
                    let domain =
                        i64::from(info.domain == layerfs_storage::location::PackDomain::Payload);
                    writer.write(&[
                        &info.pack_id,
                        &domain,
                        &info.key.as_bytes().as_slice(),
                        &(info.length as i64),
                        &pack.body(),
                    ])?;
                }
                Ok(())
            },
        )?;
        page.clear();
        Ok(())
    };
    while let Some(row) = sqlite(ids.next())? {
        let id: i64 = sqlite(row.get(0))?;
        let mut read = Vec::with_capacity(1);
        source.read_packs(&[id], &mut read)?;
        for pack in read {
            held += pack.body().len();
            bytes += pack.body().len() as u64;
            count += 1;
            page.push(pack);
        }
        if held >= PACK_BYTES || page.len() >= ROWS {
            flush(&mut page)?;
            held = 0;
        }
    }
    flush(&mut page)?;
    receipt.tables.push(("pack", count));
    Ok(bytes)
}

fn copy(
    source: &SqliteTransaction<'_>,
    tx: &Transaction<'_>,
    table: &Table,
) -> Result<u64, PersistenceError> {
    let names: Vec<&str> = table.columns.iter().map(|(name, _)| *name).collect();
    let quoted = format!("\"{}\"", table.name);
    let select = format!(
        "SELECT {} FROM {quoted} ORDER BY {}",
        names.join(","),
        table.order
    );
    let copy = format!(
        "COPY {quoted} ({}) FROM STDIN (FORMAT binary)",
        names.join(",")
    );
    let types: Vec<Type> = table
        .columns
        .iter()
        .map(|(_, kind)| match kind {
            I => Type::INT8,
            B => Type::BYTEA,
            T => Type::TEXT,
        })
        .collect();
    let mut statement = sqlite(source.connection.prepare(&select))?;
    let mut rows = sqlite(statement.query([]))?;
    let mut page: Vec<Vec<Cell>> = Vec::with_capacity(ROWS);
    let mut count = 0u64;
    loop {
        let row = sqlite(rows.next())?;
        if let Some(row) = row {
            let mut cells = Vec::with_capacity(table.columns.len());
            for (index, (name, kind)) in table.columns.iter().enumerate() {
                cells.push(match (table.name, *name, kind) {
                    // The engine-scoped version and recorded schema are PostgreSQL's.
                    ("store_policy" | "history_meta", "schema_version", _) => {
                        Cell::Integer(Some(schema::SCHEMA_VERSION))
                    }
                    ("history_meta", "schema_source" | "schema_definition", _) => {
                        Cell::Text(Some(String::new()))
                    }
                    (_, _, I) => Cell::Integer(sqlite(row.get(index))?),
                    (_, _, B) => Cell::Bytes(sqlite(row.get(index))?),
                    (_, _, T) => Cell::Text(sqlite(row.get(index))?),
                });
            }
            page.push(cells);
        }
        if page.len() >= ROWS || (row.is_none() && !page.is_empty()) {
            count += page.len() as u64;
            tx.copy_in(&copy, &types, |writer| {
                for cells in &page {
                    let values: Vec<&(dyn ToSql + Sync)> = cells.iter().map(Cell::value).collect();
                    writer.write(&values)?;
                }
                Ok(())
            })?;
            page.clear();
        }
        if row.is_none() {
            return Ok(count);
        }
    }
}

fn totals(source: &SqliteTransaction<'_>, tx: &Transaction<'_>) -> Result<(), PersistenceError> {
    let rows = source.query(
        &format!("SELECT {} FROM store_totals WHERE id=1", TOTALS.join(",")),
        vec![],
    )?;
    let row = rows
        .first()
        .filter(|_| rows.len() == 1)
        .ok_or(PersistenceError::Malformed)?;
    let assignments: Vec<String> = TOTALS
        .iter()
        .enumerate()
        .map(|(index, name)| format!("{name}=${}", index + 1))
        .collect();
    let params = (0..TOTALS.len())
        .map(|index| {
            row.get::<i64>(index)
                .map(crate::backend::records::Param::I64)
        })
        .collect::<Result<Vec<_>, _>>()?;
    tx.query(
        &format!(
            "UPDATE store_totals SET {} WHERE id=1",
            assignments.join(",")
        ),
        params,
    )?;
    Ok(())
}
