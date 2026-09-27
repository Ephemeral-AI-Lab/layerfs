use layerfs_workspace::backing::active::{Kind, Page, PageRef};

#[test]
fn page_v1_checks_identity_and_all_bytes() {
    let reference = PageRef { id: 7, epoch: 2 };
    let mut page = Page::new(Kind::Pack, [9; 32], reference, 1, 3, 1, b"record").unwrap();
    assert_eq!(
        page.verify(Kind::Pack, [9; 32], reference).unwrap(),
        b"record"
    );
    assert!(page.verify(Kind::Pack, [8; 32], reference).is_err());
    assert!(page
        .verify(Kind::Pack, [9; 32], PageRef { id: 7, epoch: 3 })
        .is_err());
    page.bytes[128] ^= 1;
    assert!(page.verify(Kind::Pack, [9; 32], reference).is_err());
}

#[cfg(target_os = "linux")]
mod linux {
    use super::{Kind, PageRef};
    use layerfs_workspace::backing::{
        active::{Index, PageStore, TinyPack},
        budget::Budget,
        metadata::MetadataHost,
        payload::PayloadHost,
    };
    use std::{
        fs,
        os::unix::fs::{DirBuilderExt, MetadataExt},
        path::PathBuf,
        sync::{
            atomic::{AtomicU64, Ordering},
            Arc,
        },
    };

    struct Fixture {
        root: PathBuf,
        payloads: Arc<PayloadHost>,
        directory: Arc<layerfs_workspace::backing::directory::Directory>,
        store: Arc<PageStore>,
    }

    impl Fixture {
        fn new(quota: u64) -> Self {
            let parent = PathBuf::from(
                std::env::var_os("LAYERFS_ACTIVE_TEST_ROOT")
                    .expect("set LAYERFS_ACTIVE_TEST_ROOT to an owned ext4 directory"),
            );
            static NEXT: AtomicU64 = AtomicU64::new(1);
            let root = parent.canonicalize().unwrap().join(format!(
                "active-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let mut builder = fs::DirBuilder::new();
            builder.mode(0o700);
            builder.create(&root).unwrap();
            let private = root.join("private-backing");
            builder.create(&private).unwrap();
            let budget = Budget::new(8 << 20);
            let payloads = PayloadHost::new(private.clone(), quota, &budget).unwrap();
            let directory = payloads
                .directory(private.join("workspace"), [9; 32])
                .unwrap();
            payloads.initialize(&directory).unwrap();
            let metadata = MetadataHost::new(payloads.clone()).unwrap();
            let store = PageStore::new(directory.clone(), metadata).unwrap();
            Self {
                root,
                payloads,
                directory,
                store,
            }
        }

        fn physical(&self) -> u64 {
            fs::read_dir(&*self.directory.path)
                .unwrap()
                .map(|entry| entry.unwrap().metadata().unwrap().blocks() * 512)
                .sum()
        }

        fn clean(self) {
            self.store.close().unwrap();
            let status = self.store.status().unwrap();
            assert_eq!(
                (status.pages, status.allocated_bytes, status.reserved_bytes),
                (0, 0, 0)
            );
            assert_eq!(self.physical(), 0);
            drop(self.store);
            self.directory.close().unwrap();
            drop(self.directory);
            self.payloads.common.close().unwrap();
            drop(self.payloads);
            fs::remove_dir(self.root.join("private-backing")).unwrap();
            fs::remove_dir(self.root).unwrap();
        }
    }

    fn key(serial: u64) -> Vec<u8> {
        let mut key = vec![b'I'];
        key.extend_from_slice(&serial.to_be_bytes());
        key
    }
    fn physical(bytes: &[u8]) -> PageRef {
        PageRef {
            id: u64::from_be_bytes(bytes[..8].try_into().unwrap()),
            epoch: u64::from_be_bytes(bytes[8..16].try_into().unwrap()),
        }
    }
    fn locator(reference: PageRef) -> Vec<u8> {
        [reference.id.to_be_bytes(), reference.epoch.to_be_bytes()].concat()
    }

    #[test]
    fn pooled_pages_capture_and_exact_refund() {
        let f = Fixture::new(8 << 20);
        let index = Index::new(f.store.clone()).unwrap();
        let pack = TinyPack::new(f.store.clone()).unwrap();
        let mut updates = Vec::new();
        for serial in 1..=128u64 {
            updates.push((key(serial), Some(vec![serial as u8; 24])));
        }
        index.prepare(&updates).unwrap().publish().unwrap();
        assert_eq!(index.get(&key(128)).unwrap(), Some(vec![128; 24]));
        let first = pack.prepare(1, 1, 2, 5, b"a").unwrap();
        let first_slot = first.slot();
        let (logical, first_physical) = first.locator();
        let locator_key = [vec![b'P'], logical.to_be_bytes().to_vec()].concat();
        index
            .prepare(&[(locator_key.clone(), Some(locator(first_physical)))])
            .unwrap()
            .publish()
            .unwrap();
        first.publish().unwrap();
        assert_eq!(pack.read(first_slot, first_physical).unwrap(), b"a");
        let snapshot = index.capture().unwrap();
        let pin = f.store.pin(first_physical).unwrap();
        let second = pack.prepare(2, 2, 3, 8, b"b").unwrap();
        let second_slot = second.slot();
        let (_, second_physical) = second.locator();
        assert_eq!(second_slot.logical_page, first_slot.logical_page);
        index
            .prepare(&[(locator_key.clone(), Some(locator(second_physical)))])
            .unwrap()
            .publish()
            .unwrap();
        second.publish().unwrap();
        assert_eq!(
            physical(&snapshot.get(&locator_key).unwrap().unwrap()),
            first_physical
        );
        assert_eq!(
            physical(&index.get(&locator_key).unwrap().unwrap()),
            second_physical
        );
        assert_eq!(pack.read(first_slot, first_physical).unwrap(), b"a");
        assert_eq!(pack.read(second_slot, second_physical).unwrap(), b"b");
        assert!(f.store.release(first_physical).is_err());
        snapshot.release().unwrap();
        drop(pin);
        f.store.release(first_physical).unwrap();
        let status = f.store.status().unwrap();
        assert_eq!(status.allocated_bytes, f.physical());
        assert_eq!(f.payloads.status().unwrap().allocated_bytes, f.physical());
        drop(pack);
        drop(index);
        f.clean();
    }

    #[test]
    fn quota_refusal_keeps_first_page() {
        let f = Fixture::new(4096);
        let first = f.store.create(Kind::Pack, 1, 1, 1, b"one").unwrap();
        assert!(f.store.create(Kind::Pack, 1, 2, 1, b"two").is_err());
        assert_eq!(
            f.store
                .read(first, Kind::Pack)
                .unwrap()
                .verify(Kind::Pack, [9; 32], first)
                .unwrap(),
            b"one"
        );
        assert_eq!(f.physical(), 4096);
        f.clean();
    }
}
