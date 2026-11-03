//! A Store's kind is decided once from its record (#535 §6.1, step 2c): a
//! record without `kind` is a sqlite Store, and a kind LayerFS never writes
//! is refused rather than read as sqlite.
use layerfs_api::calls;
use layerfs_api::home::{self, paths, write_record};
use layerfs_bridge::control::SdkValue;
use std::path::{Path, PathBuf};

fn tempdir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "layerfs-store-kind-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn status(home: &Path, name: &str) -> Result<SdkValue, (String, String)> {
    let args = SdkValue::map([("name", SdkValue::text(name))]);
    calls::call(Some(home), "store.status", args, &mut |_| {}).map_err(|r| (r.code, r.message))
}

#[test]
fn a_store_kind_is_typed_once() {
    let home = tempdir();
    home::open(&home).unwrap();
    write_record(
        &paths::store_record(&home, "old"),
        &SdkValue::map([("name", SdkValue::text("old"))]),
    )
    .unwrap();
    write_record(
        &paths::store_record(&home, "odd"),
        &SdkValue::map([
            ("name", SdkValue::text("odd")),
            ("kind", SdkValue::text("mysql")),
        ]),
    )
    .unwrap();

    let old = status(&home, "old").unwrap();
    assert_eq!(old.get_text("kind"), Some("sqlite"));
    assert_eq!(old.get_text("machine"), Some("native"));

    let (code, message) = status(&home, "odd").unwrap_err();
    assert_eq!(code, "InvalidInput", "{message}");
    assert_eq!(message, "Store record kind");
    std::fs::remove_dir_all(&home).unwrap();
}
