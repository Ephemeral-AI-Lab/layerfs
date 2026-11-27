//! The one cursor of the call surface (2.0): the opaque `next_cursor` a
//! paged call answers and takes back as `--cursor`. It carries the call
//! that made it, the place its next page starts from, the filters that
//! call was given and, where the call pins a Commit, that pin: base64 text
//! (unpadded) of one flat JSON map `{ for, at, <filter or pin>... }`.
//!
//! A cursor given to another call is refused `InvalidInput`, and so is one
//! given with a filter or pin that differs from what it carries; a filter
//! left off on the later call is the one the cursor carries. What a pin
//! names is the caller's to read: one that is gone is its `NotFound`.
use crate::json;
use crate::outcome::refused;
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};

/// The key of the call a cursor belongs to.
const FOR: &str = "for";
/// The key of the place its next page starts from.
const AT: &str = "at";

/// The rows of one page when no limit is given (`list_shape`).
pub const DEFAULT_LIMIT: i64 = 100;
/// The most rows one page may ask for.
pub const MAX_LIMIT: i64 = 1000;

/// `limit` as given, checked: 1 to [`MAX_LIMIT`], `default` when absent.
pub fn limit(call: &str, limit: Option<i64>, default: i64) -> Result<i64, SdkRefusal> {
    let limit = limit.unwrap_or(default);
    match (1..=MAX_LIMIT).contains(&limit) {
        true => Ok(limit),
        false => Err(refused(
            "InvalidInput",
            format!("{call}: `limit` is 1 to {MAX_LIMIT}"),
        )),
    }
}

/// The cursor of a page of `call` whose next page starts at `place`, with
/// `fields`: the call's filters as given (none left out but those not
/// given) and its pins.
pub fn make(call: &str, place: SdkValue, fields: &[(&str, SdkValue)]) -> String {
    let mut map = vec![
        (FOR.to_owned(), SdkValue::text(call)),
        (AT.to_owned(), place),
    ];
    let given = fields.iter().filter(|(_, value)| *value != SdkValue::Null);
    map.extend(given.map(|(name, value)| ((*name).to_owned(), value.clone())));
    json::base64(json::print(&SdkValue::Map(map)).as_bytes())
        .trim_end_matches('=')
        .to_owned()
}

/// What a cursor carries: the place its next page starts from and the
/// filters and pins of the call that made it.
#[derive(Clone, Debug)]
pub struct Carried(SdkValue);

impl Carried {
    /// The place the next page starts from.
    pub fn place(&self) -> &SdkValue {
        self.0.get(AT).unwrap_or(&SdkValue::Null)
    }

    /// The filter or pin `name` it carries; null when that call had none.
    pub fn get(&self, name: &str) -> SdkValue {
        self.0.get(name).cloned().unwrap_or_default()
    }

    /// The text filter or pin `name` it carries.
    pub fn text(&self, name: &str) -> Option<&str> {
        self.0.get_text(name)
    }
}

/// The cursor `text` of `call`, read and checked against the filters and
/// pins `given` on this call: each one given must be the one it carries.
pub fn resume(call: &str, text: &str, given: &[(&str, SdkValue)]) -> Result<Carried, SdkRefusal> {
    let padded = format!("{text}{}", "=".repeat((4 - text.len() % 4) % 4));
    let made = json::unbase64(&padded)
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .and_then(|text| json::parse(&text).ok())
        .filter(|value| value.get_text(FOR) == Some(call))
        .ok_or_else(|| {
            refused(
                "InvalidInput",
                format!("{call}: `cursor` is not the `next_cursor` of a {call} page"),
            )
        })?;
    let carried = Carried(made);
    let differs = (given.iter())
        .filter(|(_, value)| *value != SdkValue::Null)
        .any(|(name, value)| carried.get(name) != *value);
    if differs {
        return Err(refused(
            "InvalidInput",
            format!(
                "{call}: the cursor carries the filters of the call that made it; give the same filters or none"
            ),
        ));
    }
    Ok(carried)
}
