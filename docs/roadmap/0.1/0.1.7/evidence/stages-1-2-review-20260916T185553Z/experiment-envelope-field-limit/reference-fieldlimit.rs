//! Decodes the same canonical bytes with the REFERENCE decoder.
use layerfs_content_ref::{decode_bytes_object, encode_bytes_object};
use layerfs_content_ref::limits::MAX_OBJECT_FIELD_BYTES;

fn main() {
    const MIB: usize = 1024 * 1024;
    println!("reference MAX_OBJECT_FIELD_BYTES = {MAX_OBJECT_FIELD_BYTES}");
    for value_len in [MIB, 8 * MIB, 8 * MIB + 1, 12 * MIB, 16 * MIB - 14] {
        let value = vec![0x5au8; value_len];
        match encode_bytes_object(&value) {
            Ok(canonical) => println!(
                "value {:>9} B ({:>5.1} MiB): encoded={:>9} B  reference decode={}",
                value_len,
                value_len as f64 / MIB as f64,
                canonical.len(),
                match decode_bytes_object(&canonical) {
                    Ok(v) => format!("ACCEPTED ({} B)", v.len()),
                    Err(e) => format!("rejected: {e:?}"),
                }
            ),
            Err(e) => println!("value {value_len}: reference encode rejected: {e:?}"),
        }
    }
}
