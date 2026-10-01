//! Immutable file closure facts from authenticated canonical registration.
//! Namespace certification is deliberately a separate obligation.
use layerfs_content::{
    file::{
        mapping::{self, ExtentNode},
        whole_file_payload,
    },
    object::decode_bytes_object,
    ObjectId, ObjectRole,
};
use rusqlite::{params, Connection, OptionalExtension};

pub const SCHEMA: &str = "CREATE TABLE file_facts(id BLOB PRIMARY KEY,kind INTEGER,bytes INTEGER,extents INTEGER,level INTEGER,nonroot INTEGER,edges INTEGER,certified INTEGER) WITHOUT ROWID;CREATE TABLE file_edges(parent BLOB,ordinal INTEGER,child BLOB,kind INTEGER,bytes INTEGER,extents INTEGER,level INTEGER,minimum INTEGER,nonroot INTEGER,PRIMARY KEY(parent,ordinal)) WITHOUT ROWID;";
#[derive(Clone, Copy, Default, Debug)]
pub struct Work {
    pub visited: u64,
    pub reused: u64,
    pub edges: u64,
    pub newly_certified: u64,
}
struct Edge {
    id: ObjectId,
    kind: u8,
    bytes: u64,
    extents: u64,
    level: u8,
    minimum: bool,
    nonroot: bool,
}
struct Fact {
    kind: u8,
    bytes: u64,
    extents: u64,
    level: u8,
    nonroot: bool,
    edges: Vec<Edge>,
}
#[derive(Clone, Copy)]
struct Summary {
    kind: u8,
    bytes: u64,
    extents: u64,
    level: u8,
    nonroot: bool,
    edges: usize,
    certified: bool,
}
fn int(n: u64) -> Result<i64, String> {
    i64::try_from(n).map_err(|_| "file summary SQL range unsupported".into())
}
fn parse(role: ObjectRole, canonical: &[u8]) -> Result<Option<Fact>, String> {
    let mut fact = Fact {
        kind: 0,
        bytes: 0,
        extents: 0,
        level: 0,
        nonroot: false,
        edges: Vec::new(),
    };
    match role {
        ObjectRole::WholeFile => {
            let payload = whole_file_payload(canonical)
                .map_err(|e| e.to_string())?
                .ok_or("registered whole-file role")?;
            fact.bytes = payload.len() as u64;
        }
        ObjectRole::Chunk => {
            fact.kind = 1;
            fact.bytes = mapping::decode_chunk_payload(
                decode_bytes_object(canonical).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?
            .len() as u64;
        }
        ObjectRole::ExtentLeaf | ObjectRole::ExtentBranch => {
            let node = mapping::decode_node(canonical).map_err(|e| e.to_string())?;
            fact.bytes = node.logical_len();
            fact.extents = node.extent_count();
            fact.level = node.level();
            fact.nonroot = node.validate(false).is_ok();
            match node {
                ExtentNode::Leaf { extents, .. } => {
                    if role != ObjectRole::ExtentLeaf {
                        return Err("registered extent role".into());
                    }
                    fact.kind = 2;
                    for e in extents {
                        fact.edges.push(Edge {
                            id: e.payload_object_id(),
                            kind: 1,
                            bytes: u64::from(e.source_offset()) + u64::from(e.logical_length()),
                            extents: 0,
                            level: 0,
                            minimum: true,
                            nonroot: false,
                        });
                    }
                }
                ExtentNode::Branch {
                    level, children, ..
                } => {
                    if role != ObjectRole::ExtentBranch {
                        return Err("registered extent role".into());
                    }
                    fact.kind = 3;
                    let mut bytes = 0;
                    let mut extents = 0;
                    for child in children {
                        fact.edges.push(Edge {
                            id: child.child_object_id,
                            kind: if level == 1 { 2 } else { 3 },
                            bytes: child.cumulative_logical_end - bytes,
                            extents: child.cumulative_extent_end - extents,
                            level: level - 1,
                            minimum: false,
                            nonroot: true,
                        });
                        bytes = child.cumulative_logical_end;
                        extents = child.cumulative_extent_end;
                    }
                }
            }
        }
        ObjectRole::FileState => {
            let state = mapping::decode_file_state(canonical).map_err(|e| e.to_string())?;
            fact.kind = 4;
            fact.bytes = state.logical_len;
            fact.extents = state.extent_count;
            fact.level = state.tree_level;
            fact.edges.push(Edge {
                id: state.mapping_root,
                kind: if state.tree_level == 0 { 2 } else { 3 },
                bytes: state.logical_len,
                extents: state.extent_count,
                level: state.tree_level,
                minimum: false,
                nonroot: false,
            });
        }
        _ => return Ok(None),
    }
    Ok(Some(fact))
}
/// Called in the same bounded transaction as exact locator registration.
pub fn record(
    db: &Connection,
    id: ObjectId,
    role: ObjectRole,
    canonical: &[u8],
) -> Result<(), String> {
    if ObjectId::for_bytes(canonical) != id {
        return Err("file fact canonical identity".into());
    }
    let Some(fact) = parse(role, canonical)? else {
        return Ok(());
    };
    let inserted = db
        .prepare_cached("INSERT OR IGNORE INTO file_facts VALUES(?1,?2,?3,?4,?5,?6,?7,0)")
        .map_err(|e| e.to_string())?
        .execute(params![
            id.as_bytes().as_slice(),
            fact.kind,
            int(fact.bytes)?,
            int(fact.extents)?,
            fact.level,
            fact.nonroot,
            fact.edges.len() as i64
        ])
        .map_err(|e| e.to_string())?;
    if inserted == 0 {
        return Ok(());
    }
    for (i, e) in fact.edges.iter().enumerate() {
        db.prepare_cached("INSERT INTO file_edges VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)")
            .map_err(|e| e.to_string())?
            .execute(params![
                id.as_bytes().as_slice(),
                i as i64,
                e.id.as_bytes().as_slice(),
                e.kind,
                int(e.bytes)?,
                int(e.extents)?,
                e.level,
                e.minimum,
                e.nonroot
            ])
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
fn summary(db: &Connection, id: ObjectId) -> Result<Summary, String> {
    db.prepare_cached(
        "SELECT kind,bytes,extents,level,nonroot,edges,certified FROM file_facts WHERE id=?1",
    )
    .map_err(|e| e.to_string())?
    .query_row([id.as_bytes().as_slice()], |r| {
        Ok(Summary {
            kind: r.get(0)?,
            bytes: r.get::<_, i64>(1)? as u64,
            extents: r.get::<_, i64>(2)? as u64,
            level: r.get(3)?,
            nonroot: r.get(4)?,
            edges: r.get::<_, i64>(5)? as usize,
            certified: r.get(6)?,
        })
    })
    .optional()
    .map_err(|e| e.to_string())?
    .ok_or("unregistered file graph child".into())
}
fn child(db: &Connection, id: ObjectId, index: usize) -> Result<Edge, String> {
    let r:(Vec<u8>,u8,i64,i64,u8,bool,bool)=db.prepare_cached("SELECT child,kind,bytes,extents,level,minimum,nonroot FROM file_edges WHERE parent=?1 AND ordinal=?2").map_err(|e|e.to_string())?.query_row(params![id.as_bytes().as_slice(),index as i64],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?))).map_err(|e|e.to_string())?;
    Ok(Edge {
        id: ObjectId::from_bytes(&r.0).map_err(|e| e.to_string())?,
        kind: r.1,
        bytes: r.2 as u64,
        extents: r.3 as u64,
        level: r.4,
        minimum: r.5,
        nonroot: r.6,
    })
}
fn certify(
    db: &Connection,
    id: ObjectId,
    value: Summary,
    depth: usize,
    work: &mut Work,
) -> Result<(), String> {
    // FileState -> mapping levels31..0 -> Chunk, at most34 actual frames.
    if depth > mapping::MAX_LEVEL as usize + 2 {
        return Err("file certificate graph depth".into());
    }
    work.visited += 1;
    if value.certified {
        work.reused += 1;
        return Ok(());
    }
    for i in 0..value.edges {
        let e = child(db, id, i)?;
        let actual = summary(db, e.id)?;
        work.edges += 1;
        if actual.kind != e.kind
            || actual.level != e.level
            || (e.nonroot && !actual.nonroot)
            || (if e.minimum {
                actual.bytes < e.bytes
            } else {
                actual.bytes != e.bytes || actual.extents != e.extents
            })
        {
            return Err("file graph child role/range/summary/partition".into());
        }
        certify(db, e.id, actual, depth + 1, work)?;
    }
    db.prepare_cached("UPDATE file_facts SET certified=1 WHERE id=?1")
        .map_err(|e| e.to_string())?
        .execute([id.as_bytes().as_slice()])
        .map_err(|e| e.to_string())?;
    work.newly_certified += 1;
    Ok(())
}
/// One selected file root, reusing only exact immutable certified summaries.
pub fn certify_file(db: &Connection, root: ObjectId) -> Result<Work, String> {
    let value = summary(db, root)?;
    if value.kind != 0 && value.kind != 4 {
        return Err("file root role".into());
    }
    let mut work = Work::default();
    certify(db, root, value, 0, &mut work)?;
    Ok(work)
}
