//! Host naming compatibility for development homes, including a read-only diagnosis.
use crate::doctor_own;

use layerfs_home::disk::{self, private_dir, write_record};
use layerfs_inspect::{Kind, Options};
use layerfs_protocol::sdk::SdkValue;
use std::path::Path;

fn record(root: &Path, path: &str, value: SdkValue) {
    let path = root.join(path);
    private_dir(path.parent().unwrap()).unwrap();
    write_record(&path, &value).unwrap();
}

#[test]
fn development_home_reports_each_record_in_one_read_only_finding() {
    let root = std::env::temp_dir().join(format!("legacy-home-records-{}", std::process::id()));
    private_dir(&root).unwrap();
    let file = root.join("layerfs.json");
    record(
        &root,
        "layerfs.json",
        SdkValue::map([
            ("layout", SdkValue::Int(3)),
            ("id", SdkValue::text("0123456789abcdef0123456789abcdef")),
        ]),
    );
    assert!(
        matches!(disk::detect(&root).unwrap(), disk::Layout::Current(file) if file.layout == 3)
    );
    let original = std::fs::read(&file).unwrap();
    let samples = [
        (
            "machine/native/record.json",
            "platform",
            SdkValue::text("native"),
        ),
        (
            "machine/macos-host/record.json",
            "platform",
            SdkValue::text("lima"),
        ),
        (
            "machine/linux-host/record.json",
            "platform",
            SdkValue::text("docker"),
        ),
        (
            "machine/custom/record.json",
            "platform",
            SdkValue::text("native"),
        ),
        (
            "sandbox/old-runtime/record.json",
            "runtime",
            SdkValue::text("native"),
        ),
        (
            "store/old-placement/record.json",
            "machine",
            SdkValue::text("native"),
        ),
        (
            "store/old-attachment/record.json",
            "attached",
            SdkValue::List(vec![SdkValue::text("native")]),
        ),
    ];
    let mut expected = vec![
        "machine/native".to_string(),
        "machine/native/run".to_string(),
    ];
    private_dir(&root.join("machine/native/run")).unwrap();
    for (path, field, value) in samples {
        record(&root, path, SdkValue::map([(field, value)]));
        expected.push(path.to_string());
    }
    expected.sort();
    let failure = disk::open(&root).unwrap_err();
    assert_eq!(failure.code, "Unsupported");
    let report = doctor_own::own(
        &root,
        Kind::Home,
        &root.display().to_string(),
        &Options::default(),
    );
    let Some(SdkValue::List(findings)) = report.get("findings") else {
        panic!("{report:?}");
    };
    let mut layouts = findings
        .iter()
        .filter(|value| value.get_text("check") == Some("home.layout"));
    let finding = layouts.next().expect("one home layout finding");
    assert!(layouts.next().is_none());
    let actual = finding.get("evidence").unwrap().get("records").unwrap();
    let expected = SdkValue::List(expected.into_iter().map(SdkValue::Text).collect());
    assert_eq!(actual, &expected);
    assert_eq!(std::fs::read(file).unwrap(), original);
    assert!(!root.join("layerfs.lock").exists());
    assert!(!root.join("trace").exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn compatible_development_home_can_reopen_and_retire_a_local_run() {
    let root = std::env::temp_dir().join(format!("legacy-home-local-run-{}", std::process::id()));
    let id = "0123456789abcdef0123456789abcdef";
    record(
        &root,
        "layerfs.json",
        SdkValue::map([("layout", SdkValue::Int(3)), ("id", SdkValue::text(id))]),
    );
    let file = root.join("layerfs.json");
    let before = std::fs::read(&file).unwrap();
    doctor_own::own(
        &root,
        Kind::Home,
        &root.display().to_string(),
        &Options::default(),
    );
    assert_eq!(std::fs::read(&file).unwrap(), before);
    assert!(!root.join("layerfs.lock").exists());

    let opened = disk::open(&root).unwrap();
    assert_eq!(opened.id, id);
    let sandbox = disk::paths::run_root(&root).join("test-sandbox");
    private_dir(&sandbox).unwrap();
    assert_eq!(disk::open(&root).unwrap().id, id);
    std::fs::remove_dir(&sandbox).unwrap();
    assert_eq!(disk::open(&root).unwrap().id, id);
    std::fs::remove_dir_all(root).unwrap();
}
