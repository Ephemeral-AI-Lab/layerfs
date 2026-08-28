use super::super::*;
use crate::scratch::table::SCRATCH_SERIAL;
use std::sync::atomic::Ordering;

#[test]
fn observation_exposes_hidden_store_inspection_and_sql_families() {
    let anchor = std::env::temp_dir().join(format!(
        "layerfs-scratch-attribution-{}-{}",
        std::process::id(),
        SCRATCH_SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    let engine = crate::Engine::open(&anchor).unwrap();
    let table = DiskTable::create_near(&anchor, "attribution").unwrap();
    let setup = table.observation().unwrap();
    assert_eq!(setup.store_reopens, 1);
    assert_eq!(setup.store_inspection_statements, 11);
    assert_eq!(setup.owner_setup_statements, 15);
    assert_eq!(setup.derived_setup_statements, 2);
    assert_eq!(setup.operation_statements, 0);
    assert_eq!(setup.statements, 17);
    table.put(b"key", b"value").unwrap();
    let operated = table.observation().unwrap();
    assert_eq!(operated.operation_statements, 1);
    assert_eq!(operated.derived_setup_statements, 2);
    assert_eq!(operated.statements, 18);
    let path = table.path.clone();
    let finished = table.finish().unwrap();
    assert_eq!(finished.derived_setup_statements, 3);
    assert_eq!(finished.operation_statements, 1);
    assert_eq!(finished.statements, 19);
    let terminal = finished.checked_delta(operated).unwrap();
    assert_eq!(terminal.tables, 0);
    assert_eq!(terminal.statements, 1);
    assert_eq!(terminal.derived_setup_statements, 1);
    assert_eq!(terminal.operation_statements, 0);
    assert!(!path.exists());
    drop(engine);
    std::fs::remove_file(anchor).unwrap();
}
