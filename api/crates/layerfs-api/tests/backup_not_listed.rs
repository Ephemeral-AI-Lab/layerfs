//! `backup.list` names a bucket it did not ask in `not_listed` with the
//! reason `refused` (the asking refused before any request), never `lost`,
//! which is only for a request that got no answer (RF re-check N-E): a
//! bucket whose key has no saved secret, and one whose record cannot be
//! read. Nothing is sent: neither bucket has an endpoint that answers.
use layerfs_api::home::{self, paths, write_record};
use layerfs_bridge::control::SdkValue;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "layerfs-backup-not-listed-{tag}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

#[test]
fn a_bucket_not_asked_is_refused_not_lost() {
    let home = tempdir("home");
    home::open(&home).unwrap();
    // a bucket whose key's secret the home does not keep
    write_record(
        &paths::bucket_record(&home, "nokey"),
        &SdkValue::map([
            ("name", SdkValue::text("nokey")),
            ("provider", SdkValue::text("minio")),
            ("endpoint", SdkValue::text("http://127.0.0.1:9")),
            ("bucket", SdkValue::text("nothing")),
            ("prefix", SdkValue::text("")),
            ("region", SdkValue::text("us-east-1")),
            ("access_key", SdkValue::text("user")),
        ]),
    )
    .unwrap();
    // a bucket whose record cannot be read
    let bad = paths::bucket_record(&home, "bad");
    std::fs::create_dir_all(bad.parent().unwrap()).unwrap();
    std::fs::write(&bad, "not json").unwrap();

    let listed = layerfs_api::call(
        Some(&home),
        "backup.list",
        SdkValue::Map(Vec::new()),
        &mut |_| {},
    )
    .unwrap_or_else(|refusal| panic!("{refusal:?}"));
    assert_eq!(listed.get("items"), Some(&SdkValue::List(Vec::new())));
    let Some(SdkValue::List(skipped)) = listed.get("not_listed") else {
        panic!("{listed:?}");
    };
    let mut seen: Vec<(Option<&str>, Option<&str>)> = (skipped.iter())
        .map(|entry| (entry.get_text("bucket"), entry.get_text("reason")))
        .collect();
    seen.sort();
    assert_eq!(
        seen,
        [
            (Some("bad"), Some("refused")),
            (Some("nokey"), Some("refused"))
        ],
        "{listed:?}"
    );
    for entry in skipped {
        assert!(
            entry.get_text("message").is_some_and(|m| !m.is_empty()),
            "{entry:?}"
        );
    }
    std::fs::remove_dir_all(home).unwrap();
}
