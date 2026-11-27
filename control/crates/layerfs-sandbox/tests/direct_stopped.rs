//! A Sandbox whose daemon port its machine forwards, on a machine that is
//! not running (conformance R3 F5): refused `Unavailable` with the command
//! that starts the machine as its `fix`.
use layerfs_protocol::sdk::SdkValue;
use layerfs_sandbox::record::SandboxRecord;

#[test]
fn a_forward_on_a_stopped_machine_says_how_to_start_it() {
    let home = std::env::temp_dir().join(format!("lfs-direct-stopped-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    let forward = SdkValue::map([
        ("address", SdkValue::text("127.0.0.1:1")),
        ("machine", SdkValue::text("dev")),
    ]);
    let record = SandboxRecord::from_record(
        "s1",
        SdkValue::map([
            ("name", SdkValue::text("s1")),
            ("machine", SdkValue::text("dev")),
            ("daemon", SdkValue::map([("forward", forward)])),
        ]),
    );
    let daemon = record.daemon().expect("a daemon record");
    let refusal = layerfs_sandbox::direct::check(&home, "s-1", daemon).unwrap_err();
    assert_eq!(refusal.code, "Unavailable", "{refusal:?}");
    assert_eq!(
        refusal.message,
        "machine dev, which runs Sandbox s-1, is not running; start it with `layerfs machine start dev`"
    );
    assert_eq!(
        refusal.detail.get_text("fix"),
        Some("layerfs machine start dev"),
        "{refusal:?}"
    );
    std::fs::remove_dir_all(home).unwrap();
}
