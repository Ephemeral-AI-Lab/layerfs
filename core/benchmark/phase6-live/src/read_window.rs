//! One admitted authenticated pack/decode wave; no lifetime payload cache.
use crate::{minio::Minio, objects::Locator};
use layerfs_content::ObjectId;
use layerfs_storage::{
    encoding::{decode_canonical, DecompressionWorkspace, GroupCache},
    sqlite::lookup::ObjectLocation,
    StorageCapacities,
};
pub const PAGE: usize = 128;
pub const BYTES: usize = 4 * 1024 * 1024 - 1;
pub fn read(s3: &Minio, rows: &[Locator]) -> Result<Vec<Vec<u8>>, String> {
    if rows.len() > PAGE {
        return Err("read locator page capacity".into());
    }
    if rows.is_empty() {
        return Ok(Vec::new());
    }
    let total = rows.iter().try_fold(0usize, |n, r| {
        n.checked_add(r.length).ok_or("canonical byte overflow")
    })?;
    if total > BYTES {
        return Err("read canonical byte admission".into());
    }
    let mut order: Vec<_> = (0..rows.len()).collect();
    order.sort_by_key(|i| (rows[*i].pack, rows[*i].group, rows[*i].record));
    let mut values: Vec<Option<Vec<u8>>> = (0..rows.len()).map(|_| None).collect();
    let mut selected = None;
    let mut pack = Vec::new();
    let mut groups = GroupCache::new();
    let mut decoder = DecompressionWorkspace::new().map_err(|e| e.to_string())?;
    let capacities =
        StorageCapacities::from_policy(Default::default()).map_err(|e| e.to_string())?;
    for index in order {
        let row = &rows[index];
        if selected != Some(row.pack) {
            pack = s3.get(&row.pack)?;
            selected = Some(row.pack);
            groups = GroupCache::new();
        }
        // Registration's unassigned zero ID is scoped to this single selected pack.
        let id = if row.pack_id == 0 {
            1
        } else {
            i64::try_from(row.pack_id).map_err(|_| "pack ID range")?
        };
        let location = ObjectLocation {
            object_id: row.id,
            role: row.role,
            canonical_length: row.length,
            pack_id: id,
            group_number: row.group as usize,
            record_number: row.record as usize,
        };
        let value = decode_canonical(
            &pack,
            &location,
            &capacities,
            None,
            &mut decoder,
            &mut groups,
            &mut 0,
        )
        .map_err(|e| e.to_string())?;
        if ObjectId::for_bytes(&value) != row.id {
            return Err("canonical identity mismatch".into());
        }
        values[index] = Some(value);
    }
    values
        .into_iter()
        .map(|v| v.ok_or("read result cardinality".into()))
        .collect()
}
