//! Opening authenticates the complete pinned main schema, never only its markers.
use super::*;

// SQLite 3.51.2 descriptors of strict_schema.sql, sorted by (type,name).
// Hash format: four UTF-8 fields per object, each prefixed by a big-endian u32
// length; SQL NULL uses u32::MAX. This includes automatic indexes and sqlite_sequence.
pub const SCHEMA_FINGERPRINT: [u8; 32] = [
    0x0b, 0x7c, 0x20, 0xd7, 0x39, 0x43, 0x14, 0xf4, 0x0d, 0x37, 0x08, 0x99, 0xdf, 0xf6, 0x2f, 0xe9,
    0xaf, 0xcc, 0xab, 0x2d, 0x51, 0x35, 0x1d, 0x24, 0xd9, 0x20, 0x96, 0xa3, 0x36, 0x77, 0xfb, 0x40,
];
const OBJECTS: usize = 33;
const FIELD_BYTES: usize = 572;
const TEXT_BYTES: usize = 6961;
const DDL_SOURCE_SHA256: [u8; 32] = [
    0xd6, 0x13, 0x84, 0x60, 0xad, 0xf8, 0x52, 0x6c, 0xba, 0x47, 0x3e, 0xf1, 0x6c, 0xed, 0x66, 0x2f,
    0x8d, 0xcf, 0xf3, 0x94, 0x2f, 0x3b, 0x6d, 0x00, 0x3c, 0xb5, 0xe2, 0xba, 0xe2, 0x95, 0x39, 0xc2,
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
