//! Combined schema identity and deterministic definition validation.
use super::{connection::SqlWork, query, transaction::Transaction};
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
const UNIT_INDEX: &str = include_str!("../../../sql/sqlite/objects_units_index.sql");
const UNITS: &str = include_str!("../../../sql/sqlite/objects_units.sql");
const ACQUISITION: &str = include_str!("../../../sql/sqlite/acquisition/schema.sql");
type Selection = (crate::SqlitePackLayout, crate::SqliteAcquisitionSchema);
fn scripts((layout, acquisition): Selection) -> Vec<String> {
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
        "binding_hint",
        "branch",
        "commit",
        "commit_lineage",
        "content_signature",
        "file_digest",
        "history_meta",
        "inode_value",
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

impl super::connection::Session {
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
   tx.query(&format!("INSERT INTO store_policy(id,schema_version,format_profile,small_file_threshold_bytes,whole_file_delta_max_depth,chunk_delta_max_depth,metadata_delta_max_depth) VALUES(1,{version},?1,?2,?3,?4,?5)"),vec![Param::I64(i64::from(policy.format_profile())),Param::I64(policy.small_file_threshold_bytes() as i64),Param::I64(i64::from(policy.whole_file_delta_max_depth())),Param::I64(i64::from(policy.chunk_delta_max_depth())),Param::I64(i64::from(policy.metadata_delta_max_depth()))])?;
   let definition=definition(tx.connection,tx.work)?;
   tx.query(&format!("INSERT INTO history_meta(id,catalog_id,catalog_incarnation,binding_key,identity_format,schema_version,schema_source,schema_definition) VALUES(1,?1,?2,?3,3,{version},?4,?5)"),vec![Param::Bytes(catalog_id.as_slice().to_vec()),Param::I64(history.incarnation as i64),Param::Bytes(history.binding_key.clone()),Param::Text(source(selection)),Param::Text(definition)])?;
   Ok(())
  })?;
        Ok(())
    }
    /// Adds the `binding_hint` table (#352 `49499abac`) to a Store written
    /// before it existed, then records the current schema: an additive
    /// migration (REBUILD.md kind 2). Only a Store whose recorded schema is
    /// exactly the earlier one, and whose live schema still matches what it
    /// recorded, is migrated; any other Store is left for `validate` to judge.
    /// Saved trees keep no hints, so a difference over them takes the full
    /// walk; hints are recorded for what is saved from then on.
    pub(crate) fn add_binding_hints(&self) -> Result<(), BackendError> {
        self.run::<_, BackendError>(true, |tx| {
            let present = tx.query(
                "SELECT 1 FROM sqlite_schema WHERE type='table' AND name='binding_hint'",
                vec![],
            )?;
            if !present.is_empty() {
                return Ok(());
            }
            let rows = tx.query(
                "SELECT schema_source,schema_definition FROM history_meta WHERE id=1",
                vec![],
            )?;
            let Some(row) = rows.first() else {
                return Ok(());
            };
            // The schema this migration records is the one before the
            // lineage and merge tables; later migrations add those.
            let current = source((self.layout, self.acquisition))
                .replacen(DIGESTS, "", 1)
                .replacen(SCRATCH, "", 1)
                .replacen(LINEAGE, "", 1);
            let table = HISTORY
                .find("CREATE TABLE binding_hint (")
                .and_then(|start| Some((start, start + HISTORY[start..].find(";\n")? + 2)))
                .map(|(start, end)| &HISTORY[start..end])
                .ok_or(BackendError::Integrity)?;
            let earlier = current.replacen(&format!("{table}\n"), "", 1);
            if row.get::<String>(0)? != earlier
                || row.get::<String>(1)? != definition(tx.connection, tx.work)?
            {
                return Ok(());
            }
            tx.bootstrap(table)?;
            let definition = definition(tx.connection, tx.work)?;
            tx.query(
                "UPDATE history_meta SET schema_source=?1,schema_definition=?2 WHERE id=1",
                vec![Param::Text(current), Param::Text(definition)],
            )?;
            Ok(())
        })
    }
    /// Adds the lineage tables (#353) to a Store written before them, fills
    /// them with `backfill` in the same write transaction and records the
    /// current schema: an additive migration (REBUILD.md kind 2) under the
    /// same conditions as [`Self::add_binding_hints`].
    pub(crate) fn add_lineage(
        &self,
        backfill: impl FnOnce(&Transaction<'_>) -> Result<(), BackendError>,
    ) -> Result<(), BackendError> {
        self.run::<_, BackendError>(true, |tx| {
            let present = tx.query(
                "SELECT 1 FROM sqlite_schema WHERE type='table' AND name='commit_lineage'",
                vec![],
            )?;
            if !present.is_empty() {
                return Ok(());
            }
            let rows = tx.query(
                "SELECT schema_source,schema_definition FROM history_meta WHERE id=1",
                vec![],
            )?;
            let Some(row) = rows.first() else {
                return Ok(());
            };
            let current = source((self.layout, self.acquisition))
                .replacen(DIGESTS, "", 1)
                .replacen(SCRATCH, "", 1);
            if row.get::<String>(0)? != current.replacen(LINEAGE, "", 1)
                || row.get::<String>(1)? != definition(tx.connection, tx.work)?
            {
                return Ok(());
            }
            tx.bootstrap(LINEAGE)?;
            backfill(tx)?;
            let definition = definition(tx.connection, tx.work)?;
            tx.query(
                "UPDATE history_meta SET schema_source=?1,schema_definition=?2 WHERE id=1",
                vec![Param::Text(current), Param::Text(definition)],
            )?;
            Ok(())
        })
    }
    /// Adds the merge build tables (#355) to a Store written before them and
    /// records the current schema: an additive migration (REBUILD.md kind 2)
    /// under the same conditions as [`Self::add_binding_hints`].
    pub(crate) fn add_merge_scratch(&self) -> Result<(), BackendError> {
        self.run::<_, BackendError>(true, |tx| {
            let present = tx.query(
                "SELECT 1 FROM sqlite_schema WHERE type='table' AND name='merge_record'",
                vec![],
            )?;
            if !present.is_empty() {
                return Ok(());
            }
            let rows = tx.query(
                "SELECT schema_source,schema_definition FROM history_meta WHERE id=1",
                vec![],
            )?;
            let Some(row) = rows.first() else {
                return Ok(());
            };
            let current = source((self.layout, self.acquisition)).replacen(DIGESTS, "", 1);
            if row.get::<String>(0)? != current.replacen(SCRATCH, "", 1)
                || row.get::<String>(1)? != definition(tx.connection, tx.work)?
            {
                return Ok(());
            }
            tx.bootstrap(SCRATCH)?;
            let definition = definition(tx.connection, tx.work)?;
            tx.query(
                "UPDATE history_meta SET schema_source=?1,schema_definition=?2 WHERE id=1",
                vec![Param::Text(current), Param::Text(definition)],
            )?;
            Ok(())
        })
    }
    /// Adds the file digest table (#364) to a Store written before it and
    /// records the current schema: an additive migration (REBUILD.md kind 2)
    /// under the same conditions as [`Self::add_binding_hints`]. Saved roots
    /// keep no digest; a Commit records them for the roots it builds.
    pub(crate) fn add_file_digests(&self) -> Result<(), BackendError> {
        self.run::<_, BackendError>(true, |tx| {
            let present = tx.query(
                "SELECT 1 FROM sqlite_schema WHERE type='table' AND name='file_digest'",
                vec![],
            )?;
            if !present.is_empty() {
                return Ok(());
            }
            let rows = tx.query(
                "SELECT schema_source,schema_definition FROM history_meta WHERE id=1",
                vec![],
            )?;
            let Some(row) = rows.first() else {
                return Ok(());
            };
            let current = source((self.layout, self.acquisition));
            if row.get::<String>(0)? != current.replacen(DIGESTS, "", 1)
                || row.get::<String>(1)? != definition(tx.connection, tx.work)?
            {
                return Ok(());
            }
            tx.bootstrap(DIGESTS)?;
            let definition = definition(tx.connection, tx.work)?;
            tx.query(
                "UPDATE history_meta SET schema_source=?1,schema_definition=?2 WHERE id=1",
                vec![Param::Text(current), Param::Text(definition)],
            )?;
            Ok(())
        })
    }
    pub(crate) fn validate(
        &self,
        catalog_id: layerfs_history::CatalogId,
        binding: &[u8],
    ) -> Result<u64, BackendError> {
        self.run::<_,BackendError>(false,|tx| {
   let selection=(self.layout,self.acquisition);
   check(tx.connection,tx.work,selection)?;
   let rows=tx.query("SELECT catalog_id,catalog_incarnation,binding_key,identity_format,schema_version,schema_source,schema_definition FROM history_meta WHERE id=1",vec![])?;
   let r=rows.first().filter(|_|rows.len()==1).ok_or(BackendError::Integrity)?;
   let incarnation:i64=r.get(1)?;
   if r.get::<Vec<u8>>(0)?!=catalog_id.as_slice() || r.get::<Vec<u8>>(2)?!=binding || incarnation<=0 || r.get::<i64>(3)?!=3 || r.get::<i64>(4)?!=self.schema_version() || r.get::<String>(5)?!=source(selection) || r.get::<String>(6)?!=definition(tx.connection,tx.work)? {return Err(BackendError::Integrity);}
   Ok(incarnation as u64)
  })
    }
}
