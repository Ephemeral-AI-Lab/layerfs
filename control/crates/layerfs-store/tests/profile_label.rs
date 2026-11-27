//! A sqlite Store record's profile label (owner, 2026-11-23): new and
//! restored records are written `normal`, and a record written before then,
//! which says `disposable`, reads as `normal` without being rewritten.
use layerfs_protocol::provision::install::SdkValue;
use layerfs_store::record::{profile, StoreRecord, PROFILE};

fn record(label: Option<&str>) -> StoreRecord {
    let fields = match label {
        Some(label) => SdkValue::map([
            ("name", SdkValue::text("shop")),
            ("kind", SdkValue::text("sqlite")),
            ("machine", SdkValue::text(layerfs_home::names::HOST_MACHINE)),
            ("profile", SdkValue::text(label)),
        ]),
        None => SdkValue::map([
            ("name", SdkValue::text("shop")),
            ("kind", SdkValue::text("sqlite")),
            ("machine", SdkValue::text(layerfs_home::names::HOST_MACHINE)),
        ]),
    };
    StoreRecord::from_record("shop", fields)
}

#[test]
fn new_records_are_written_normal() {
    assert_eq!(PROFILE, "normal");
    let restored = StoreRecord::restored(
        "shop",
        layerfs_home::names::HOST_MACHINE,
        &[7; 16],
        &[9; 32],
    );
    assert_eq!(restored.text("profile"), Some("normal"));
    assert_eq!(profile(&restored), Some("normal"));
}

#[test]
fn an_earlier_disposable_label_reads_as_normal() {
    let earlier = record(Some("disposable"));
    assert_eq!(profile(&earlier), Some("normal"));
    // read, never rewritten
    assert_eq!(earlier.text("profile"), Some("disposable"));
}

#[test]
fn other_labels_pass_through() {
    assert_eq!(profile(&record(Some("normal"))), Some("normal"));
    assert_eq!(profile(&record(None)), None);
}
