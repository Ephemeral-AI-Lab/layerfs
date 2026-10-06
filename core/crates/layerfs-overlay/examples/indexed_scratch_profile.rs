//! One count-driven diagnostic with exact plans and actual scoped SQLite work.
//! Usage: indexed_scratch_profile NEW_DATABASE_PATH [UNRELATED_ROWS].
//! The caller retains the fresh database artifact; this is no timing/cold proof.
use layerfs_overlay::{
    ExpectedValue, IndexedApply, IndexedChange, IndexedKey, IndexedScope, MaintenanceCursor,
    Overlay, ProfileConfig, SCRATCH_BYTES,
};
use std::{error::Error, path::Path};

fn key(kind: u32, number: u64) -> IndexedKey {
    let mut key = [0; 32];
    key[24..].copy_from_slice(&number.to_be_bytes());
    IndexedKey { kind, key }
}
fn put(key: IndexedKey, value: Vec<u8>) -> IndexedChange {
    IndexedChange {
        key,
        expected: ExpectedValue::Missing,
        value: Some(value),
    }
}
fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let path = args.next().ok_or("a fresh database path is required")?;
    let population = args
        .next()
        .map(|s| s.parse::<u64>())
        .transpose()?
        .unwrap_or(1024);
    if args.next().is_some() {
        return Err("usage: indexed_scratch_profile NEW_DATABASE_PATH [UNRELATED_ROWS]".into());
    }
    let db = Overlay::create(Path::new(&path), ProfileConfig::default())?;
    println!(
        "artifact={path:?} population={population} profile={:?}",
        db.profile()
    );
    let route = db.open_workspace([11; 32], [12; 32])?;
    let owner = db.acquire_operation(route, 1)?;
    let scope = IndexedScope {
        owner,
        file_scope: 1,
    };
    let other = IndexedScope {
        file_scope: u64::MAX,
        ..scope
    };
    for start in (0..population).step_by(128) {
        let end = start.saturating_add(128).min(population);
        let changes: Vec<_> = (start..end).map(|n| put(key(1, n), vec![])).collect();
        assert_eq!(
            db.indexed_scratch_apply(other, &changes)?,
            IndexedApply::Applied
        );
    }
    assert_eq!(
        db.indexed_scratch_apply(scope, &[put(key(1, 7), vec![7; 8192])])?,
        IndexedApply::Applied
    );
    let changes: Vec<_> = (0..65).map(|n| put(key(2, n), vec![])).collect();
    assert_eq!(
        db.indexed_scratch_apply(scope, &changes)?,
        IndexedApply::Applied
    );
    for plan in db.explain_indexed_scratch(scope, key(1, 7), key(2, 0).key)? {
        println!("PLAN {plan}");
    }
    let before = db.diagnostics();
    assert!(db.indexed_scratch_contains(scope, key(1, 7))?);
    println!("CONTAINS {:?}", db.diagnostics().since(&before));
    let before = db.diagnostics();
    let value = db
        .indexed_scratch_get(scope, key(1, 7))?
        .ok_or("missing original value")?;
    assert_eq!(value.as_slice(), [7; 8192]);
    println!(
        "GET result_len={} result_capacity={} work={:?}",
        value.len(),
        value.capacity(),
        db.diagnostics().since(&before)
    );
    drop(value);
    let before = db.diagnostics();
    let keys = db.indexed_scratch_first_keys(scope, 2, key(2, 0).key)?;
    assert_eq!(keys.len(), 64);
    println!(
        "KEYS rows={} capacity={} work={:?}",
        keys.len(),
        keys.capacity(),
        db.diagnostics().since(&before)
    );
    drop(keys);
    let before = db.diagnostics();
    assert_eq!(
        db.indexed_scratch_apply(
            scope,
            &[IndexedChange {
                key: key(1, 7),
                expected: ExpectedValue::ExactBytes(vec![7; 8192]),
                value: Some(vec![8; 4096]),
            }]
        )?,
        IndexedApply::Applied
    );
    println!("UPDATE {:?}", db.diagnostics().since(&before));
    println!("RESOURCES_BEFORE_RELEASE {:?}", db.resources(Some(route))?);
    let before = db.diagnostics();
    db.release_operation(owner)?;
    println!("RELEASE {:?}", db.diagnostics().since(&before));
    let mut cursor = MaintenanceCursor::default();
    let mut done = false;
    for turn in 0..population.div_ceil(64).saturating_add(32) {
        let before = db.diagnostics();
        let Some(step) = db.maintain(cursor)? else {
            done = true;
            break;
        };
        assert!(step.work.rows <= 64 && step.work.data_bytes <= SCRATCH_BYTES as u64);
        println!(
            "CLEANUP turn={turn} step={step:?} work={:?}",
            db.diagnostics().since(&before)
        );
        cursor = step.cursor;
    }
    assert!(done, "declared cleanup turn count exhausted");
    println!("RESOURCES_AFTER_RELEASE {:?}", db.resources(Some(route))?);
    println!("LIMITS indexed visited-row counter unavailable; returned rows/bytes and executed VM are observed; logical scratch bytes exclude pager/journal/OS cache residency; no exclusive CPU or timing claim");
    Ok(())
}
