//! The typed names (#536 §9.2): each kind is checked once with today's
//! refusals and texts, and a project and a Workspace print as the calls
//! write them.
use layerfs_control::common::names::{
    BucketName, MachineName, ProjectName, ProjectRef, SandboxName, StoreName, WorkspaceRef,
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
fn a_workspace_is_its_sandbox_and_path() {
    let ws = WorkspaceRef::parse("box:/w/1").unwrap();
    assert_eq!((ws.sandbox.as_str(), ws.path.as_str()), ("box", "/w/1"));
    assert_eq!(ws.to_string(), "box:/w/1");
    for bad in ["box", ":/w", "box:w"] {
        let refusal = WorkspaceRef::parse(bad).unwrap_err();
        assert_eq!(refusal.code, "InvalidInput");
        assert_eq!(
            refusal.message,
            format!("a Workspace is <sandbox>:<path>, not {bad:?}")
        );
    }
}
