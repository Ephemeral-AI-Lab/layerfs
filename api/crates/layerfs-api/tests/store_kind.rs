//! A Store's kind is decided once from its record (#535 §6.1, step 2c): a
//! record without `kind` (one written before 2.0) and a kind LayerFS never
//! writes are never read as sqlite: `store.get` answers each `broken`, and
//! a Store call refuses each.
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

fn call(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> Result<SdkValue, (String, String)> {
    let args = SdkValue::Map(
        args.iter()
            .map(|(n, v)| ((*n).to_owned(), v.clone()))
            .collect(),
    );
    layerfs_api::call(Some(home), name, args, &mut |_| {}).map_err(|r| (r.code, r.message))
}

fn get(home: &Path, name: &str) -> SdkValue {
    call(home, "store.get", &[("store", SdkValue::text(name))]).unwrap()
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

    let old = get(&home, "old");
    assert_eq!(old.get_text("engine"), Some("unknown"), "{old:?}");
    assert_eq!(old.get_text("state"), Some("broken"), "{old:?}");
    assert_eq!(old.get_text("reason"), Some("unsupported"), "{old:?}");
    let compact = [("store", SdkValue::text("old"))];
    let (code, message) = call(&home, "store.compact", &compact).unwrap_err();
    assert_eq!(code, "InvalidInput", "{message}");
    assert_eq!(message, "Store record kind");

    let odd = get(&home, "odd");
    assert_eq!(odd.get_text("engine"), Some("mysql"));
    assert_eq!(odd.get_text("state"), Some("broken"));
    assert_eq!(odd.get_text("reason"), Some("unsupported"));
    let compact = [("store", SdkValue::text("odd"))];
    let (code, message) = call(&home, "store.compact", &compact).unwrap_err();
    assert_eq!(code, "InvalidInput", "{message}");
    assert_eq!(message, "Store record kind");
    std::fs::remove_dir_all(&home).unwrap();
}
