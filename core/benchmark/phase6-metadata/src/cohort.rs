use crate::engine::{Engine, Result, LIVE};
use std::{fs, path::Path, time::Instant};
const SIZE: i64 = 10 * 1024 * 1024;

pub fn prepare(case: &str, path: &Path) -> Result<()> {
    if path.exists() {
        return Err("refuse to overwrite cohort fixture".into());
    }
    let mut e = Engine::open(path)?;
    e.db.execute_batch(include_str!("../schema.sql"))?;
    e.db.execute_batch(include_str!("../cohort.sql"))?;
    e.track_changes = true;
    e.exec("INSERT INTO workspace(w,active) VALUES(1,1)", &[])?;
    e.begin()?;
    e.exec("INSERT INTO kinds VALUES(100,1)", &[])?;
    e.inode(1, 100, -1, 0)?;
    e.commit()?;
    if case.contains("retained") || case == "cohort-overwrite-4k" {
        e.begin()?;
        e.cohort_source(0, b"A", true, SIZE)?;
        e.exec("INSERT INTO kinds VALUES(1,2)", &[])?;
        e.inode(1, 1, 0, SIZE)?;
        e.exec(
            "INSERT INTO extents VALUES(1,1,0,1,9223372036854775807,?1,0,0)",
            &[&SIZE],
        )?;
        let name = if case.contains("retained") {
            b"data.bin".as_slice()
        } else {
            b"large.bin".as_slice()
        };
        e.exec(
            "INSERT INTO names VALUES(1,100,?1,1,9223372036854775807,1)",
            &[&name],
        )?;
        e.commit()?;
    } else if case == "cohort-namespace67" {
        for (parent, name, ino) in [
            (100, "packages", 101),
            (101, "old", 102),
            (101, "new", 103),
            (102, "subtree", 104),
            (104, "child", 105),
        ] {
            e.cohort_create(parent, name.as_bytes(), ino, None)?;
        }
        e.cohort_create(105, b"grand.txt", 106, Some(b"grand-base"))?;
        e.cohort_create(104, b"sibling.txt", 107, Some(b"sibling-base"))?;
        for i in 0..64 {
            e.cohort_create(
                105,
                format!("extra{i:03}.txt").as_bytes(),
                108 + i,
                Some(b"x"),
            )?;
        }
        e.cohort_create(100, b".marker", 172, Some(b"baseline"))?;
    } else if !["cohort-components270", "cohort-many128"].contains(&case) {
        return Err("unknown cohort fixture".into());
    }
    e.exec("DELETE FROM changed", &[])?;
    let version: String =
        e.db.query_row("SELECT sqlite_version()", [], |r| r.get(0))?;
    e.db.close().map_err(|(_, err)| err)?;
    println!("{{\"prepared\":true,\"sqlite_version\":\"{version}\",\"schema\":2}}");
    Ok(())
}
fn writes(e: &mut Engine) -> Result<()> {
    for i in 0..4097_i64 {
        let source = 10000 + i;
        e.cohort_source(source, &[b'B' + (i % 24) as u8], false, 1)?;
        let offset = (104729 + i * 2654435761) % SIZE;
        e.overwrite(1, offset, offset + 1, source)?;
    }
    Ok(())
}
fn mutations(e: &mut Engine, case: &str) -> Result<()> {
    match case {
        "cohort-clean-retained-4097" => {}
        "cohort-one-edit-retained-4097" => {
            e.cohort_source(20000, b"X", false, 1)?;
            e.overwrite(1, 0, 1, 20000)?;
        }
        "cohort-overwrite-4k" => {
            e.cohort_source(20000, &[b'P'; 4096], false, 4096)?;
            e.overwrite(1, 5242880, 5246976, 20000)?;
        }
        "cohort-namespace67" => {
            e.cohort_move(102, b"subtree", 103, b"subtree")?;
            e.cohort_move(104, b"child", 104, b"renamed")?;
            e.cohort_move(104, b"renamed", 104, b"child")?;
            e.cohort_move(103, b"subtree", 102, b"subtree")?;
            e.cohort_move(102, b"subtree", 103, b"subtree")?;
            e.cohort_create(105, b"grand.txt.next", 173, Some(b"grand-new"))?;
            e.cohort_move(105, b"grand.txt.next", 105, b"grand.txt")?;
        }
        "cohort-components270" => {
            let mut parent = 100;
            for i in 0..270 {
                let ino = 200 + i;
                e.cohort_create(parent, b"d", ino, None)?;
                parent = ino;
            }
            e.cohort_create(parent, b"file", 470, Some(b"leaf"))?;
        }
        "cohort-many128" => {
            e.cohort_create(100, b"many", 200, None)?;
            for i in 0..128 {
                e.cohort_create(
                    200,
                    format!("f{i}").as_bytes(),
                    201 + i,
                    Some(format!("new-{i}").as_bytes()),
                )?;
            }
        }
        _ => return Err("unknown cohort mutation".into()),
    }
    Ok(())
}
pub fn run(case: &str, path: &Path, out: &Path) -> Result<()> {
    let opening = Instant::now();
    let mut e = Engine::open(path)?;
    e.track_changes = true;
    e.reset();
    let open_setup_ns = opening.elapsed().as_nanos();
    if e.scalar("PRAGMA user_version", &[])? != 2 {
        return Err("cohort schema".into());
    }
    e.reset();
    crate::engine::memory(true)?;
    let entire = Instant::now();
    let (mut prelude_mutation_ns, mut prelude_prepare_ns, mut prelude_complete_ns) = (0, 0, 0);
    let (mut prelude_inodes, mut prelude_rows) = (0, 0);
    let retained = case.contains("retained");
    let mut held_g = None;
    if retained {
        let t = Instant::now();
        writes(&mut e)?;
        prelude_mutation_ns = t.elapsed().as_nanos();
        let t = Instant::now();
        let (g, i, r) = e.cohort_prepare(&out.join("prelude.catalog"))?;
        prelude_prepare_ns = t.elapsed().as_nanos();
        prelude_inodes = i;
        prelude_rows = r;
        let t = Instant::now();
        e.cohort_complete(g)?;
        prelude_complete_ns = t.elapsed().as_nanos();
        e.begin()?;
        e.exec("INSERT INTO pins VALUES(1,1,?1)", &[&g])?;
        e.commit()?;
        held_g = Some(g);
    }
    let t = Instant::now();
    mutations(&mut e, case)?;
    let mutation_ns = t.elapsed().as_nanos();
    let t = Instant::now();
    let (g, dirty_inodes, catalog_rows) = e.cohort_prepare(&out.join("final.catalog"))?;
    let metadata_prepare_ns = t.elapsed().as_nanos();
    let t = Instant::now();
    e.cohort_complete(g)?;
    let metadata_complete_ns = t.elapsed().as_nanos();
    let (mut pin_result_ns, mut pin_rows, mut pin_release_ns) = (0, 0, 0);
    if let Some(selected) = held_g {
        let t = Instant::now();
        pin_rows = e.cohort_pin_result(selected, &out.join("retained.catalog"))?;
        pin_result_ns = t.elapsed().as_nanos();
        let t = Instant::now();
        e.begin()?;
        e.exec("DELETE FROM pins WHERE id=1", &[])?;
        e.commit()?;
        e.cohort_retire()?;
        pin_release_ns = t.elapsed().as_nanos();
    }
    let operation_ns = entire.elapsed().as_nanos();
    let (current, high) = crate::engine::memory(false)?;
    let memory_status = if current == 0 && high == 0 {
        "UNAVAILABLE"
    } else {
        "OBSERVED_ENGINE_ONLY"
    };
    let memory_current = if memory_status == "UNAVAILABLE" {
        "null".to_owned()
    } else {
        current.to_string()
    };
    let memory_high = if memory_status == "UNAVAILABLE" {
        "null".to_owned()
    } else {
        high.to_string()
    };
    let metrics = e.m.json();
    let tx_status = if e.m.tx_max_changed <= 512 {
        "PASS"
    } else {
        "FAIL"
    };
    let cursor_status = if e.m.page_fields_bytes <= 16384 {
        "PASS"
    } else {
        "FAIL"
    };
    let close = Instant::now();
    e.db.close().map_err(|(_, err)| err)?;
    let close_ns = close.elapsed().as_nanos();
    let observed = Engine::open(path)?;
    let version: String = observed
        .db
        .query_row("SELECT sqlite_version()", [], |r| r.get(0))?;
    let mut profile = Vec::new();
    for p in [
        "synchronous",
        "temp_store",
        "foreign_keys",
        "busy_timeout",
        "cache_size",
        "page_size",
        "mmap_size",
    ] {
        let v: i64 = observed
            .db
            .query_row(&format!("PRAGMA {p}"), [], |r| r.get(0))?;
        profile.push(format!("\"{p}\":{v}"));
    }
    let journal: String = observed
        .db
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))?;
    let mut owners = Vec::new();
    for table in [
        "changed",
        "captures",
        "pins",
        "retained_orphans",
        "sources",
        "inodes",
        "extents",
    ] {
        let count: i64 =
            observed
                .db
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))?;
        owners.push(format!("\"{table}\":{count}"));
    }
    let obsolete:i64=observed.db.query_row("SELECT (SELECT COUNT(*) FROM inodes WHERE dead<?1)+(SELECT COUNT(*) FROM extents WHERE dead<?1)+(SELECT COUNT(*) FROM names WHERE dead<?1)",[LIVE],|r|r.get(0))?;
    observed.db.close().map_err(|(_, err)| err)?;
    let db_bytes = fs::metadata(path)?.len();
    let receipt=format!("{{\"schema\":2,\"case\":\"{case}\",\"operation_ns\":{operation_ns},\"open_setup_ns\":{open_setup_ns},\"close_ns\":{close_ns},\"prelude_mutation_ns\":{prelude_mutation_ns},\"prelude_metadata_prepare_ns\":{prelude_prepare_ns},\"prelude_metadata_complete_ns\":{prelude_complete_ns},\"mutation_ns\":{mutation_ns},\"metadata_prepare_ns\":{metadata_prepare_ns},\"metadata_complete_ns\":{metadata_complete_ns},\"pin_result_ns\":{pin_result_ns},\"pin_release_ns\":{pin_release_ns},\"prelude_dirty_inodes\":{prelude_inodes},\"prelude_catalog_rows\":{prelude_rows},\"dirty_inodes\":{dirty_inodes},\"catalog_rows\":{catalog_rows},\"retained_pin_rows\":{pin_rows},\"metrics\":{metrics},\"owners\":{{{}}},\"obsolete_versions\":{obsolete},\"db_bytes\":{db_bytes},\"sqlite_version\":\"{version}\",\"profile\":{{\"journal_mode\":\"{journal}\",{}}},\"memory_status\":\"{memory_status}\",\"engine_current_bytes\":{memory_current},\"engine_high_bytes\":{memory_high},\"transaction_row_status\":\"{tx_status}\",\"cursor_field_status\":\"{cursor_status}\",\"exec_ns\":null,\"full_commit_ns\":null,\"performance_claim\":false,\"cache_verdict\":\"INELIGIBLE\"}}\n",owners.join(","),profile.join(","));
    fs::write(out.join("operation.json"), &receipt)?;
    print!("{receipt}");
    Ok(())
}
