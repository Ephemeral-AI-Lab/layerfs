//! The SDK call channel: one named method with a self-describing argument
//! value, answered by a value or a typed refusal.
//!
//! The JavaScript SDK is the only public SDK and its arguments and results are
//! plain objects, so its calls travel as a small value tree instead of one
//! record type per method. The tree is bounded like every control record: at
//! most [`MAXIMUM_VALUE_DEPTH`] levels, and the whole record within the
//! control limit. A refusal carries a stable code (for example `NoSuchPath`),
//! a message for people and a detail value; the code alone decides.
use crate::{
    control::ControlError,
    wire::{Reader, Writer},
};

/// Deepest nesting of lists and maps.
pub const MAXIMUM_VALUE_DEPTH: usize = 16;
/// Longest method name or refusal code.
pub const MAXIMUM_NAME_BYTES: usize = 64;

/// One self-describing value.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub enum SdkValue {
    /// No value.
    #[default]
    Null,
    /// A boolean.
    Bool(bool),
    /// A signed integer.
    Int(i64),
    /// UTF-8 text.
    Text(String),
    /// Bytes.
    Bytes(Vec<u8>),
    /// An ordered list.
    List(Vec<SdkValue>),
    /// Named fields in order; names are unique.
    Map(Vec<(String, SdkValue)>),
}

/// One SDK call.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SdkCall {
    /// Method, for example `branch.merge`.
    pub method: String,
    /// Arguments, normally a map.
    pub args: SdkValue,
}

/// A refused SDK call.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SdkRefusal {
    /// Stable code, for example `NoSuchPath`.
    pub code: String,
    /// Message for people.
    pub message: String,
    /// Structured detail.
    pub detail: SdkValue,
}

impl SdkValue {
    /// A map from fields.
    pub fn map<const N: usize>(fields: [(&str, SdkValue); N]) -> Self {
        Self::Map(
            fields
                .into_iter()
                .map(|(name, value)| (name.to_owned(), value))
                .collect(),
        )
    }

    /// Text.
    pub fn text(value: impl Into<String>) -> Self {
        Self::Text(value.into())
    }

    /// Optional text, `Null` when absent.
    pub fn opt_text(value: Option<impl Into<String>>) -> Self {
        value.map_or(Self::Null, |value| Self::Text(value.into()))
    }

    /// One field of a map.
    pub fn get(&self, name: &str) -> Option<&SdkValue> {
        match self {
            Self::Map(fields) => fields
                .iter()
                .find(|(field, _)| field == name)
                .map(|(_, value)| value),
            _ => None,
        }
    }

    /// The text of a field.
    pub fn get_text(&self, name: &str) -> Option<&str> {
        match self.get(name) {
            Some(Self::Text(text)) => Some(text),
            _ => None,
        }
    }

    /// The boolean of a field; absent or null is false.
    pub fn get_bool(&self, name: &str) -> bool {
        matches!(self.get(name), Some(Self::Bool(true)))
    }

    /// The integer of a field.
    pub fn get_int(&self, name: &str) -> Option<i64> {
        match self.get(name) {
            Some(Self::Int(value)) => Some(*value),
            _ => None,
        }
    }
}

pub(crate) fn put_value(
    out: &mut Writer,
    value: &SdkValue,
    depth: usize,
) -> Result<(), ControlError> {
    if depth > MAXIMUM_VALUE_DEPTH {
        return Err(ControlError("SDK value depth"));
    }
    match value {
        SdkValue::Null => out.byte(0),
        SdkValue::Bool(value) => out.byte(if *value { 2 } else { 1 }),
        SdkValue::Int(value) => {
            out.byte(3)?;
            out.put(&value.to_be_bytes())
        }
        SdkValue::Text(value) => {
            out.byte(4)?;
            put_long(out, value.as_bytes())
        }
        SdkValue::Bytes(value) => {
            out.byte(5)?;
            put_long(out, value)
        }
        SdkValue::List(items) => {
            out.byte(6)?;
            out.put(&(items.len() as u32).to_be_bytes())?;
            items
                .iter()
                .try_for_each(|item| put_value(out, item, depth + 1))
        }
        SdkValue::Map(fields) => {
            out.byte(7)?;
            out.put(&(fields.len() as u32).to_be_bytes())?;
            for (name, value) in fields {
                if name.len() > MAXIMUM_NAME_BYTES {
                    return Err(ControlError("SDK field name"));
                }
                out.blob(name.as_bytes())?;
                put_value(out, value, depth + 1)?;
            }
            Ok(())
        }
    }
}

