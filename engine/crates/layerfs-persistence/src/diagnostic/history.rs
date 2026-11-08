//! Authoritative history validation and exact root-context qualification.
use super::{
    emit, reader::Reader, scratch::Scratch, transaction::View, Observation, Options, Outcome,
};
use crate::backend::records::{BackendError, Param};
use layerfs_content::{
    filesystem::{
        qualify::{qualify_root, QualificationWork, RootContext},
        FilesystemRootId, InodeScope,
    },
    ObjectId,
};
use layerfs_history::LayerId;
use rusqlite::params;

pub(super) fn run(
    view: &dyn View,
    reader: Option<&Reader<'_>>,
    options: Options,
    observer: &mut dyn FnMut(Observation),
) -> Result<(), BackendError> {
    let mut scratch = Scratch::new().map_err(|_| BackendError::Capacity)?;
    scratch.observe(options, |scratch| {
        inspect(view, scratch, reader, options, observer)
    })
}

fn inspect(
    view: &dyn View,
    scratch: &mut Scratch,
    reader: Option<&Reader<'_>>,
    options: Options,
    observer: &mut dyn FnMut(Observation),
) -> Result<(), BackendError> {
    scratch.0.execute_batch("CREATE TABLE nodes(id BLOB PRIMARY KEY,kind INTEGER NOT NULL,stack BLOB NOT NULL,root BLOB NOT NULL,parent BLOB,base BLOB,second BLOB,source BLOB,source_branch BLOB,done INTEGER NOT NULL DEFAULT 0,height INTEGER,depth INTEGER,newest INTEGER,run BLOB,overwrote INTEGER NOT NULL DEFAULT 0) WITHOUT ROWID; CREATE TABLE edges(child BLOB NOT NULL,parent BLOB NOT NULL,PRIMARY KEY(child,parent)) WITHOUT ROWID; CREATE INDEX parent_edges ON edges(parent,child); CREATE TABLE stacks(id BLOB PRIMARY KEY,scope BLOB NOT NULL,profile BLOB NOT NULL,head BLOB NOT NULL) WITHOUT ROWID; CREATE TABLE branches(id BLOB PRIMARY KEY,stack BLOB NOT NULL,base BLOB NOT NULL,head BLOB,fork BLOB) WITHOUT ROWID; CREATE TABLE contexts(root BLOB,scope BLOB,profile BLOB,PRIMARY KEY(root,scope,profile)) WITHOUT ROWID;").map_err(sql)?;
    let mut failures = 0u64;
    inventory(view, scratch, options, observer, &mut failures)?;
    for (id,query,message) in [
        ("store.history.references","SELECT e.child,e.parent FROM edges e LEFT JOIN nodes p ON p.id=e.parent LEFT JOIN nodes c ON c.id=e.child WHERE p.id IS NULL OR p.stack!=c.stack","missing or cross-stack history dependency"),
        ("store.history.stacks","SELECT n.id,n.stack FROM nodes n LEFT JOIN stacks s ON s.id=n.stack WHERE s.id IS NULL","history node references absent stack"),
        ("store.history.heads","SELECT s.id,s.head FROM stacks s LEFT JOIN nodes n ON n.id=s.head WHERE n.id IS NULL OR n.kind!=2 OR n.stack!=s.id","stack head is missing or belongs to another stack"),
        ("store.history.branches","SELECT b.id,b.base FROM branches b LEFT JOIN nodes n ON n.id=b.base LEFT JOIN stacks s ON s.id=b.stack WHERE n.id IS NULL OR n.kind!=2 OR n.stack!=b.stack OR s.id IS NULL","branch base/stack invalid"),
        ("store.history.branches","SELECT b.id,b.head FROM branches b LEFT JOIN nodes n ON n.id=b.head WHERE b.head IS NOT NULL AND (n.id IS NULL OR n.kind!=1 OR n.stack!=b.stack)","branch head invalid"),
        ("store.history.branches","SELECT b.id,b.fork FROM branches b LEFT JOIN nodes n ON n.id=b.fork WHERE b.fork IS NOT NULL AND (n.id IS NULL OR n.stack!=b.stack)","branch fork provenance invalid"),
        ("store.history.provenance","SELECT n.id,n.source FROM nodes n LEFT JOIN nodes c ON c.id=n.source LEFT JOIN branches b ON b.id=n.source_branch WHERE n.kind=2 AND n.parent IS NOT NULL AND (c.id IS NULL OR c.kind!=1 OR c.stack!=n.stack OR c.root!=n.root OR b.id IS NULL OR b.stack!=n.stack)","Layer source Commit/Branch/root mismatch"),
    ]{
        let mut stmt=scratch.0.prepare(query).map_err(sql)?;
        let mut rows=stmt.query([]).map_err(sql)?;
        while let Some(row)=rows.next().map_err(sql)? {options.check()?;failures+=1;let entity:Vec<u8>=row.get(0).map_err(sql)?;emit(observer,id,Outcome::Fail,format!("{}: {message}",hex(&entity)));}
    }
    scratch.0.execute("INSERT OR IGNORE INTO edges(child,parent) SELECT id,parent FROM nodes WHERE parent IS NOT NULL",[]).map_err(sql)?;
    // All semantic edges are already copied. Peel parents before children;
    // bounded batches and indexed work avoid resident graph-sized sets.
    loop {
        options.check()?;
        let batch = {
            let mut stmt=scratch.0.prepare("SELECT n.id FROM nodes n WHERE done=0 AND NOT EXISTS(SELECT 1 FROM edges e JOIN nodes p ON p.id=e.parent WHERE e.child=n.id AND p.done=0) ORDER BY n.id LIMIT 64").map_err(sql)?;
            let rows = stmt
                .query_map([], |r| r.get::<_, Vec<u8>>(0))
                .map_err(sql)?;
            rows.collect::<Result<Vec<_>, _>>().map_err(sql)?
        };
        if batch.is_empty() {
            break;
        }
        for id in batch {
            scratch
                .0
                .execute("UPDATE nodes SET done=1 WHERE id=?1", [id])
                .map_err(sql)?;
        }
    }
    let cyclic: i64 = scratch
        .0
        .query_row("SELECT count(*) FROM nodes WHERE done=0", [], |r| r.get(0))
        .map_err(sql)?;
    if cyclic > 0 {
        failures += 1;
        emit(
            observer,
            "store.history.cycles",
            Outcome::Fail,
            format!("{cyclic} nodes are in or depend on cyclic history"),
        );
    }
    emit(observer,"store.history.summary",if failures==0{Outcome::Pass}else{Outcome::Fail},format!("all stored history records, identities, references, stack membership and graph checked; {failures} findings"));
    if let Some(reader) = reader {
        qualify(scratch, reader, options, observer)?;
    }
    if failures == 0 {
        super::lineage::run(view, scratch, options, observer)?;
    } else {
        emit(
            observer,
            "store.history.lineage",
            Outcome::Unknown,
            "independent lineage reconstruction blocked by invalid authoritative history",
        );
    }
    Ok(())
}

