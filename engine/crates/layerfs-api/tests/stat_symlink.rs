//! `fs.stat` gives a symlink the size a mounted Workspace gives it: the
//! length of its target, not of the object that stores the target.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_bridge::control::SdkValue;
use std::fs;
use support::Fixture;

#[test]
fn a_symlink_size_is_its_target_length() {
    let fixture = Fixture::new("stat-symlink", |source| {
        fs::write(source.join("a.txt"), b"a\n").unwrap();
        std::os::unix::fs::symlink("a.txt", source.join("link")).unwrap();
        std::os::unix::fs::symlink("/does/not/exist", source.join("dangling")).unwrap();
        3
    });
    for (path, target) in [("/link", "a.txt"), ("/dangling", "/does/not/exist")] {
        let stat = fixture.call(
            "fs.stat",
            vec![
                ("branch", SdkValue::text("main")),
                ("path", SdkValue::text(path)),
            ],
        );
        assert_eq!(stat.get("type"), Some(&SdkValue::text("symlink")), "{path}");
        assert_eq!(
            stat.get("size"),
            Some(&SdkValue::Int(target.len() as i64)),
            "{path}"
        );
    }
    fixture.cleanup();
}
