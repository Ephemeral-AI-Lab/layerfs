//! T10: the §11 report wire shape and the `store.status` compaction op
//! projection. The report renders snake_case from the shared `CompactReport`;
//! status reads only the `<store>.compact-op` sidecar and never opens the
//! Store database.
use layerfs_bridge::control::SdkValue;
use layerfs_home::home::{paths, private_dir, write_entity};
use layerfs_home::names::StoreName;
use layerfs_persistence::{
    CompactKeptWaste, CompactPath, CompactReceipt, CompactReport, CompactResidue, CompactZeroReason,
};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "layerfs-t10-report-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn text(value: &SdkValue) -> &str {
    match value {
        SdkValue::Text(text) => text,
        other => panic!("not text: {other:?}"),
    }
}

fn texts(value: &SdkValue) -> Vec<String> {
    let SdkValue::List(items) = value else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| match item {
            SdkValue::Text(text) => Some(text.clone()),
            _ => None,
        })
        .collect()
}

/// A home holding one empty sqlite Store `s` on native.
fn store_home() -> PathBuf {
    let home = tempdir("home");
    let record = SdkValue::map([
        ("name", SdkValue::text("s")),
        ("kind", SdkValue::text("sqlite")),
        ("machine", SdkValue::text("native")),
    ]);
    write_entity(&paths::store_record(&home, "s"), &record).unwrap();
    private_dir(&paths::store_dir(&home, "s")).unwrap();
    std::fs::write(paths::store_file(&home, "s"), b"").unwrap();
    home
}

/// A complete `<store>.compact-op` record, as the persistence layer writes it.
fn sidecar(home: &Path, phase: &str, owner: u32) -> PathBuf {
    let mut path = paths::store_file(home, "s").into_os_string();
    path.push(".compact-op");
    let path = PathBuf::from(path);
    let zeros = "00".repeat(32);
    std::fs::write(
        &path,
        format!(
            "version=1\nop_id=00112233445566778899aabbccddeeff\ngeneration=3\n\
phase={phase}\nstarted_ms=1234\nowner_pid={owner}\nowner_pid_start=\nowner_pid_boot=\n\
catalog_id=0102030405060708090a0b0c0d0e0f10\nnext_history_serial=5\nnext_pack_id=6\n\
next_ordinal=7\nmark_serial=4\nmark_pack_id=6\nmark_digest={zeros}\nmark_totals=1,2,3,4\n\
mark_stamp=9\nhistory_rows=10\nlive_heads_digest={zeros}\n"
        ),
    )
    .unwrap();
    path
}

fn receipt() -> CompactReceipt {
    CompactReceipt {
        path: CompactPath::Rebuild,
        generation_before: 3,
        generation_after: 4,
        dead_bytes_bound: 100,
        dead_bytes_refined: 80,
        logical_discarded: 40,
        backend_reusable: 4096,
        physical_shrink: 8192,
        kept_waste: CompactKeptWaste {
            dead_in_retained_packs_bytes: 5,
            delta_bases_bytes: 6,
            delta_base_references: vec![[7; 32]],
            inode_blocks_bytes: 8,
        },
        peak_temp_bytes: 900,
        residue: CompactResidue {
            bytes: 0,
            artifacts: Vec::new(),
            reason: None,
        },
        estimate: true,
        zero_reasons: vec![
            CompactZeroReason {
                field: "deadBytesBound",
                reason: "no-waste",
            },
            CompactZeroReason {
                field: "logicalDiscarded",
                reason: "no-history",
            },
        ],
    }
}

#[test]
fn the_report_wire_is_the_snake_case_contract_shape() {
    let wire = layerfs_store::report::compact_report(&CompactReport::from(&receipt()));
    assert_eq!(text(wire.get("kind").unwrap()), "Compacted");
    assert_eq!(text(wire.get("path").unwrap()), "rebuild");
    let generation = wire.get("generation").unwrap();
    assert_eq!(generation.get_int("before"), Some(3));
    assert_eq!(generation.get_int("after"), Some(4));
    assert_eq!(wire.get_int("dead_bytes_bound"), Some(100));
    assert_eq!(wire.get_int("dead_bytes_refined"), Some(80));
    assert_eq!(wire.get_int("logical_discarded"), Some(40));
    assert_eq!(wire.get_int("backend_reusable"), Some(4096));
    assert_eq!(wire.get_int("physical_shrink"), Some(8192));
    assert_eq!(wire.get_int("peak_temp_bytes"), Some(900));
    assert_eq!(wire.get("estimate"), Some(&SdkValue::Bool(true)));
    let waste = wire.get("kept_waste").unwrap();
    assert_eq!(waste.get_int("dead_in_retained_packs_bytes"), Some(5));
    assert_eq!(waste.get_int("inode_blocks_bytes"), Some(8));
    let bases = waste.get("delta_bases").unwrap();
    assert_eq!(bases.get_int("bytes"), Some(6));
    assert_eq!(
        texts(bases.get("references").unwrap()),
        vec!["0707070707070707070707070707070707070707070707070707070707070707"]
    );
    let residue = wire.get("residue").unwrap();
    assert_eq!(residue.get_int("bytes"), Some(0));
    assert!(texts(residue.get("artifacts").unwrap()).is_empty());
    assert!(residue.get("reason").is_none());
    // One reason per zero field, named by its snake_case wire key (§10).
    let SdkValue::List(zero) = wire.get("zero_reasons").unwrap() else {
        panic!("zero_reasons is a list");
    };
    let fields: Vec<&str> = zero
        .iter()
        .map(|zero| text(zero.get("field").unwrap()))
        .collect();
    assert_eq!(fields, ["dead_bytes_bound", "logical_discarded"]);
    assert_eq!(
        text(zero[0].get("reason").unwrap()),
        "no-waste",
        "the closed vocabulary passes through"
    );
    assert!(wire.get("zeroReason").is_none(), "no camelCase key");
}

