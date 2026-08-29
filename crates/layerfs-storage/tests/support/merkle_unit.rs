use super::*;

#[test]
fn deferred_objects_are_memory_first_and_spill_after_eight_mib() {
    let mut small = DeferredObjectStore::new().unwrap();
    small
        .stage(CanonicalObject::new(layerfs_content::encode_bytes_object(b"small").unwrap()).unwrap())
        .unwrap();
    assert!(!small.spilled());

    let mut large = DeferredObjectStore::new().unwrap();
    for index in 0..9_u8 {
        let mut bytes = vec![index; 1024 * 1024];
        bytes[0] = index;
        large
            .stage(
                CanonicalObject::new(layerfs_content::encode_bytes_object(&bytes).unwrap()).unwrap(),
            )
            .unwrap();
    }
    assert!(large.spilled());
    let mut count = 0;
    while large.pop_first().unwrap().is_some() {
        count += 1;
    }
    assert_eq!(count, 9);
}

#[test]
fn seen_ids_spill_in_fixed_pages_and_preserve_membership() {
    let root = ObjectId::for_bytes(b"seen-root");
    let ids = (0..=DEFERRED_MEMORY_BYTES / 64)
        .map(|index| ObjectId::for_bytes(&(index as u64).to_be_bytes()))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let mut seen = SeenIds::new(root).unwrap();
    let mut inserted = 0;
    for page in ids.chunks(crate::ID_BATCH_COUNT) {
        inserted += seen.insert_page(page).unwrap().len();
    }
    let SeenIds::Spill(connection) = &mut seen else {
        panic!("expected spill")
    };
    assert_eq!(
        connection
            .query_row("SELECT count(*) FROM seen", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        (inserted + usize::from(!ids.contains(&root))) as i64
    );
    assert!(seen
        .insert_page(&ids[..crate::ID_BATCH_COUNT])
        .unwrap()
        .is_empty());
}
