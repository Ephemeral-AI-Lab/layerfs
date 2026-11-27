//! `methods::call_streamed` (#819 S2.2): a write's content and an apply's
//! changes pulled from a source one piece and one change at a time, one
//! Commit after the source's whole input, none when the input stops early.
#[path = "../support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "../support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_protocol::sdk::{SdkCall, SdkRefusal, SdkValue};
use layerfs_store_service::families::{call_streamed, Source};
use std::collections::VecDeque;
use support::Fixture;

fn t(text: &str) -> SdkValue {
    SdkValue::text(text)
}

enum Item {
    Change(SdkValue),
    Piece(Vec<u8>),
    EndContent,
    Lost,
}

/// A scripted source; it counts what the call pulled.
struct Script {
    items: VecDeque<Item>,
    piece: Vec<u8>,
    pulled: usize,
}

impl Script {
    fn new(items: Vec<Item>) -> Self {
        Self {
            items: items.into(),
            piece: Vec::new(),
            pulled: 0,
        }
    }
}

fn lost() -> SdkRefusal {
    SdkRefusal {
        code: "Unavailable".to_owned(),
        message: "the input stopped".to_owned(),
        detail: SdkValue::Null,
    }
}

impl Source for Script {
    fn next(&mut self) -> Result<Option<SdkValue>, SdkRefusal> {
        self.pulled += 1;
        match self.items.pop_front() {
            None => Ok(None),
            Some(Item::Change(change)) => Ok(Some(change)),
            Some(Item::Lost) => Err(lost()),
            Some(_) => panic!("a piece where a change was expected"),
        }
    }

    fn piece(&mut self) -> Result<Option<&[u8]>, SdkRefusal> {
        self.pulled += 1;
        match self.items.pop_front() {
            Some(Item::Piece(piece)) => {
                self.piece = piece;
                Ok(Some(&self.piece))
            }
            Some(Item::EndContent) => Ok(None),
            Some(Item::Lost) => Err(lost()),
            _ => panic!("a change where a piece was expected"),
        }
    }
}

fn streamed(
    fixture: &Fixture,
    method: &str,
    fields: Vec<(&str, SdkValue)>,
    source: &mut Script,
) -> Result<SdkValue, SdkRefusal> {
    let mut fields: Vec<(String, SdkValue)> = fields
        .into_iter()
        .map(|(name, value)| (name.to_owned(), value))
        .collect();
    fields.push(("project".to_owned(), t("installed")));
    let call = SdkCall {
        method: method.to_owned(),
        args: SdkValue::Map(fields),
    };
    call_streamed(&fixture.service, &call, source)
}

fn commits(fixture: &Fixture) -> usize {
    let listed = fixture.call("commit.list", vec![("branch", t("feat"))]);
    support::items(&listed).len()
}

fn read(fixture: &Fixture, path: &str) -> String {
    let value = fixture.call("file.read", vec![("branch", t("feat")), ("path", t(path))]);
    value.get_text("text").unwrap().to_owned()
}

