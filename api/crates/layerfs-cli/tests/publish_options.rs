//! `branch.publish`'s expected-base options are generated from the call
//! table: the shell accepts them and its usage line names them (#691).
use layerfs_bridge::control::SdkValue;

fn publish() -> &'static layerfs_api::Call {
    layerfs_api::calls()
        .into_iter()
        .find(|call| call.family == "branch" && call.call == "publish")
        .expect("branch.publish is in the table")
}

#[test]
fn the_shell_reads_expected_base_expected_head_and_force() {
    let base = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let head = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
    let words: Vec<String> = [
        "shop/api",
        "work",
        "--expected-base",
        base,
        "--expected-head",
        head,
        "--force",
    ]
    .iter()
    .map(|word| (*word).to_owned())
    .collect();
    let parsed = layerfs_cli::parse::parse(publish(), &words, &mut std::io::empty())
        .ok()
        .expect("the options parse");
    assert_eq!(parsed.args.get_text("expected_base"), Some(base));
    assert_eq!(parsed.args.get_text("expected_head"), Some(head));
    assert!(
        matches!(parsed.args.get("force"), Some(SdkValue::Bool(true))),
        "{:?}",
        parsed.args
    );
    assert_eq!(
        layerfs_cli::help::usage(publish()),
        "layerfs branch publish <project> <branch> [<commit>] [--expected-base <expected-base>] [--expected-head <expected-head>] [--force]"
    );
}
