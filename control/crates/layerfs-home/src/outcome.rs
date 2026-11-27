//! What one host operation answers: a value, or a refusal with a stable
//! code. Moved from layerfs-node's job.rs (#533 step 2). The public
//! refusal codes, the fold of every other code into them and what a refusal
//! carries for the caller's next step (`fix`, flat fields) are declared
//! here once (call surface 2.0).
use crate::home::with_field;
use layerfs_protocol::provision::install::{SdkRefusal, SdkValue};

/// What one operation answers.
pub type Outcome = Result<SdkValue, SdkRefusal>;

/// A refusal with a stable code.
pub fn refused(code: &str, message: impl Into<String>) -> SdkRefusal {
    SdkRefusal {
        code: code.to_owned(),
        message: message.into(),
        detail: SdkValue::Null,
    }
}

/// A failure that has no SDK code of its own, in the error's own words
/// (its `Display`): no public answer carries Rust `Debug` text.
pub fn failed(error: impl std::fmt::Display) -> SdkRefusal {
    refused("Failed", error.to_string())
}

/// Which shape of an entity a call answers (#536 §9.7): a get's whole
/// record, or one item of a list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    /// What the entity's get answers.
    Full,
    /// One item of the entity's list.
    List,
}

/// The thirteen refusal codes a caller can meet, the closed list of the
/// call surface.
pub const CODES: [&str; 13] = [
    "NotFound",
    "Exists",
    "InUse",
    "Busy",
    "Unresolved",
    "Conflict",
    "Dirty",
    "Capacity",
    "InvalidInput",
    "Unsupported",
    "Unavailable",
    "Failed",
    "Unknown",
];

/// The codes the control and engine crates refuse with that are not public:
/// each public code and the codes folded into it at the call boundary.
pub const FOLDS: [(&str, &[&str]); 9] = [
    (
        "NotFound",
        &[
            "NoSuchStore",
            "NoSuchProject",
            "NoSuchBranch",
            "NoSuchCommit",
            "NoSuchPath",
            "NoSuchSandbox",
            "NoStore",
            "NoCommit",
            "NotInHistory",
            "NotMounted",
            "Removed",
            "NoMatch",
        ],
    ),
    ("Exists", &["NameTaken"]),
    ("InUse", &["MountsActive"]),
    ("Conflict", &["HeadMoved"]),
    (
        "InvalidInput",
        &[
            "NotFile",
            "IsDirectory",
            "NotDirectory",
            "NotEmpty",
            "Binary",
            "ReservedName",
            "ReadOnly",
            "NoMergeBase",
            "TooLarge",
            "AmbiguousMatch",
        ],
    ),
    ("Capacity", &["NoSpace"]),
    ("Unsupported", &["OldStore"]),
    (
        "Unavailable",
        &[
            "Denied",
            "Unauthorized",
            "StaleSandbox",
            "StaleStore",
            "Timeout",
        ],
    ),
    ("Failed", &["UnknownMethod", "Cancelled"]),
];

/// The public code `code` is answered as: itself when it is one of
/// [`CODES`], the code [`FOLDS`] folds it into, and `Failed` for a code
/// neither names.
pub fn public_code(code: &str) -> &'static str {
    if let Some(public) = CODES.iter().find(|public| **public == code) {
        return public;
    }
    FOLDS
        .iter()
        .find(|(_, folded)| folded.contains(&code))
        .map_or("Failed", |(public, _)| public)
}

/// The key of a refusal's detail that holds its fix line.
pub const FIX: &str = "fix";
/// The key of a refusal's detail that holds the fields the next call needs.
pub const NEXT: &str = "next";

/// `refusal` with its `fix`: one runnable shell line (two commands joined
/// with `&&`) that the call boundary answers beside `error` and `message`.
pub fn with_fix(mut refusal: SdkRefusal, fix: impl Into<String>) -> SdkRefusal {
    refusal.detail = with_field(refusal.detail, FIX, SdkValue::Text(fix.into()));
    refusal
}

/// `text` as one shell word in a fix line: as it is when a shell takes
/// every character literally, else single-quoted.
pub fn shell_word(text: &str) -> String {
    let plain = |c: char| c.is_ascii_alphanumeric() || "/._-+:@%,=".contains(c);
    match !text.is_empty() && text.chars().all(plain) {
        true => text.to_owned(),
        false => format!("'{}'", text.replace('\'', "'\\''")),
    }
}

/// `refusal` with one field the caller's next call needs, answered flat
/// beside `error` and `message`: the head as `commit` on a `Conflict`, the
/// `op` on a `Busy` or `Unresolved`, the `paths` on a `Dirty`.
pub fn with_next(mut refusal: SdkRefusal, name: &str, value: SdkValue) -> SdkRefusal {
    let next = match refusal.detail.get(NEXT) {
        Some(next @ SdkValue::Map(_)) => next.clone(),
        _ => SdkValue::Map(Vec::new()),
    };
    refusal.detail = with_field(refusal.detail, NEXT, with_field(next, name, value));
    refusal
}
