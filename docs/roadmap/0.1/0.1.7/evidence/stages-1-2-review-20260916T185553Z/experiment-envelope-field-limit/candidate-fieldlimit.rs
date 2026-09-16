//! Reviewer experiment: does the candidate enforce the reference's 8 MiB
//! per-field ceiling that the reference's decode_bytes_object applies?
use layerfs_content::object::codec::{decode_bytes_object, encode_bytes_object};

fn main() {
    const MIB: usize = 1024 * 1024;
    for value_len in [MIB, 4 * MIB, 8 * MIB, 8 * MIB + 1, 12 * MIB, 16 * MIB - 14] {
        let value = vec![0x5au8; value_len];
        match encode_bytes_object(&value) {
            Ok(canonical) => {
                let decoded = decode_bytes_object(&canonical);
                println!(
                    "value {:>9} B ({:>5.1} MiB): encoded={:>9} B  decode={}",
                    value_len,
                    value_len as f64 / MIB as f64,
                    canonical.len(),
                    match decoded {
                        Ok(v) => format!("ACCEPTED ({} B)", v.len()),
                        Err(e) => format!("rejected: {e}"),
                    }
                );
            }
            Err(e) => println!("value {value_len}: encode rejected: {e}"),
        }
    }
    println!();
    println!("reference: rejects value_len > MAX_OBJECT_FIELD_BYTES = 8 MiB at encode AND decode");
}
