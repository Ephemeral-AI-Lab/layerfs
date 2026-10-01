mod cohort;
mod cohort_catalog;
mod cohort_fs;
mod engine;
mod extents;
mod generation;
mod namespace;
use engine::{Engine, Metrics, Result, LIVE};
use std::{env, fs, path::Path, time::Instant};

fn prepare(case: &str, path: &Path) -> Result<()> {
    if path.exists() {
        return Err("refuse to overwrite fixture".into());
    }
    let mut e = Engine::open(path)?;
    e.db.execute_batch(include_str!("../schema.sql"))?;
    let count = if let Some(s) = case.strip_prefix("overlap-") {
        s.parse::<i64>()?
    } else {
        1
    };
    for w in 1..=count {
        e.exec("INSERT INTO workspace(w,active) VALUES(?1,1)", &[&w])?;
        e.begin()?;
        if let Some(n) = case.strip_prefix("namespace-") {
            let n = n.parse::<i64>()?;
            e.inode(w, 1, 0, 0)?;
            for i in 0..n {
                let ino = i + 2;
                e.inode(w, ino, i, 0)?;
                e.exec(
                    "INSERT INTO names VALUES(?1,1,?2,1,9223372036854775807,?3)",
                    &[&w, &namespace::name('f', i), &ino],
                )?;
            }
        } else if case == "deep-270" {
            for i in 0..=270 {
                e.inode(w, i + 1, i, 0)?;
            }
            for i in 0..270 {
                e.exec(
                    "INSERT INTO names VALUES(?1,?2,?3,1,9223372036854775807,?4)",
                    &[&w, &(i + 1), &namespace::name('d', i), &(i + 2)],
                )?;
            }
        } else if case.starts_with("extent-") {
            let fragmented = case == "extent-fragmented-8192" || case == "extent-truncate-regrow";
            let size = if fragmented {
                65536
            } else if case == "extent-dispersed-512" {
                32768
            } else {
                16384
            };
            e.inode(w, 1, 0, size)?;
            if fragmented {
                for i in 0..8192 {
                    e.exec(
                        "INSERT INTO extents VALUES(?1,1,?2,1,9223372036854775807,?3,?4,0)",
                        &[&w, &(i * 8), &(i * 8 + 8), &(i % 2 + 1)],
                    )?;
                }
            } else {
                e.exec(
                    "INSERT INTO extents VALUES(?1,1,0,1,9223372036854775807,?2,0,0)",
                    &[&w, &size],
                )?;
            }
        } else if case.starts_with("generation-") || case.starts_with("overlap-") {
            for i in 1..=10000 {
                e.inode(w, i, i, 0)?;
            }
        } else if !case.starts_with("lifecycle-") {
            return Err("unknown case".into());
        }
        e.commit()?;
    }
    let version: String =
        e.db.query_row("SELECT sqlite_version()", [], |r| r.get(0))?;
    e.db.close().map_err(|(_, err)| err)?;
    println!("{{\"prepared\":true,\"sqlite_version\":\"{version}\"}}");
    Ok(())
}
fn run(case: &str, path: &Path, output: &Path) -> Result<()> {
    let (mut e, open_ns) = if case.starts_with("lifecycle-") {
        (None, 0)
    } else {
        let t = Instant::now();
        let mut e = Engine::open(path)?;
        e.reset();
        (Some(e), t.elapsed().as_nanos())
    };
    engine::memory(true)?;
    let t = Instant::now();
    let mut combined = Metrics::default();
    let mut overlap_completed = 0;
    if case.starts_with("lifecycle-") {
        for _ in 0..128 {
            if e.is_none() {
                e = Some(Engine::open(path)?);
            }
            let db = e.as_mut().unwrap();
            if db.scalar("PRAGMA user_version", &[])? != 1 {
                return Err("schema version".into());
            }
            db.begin()?;
            db.exec("UPDATE workspace SET counter=counter+1 WHERE w=1", &[])?;
            db.commit()?;
            if case == "lifecycle-reopen-128" {
                let db = e.take().unwrap();
                combined.absorb(&db.m);
                db.db.close().map_err(|(_, err)| err)?;
            }
        }
        if let Some(db) = e.take() {
            combined.absorb(&db.m);
            db.db.close().map_err(|(_, err)| err)?;
        }
    } else if let Some(n) = case.strip_prefix("namespace-") {
        namespace::run(e.as_mut().unwrap(), n.parse()?)?;
    } else if case == "deep-270" {
        namespace::deep(e.as_mut().unwrap())?;
    } else if case.starts_with("extent-") {
        extents::run(e.as_mut().unwrap(), case)?;
    } else if case.starts_with("generation-") {
        generation::generations(e.as_mut().unwrap(), &output.join("selected.tsv"))?;
    } else if let Some(n) = case.strip_prefix("overlap-") {
        let (db, completed) =
            generation::overlap(e.take().unwrap(), n.parse()?, &output.join("selected.tsv"))?;
        e = Some(db);
        overlap_completed = completed;
    } else {
        return Err("unknown case".into());
    }
    let operation_ns = t.elapsed().as_nanos();
    let (engine_current, engine_high) = engine::memory(false)?;
    // All-zero counters while the engine has performed work are unavailable,
    // including native builds with DEFAULT_MEMSTATUS=0. Never report zero use.
    let memory_status = if engine_current == 0 && engine_high == 0 {
        "UNAVAILABLE"
    } else {
        "OBSERVED_ENGINE_ONLY"
    };
    let engine_current = if memory_status == "UNAVAILABLE" {
        "null".to_owned()
    } else {
        engine_current.to_string()
    };
    let engine_high = if memory_status == "UNAVAILABLE" {
        "null".to_owned()
    } else {
        engine_high.to_string()
    };
    let close = Instant::now();
    if let Some(db) = e.take() {
        combined.absorb(&db.m);
        db.db.close().map_err(|(_, err)| err)?;
    }
    let close_ns = close.elapsed().as_nanos();
    // All observations below occur after the operation timer and closed owner.
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
        let value: i64 = observed
            .db
            .query_row(&format!("PRAGMA {p}"), [], |r| r.get(0))?;
        profile.push(format!("\"{p}\":{value}"));
    }
    let journal: String = observed
        .db
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))?;
    let inode_rows: i64 = observed
        .db
        .query_row("SELECT COUNT(*) FROM inodes", [], |r| r.get(0))?;
    let obsolete: i64 =
        observed
            .db
            .query_row("SELECT COUNT(*) FROM inodes WHERE dead<?1", [LIVE], |r| {
                r.get(0)
            })?;
    let extent_rows: i64 = observed
        .db
        .query_row("SELECT COUNT(*) FROM extents", [], |r| r.get(0))?;
    let captures: i64 = observed
        .db
        .query_row("SELECT COUNT(*) FROM captures", [], |r| r.get(0))?;
    observed.db.close().map_err(|(_, err)| err)?;
    let db_bytes = fs::metadata(path)?.len();
    let receipt=format!("{{\"schema\":2,\"case\":\"{case}\",\"operation_ns\":{operation_ns},\"open_setup_ns\":{open_ns},\"close_ns\":{close_ns},\"metrics\":{},\"sqlite_version\":\"{version}\",\"profile\":{{\"journal_mode\":\"{journal}\",{}}},\"sqlite_memory_status\":\"{memory_status}\",\"sqlite_memory_current_bytes\":{engine_current},\"sqlite_memory_operation_high_bytes\":{engine_high},\"db_bytes\":{db_bytes},\"inode_rows\":{inode_rows},\"obsolete_inode_rows\":{obsolete},\"extent_rows\":{extent_rows},\"capture_rows\":{captures},\"coordinated_mutators_completed\":{overlap_completed},\"row_window_status\":\"{}\",\"transaction_row_status\":\"{}\",\"cache_verdict\":\"INELIGIBLE\",\"performance_claim\":false,\"admission_eligible\":false}}\n",combined.json(),profile.join(","),if combined.page_fields_bytes<=16384 {"PASS"}else{"FAIL"},if combined.tx_max_changed<=512 {"PASS"}else{"FAIL"});
    fs::write(output.join("operation.json"), &receipt)?;
    print!("{receipt}");
    Ok(())
}
fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 4 {
        return Err("usage: prepare CASE DATABASE | run CASE DATABASE OUTPUT".into());
    }
    match args[1].as_str() {
        "prepare" => prepare(&args[2], Path::new(&args[3])),
        "prepare-cohort" => cohort::prepare(&args[2], Path::new(&args[3])),
        "run-cohort" => {
            if args.len() != 5 {
                return Err("run-cohort requires OUTPUT".into());
            }
            cohort::run(&args[2], Path::new(&args[3]), Path::new(&args[4]))
        }
        "inspect-memory" => {
            let db = Engine::open(Path::new(&args[3]))?;
            let (current, high) = engine::memory(false)?;
            let unavailable = current == 0 && high == 0;
            println!(
                "{{\"schema\":2,\"memory_status\":\"{}\",\"current_bytes\":{},\"high_bytes\":{}}}",
                if unavailable {
                    "UNAVAILABLE"
                } else {
                    "OBSERVED_ENGINE_ONLY"
                },
                if unavailable {
                    "null".to_owned()
                } else {
                    current.to_string()
                },
                if unavailable {
                    "null".to_owned()
                } else {
                    high.to_string()
                }
            );
            db.db.close().map_err(|(_, err)| err)?;
            Ok(())
        }
        "run" => {
            if args.len() != 5 {
                return Err("run requires OUTPUT".into());
            }
            run(&args[2], Path::new(&args[3]), Path::new(&args[4]))
        }
        _ => Err("unknown action".into()),
    }
}
