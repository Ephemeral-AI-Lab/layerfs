//! `branch.publish`'s `--at`, `--expected` and `--force` are generated from
//! the call table: the shell accepts them, its usage line names them, and a
//! malformed value is a usage error naming the flag (#691; 2.0 call
//! surface).
use layerfs_api::Call;
use layerfs_protocol::sdk::SdkValue;

fn publish() -> &'static Call {
    layerfs_api::dispatch()
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

const MAIN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const AT: &str = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";

#[test]
fn the_shell_reads_at_expected_and_force() {
    let parsed = parse(&[
        "shop/api",
        "work",
        "--at",
        AT,
        "--expected",
        MAIN,
        "--force",
    ])
    .expect("the options parse");
    assert_eq!(parsed.args.get_text("at"), Some(AT));
    assert_eq!(parsed.args.get_text("expected"), Some(MAIN));
    assert!(
        matches!(parsed.args.get("force"), Some(SdkValue::Bool(true))),
        "{:?}",
        parsed.args
    );
    assert_eq!(
        layerfs_cli::help::usage(publish()),
        "layerfs branch publish <project> <branch> [--at C] [--expected C] [--force]"
    );
}

#[test]
fn the_options_take_inline_values_and_a_boolean_force() {
    // `--flag=value` is the same option as `--flag value`, and a boolean
    // `--force` carries no value of its own.
    let inline = parse(&[
        "shop/api",
        "work",
        &format!("--at={AT}"),
        &format!("--expected={MAIN}"),
        "--force=false",
    ])
    .expect("the inline options parse");
    assert_eq!(inline.args.get_text("at"), Some(AT));
    assert_eq!(inline.args.get_text("expected"), Some(MAIN));
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
        usage_error(&["shop/api", "work", "--expected"]),
        "--expected needs a value"
    );
    assert_eq!(
        usage_error(&["shop/api", "work", "--force=yes"]),
        "branch.publish: `force` is true or false"
    );
    // the old options are gone
    let unknown = usage_error(&["shop/api", "work", &format!("--expected-base={MAIN}")]);
    assert!(
        unknown.starts_with("branch.publish: unknown option --expected-base\naccepted options: "),
        "{unknown}"
    );
    // The refusal lists the flags the call accepts.
    for flag in ["--at", "--expected", "--force"] {
        assert!(unknown.contains(flag), "{flag}: {unknown}");
    }
}
