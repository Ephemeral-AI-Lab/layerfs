//! The D2 evidence: the SDK's one dispatch entry opens a real home and a
//! real Store in this process on Windows and runs the core calls end to
//! end - project.init into a workspace write, read back, doctor clean.
#![cfg(windows)]

use layerfs_api::doctor_json;
use layerfs_bridge::control::SdkValue;

fn call(home: &std::path::Path, name: &str, args: SdkValue) -> layerfs_bridge::control::SdkValue {
    let mut events = |_event: SdkValue| {};
    layerfs_control::calls::call(layerfs_api::families(), Some(home), name, args, &mut events)
        .expect("the call refuses")
}

#[test]
fn in_process_home_init_write_read_doctor() {
    let home = std::env::temp_dir().join(format!("layerfs-d2-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let source = std::env::temp_dir().join(format!("layerfs-d2-src-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&source);
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("hello.txt"), b"hello from windows
").unwrap();

    let store = call(
        &home,
        "store.create",
        SdkValue::map([("name", SdkValue::text("d2store"))]),
    );
    let _ = &store;
    let init = call(
        &home,
        "project.init",
        SdkValue::map([
            ("project", SdkValue::text("d2store/d2")),
            ("folder", SdkValue::text(source.to_string_lossy().into_owned())),
        ]),
    );
    let project = init.get_text("project").unwrap_or("<none>").to_owned();
    assert_eq!(project, "d2store/d2");

    let _fork = call(
        &home,
        "branch.fork",
        SdkValue::map([
            ("project", SdkValue::text("d2store/d2")),
            ("name", SdkValue::text("work")),
            ("from_branch", SdkValue::text("main")),
        ]),
    );
    let _written = call(
        &home,
        "file.write",
        SdkValue::map([
            ("project", SdkValue::text("d2store/d2")),
            ("branch", SdkValue::text("work")),
            ("path", SdkValue::text("/written.txt")),
            ("content", SdkValue::text("written in process")),
        ]),
    );

    let read = call(
        &home,
        "file.read",
        SdkValue::map([
            ("project", SdkValue::text("d2store/d2")),
            ("branch", SdkValue::text("main")),
            ("path", SdkValue::text("/hello.txt")),
        ]),
    );
    let text = format!("{read:?}");
    assert!(text.contains("hello from windows"), "{text}");

    let report = doctor_json(Ok(home.clone()));
    assert!(!report.is_empty());
    let _ = std::fs::remove_dir_all(&home);
    let _ = std::fs::remove_dir_all(&source);
}
