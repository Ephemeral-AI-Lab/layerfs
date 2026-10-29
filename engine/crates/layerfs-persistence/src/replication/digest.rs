//! State identities and the bundle digest.
use super::lookup::table_exists;
use super::tables::{key_columns, quoted, LOGGED};
use super::{ChangeCursor, ChangeError};
use rusqlite::{types::ValueRef, Connection};

pub(super) fn base_state(catalog: &[u8], seq: u64, token: &[u8; 32]) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    h.update(b"layerfs-change-base-v1\0");
    h.update(&(catalog.len() as u64).to_le_bytes());
    h.update(catalog);
    h.update(&seq.to_le_bytes());
    h.update(token);
    h.finalize().into()
}

pub(super) fn next_state(
    catalog: &[u8],
    before: &ChangeCursor,
    after_seq: u64,
    digest: &[u8; 32],
) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    h.update(b"layerfs-change-v1\0");
    h.update(&(catalog.len() as u64).to_le_bytes());
    h.update(catalog);
    h.update(&before.state);
    h.update(&before.seq.to_le_bytes());
    h.update(&after_seq.to_le_bytes());
    h.update(digest);
    h.finalize().into()
}

/// Feeds one value with its type tag and length.
fn feed(h: &mut blake3::Hasher, value: ValueRef<'_>) {
    match value {
        ValueRef::Null => {
            h.update(&[0]);
        }
        ValueRef::Integer(i) => {
            h.update(&[1]);
            h.update(&i.to_le_bytes());
        }
        ValueRef::Real(r) => {
            h.update(&[2]);
            h.update(&r.to_bits().to_le_bytes());
        }
        ValueRef::Text(t) => {
            h.update(&[3]);
            h.update(&(t.len() as u64).to_le_bytes());
            h.update(t);
        }
        ValueRef::Blob(b) => {
            h.update(&[4]);
            h.update(&(b.len() as u64).to_le_bytes());
            h.update(b);
        }
    }
}

/// The digest of the bundle tables in `schema`: for each logged table in code
/// order, its rows ordered by primary key, then its deleted keys in order.
/// Export and apply both compute it from the bundle file itself.
pub(super) fn bundle_digest(
    c: &Connection,
    schema: &str,
) -> Result<([u8; 32], u64, u64), ChangeError> {
    let mut h = blake3::Hasher::new();
    h.update(b"layerfs-change-bundle-v1\0");
    let (mut rows, mut deletes) = (0u64, 0u64);
    for t in &LOGGED {
        if !table_exists(c, schema, t.table)? {
            continue;
        }
        h.update(&t.code.to_le_bytes());
        let mut stmt = c.prepare(&format!(
            "SELECT * FROM {schema}.{} ORDER BY {}",
            quoted(t.table),
            t.keys.join(",")
        ))?;
        let width = stmt.column_count();
        let mut cursor = stmt.query([])?;
        while let Some(row) = cursor.next()? {
            h.update(b"r");
            h.update(&(width as u32).to_le_bytes());
            for i in 0..width {
                feed(&mut h, row.get_ref(i)?);
            }
            rows += 1;
        }
        let columns = key_columns(t.keys.len());
        let mut stmt = c.prepare(&format!(
            "SELECT {columns} FROM {schema}.bundle_delete WHERE tbl=?1 ORDER BY {columns}"
        ))?;
        let mut cursor = stmt.query([t.code])?;
        while let Some(row) = cursor.next()? {
            h.update(b"d");
            for i in 0..t.keys.len() {
                feed(&mut h, row.get_ref(i)?);
            }
            deletes += 1;
        }
    }
    Ok((h.finalize().into(), rows, deletes))
}
