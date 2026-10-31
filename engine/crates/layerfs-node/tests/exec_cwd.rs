//! Pure validation and joining for the binding's internal workspace.exec cwd.
use layerfs_api::internal::exec_directory;

#[test]
fn omitted_and_root_cwd_select_the_workspace() {
    assert_eq!(
        exec_directory("/workspaces/w1", None).unwrap(),
        "/workspaces/w1"
    );
    assert_eq!(
        exec_directory("/workspaces/w1", Some("/")).unwrap(),
        "/workspaces/w1"
    );
}

#[test]
fn canonical_cwd_joins_beneath_the_workspace() {
    assert_eq!(
        exec_directory("/workspaces/w1", Some("/packages/editor")).unwrap(),
        "/workspaces/w1/packages/editor"
    );
    assert_eq!(
        exec_directory("/workspaces/w1", Some("/packages/editor/src")).unwrap(),
        "/workspaces/w1/packages/editor/src"
    );
}

#[test]
fn noncanonical_cwd_is_invalid_input() {
    for cwd in [
        "",
        "relative",
        "/packages/",
        "/packages//editor",
        "/packages/./editor",
        "/packages/../editor",
        ".",
        "..",
        "//",
    ] {
        let refusal = exec_directory("/workspaces/w1", Some(cwd)).unwrap_err();
        assert_eq!(refusal.code, "InvalidInput", "cwd {cwd:?}");
        assert!(
            refusal.message.starts_with("workspace.exec cwd"),
            "cwd {cwd:?}"
        );
    }
}

#[test]
fn joined_directory_is_contained_by_component_boundary() {
    let joined = exec_directory("/workspaces/w1", Some("/packages/editor")).unwrap();
    assert!(std::path::Path::new(&joined).starts_with("/workspaces/w1"));
    assert!(!std::path::Path::new(&joined).starts_with("/workspaces/w10"));
}
