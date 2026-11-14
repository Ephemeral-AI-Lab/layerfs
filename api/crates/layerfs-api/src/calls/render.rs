//! A call as one line of the call log (#659 §4, §5): its `argv`, the CLI
//! words that would make the same call, rendered from its table entry and
//! its checked arguments before the scope rewrites them; and the saved
//! version its result names. Free text and credentials are logged only as
//! byte counts, and no other result value is kept.
use super::table::{Call, Kind};
use super::Scope;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::outcome::Outcome;

/// The `Text` arguments logged only as `<N bytes>`: exec command text,
/// Commit messages, edit strings, Store passwords and connection URLs that
/// may embed `user:pass@`. A new call with such an argument adds its name.
pub const REDACTED: [&str; 6] = ["command", "message", "old", "new", "password", "url"];

/// The most characters a logged value keeps.
const CUT: usize = 200;

/// The calls whose `Committed` or `UpToDate` result names a Commit.
const SAVING: [&str; 8] = [
    "file.write",
    "file.edit",
    "file.apply",
    "file.rm",
    "file.mkdir",
    "file.mv",
    "file.cp",
    "workspace.commit",
];

fn cut(text: &str) -> String {
    match text.char_indices().nth(CUT) {
        Some((at, _)) => format!("{}…", &text[..at]),
        None => text.to_owned(),
    }
}

fn bytes(n: usize) -> SdkValue {
    SdkValue::Text(format!("<{n} bytes>"))
}

/// One argument's value as a word, or none when it is left out.
fn word(name: &str, kind: Kind, value: &SdkValue) -> Option<SdkValue> {
    Some(match (kind, value) {
        (Kind::Function, _) => return None,
        (Kind::Content, SdkValue::Text(text)) => bytes(text.len()),
        (Kind::Content, SdkValue::Bytes(raw)) => bytes(raw.len()),
        (Kind::Content | Kind::Json, value) => bytes(layerfs_home::json::print(value).len()),
        (Kind::Text, SdkValue::Text(text)) if REDACTED.contains(&name) => bytes(text.len()),
        (_, SdkValue::Text(text)) => SdkValue::Text(cut(text)),
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
/// option as a bare flag when true), then the folder scope.
pub fn argv(spec: &Call, args: &SdkValue, scope: Option<&Scope>) -> Vec<SdkValue> {
    let mut words = bare(&spec.name());
    for arg in spec.args {
        match args.get(arg.name) {
            None | Some(SdkValue::Null) => break,
            Some(value) => words.extend(word(arg.name, arg.kind, value)),
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
        if let Some(value) = word(option.name, option.kind, value) {
            words.push(SdkValue::Text(flag(option.name)));
            words.push(value);
        }
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

/// A Commit or Layer id: 66 lowercase hex digits.
fn id(value: Option<&SdkValue>) -> Option<String> {
    match value {
        Some(SdkValue::Text(text))
            if text.len() == 66
                && text
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) =>
        {
            Some(text.clone())
        }
        _ => None,
    }
}

/// The result's `kind` and the saved version it names (§4.2): `commit` or
/// `layer`, only for the calls and kinds that save or name one.
pub fn saved(name: &str, value: &SdkValue) -> Vec<(String, SdkValue)> {
    let Some(kind) = value.get_text("kind") else {
        return Vec::new();
    };
    let inner = |outer: &str| value.get(outer).and_then(|v| v.get("id"));
    let either = |commit: Option<String>, layer: Option<String>| match (commit, layer) {
        (Some(commit), _) => Some(("commit", commit)),
        (None, Some(layer)) => Some(("layer", layer)),
        _ => None,
    };
    let saving = SAVING.contains(&name);
    let version = match (name, kind) {
        (_, "Committed") if saving => either(id(inner("commit")).or(id(value.get("head"))), None),
        (_, "UpToDate") if saving => either(id(value.get("head")), id(value.get("base"))),
        ("branch.fork", "Forked") => either(id(value.get("head")), id(value.get("base"))),
        ("branch.merge", "Merged") => either(id(value.get("head")), None),
        ("branch.publish", "Added") => either(None, id(inner("published_layer"))),
        ("branch.publish", "UpToDate") => either(None, id(value.get("layer"))),
        ("branch.publish", "NoChanges") => either(None, id(value.get("head_layer"))),
        _ => None,
    };
    let mut fields = vec![("kind".to_owned(), SdkValue::Text(cut(kind)))];
    if let Some((field, id)) = version {
        fields.push((field.to_owned(), SdkValue::Text(id)));
    }
    fields
}

/// One line: `{ v, at, ms, client, argv, outcome, kind?, commit?, layer?,
/// tag? }`.
pub fn line(
    at: i64,
    ms: i64,
    client: &str,
    argv: Vec<SdkValue>,
    name: &str,
    outcome: &Outcome,
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
        Ok(value) => {
            fields.push(("outcome".to_owned(), SdkValue::text("ok")));
            fields.extend(saved(name, value));
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
