//! Fixed indexed mutation windows on the existing SQLite owner.
use rusqlite::Connection;
pub const ROWS: usize = 64;
#[derive(Clone, Copy)]
pub enum Table {
    Prepared,
    Edits,
    NamespaceChanges,
    NamespaceEffects,
    NamespaceWalk,
    NamespaceStack,
}
impl Table {
    fn name(self) -> &'static str {
        match self {
            Self::Prepared => "prepared",
            Self::Edits => "edits",
            Self::NamespaceChanges => "namespace_changes",
            Self::NamespaceEffects => "namespace_effects",
            Self::NamespaceWalk => "namespace_walk",
            Self::NamespaceStack => "namespace_stack",
        }
    }
}
pub fn clear(db: &Connection, table: Table) -> Result<u64, String> {
    let name = table.name();
    let sql = format!(
        "DELETE FROM {name} WHERE rowid IN (SELECT rowid FROM {name} ORDER BY rowid LIMIT 64)"
    );
    let mut batches = 0;
    loop {
        let n = db
            .prepare_cached(&sql)
            .map_err(|e| e.to_string())?
            .execute([])
            .map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        if n > ROWS {
            return Err("SQL clear window invariant".into());
        }
        batches += 1;
    }
    Ok(batches)
}
pub fn clear_namespace(db: &Connection) -> Result<(), String> {
    for table in [
        Table::NamespaceChanges,
        Table::NamespaceEffects,
        Table::NamespaceWalk,
        Table::NamespaceStack,
    ] {
        clear(db, table)?;
    }
    Ok(())
}
