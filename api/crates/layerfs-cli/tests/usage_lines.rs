//! Every usage line shows its options as the call's row in the frozen
//! specification does (conformance R2 G1): the same value word or default
//! (`[--at C]`, `[--limit 100]`), `-m msg` for a Commit message, and a
//! required option without brackets.
use layerfs_protocol::provision::install::SdkValue;

const SPEC: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../core/docs/issues/call-surface-2-0/calls.json"
);

fn list(value: Option<&SdkValue>) -> Vec<SdkValue> {
    match value {
        Some(SdkValue::List(items)) => items.clone(),
        _ => Vec::new(),
    }
}

/// The `--flag WORD` and `-m WORD` pairs of a command line, each with
/// whether it stood in brackets.
fn options(line: &str) -> Vec<(String, String, bool)> {
    let words: Vec<&str> = line.split_whitespace().collect();
    let mut out = Vec::new();
    for (at, word) in words.iter().enumerate() {
        let bracketed = word.starts_with('[');
        let flag = word.trim_start_matches('[').trim_end_matches(']');
        if !(flag.starts_with("--") || flag == "-m") || word.ends_with(']') {
            continue;
        }
        let Some(value) = words.get(at + 1) else {
            continue;
        };
        if value.starts_with('-') || value.starts_with('[') || value.starts_with('<') {
            continue;
        }
        out.push((
            flag.to_owned(),
            value
                .trim_end_matches("]...")
                .trim_end_matches(']')
                .to_owned(),
            bracketed,
        ));
    }
    out
}

#[test]
fn usage_lines_show_the_rows_value_words() {
    let text = std::fs::read_to_string(SPEC).expect("the frozen spec");
    let spec = layerfs_home::json::parse(&text).expect("calls.json");
    let mut checked = 0;
    for family in list(spec.get("families")) {
        for row in list(family.get("calls")) {
            let name = row.get_text("call").unwrap();
            let Some(call) = layerfs_api::find(name) else {
                continue;
            };
            let usage = layerfs_cli::help::usage(call);
            let have = options(&usage);
            for want in options(row.get_text("cli").unwrap_or("")) {
                if want.0 == "--with" || want.0 == "--env" {
                    continue;
                }
                assert!(have.contains(&want), "{name}: {want:?} not in `{usage}`");
                checked += 1;
            }
        }
    }
    assert!(checked > 100, "{checked} options checked");
}
