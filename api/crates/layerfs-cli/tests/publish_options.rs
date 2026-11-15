//! `branch.publish`'s expected-base options are generated from the call
//! table: the shell accepts them, its usage line names them, and a malformed
//! value is a usage error naming the flag (#691).
use layerfs_api::Call;
use layerfs_bridge::control::SdkValue;

fn publish() -> &'static Call {
    layerfs_api::calls()
        .into_iter()
        .find(|call| call.family == "branch" && call.call == "publish")
        .expect("branch.publish is in the table")
}

fn parse(words: &[&str]) -> Result<layerfs_cli::parse::Parsed, String> {
    let words: Vec<String> = words.iter().map(|word| (*word).to_owned()).collect();
    layerfs_cli::parse::parse(publish(), &words, &mut std::io::empty())
        .map_err(|why| why.0.to_owned())
}

/// The usage error `words` are refused with (`Parsed` has no `Debug`, so
/// `unwrap_err` cannot be used).
fn usage_error(words: &[&str]) -> String {
    match parse(words) {
        Err(why) => why,
        Ok(_) => panic!("{words:?} must be a usage error"),
    }
}

const BASE: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const HEAD: &str = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";

#[test]
fn the_shell_reads_expected_base_expected_head_and_force() {
    let parsed = parse(&[
        "shop/api",
        "work",
        "--expected-base",
        BASE,
        "--expected-head",
        HEAD,
        "--force",
    ])
    .expect("the options parse");
    assert_eq!(parsed.args.get_text("expected_base"), Some(BASE));
    assert_eq!(parsed.args.get_text("expected_head"), Some(HEAD));
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

#[test]
fn the_options_take_inline_values_and_a_boolean_force() {
    // `--flag=value` is the same option as `--flag value`, and a boolean
    // `--force` carries no value of its own.
    let inline = parse(&[
        "shop/api",
        "work",
        &format!("--expected-base={BASE}"),
        &format!("--expected-head={HEAD}"),
        "--force=false",
    ])
    .expect("the inline options parse");
    assert_eq!(inline.args.get_text("expected_base"), Some(BASE));
    assert_eq!(inline.args.get_text("expected_head"), Some(HEAD));
    assert!(
        matches!(inline.args.get("force"), Some(SdkValue::Bool(false))),
        "{:?}",
        inline.args
    );

    // Options may come before the positionals: `--force` consumes no word.
    let ordered = parse(&["--force", "shop/api", "work"]).expect("the flag parses");
    assert!(
        matches!(ordered.args.get("force"), Some(SdkValue::Bool(true))),
        "{:?}",
        ordered.args
    );
}

#[test]
fn a_malformed_expectation_is_a_usage_error() {
    assert_eq!(
        usage_error(&["shop/api", "work", "--expected-base"]),
        "--expected-base needs a value"
    );
    assert_eq!(
        usage_error(&["shop/api", "work", "--force=yes"]),
        "branch.publish: `force` is true or false"
    );
    let unknown = usage_error(&["shop/api", "work", &format!("--expected-bass={BASE}")]);
    assert!(
        unknown.starts_with("branch.publish: unknown option --expected-bass\naccepted options: "),
        "{unknown}"
    );
    // The refusal lists the flags the call accepts, including the new ones.
    for flag in ["--expected-base", "--expected-head", "--force"] {
        assert!(unknown.contains(flag), "{flag}: {unknown}");
    }
}
