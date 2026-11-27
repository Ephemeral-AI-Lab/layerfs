//! Result keys in snake_case for every front end (S §2.1): `exit_code`, not
//! 1.1.0's `exitCode`. Only identifier-like keys that start with a lowercase
//! letter change; a key holding a path or another name never does.
use layerfs_protocol::provision::install::SdkValue;

/// `value` with every field name in snake_case.
pub fn snake(value: SdkValue) -> SdkValue {
    match value {
        SdkValue::Map(fields) => SdkValue::Map(
            fields
                .into_iter()
                .map(|(name, value)| (snake_name(&name), snake(value)))
                .collect(),
        ),
        SdkValue::List(items) => SdkValue::List(items.into_iter().map(snake).collect()),
        other => other,
    }
}

/// `exitCode` as `exit_code`; a key that is not a camelCase identifier is
/// returned unchanged.
pub fn snake_name(name: &str) -> String {
    let identifier = name.starts_with(|c: char| c.is_ascii_lowercase())
        && name.chars().all(|c| c.is_ascii_alphanumeric());
    if !identifier || !name.chars().any(|c| c.is_ascii_uppercase()) {
        return name.to_owned();
    }
    let mut out = String::with_capacity(name.len() + 4);
    for c in name.chars() {
        if c.is_ascii_uppercase() {
            out.push('_');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}
