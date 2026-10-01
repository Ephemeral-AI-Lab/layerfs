//! Certified single-Branch verification index and operation-local SQL work.
use crate::{sql_windows, tree_diff, tree_facts, wire::Snapshot};
use layerfs_content::{
    filesystem::FilesystemRoot,
    inode_leaf::{decode_inode_value, encode_inode_value, InodeKind, InodeValue},
    ObjectId,
};
use rusqlite::{params, Connection, OptionalExtension};
pub const SCHEMA: &str = "
CREATE TABLE namespace_state(singleton INTEGER PRIMARY KEY CHECK(singleton=1),stamp BLOB NOT NULL,installing INTEGER NOT NULL DEFAULT 0);
CREATE TABLE namespace_inodes(serial INTEGER PRIMARY KEY,value BLOB NOT NULL,parent INTEGER);
CREATE TABLE namespace_pending(singleton INTEGER PRIMARY KEY CHECK(singleton=1),base BLOB NOT NULL,candidate BLOB NOT NULL,proved INTEGER NOT NULL,ready INTEGER NOT NULL);
CREATE TABLE namespace_changes(serial INTEGER PRIMARY KEY,before BLOB,after BLOB);
CREATE TABLE namespace_effects(serial INTEGER PRIMARY KEY,removed INTEGER NOT NULL,added INTEGER NOT NULL,old_parent INTEGER,new_parent INTEGER);
CREATE TABLE namespace_walk(serial INTEGER PRIMARY KEY,color INTEGER NOT NULL);
CREATE TABLE namespace_stack(depth INTEGER PRIMARY KEY,serial INTEGER NOT NULL);
";
#[derive(Default, Debug)]
pub struct Work {
    pub intrinsic: tree_facts::Work,
    pub inode_diff: tree_diff::Work,
    pub directory_expanded: u64,
    pub directory_compared: u64,
    pub directory_skipped: u64,
    pub changed_names: u64,
    pub affected_inodes: u64,
    pub parent_steps: u64,
    pub peak_owned_bytes: usize,
}
pub fn stamp(s: &Snapshot) -> Vec<u8> {
    let mut b = Vec::with_capacity(197);
    s.encode(&mut b);
    b
}
pub fn serial(key: &[u8]) -> Result<i64, String> {
    let n = u64::from_be_bytes(key.try_into().map_err(|_| "namespace serial width")?);
    if n == 0 || n > i64::MAX as u64 {
        return Err("namespace serial range".into());
    }
    Ok(n as i64)
}
pub fn value(bytes: Option<Vec<u8>>) -> Result<Option<InodeValue>, String> {
    bytes
        .map(|b| decode_inode_value(&b).map_err(|e| e.to_string()))
        .transpose()
}
pub fn committed(db: &Connection, id: i64) -> Result<Option<InodeValue>, String> {
    value(
        db.prepare_cached("SELECT value FROM namespace_inodes WHERE serial=?1")
            .map_err(|e| e.to_string())?
            .query_row([id], |r| r.get(0))
            .optional()
            .map_err(|e| e.to_string())?,
    )
}
pub fn proposed(db: &Connection, id: i64) -> Result<Option<InodeValue>, String> {
    let row: Option<Option<Vec<u8>>> = db
        .prepare_cached("SELECT after FROM namespace_changes WHERE serial=?1")
        .map_err(|e| e.to_string())?
        .query_row([id], |r| r.get(0))
        .optional()
        .map_err(|e| e.to_string())?;
    match row {
        Some(b) => value(b),
        None => committed(db, id),
    }
}
pub type Changed = (i64, Option<InodeValue>, Option<InodeValue>);
type RawChanged = (i64, Option<Vec<u8>>, Option<Vec<u8>>);
pub fn next_changed(db: &Connection, after: i64) -> Result<Option<Changed>, String> {
    let row: Option<RawChanged> = db.prepare_cached("SELECT serial,before,after FROM namespace_changes WHERE serial>?1 ORDER BY serial LIMIT 1").map_err(|e|e.to_string())?.query_row([after],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(|e|e.to_string())?;
    row.map(|(id, a, b)| Ok((id, value(a)?, value(b)?)))
        .transpose()
}
fn root_matches(root: FilesystemRoot, s: &Snapshot) -> Result<(), String> {
    if root.root_inode().serial() != 1
        || root.scope().object().as_bytes() != &s.scope
        || root.profile().as_bytes() != &s.profile
        || ObjectId::for_bytes(&root.encode().map_err(|e| e.to_string())?).as_bytes() != &s.root
    {
        return Err("namespace root/scope/profile stamp".into());
    }
    Ok(())
}
pub fn initialize_empty(db: &Connection, s: &Snapshot, fs: FilesystemRoot) -> Result<(), String> {
    root_matches(fs, s)?;
    if s.head.is_some() {
        return Err("namespace fresh genesis required".into());
    }
    let mut w = tree_facts::Work::default();
    let table = tree_facts::certify(db, fs.inode_table(), 0, true, 0, &mut w)?;
    if table.level != 0 || table.count != 1 {
        return Err("namespace genesis must be root-only".into());
    }
    let row = tree_facts::item(db, fs.inode_table(), 0)?;
    if serial(&row.key)? != 1 {
        return Err("namespace genesis root serial".into());
    }
    let bytes = row.value.ok_or("namespace genesis value")?;
    let root = decode_inode_value(&bytes).map_err(|e| e.to_string())?;
    root.validate(true).map_err(|e| e.to_string())?;
    if tree_facts::certify(db, root.content_root, 1, true, 0, &mut w)?.count != 0 {
        return Err("namespace genesis not empty".into());
    }
    db.execute("INSERT INTO namespace_inodes VALUES(1,?1,NULL)", [bytes])
        .map_err(|e| e.to_string())?;
    db.execute(
        "INSERT INTO namespace_state(singleton,stamp) VALUES(1,?1)",
        [stamp(s)],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
pub fn check_base(db: &Connection, s: &Snapshot) -> Result<(), String> {
    let (saved, installing): (Vec<u8>, bool) = db
        .query_row(
            "SELECT stamp,installing FROM namespace_state WHERE singleton=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|e| e.to_string())?;
    if installing {
        return Err("namespace index installation incomplete".into());
    }
    if saved != stamp(s) {
        return Err("namespace stale or uncertified selected base".into());
    }
    Ok(())
}
pub fn prepare(
    db: &Connection,
    s: &Snapshot,
    old: FilesystemRoot,
    new: FilesystemRoot,
) -> Result<Work, String> {
    check_base(db, s)?;
    root_matches(old, s)?;
    if new.root_inode() != old.root_inode()
        || new.scope() != old.scope()
        || new.profile() != old.profile()
    {
        return Err("namespace candidate context".into());
    }
    sql_windows::clear_namespace(db)?;
    db.execute("DELETE FROM namespace_pending", [])
        .map_err(|e| e.to_string())?;
    let candidate = ObjectId::for_bytes(&new.encode().map_err(|e| e.to_string())?);
    db.execute(
        "INSERT INTO namespace_pending VALUES(1,?1,?2,0,0)",
        params![stamp(s), candidate.as_bytes().as_slice()],
    )
    .map_err(|e| e.to_string())?;
    let mut w = Work::default();
    tree_facts::certify(db, old.inode_table(), 0, true, 0, &mut w.intrinsic)?;
    if tree_facts::node(db, new.inode_table())?.count > 512 {
        return Err("namespace population admission".into());
    }
    let node = tree_facts::certify(db, new.inode_table(), 0, true, 0, &mut w.intrinsic)?;
    if node.count > 512 {
        return Err("namespace population admission".into());
    }
    w.inode_diff = tree_diff::run(
        db,
        Some(old.inode_table()),
        Some(new.inode_table()),
        |key, a, b| {
            let id = serial(key)?;
            let before = a
                .map(|b| decode_inode_value(b).map_err(|e| e.to_string()))
                .transpose()?;
            if committed(db, id)? != before {
                return Err("namespace selected source/index mismatch".into());
            }
            db.prepare_cached("INSERT INTO namespace_changes VALUES(?1,?2,?3)")
                .map_err(|e| e.to_string())?
                .execute(params![id, a, b])
                .map_err(|e| e.to_string())?;
            Ok(())
        },
    )?;
    w.peak_owned_bytes = w.inode_diff.peak_owned_bytes;
    let mut after = 0;
    while let Some((id, a, b)) = next_changed(db, after)? {
        after = id;
        if a.zip(b).is_some_and(|(x, y)| x.kind != y.kind) {
            return Err("namespace inode kind transition unsupported".into());
        }
        if b.is_some_and(|v| v.kind == InodeKind::Symlink) {
            return Err("namespace symlink capability unsupported".into());
        }
        let x = a
            .filter(|v| v.kind == InodeKind::Directory)
            .map(|v| v.content_root);
        let y = b
            .filter(|v| v.kind == InodeKind::Directory)
            .map(|v| v.content_root);
        if x != y {
            for root in [x, y].into_iter().flatten() {
                if tree_facts::node(db, root)?.count > 512 {
                    return Err("namespace directory count admission".into());
                }
                tree_facts::certify(db, root, 1, true, 0, &mut w.intrinsic)?;
            }
            let mut names = 0u64;
            let diff = tree_diff::run(db, x, y, |_, old, new| {
                names += 1;
                if w.changed_names + names > 1024 {
                    return Err("namespace name effect admission".into());
                }
                for (bytes, add) in [(old, false), (new, true)] {
                    if let Some(bytes) = bytes {
                        crate::namespace_semantics::effect(db, serial(bytes)?, id, add)?;
                    }
                }
                Ok(())
            })?;
            w.directory_expanded += diff.expanded;
            w.directory_compared += diff.compared;
            w.directory_skipped += diff.skipped;
            w.changed_names += diff.changed;
            w.peak_owned_bytes = w.peak_owned_bytes.max(diff.peak_owned_bytes);
            if w.changed_names > 1024 {
                return Err("namespace name effect admission".into());
            }
        }
    }
    crate::namespace_semantics::validate(db, &mut w)?;
    db.execute(
        "UPDATE namespace_pending SET proved=1 WHERE singleton=1",
        [],
    )
    .map_err(|e| e.to_string())?;
    Ok(w)
}
pub fn seal(db: &Connection, s: &Snapshot, candidate: ObjectId) -> Result<(), String> {
    check_base(db, s)?;
    if db.execute("UPDATE namespace_pending SET ready=1 WHERE singleton=1 AND base=?1 AND candidate=?2 AND proved=1 AND ready=0",params![stamp(s),candidate.as_bytes().as_slice()]).map_err(|e|e.to_string())?!=1 {return Err("namespace pending seal identity".into());}
    Ok(())
}
pub fn install_known(db: &Connection, old: &Snapshot, new: &Snapshot) -> Result<(), String> {
    check_base(db, old)?;
    if old.stack != new.stack
        || old.branch != new.branch
        || old.base != new.base
        || old.scope != new.scope
        || old.profile != new.profile
    {
        return Err("namespace known publication context".into());
    }
    let pending: (Vec<u8>, Vec<u8>, bool) = db
        .query_row(
            "SELECT base,candidate,ready FROM namespace_pending WHERE singleton=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(|e| e.to_string())?;
    if pending.0 != stamp(old) || pending.1 != new.root || !pending.2 {
        return Err("namespace known publication seal".into());
    }
    if db
        .execute(
            "UPDATE namespace_state SET installing=1 WHERE singleton=1 AND installing=0",
            [],
        )
        .map_err(|e| e.to_string())?
        != 1
    {
        return Err("namespace installation owner".into());
    }
    let mut after = 0;
    while let Some((id, _, value)) = next_changed(db, after)? {
        after = id;
        match value {
            None => {
                db.execute("DELETE FROM namespace_inodes WHERE serial=?1", [id])
                    .map_err(|e| e.to_string())?;
            }
            Some(v) => {
                let parent = if id == 1 {
                    None
                } else {
                    Some(
                        crate::namespace_semantics::parent(db, id)?
                            .ok_or("namespace installed parent")?,
                    )
                };
                db.prepare_cached("INSERT INTO namespace_inodes VALUES(?1,?2,?3) ON CONFLICT(serial) DO UPDATE SET value=excluded.value,parent=excluded.parent").map_err(|e|e.to_string())?.execute(params![id,encode_inode_value(v).as_slice(),parent]).map_err(|e|e.to_string())?;
            }
        }
    }
    // Unchanged inode values can acquire a new parent through rename.
    let mut after = 0;
    loop {
        let row:Option<(i64,Option<i64>)>=db.prepare_cached("SELECT serial,new_parent FROM namespace_effects WHERE serial>?1 AND added=1 ORDER BY serial LIMIT 1").map_err(|e|e.to_string())?.query_row([after],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(|e|e.to_string())?;
        let Some((id, parent)) = row else { break };
        after = id;
        db.execute(
            "UPDATE namespace_inodes SET parent=?2 WHERE serial=?1",
            params![id, parent],
        )
        .map_err(|e| e.to_string())?;
    }
    sql_windows::clear_namespace(db)?;
    let tx = db.unchecked_transaction().map_err(|e| e.to_string())?;
    if tx
        .execute(
            "UPDATE namespace_state SET stamp=?1,installing=0 WHERE singleton=1 AND installing=1",
            [stamp(new)],
        )
        .map_err(|e| e.to_string())?
        != 1
    {
        return Err("namespace installation stamp identity".into());
    }
    if tx
        .execute("DELETE FROM namespace_pending WHERE singleton=1", [])
        .map_err(|e| e.to_string())?
        != 1
    {
        return Err("namespace pending installation identity".into());
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(())
}
