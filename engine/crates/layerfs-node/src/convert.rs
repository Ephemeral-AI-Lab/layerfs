//! JavaScript values to and from SDK values.
//!
//! Objects become maps (undefined fields are left out), arrays lists, strings
//! text, Buffers bytes, safe integers integers. Node's buffer check accepts
//! any typed array or DataView, so each becomes the bytes it views. Anything
//! else, a number that is not a safe integer and nesting deeper than the SDK
//! allows, is refused. A bare ArrayBuffer is an object without fields here;
//! the package turns every binary write body into a Buffer before the call.
use layerfs_bridge::control::SdkValue;
use napi::{Env, JsBuffer, JsObject, JsUnknown, Result, ValueType};

/// Deepest value the SDK channel carries.
const DEPTH: usize = 16;
/// Largest integer a JavaScript number holds exactly.
const SAFE: f64 = 9_007_199_254_740_991.0;

fn invalid(message: &str) -> napi::Error {
    napi::Error::from_reason(format!("InvalidInput: {message}"))
}

/// One JavaScript value as an SDK value.
pub(crate) fn from_js(value: JsUnknown) -> Result<SdkValue> {
    convert(value, 0)
}

fn convert(value: JsUnknown, depth: usize) -> Result<SdkValue> {
    if depth > DEPTH {
        return Err(invalid("value nests too deeply"));
    }
    Ok(match value.get_type()? {
        ValueType::Null | ValueType::Undefined => SdkValue::Null,
        ValueType::Boolean => SdkValue::Bool(value.coerce_to_bool()?.get_value()?),
        ValueType::Number => {
            let number = value.coerce_to_number()?.get_double()?;
            if number.fract() != 0.0 || number.abs() > SAFE {
                return Err(invalid("numbers must be safe integers"));
            }
            SdkValue::Int(number as i64)
        }
        ValueType::String => SdkValue::Text(value.coerce_to_string()?.into_utf8()?.into_owned()?),
        ValueType::Object if value.is_buffer()? => {
            let buffer = JsBuffer::try_from(value)?;
            SdkValue::Bytes(buffer.into_value()?.to_vec())
        }
        ValueType::Object => {
            let object = value.coerce_to_object()?;
            if object.is_array()? {
                let length = object.get_array_length()?;
                let mut items = Vec::with_capacity(length as usize);
                for index in 0..length {
                    items.push(convert(object.get_element::<JsUnknown>(index)?, depth + 1)?);
                }
                SdkValue::List(items)
            } else {
                map(&object, depth)?
            }
        }
        _ => return Err(invalid("functions, symbols and bigints are not values")),
    })
}

fn map(object: &JsObject, depth: usize) -> Result<SdkValue> {
    let names = object.get_property_names()?;
    let mut fields = Vec::new();
    for index in 0..names.get_array_length()? {
        let name = names
            .get_element::<JsUnknown>(index)?
            .coerce_to_string()?
            .into_utf8()?
            .into_owned()?;
        let field = object.get_named_property::<JsUnknown>(&name)?;
        if field.get_type()? != ValueType::Undefined {
            fields.push((name, convert(field, depth + 1)?));
        }
    }
    Ok(SdkValue::Map(fields))
}

/// One SDK value as a JavaScript value.
pub(crate) fn to_js(env: &Env, value: &SdkValue) -> Result<JsUnknown> {
    Ok(match value {
        SdkValue::Null => env.get_null()?.into_unknown(),
        SdkValue::Bool(value) => env.get_boolean(*value)?.into_unknown(),
        SdkValue::Int(value) => env.create_int64(*value)?.into_unknown(),
        SdkValue::Text(text) => env.create_string(text)?.into_unknown(),
        // Copied into memory V8 allocates and counts toward collection, with
        // no second Rust copy; an external buffer is neither (#352 fix 056).
        SdkValue::Bytes(bytes) => env.create_buffer_copy(bytes)?.into_raw().into_unknown(),
        SdkValue::List(items) => {
            let mut array = env.create_array_with_length(items.len())?;
            for (index, item) in items.iter().enumerate() {
                array.set_element(index as u32, to_js(env, item)?)?;
            }
            array.into_unknown()
        }
        SdkValue::Map(fields) => {
            let mut object = env.create_object()?;
            for (name, field) in fields {
                object.set_named_property(name, to_js(env, field)?)?;
            }
            object.into_unknown()
        }
    })
}
