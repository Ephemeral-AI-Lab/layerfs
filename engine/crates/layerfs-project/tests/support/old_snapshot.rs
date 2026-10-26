//! Exact declared pack bytes from the retained old path, test oracle only.
use layerfs_content::ObjectId;
use layerfs_storage::{
    location::{ObjectLocation, PackDomain, PackInfo, ValueGroupRow},
    port::*,
};
pub fn snapshot(path: &std::path::Path) -> (Registration, Vec<(ObjectKey, Vec<u8>)>) {
    let connection = rusqlite::Connection::open(path).unwrap();
    let mut batch = Registration::default();
    let mut payload = Vec::new();
    let mut stmt = connection
        .prepare("SELECT pack_id,data FROM object_packs ORDER BY pack_id")
        .unwrap();
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?))
        })
        .unwrap();
    for row in rows {
        let (pack_id, mut body) = row.unwrap();
        let length = layerfs_storage::pack::layout::declared_length(&body).unwrap();
        body.truncate(length);
        let header = layerfs_storage::pack::layout::parse_header(&body).unwrap();
        let domain = PackDomain::for_lane(header.lane);
        let key = ObjectKey::for_bytes(&body);
        let info = PackInfo {
            pack_id,
            domain,
            key,
            length,
        };
        if domain == PackDomain::Payload {
            payload.push((key, body.clone()));
        }
        batch.packs.push(RegisteredPack {
            info,
            body: (domain == PackDomain::Metadata).then_some(body),
        });
    }
    let mut stmt=connection.prepare("SELECT object_id,object_role,canonical_length,pack_id,group_number,record_number FROM objects ORDER BY pack_id,group_number,record_number").unwrap();
    batch.objects = stmt
        .query_map([], |row| {
            Ok(ObjectLocation {
                object_id: ObjectId::from_bytes(&row.get::<_, Vec<u8>>(0)?).unwrap(),
                role: layerfs_content::ObjectRole::from_code(row.get::<_, u8>(1)?).unwrap(),
                canonical_length: usize::try_from(row.get::<_, i64>(2)?).unwrap(),
                pack_id: row.get(3)?,
                group_number: usize::try_from(row.get::<_, i64>(4)?).unwrap(),
                record_number: usize::try_from(row.get::<_, i64>(5)?).unwrap(),
            })
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    let mut stmt=connection.prepare("SELECT first_ordinal,count,pack_id,group_number,digest FROM metadata_value_groups ORDER BY first_ordinal").unwrap();
    batch.value_groups = stmt
        .query_map([], |row| {
            Ok(ValueGroupRow {
                first_ordinal: row.get(0)?,
                count: usize::try_from(row.get::<_, i64>(1)?).unwrap(),
                pack_id: row.get(2)?,
                group_number: usize::try_from(row.get::<_, i64>(3)?).unwrap(),
                digest: ObjectId::from_bytes(&row.get::<_, Vec<u8>>(4)?).unwrap(),
            })
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    (batch, payload)
}
