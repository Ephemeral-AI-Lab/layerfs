//! T10: the report `store.compact` answers (call surface 2.0: flat, byte
//! totals named `_bytes`) and what a live read (`store get --with live`)
//! makes of a compaction's own record. The report renders from the
//! `CompactReceipt`; the live read reads only the `<store>.compact-op`
//! sidecar and never opens the Store database.
use layerfs_bridge::control::SdkValue;
use layerfs_content::ObjectId;
use layerfs_home::events::Events;
use layerfs_home::home::{paths, private_dir, write_entity};
use layerfs_persistence::{
    CompactKeptWaste, CompactPath, CompactReceipt, CompactResidue, CompactZeroReason, KeptWaste,
};
use layerfs_store::get::With;
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
    // a home's mark, so its generation counts record writes
    std::fs::write(home.join("layerfs.json"), b"{}").unwrap();
    home
}

/// A complete `<store>.compact-op` record, as the persistence layer writes it.
/// `returned` marks a record whose run has already replied to its caller.
fn sidecar_with(home: &Path, phase: &str, owner: u32, returned: bool) -> PathBuf {
    let mut path = paths::store_file(home, "s").into_os_string();
    path.push(".compact-op");
    let path = PathBuf::from(path);
    let zeros = "00".repeat(32);
    let marker = if returned { "returned=1\n" } else { "" };
    std::fs::write(
        &path,
        format!(
            "version=1\nop_id=00112233445566778899aabbccddeeff\ngeneration=3\n\
phase={phase}\nstarted_ms=1234\nowner_pid={owner}\nowner_pid_start=\nowner_pid_boot=\n\
catalog_id=0102030405060708090a0b0c0d0e0f10\nnext_history_serial=5\nnext_pack_id=6\n\
next_ordinal=7\nmark_serial=4\nmark_pack_id=6\nmark_digest={zeros}\nmark_totals=1,2,3,4\n\
mark_stamp=9\nhistory_rows=10\nlive_heads_digest={zeros}\n{marker}"
        ),
    )
    .unwrap();
    path
}

