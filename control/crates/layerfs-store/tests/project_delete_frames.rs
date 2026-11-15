//! The `project.delete` worker frame and its reply: the map the caller
//! builds, the fields the worker decodes, and the reply it sends back.
use layerfs_bridge::control::SdkValue;
use layerfs_bridge::sdk_values::hex;
use layerfs_history::LayerStackId;
use layerfs_home::names::StoreName;
use layerfs_store::project::frame::{DeleteFrame, DeleteReply};

fn t(text: &str) -> SdkValue {
    SdkValue::text(text)
}

fn map(fields: Vec<(&str, SdkValue)>) -> SdkValue {
    SdkValue::Map(fields.into_iter().map(|(n, v)| (n.to_owned(), v)).collect())
}

fn stack(body: u8) -> LayerStackId {
    LayerStackId::from_authority([body; 16])
}

#[test]
fn the_frame_names_the_store_stack_and_optional_context() {
    let stack = stack(1);
    let record = map(vec![("name", t("shop")), ("kind", t("sqlite"))]);
    let frame = DeleteFrame {
        store: StoreName::parse("shop").unwrap(),
        project: Some("app".to_owned()),
        stack,
        deleted_ms: Some(5),
        store_record: Some(record.clone()),
    };
    let encoded = frame.encode();
    assert_eq!(
        encoded,
        map(vec![
            ("store", t("shop")),
            ("stack_id", t(&hex(&stack.to_bytes()))),
            ("deleted_ms", SdkValue::Int(5)),
            ("project", t("app")),
            ("store_record", record),
        ])
    );
    let decoded = DeleteFrame::decode(&encoded).unwrap();
    assert_eq!(decoded.store, frame.store);
    assert_eq!(decoded.project, frame.project);
    assert_eq!(decoded.stack, stack);
    assert_eq!(decoded.deleted_ms, Some(5));
    assert_eq!(decoded.store_record, frame.store_record);
}

#[test]
fn a_frame_with_only_the_store_and_stack_decodes() {
    let stack = stack(2);
    let minimal = map(vec![
        ("store", t("shop")),
        ("stack_id", t(&hex(&stack.to_bytes()))),
    ]);
    let decoded = DeleteFrame::decode(&minimal).unwrap();
    assert_eq!((decoded.project, decoded.deleted_ms), (None, None));
    assert_eq!(decoded.store_record, None);
    assert_eq!(decoded.stack, stack);
}

#[test]
fn the_worker_refuses_a_bad_frame_in_order() {
    let decode = |fields| DeleteFrame::decode(&map(fields)).unwrap_err().message;
    assert_eq!(decode(vec![]), "missing text argument `store`");
    assert!(decode(vec![("store", t("../x"))]).starts_with("store name"));
    assert_eq!(
        decode(vec![("store", t("shop"))]),
        "missing text argument `stack_id`"
    );
    assert_eq!(
        decode(vec![("store", t("shop")), ("stack_id", t("zz"))]),
        "`stack_id` must be hex digits"
    );
    assert_eq!(
        decode(vec![("store", t("shop")), ("stack_id", t("ab"))]),
        "`stack_id` must be a LayerStack identity (17 bytes)"
    );
}

#[test]
fn the_reply_round_trips_and_requires_removed() {
    let stack = stack(3);
    let reply = DeleteReply {
        project: "app".to_owned(),
        stack,
        deleted_ms: 42,
    };
    let encoded = reply.encode();
    assert_eq!(
        encoded,
        map(vec![
            ("kind", t("Removed")),
            ("project", t("app")),
            ("stack_id", t(&hex(&stack.to_bytes()))),
            ("deleted_ms", SdkValue::Int(42)),
        ])
    );
    assert_eq!(DeleteReply::decode(&encoded).unwrap(), reply);
    let other = map(vec![("kind", t("UpToDate"))]);
    assert_eq!(
        DeleteReply::decode(&other).unwrap_err().message,
        "the worker project.delete reply is not `Removed`"
    );
}
