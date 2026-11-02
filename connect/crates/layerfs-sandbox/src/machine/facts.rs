//! Facts read from provider and guest output: one string field of a JSON
//! answer, free bytes from `df -kP`, and whether a port listens in
//! `/proc/net/tcp`.
use crate::json::Json;
use crate::RuntimeError;
use std::io::Read;

/// The string at `path` in a JSON document; an array on the way stands for
/// its first element. `None` when the path is absent, not a string, or longer
/// than 128 bytes.
pub(crate) fn json_string(text: &str, path: &[&str]) -> Result<Option<String>, RuntimeError> {
    let mut json = Json::new(text.as_bytes());
    let found = descend(&mut json, path, 0)?;
    Ok(found)
}

fn descend<R: Read>(
    json: &mut Json<R>,
    path: &[&str],
    depth: u8,
) -> Result<Option<String>, RuntimeError> {
    if depth > 32 {
        return Err(RuntimeError::Protocol("JSON depth"));
    }
    json.whitespace()?;
    match (json.peek()?, path.split_first()) {
        (Some(b'"'), None) => {
            let text = json.string(true)?;
            Ok((!text.overflow).then(|| text.text().to_owned()))
        }
        (Some(b'['), _) => {
            let mut found = None;
            let mut first = true;
            json.array(|json| {
                if first {
                    first = false;
                    found = descend(json, path, depth + 1)?;
                    Ok(())
                } else {
                    json.skip(depth + 1)
                }
            })?;
            Ok(found)
        }
        (Some(b'{'), Some((key, rest))) => {
            let mut found = None;
            json.object(|json, name| {
                if found.is_none() && name.equals(key) {
                    found = descend(json, rest, depth + 1)?;
                    Ok(())
                } else {
                    json.skip(depth + 1)
                }
            })?;
            Ok(found)
        }
        _ => {
            json.skip(depth)?;
            Ok(None)
        }
    }
}

/// Free bytes from the last line of `df -kP <path>`.
pub(crate) fn df_free(stdout: &str) -> Option<u64> {
    let line = stdout.trim().lines().last()?;
    let kib: u64 = line.split_whitespace().nth(3)?.parse().ok()?;
    kib.checked_mul(1024)
}

/// Whether `/proc/net/tcp` (or `tcp6`) text shows a socket listening on `port`.
pub(crate) fn listening(proc_net_tcp: &str, port: u16) -> bool {
    let suffix = format!(":{port:04X}");
    proc_net_tcp.lines().any(|line| {
        let mut fields = line.split_whitespace();
        let local = fields.nth(1);
        let state = fields.nth(1);
        matches!((local, state), (Some(local), Some("0A")) if local.ends_with(&suffix))
    })
}
