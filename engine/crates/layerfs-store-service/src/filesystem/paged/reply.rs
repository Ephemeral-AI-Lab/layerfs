//! Reserve saved-reference/header space before returning a bounded text window.
use super::{
    super::{facts, text::window},
    query,
};
use crate::lookup as args;
use layerfs_protocol::sdk::SdkValue;

fn set(fields: &mut Vec<(String, SdkValue)>, name: &str, value: SdkValue) {
    match fields.iter_mut().find(|(key, _)| key == name) {
        Some((_, v)) => *v = value,
        None => fields.push((name.to_owned(), value)),
    }
}

/// Whole numbered pages refuse on overflow; a text window can end earlier.
pub fn finish(mut fields: Vec<(String, SdkValue)>) -> args::Answer {
    let mut answer = SdkValue::Map(fields.clone());
    if facts::value_bytes(&answer) <= window::BYTES {
        return Ok(answer);
    }
    let Some(text) = answer.get_text("text").map(str::to_owned) else {
        return Err(query::capacity("saved response exceeds reply budget"));
    };
    let start = answer.get_int("startLine").unwrap_or(1);
    let boundaries = std::iter::once(0)
        .chain(text.char_indices().skip(1).map(|(i, _)| i))
        .chain(std::iter::once(text.len()))
        .collect::<Vec<_>>();
    let (mut low, mut high) = (0usize, boundaries.len());
    while low + 1 < high {
        let mid = (low + high) / 2;
        set(
            &mut fields,
            "text",
            SdkValue::text(&text[..boundaries[mid]]),
        );
        if facts::value_bytes(&SdkValue::Map(fields.clone())) + 256 <= window::BYTES {
            low = mid;
        } else {
            high = mid;
        }
    }
    let mut end = boundaries[low];
    if let Some(newline) = text[..end].rfind('\n') {
        end = newline + 1;
    }
    if end == 0 && !text.is_empty() {
        return Err(query::capacity("saved text line cannot fit reply metadata"));
    }
    let shown = &text[..end];
    let count = shown.split_inclusive('\n').count() as i64;
    let partial = !shown.ends_with('\n') && end < text.len();
    set(&mut fields, "text", SdkValue::text(shown));
    set(
        &mut fields,
        "endLine",
        if count == 0 {
            SdkValue::Null
        } else {
            SdkValue::Int(start + count - 1)
        },
    );
    // a next line exists past what is shown in the window's text, or past
    // the window itself; a cut last line has none after it
    let rest = &text[end..];
    let more = match partial {
        true => rest.find('\n').is_some_and(|at| at + 1 < rest.len()),
        false => !rest.is_empty(),
    };
    let next = match more || answer.get_int("nextLine").is_some() {
        true => SdkValue::Int(start + count),
        false => SdkValue::Null,
    };
    set(&mut fields, "nextLine", next);
    set(
        &mut fields,
        "lineTruncated",
        SdkValue::Bool(partial || answer.get_bool("lineTruncated")),
    );
    set(&mut fields, "truncated", SdkValue::Bool(true));
    answer = SdkValue::Map(fields);
    if facts::value_bytes(&answer) > window::BYTES {
        return Err(query::capacity("saved text response exceeds reply budget"));
    }
    Ok(answer)
}
