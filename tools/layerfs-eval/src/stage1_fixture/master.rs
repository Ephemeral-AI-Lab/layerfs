use super::contract::{
    BaseManifest, EvalResult, Master, RootId, BASES, EXPECTED_CDC_REFERENCES,
    EXPECTED_CDC_SEQUENCE, EXPECTED_RAW_DIGEST, FILE_BYTES, FILE_PATH,
};
use super::error::{display_error, io_error};
use super::oracle::hash_file;
use super::tree::{tree_digest, verify_sealed};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

pub fn read_master(root: &Path) -> EvalResult<Master> {
    let json = fs::read_to_string(root.join("master.json")).map_err(io_error)?;
    let raw_digest = json_string(&json, "raw_blake3")?;
    let replacement_digest = json_string(&json, "replacement_blake3")?;
    let inventory_digest = json_string(&json, "inventory_blake3")?;
    let new_file_aggregate_rope_references = json_u64(&json, "new_file_aggregate_rope_references")?;
    if raw_digest != EXPECTED_RAW_DIGEST {
        return Err("master raw digest does not match frozen S1-100".to_owned());
    }
    if json_u64(&json, "bytes")? != FILE_BYTES
        || json_u64(&json, "cdc_references")? != EXPECTED_CDC_REFERENCES
        || json_string(&json, "cdc_sequence_blake3")? != EXPECTED_CDC_SEQUENCE
        || json_string(&json, "cdc_counter_scope")? != "independent_streamed_file_oracle"
        || json_u64(&json, "logical_file_mode")? != 0o644
        || json_u64(&json, "logical_mtime_seconds")? != 0
        || new_file_aggregate_rope_references < EXPECTED_CDC_REFERENCES
    {
        return Err("master generator population does not match poc/14".to_owned());
    }
    let bases_object = json_object(&json, "bases")?;
    let mut bases = BTreeMap::new();
    for name in BASES {
        let object = json_object(bases_object, name)?;
        let root = json_string(object, "root")?
            .parse::<RootId>()
            .map_err(display_error)?;
        let root_a = if *name == "refresh-a-b" {
            Some(
                json_string(object, "root_a")?
                    .parse::<RootId>()
                    .map_err(display_error)?,
            )
        } else {
            None
        };
        let root_b = if *name == "refresh-a-b" {
            Some(
                json_string(object, "root_b")?
                    .parse::<RootId>()
                    .map_err(display_error)?,
            )
        } else {
            None
        };
        bases.insert(
            (*name).to_owned(),
            BaseManifest {
                name: (*name).to_owned(),
                root,
                root_a,
                root_b,
                generation: json_u64(object, "generation")?,
                selector_generation: json_u64(object, "selector_generation")?,
                store_id: json_string(object, "store_id")?,
                profile_id: json_string(object, "profile_id")?,
                store_database_bytes: json_u64(object, "store_database_bytes")?,
            },
        );
    }
    Ok(Master {
        raw_digest,
        replacement_digest,
        inventory_digest,
        new_file_aggregate_rope_references,
        bases,
    })
}