#[test]
fn status_emits_bounded_busy_and_unresolved_metrics() {
    let home = store_home();
    let name = StoreName::parse("s").unwrap();
    let capture = layerfs_persistence::compact_metrics::CompactMetricsCapture::start();
    // Idle: no compaction record, so no `compact.status` sample.
    let _ = layerfs_store::sqlite::family::status(&home, &name).unwrap();
    // A live owner is `busy`; a gone one is `unresolved`.
    sidecar(&home, "build", std::process::id());
    let _ = layerfs_store::sqlite::family::status(&home, &name).unwrap();
    sidecar(&home, "build", 99_999_999);
    let _ = layerfs_store::sqlite::family::status(&home, &name).unwrap();
    let metrics = capture.finish();
    let value = |state: &str| {
        metrics
            .samples()
            .iter()
            .find(|sample| sample.name == "compact.status" && sample.label("state") == Some(state))
            .map_or(0, |sample| sample.value)
    };
    assert_eq!(value("busy"), 1);
    assert_eq!(value("unresolved"), 1);
    assert_eq!(value("idle"), 0, "an idle status is not a compaction op");
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn status_projects_the_sidecar_phase_and_state() {
    let home = store_home();
    let name = StoreName::parse("s").unwrap();

    // No op: the existing idle shape, no phases.
    let idle = layerfs_store::sqlite::family::status(&home, &name).unwrap();
    assert_eq!(idle.get("op"), Some(&SdkValue::Null));
    assert_eq!(idle.get("phases"), Some(&SdkValue::Null));
    assert_eq!(idle.get_text("state"), Some("idle"));

    // A live owner is busy and certain; `build` is the sidecar's phase.
    let op_path = sidecar(&home, "build", std::process::id());
    let busy = layerfs_store::sqlite::family::status(&home, &name).unwrap();
    assert_eq!(busy.get_text("state"), Some("busy"));
    let op = busy.get("op").unwrap();
    assert_eq!(
        text(op.get("id").unwrap()),
        "op-00112233445566778899aabbccddeeff"
    );
    assert_eq!(text(op.get("name").unwrap()), "store.compact");
    assert_eq!(text(op.get("phase").unwrap()), "build");
    assert_eq!(op.get_int("started"), Some(1234));
    // The record remains unsettled while it runs; `state` is what tells a
    // live owner from a leftover.
    assert_eq!(op.get("uncertain"), Some(&SdkValue::Bool(true)));
    assert_eq!(
        texts(busy.get("phases").unwrap()),
        ["mark", "build", "verify", "install"]
    );

    // A gone owner leaves an uncertain op: unresolved, same phase, same id.
    sidecar(&home, "build", 99_999_999);
    let leftover = layerfs_store::sqlite::family::status(&home, &name).unwrap();
    assert_eq!(leftover.get_text("state"), Some("unresolved"));
    let op = leftover.get("op").unwrap();
    assert_eq!(text(op.get("phase").unwrap()), "build");
    assert_eq!(op.get("uncertain"), Some(&SdkValue::Bool(true)));
    assert_eq!(
        text(op.get("id").unwrap()),
        "op-00112233445566778899aabbccddeeff"
    );

    // A settled run removes the record: the idle shape is back.
    std::fs::remove_file(op_path).unwrap();
    let idle = layerfs_store::sqlite::family::status(&home, &name).unwrap();
    assert_eq!(idle.get("op"), Some(&SdkValue::Null));
    assert_eq!(idle.get("phases"), Some(&SdkValue::Null));
    assert_eq!(idle.get_text("state"), Some("idle"));
    std::fs::remove_dir_all(home).unwrap();
}
