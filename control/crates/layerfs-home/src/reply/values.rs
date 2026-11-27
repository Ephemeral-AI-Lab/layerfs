//! Values every entity's answer is built from (moved from
//! `sandbox/registry.rs`, layerfs-project#103): a required text argument,
//! an answer's list with its `<list>_count` companion, and a map of ordered
//! fields.
use crate::reply::outcome::refused;
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};

/// A required text field `name` of a map (`value`, a record's or a frame's).
pub fn text<'a>(value: &'a SdkValue, name: &str) -> Result<&'a str, SdkRefusal> {
    value
        .get_text(name)
        .ok_or_else(|| refused("InvalidInput", format!("missing text argument `{name}`")))
}

/// A required text field `name` of a typed request or record: refused as
/// [`text`] refuses a missing argument.
pub fn required<'a>(value: Option<&'a str>, name: &str) -> Result<&'a str, SdkRefusal> {
    value.ok_or_else(|| refused("InvalidInput", format!("missing text argument `{name}`")))
}

/// The entries an answer's list holds (call surface 2.0 SCHEMA): a list
/// keeps its first `SHOWN`.
pub const SHOWN: usize = 20;

/// `items` as an answer's list and its `<list>_count` companion: the first
/// [`SHOWN`] entries, and the count only when the list was cut (SCHEMA:
/// "`<list>_count` only beside a cut list"), else null. `total` is how many
/// there are when the source knows more than `items` holds (at least
/// `items.len()` is used); `cut` says the source itself left some out
/// without knowing how many, so the count is then what is known.
pub fn shown(mut items: Vec<SdkValue>, total: usize, cut: bool) -> (SdkValue, SdkValue) {
    let total = total.max(items.len());
    items.truncate(SHOWN);
    let count = match total > items.len() || cut {
        true => SdkValue::Int(i64::try_from(total).unwrap_or(i64::MAX)),
        false => SdkValue::Null,
    };
    (SdkValue::List(items), count)
}

/// `names` as an answer's list of text and its `<list>_count` companion, as
/// [`shown`] makes them.
pub fn shown_names(names: &[String]) -> (SdkValue, SdkValue) {
    shown(names.iter().map(SdkValue::text).collect(), 0, false)
}

/// A map of `fields`, in their order.
pub fn map_of(fields: Vec<(&str, SdkValue)>) -> SdkValue {
    SdkValue::Map(
        fields
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value))
            .collect(),
    )
}
