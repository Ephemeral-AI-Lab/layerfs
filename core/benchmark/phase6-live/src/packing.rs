//! Actual C2 lane/group placement with exact object locators.
use crate::{minio::digest, objects::Locator};
use layerfs_content::{FinalizedObject, ObjectRole};
use layerfs_storage::{
    encoding::{encode_full, CompressionWorkspace},
    pack::{
        assemble::{assemble_consuming, build_group, framed_group_length},
        layout::{append_fits, assembled_length, EncodedGroup, PackLane},
    },
    policy::GROUP_TARGET,
    StorageCapacities,
};
pub struct Pack {
    pub bytes: Vec<u8>,
    pub rows: Vec<Locator>,
}
struct Record {
    row: Locator,
    bytes: Vec<u8>,
}
fn group(
    lane: PackLane,
    records: Vec<Record>,
    codec: &mut CompressionWorkspace,
) -> Result<(EncodedGroup, Vec<Locator>), String> {
    let (rows, bytes): (Vec<_>, Vec<_>) = records.into_iter().map(|r| (r.row, r.bytes)).unzip();
    Ok((
        build_group(lane, &bytes, Some(codec)).map_err(|e| e.to_string())?,
        rows,
    ))
}
fn seal(lane: PackLane, groups: Vec<EncodedGroup>, mut rows: Vec<Locator>) -> Result<Pack, String> {
    let bytes = assemble_consuming(lane, groups).map_err(|e| e.to_string())?;
    let key = digest(&bytes);
    for row in &mut rows {
        row.pack = key;
    }
    Ok(Pack { bytes, rows })
}
pub fn build(
    objects: Vec<FinalizedObject>,
    codec: &mut CompressionWorkspace,
) -> Result<Vec<Pack>, String> {
    let capacities =
        StorageCapacities::from_policy(Default::default()).map_err(|e| e.to_string())?;
    let mut lanes: Vec<(PackLane, Vec<Record>)> = Vec::new();
    for object in objects {
        let (lane, bytes) = if object.role() == ObjectRole::InodeLeaf {
            let mut bytes = vec![0];
            bytes.extend_from_slice(object.canonical());
            (PackLane::Ordinary, bytes)
        } else {
            let encoded = encode_full(
                object.canonical(),
                object.role(),
                &capacities,
                codec,
                &mut Default::default(),
            )
            .map_err(|e| e.to_string())?;
            (encoded.lane, encoded.record)
        };
        let row = Locator {
            id: object.id(),
            role: object.role(),
            length: object.canonical_len(),
            pack: [0; 32],
            group: 0,
            record: 0,
            pack_id: 0,
        };
        let index = match lanes.iter().position(|(l, _)| *l == lane) {
            Some(i) => i,
            None => {
                lanes.push((lane, Vec::new()));
                lanes.len() - 1
            }
        };
        lanes[index].1.push(Record { row, bytes });
    }
    let mut packs = Vec::new();
    for (lane, records) in lanes {
        let mut grouped = Vec::new();
        let mut pending = Vec::new();
        let mut payload = 0;
        for record in records {
            let width = record.bytes.len();
            let next = framed_group_length(pending.len() + 1, payload + width)
                .map_err(|e| e.to_string())?;
            if !pending.is_empty()
                && (matches!(lane, PackLane::Native | PackLane::Singleton) || next > GROUP_TARGET)
            {
                grouped.push(group(lane, std::mem::take(&mut pending), codec)?);
                payload = 0;
            }
            payload += width;
            pending.push(record);
        }
        if !pending.is_empty() {
            grouped.push(group(lane, pending, codec)?);
        }
        let mut groups = Vec::new();
        let mut rows = Vec::new();
        let mut assembled = 0usize;
        for (g, members) in grouped {
            if !groups.is_empty()
                && !append_fits(lane, assembled, groups.len(), &g).map_err(|e| e.to_string())?
            {
                packs.push(seal(
                    lane,
                    std::mem::take(&mut groups),
                    std::mem::take(&mut rows),
                )?);
                assembled = 0;
            }
            if groups.is_empty() {
                groups.push(g);
                assembled = assembled_length(lane, &groups).map_err(|e| e.to_string())?;
            } else {
                assembled = assembled
                    .checked_add(g.body_size(lane).map_err(|e| e.to_string())?)
                    .ok_or("pack byte overflow")?;
                groups.push(g);
            }
            let number = (groups.len() - 1) as u32;
            for (index, mut row) in members.into_iter().enumerate() {
                row.group = number;
                row.record = index as u32;
                rows.push(row);
            }
        }
        if !groups.is_empty() {
            packs.push(seal(lane, groups, rows)?);
        }
    }
    Ok(packs)
}