/// A length-prefixed run that may exceed one `u16` blob.
fn put_long(out: &mut Writer, bytes: &[u8]) -> Result<(), ControlError> {
    let size = u32::try_from(bytes.len()).map_err(|_| ControlError("SDK field limit"))?;
    out.put(&size.to_be_bytes())?;
    out.put(bytes)
}

fn long<'a>(input: &mut Reader<'a>) -> Result<&'a [u8], ControlError> {
    let size = u32::from_be_bytes(input.array()?) as usize;
    input.take(size)
}

pub(crate) fn value(input: &mut Reader<'_>, depth: usize) -> Result<SdkValue, ControlError> {
    if depth > MAXIMUM_VALUE_DEPTH {
        return Err(ControlError("SDK value depth"));
    }
    Ok(match input.byte()? {
        0 => SdkValue::Null,
        1 => SdkValue::Bool(false),
        2 => SdkValue::Bool(true),
        3 => SdkValue::Int(i64::from_be_bytes(input.array()?)),
        4 => SdkValue::Text(
            std::str::from_utf8(long(input)?)
                .map_err(|_| ControlError("control UTF-8"))?
                .to_owned(),
        ),
        5 => SdkValue::Bytes(long(input)?.to_vec()),
        6 => {
            let count = u32::from_be_bytes(input.array()?);
            let mut items = Vec::new();
            for _ in 0..count {
                items.push(value(input, depth + 1)?);
            }
            SdkValue::List(items)
        }
        7 => {
            let count = u32::from_be_bytes(input.array()?);
            let mut fields: Vec<(String, SdkValue)> = Vec::new();
            for _ in 0..count {
                let name = input.text(MAXIMUM_NAME_BYTES)?;
                if fields.iter().any(|(field, _)| *field == name) {
                    return Err(ControlError("SDK field repeated"));
                }
                let item = value(input, depth + 1)?;
                fields.push((name, item));
            }
            SdkValue::Map(fields)
        }
        _ => return Err(ControlError("SDK value kind")),
    })
}

pub(crate) fn put_call(out: &mut Writer, call: &SdkCall) -> Result<(), ControlError> {
    if call.method.is_empty() || call.method.len() > MAXIMUM_NAME_BYTES {
        return Err(ControlError("SDK method"));
    }
    out.blob(call.method.as_bytes())?;
    put_value(out, &call.args, 0)
}

pub(crate) fn call(input: &mut Reader<'_>) -> Result<SdkCall, ControlError> {
    let method = input.text(MAXIMUM_NAME_BYTES)?;
    if method.is_empty() {
        return Err(ControlError("SDK method"));
    }
    Ok(SdkCall {
        method,
        args: value(input, 0)?,
    })
}

pub(crate) fn put_refusal(out: &mut Writer, refusal: &SdkRefusal) -> Result<(), ControlError> {
    if refusal.code.is_empty() || refusal.code.len() > MAXIMUM_NAME_BYTES {
        return Err(ControlError("SDK refusal code"));
    }
    out.blob(refusal.code.as_bytes())?;
    put_long(out, refusal.message.as_bytes())?;
    put_value(out, &refusal.detail, 0)
}

pub(crate) fn refusal(input: &mut Reader<'_>) -> Result<SdkRefusal, ControlError> {
    let code = input.text(MAXIMUM_NAME_BYTES)?;
    if code.is_empty() {
        return Err(ControlError("SDK refusal code"));
    }
    let message = std::str::from_utf8(long(input)?)
        .map_err(|_| ControlError("control UTF-8"))?
        .to_owned();
    Ok(SdkRefusal {
        code,
        message,
        detail: value(input, 0)?,
    })
}
