//! Combined schema identity and deterministic definition validation.
use super::session::connection::SqlWork;
use super::session::query;
use super::session::transaction::Transaction;
use crate::backend::records::{BackendError, Param, Record};
use rusqlite::Connection;
use std::cell::RefCell;
pub(crate) const APPLICATION_ID: i64 = 1279677264;
pub(crate) const OBJECTS: &str = include_str!("../../../sql/sqlite/objects.sql");
pub(crate) const METADATA: &str = include_str!("../../../sql/sqlite/metadata.sql");
pub(crate) const HISTORY: &str = include_str!("../../../sql/sqlite/history.sql");
/// Lineage tables (#353); a Store written before them gains them at open.
pub(crate) const LINEAGE: &str = include_str!("../../../sql/sqlite/lineage.sql");
/// Merge build records (#355); a Store written before them gains them at open.
pub(crate) const SCRATCH: &str = include_str!("../../../sql/sqlite/merge_scratch.sql");
/// Whole-content file digests (#364); a Store written before them gains them at open.
pub(crate) const DIGESTS: &str = include_str!("../../../sql/sqlite/file_digest.sql");
/// GC markers, positions and admission records; a Store written before them
/// gains them at open.
pub(crate) const GC: &str = include_str!("../../../sql/sqlite/gc.sql");
/// The position columns T1.2a/T1.4 append, as `("table", "column", ALTER)`;
/// a Store written before them gains them at open.
pub(crate) const POSITION_COLUMNS: [(&str, &str, &str); 4] = [
    (
        "\"commit\"",
        "history_serial",
        "ALTER TABLE \"commit\" ADD COLUMN history_serial INTEGER NOT NULL DEFAULT 0",
    ),
    (
        "layer",
        "history_serial",
        "ALTER TABLE layer ADD COLUMN history_serial INTEGER NOT NULL DEFAULT 0",
    ),
    (
        "store_policy",
        "trace_salt",
        "ALTER TABLE store_policy ADD COLUMN trace_salt BLOB NOT NULL DEFAULT (X'0000000000000000000000000000000000000000000000000000000000000000') CHECK(length(trace_salt)=32)",
    ),
    (
        "history_meta",
        "next_history_serial",
        "ALTER TABLE history_meta ADD COLUMN next_history_serial INTEGER NOT NULL DEFAULT 1",
    ),
];
pub(crate) const UNIT_INDEX: &str = include_str!("../../../sql/sqlite/objects_units_index.sql");
pub(crate) const UNITS: &str = include_str!("../../../sql/sqlite/objects_units.sql");
pub(crate) const ACQUISITION: &str = include_str!("../../../sql/sqlite/acquisition/schema.sql");
type Selection = (crate::SqlitePackLayout, crate::SqliteAcquisitionSchema);
pub(super) fn scripts((layout, acquisition): Selection) -> Vec<String> {
    let objects = match layout {
        crate::SqlitePackLayout::Monolithic => OBJECTS.to_owned(),
        crate::SqlitePackLayout::GroupRows => UNITS.to_owned(),
        crate::SqlitePackLayout::GroupRowsIndexed => format!("{UNITS}{UNIT_INDEX}"),
    };
    let stored = layout.version() + acquisition.version_offset();
    let version = format!("schema_version={stored}");
    let mut scripts = vec![
        objects,
        METADATA.replace("schema_version=1", &version),
        HISTORY.replace("schema_version=1", &version),
        LINEAGE.to_owned(),
        SCRATCH.to_owned(),
        DIGESTS.to_owned(),
        GC.to_owned(),
    ];
    if acquisition == crate::SqliteAcquisitionSchema::Tables {
        scripts.push(ACQUISITION.to_owned());
    }
    scripts
}
pub(crate) fn source(selection: Selection) -> String {
    scripts(selection).concat()
}
pub(crate) fn definition(c: &Connection, w: &RefCell<SqlWork>) -> Result<String, BackendError> {
    let rows: Vec<Record> = query::run(
        c,
        "SELECT type,name,sql FROM sqlite_schema WHERE substr(name,1,7)!='sqlite_' ORDER BY type,name",
        vec![],
        w,
    )?;
    let mut s = String::new();
    for r in rows {
        for i in 0..3 {
            let v: String = r.get(i)?;
            s.push_str(&v);
            s.push('\n');
        }
    }
    Ok(s)
}
/// The `history_meta` next serial column as `history.sql` writes it; it sits
/// on its own line, so removing the line leaves the text an older Store
/// recorded.
fn next_history_serial_column() -> &'static str {
    let start = HISTORY
        .find("    next_history_serial INTEGER NOT NULL DEFAULT 1")
        .expect("history schema holds the next serial column");
    let end = start + HISTORY[start..].find('\n').expect("column line ends") + 1;
    &HISTORY[start..end]
}

