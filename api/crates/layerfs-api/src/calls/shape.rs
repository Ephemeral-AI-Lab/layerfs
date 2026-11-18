//! The one place that makes a migrated call's answer (call surface 2.0).
//! A run function returns one full map; this makes the long form (every
//! declared key, in order, null where the run function gave none) or the
//! short form (the long form projected onto the short keys, with null,
//! false and empty values left out), and adds the sections the caller
//! asked for. It reads nothing: the long form shows what was already read.
//!
//! A key the entry does not declare is a bug in its run function: it
//! panics in a debug build, so the author sees it, and is dropped in a
//! release build. What a declared section would add is not such a key: when
//! the section was not asked for it is left out without a word.
use super::form::{Asked, Form, Part, Place, Section, ITEMS, OK};
use layerfs_bridge::control::SdkValue;

type Fields = Vec<(String, SdkValue)>;

/// Whether a short form leaves `value` out: null, false, or an empty text,
/// list or map.
fn empty(value: &SdkValue) -> bool {
    match value {
        SdkValue::Null | SdkValue::Bool(false) => true,
        SdkValue::Text(text) => text.is_empty(),
        SdkValue::List(items) => items.is_empty(),
        SdkValue::Map(fields) => fields.is_empty(),
        _ => false,
    }
}

/// `value` as a short form shows it where no form says more: every map
/// inside it without its null, false and empty fields.
fn pruned(value: SdkValue) -> SdkValue {
    match value {
        SdkValue::Map(fields) => SdkValue::Map(
            fields
                .into_iter()
                .map(|(name, value)| (name, pruned(value)))
                .filter(|(_, value)| !empty(value))
                .collect(),
        ),
        SdkValue::List(items) => SdkValue::List(items.into_iter().map(pruned).collect()),
        other => other,
    }
}

/// Whether `value` under the `sometimes` key `key` is its usual value, so
/// not news: `state` when ready (a list row leaves it out), and what the
/// form says is usual.
fn usual(form: &Form, key: &str, value: &SdkValue) -> bool {
    let SdkValue::Text(text) = value else {
        return false;
    };
    (key == "state" && text == "ready")
        || (form.usual.iter()).any(|(name, word)| *name == key && *word == text)
}

fn take(fields: &mut Fields, key: &str) -> Option<SdkValue> {
    let at = fields.iter().position(|(name, _)| name == key)?;
    Some(fields.remove(at).1)
}

/// What is left of a map once its declared keys were taken is undeclared.
fn undeclared(what: &str, left: &Fields) {
    if cfg!(debug_assertions) && !left.is_empty() {
        let keys: Vec<&str> = left.iter().map(|(name, _)| name.as_str()).collect();
        panic!(
            "{what}: the answer holds `{}`, which its table entry does not declare",
            keys.join("`, `")
        );
    }
}

/// One shaping: the call it is for (for a bug's message), the form asked
/// and what the sections asked for add.
struct Shaping<'a> {
    what: &'a str,
    detail: bool,
    /// The sections asked for.
    asked: Vec<&'a Section>,
    /// The keys the sections asked for put flat into each item.
    flat: Vec<&'a str>,
    /// What a section not asked for would have added to the top level, and
    /// to each item: a run function may answer it anyway, and it is left
    /// out without a word.
    spare_top: Vec<&'a str>,
    spare_item: Vec<&'a str>,
}

/// Where a map sits in the answer.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Level {
    /// The call's whole answer.
    Top,
    /// One row of its `items`.
    Item,
    /// A map inside either, or inside a section.
    Inner,
}