fn sidecar(home: &Path, phase: &str, owner: u32) -> PathBuf {
    sidecar_with(home, phase, owner, false)
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
            delta_base_references_capped: false,
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

/// `store get <s> --with live` against `home`.
fn live(home: &Path) -> SdkValue {
    let mut sink = |_| {};
    let with = With {
        live: true,
        stats: false,
    };
    layerfs_store::get::get(home, &mut Events::new(&mut sink), "s", with).unwrap()
}

/// `store get <s>` from the records alone.
fn recorded(home: &Path) -> SdkValue {
    let mut sink = |_| {};
    let with = With::default();
    layerfs_store::get::get(home, &mut Events::new(&mut sink), "s", with).unwrap()
}

#[test]
fn a_capped_reference_list_shows_its_first_ids_and_counts_them_all() {
    let mut capped = receipt();
    capped.kept_waste.delta_base_references =
        vec![[9; 32]; layerfs_persistence::DELTA_BASE_REFERENCES_CAP];
    capped.kept_waste.delta_base_references_capped = true;
    let wire = layerfs_store::report::compact_report("s", &capped);
    assert_eq!(
        texts(wire.get("delta_base_references").unwrap()).len(),
        layerfs_store::report::REFERENCES_SHOWN
    );
    assert_eq!(
        wire.get_int("delta_base_references_count"),
        Some(layerfs_persistence::DELTA_BASE_REFERENCES_CAP as i64),
    );
    // The bytes are still the whole kept set's bytes.
    let waste = wire.get("kept_waste").unwrap();
    assert_eq!(waste.get_int("delta_bases_bytes"), Some(6));
}

/// A mark over the delta-base cap keeps the cap's identities: the count is
/// the cap, the answer shows the first ones, and the byte total is the
/// whole kept set's.
#[test]
fn a_mark_over_the_delta_base_cap_counts_the_cap() {
    let references: Vec<ObjectId> = (0..=layerfs_persistence::DELTA_BASE_REFERENCES_CAP)
        .map(|index| {
            let mut bytes = [0u8; 32];
            bytes[..8].copy_from_slice(&(index as u64).to_le_bytes());
            ObjectId::from_bytes(&bytes).unwrap()
        })
        .collect();
    let waste = KeptWaste::from_parts(0, 12, references, 0);
    let mut receipt = receipt();
    receipt.kept_waste = CompactKeptWaste::from_waste(&waste);
    let wire = layerfs_store::report::compact_report("s", &receipt);
    assert_eq!(
        wire.get_int("delta_base_references_count"),
        Some(layerfs_persistence::DELTA_BASE_REFERENCES_CAP as i64),
        "the mark keeps exactly the cap's identities"
    );
    assert_eq!(
        texts(wire.get("delta_base_references").unwrap()).len(),
        layerfs_store::report::REFERENCES_SHOWN
    );
    let waste = wire.get("kept_waste").unwrap();
    assert_eq!(waste.get_int("delta_bases_bytes"), Some(12));
}

#[test]
fn the_report_is_flat_with_byte_totals_named_bytes() {
    let wire = layerfs_store::report::compact_report("s", &receipt());
    assert_eq!(wire.get_int("freed_bytes"), Some(8192));
    assert_eq!(text(wire.get("outcome").unwrap()), "compacted");
    assert_eq!(text(wire.get("store").unwrap()), "s");
    assert_eq!(wire.get_int("generation_before"), Some(3));
    assert_eq!(wire.get_int("generation_after"), Some(4));
    assert_eq!(wire.get_int("dead_bytes"), Some(80));
    assert_eq!(wire.get_int("discarded_bytes"), Some(40));
    assert_eq!(wire.get_int("reusable_bytes"), Some(4096));
    assert_eq!(wire.get_int("peak_temp_bytes"), Some(900));
    assert_eq!(wire.get("estimate"), Some(&SdkValue::Bool(true)));
    let waste = wire.get("kept_waste").unwrap();
    assert_eq!(waste.get_int("dead_in_retained_packs_bytes"), Some(5));
    assert_eq!(waste.get_int("delta_bases_bytes"), Some(6));
    assert_eq!(waste.get_int("inode_blocks_bytes"), Some(8));
    assert_eq!(
        texts(wire.get("delta_base_references").unwrap()),
        vec!["0707070707070707070707070707070707070707070707070707070707070707"]
    );
    assert_eq!(wire.get_int("delta_base_references_count"), Some(1));
    assert_eq!(wire.get_int("residue_bytes"), Some(0));
    assert!(texts(wire.get("residue_artifacts").unwrap()).is_empty());
    assert_eq!(wire.get("residue_reason"), Some(&SdkValue::Null));
    // One reason per zero field, named by its key in this answer.
    let SdkValue::List(zero) = wire.get("zero_reasons").unwrap() else {
        panic!("zero_reasons is a list");
    };
    let fields: Vec<&str> = zero
        .iter()
        .map(|zero| text(zero.get("field").unwrap()))
        .collect();
    assert_eq!(fields, ["dead_bytes", "discarded_bytes"]);
    assert_eq!(
        text(zero[0].get("reason").unwrap()),
        "no-waste",
        "the closed vocabulary passes through"
    );
    // a run below its threshold wrote nothing
    let mut none = receipt();
    none.path = CompactPath::None;
    let wire = layerfs_store::report::compact_report("s", &none);
    assert_eq!(text(wire.get("outcome").unwrap()), "unchanged");
}

#[test]
fn a_live_read_emits_bounded_busy_and_unresolved_metrics() {
    let home = store_home();
    let capture = layerfs_persistence::compact_metrics::CompactMetricsCapture::start();
    // Idle: no compaction record, so no `compact.status` sample.
    let _ = live(&home);
    // A live owner is `busy`; a gone one is `unresolved`.
    sidecar(&home, "build", std::process::id());
    let _ = live(&home);
    sidecar(&home, "build", 99_999_999);
    let _ = live(&home);
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
    assert_eq!(value("idle"), 0, "an idle Store is not a compaction op");
    std::fs::remove_dir_all(home).unwrap();
}

/// #49 round 3: a returned run's record names an owner pid that is still
/// alive, but the run is over. A live read must show that leftover as
/// `broken` `unresolved`, never `busy`, with the next compaction as its fix.
#[test]
fn a_returned_leftover_is_unresolved_even_with_a_live_owner_pid() {
    let home = store_home();
    let op_path = sidecar_with(&home, "install", std::process::id(), true);
    let seen = live(&home);
    assert_eq!(seen.get_text("state"), Some("broken"));
    assert_eq!(seen.get_text("reason"), Some("unresolved"));
    assert_eq!(seen.get_text("fix"), Some("layerfs store compact s"));
    std::fs::remove_file(op_path).unwrap();
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_live_read_shows_the_sidecar_phase_and_state_and_remembers_a_leftover() {
    let home = store_home();
    let generation = || layerfs_home::home::generation::read(&home);

    // No op: ready, no phases; the observation moves no generation.
    let before = generation();
    let idle = live(&home);
    assert_eq!(generation(), before);
    assert_eq!(idle.get_text("state"), Some("ready"));
    assert_eq!(
        idle.get("live").unwrap().get("phases"),
        Some(&SdkValue::Null)
    );
    assert!(idle.get_text("observed_at").is_some());

    // A live owner is busy; `build` is the sidecar's phase.
    let op_path = sidecar(&home, "build", std::process::id());
    let busy = live(&home);
    assert_eq!(busy.get_text("state"), Some("busy"));
    let op = busy.get("op").unwrap();
    assert_eq!(text(op.get("name").unwrap()), "compact");
    assert_eq!(text(op.get("phase").unwrap()), "build");
    assert_eq!(
        text(op.get("started_at").unwrap()),
        layerfs_home::time::iso(1234)
    );
    assert_eq!(
        texts(busy.get("live").unwrap().get("phases").unwrap()),
        ["mark", "build", "verify", "install", "cleanup"]
    );
    // a busy compaction is no change of the record: a records-only read
    // still answers ready
    assert_eq!(recorded(&home).get_text("state"), Some("ready"));

    // A gone owner leaves an uncertain op: unresolved, written to the
    // record once, so a records-only read answers it too.
    sidecar(&home, "build", 99_999_999);
    let before = generation();
    let leftover = live(&home);
    assert!(generation() > before, "a changed state writes the record");
    assert_eq!(leftover.get_text("state"), Some("broken"));
    assert_eq!(leftover.get_text("reason"), Some("unresolved"));
    let after = generation();
    let _ = live(&home);
    assert_eq!(generation(), after, "an unchanged state writes nothing");
    let records = recorded(&home);
    assert_eq!(records.get_text("state"), Some("broken"));
    assert_eq!(records.get_text("reason"), Some("unresolved"));

    // A settled run removes the record: ready again, and remembered.
    std::fs::remove_file(op_path).unwrap();
    let idle = live(&home);
    assert_eq!(idle.get_text("state"), Some("ready"));
    assert_eq!(recorded(&home).get_text("state"), Some("ready"));
    std::fs::remove_dir_all(home).unwrap();
}