/// The `kept_node` table as `gc.sql` writes it: one appended statement at the
/// end of the GC script, so removing exactly this text leaves the source a
/// Store written before T7.1 recorded.
fn kept_node_table() -> &'static str {
    let start = GC
        .find("CREATE TABLE kept_node (")
        .expect("the GC schema holds kept_node");
    let end = start + GC[start..].find(";\n").expect("kept_node ends") + 2;
    &GC[start..end]
}

/// The `binding_hint_block` table as `history.sql` writes it.
fn binding_hint_table() -> &'static str {
    let start = HISTORY
        .find("CREATE TABLE binding_hint_block (")
        .expect("the history schema holds binding_hint_block");
    let end = start
        + HISTORY[start..]
            .find(";\n")
            .expect("binding_hint_block ends")
        + 2;
    &HISTORY[start..end]
}

/// The query that returns a row when table `name` exists.
fn has_table(name: &str) -> String {
    format!("SELECT 1 FROM sqlite_schema WHERE type='table' AND name='{name}'")
}

/// The `commit` and `layer` position column as `history.sql` writes it; it
/// appears once in each of the two tables.
fn history_serial_column() -> &'static str {
    let start = HISTORY
        .find("    history_serial INTEGER NOT NULL DEFAULT 0")
        .expect("history schema holds the position column");
    let end = start + HISTORY[start..].find('\n').expect("column line ends") + 1;
    &HISTORY[start..end]
}

/// The `store_policy` position salt column as `metadata.sql` writes it,
/// together with the comma that ends `next_ordinal`: it was appended as the
/// table's last column, so a Store written before it recorded neither the
/// line nor the comma.
fn trace_salt_column() -> &'static str {
    let start = METADATA
        .find(",\n trace_salt BLOB NOT NULL DEFAULT")
        .expect("metadata schema holds the salt column");
    // The search starts past the comma and newline; the newline that ends the
    // column is kept, since it is also the one before the table's closing
    // `) STRICT;` that the pre-T1.2a text held.
    let line = start + 2;
    let end = line + METADATA[line..].find('\n').expect("column line ends");
    &METADATA[start..end]
}

/// `source` as a Store written before the position columns recorded it: the
/// two `history_serial` columns, `store_policy.trace_salt` and
/// `history_meta.next_history_serial` removed. The table migrations compare
/// against this so a Store from before those columns still matches its
/// recorded source.
fn without_position_columns(source: String) -> String {
    source
        .replacen(history_serial_column(), "", 2)
        .replacen(trace_salt_column(), "", 1)
        .replacen(next_history_serial_column(), "", 1)
}

/// The ALTERs and salt backfill an old Store needs; existing rows keep serial
/// 0 and the Store keeps one random salt.
fn add_position_columns(tx: &Transaction<'_>) -> Result<(), BackendError> {
    for (_, _, alter) in POSITION_COLUMNS.iter().take(3) {
        tx.query(alter, vec![])?;
    }
    tx.query(
        "UPDATE store_policy SET trace_salt=randomblob(32) WHERE id=1",
        vec![],
    )?;
    Ok(())
}

