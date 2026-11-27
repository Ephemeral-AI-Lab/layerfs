//! The `project.list` worker frame and its reply (T4.5c): the map the
//! caller builds, the fields the worker decodes, and the filtered listing
//! it sends back.
use layerfs_history::LayerStackId;
use layerfs_home::names::StoreName;
use layerfs_protocol::sdk::hex;
use layerfs_protocol::sdk::SdkValue;
use layerfs_store::project::frame::{ListedProject, ListingFrame, ListingReply};

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
fn the_frame_names_the_store_and_carries_its_record() {
    let record = map(vec![("name", t("shop")), ("kind", t("sqlite"))]);
    let frame = ListingFrame {
        store: StoreName::parse("shop").unwrap(),
        store_record: Some(record.clone()),
        stats: false,
    };
    let encoded = frame.encode();
    assert_eq!(
        encoded,
        map(vec![("store", t("shop")), ("store_record", record.clone())])
    );
    let decoded = ListingFrame::decode(&encoded).unwrap();
    assert_eq!(decoded.store, frame.store);
    assert_eq!(decoded.store_record, Some(record));
    assert!(!decoded.stats);
}

#[test]
fn a_frame_asking_for_sizes_says_so() {
    let frame = ListingFrame {
        store: StoreName::parse("shop").unwrap(),
        store_record: None,
        stats: true,
    };
    let encoded = frame.encode();
    assert_eq!(
        encoded,
        map(vec![("store", t("shop")), ("stats", SdkValue::Bool(true))])
    );
    assert!(ListingFrame::decode(&encoded).unwrap().stats);
}

#[test]
fn a_frame_with_only_the_store_decodes() {
    let decoded = ListingFrame::decode(&map(vec![("store", t("shop"))])).unwrap();
    assert_eq!(decoded.store.as_str(), "shop");
    assert_eq!(decoded.store_record, None);
}

#[test]
fn the_worker_refuses_a_bad_frame_in_order() {
    let decode = |fields| ListingFrame::decode(&map(fields)).unwrap_err().message;
    assert_eq!(decode(vec![]), "missing text argument `store`");
    assert!(decode(vec![("store", t("../x"))]).starts_with("store name"));
}

#[test]
fn the_reply_round_trips_and_requires_listed() {
    let reply = ListingReply {
        projects: vec![
            ListedProject {
                name: "app".to_owned(),
                stack: stack(1),
                head: Some("aa".repeat(33)),
                size: Some(12),
            },
            ListedProject {
                name: "web".to_owned(),
                stack: stack(2),
                head: None,
                size: None,
            },
        ],
    };
    let listed = |name: &str, stack: &LayerStackId, head: SdkValue, size: SdkValue| {
        map(vec![
            ("name", t(name)),
            ("stack_id", t(&hex(&stack.to_bytes()))),
            ("head", head),
            ("size", size),
        ])
    };
    let encoded = reply.encode();
    assert_eq!(
        encoded,
        map(vec![
            ("kind", t("Listed")),
            (
                "projects",
                SdkValue::List(vec![
                    listed("app", &stack(1), t(&"aa".repeat(33)), SdkValue::Int(12)),
                    listed("web", &stack(2), SdkValue::Null, SdkValue::Null),
                ]),
            ),
        ])
    );
    assert_eq!(ListingReply::decode(&encoded).unwrap(), reply);

    let wrong_kind = map(vec![("kind", t("Removed"))]);
    assert_eq!(
        ListingReply::decode(&wrong_kind).unwrap_err().message,
        "the worker project.list reply is not `Listed`"
    );
    let no_list = map(vec![("kind", t("Listed"))]);
    assert_eq!(
        ListingReply::decode(&no_list).unwrap_err().message,
        "reply has no `projects` list"
    );
    let no_name = map(vec![
        ("kind", t("Listed")),
        (
            "projects",
            SdkValue::List(vec![map(vec![("stack_id", t(&hex(&stack(3).to_bytes())))])]),
        ),
    ]);
    assert_eq!(
        ListingReply::decode(&no_name).unwrap_err().message,
        "reply project has no `name`"
    );
    let bad_stack = map(vec![
        ("kind", t("Listed")),
        (
            "projects",
            SdkValue::List(vec![map(vec![("name", t("app")), ("stack_id", t("zz"))])]),
        ),
    ]);
    assert_eq!(
        ListingReply::decode(&bad_stack).unwrap_err().message,
        "`stack_id` must be hex digits"
    );
    let empty = ListingReply {
        projects: Vec::new(),
    };
    assert_eq!(ListingReply::decode(&empty.encode()).unwrap(), empty);
}
