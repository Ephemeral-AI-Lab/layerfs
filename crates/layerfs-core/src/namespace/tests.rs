#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct MemoryStore(BTreeMap<ObjectId, Vec<u8>>);

    impl ObjectStore for MemoryStore {
        fn get(&self, id: ObjectId) -> CoreResult<Vec<u8>> {
            self.0.get(&id).cloned().ok_or(CoreError::MissingObject)
        }

        fn put(&mut self, canonical: &[u8]) -> CoreResult<ObjectId> {
            let id = ObjectId::for_bytes(canonical);
            self.0.insert(id, canonical.to_vec());
            Ok(id)
        }
    }

    #[test]
    fn directory_pages_resume_without_rescanning_or_skipping() {
        let mut store = MemoryStore::default();
        let mut root = empty_directory(&mut store).unwrap();
        for serial in 0..300_u64 {
            let name = CanonicalName::new(&format!("entry-{serial:03}")).unwrap();
            root = directory_insert(
                &mut store,
                root,
                name,
                InodeId::allocate([0x31; 32], serial),
            )
            .unwrap()
            .0;
        }
        let mut after = None;
        let mut names = Vec::new();
        loop {
            let page = directory_page_after(
                &store,
                root,
                after.as_ref(),
                17,
                2048,
                &mut NamespaceCounters::default(),
            )
            .unwrap();
            names.extend(page.entries.iter().map(|entry| entry.0.as_str().to_owned()));
            after = page.continuation;
            if after.is_none() {
                break;
            }
        }
        assert_eq!(names.len(), 300);
        assert!(names.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn variable_width_directory_borrows_until_both_branches_are_filled() {
        let mut store = MemoryStore::default();
        let mut serial = 0_u64;
        let mut children = |prefix: char, count: usize, width: usize, store: &mut MemoryStore| {
            (0..count)
                .map(|index| {
                    let text = if width == 4 {
                        format!("{prefix}{index:03}")
                    } else {
                        format!("{prefix}{}{index:04}", prefix.to_string().repeat(width - 5))
                    };
                    let name = CanonicalName::new(&text).unwrap();
                    let inode = InodeId::allocate([0x51; 32], serial);
                    serial += 1;
                    let id = store.put(
                        &encode_directory_node(&leaf(vec![(name.clone(), inode)])?).unwrap(),
                    )?;
                    Ok((name, id))
                })
                .collect::<CoreResult<Vec<_>>>()
        };
        let left_children = children('a', 131, 4, &mut store).unwrap();
        let right_children = children('m', 11, 255, &mut store).unwrap();
        let left = store
            .put(
                &encode_directory_node(
                    &branch(&store, 1, left_children, &mut NamespaceCounters::default()).unwrap(),
                )
                .unwrap(),
            )
            .unwrap();
        let right = store
            .put(
                &encode_directory_node(
                    &branch(&store, 1, right_children, &mut NamespaceCounters::default()).unwrap(),
                )
                .unwrap(),
            )
            .unwrap();
        let replacements = try_directory_borrow(
            &mut store,
            1,
            left,
            right,
            true,
            &mut NamespaceCounters::default(),
        )
        .unwrap()
        .unwrap();
        let counts = replacements
            .into_iter()
            .map(
                |summary| match decode_directory_node(&store.0[&summary.id]).unwrap() {
                    DirectoryNodeV1::Branch { children, .. } => children.len(),
                    _ => panic!("expected branch"),
                },
            )
            .collect::<Vec<_>>();
        assert_eq!(counts, [129, 13]);
    }
}
