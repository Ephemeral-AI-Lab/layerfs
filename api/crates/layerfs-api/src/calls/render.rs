//! A call as one line of the call log (#659 §4, §5): its `argv`, the CLI
//! words that would make the same call, rendered from its table entry and
//! its checked arguments before the scope rewrites them; and the saved
//! version its result names. Free text and credentials are logged only as
//! byte counts (each argument's entry says so: `redact`), and no other
//! result value is kept.
use super::form::Asked;
use super::table::{Arg, Call, Kind, Redact};
use super::Scope;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::outcome::Outcome;

/// A path argument's logged byte length: the §4.3 normalization (strip one
/// leading `/`) applied before counting.
fn path_len(text: &str) -> usize {
    text.strip_prefix('/').unwrap_or(text).len()
}

/// The most characters a logged value keeps.
const CUT: usize = 200;

fn cut(text: &str) -> String {
    match text.char_indices().nth(CUT) {
        Some((at, _)) => format!("{}…", &text[..at]),
        None => text.to_owned(),
    }
}

fn bytes(n: usize) -> SdkValue {
    SdkValue::Text(format!("<{n} bytes>"))
}

/// One argument's value as a word, or none when it is left out. Content
/// and JSON are logged only as a byte count; so is an argument its entry
/// marks `redact` (exec command text, Commit messages, edit strings, Store
/// passwords, connection URLs that may embed `user:pass@`, a glob or grep
/// pattern), and a path argument as the count of its normalized form, so a
/// `project.hide_path` or `file.*` path never enters the call log (#41
/// §4.3).
fn word(arg: &Arg, value: &SdkValue) -> Option<SdkValue> {
    Some(match (arg.kind, value) {
        (Kind::Function, _) => return None,
        (Kind::Content, SdkValue::Text(text)) => bytes(text.len()),
        (Kind::Content, SdkValue::Bytes(raw)) => bytes(raw.len()),
        (Kind::Content | Kind::Json, value) => bytes(layerfs_home::json::print(value).len()),
        (_, SdkValue::Text(text)) if arg.redact == Redact::Path => bytes(path_len(text)),
        (_, SdkValue::Text(text)) if arg.redact == Redact::Bytes => bytes(text.len()),
        (_, SdkValue::Text(text)) => SdkValue::Text(cut(text)),
        (_, value) if arg.redact != Redact::No => bytes(layerfs_home::json::print(value).len()),
        (_, SdkValue::Int(n)) => SdkValue::Text(n.to_string()),
        (_, SdkValue::Bool(b)) => SdkValue::Text(b.to_string()),
        (_, value) => bytes(layerfs_home::json::print(value).len()),
    })
}

fn flag(name: &str) -> String {
    format!("--{}", name.replace('_', "-"))
}

/// `family` and `call` (with `-` for `_`): the whole `argv` of a call
/// refused before its arguments were checked.
pub fn bare(name: &str) -> Vec<SdkValue> {
    match name.split_once('.') {
        Some((family, call)) => vec![
            SdkValue::Text(cut(family)),
            SdkValue::Text(cut(&call.replace('_', "-"))),
        ],
        None => vec![SdkValue::Text(cut(name))],
    }
}

/// The call's `argv` from its checked arguments (in table order): its
/// positionals while present, then each option as `--name value` (a Bool
/// option as a bare flag when true, a secret as its stdin flag and byte
/// count), then `--detail` and `--with` as asked,
/// then the folder scope.
pub fn argv(spec: &Call, args: &SdkValue, asked: &Asked, scope: Option<&Scope>) -> Vec<SdkValue> {
    let mut words = bare(&spec.name());
    for arg in spec.args {
        match args.get(arg.name) {
            None | Some(SdkValue::Null) => break,
            Some(value) => words.extend(word(arg, value)),
        }
    }
    for option in spec.options {
        let value = match args.get(option.name) {
            None | Some(SdkValue::Null) => continue,
            Some(value) => value,
        };
        if option.kind == Kind::Bool {
            if matches!(value, SdkValue::Bool(true)) {
                words.push(SdkValue::Text(flag(option.name)));
            }
            continue;
        }
        if let Some(value) = word(option, value) {
            // a secret goes by the shell flag that reads it from stdin
            // (`--key-stdin`), followed by its byte count
            let named = option
                .shell
                .map_or_else(|| flag(option.name), str::to_owned);
            words.push(SdkValue::Text(named));
            words.push(value);
        }
    }
    if asked.detail {
        words.push(SdkValue::text("--detail"));
    }
    if !asked.with.is_empty() {
        words.push(SdkValue::text("--with"));
        words.push(SdkValue::Text(cut(&asked.with.join(","))));
    }
    if let Some(scope) = scope {
        words.push(SdkValue::text("--scope"));
        words.push(SdkValue::Text(cut(&scope.folder)));
        words.push(SdkValue::text("--scope-project"));
        words.push(SdkValue::Text(cut(&scope.project)));
        if let Some(branch) = &scope.branch {
            words.push(SdkValue::text("--scope-branch"));
            words.push(SdkValue::Text(cut(branch)));
        }
    }
    words
}

/// Whether `text` is a Commit or Layer id: 66 lowercase hex digits.
pub(super) fn is_id(text: &str) -> bool {
    text.len() == 66
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// `value` when it is a Commit or Layer id ([`is_id`]).
fn id(value: Option<&SdkValue>) -> Option<String> {
    match value {
        Some(SdkValue::Text(text)) if is_id(text) => Some(text.clone()),
        _ => None,
    }
}

/// The result's word and the saved version it names (§4.2), as the line's
/// `kind` and `commit`: a call's full result names its word as `outcome`
/// and, for a call that saves, its Commit as `commit` (a read's `commit`
/// is no saved version).
pub fn saved(spec: &Call, value: &SdkValue) -> Vec<(String, SdkValue)> {
    let mut fields = Vec::new();
    if let Some(word) = value.get_text("outcome") {
        fields.push(("kind".to_owned(), SdkValue::Text(cut(word))));
    }
    match id(value.get("commit")) {
        Some(commit) if spec.saves => {
            fields.push(("commit".to_owned(), SdkValue::Text(commit)));
        }
        _ => {}
    }
    fields
}

/// One line: `{ v, at, ms, client, argv, outcome, kind?, commit?, layer?,
/// tag? }`. `outcome` is `ok` or the refusal's code; `saved` is what
/// [`saved`] found in the call's full result.
pub fn line(
    at: i64,
    ms: i64,
    client: &str,
    argv: Vec<SdkValue>,
    outcome: &Outcome,
    saved: Vec<(String, SdkValue)>,
    tag: Option<&str>,
) -> SdkValue {
    let mut fields = vec![
        ("v".to_owned(), SdkValue::Int(1)),
        ("at".to_owned(), SdkValue::Int(at)),
        ("ms".to_owned(), SdkValue::Int(ms)),
        ("client".to_owned(), SdkValue::text(client)),
        ("argv".to_owned(), SdkValue::List(argv)),
    ];
    match outcome {
        Ok(_) => {
            fields.push(("outcome".to_owned(), SdkValue::text("ok")));
            fields.extend(saved);
        }
        Err(SdkRefusal { code, .. }) => {
            fields.push(("outcome".to_owned(), SdkValue::Text(cut(code))));
        }
    }
    if let Some(tag) = tag {
        fields.push(("tag".to_owned(), SdkValue::text(tag)));
    }
    SdkValue::Map(fields)
}