impl Shaping<'_> {
    /// The value under a key that has its own form.
    fn part(&self, part: &Part, value: SdkValue, level: Level) -> SdkValue {
        match (part, value) {
            (Part::Rows(form), SdkValue::List(rows)) => SdkValue::List(
                rows.into_iter()
                    .map(|row| match row {
                        SdkValue::Map(fields) => self.map(form, fields, level),
                        other => other,
                    })
                    .collect(),
            ),
            (Part::Rows(form), SdkValue::Map(fields)) => self.map(form, fields, level),
            (_, other) => other,
        }
    }

    /// One map by its form.
    fn map(&self, form: &Form, mut fields: Fields, level: Level) -> SdkValue {
        let top = level == Level::Top;
        let mut out: Fields = Vec::new();
        for key in form.long {
            let short = form.always.contains(key) || form.sometimes.contains(key);
            let given = take(&mut fields, key);
            if !self.detail && !short {
                continue;
            }
            let mut value = given.unwrap_or_default();
            if *key == OK && top && value == SdkValue::Null {
                value = SdkValue::Bool(true);
            }
            let below = match (top, *key == ITEMS) {
                (true, true) => Level::Item,
                _ => Level::Inner,
            };
            let value = match form.part(key) {
                Some(part) => self.part(part, value, below),
                None if self.detail => value,
                None => pruned(value),
            };
            let kept = self.detail
                || match form.always.contains(key) {
                    true => value != SdkValue::Null,
                    false => !empty(&value) && !usual(form, key, &value),
                };
            if kept {
                out.push(((*key).to_owned(), value));
            }
        }
        // an item takes the flat keys of the sections asked for; the top
        // level takes those of the sections that answer there
        let (asked, flat): (&[&Section], &[&str]) = match level {
            Level::Top => (&self.asked, &[]),
            Level::Item => (&[], &self.flat),
            Level::Inner => (&[], &[]),
        };
        let at_top = asked.iter().filter(|section| section.place == Place::Top);
        let flat_keys = flat
            .iter()
            .chain(at_top.flat_map(|section| section.keys.iter()));
        // a section key the form lists too (`machine list --with live`'s
        // `observed_at`) was answered in its place above
        for key in flat_keys.filter(|key| !form.long.contains(key)) {
            let value = take(&mut fields, key).unwrap_or_default();
            let value = if self.detail { value } else { pruned(value) };
            if self.detail || !empty(&value) {
                out.push(((*key).to_owned(), value));
            }
        }
        for section in asked.iter().filter(|section| section.place == Place::Own) {
            let value = match take(&mut fields, section.name).unwrap_or_default() {
                SdkValue::Map(inner) => match section.form {
                    Some(form) => self.map(form, inner, Level::Inner),
                    None => self.keyed(section, inner),
                },
                other => other,
            };
            if self.detail || value != SdkValue::Null {
                out.push((section.name.to_owned(), value));
            }
        }
        let spare: &[&str] = match level {
            Level::Top => &self.spare_top,
            Level::Item => &self.spare_item,
            Level::Inner => &[],
        };
        fields.retain(|(name, _)| !spare.contains(&name.as_str()));
        undeclared(self.what, &fields);
        if !self.detail {
            if top && out.len() > 1 {
                out.retain(|(name, _)| name != OK);
            }
            if top && out.is_empty() {
                out.push((OK.to_owned(), SdkValue::Bool(true)));
            }
            if !top && out.is_empty() {
                // an item never becomes `{"ok":true}`: it keeps what names it
                out.extend(
                    form.always
                        .iter()
                        .map(|key| ((*key).to_owned(), SdkValue::Null)),
                );
            }
        }
        SdkValue::Map(out)
    }

    /// A section's own map when it has no form: exactly its keys.
    fn keyed(&self, section: &Section, mut fields: Fields) -> SdkValue {
        let mut out: Fields = Vec::new();
        for key in section.keys {
            let value = take(&mut fields, key).unwrap_or_default();
            let value = if self.detail { value } else { pruned(value) };
            if self.detail || !empty(&value) {
                out.push(((*key).to_owned(), value));
            }
        }
        undeclared(self.what, &fields);
        SdkValue::Map(out)
    }
}

/// The answer of the call named `what` (for a bug's message) from the full
/// map `value` its run function returned: its long form when `asked.detail`,
/// else its short form, with the sections of `sections` that were asked for.
/// What a section not asked for would have added is left out, so a run
/// function that has it at no cost may answer it anyway.
pub fn shape(
    what: &str,
    form: &Form,
    sections: &[Section],
    value: SdkValue,
    asked: &Asked,
) -> SdkValue {
    let fields = match value {
        SdkValue::Map(fields) => fields,
        _ => Vec::new(),
    };
    let (taken, spare): (Vec<&Section>, Vec<&Section>) = sections
        .iter()
        .partition(|section| asked.with(section.name));
    let keys = |of: &[&Section], place: Place| -> Vec<&str> {
        of.iter()
            .filter(|section| section.place == place)
            .flat_map(|section| section.keys.iter().copied())
            .collect()
    };
    let mut spare_top = keys(&spare, Place::Top);
    let own = spare.iter().filter(|section| section.place == Place::Own);
    spare_top.extend(own.map(|section| section.name));
    let shaping = Shaping {
        what,
        detail: asked.detail,
        flat: keys(&taken, Place::Item),
        spare_item: keys(&spare, Place::Item),
        spare_top,
        asked: taken,
    };
    shaping.map(form, fields, Level::Top)
}
