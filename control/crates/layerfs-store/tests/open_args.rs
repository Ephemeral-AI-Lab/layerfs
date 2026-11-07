//! A Store call's open fields read from its `store.call` frame
//! (layerfs-project#109 R8).
use layerfs_bridge::control::SdkValue;
use layerfs_store::call::CallFrame;
use layerfs_store::open_args::ROUTING;

#[test]
fn reads_the_routing_fields_and_refuses_a_missing_store_path() {
    let args = SdkValue::map([
        ("home", SdkValue::text("/h")),
        ("store", SdkValue::text("s")),
        ("project", SdkValue::text("p")),
        ("storePath", SdkValue::text("")),
        (
            "postgresql",
            SdkValue::map([("url", SdkValue::text("postgresql://u:pw@x/db"))]),
        ),
    ]);
    let frame = SdkValue::map([("op", SdkValue::text("m")), ("args", args)]);
    let open = CallFrame::decode(&frame).unwrap().open();
    assert_eq!(open.home.as_deref(), Some(std::path::Path::new("/h")));
    assert_eq!(
        (open.store.as_deref(), open.project.as_deref()),
        (Some("s"), Some("p"))
    );
    let postgresql = open.postgresql.as_ref().unwrap();
    assert_eq!(postgresql.url().unwrap(), "postgresql://u:pw@x/db");
    assert_eq!(postgresql.setting().unwrap(), "off");
    assert_eq!(
        postgresql.schema().unwrap_err().message,
        "postgresql.schema is required"
    );
    let refusal = open.store_path().unwrap_err();
    assert_eq!(refusal.code, "InvalidInput");
    assert_eq!(
        refusal.message,
        "a Store call needs storePath, the Store volume's Store file"
    );
    assert!(ROUTING.contains(&"store_record") && !ROUTING.contains(&"project"));
}
