//! The `sandbox.host` frame (layerfs-project#108): built from the typed
//! request in the order the untyped request had, read back to the same
//! request, and refused before any effect when it names no host operation.
use layerfs_bridge::control::SdkValue;
use layerfs_home::json::{parse, print};
use layerfs_sandbox::deploy::Choice;
use layerfs_sandbox::host::{decode, encode, Call, Message, Op, Sent, OPS};
use layerfs_store::project::record::ProjectRecord;
use layerfs_store::record::StoreRecord;

fn sent(call: Call, message: Message) -> Sent {
    Sent {
        call,
        message,
        sandbox: Some("t1".to_owned()),
        store: "shop".to_owned(),
        project: "api".to_owned(),
        store_record: StoreRecord::from_record(
            "shop",
            parse(r#"{"name":"shop","url":"postgresql://u@h/d","password":"env:P"}"#).unwrap(),
        ),
        project_record: ProjectRecord::from_record("shop", "api", SdkValue::Null),
        choice: Choice {
            image: None,
            platform: Some("docker".to_owned()),
            archive: None,
        },
        postgresql: None,
        machine: None,
        reach_hint: None,
    }
}

#[test]
fn a_frame_keeps_the_untyped_request_s_fields_and_order() {
    let exec = Call::Exec {
        path: Some("/w/1".to_owned()),
        directory: None,
        command: Some("ls".to_owned()),
        spool: false,
        exec_id: None,
        workspace: None,
    };
    assert_eq!(
        print(&encode(&sent(exec, Message::Null)).unwrap()),
        r#"{"op":"workspace.exec","store":"shop","project":"api","store_record":{"name":"shop","url":"postgresql://u@h/d","password":"env:P"},"project_record":null,"deploy":{"backend":"local","platform":"docker"},"more":{"path":"/w/1","command":"ls","message":null,"sandbox":"t1"}}"#
    );
    let close = Call::Close {
        force: false,
        held: Some(vec!["/w/2".to_owned()]),
    };
    let frame = print(&encode(&sent(close, Message::Absent)).unwrap());
    assert!(
        frame.ends_with(r#""more":{"force":false,"held":["/w/2"],"sandbox":"t1"}}"#),
        "{frame}"
    );
    let commit = Call::Commit {
        path: Some("w".to_owned()),
        force: Some(true),
    };
    let frame = print(&encode(&sent(commit, Message::Text("m".to_owned()))).unwrap());
    assert!(
        frame.ends_with(r#""more":{"path":"w","message":"m","force":true,"sandbox":"t1"}}"#),
        "{frame}"
    );
}

#[test]
fn a_frame_reads_back_to_its_request() {
    let calls = [
        Call::Create { platform: None },
        Call::Close {
            force: true,
            held: None,
        },
        Call::ListWorkspaces { path: None },
        Call::Capacity,
        Call::Mount {
            branch: Some("dev".to_owned()),
            name: Some("w2".to_owned()),
        },
        Call::Exec {
            path: Some("/w".to_owned()),
            directory: Some("/w".to_owned()),
            command: Some("true".to_owned()),
            spool: true,
            exec_id: Some("e1".to_owned()),
            workspace: Some("ab".repeat(32)),
        },
        Call::ExecCancel {
            path: Some("w2".to_owned()),
            exec_id: Some("e1".to_owned()),
            grace_ms: Some(250),
            workspace: None,
        },
        Call::Commit {
            path: Some("/w".to_owned()),
            force: None,
        },
        Call::Unmount {
            path: Some("/w".to_owned()),
            discard: Some(false),
            force: Some(true),
        },
        Call::Status {
            path: Some("w2".to_owned()),
        },
    ];
    assert_eq!(calls.len(), OPS.len());
    for call in calls {
        let read = decode(&encode(&sent(call.clone(), Message::Absent)).unwrap()).unwrap();
        assert_eq!(read.call, call);
        assert_eq!(Op::parse(call.op().name()), Some(call.op()));
        assert_eq!(read.sandbox.as_deref(), Some("t1"));
        assert_eq!(read.message, Message::Absent);
        assert_eq!(read.choice.platform.as_deref(), Some("docker"));
        // the URL and its password reference stay with the caller
        let record = print(read.store_record.to_record());
        assert_eq!(record, r#"{"name":"shop"}"#);
    }
}

#[test]
fn a_frame_naming_no_host_operation_is_refused() {
    let refusal = decode(&parse(r#"{"store":"shop"}"#).unwrap())
        .err()
        .unwrap();
    assert_eq!(
        (refusal.code.as_str(), refusal.message.as_str()),
        ("InvalidInput", "operation name")
    );
    let refusal = decode(&parse(r#"{"op":"store.call"}"#).unwrap())
        .err()
        .unwrap();
    assert_eq!(
        refusal.message,
        "the worker runs no Sandbox operation store.call"
    );
    // a message that is not text is kept, to be refused where it was
    let frame = parse(r#"{"op":"workspace.mount","more":{"message":3}}"#).unwrap();
    assert_eq!(decode(&frame).unwrap().message, Message::NotText);
}

#[test]
fn a_message_that_is_not_text_is_refused_before_it_is_sent() {
    let commit = Call::Commit {
        path: Some("w".to_owned()),
        force: None,
    };
    let refusal = encode(&sent(commit, Message::NotText)).err().unwrap();
    assert_eq!(
        (refusal.code.as_str(), refusal.message.as_str()),
        ("InvalidInput", "message must be text")
    );
}

/// A PostgreSQL Store's selection travels after the deployment, with the
/// machine and hint its reach check names, and reads back as sent.
#[test]
fn a_frame_carries_the_postgresql_selection() {
    let mut with = sent(Call::Status { path: None }, Message::Absent);
    with.postgresql = Some(layerfs_store::open_args::PgSelection {
        url: Some("postgresql://u:pw@h/d".to_owned()),
        schema: Some("layerfs".to_owned()),
        synchronous_commit: Some("off".to_owned()),
    });
    with.machine = Some("m1".to_owned());
    let frame = encode(&with).unwrap();
    assert!(print(&frame).contains(
        r#""deploy":{"backend":"local","platform":"docker"},"postgresql":{"url":"postgresql://u:pw@h/d","schema":"layerfs","synchronousCommit":"off"},"machine":"m1","more":{"sandbox":"t1"}}"#
    ));
    let back = decode(&frame).unwrap().postgresql.unwrap();
    assert_eq!(
        print(&back.encode()),
        print(&frame.get("postgresql").cloned().unwrap())
    );
}
