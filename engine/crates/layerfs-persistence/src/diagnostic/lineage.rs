//! Independent ancestry facts; saved run labels are validated, never invented.
use super::{emit, scratch::Scratch, transaction::View, Observation, Options, Outcome};
use crate::backend::records::{BackendError, Param};
use rusqlite::{params, OptionalExtension};

pub(super) fn run(
    view: &dyn View,
    scratch: &mut Scratch,
    options: Options,
    observer: &mut dyn FnMut(Observation),
) -> Result<(), BackendError> {
    let db = &scratch.0;
    db.execute_batch("CREATE TABLE saved_commit(id BLOB PRIMARY KEY,run BLOB NOT NULL,depth INTEGER NOT NULL,newest INTEGER NOT NULL,merged BLOB) WITHOUT ROWID; CREATE TABLE saved_layer(id BLOB PRIMARY KEY,stack BLOB NOT NULL,height INTEGER NOT NULL,overwrote INTEGER NOT NULL) WITHOUT ROWID; CREATE TABLE saved_absorbed(run BLOB NOT NULL,depth INTEGER NOT NULL,height INTEGER NOT NULL,PRIMARY KEY(run,depth)) WITHOUT ROWID; CREATE TABLE saved_reach(k BLOB NOT NULL,x INTEGER NOT NULL,height INTEGER NOT NULL,PRIMARY KEY(k,x)) WITHOUT ROWID; CREATE TABLE absorption(id BLOB PRIMARY KEY,height INTEGER NOT NULL) WITHOUT ROWID; CREATE TABLE expected_reach(k BLOB NOT NULL,x INTEGER NOT NULL,height INTEGER NOT NULL,PRIMARY KEY(k,x)) WITHOUT ROWID; CREATE TABLE local_edges(child BLOB NOT NULL,parent BLOB NOT NULL,PRIMARY KEY(child,parent)) WITHOUT ROWID; INSERT INTO local_edges SELECT id,parent FROM nodes WHERE kind=1 AND parent IS NOT NULL; INSERT OR IGNORE INTO local_edges SELECT c.id,c.second FROM nodes c JOIN nodes p ON p.id=c.second WHERE c.kind=1 AND p.kind=1;").map_err(sql)?;
    copy(view, db, options)?;
    // Compute immutable facts in indexed batches. A run partition is not used
    // in these equations: depth and newest come only from actual graph edges.
    loop {
        options.check()?;
        let ids = {
            let mut stmt=db.prepare("SELECT n.id FROM nodes n WHERE height IS NULL AND depth IS NULL AND ((kind=2 AND (parent IS NULL OR EXISTS(SELECT 1 FROM nodes p WHERE p.id=n.parent AND p.height IS NOT NULL))) OR (kind=1 AND (CASE WHEN parent IS NULL THEN EXISTS(SELECT 1 FROM nodes b WHERE b.id=n.base AND b.height IS NOT NULL) ELSE EXISTS(SELECT 1 FROM nodes p WHERE p.id=n.parent AND p.depth IS NOT NULL) END) AND (second IS NULL OR EXISTS(SELECT 1 FROM nodes s WHERE s.id=n.second AND (s.depth IS NOT NULL OR s.height IS NOT NULL))))) ORDER BY id LIMIT 64").map_err(sql)?;
            let rows = stmt
                .query_map([], |r| r.get::<_, Vec<u8>>(0))
                .map_err(sql)?;
            rows.collect::<Result<Vec<_>, _>>().map_err(sql)?
        };
        if ids.is_empty() {
            break;
        }
        for id in ids {
            db.execute("UPDATE nodes SET height=CASE WHEN kind=2 THEN COALESCE((SELECT p.height+1 FROM nodes p WHERE p.id=nodes.parent),0) END,depth=CASE WHEN kind=1 THEN COALESCE((SELECT p.depth+1 FROM nodes p WHERE p.id=nodes.parent),1) END,newest=CASE WHEN kind=1 THEN max(CASE WHEN parent IS NULL THEN (SELECT b.height FROM nodes b WHERE b.id=nodes.base) ELSE (SELECT p.newest FROM nodes p WHERE p.id=nodes.parent) END,COALESCE((SELECT COALESCE(s.newest,s.height) FROM nodes s WHERE s.id=nodes.second),0)) END WHERE id=?1",[id]).map_err(sql)?;
        }
    }
    let mut failures = 0u64;
    for(query,message)in[
        ("SELECT n.id FROM nodes n LEFT JOIN saved_layer s ON s.id=n.id WHERE n.kind=2 AND (s.id IS NULL OR s.stack!=n.stack OR n.height IS NULL OR s.height!=n.height OR s.overwrote<0 OR s.overwrote>n.height)","Layer height/stack/overwrite bounds disagree with authoritative graph"),
        ("SELECT s.id FROM saved_layer s LEFT JOIN nodes n ON n.id=s.id WHERE n.id IS NULL OR n.kind!=2","orphan Layer lineage row"),
        ("SELECT n.id FROM nodes n LEFT JOIN saved_commit s ON s.id=n.id WHERE n.kind=1 AND (s.id IS NULL OR n.depth IS NULL OR n.depth!=s.depth OR n.newest!=s.newest OR NOT (s.merged IS CASE WHEN substr(n.second,1,1)=X'12' THEN n.second ELSE NULL END))","Commit depth/newest/merge disagree with authoritative graph"),
        ("SELECT s.id FROM saved_commit s LEFT JOIN nodes n ON n.id=s.id WHERE n.id IS NULL OR n.kind!=1","orphan Commit lineage row"),
        ("SELECT s.id FROM saved_commit s JOIN nodes n ON n.id=s.id LEFT JOIN nodes a ON a.id=s.run LEFT JOIN saved_commit anchor ON anchor.id=s.run WHERE a.id IS NULL OR a.kind!=1 OR a.stack!=n.stack OR anchor.id IS NULL OR anchor.run!=anchor.id OR (s.id!=s.run AND NOT EXISTS(SELECT 1 FROM saved_commit p WHERE p.id=n.parent AND p.run=s.run AND p.depth=s.depth-1))","invalid run anchor or discontinuous first-parent continuation"),
        ("SELECT min(id) FROM saved_commit GROUP BY run,depth HAVING count(*)!=1","multiple Commits occupy the same run depth"),
    ]{
        let mut stmt=db.prepare(query).map_err(sql)?;let mut rows=stmt.query([]).map_err(sql)?;
        while let Some(row)=rows.next().map_err(sql)?{options.check()?;failures+=1;let id:Vec<u8>=row.get(0).map_err(sql)?;emit(observer,"store.history.lineage",Outcome::Fail,format!("{}: {message}",hex(&id)));}
    }
    if failures != 0 {
        emit(observer,"store.history.lineage.coverage",Outcome::Unknown,"run labels failed validation; absorption/reach comparison blocked by invalid lineage prerequisites");
        return Ok(());
    }
    emit(observer,"store.history.lineage.graph",Outcome::Pass,"immutable Layer heights and Commit depths/newest/merge edges independently checked; publication reconstruction pending");
    db.execute(
        "CREATE INDEX diagnostic_layer_heights ON nodes(stack,height) WHERE kind=2",
        [],
    )
    .map_err(sql)?;
    // Validated run labels only compress graph-proven facts below; no ordinary
    // History ancestry API or saved absorption/reach answer is consulted.
    let mut last_height = -1i64;
    let mut last_stack = Vec::new();
    loop {
        options.check()?;
        let rows = {
            let mut stmt=db.prepare("SELECT n.id,n.stack,n.source,n.height FROM nodes n WHERE n.kind=2 AND n.source IS NOT NULL AND (n.stack>?1 OR (n.stack=?1 AND n.height>?2)) ORDER BY n.stack,n.height LIMIT 32").map_err(sql)?;
            let rows = stmt
                .query_map(params![last_stack, last_height], |r| {
                    Ok((
                        r.get::<_, Vec<u8>>(0)?,
                        r.get::<_, Vec<u8>>(1)?,
                        r.get::<_, Vec<u8>>(2)?,
                        r.get::<_, i64>(3)?,
                    ))
                })
                .map_err(sql)?;
            rows.collect::<Result<Vec<_>, _>>().map_err(sql)?
        };
        if rows.is_empty() {
            break;
        }
        for (id, stack, source, height) in rows {
            options.check()?;
            last_stack = stack.clone();
            last_height = height;
            // Evaluate BEFORE this publication changes absorption. A Commit
            // contains its local Commit ancestors and all ancestry through its
            // newest Layer. A previous publication is covered when its source
            // is local, or was already absorbed by that newest Layer. This
            // graph equation deliberately avoids saved reach/run answers.
            let (covered, saved): (i64, i64) = db.query_row(
                "WITH RECURSIVE ancestors(id) AS (SELECT ?1 UNION SELECT e.parent FROM local_edges e JOIN ancestors a ON e.child=a.id) SELECT min(?3-1,max(c.newest,COALESCE((SELECT max(p.height) FROM nodes p WHERE p.kind=2 AND p.stack=?2 AND p.height<?3 AND (p.source IN (SELECT id FROM ancestors) OR EXISTS(SELECT 1 FROM absorption a WHERE a.id=p.source AND a.height<=c.newest))),0))),s.overwrote FROM nodes c JOIN saved_layer s ON s.id=?4 WHERE c.id=?1",
                params![source,stack,height,id],
                |r| Ok((r.get(0)?,r.get(1)?)),
            ).map_err(sql)?;
            let expected = height - 1 - covered;
            if saved != expected {
                failures += 1;
                emit(observer,"store.history.overwrite",Outcome::Fail,format!("{}: Layer overwrite count {saved} differs from authoritative ancestry count {expected}",hex(&id)));
            }
            let before: Option<i64> = db
                .query_row(
                    "SELECT height FROM absorption WHERE id=?1",
                    [&source],
                    |r| r.get(0),
                )
                .optional()
                .map_err(sql)?;
            db.execute("WITH RECURSIVE ancestors(id) AS (SELECT ?1 UNION SELECT e.parent FROM local_edges e JOIN ancestors a ON e.child=a.id) INSERT OR IGNORE INTO absorption(id,height) SELECT id,?2 FROM ancestors",params![source,height]).map_err(sql)?;
            let (run, depth): (Vec<u8>, i64) = db
                .query_row(
                    "SELECT run,depth FROM saved_commit WHERE id=?1",
                    [&source],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .map_err(sql)?;
            insert_frontier(db, &run, depth, height)?;
            if let Some(before) = before {
                insert_frontier(db, &stack, before, height)?;
            }
        }
    }
    db.execute_batch("CREATE TABLE expected_absorbed AS SELECT c.run,max(c.depth) AS depth,a.height FROM absorption a JOIN saved_commit c ON c.id=a.id GROUP BY c.run,a.height;").map_err(sql)?;
    for (table, columns, label) in [
        ("absorbed", "run,depth,height", "absorption"),
        ("reach", "k,x,height", "publication reach frontier"),
    ] {
        let query = format!(
            "SELECT {columns} FROM expected_{table} EXCEPT SELECT {columns} FROM saved_{table}"
        );
        let reverse = format!(
            "SELECT {columns} FROM saved_{table} EXCEPT SELECT {columns} FROM expected_{table}"
        );
        for query in [query, reverse] {
            let mut stmt = db.prepare(&query).map_err(sql)?;
            let mut rows = stmt.query([]).map_err(sql)?;
            while rows.next().map_err(sql)?.is_some() {
                options.check()?;
                failures += 1;
                emit(
                    observer,
                    "store.history.lineage",
                    Outcome::Fail,
                    format!("{label} differs from independent authoritative-graph reconstruction"),
                );
            }
        }
    }
    emit(observer,"store.history.lineage.summary",if failures==0{Outcome::Pass}else{Outcome::Fail},format!("Layer heights/overwrite counts, Commit depths/newest, permitted run partitions, absorption and reach independently checked; {failures} findings"));
    Ok(())
}

fn insert_frontier(
    db: &rusqlite::Connection,
    key: &[u8],
    x: i64,
    height: i64,
) -> Result<(), BackendError> {
    db.execute(
        "DELETE FROM expected_reach WHERE k=?1 AND x>=?2",
        params![key, x],
    )
    .map_err(sql)?;
    db.execute(
        "INSERT INTO expected_reach(k,x,height) VALUES(?1,?2,?3)",
        params![key, x, height],
    )
    .map_err(sql)?;
    Ok(())
}
fn copy(view: &dyn View, db: &rusqlite::Connection, options: Options) -> Result<(), BackendError> {
    for (table, key, columns, destination) in [
        (
            "commit_lineage",
            "commit_id",
            "commit_id,run,depth,newest,merged",
            "saved_commit",
        ),
        (
            "layer_lineage",
            "layer_id",
            "layer_id,layer_stack_id,height,overwrote_count",
            "saved_layer",
        ),
    ] {
        let mut after = Vec::new();
        loop {
            options.check()?;
            let rows = view.query(
                &format!("SELECT {columns} FROM {table} WHERE {key}>?1 ORDER BY {key} LIMIT 64"),
                vec![Param::Bytes(after.clone())],
            )?;
            if rows.is_empty() {
                break;
            }
            for row in rows {
                after = row.get(0)?;
                if destination == "saved_commit" {
                    db.execute(
                        "INSERT INTO saved_commit VALUES(?1,?2,?3,?4,?5)",
                        params![
                            after,
                            row.get::<Vec<u8>>(1)?,
                            row.get::<i64>(2)?,
                            row.get::<i64>(3)?,
                            row.get::<Option<Vec<u8>>>(4)?
                        ],
                    )
                    .map_err(sql)?;
                } else {
                    db.execute(
                        "INSERT INTO saved_layer VALUES(?1,?2,?3,?4)",
                        params![
                            after,
                            row.get::<Vec<u8>>(1)?,
                            row.get::<i64>(2)?,
                            row.get::<i64>(3)?
                        ],
                    )
                    .map_err(sql)?;
                }
            }
        }
    }
    for (table, key, number, columns, destination) in [
        (
            "absorbed",
            "run",
            "depth",
            "run,depth,height",
            "saved_absorbed",
        ),
        ("reach", "key", "x", "key,x,height", "saved_reach"),
    ] {
        let (mut after, mut ordinal) = (Vec::new(), -1i64);
        loop {
            options.check()?;
            let rows=view.query(&format!("SELECT {columns} FROM {table} WHERE {key}>?1 OR ({key}=?1 AND {number}>?2) ORDER BY {key},{number} LIMIT 64"),vec![Param::Bytes(after.clone()),Param::I64(ordinal)])?;
            if rows.is_empty() {
                break;
            }
            for row in rows {
                after = row.get(0)?;
                ordinal = row.get(1)?;
                db.execute(
                    &format!("INSERT INTO {destination} VALUES(?1,?2,?3)"),
                    params![after, ordinal, row.get::<i64>(2)?],
                )
                .map_err(sql)?;
            }
        }
    }
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
