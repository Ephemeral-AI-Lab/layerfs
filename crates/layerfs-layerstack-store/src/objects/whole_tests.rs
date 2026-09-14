use super::super::spill::{temporary_file, TempPath};
use super::*;

struct Store {
    db: StoreDb,
    _path: TempPath,
}
impl Store {
    fn new() -> Self {
        let (file, path) = temporary_file("whole-content-test").unwrap();
        drop(file);
        std::fs::remove_file(&path).unwrap();
        Self {
            db: StoreDb::create(&path).unwrap(),
            _path: TempPath::new(path),
        }
    }
    fn put(&self, canonical: &[u8], record: Vec<u8>) -> ObjectId {
        let id = ObjectId::for_bytes(canonical);
        let pack = assemble(&[record]).unwrap();
        let mut connection = self.db.writer().unwrap();
        let transaction = connection.transaction().unwrap();
        let next: i64 = transaction
            .query_row(
                "SELECT COALESCE(MAX(pack_id),0)+1 FROM object_packs",
                [],
                |row| row.get(0),
            )
            .unwrap();
        transaction
            .execute(
                "INSERT INTO object_packs(pack_id,data) VALUES (?1,?2)",
                rusqlite::params![next, pack],
            )
            .unwrap();
        transaction.execute("INSERT INTO objects(object_id,canonical_length,pack_id,group_number,record_number) VALUES (?1,?2,?3,0,0)", rusqlite::params![id.as_bytes().as_slice(), canonical.len() as i64, next]).unwrap();
        transaction.commit().unwrap();
        id
    }
    fn replace(&self, id: ObjectId, record: Vec<u8>) {
        let location = self.db.object_locations(&[id]).unwrap()[&id];
        self.db
            .writer()
            .unwrap()
            .execute(
                "UPDATE object_packs SET data=?1 WHERE pack_id=?2",
                rusqlite::params![assemble(&[record]).unwrap(), location.pack],
            )
            .unwrap();
    }
}
fn random(length: usize) -> Vec<u8> {
    let mut n = 0x42db584bu32;
    (0..length)
        .map(|_| {
            n ^= n << 13;
            n ^= n >> 17;
            n ^= n << 5;
            n as u8
        })
        .collect()
}

#[test]
fn whole_owner_prefix_slices_boundaries_and_exact_reads() {
    let mut encoder = pack::NativeEncoder::new_whole().unwrap();
    for length in [
        content::SMALL_LIMIT,
        content::SMALL_LIMIT + 1,
        content::WHOLE_LIMIT,
    ] {
        let store = Store::new();
        let base = random(length);
        let base_canonical = Role::Whole.canonical(&base).unwrap();
        let base_frame = encoder.compress(&base, None).unwrap();
        let base_id = store.put(
            &base_canonical,
            encode(Role::Whole, length, None, &base_frame).unwrap(),
        );
        let mut target = base.clone();
        target[1234] ^= 1;
        target[length - 1] ^= 1;
        let canonical = Role::Whole.canonical(&target).unwrap();
        let prefix = encoder.compress(&target, Some(&base)).unwrap();
        let id = store.put(
            &canonical,
            encode(Role::Whole, length, Some(base_id), &prefix).unwrap(),
        );
        assert_eq!(store.db.read_object_row(id).unwrap(), canonical);
        assert_eq!(store.db.read_object_row(base_id).unwrap(), base_canonical);
        let native = Role::Native.canonical(&target[1000..1000 + 32768]).unwrap();
        let native_id = store.put(&native, slice(id, 1000, 32768).unwrap());
        assert_eq!(store.db.read_object_row(native_id).unwrap(), native);
        let known = store.db.object_locations(&[id, native_id]).unwrap();
        super::super::admission::compare(
            &store.db,
            &known,
            &mut vec![(id, canonical.as_slice()), (native_id, native.as_slice())],
            &mut Default::default(),
            0,
        )
        .unwrap();
        assert_eq!(store.db.small_physical_base(native_id).unwrap(), Some(id));
        assert_eq!(store.db.small_physical_base(id).unwrap(), Some(base_id));
    }
    assert!(Role::Whole
        .canonical(&vec![0; content::SMALL_LIMIT - 1])
        .is_err());
    assert!(Role::Whole
        .canonical(&vec![0; content::WHOLE_LIMIT + 1])
        .is_err());
}