/// The serials and epochs a Store written before T1.4 needs: Commits take the
/// next Store serials in `(created_ms, commit_id)` order, then each project's
/// genesis Layer takes the next one in stack-id order; every other Layer
/// republishes its source Commit and carries that Commit's serial. Existing
/// serials are kept, `project_epoch` gets one zero row per project and
/// `next_history_serial` never falls below `MAX(serial) + 1`.
fn backfill_position_state(tx: &Transaction<'_>) -> Result<(), BackendError> {
    let next: i64 = tx
        .query(
            "SELECT next_history_serial FROM history_meta WHERE id=1",
            vec![],
        )?
        .first()
        .ok_or(BackendError::Integrity)?
        .get(0)?;
    let commit_max: i64 = tx
        .query(
            "SELECT COALESCE(MAX(history_serial),0) FROM \"commit\"",
            vec![],
        )?
        .first()
        .ok_or(BackendError::Integrity)?
        .get(0)?;
    let layer_max: i64 = tx
        .query("SELECT COALESCE(MAX(history_serial),0) FROM layer", vec![])?
        .first()
        .ok_or(BackendError::Integrity)?
        .get(0)?;
    let base = next.max(commit_max.max(layer_max) + 1);
    tx.query(
        &format!(
            "WITH ordered(commit_id,serial) AS ( \
               SELECT commit_id,{base}+ROW_NUMBER() OVER (ORDER BY created_ms,layer_stack_id,commit_id)-1 \
               FROM \"commit\" WHERE history_serial=0) \
             UPDATE \"commit\" SET history_serial=( \
               SELECT ordered.serial FROM ordered \
               WHERE ordered.commit_id=\"commit\".commit_id) \
             WHERE history_serial=0"
        ),
        vec![],
    )?;
    let after_commits: i64 = tx
        .query(
            "SELECT COALESCE(MAX(history_serial),0)+1 FROM \"commit\"",
            vec![],
        )?
        .first()
        .ok_or(BackendError::Integrity)?
        .get(0)?;
    tx.query(
        &format!(
            "WITH ordered(layer_id,serial) AS ( \
               SELECT layer_id,{after_commits}+ROW_NUMBER() OVER ( \
                 ORDER BY layer_stack_id,layer_id)-1 \
               FROM layer WHERE history_serial=0 AND parent_layer_id IS NULL) \
             UPDATE layer SET history_serial=( \
               SELECT ordered.serial FROM ordered \
               WHERE ordered.layer_id=layer.layer_id) \
             WHERE history_serial=0 AND parent_layer_id IS NULL"
        ),
        vec![],
    )?;
    tx.query(
        "UPDATE layer SET history_serial=( \
           SELECT \"commit\".history_serial FROM \"commit\" \
           WHERE \"commit\".layer_stack_id=layer.layer_stack_id \
             AND \"commit\".commit_id=layer.source_commit_id) \
         WHERE history_serial=0 AND source_commit_id IS NOT NULL",
        vec![],
    )?;
    // The rest: a Layer that neither opens a Stack nor republishes a Commit
    // takes the next serials by its source Commit's creation order, then its
    // Stack and identity, so the assignment is the same on every Store.
    tx.query(
        "WITH ordered(layer_id,serial) AS ( \
           SELECT l.layer_id, \
             (SELECT COALESCE(MAX(history_serial),0)+1 FROM layer) + ROW_NUMBER() OVER ( \
               ORDER BY COALESCE(c.created_ms,0),l.layer_stack_id,l.layer_id)-1 \
           FROM layer l LEFT JOIN \"commit\" c \
             ON c.layer_stack_id=l.layer_stack_id AND c.commit_id=l.source_commit_id \
           WHERE l.history_serial=0) \
         UPDATE layer SET history_serial=( \
           SELECT ordered.serial FROM ordered \
           WHERE ordered.layer_id=layer.layer_id) \
         WHERE history_serial=0",
        vec![],
    )?;
    tx.query(
        "INSERT INTO project_epoch(layer_stack_id,epoch) \
         SELECT s.layer_stack_id,0 FROM layer_stack s \
         WHERE NOT EXISTS ( \
           SELECT 1 FROM project_epoch e WHERE e.layer_stack_id=s.layer_stack_id)",
        vec![],
    )?;
    tx.query(
        "UPDATE history_meta SET next_history_serial=MAX(next_history_serial,( \
           SELECT COALESCE(MAX(history_serial),0)+1 FROM ( \
             SELECT history_serial FROM \"commit\" \
             UNION ALL SELECT history_serial FROM layer))) WHERE id=1",
        vec![],
    )?;
    Ok(())
}
pub(crate) fn check(
    c: &Connection,
    w: &RefCell<SqlWork>,
    (layout, acquisition): Selection,
) -> Result<(), BackendError> {
    let app = query::run(c, "PRAGMA application_id", vec![], w)?
        .first()
        .ok_or(BackendError::Integrity)?
        .get::<i64>(0)?;
    let version = query::run(c, "PRAGMA user_version", vec![], w)?
        .first()
        .ok_or(BackendError::Integrity)?
        .get::<i64>(0)?;
    if app != APPLICATION_ID || version != layout.version() + acquisition.version_offset() {
        return Err(BackendError::Integrity);
    }
    let rows = query::run(
        c,
        "SELECT name FROM sqlite_schema WHERE type='table' ORDER BY name",
        vec![],
        w,
    )?;
    let actual: Vec<String> = rows.iter().map(|r| r.get(0)).collect::<Result<_, _>>()?;
    let mut expected = vec![
        "absorbed",
        "binding_hint_block",
        "branch",
        "commit",
        "commit_lineage",
        "content_signature",
        "file_digest",
        "gc_dead",
        "gc_intent",
        "gc_op",
        "project_epoch",
        "branch_retired",
        "layer_stack_retired",
        "path_removed",
        "history_meta",
        "inode_value_block",
        "kept_node",
        "layer",
        "layer_lineage",
        "layer_stack",
        "merge_operation",
        "merge_record",
        "object_location",
        "pack",
        "reach",
        "scope_allocator",
        "store_policy",
        "store_totals",
    ];
    if layout.uses_units() {
        expected.push("pack_unit");
    }
    if acquisition == crate::SqliteAcquisitionSchema::Tables {
        // `sqlite_sequence` is the engine's never-reused operation identity counter.
        expected.extend([
            "init_entry",
            "init_host_file",
            "init_operation",
            "sqlite_sequence",
        ]);
    }
    expected.sort_unstable();
    if actual != expected {
        return Err(BackendError::Integrity);
    }
    Ok(())
}

