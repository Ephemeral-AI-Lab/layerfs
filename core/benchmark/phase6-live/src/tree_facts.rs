//! Authenticated compact C1 tree facts, stored on existing locator SQL.
use layerfs_content::{
    filesystem::{directory::codec as d, inode::codec as i},
    inode_leaf::{decode_inode_value, encode_inode_value},
    ObjectId, ObjectRole,
};
use rusqlite::{params, Connection, OptionalExtension};
pub const SCHEMA:&str="CREATE TABLE tree_nodes(id BLOB PRIMARY KEY,flavor INTEGER,level INTEGER,entries INTEGER,count INTEGER,bytes INTEGER,minimum BLOB,maximum BLOB,filled INTEGER,certified INTEGER) WITHOUT ROWID;CREATE TABLE tree_items(parent BLOB,ordinal INTEGER,key BLOB,child BLOB,value BLOB,PRIMARY KEY(parent,ordinal)) WITHOUT ROWID;";
#[derive(Clone, Debug)]
pub struct Node {
    pub flavor: u8,
    pub level: u8,
    pub entries: usize,
    pub count: u64,
    pub bytes: u64,
    pub min: Vec<u8>,
    pub max: Vec<u8>,
    pub filled: bool,
    pub certified: bool,
}
#[derive(Clone, Debug)]
pub struct Item {
    pub key: Vec<u8>,
    pub child: Option<ObjectId>,
    pub value: Option<Vec<u8>>,
}
#[derive(Default, Debug)]
pub struct Work {
    pub visited: u64,
    pub reused: u64,
    pub edges: u64,
}
fn n(x: u64) -> Result<i64, String> {
    i64::try_from(x).map_err(|_| "tree SQL integer range".into())
}
pub fn record(
    db: &Connection,
    id: ObjectId,
    role: ObjectRole,
    canonical: &[u8],
) -> Result<(), String> {
    if !matches!(
        role,
        ObjectRole::InodeLeaf
            | ObjectRole::InodeBranch
            | ObjectRole::DirectoryLeaf
            | ObjectRole::DirectoryBranch
    ) {
        return Ok(());
    }
    if ObjectId::for_bytes(canonical) != id {
        return Err("tree canonical identity".into());
    }
    let (flavor, level, count, bytes, filled, items) = match role {
        ObjectRole::InodeLeaf | ObjectRole::InodeBranch => {
            let page = i::decode_inode_page(canonical).map_err(|e| e.to_string())?;
            let level = page.level();
            let count = page.subtree_count();
            let bytes = page.subtree_bytes();
            let rows = match page {
                i::InodePage::Leaf { entries } => {
                    if role != ObjectRole::InodeLeaf {
                        return Err("inode registered role".into());
                    }
                    entries
                        .into_iter()
                        .map(|(key, value)| Item {
                            key: key.to_be_bytes().to_vec(),
                            child: None,
                            value: Some(encode_inode_value(value).to_vec()),
                        })
                        .collect::<Vec<_>>()
                }
                i::InodePage::Branch { children, .. } => {
                    if role != ObjectRole::InodeBranch {
                        return Err("inode registered role".into());
                    }
                    children
                        .into_iter()
                        .map(|(key, id)| Item {
                            key: key.to_be_bytes().to_vec(),
                            child: Some(id),
                            value: None,
                        })
                        .collect()
                }
            };
            (
                0u8,
                level,
                count,
                bytes,
                i::filled_page(rows.len(), level),
                rows,
            )
        }
        _ => {
            let page = d::decode_directory_page(canonical).map_err(|e| e.to_string())?;
            let level = page.level();
            let count = page.subtree_count();
            let bytes = page.subtree_bytes();
            let rows = match page {
                d::DirectoryPage::Leaf { entries } => {
                    if role != ObjectRole::DirectoryLeaf {
                        return Err("directory registered role".into());
                    }
                    entries
                        .into_iter()
                        .map(|(key, value)| Item {
                            key: key.as_bytes().to_vec(),
                            child: None,
                            value: Some(value.to_be_bytes().to_vec()),
                        })
                        .collect::<Vec<_>>()
                }
                d::DirectoryPage::Branch { children, .. } => {
                    if role != ObjectRole::DirectoryBranch {
                        return Err("directory registered role".into());
                    }
                    children
                        .into_iter()
                        .map(|(key, id)| Item {
                            key: key.as_bytes().to_vec(),
                            child: Some(id),
                            value: None,
                        })
                        .collect()
                }
            };
            (
                1u8,
                level,
                count,
                bytes,
                d::filled_page(canonical.len()),
                rows,
            )
        }
    };
    let max = items.last().map_or(Vec::new(), |r| r.key.clone());
    let min = if level == 0 {
        items.first().map_or(Vec::new(), |r| r.key.clone())
    } else {
        Vec::new()
    };
    if db
        .prepare_cached("INSERT OR IGNORE INTO tree_nodes VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,0)")
        .map_err(|e| e.to_string())?
        .execute(params![
            id.as_bytes().as_slice(),
            flavor,
            level,
            items.len() as i64,
            n(count)?,
            n(bytes)?,
            min,
            max,
            filled
        ])
        .map_err(|e| e.to_string())?
        == 0
    {
        return Ok(());
    }
    for (j, row) in items.into_iter().enumerate() {
        db.prepare_cached("INSERT INTO tree_items VALUES(?1,?2,?3,?4,?5)")
            .map_err(|e| e.to_string())?
            .execute(params![
                id.as_bytes().as_slice(),
                j as i64,
                row.key,
                row.child.map(|id| id.as_bytes().to_vec()),
                row.value
            ])
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
pub fn node(db: &Connection, id: ObjectId) -> Result<Node, String> {
    db.prepare_cached("SELECT flavor,level,entries,count,bytes,minimum,maximum,filled,certified FROM tree_nodes WHERE id=?1").map_err(|e|e.to_string())?.query_row([id.as_bytes().as_slice()],|r|Ok(Node{flavor:r.get(0)?,level:r.get(1)?,entries:r.get::<_,i64>(2)? as usize,count:r.get::<_,i64>(3)? as u64,bytes:r.get::<_,i64>(4)? as u64,min:r.get(5)?,max:r.get(6)?,filled:r.get(7)?,certified:r.get(8)?})).optional().map_err(|e|e.to_string())?.ok_or("unregistered tree node".into())
}
pub fn item(db: &Connection, id: ObjectId, ordinal: usize) -> Result<Item, String> {
    let mut q = db
        .prepare_cached("SELECT key,child,value FROM tree_items WHERE parent=?1 AND ordinal=?2")
        .map_err(|e| e.to_string())?;
    let row: (Vec<u8>, Option<Vec<u8>>, Option<Vec<u8>>) = q
        .query_row(params![id.as_bytes().as_slice(), ordinal as i64], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .map_err(|e| e.to_string())?;
    Ok(Item {
        key: row.0,
        child: row
            .1
            .map(|b| ObjectId::from_bytes(&b).map_err(|e| e.to_string()))
            .transpose()?,
        value: row.2,
    })
}
pub fn certify(
    db: &Connection,
    id: ObjectId,
    flavor: u8,
    root: bool,
    depth: usize,
    work: &mut Work,
) -> Result<Node, String> {
    if depth > 31 {
        return Err("tree certificate depth".into());
    }
    let mut value = node(db, id)?;
    work.visited += 1;
    if value.flavor != flavor || (!root && !value.filled) || (value.level > 0 && value.entries < 2)
    {
        return Err("tree role/fill context".into());
    }
    if value.certified {
        work.reused += 1;
        return Ok(value);
    }
    if value.level > 0 {
        let mut count = 0;
        let mut bytes = 0;
        let mut max = Vec::new();
        let mut min = Vec::new();
        for j in 0..value.entries {
            let row = item(db, id, j)?;
            let child = certify(
                db,
                row.child.ok_or("tree child absent")?,
                flavor,
                false,
                depth + 1,
                work,
            )?;
            work.edges += 1;
            if child.level + 1 != value.level
                || child.max != row.key
                || child.min.is_empty()
                || (j > 0 && child.min <= max)
            {
                return Err("tree child level/range/summary".into());
            }
            if j == 0 {
                min = child.min.clone()
            }
            max = child.max;
            count += child.count;
            bytes += child.bytes;
        }
        if count != value.count || bytes != value.bytes || max != value.max {
            return Err("tree aggregate child count/bytes".into());
        }
        value.min = min;
    } else if flavor == 0 {
        for j in 0..value.entries {
            let row = item(db, id, j)?;
            let serial = u64::from_be_bytes(row.key.try_into().map_err(|_| "inode key width")?);
            decode_inode_value(&row.value.ok_or("inode value absent")?)
                .map_err(|e| e.to_string())?
                .validate(serial == 1)
                .map_err(|e| e.to_string())?;
        }
    }
    db.prepare_cached("UPDATE tree_nodes SET minimum=?2,certified=1 WHERE id=?1")
        .map_err(|e| e.to_string())?
        .execute(params![id.as_bytes().as_slice(), &value.min])
        .map_err(|e| e.to_string())?;
    value.certified = true;
    Ok(value)
}