#[test]
fn streamed_content_and_changes_commit_once_and_lost_input_commits_nothing() {
    let fixture = Fixture::new("streamed-writes", |source| {
        std::fs::write(source.join("old.txt"), b"0123456789").unwrap();
        1
    });
    fixture.fork("feat");

    // content in pieces larger and smaller than one engine record
    let big = vec![b'a'; 200_000];
    let mut source = Script::new(vec![
        Item::Piece(big.clone()),
        Item::Piece(b"tail\n".to_vec()),
        Item::EndContent,
    ]);
    let fields = vec![
        ("branch", t("feat")),
        ("path", t("/big.txt")),
        ("streamed", SdkValue::Bool(true)),
    ];
    let answer = streamed(&fixture, "file.write", fields.clone(), &mut source).unwrap();
    assert_eq!(answer.get_text("kind"), Some("Committed"));
    assert_eq!(commits(&fixture), 1);
    let stat = fixture.call(
        "file.stat",
        vec![("branch", t("feat")), ("path", t("/big.txt"))],
    );
    assert_eq!(stat.get_int("size"), Some(200_005));
    // the same pieces again: compared, not written, no Commit
    let mut again = Script::new(vec![
        Item::Piece(big),
        Item::Piece(b"tail\n".to_vec()),
        Item::EndContent,
    ]);
    let answer = streamed(&fixture, "file.write", fields, &mut again).unwrap();
    assert_eq!(answer.get_text("kind"), Some("UpToDate"));
    assert_eq!(commits(&fixture), 1);

    // a shorter prefix of an existing file truncates it
    let mut prefix = Script::new(vec![Item::Piece(b"01234".to_vec()), Item::EndContent]);
    let fields = vec![
        ("branch", t("feat")),
        ("path", t("/old.txt")),
        ("streamed", SdkValue::Bool(true)),
    ];
    streamed(&fixture, "file.write", fields, &mut prefix).unwrap();
    assert_eq!(read(&fixture, "/old.txt"), "01234");
    assert_eq!(commits(&fixture), 2);

    // an apply: changes and a streamed write's content, one Commit
    let mut apply = Script::new(vec![
        Item::Change(SdkValue::map([("op", t("mkdir")), ("path", t("/d"))])),
        Item::Change(SdkValue::map([
            ("op", t("write")),
            ("path", t("/d/f")),
            ("streamed", SdkValue::Bool(true)),
        ])),
        Item::Piece(b"one ".to_vec()),
        Item::Piece(b"two\n".to_vec()),
        Item::EndContent,
        Item::Change(SdkValue::map([
            ("op", t("write")),
            ("path", t("/d/g")),
            ("content", t("inline\n")),
        ])),
    ]);
    streamed(
        &fixture,
        "file.apply",
        vec![("branch", t("feat"))],
        &mut apply,
    )
    .unwrap();
    assert_eq!(read(&fixture, "/d/f"), "one two\n");
    assert_eq!(read(&fixture, "/d/g"), "inline\n");
    assert_eq!(commits(&fixture), 3);

    // the input stops inside a content, then between changes: no Commit
    for items in [
        vec![
            Item::Change(SdkValue::map([("op", t("mkdir")), ("path", t("/x"))])),
            Item::Change(SdkValue::map([
                ("op", t("write")),
                ("path", t("/x/f")),
                ("streamed", SdkValue::Bool(true)),
            ])),
            Item::Piece(b"part".to_vec()),
            Item::Lost,
        ],
        vec![
            Item::Change(SdkValue::map([("op", t("mkdir")), ("path", t("/y"))])),
            Item::Lost,
        ],
    ] {
        let mut stopped = Script::new(items);
        let refusal = streamed(
            &fixture,
            "file.apply",
            vec![("branch", t("feat"))],
            &mut stopped,
        )
        .unwrap_err();
        assert_eq!(refusal.code, "Unavailable");
        assert_eq!(commits(&fixture), 3);
    }

    // a change refused mid-stream stops pulling: later input is unread
    let mut refused = Script::new(vec![
        Item::Change(SdkValue::map([("op", t("rm")), ("path", t("/absent"))])),
        Item::Change(SdkValue::map([("op", t("mkdir")), ("path", t("/z"))])),
    ]);
    let refusal = streamed(
        &fixture,
        "file.apply",
        vec![("branch", t("feat"))],
        &mut refused,
    )
    .unwrap_err();
    assert_eq!(refusal.code, "NoSuchPath");
    assert_eq!(refusal.detail.get_int("index"), Some(0));
    assert_eq!(refused.pulled, 1);
    assert_eq!(commits(&fixture), 3);

    // without a source the streamed form is refused, as before
    let r = fixture
        .try_call(
            "file.apply",
            vec![
                ("branch", t("feat")),
                (
                    "changes",
                    SdkValue::List(vec![SdkValue::map([
                        ("op", t("write")),
                        ("path", t("/s")),
                        ("streamed", SdkValue::Bool(true)),
                    ])]),
                ),
            ],
        )
        .unwrap_err();
    assert_eq!(r.code, "InvalidInput");
    assert_eq!(commits(&fixture), 3);
    fixture.cleanup();
}