fn inventory(
    view: &dyn View,
    scratch: &mut Scratch,
    options: Options,
    observer: &mut dyn FnMut(Observation),
    failures: &mut u64,
) -> Result<(), BackendError> {
    for (table,key,columns,kind) in [
        ("layer_stack","layer_stack_id","layer_stack_id,name,scope_id,profile_id,head_layer_id",0),
        ("branch","branch_id","branch_id,layer_stack_id,name,base_layer_id,head_commit_id,forked_from_id",3),
        ("layer","layer_id","layer_id,layer_stack_id,parent_layer_id,root_id,source_branch_id,source_commit_id,logical,0",2),
        ("\"commit\"","commit_id","commit_id,layer_stack_id,root_id,parent_commit_id,base_layer_id,second_parent_id,branch_id,via,message,merged_from_id,created_ms,logical,new_canonical,new_stored,new_metadata,lww,displaced_id",1),
    ] {
        let mut after=Vec::<u8>::new();
        loop {
            options.check()?;
            let rows=view.query(&format!("SELECT {columns} FROM {table} WHERE {key}>?1 ORDER BY {key} LIMIT 64"),vec![Param::Bytes(after.clone())])?;
            if rows.is_empty(){break;}
            for row in rows{
                after=row.get(0)?;
                let result=(||->Result<(),String>{
                    match kind{
                        0=>{let s=crate::history::diagnostic_stack(&row).map_err(|e|e.to_string())?;scratch.0.execute("INSERT INTO stacks(id,scope,profile,head) VALUES(?1,?2,?3,?4)",params![s.id.as_slice(),s.scope.as_bytes().as_slice(),s.profile.as_bytes().as_slice(),s.head_layer.as_slice()]).map_err(|_|"diagnostic scratch".to_owned())?;},
                        3=>{let b=crate::history::diagnostic_branch(&row).map_err(|e|e.to_string())?;scratch.0.execute("INSERT INTO branches(id,stack,base,head,fork) VALUES(?1,?2,?3,?4,?5)",params![b.id.as_slice(),b.stack.as_slice(),b.base_layer.as_slice(),b.head_commit.map(|v|v.as_slice().to_vec()),b.forked_from.map(|v|v.as_slice().to_vec())]).map_err(|_|"diagnostic scratch".to_owned())?;},
                        2=>{let l=crate::history::diagnostic_layer(&row).map_err(|e|e.to_string())?;if LayerId::derive(l.stack,l.parent,l.root)!=l.id{return Err("Layer identity mismatch".to_owned());}scratch.0.execute("INSERT INTO nodes(id,kind,stack,root,parent,source,source_branch,overwrote) VALUES(?1,2,?2,?3,?4,?5,?6,?7)",params![l.id.as_slice(),l.stack.as_slice(),l.root.as_bytes().as_slice(),l.parent.map(|v|v.as_slice().to_vec()),l.source_commit.map(|v|v.as_slice().to_vec()),l.source_branch.map(|v|v.as_slice().to_vec()),i64::try_from(l.overwrote_count).map_err(|_|"Layer overwrite count".to_owned())?]).map_err(|_|"diagnostic scratch".to_owned())?;},
                        _=>{let c=crate::history::diagnostic_commit(&row).map_err(|e|e.to_string())?;scratch.0.execute("INSERT INTO nodes(id,kind,stack,root,parent,base,second) VALUES(?1,1,?2,?3,?4,?5,?6)",params![c.id.as_slice(),c.stack.as_slice(),c.root.as_bytes().as_slice(),c.parent.map(|v|v.as_slice().to_vec()),c.base_layer.as_slice(),c.second_parent.map(|v|v.as_slice().to_vec())]).map_err(|_|"diagnostic scratch".to_owned())?;
                            for edge in [c.meta.merged_from,c.meta.displaced.map(layerfs_history::NodeId::Commit)].into_iter().flatten(){scratch.0.execute("INSERT OR IGNORE INTO edges VALUES(?1,?2)",params![c.id.as_slice(),edge.as_slice()]).map_err(|_|"diagnostic scratch".to_owned())?;}
                        }
                    }Ok(())
                })();
                if let Err(message)=result{*failures+=1;emit(observer,"store.history.record",Outcome::Fail,format!("{table} {}: {message}",hex(&after)));}
            }
        }
    }
    scratch.0.execute_batch("INSERT OR IGNORE INTO edges SELECT id,parent FROM nodes WHERE parent IS NOT NULL; INSERT OR IGNORE INTO edges SELECT id,base FROM nodes WHERE base IS NOT NULL; INSERT OR IGNORE INTO edges SELECT id,second FROM nodes WHERE second IS NOT NULL; INSERT OR IGNORE INTO edges SELECT id,source FROM nodes WHERE source IS NOT NULL; INSERT OR IGNORE INTO contexts SELECT n.root,s.scope,s.profile FROM nodes n JOIN stacks s ON s.id=n.stack;").map_err(sql)?;
    Ok(())
}

