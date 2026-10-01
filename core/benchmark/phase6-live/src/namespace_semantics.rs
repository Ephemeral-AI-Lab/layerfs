//! Exact single-link reference effects and memoized proposed parent chains.
use crate::namespace_index::{self as index, Work};
use layerfs_content::inode_leaf::InodeKind;
use rusqlite::{params, Connection, OptionalExtension};
type Effect = (i64, i64, Option<i64>, Option<i64>);
fn get(db: &Connection, id: i64) -> Result<Effect, String> {
    Ok(db
        .prepare_cached(
            "SELECT removed,added,old_parent,new_parent FROM namespace_effects WHERE serial=?1",
        )
        .map_err(|e| e.to_string())?
        .query_row([id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .optional()
        .map_err(|e| e.to_string())?
        .unwrap_or((0, 0, None, None)))
}
pub fn effect(db: &Connection, id: i64, parent: i64, add: bool) -> Result<(), String> {
    if id == 1 {
        return Err("namespace root binding forbidden".into());
    }
    let (removed, added, old, new) = get(db, id)?;
    if (add && added != 0) || (!add && removed != 0) {
        return Err("namespace alias/reference multiplicity unsupported".into());
    }
    db.prepare_cached("INSERT INTO namespace_effects VALUES(?1,?2,?3,?4,?5) ON CONFLICT(serial) DO UPDATE SET removed=excluded.removed,added=excluded.added,old_parent=excluded.old_parent,new_parent=excluded.new_parent").map_err(|e|e.to_string())?.execute(params![id,removed+i64::from(!add),added+i64::from(add),if add{old}else{Some(parent)},if add{Some(parent)}else{new}]).map_err(|e|e.to_string())?;
    Ok(())
}
fn old_parent(db: &Connection, id: i64) -> Result<Option<i64>, String> {
    Ok(db
        .prepare_cached("SELECT parent FROM namespace_inodes WHERE serial=?1")
        .map_err(|e| e.to_string())?
        .query_row([id], |r| r.get(0))
        .optional()
        .map_err(|e| e.to_string())?
        .flatten())
}
pub fn parent(db: &Connection, id: i64) -> Result<Option<i64>, String> {
    let (removed, added, _, new) = get(db, id)?;
    if added != 0 {
        Ok(new)
    } else if removed != 0 {
        Ok(None)
    } else {
        old_parent(db, id)
    }
}
fn next_affected(db: &Connection, after: i64) -> Result<Option<i64>, String> {
    let a: Option<i64> = db
        .prepare_cached(
            "SELECT serial FROM namespace_changes WHERE serial>?1 ORDER BY serial LIMIT 1",
        )
        .map_err(|e| e.to_string())?
        .query_row([after], |r| r.get(0))
        .optional()
        .map_err(|e| e.to_string())?;
    let b: Option<i64> = db
        .prepare_cached(
            "SELECT serial FROM namespace_effects WHERE serial>?1 ORDER BY serial LIMIT 1",
        )
        .map_err(|e| e.to_string())?
        .query_row([after], |r| r.get(0))
        .optional()
        .map_err(|e| e.to_string())?;
    Ok(match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    })
}
pub fn validate(db: &Connection, w: &mut Work) -> Result<(), String> {
    let mut after = 0;
    while let Some(id) = next_affected(db, after)? {
        after = id;
        w.affected_inodes += 1;
        if w.affected_inodes > 1024 {
            return Err("namespace affected record admission".into());
        }
        let old = index::committed(db, id)?;
        let new = index::proposed(db, id)?;
        let (removed, added, before, _) = get(db, id)?;
        if removed != 0 && (old.is_none() || before != old_parent(db, id)?) {
            return Err("namespace removal source/parent mismatch".into());
        }
        let count = old.map_or(0, |v| v.namespace_ref_count as i64) - removed + added;
        if count < 0
            || new.map_or(0, |v| v.namespace_ref_count) != count as u64
            || new.is_some_and(|v| v.namespace_ref_count != u64::from(id != 1))
        {
            return Err("namespace reference count/record mismatch".into());
        }
        if id == 1 && new.is_none() {
            return Err("namespace root removal".into());
        }
        if let Some(v) = new {
            v.validate(id == 1).map_err(|e| e.to_string())?;
            if v.kind == InodeKind::Symlink {
                return Err("namespace symlink unsupported".into());
            }
            if id != 1 {
                let p = parent(db, id)?.ok_or("namespace missing proposed parent")?;
                if index::proposed(db, p)?.is_none_or(|v| v.kind != InodeKind::Directory) {
                    return Err("namespace parent not a live directory".into());
                }
            }
        }
    }
    let mut after = 0;
    while let Some(id) = next_affected(db, after)? {
        after = id;
        if index::proposed(db, id)?.is_some_and(|v| v.kind == InodeKind::Directory) {
            walk(db, id, w)?;
        }
    }
    Ok(())
}
fn walk(db: &Connection, start: i64, w: &mut Work) -> Result<(), String> {
    let mut id = start;
    let mut depth = 0;
    crate::sql_windows::clear(db, crate::sql_windows::Table::NamespaceStack)?;
    loop {
        if id == 1 {
            break;
        }
        let color: Option<i64> = db
            .prepare_cached("SELECT color FROM namespace_walk WHERE serial=?1")
            .map_err(|e| e.to_string())?
            .query_row([id], |r| r.get(0))
            .optional()
            .map_err(|e| e.to_string())?;
        match color {
            Some(1) => return Err("namespace proposed parent cycle".into()),
            Some(2) => break,
            _ => {}
        }
        if index::proposed(db, id)?.is_none_or(|v| v.kind != InodeKind::Directory) {
            return Err("namespace parent chain not a live directory".into());
        }
        depth += 1;
        w.parent_steps += 1;
        if depth > 512 || w.parent_steps > 512 {
            return Err("namespace parent walk admission".into());
        }
        db.prepare_cached("INSERT INTO namespace_walk VALUES(?1,1)")
            .map_err(|e| e.to_string())?
            .execute([id])
            .map_err(|e| e.to_string())?;
        db.prepare_cached("INSERT INTO namespace_stack VALUES(?1,?2)")
            .map_err(|e| e.to_string())?
            .execute(params![depth, id])
            .map_err(|e| e.to_string())?;
        id = parent(db, id)?.ok_or("namespace disconnected parent chain")?;
    }
    loop {
        let tx = db.unchecked_transaction().map_err(|e| e.to_string())?;
        let n=tx.execute("UPDATE namespace_walk SET color=2 WHERE serial IN (SELECT serial FROM namespace_stack ORDER BY depth LIMIT 64)",[]).map_err(|e|e.to_string())?;
        tx.execute("DELETE FROM namespace_stack WHERE depth IN (SELECT depth FROM namespace_stack ORDER BY depth LIMIT 64)",[]).map_err(|e|e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        if n > crate::sql_windows::ROWS {
            return Err("namespace parent color window".into());
        }
    }
    Ok(())
}
