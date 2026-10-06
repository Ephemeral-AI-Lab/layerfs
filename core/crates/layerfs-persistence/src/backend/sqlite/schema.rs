//! Combined schema identity and deterministic definition validation.
use super::{connection::SqlWork, query};
use crate::backend::records::{BackendError, Param, Record};
use rusqlite::Connection;
use std::cell::RefCell;
pub(crate) const APPLICATION_ID: i64 = 1279677264;
pub(crate) const OBJECTS: &str = include_str!("../../../sql/sqlite/objects.sql");
pub(crate) const METADATA: &str = include_str!("../../../sql/sqlite/metadata.sql");
pub(crate) const HISTORY: &str = include_str!("../../../sql/sqlite/history.sql");
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
        "branch",
        "commit",
        "content_signature",
        "history_meta",
        "layer",
        "layer_stack",
        "metadata_value_group",
        "object_location",
        "pack",
        "scope_allocator",
        "store_policy",
        "workspace_stage",
    ];
    if layout.uses_units() {
        expected.push("pack_unit");
    }
    if acquisition == crate::SqliteAcquisitionSchema::Tables {
        // `sqlite_sequence` is the engine's never-reused operation identity counter.
        expected.extend([
            "init_entry",
            "init_native_file",
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
   tx.query(&format!("INSERT INTO history_meta(id,catalog_id,catalog_incarnation,binding_key,identity_format,schema_version,schema_source,schema_definition,next_stage_token) VALUES(1,?1,?2,?3,1,{version},?4,?5,1)"),vec![Param::Bytes(catalog_id.as_slice().to_vec()),Param::I64(history.incarnation as i64),Param::Bytes(history.binding_key.clone()),Param::Text(source(selection)),Param::Text(definition)])?;
   Ok(())
  })?;
        Ok(())
    }
    pub(crate) fn validate(
        &self,
        catalog_id: layerfs_history::CatalogId,
        binding: &[u8],
    ) -> Result<u64, BackendError> {
        self.run::<_,BackendError>(false,|tx| {
   let selection=(self.layout,self.acquisition);
   check(tx.connection,tx.work,selection)?;
   let rows=tx.query("SELECT catalog_id,catalog_incarnation,binding_key,identity_format,schema_version,schema_source,schema_definition,next_stage_token FROM history_meta WHERE id=1",vec![])?;
   let r=rows.first().filter(|_|rows.len()==1).ok_or(BackendError::Integrity)?;
   let incarnation:i64=r.get(1)?;
   if r.get::<Vec<u8>>(0)?!=catalog_id.as_slice() || r.get::<Vec<u8>>(2)?!=binding || incarnation<=0 || r.get::<i64>(3)?!=1 || r.get::<i64>(4)?!=self.schema_version() || r.get::<String>(5)?!=source(selection) || r.get::<String>(6)?!=definition(tx.connection,tx.work)? || r.get::<i64>(7)?<=0 {return Err(BackendError::Integrity);}
   Ok(incarnation as u64)
  })
    }
}