fn qualify(
    scratch: &mut Scratch,
    reader: &Reader<'_>,
    options: Options,
    observer: &mut dyn FnMut(Observation),
) -> Result<(), BackendError> {
    let (mut after, mut checked, mut failed) = (Vec::<u8>::new(), 0u64, 0u64);
    // A compound encoded continuation avoids deduplication by root alone.
    loop {
        options.check()?;
        let contexts = {
            let mut stmt=scratch.0.prepare("SELECT root,scope,profile FROM contexts WHERE CAST(root||scope||profile AS BLOB)>?1 ORDER BY CAST(root||scope||profile AS BLOB) LIMIT 32").map_err(sql)?;
            let rows = stmt
                .query_map([after.as_slice()], |r| {
                    Ok((
                        r.get::<_, Vec<u8>>(0)?,
                        r.get::<_, Vec<u8>>(1)?,
                        r.get::<_, Vec<u8>>(2)?,
                    ))
                })
                .map_err(sql)?;
            rows.collect::<Result<Vec<_>, _>>().map_err(sql)?
        };
        if contexts.is_empty() {
            break;
        }
        for (root, scope, profile) in contexts {
            options.check()?;
            after = [root.as_slice(), scope.as_slice(), profile.as_slice()].concat();
            checked += 1;
            scratch.clear().map_err(|_| BackendError::Capacity)?;
            let result = (|| {
                let context = RootContext {
                    root: FilesystemRootId(ObjectId::from_bytes(&root)?),
                    scope: InodeScope::from_object(ObjectId::from_bytes(&scope)?),
                    profile: ObjectId::from_bytes(&profile)?,
                    root_serial: None,
                };
                qualify_root(reader, scratch, &context, &mut QualificationWork::default())
            })();
            options.check()?;
            if let Err(error) = result {
                failed += 1;
                emit(
                    observer,
                    "store.namespace",
                    Outcome::Fail,
                    format!(
                        "root {} scope {} profile {}: {error}",
                        hex(&root),
                        hex(&scope),
                        hex(&profile)
                    ),
                );
            }
        }
    }
    emit(
        observer,
        "store.namespace.summary",
        if failed == 0 {
            Outcome::Pass
        } else {
            Outcome::Fail
        },
        format!("{checked} historical root/scope/profile contexts qualified; {failed} failed"),
    );
    Ok(())
}
fn sql(_: rusqlite::Error) -> BackendError {
    BackendError::Capacity
}
fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 15) as usize] as char);
    }
    out
}