#[test]
fn whole_graph_depth_and_canonical_closure_are_enforced() {
    for (role, length, accepted_edges) in [
        (Role::Small, 4096, 50),
        (Role::Whole, content::WHOLE_LIMIT, 30),
    ] {
        let store = Store::new();
        let mut encoder = if role == Role::Small {
            pack::NativeEncoder::new_small().unwrap()
        } else {
            pack::NativeEncoder::new_whole().unwrap()
        };
        let mut raw = vec![b'x'; length];
        let canonical = role.canonical(&raw).unwrap();
        let mut id = store.put(
            &canonical,
            encode(role, length, None, &encoder.compress(&raw, None).unwrap()).unwrap(),
        );
        for edge in 1..=accepted_edges + 1 {
            let mut next = raw.clone();
            next[edge * 17] = (edge + 1) as u8;
            let canonical = role.canonical(&next).unwrap();
            let frame = encoder.compress(&next, Some(&raw)).unwrap();
            id = store.put(&canonical, encode(role, length, Some(id), &frame).unwrap());
            raw = next;
            if edge == accepted_edges {
                assert_eq!(store.db.read_object_row(id).unwrap(), canonical);
            }
        }
        assert!(store.db.read_object_row(id).is_err());
    }
}

#[test]
fn whole_graph_authenticates_unused_bases_and_rejects_corrupt_slices() {
    let store = Store::new();
    let mut encoder = pack::NativeEncoder::new_whole().unwrap();
    let raw = random(content::SMALL_LIMIT);
    let base = Role::Whole.canonical(&raw).unwrap();
    let base_record = encode(
        Role::Whole,
        raw.len(),
        None,
        &encoder.compress(&raw, None).unwrap(),
    )
    .unwrap();
    let base_id = store.put(&base, base_record.clone());
    let mut changed = raw.clone();
    changed[10] ^= 7;
    let canonical = Role::Whole.canonical(&changed).unwrap();
    let target_record = encode(
        Role::Whole,
        raw.len(),
        Some(base_id),
        &encoder.compress(&changed, Some(&raw)).unwrap(),
    )
    .unwrap();
    let target_id = store.put(&canonical, target_record.clone());
    let mut wrong_base = raw.clone();
    wrong_base[10] ^= 3;
    store.replace(
        base_id,
        encode(
            Role::Whole,
            raw.len(),
            None,
            &encoder.compress(&wrong_base, None).unwrap(),
        )
        .unwrap(),
    );
    assert!(matches!(
        store.db.read_object_row(target_id),
        Err(StoreError::Integrity("object identity"))
    ));
    store.replace(base_id, base_record);
    assert_eq!(store.db.read_object_row(target_id).unwrap(), canonical);
    let native = Role::Native.canonical(&changed[..100]).unwrap();
    let native_id = store.put(&native, slice(target_id, 0, 100).unwrap());
    assert_eq!(store.db.read_object_row(native_id).unwrap(), native);
    store.replace(native_id, slice(target_id, raw.len(), 100).unwrap());
    assert!(store.db.read_object_row(native_id).is_err());
    let mut cyclic = target_record.clone();
    cyclic[1..33].copy_from_slice(target_id.as_bytes());
    store.replace(target_id, cyclic);
    assert!(store.db.read_object_row(target_id).is_err());
    store.replace(target_id, target_record.clone());
    let mut corrupt = target_record;
    *corrupt.last_mut().unwrap() ^= 1;
    store.replace(target_id, corrupt);
    assert!(store.db.read_object_row(target_id).is_err());
    store
        .db
        .writer()
        .unwrap()
        .execute(
            "DELETE FROM objects WHERE object_id=?1",
            [target_id.as_bytes().as_slice()],
        )
        .unwrap();
    store.replace(native_id, slice(target_id, 0, 100).unwrap());
    assert!(store.db.read_object_row(native_id).is_err());
}

#[test]
fn whole_container_rejects_invalid_directory_and_slice_owner_role() {
    let store = Store::new();
    let raw = random(1024);
    let mut encoder = pack::NativeEncoder::new_small().unwrap();
    let canonical = Role::Small.canonical(&raw).unwrap();
    let id = store.put(
        &canonical,
        encode(
            Role::Small,
            raw.len(),
            None,
            &encoder.compress(&raw, None).unwrap(),
        )
        .unwrap(),
    );
    let native = Role::Native.canonical(&raw[..32]).unwrap();
    let native_id = store.put(&native, slice(id, 0, 32).unwrap());
    assert!(store.db.read_object_row(native_id).is_err());
    let location = store.db.object_locations(&[id]).unwrap()[&id];
    let mut pack: Vec<u8> = store
        .db
        .reader()
        .unwrap()
        .query_row(
            "SELECT data FROM object_packs WHERE pack_id=?1",
            [location.pack],
            |row| row.get(0),
        )
        .unwrap();
    pack[16..20].copy_from_slice(&21u32.to_le_bytes());
    store
        .db
        .writer()
        .unwrap()
        .execute(
            "UPDATE object_packs SET data=?1 WHERE pack_id=?2",
            rusqlite::params![pack, location.pack],
        )
        .unwrap();
    assert!(store.db.read_object_row(id).is_err());
}
