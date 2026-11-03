//! Each typed record's `to_sdk` gives the shape its calls answered before
//! the records were typed (#536 §10 "Result keys stay stable"), byte for
//! byte as JSON text, and reads past a layout-3 `v`.
use layerfs_bridge::control::SdkValue;
use layerfs_control::backup::record::BucketRecord;
use layerfs_control::common::json::{parse, print};
use layerfs_control::common::outcome::View;
use layerfs_control::project::record::ProjectRecord;
use layerfs_control::store::record::{StoreDerived, StoreRecord};

fn value(text: &str) -> SdkValue {
    parse(text).unwrap()
}

fn projects(names: &[&str]) -> StoreDerived {
    StoreDerived {
        projects: names.iter().map(|n| n.to_string()).collect(),
    }
}

#[test]
fn a_store_shows_its_record_without_secrets_then_its_projects() {
    let record = StoreRecord::from_record(
        "shop",
        value(
            r#"{"v":1,"binding":"b","name":"old","cursor":"c","kind":"sqlite","machine":"box",
               "profile":"disposable","created":5,"sqlite":"3.50","backups":{"r2":{"file":"shop"}},
               "in_progress":null,"last_move":{"from":"native","to":"box"}}"#,
        ),
    );
    let want = r#"{"name":"shop","kind":"sqlite","machine":"box","profile":"disposable","created":5,"sqlite":"3.50","backups":{"r2":{"file":"shop"}},"in_progress":null,"last_move":{"from":"native","to":"box"},"projects":["api","web"]}"#;
    for view in [View::Full, View::List] {
        let shown = record.to_sdk(view, &projects(&["api", "web"])).unwrap();
        assert_eq!(print(&shown), want);
    }
    // the record itself is written back as read, `v` and all
    assert_eq!(record.to_record().get("v"), Some(&SdkValue::Int(1)));
}

#[test]
fn a_store_record_of_an_unknown_kind_still_shows() {
    let record = StoreRecord::from_record("odd", value(r#"{"kind":"oracle"}"#));
    let shown = record.to_sdk(View::List, &projects(&[])).unwrap();
    assert_eq!(
        print(&shown),
        r#"{"name":"odd","kind":"oracle","projects":[]}"#
    );
    let broken = StoreRecord::from_record("odd", SdkValue::Text("x".into()));
    let refusal = broken.to_sdk(View::Full, &projects(&[])).unwrap_err();
    assert_eq!(refusal.code, "Failed");
    assert_eq!(refusal.message, "Store odd's record is not a record");
}

#[test]
fn a_project_shows_its_store_kind_machine_and_creation() {
    let sqlite = StoreRecord::from_record("shop", value(r#"{"kind":"sqlite"}"#));
    let project =
        ProjectRecord::from_record("shop", "api", value(r#"{"v":1,"name":"api","created":7}"#));
    assert_eq!(
        print(&project.to_sdk(View::Full, &sqlite)),
        r#"{"project":"shop/api","store":"shop","name":"api","kind":"sqlite","machine":"native","created":7}"#
    );
    let placed = StoreRecord::from_record("shop", value(r#"{"machine":"box"}"#));
    let missing = ProjectRecord::from_record("shop", "web", SdkValue::Null);
    assert_eq!(
        print(&missing.to_sdk(View::List, &placed)),
        r#"{"project":"shop/web","store":"shop","name":"web","kind":"sqlite","machine":"box","created":null}"#
    );
    let unknown = StoreRecord::from_record("shop", value(r#"{"kind":"oracle","machine":null}"#));
    assert_eq!(
        print(&missing.to_sdk(View::List, &unknown)),
        r#"{"project":"shop/web","store":"shop","name":"web","kind":null,"machine":null,"created":null}"#
    );
    let postgres = StoreRecord::from_record("pg", value(r#"{"kind":"postgresql"}"#));
    let project = ProjectRecord::from_record("pg", "api", value(r#"{"name":"api","stack":"s"}"#));
    assert_eq!(
        print(&project.to_sdk(View::Full, &postgres)),
        r#"{"project":"pg/api","store":"pg","name":"api","kind":"postgresql","machine":"native","created":null}"#
    );
}

#[test]
fn a_bucket_shows_its_record() {
    let text = r#"{"name":"r","provider":"r2","account":"a","jurisdiction":"","bucket":"b","prefix":"p","region":"auto","key":"env:K","part_size":8}"#;
    let record = BucketRecord::from_record("r", value(text));
    assert_eq!(print(&record.to_sdk(View::Full)), text);
    let versioned = BucketRecord::from_record("r", value(&text.replacen('{', r#"{"v":1,"#, 1)));
    assert_eq!(print(&versioned.to_sdk(View::Full)), text);
}