pub(super) fn write_master(
    path: &Path,
    master: &Master,
    preparation_wall_ns: u128,
) -> EvalResult<()> {
    let mut json = String::from("{\n");
    json.push_str("  \"schema\":\"layerfs-stage1-single-file-master-v1\",\n");
    json.push_str(&format!(
        "  \"generator\":{{\"version\":\"phase4-fill-retained-buffer-v1\",\"label\":\"S1-100\",\"seed\":81,\"bytes\":{FILE_BYTES},\"raw_blake3\":\"{}\",\"cdc_references\":{EXPECTED_CDC_REFERENCES},\"cdc_sequence_blake3\":\"{EXPECTED_CDC_SEQUENCE}\",\"cdc_counter_scope\":\"independent_streamed_file_oracle\",\"new_file_aggregate_rope_references\":{}}},\n",
        master.raw_digest,
        master.new_file_aggregate_rope_references,
    ));
    json.push_str(&format!(
        "  \"replacement_blake3\":\"{}\",\n  \"inventory_blake3\":\"{}\",\n  \"logical_file_mode\":420,\n  \"logical_mtime_seconds\":0,\n  \"preparation_wall_ns\":{preparation_wall_ns},\n  \"bases\":{{\n",
        master.replacement_digest, master.inventory_digest
    ));
    for (index, base) in master.bases.values().enumerate() {
        if index != 0 {
            json.push_str(",\n");
        }
        json.push_str(&format!(
            "    \"{}\":{{\"root\":\"{}\",{}\"generation\":{},\"selector_generation\":{},\"store_id\":\"{}\",\"profile_id\":\"{}\",\"store_database_bytes\":{}}}",
            base.name,
            base.root,
            match (base.root_a, base.root_b) {
                (Some(root_a), Some(root_b)) => {
                    format!("\"root_a\":\"{root_a}\",\"root_b\":\"{root_b}\",")
                }
                _ => String::new(),
            },
            base.generation,
            base.selector_generation,
            base.store_id,
            base.profile_id,
            base.store_database_bytes
        ));
    }
    json.push_str("\n  }\n}\n");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(io_error)?;
    file.write_all(json.as_bytes()).map_err(io_error)?;
    file.sync_all().map_err(io_error)
}

pub fn verify_master(root: &Path, master: &Master, full_hash: bool) -> EvalResult<String> {
    verify_sealed(root)?;
    if fs::metadata(root.join("input").join(FILE_PATH))
        .map_err(io_error)?
        .len()
        != FILE_BYTES
        || fs::metadata(root.join("input/S1-replace-100.bin"))
            .map_err(io_error)?
            .len()
            != FILE_BYTES
    {
        return Err("fixture input size mismatch".to_owned());
    }
    if full_hash {
        if hash_file(&root.join("input").join(FILE_PATH))? != master.raw_digest
            || hash_file(&root.join("input/S1-replace-100.bin"))? != master.replacement_digest
        {
            return Err("fixture input digest mismatch".to_owned());
        }
        let inventory = tree_digest(root, Some(Path::new("master.json")))?;
        if inventory != master.inventory_digest {
            return Err(format!(
                "sealed fixture inventory mismatch: expected {}, got {inventory}",
                master.inventory_digest
            ));
        }
    }
    tree_digest(root, None)
}

fn json_string(json: &str, key: &str) -> EvalResult<String> {
    let needle = format!("\"{key}\":\"");
    let start = json
        .find(&needle)
        .ok_or_else(|| format!("missing JSON string {key}"))?
        + needle.len();
    let end = json[start..]
        .find('"')
        .ok_or_else(|| format!("unterminated JSON string {key}"))?
        + start;
    Ok(json[start..end].to_owned())
}

fn json_u64(json: &str, key: &str) -> EvalResult<u64> {
    let needle = format!("\"{key}\":");
    let start = json
        .find(&needle)
        .ok_or_else(|| format!("missing JSON integer {key}"))?
        + needle.len();
    let end = json[start..]
        .find(|character: char| !character.is_ascii_digit())
        .map_or(json.len(), |offset| start + offset);
    json[start..end].parse::<u64>().map_err(display_error)
}

fn json_object<'a>(json: &'a str, key: &str) -> EvalResult<&'a str> {
    let needle = format!("\"{key}\":{{");
    let start = json
        .find(&needle)
        .ok_or_else(|| format!("missing JSON object {key}"))?
        + needle.len()
        - 1;
    let mut depth = 0_u64;
    for (offset, byte) in json.as_bytes()[start..].iter().enumerate() {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Ok(&json[start..=start + offset]);
                }
            }
            _ => {}
        }
    }
    Err(format!("unterminated JSON object {key}"))
}

#[cfg(test)]
mod tests {
    use super::{json_object, json_string, json_u64};

    #[test]
    fn json_helpers_read_nested_objects() {
        let json = r#"{"bases":{"x":{"root":"abc","generation":2}}}"#;
        let bases = json_object(json, "bases").unwrap();
        let x = json_object(bases, "x").unwrap();
        assert_eq!(json_string(x, "root").unwrap(), "abc");
        assert_eq!(json_u64(x, "generation").unwrap(), 2);
    }
}
