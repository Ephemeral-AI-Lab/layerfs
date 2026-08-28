use super::artifact::{json_string, json_u128};
use super::row_parse::{json_object, parse_digits};
use crate::stage1_fixture::EvalResult;
pub(crate) fn validate_named_wall_equation(summary: &str) -> EvalResult<()> {
    let walls = json_object(summary, "walls_ns")?;
    let complete = json_u128(walls, "complete_wall")?;
    let row_wall = json_u128(walls, "row_wall_sum")?;
    let outside = json_u128(walls, "outside_rows_wall")?;
    let residual = json_u128(walls, "timer_residual")?;
    if complete
        != row_wall
            .checked_add(outside)
            .and_then(|value| value.checked_add(residual))
            .ok_or_else(|| "summary row/outside wall overflow".to_owned())?
    {
        return Err("summary row/outside named wall equation".to_owned());
    }
    let named = [
        "admission",
        "reset",
        "store_open",
        "initial_materialization",
        "physical_phase",
        "physical_history_phase",
        "logical_refresh_phase",
        "logical_history_phase",
        "burst_phase",
        "milestone_materialization_phase",
        "cleanup",
        "artifact_write",
    ]
    .into_iter()
    .try_fold(0_u128, |total, key| {
        total
            .checked_add(json_u128(walls, key)?)
            .ok_or_else(|| "summary named wall overflow".to_owned())
    })?;
    if complete
        != named
            .checked_add(residual)
            .ok_or_else(|| "summary named wall residual overflow".to_owned())?
    {
        return Err("summary complete named wall equation".to_owned());
    }
    Ok(())
}
pub(crate) fn json_object_member_names(object: &str) -> EvalResult<Vec<String>> {
    let bytes = object.as_bytes();
    if bytes.first() != Some(&b'{') {
        return Err("JSON object does not start with {".to_owned());
    }
    let mut names = Vec::new();
    let mut index = 1_usize;
    let mut object_depth = 1_u32;
    let mut array_depth = 0_u32;
    let mut expects_key = true;
    while index < bytes.len() {
        match bytes[index] {
            b'"' => {
                let start = index + 1;
                index = start;
                let mut escaped = false;
                while index < bytes.len() {
                    let byte = bytes[index];
                    if escaped {
                        escaped = false;
                    } else if byte == b'\\' {
                        escaped = true;
                    } else if byte == b'"' {
                        break;
                    }
                    index += 1;
                }
                if index == bytes.len() {
                    return Err("unterminated JSON string".to_owned());
                }
                if object_depth == 1 && array_depth == 0 && expects_key {
                    let key = &object[start..index];
                    if key.contains('\\') {
                        return Err("escaped summary JSON key is unsupported".to_owned());
                    }
                    names.push(key.to_owned());
                    expects_key = false;
                }
            }
            b'{' => object_depth += 1,
            b'}' => {
                object_depth = object_depth
                    .checked_sub(1)
                    .ok_or_else(|| "summary JSON object depth underflow".to_owned())?;
                if object_depth == 0 {
                    return Ok(names);
                }
            }
            b'[' => array_depth += 1,
            b']' => {
                array_depth = array_depth
                    .checked_sub(1)
                    .ok_or_else(|| "summary JSON array depth underflow".to_owned())?;
            }
            b',' if object_depth == 1 && array_depth == 0 => expects_key = true,
            _ => {}
        }
        index += 1;
    }
    Err("unterminated summary JSON object".to_owned())
}
pub(crate) fn json_top_level_value<'a>(json: &'a str, expected: &str) -> EvalResult<&'a str> {
    let bytes = json.as_bytes();
    if bytes.first() != Some(&b'{') {
        return Err("JSON object does not start with {".to_owned());
    }
    let mut index = 1_usize;
    loop {
        while bytes
            .get(index)
            .is_some_and(|byte| byte.is_ascii_whitespace() || *byte == b',')
        {
            index += 1;
        }
        if bytes.get(index) == Some(&b'}') {
            return Err(format!("missing top-level JSON value {expected}"));
        }
        if bytes.get(index) != Some(&b'"') {
            return Err("invalid top-level JSON key".to_owned());
        }
        let key_start = index + 1;
        index = key_start;
        while bytes.get(index).is_some_and(|byte| *byte != b'"') {
            if bytes[index] == b'\\' {
                return Err("escaped top-level JSON key is unsupported".to_owned());
            }
            index += 1;
        }
        if bytes.get(index) != Some(&b'"') {
            return Err("unterminated top-level JSON key".to_owned());
        }
        let key = &json[key_start..index];
        index += 1;
        while bytes
            .get(index)
            .is_some_and(|byte| byte.is_ascii_whitespace())
        {
            index += 1;
        }
        if bytes.get(index) != Some(&b':') {
            return Err("top-level JSON key lacks colon".to_owned());
        }
        index += 1;
        while bytes
            .get(index)
            .is_some_and(|byte| byte.is_ascii_whitespace())
        {
            index += 1;
        }
        if key == expected {
            return Ok(&json[index..]);
        }
        let mut object_depth = 0_u32;
        let mut array_depth = 0_u32;
        let mut string = false;
        let mut escaped = false;
        loop {
            let byte = *bytes
                .get(index)
                .ok_or_else(|| "unterminated top-level JSON value".to_owned())?;
            if string {
                if escaped {
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else if byte == b'"' {
                    string = false;
                }
                index += 1;
                continue;
            }
            if object_depth == 0 && array_depth == 0 && matches!(byte, b',' | b'}') {
                break;
            }
            match byte {
                b'"' => string = true,
                b'{' => object_depth += 1,
                b'}' => {
                    object_depth = object_depth
                        .checked_sub(1)
                        .ok_or_else(|| "top-level JSON object depth underflow".to_owned())?;
                }
                b'[' => array_depth += 1,
                b']' => {
                    array_depth = array_depth
                        .checked_sub(1)
                        .ok_or_else(|| "top-level JSON array depth underflow".to_owned())?;
                }
                _ => {}
            }
            index += 1;
        }
    }
}
pub(crate) fn json_top_level_string(json: &str, key: &str) -> EvalResult<String> {
    let value = json_top_level_value(json, key)?;
    if !value.starts_with('"') {
        return Err(format!("invalid top-level JSON string {key}"));
    }
    json_string(&format!("{{\"value\":{value}"), "value")
}
pub(crate) fn json_top_level_u128(json: &str, key: &str) -> EvalResult<u128> {
    parse_digits(json_top_level_value(json, key)?, key)
}
pub(crate) fn require_json_keys(
    json: &str,
    object: Option<&str>,
    expected: &[&str],
) -> EvalResult<()> {
    let value = object.map_or(Ok(json), |key| json_object(json, key))?;
    let actual = json_object_member_names(value)?;
    if actual != expected {
        return Err(format!(
            "summary JSON {} keys {actual:?} != {expected:?}",
            object.unwrap_or("top-level")
        ));
    }
    Ok(())
}
