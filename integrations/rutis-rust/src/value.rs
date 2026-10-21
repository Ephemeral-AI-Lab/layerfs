//! JSON values and the SDK's `SdkValue`.
//!
//! Tier 0 crosses serde JSON; text, integers and structured data work. Binary
//! bytes have no JSON form here and are returned as an array of numbers; the
//! public SDK's text methods do not need them.

use crate::error::LayerFsError;
use layerfs_bridge::control::SdkValue;
use serde_json::{Map, Value};

const DEPTH: usize = 16;
const SAFE_INTEGER: u64 = 9_007_199_254_740_991;

pub(crate) fn to_sdk(value: Value) -> Result<SdkValue, LayerFsError> {
    convert(value, 0)
}

fn convert(value: Value, depth: usize) -> Result<SdkValue, LayerFsError> {
    if depth > DEPTH {
        return Err(LayerFsError::invalid("value nests too deeply"));
    }
    Ok(match value {
        Value::Null => SdkValue::Null,
        Value::Bool(value) => SdkValue::Bool(value),
        Value::Number(number) => {
            let Some(value) = number.as_i64() else {
                return Err(LayerFsError::invalid(
                    "numbers must be signed 64-bit integers",
                ));
            };
            if value.unsigned_abs() > SAFE_INTEGER {
                return Err(LayerFsError::invalid(
                    "numbers must be within the JavaScript safe integer range",
                ));
            }
            SdkValue::Int(value)
        }
        Value::String(text) => SdkValue::Text(text),
        Value::Array(items) => SdkValue::List(
            items
                .into_iter()
                .map(|item| convert(item, depth + 1))
                .collect::<Result<_, _>>()?,
        ),
        Value::Object(fields) => SdkValue::Map(
            fields
                .into_iter()
                .map(|(name, value)| Ok((name, convert(value, depth + 1)?)))
                .collect::<Result<_, LayerFsError>>()?,
        ),
    })
}

/// Converts a trusted core result. Values from the core are already bounded by
/// the SDK channel; conversion cannot fail after a mutation has committed.
pub(crate) fn to_json(value: &SdkValue) -> Value {
    match value {
        SdkValue::Null => Value::Null,
        SdkValue::Bool(value) => Value::Bool(*value),
        SdkValue::Int(value) => Value::Number((*value).into()),
        SdkValue::Text(text) => Value::String(text.clone()),
        SdkValue::Bytes(bytes) => Value::Array(
            bytes
                .iter()
                .map(|byte| Value::Number((*byte).into()))
                .collect(),
        ),
        SdkValue::List(items) => Value::Array(items.iter().map(to_json).collect()),
        SdkValue::Map(fields) => {
            let mut map = Map::new();
            for (name, value) in fields {
                map.insert(name.clone(), to_json(value));
            }
            Value::Object(map)
        }
    }
}