impl super::session::connection::Session {
    pub(crate) fn initialize(
        &self,
        policy: layerfs_storage::StoragePolicy,
        history: &layerfs_history::HistoryCatalogConfig,
        catalog_id: layerfs_history::CatalogId,
    ) -> Result<(), BackendError> {
        self.run::<_,BackendError>(true,|tx| {
   let version=self.schema_version();
   let selection=(self.layout,self.acquisition);
   for script in scripts(selection) { tx.bootstrap(&script)?; }
   tx.query(&format!("PRAGMA application_id={APPLICATION_ID}"),vec![])?;
   tx.query(&format!("PRAGMA user_version={version}"),vec![])?;
   tx.query(&format!("INSERT INTO store_policy(id,schema_version,format_profile,small_file_threshold_bytes,whole_file_delta_max_depth,chunk_delta_max_depth,metadata_delta_max_depth,trace_salt) VALUES(1,{version},?1,?2,?3,?4,?5,randomblob(32))"),vec![Param::I64(i64::from(policy.format_profile())),Param::I64(policy.small_file_threshold_bytes() as i64),Param::I64(i64::from(policy.whole_file_delta_max_depth())),Param::I64(i64::from(policy.chunk_delta_max_depth())),Param::I64(i64::from(policy.metadata_delta_max_depth()))])?;
   let definition=definition(tx.connection,tx.work)?;
   tx.query(&format!("INSERT INTO history_meta(id,catalog_id,catalog_incarnation,binding_key,identity_format,schema_version,schema_source,schema_definition) VALUES(1,?1,?2,?3,3,{version},?4,?5)"),vec![Param::Bytes(catalog_id.as_slice().to_vec()),Param::I64(history.incarnation as i64),Param::Bytes(history.binding_key.clone()),Param::Text(source(selection)),Param::Text(definition)])?;
   Ok(())
  })?;
        Ok(())
    }
    /// One additive migration (REBUILD.md kind 2) in one write transaction.
    /// A Store that already has it (`present` returns a row), has no
    /// `history_meta` row, recorded a source other than the accepted ones, or
    /// whose live schema no longer matches what it recorded is left for
    /// `validate` to judge. Otherwise `apply` runs and the Store records the
    /// new source with its new definition. `sources` maps the current source
    /// to `(accepted earlier sources, source to record)`.
    fn migrate(
        &self,
        present: &str,
        sources: impl FnOnce(String) -> (Vec<String>, String),
        apply: impl FnOnce(&Transaction<'_>) -> Result<(), BackendError>,
    ) -> Result<(), BackendError> {
        self.run::<_, BackendError>(true, |tx| {
            if !tx.query(present, vec![])?.is_empty() {
                return Ok(());
            }
            let rows = tx.query(
                "SELECT schema_source,schema_definition FROM history_meta WHERE id=1",
                vec![],
            )?;
            let Some(row) = rows.first() else {
                return Ok(());
            };
            let (accepted, record) = sources(source((self.layout, self.acquisition)));
            if !accepted.contains(&row.get::<String>(0)?)
                || row.get::<String>(1)? != definition(tx.connection, tx.work)?
            {
                return Ok(());
            }
            apply(tx)?;
            let definition = definition(tx.connection, tx.work)?;
            tx.query(
                "UPDATE history_meta SET schema_source=?1,schema_definition=?2 WHERE id=1",
                vec![Param::Text(record), Param::Text(definition)],
            )?;
            Ok(())
        })
    }
    /// Adds the three position columns T1.2a introduced to a Store written
    /// before them: `commit.history_serial`, `layer.history_serial` (default 0)
    /// and `store_policy.trace_salt` (32 random bytes). Because it runs before
    /// the GC tables are added, the sources it matches and records are the
    /// current one with `GC` stripped.
    pub(crate) fn add_gc_columns(&self) -> Result<(), BackendError> {
        self.migrate(
            "SELECT 1 FROM pragma_table_info('commit') WHERE name='history_serial'",
            |current| {
                let record = current.replacen(GC, "", 1);
                (vec![without_position_columns(record.clone())], record)
            },
            add_position_columns,
        )
    }
    /// Adds the binding hint table (#352 `49499abac`, blocks since #91) to a
    /// Store written before it existed. Saved trees keep no hints, so a
    /// difference over them takes the full walk; hints are recorded for what
    /// is saved from then on. The source it records is the one before the
    /// lineage, merge, digest and GC tables; later migrations add those.
    pub(crate) fn add_binding_hints(&self) -> Result<(), BackendError> {
        self.migrate(
            &has_table("binding_hint_block"),
            |current| {
                let record = without_position_columns(current)
                    .replacen(DIGESTS, "", 1)
                    .replacen(SCRATCH, "", 1)
                    .replacen(LINEAGE, "", 1)
                    .replacen(GC, "", 1);
                let table = format!("{}\n", binding_hint_table());
                (vec![record.replacen(&table, "", 1)], record)
            },
            |tx| tx.bootstrap(binding_hint_table()),
        )
    }
    /// Adds the lineage tables (#353) to a Store written before them and
    /// fills them with `backfill` in the same write transaction.
    pub(crate) fn add_lineage(
        &self,
        backfill: impl FnOnce(&Transaction<'_>) -> Result<(), BackendError>,
    ) -> Result<(), BackendError> {
        self.migrate(
            &has_table("commit_lineage"),
            |current| {
                let record = without_position_columns(current)
                    .replacen(DIGESTS, "", 1)
                    .replacen(SCRATCH, "", 1)
                    .replacen(GC, "", 1);
                (vec![record.replacen(LINEAGE, "", 1)], record)
            },
            |tx| {
                tx.bootstrap(LINEAGE)?;
                backfill(tx)
            },
        )
    }
    /// Adds the merge build tables (#355) to a Store written before them.
    pub(crate) fn add_merge_scratch(&self) -> Result<(), BackendError> {
        self.migrate(
            &has_table("merge_record"),
            |current| {
                let record = without_position_columns(current)
                    .replacen(DIGESTS, "", 1)
                    .replacen(GC, "", 1);
                (vec![record.replacen(SCRATCH, "", 1)], record)
            },
            |tx| tx.bootstrap(SCRATCH),
        )
    }
    /// Adds the file digest table (#364) to a Store written before it. Saved
    /// roots keep no digest; a Commit records them for the roots it builds.
    pub(crate) fn add_file_digests(&self) -> Result<(), BackendError> {
        self.migrate(
            &has_table("file_digest"),
            |current| {
                let record = without_position_columns(current).replacen(GC, "", 1);
                (vec![record.replacen(DIGESTS, "", 1)], record)
            },
            |tx| tx.bootstrap(DIGESTS),
        )
    }
    /// Adds the GC marker, position and admission tables to a Store written
    /// before them.
    pub(crate) fn add_gc(&self) -> Result<(), BackendError> {
        self.migrate(
            &has_table("gc_intent"),
            |current| (vec![current.replacen(GC, "", 1)], current),
            |tx| tx.bootstrap(GC),
        )
    }
    /// Adds the `kept_node` history-closure table (T7.1) to a Store written
    /// before it, with or without the T1.4 next-serial column. The table is
    /// empty until a mark writes it.
    pub(crate) fn add_kept_node(&self) -> Result<(), BackendError> {
        self.migrate(
            &has_table("kept_node"),
            |current| {
                let without = current.replacen(kept_node_table(), "", 1);
                let earlier = without.replacen(next_history_serial_column(), "", 1);
                (vec![earlier, without], current)
            },
            |tx| tx.bootstrap(kept_node_table()),
        )
    }
    /// Adds the position state T1.4 introduced (`history_meta.next_history_serial`,
    /// appended NOT NULL DEFAULT 1) and backfills the `commit`/`layer` serials
    /// and `project_epoch` rows in the same write transaction. A recorded
    /// source equal to the current one is also accepted: it is a Store an
    /// earlier step of this same open just brought forward, because those
    /// steps record the current source before this column is appended.
    pub(crate) fn add_position_state(&self) -> Result<(), BackendError> {
        self.migrate(
            "SELECT 1 FROM pragma_table_info('history_meta') WHERE name='next_history_serial'",
            |current| {
                let before = current.replacen(next_history_serial_column(), "", 1);
                (vec![before, current.clone()], current)
            },
            |tx| {
                tx.query(POSITION_COLUMNS[3].2, vec![])?;
                backfill_position_state(tx)
            },
        )
    }
    /// Whether the file records the Store identity `catalog_id` and
    /// `binding`, read alone, without the schema checks: a doctor tells a
    /// file of another Store from one with another schema by it.
    pub(crate) fn identity_matches(
        &self,
        catalog_id: layerfs_history::CatalogId,
        binding: &[u8],
    ) -> Result<bool, BackendError> {
        self.run::<_, BackendError>(false, |tx| {
            let rows = tx.query(
                "SELECT catalog_id,binding_key FROM history_meta WHERE id=1",
                vec![],
            )?;
            let row = (rows.first())
                .filter(|_| rows.len() == 1)
                .ok_or(BackendError::Integrity)?;
            Ok(
                row.get::<Vec<u8>>(0)? == catalog_id.as_slice()
                    && row.get::<Vec<u8>>(1)? == binding,
            )
        })
    }
    pub(crate) fn validate(
        &self,
        catalog_id: layerfs_history::CatalogId,
        binding: &[u8],
    ) -> Result<(u64, u64, [u8; 32]), BackendError> {
        self.run::<_,BackendError>(false,|tx| {
   let selection=(self.layout,self.acquisition);
   if self.diagnostic { super::schema_diagnostic::check(tx,self.acquisition)?; }
   check(tx.connection,tx.work,selection)?;
   let rows=tx.query("SELECT catalog_id,catalog_incarnation,binding_key,identity_format,schema_version,schema_source,schema_definition FROM history_meta WHERE id=1",vec![])?;
   let r=rows.first().filter(|_|rows.len()==1).ok_or(BackendError::Integrity)?;
   let incarnation:i64=r.get(1)?;
   if r.get::<Vec<u8>>(0)?!=catalog_id.as_slice() || r.get::<Vec<u8>>(2)?!=binding || incarnation<=0 || r.get::<i64>(3)?!=3 || r.get::<i64>(4)?!=self.schema_version() || r.get::<String>(5)?!=source(selection) || r.get::<String>(6)?!=definition(tx.connection,tx.work)? {return Err(BackendError::Integrity);}
   let policy=tx.query("SELECT small_file_threshold_bytes,trace_salt FROM store_policy WHERE id=1",vec![])?;
   let row=policy.first().ok_or(BackendError::Integrity)?;
   let floor=u64::try_from(row.get::<i64>(0)?).map_err(|_|BackendError::Integrity)?;
   let salt:Vec<u8>=row.get(1)?;
   let salt:[u8;32]=salt.as_slice().try_into().map_err(|_|BackendError::Integrity)?;
   Ok((incarnation as u64, floor, salt))
  })
    }
}
