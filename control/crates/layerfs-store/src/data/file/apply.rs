//! `file.apply`: several changes as one Commit, all or none, each change
//! checked and named as the Store takes it.
use super::requests::FileApply;
use super::write::{base, commit, mode};
use super::Change;
use layerfs_home::events::Events;
use layerfs_home::outcome::{refused, Outcome};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::path::Path;

/// The fields a change of `file.apply` may have.
const CHANGE_FIELDS: [&str; 10] = [
    "op",
    "path",
    "from",
    "to",
    "content",
    "old_string",
    "new_string",
    "replace_all",
    "mode",
    "recursive",
];

/// A `mode` that is not text, named by what it was given as, never as
/// Rust `Debug` text.
fn not_text_mode(value: &SdkValue) -> SdkRefusal {
    let given = match value {
        SdkValue::Int(number) => format!("the number {number} (give it as text)"),
        SdkValue::Bool(value) => format!("{value}"),
        SdkValue::Bytes(_) => "bytes".to_owned(),
        SdkValue::List(_) => "a list".to_owned(),
        SdkValue::Map(_) => "a map".to_owned(),
        SdkValue::Text(_) | SdkValue::Null => "text".to_owned(),
    };
    refused(
        "InvalidInput",
        format!("`mode` is octal text of four digits, 0000 to 7777, such as 0644; not {given}"),
    )
}

/// One change of `file.apply` as the Store takes it: its fields under the
/// Store's names, its mode a number, and a removal that is not recursive
/// unless it says so.
pub(crate) fn change(change: &Change) -> Result<SdkValue, SdkRefusal> {
    owned_change(change.fields.clone())
}

/// [`change`] of fields the caller gives up: each value is moved, never
/// copied (a streamed `file apply` holds one change at a time).
pub(crate) fn owned_change(input: Vec<(String, SdkValue)>) -> Result<SdkValue, SdkRefusal> {
    let mut fields = Vec::with_capacity(input.len() + 1);
    for (name, value) in input {
        if !CHANGE_FIELDS.contains(&name.as_str()) {
            return Err(refused(
                "InvalidInput",
                format!(
                    "`{name}` is not a change field; a change uses {}",
                    CHANGE_FIELDS.join(", ")
                ),
            ));
        }
        let field = match (name.as_str(), value) {
            ("content", value @ SdkValue::Bytes(_)) => ("bytes", value),
            ("old_string", value) => ("oldText", value),
            ("new_string", value) => ("newText", value),
            ("replace_all", value) => ("replaceAll", value),
            ("mode", SdkValue::Text(text)) => match mode(Some(&text))? {
                Some(bits) => ("mode", SdkValue::Int(bits)),
                None => ("mode", SdkValue::Null),
            },
            ("mode", SdkValue::Null) => ("mode", SdkValue::Null),
            ("mode", other) => return Err(not_text_mode(&other)),
            (other, value) => (other, value),
        };
        fields.push((field.0.to_owned(), field.1));
    }
    let removal =
        (fields.iter()).any(|(name, value)| name == "op" && *value == SdkValue::text("rm"));
    if removal && !fields.iter().any(|(name, _)| name == "recursive") {
        fields.push(("recursive".to_owned(), SdkValue::Bool(false)));
    }
    Ok(SdkValue::Map(fields))
}

/// `file.apply`: several changes as one Commit, all or none.
pub fn apply(home: &Path, events: &mut Events<'_>, request: &FileApply) -> Outcome {
    let changes = (request.changes.iter())
        .map(change)
        .collect::<Result<Vec<_>, _>>()?;
    let mut args = base(&request.branch, &request.guard);
    args.put("changes", SdkValue::List(changes));
    commit(home, events, &request.project, "fs.apply", args)
}
