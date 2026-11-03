//! The typed names (#536 §9.2): each kind is checked once with today's
//! refusals and texts, and a project and a Workspace print as the calls
//! write them.
use layerfs_control::common::names::{
    BucketName, MachineName, ProjectName, ProjectRef, SandboxName, StoreName, WorkspaceAt,
    WorkspaceName, WorkspaceRef,
};

#[test]
fn each_kind_refuses_with_its_own_word() {
    assert_eq!(StoreName::parse("main").unwrap().as_str(), "main");
    let cases: [(&str, String); 5] = [
        ("store", StoreName::parse("Main").unwrap_err().message),
        ("machine", MachineName::parse("-x").unwrap_err().message),
        ("project", ProjectName::parse("").unwrap_err().message),
        ("sandbox", SandboxName::parse("a b").unwrap_err().message),
        ("bucket", BucketName::parse("x.").unwrap_err().message),
    ];
    for (what, message) in cases {
        assert!(
            message.starts_with(&format!("{what} name `")),
            "{what}: {message}"
        );
        assert!(message.ends_with(
            "must be 1-63 lowercase letters, digits, _ . -, starting and ending with a letter or digit"
        ));
    }
    let reserved = SandboxName::parse("com1").unwrap_err();
    assert_eq!(reserved.code, "InvalidInput");
    assert_eq!(reserved.message, "sandbox name `com1` is reserved");
}

#[test]
fn a_project_is_its_store_and_name() {
    let project = ProjectRef::parse("main/app").unwrap();
    assert_eq!(project.store.as_str(), "main");
    assert_eq!(project.project.as_str(), "app");
    assert_eq!(project.to_string(), "main/app");
    let bare = ProjectRef::parse("app").unwrap_err();
    assert_eq!(bare.code, "InvalidInput");
    assert_eq!(
        bare.message,
        "project `app` must be written with its Store, as <store>/<project>"
    );
    assert_eq!(
        ProjectRef::parse("Main/app").unwrap_err().message,
        "Store name `Main` must be 1-63 lowercase letters, digits, _ . -, starting and ending with a letter or digit"
    );
}

#[test]
fn a_workspace_is_its_sandbox_and_path_or_name() {
    let ws = WorkspaceRef::parse("box:/w/1").unwrap();
    assert_eq!(ws.sandbox, "box");
    assert_eq!(ws.at, WorkspaceAt::Path("/w/1".to_owned()));
    assert_eq!(ws.to_string(), "box:/w/1");
    let ws = WorkspaceRef::parse("box:dev").unwrap();
    assert_eq!(
        ws.at,
        WorkspaceAt::Name(WorkspaceName::parse("dev").unwrap())
    );
    assert_eq!(
        (ws.at.as_str(), ws.to_string().as_str()),
        ("dev", "box:dev")
    );
    for bad in ["box", ":/w", "box:"] {
        let refusal = WorkspaceRef::parse(bad).unwrap_err();
        assert_eq!(refusal.code, "InvalidInput");
        assert_eq!(
            refusal.message,
            format!("a Workspace is <sandbox>:<path> or <sandbox>:<name>, not {bad:?}")
        );
    }
    // a name follows the name rules
    let refusal = WorkspaceRef::parse("box:Dev").unwrap_err();
    assert_eq!(refusal.code, "InvalidInput");
    assert!(
        refusal.message.starts_with("workspace name `Dev`"),
        "{}",
        refusal.message
    );
}
