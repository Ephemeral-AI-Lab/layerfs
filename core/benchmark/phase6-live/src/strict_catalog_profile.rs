//! Opening authenticates the complete pinned main schema, never only its markers.
use super::*;

// SQLite 3.51.2 descriptors of strict_schema.sql, sorted by (type,name).
// Hash format: four UTF-8 fields per object, each prefixed by a big-endian u32
// length; SQL NULL uses u32::MAX. This includes automatic indexes and sqlite_sequence.
pub const SCHEMA_FINGERPRINT: [u8; 32] = [
    0xf7, 0xa6, 0xd0, 0xa2, 0xda, 0x13, 0x1c, 0x1e, 0x9f, 0x23, 0x8c, 0xbb, 0xe1, 0xbd, 0x94, 0xed,
    0x63, 0xd4, 0x1c, 0xeb, 0x95, 0xad, 0x54, 0x6c, 0x27, 0x00, 0x7b, 0xdc, 0xff, 0x4d, 0xc2, 0xc3,
];
const OBJECTS: usize = 32;
const FIELD_BYTES: usize = 572;
const TEXT_BYTES: usize = 6865;
const DDL_SOURCE_SHA256: [u8; 32] = [
    0xbf, 0x68, 0xac, 0x3e, 0x07, 0x28, 0x60, 0x52, 0x0c, 0x0e, 0x96, 0x23, 0x87, 0xf7, 0x01, 0xe3,
    0xe8, 0x9e, 0x7a, 0xed, 0x10, 0xfa, 0xfd, 0x49, 0xe4, 0xf1, 0x11, 0xcd, 0x3f, 0x41, 0x47, 0x90,
];
pub(super) fn validate(db: &Connection) -> Result<(), String> {
    if Sha256::digest(include_bytes!("strict_schema.sql")).as_slice() != DDL_SOURCE_SHA256 {
        return Err("strict catalog source schema fingerprint is stale".into());
    }
    let mut statement=db.prepare("SELECT type,name,tbl_name,sql,(length(CAST(type AS BLOB))>?1 OR length(CAST(name AS BLOB))>?1 OR length(CAST(tbl_name AS BLOB))>?1 OR COALESCE(length(CAST(sql AS BLOB)),0)>?1) FROM main.sqlite_schema ORDER BY type,name LIMIT ?2").map_err(err)?;
    let mut rows = statement
        .query(params![FIELD_BYTES as i64, OBJECTS as i64 + 1])
        .map_err(err)?;
    let mut hash = Sha256::new();
    let mut count = 0;
    let mut bytes = 0;
    while let Some(row) = rows.next().map_err(err)? {
        count += 1;
        if count > OBJECTS || row.get::<_, bool>(4).map_err(err)? {
            return Err("strict catalog schema shape exceeds pinned contract".into());
        }
        for column in 0..4 {
            let field: Option<String> = row.get(column).map_err(err)?;
            if column < 3 && field.is_none() {
                return Err("strict catalog schema descriptor missing".into());
            }
            match field {
                Some(field) => {
                    bytes += field.len();
                    if bytes > TEXT_BYTES {
                        return Err("strict catalog schema text exceeds pinned contract".into());
                    }
                    hash.update((field.len() as u32).to_be_bytes());
                    hash.update(field.as_bytes());
                }
                None => hash.update(u32::MAX.to_be_bytes()),
            }
        }
    }
    if count != OBJECTS || bytes != TEXT_BYTES || hash.finalize().as_slice() != SCHEMA_FINGERPRINT {
        return Err("strict catalog schema shape/fingerprint mismatch".into());
    }
    Ok(())
}
