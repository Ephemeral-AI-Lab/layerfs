//! C5 publication custody and strict compound storage enforcement, without corpus work.
use fs_bench_storage_content::{
    gates::Status,
    ops::history_retained::{self, RetainedHistory},
    workload::history::Row,
};
use layerfs_content::{filesystem::scope_for_seed, ObjectId};
use std::path::PathBuf;

#[test]
fn compound_canonical_pins_never_accept_missing_or_drifted_counters() {
    for (row, bytes, objects) in [
        (Row::Stride10, 380_921_328, 52_032),
        (Row::Stride3, 589_423_458, 73_476),
        (Row::Stride1, 871_588_115, 104_705),
    ] {
        assert!(history_retained::counter_gates(row, &[])
            .iter()
            .all(|gate| gate.status == Status::Incomplete));
        let mut values = vec![
            ("history.canonical_bytes".into(), bytes),
            ("history.canonical_objects".into(), objects),
        ];
        assert!(history_retained::counter_gates(row, &values)
            .iter()
            .all(|gate| gate.status == Status::Pass));
        values[0].1 += 1;
        assert!(history_retained::counter_gates(row, &values)
            .iter()
            .any(|gate| gate.status == Status::Fail));
    }
}

#[test]
fn retained_states_reopen_and_allocation_never_passes_missing_or_shared_owners() {
    let scratch = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/retained-checks");
    std::fs::create_dir_all(&scratch).unwrap();
    let directory: PathBuf = scratch.join(format!(
        "layerfs-retained-check-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&directory).unwrap();
    let scope = scope_for_seed([3; 32]);
    let roots: Vec<_> = (0..17).map(|n| ObjectId::for_bytes(&[n])).collect();
    let path = directory.join("history.sqlite");
    let mut catalog = RetainedHistory::create(&path, scope, roots[0]).unwrap();
    for (index, root) in roots.iter().copied().enumerate().skip(1) {
        assert!(catalog.publish(index + 1, root).unwrap().commit.is_some());
    }
    drop(catalog);
    assert_eq!(history_retained::verify(&path, scope, &roots).unwrap(), 17);
    let mut bad = roots.clone();
    bad[4] = ObjectId::for_bytes(b"wrong independent root");
    assert!(history_retained::verify(&path, scope, &bad).is_err());
    assert_eq!(
        history_retained::storage_gate(&directory, Row::Stride10).status,
        Status::Incomplete
    );
    let store = directory.join("sample.sqlite");
    std::fs::write(&store, b"allocation gate checks actual blocks").unwrap();
    assert_eq!(
        history_retained::storage_gate(&directory, Row::Stride10).status,
        Status::Pass
    );
    let alias = directory.join("shared");
    std::fs::hard_link(&store, &alias).unwrap();
    assert_eq!(
        history_retained::storage_gate(&directory, Row::Stride10).status,
        Status::Ineligible
    );
    std::fs::remove_file(alias).unwrap();
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(store)
        .unwrap();
    for _ in 0..49 {
        file.write_all(&vec![7; 1024 * 1024]).unwrap();
    }
    drop(file);
    assert_eq!(
        history_retained::storage_gate(&directory, Row::Stride10).status,
        Status::Fail
    );
    std::fs::remove_dir_all(directory).unwrap();
}
