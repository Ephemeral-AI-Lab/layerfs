//! A call's arguments checked against its table entry before any effect:
//! every name known, every required positional present, every value of its
//! kind. Sizes become whole bytes. Front ends strip their functions
//! (`on_progress`, exec sinks) before the call reaches the library.
use super::table::{Call, Kind};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_control::common::outcome::refused;

/// `args` as the call's checked fields, in the table's order.
pub fn check(call: &Call, args: SdkValue) -> Result<Vec<(String, SdkValue)>, SdkRefusal> {
    let fields = match args {
        SdkValue::Map(fields) => fields,
        SdkValue::Null => Vec::new(),
        _ => return Err(invalid(call, "arguments must be named fields")),
    };
    for (name, _) in &fields {
        match call.arg(name) {
            Some(arg) if arg.kind != Kind::Function => {}
            _ => return Err(invalid(call, &format!("unknown argument `{name}`"))),
        }
    }
    let mut out = Vec::with_capacity(fields.len());
    for arg in call.args.iter().chain(call.options) {
        let value = fields
            .iter()
            .find(|(name, _)| name == arg.name)
            .map(|(_, value)| value.clone())
            .unwrap_or(SdkValue::Null);
        if matches!(value, SdkValue::Null) {
            if !arg.optional {
                return Err(invalid(call, &format!("`{}` is required", arg.name)));
            }
            continue;
        }
        let value = kind(call, arg.name, arg.kind, value)?;
        out.push((arg.name.to_owned(), value));
    }
    Ok(out)
}

fn kind(call: &Call, name: &str, kind: Kind, value: SdkValue) -> Result<SdkValue, SdkRefusal> {
    let ok = match (&kind, &value) {
        (Kind::Name | Kind::Text, SdkValue::Text(_)) => true,
        (Kind::Project, SdkValue::Text(text)) => {
            crate::names::project(text).map_err(|refusal| refused("InvalidInput", refusal))?;
            true
        }
        (Kind::Int, SdkValue::Int(_)) | (Kind::Bool, SdkValue::Bool(_)) => true,
        (Kind::Json, _) => true,
        (Kind::Content, SdkValue::Text(_) | SdkValue::Bytes(_)) => true,
        (Kind::Size, SdkValue::Int(n)) => *n >= 0,
        (Kind::Size, SdkValue::Text(text)) => {
            let bytes = size(text)
                .ok_or_else(|| invalid(call, &format!("`{name}` is a size such as 512M or 8G")))?;
            return Ok(SdkValue::Int(bytes));
        }
        _ => false,
    };
    if ok {
        Ok(value)
    } else {
        Err(invalid(
            call,
            &format!("`{name}` must be {}", describe(kind)),
        ))
    }
}

/// Bytes from `512`, `64K`, `512M` or `8G` (powers of 1024).
pub fn size(text: &str) -> Option<i64> {
    let text = text.trim();
    let (digits, unit) = match text.char_indices().find(|(_, c)| !c.is_ascii_digit()) {
        Some((i, _)) => text.split_at(i),
        None => (text, ""),
    };
    let n: i64 = digits.parse().ok()?;
    let scale: i64 = match unit {
        "" => 1,
        "K" | "k" => 1 << 10,
        "M" | "m" => 1 << 20,
        "G" | "g" => 1 << 30,
        _ => return None,
    };
    n.checked_mul(scale)
}

fn describe(kind: Kind) -> &'static str {
    match kind {
        Kind::Name | Kind::Text => "text",
        Kind::Project => "a project, <store>/<project>",
        Kind::Int => "a whole number",
        Kind::Bool => "true or false",
        Kind::Json => "JSON",
        Kind::Content => "text or bytes",
        Kind::Size => "a size such as 512M or 8G",
        Kind::Function => "a function",
    }
}

fn invalid(call: &Call, why: &str) -> SdkRefusal {
    refused("InvalidInput", format!("{}: {why}", call.name()))
}
