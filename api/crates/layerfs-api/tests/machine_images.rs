//! Image discovery never starts a stopped machine to answer a read.
#![cfg(feature = "machine")]
use layerfs_api::call;
use layerfs_bridge::control::SdkValue;
use layerfs_machine::record::MachineRecord;

#[test]
fn stopped_machine_images_is_unavailable_and_preserves_state() {
    let home = std::env::temp_dir().join(format!("layerfs-image-discovery-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    call(Some(&home), "machine.list", SdkValue::Null, &mut |_| {}).unwrap();
    let record = MachineRecord::from_record(
        "image-test",
        SdkValue::map([
            ("name", SdkValue::text("image-test")),
            ("platform", SdkValue::text("lima")),
            ("state", SdkValue::text("stopped")),
        ]),
    );
    record.write(&home).unwrap();
    let before = std::fs::read(home.join("machine/image-test/record.json")).unwrap();
    let error = call(
        Some(&home),
        "machine.images",
        SdkValue::map([("name", SdkValue::text("image-test"))]),
        &mut |_| {},
    )
    .unwrap_err();
    assert_eq!(error.code, "Unavailable", "{}", error.message);
    assert_eq!(
        before,
        std::fs::read(home.join("machine/image-test/record.json")).unwrap()
    );
    assert!(!home.join("machine/image-test/worker.json").exists());
    std::fs::remove_dir_all(home).unwrap();
}
