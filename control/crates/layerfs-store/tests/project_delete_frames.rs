//! The `project.delete` worker frame and its reply: the map the caller
//! builds, the fields the worker decodes, and the reply it sends back.
use layerfs_bridge::control::SdkValue;
use layerfs_bridge::sdk_values::hex;
use layerfs_history::LayerStackId;
use layerfs_home::names::StoreName;
use layerfs_store::project::frame::{
    DeleteAdmitFrame, DeleteEntryFrame, DeleteEntryReply, DeleteFrame, DeleteReleaseFrame,
    DeleteReply,
};

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
    let op = [7u8; 16];
    let record = map(vec![("name", t("shop")), ("kind", t("sqlite"))]);
    let frame = DeleteFrame {
        store: StoreName::parse("shop").unwrap(),
        project: Some("app".to_owned()),
        stack,
        deleted_ms: Some(5),
        op_id: Some(op),
        pending_retry: Some(true),
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
            ("op_id", t(&hex(&op))),
            ("pending_retry", SdkValue::Bool(true)),
            ("store_record", record),
        ])
    );
    let decoded = DeleteFrame::decode(&encoded).unwrap();
    assert_eq!(decoded.store, frame.store);
    assert_eq!(decoded.project, frame.project);
    assert_eq!(decoded.stack, stack);
    assert_eq!(decoded.deleted_ms, Some(5));
    assert_eq!(decoded.op_id, Some(op));
    assert_eq!(decoded.pending_retry, Some(true));
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
    assert_eq!(decoded.op_id, None);
    assert_eq!(decoded.pending_retry, None);
    assert_eq!(decoded.store_record, None);
    assert_eq!(decoded.stack, stack);
}

#[test]
fn the_admission_and_release_frames_round_trip() {
    let stack = stack(4);
    let op = [9u8; 16];
    let record = map(vec![("name", t("shop")), ("kind", t("sqlite"))]);
    let admit = DeleteAdmitFrame {
        store: StoreName::parse("shop").unwrap(),
        stack,
        op,
        project: Some("app".to_owned()),
        store_record: Some(record.clone()),
    };
    let encoded = admit.encode();
    assert_eq!(
        encoded,
        map(vec![
            ("store", t("shop")),
            ("stack_id", t(&hex(&stack.to_bytes()))),
            ("op_id", t(&hex(&op))),
            ("project", t("app")),
            ("store_record", record.clone()),
        ])
    );
    assert_eq!(DeleteAdmitFrame::decode(&encoded).unwrap(), admit);

    let release = DeleteReleaseFrame {
        store: StoreName::parse("shop").unwrap(),
        op,
        store_record: None,
    };
    let encoded = release.encode();
    assert_eq!(
        encoded,
        map(vec![("store", t("shop")), ("op_id", t(&hex(&op)))])
    );
    assert_eq!(DeleteReleaseFrame::decode(&encoded).unwrap(), release);
}

#[test]
fn an_unnamed_admit_frame_is_the_older_caller() {
    // The caller that does not know the project's name omits `project`; the
    // worker fills it from the Stack record, so the frame still decodes.
    let stack = stack(6);
    let op = [5u8; 16];
    let frame = map(vec![
        ("store", t("shop")),
        ("stack_id", t(&hex(&stack.to_bytes()))),
        ("op_id", t(&hex(&op))),
    ]);
    let decoded = DeleteAdmitFrame::decode(&frame).unwrap();
    assert_eq!(decoded.project, None);
    assert_eq!((decoded.stack, decoded.op), (stack, op));
    assert_eq!(decoded.store_record, None);
    // and the encode of an unnamed frame leaves the field out
    assert_eq!(decoded.encode(), frame);
}

#[test]
fn the_admission_frames_refuse_a_bad_operation_identity() {
    let decode = |fields| DeleteAdmitFrame::decode(&map(fields)).unwrap_err().message;
    let stack = stack(5);
    assert_eq!(
        decode(vec![
            ("store", t("shop")),
            ("stack_id", t(&hex(&stack.to_bytes()))),
            ("op_id", t("zz")),
        ]),
        "`op_id` must be hex digits"
    );
    assert_eq!(
        decode(vec![
            ("store", t("shop")),
            ("stack_id", t(&hex(&stack.to_bytes()))),
            ("op_id", t("ab")),
        ]),
        "`op_id` must be an operation identity (16 bytes)"
    );
    assert_eq!(
        delete_release_decode(&map(vec![("store", t("shop"))])),
        "missing text argument `op_id`"
    );
}

fn delete_release_decode(frame: &SdkValue) -> String {
    DeleteReleaseFrame::decode(frame).unwrap_err().message
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
fn the_entry_frame_and_reply_round_trip() {
    let stack = stack(7);
    let frame = DeleteEntryFrame {
        store: StoreName::parse("shop").unwrap(),
        stack,
        project: Some("app".to_owned()),
    };
    let encoded = frame.encode();
    assert_eq!(
        encoded,
        map(vec![
            ("store", t("shop")),
            ("stack_id", t(&hex(&stack.to_bytes()))),
            ("project", t("app")),
        ])
    );
    assert_eq!(DeleteEntryFrame::decode(&encoded).unwrap(), frame);
    // an unnamed caller omits `project`, and the encode leaves it out
    let unnamed = map(vec![
        ("store", t("shop")),
        ("stack_id", t(&hex(&stack.to_bytes()))),
    ]);
    let decoded = DeleteEntryFrame::decode(&unnamed).unwrap();
    assert_eq!(decoded.project, None);
    assert_eq!(decoded.stack, stack);
    assert_eq!(decoded.encode(), unnamed);

    let reply = DeleteEntryReply {
        project: "app".to_owned(),
        stack,
    };
    let encoded = reply.encode();
    assert_eq!(
        encoded,
        map(vec![
            ("kind", t("Removed")),
            ("project", t("app")),
            ("stack_id", t(&hex(&stack.to_bytes()))),
        ])
    );
    assert_eq!(DeleteEntryReply::decode(&encoded).unwrap(), reply);
    let other = map(vec![("kind", t("UpToDate"))]);
    assert_eq!(
        DeleteEntryReply::decode(&other).unwrap_err().message,
        "the worker project.delete entry reply is not `Removed`"
    );
}

#[test]
fn the_reply_round_trips_and_requires_removed() {
    let stack = stack(3);
    let reply = DeleteReply {
        project: "app".to_owned(),
        stack,
        deleted_ms: 42,
        retired: Some(true),
    };
    let encoded = reply.encode();
    assert_eq!(
        encoded,
        map(vec![
            ("kind", t("Removed")),
            ("project", t("app")),
            ("stack_id", t(&hex(&stack.to_bytes()))),
            ("deleted_ms", SdkValue::Int(42)),
            ("retired", SdkValue::Bool(true)),
        ])
    );
    assert_eq!(DeleteReply::decode(&encoded).unwrap(), reply);
    // An older worker's reply without `retired` decodes as a fresh marker.
    let older = DeleteReply::decode(&map(vec![
        ("kind", t("Removed")),
        ("project", t("app")),
        ("stack_id", t(&hex(&stack.to_bytes()))),
        ("deleted_ms", SdkValue::Int(42)),
    ]))
    .unwrap();
    assert_eq!(older.retired, None);
    let other = map(vec![("kind", t("UpToDate"))]);
    assert_eq!(
        DeleteReply::decode(&other).unwrap_err().message,
        "the worker project.delete reply is not `Removed`"
    );
}
