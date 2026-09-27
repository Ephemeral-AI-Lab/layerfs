use layerfs_workspace::backing::active::{namespace_key, Extent, ExtentKind, Kind, Page, PageRef};

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

#[test]
fn namespace_keys_follow_canonical_name_order() {
    let mut keys = [b"z".as_slice(), b"aa", b"a"].map(|name| namespace_key(7, name).unwrap());
    keys.sort();
    assert_eq!(
        keys,
        [b"a".as_slice(), b"aa", b"z"].map(|name| namespace_key(7, name).unwrap())
    );
    assert!(namespace_key(7, b"z").unwrap() < namespace_key(8, b"a").unwrap());
}

#[test]
fn large_payload_extent_has_checked_range_and_inverse_identity() {
    let extent = Extent::payload(10, 5, 9, 5).unwrap();
    assert_eq!(extent.kind, ExtentKind::Payload);
    let key = Extent::key(7, 10);
    assert_eq!(
        Extent::parse(&key, &extent.value().unwrap(), 7).unwrap(),
        extent
    );
    let inverse = extent.inverse_key(7).unwrap();
    assert_eq!(inverse.len(), 25);
    assert_eq!(inverse[0], b'L');
    assert_eq!(&inverse[1..9], &9u64.to_be_bytes());
    assert_eq!(&inverse[9..17], &7u64.to_be_bytes());
    assert_eq!(&inverse[17..25], &10u64.to_be_bytes());
    assert!(Extent::payload(10, 6, 9, 5).is_err());
    assert!(Extent::payload(10, 5, 0, 5).is_err());
}

#[cfg(target_os = "linux")]
mod linux {
    use super::{Kind, PageRef};
    use layerfs_bridge::contract::Source;
    use layerfs_workspace::backing::{
        active::{
            dirty_key, inode_key, namespace_key, ActiveBacking, Extent, ExtentKind, ExtentPlan,
            HotInode, Index, NamespaceRecord, PackedSlot, PageStore, TinyPack,
        },
        budget::Budget,
        metadata::MetadataHost,
        payload::PayloadHost,
    };
    use layerfs_workspace::{NodeKind, OwnedPayload, PortableAttributes, WorkspaceError};
    use std::{
        collections::BTreeSet,
        fs,
        os::unix::fs::{DirBuilderExt, MetadataExt},
        path::PathBuf,
        sync::{
            atomic::{AtomicBool, AtomicU64, Ordering},
            Arc,
        },
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
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
            let started = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let root = parent.canonicalize().unwrap().join(format!(
                "active-{}-{}-{started}",
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

    fn render(index: &Index, pack: &TinyPack, length: usize) -> Vec<u8> {
        let mut lower = Extent::key(1, 0).to_vec();
        let upper = Extent::key(1, layerfs_bridge::contract::MAX_FILE);
        let mut actual = Vec::new();
        loop {
            let page = index.scan(&lower, &upper, 128).unwrap();
            if page.entries().is_empty() {
                break;
            }
            for (key, value) in page.entries() {
                let extent = Extent::parse(key, value, 1).unwrap();
                assert_eq!(extent.start as usize, actual.len());
                match extent.kind {
                    ExtentKind::Base => {
                        actual.extend(vec![b'A'; (extent.end - extent.start) as usize])
                    }
                    ExtentKind::Zero => {
                        actual.extend(vec![0; (extent.end - extent.start) as usize])
                    }
                    ExtentKind::Packed => {
                        let locator_key =
                            [vec![b'P'], extent.logical_page.to_be_bytes().to_vec()].concat();
                        let physical = physical(&index.get(&locator_key).unwrap().unwrap());
                        let slot = PackedSlot {
                            logical_page: extent.logical_page,
                            ordinal: extent.ordinal,
                            inode: 1,
                            generation: extent.generation,
                            revision: extent.revision,
                            offset: extent.start - extent.source_offset,
                            length: extent.slot_length,
                        };
                        let bytes = pack.read(slot, physical).unwrap();
                        actual.extend(
                            &bytes[extent.source_offset as usize
                                ..extent.source_offset as usize
                                    + (extent.end - extent.start) as usize],
                        );
                    }
                    ExtentKind::Payload => panic!("this fixture uses packed slots only"),
                }
            }
            lower = page.entries().last().unwrap().0.clone();
            lower.push(0);
        }
        assert_eq!(actual.len(), length);
        actual
    }

    fn read_owned(
        payload: &OwnedPayload,
        source: u64,
        output: &mut [u8],
    ) -> Result<(), WorkspaceError> {
        let mut reader = payload.reader(source..source + output.len() as u64)?;
        let cancel = AtomicBool::new(false);
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut received = 0;
        while received < output.len() {
            let count = Source::read(&mut reader, &mut output[received..], deadline, &cancel)
                .map_err(|_| WorkspaceError::Io)?;
            if count == 0 {
                return Err(WorkspaceError::Io);
            }
            received += count;
        }
        Ok(())
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
        assert_eq!(index.floor(&key(64)).unwrap().unwrap().0, key(64));
        let page = index.scan(&key(120), &key(129), 128).unwrap();
        assert_eq!(page.entries().len(), 9);
        assert_eq!(page.entries().first().unwrap().0, key(120));
        drop(page);
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

    #[test]
    fn corrupted_inactive_tail_aborts_without_changing_acknowledged_bytes() {
        use std::os::unix::fs::FileExt;
        let f = Fixture::new(64 << 10);
        let pack = TinyPack::new(f.store.clone()).unwrap();
        let first = pack.prepare(1, 1, 1, 2, b"A").unwrap();
        let first_slot = first.slot();
        let (_, acknowledged) = first.locator();
        first.publish().unwrap();
        let pending = pack.prepare(2, 1, 2, 4, b"B").unwrap();
        let (_, candidate) = pending.locator();
        let path = f.directory.path.join(format!(
            "a-pack-v1-{:016x}-{:016x}",
            candidate.id, candidate.epoch
        ));
        let file = fs::OpenOptions::new().write(true).open(path).unwrap();
        assert_eq!(file.write_at(&[0xff], 128).unwrap(), 1);
        assert!(f.store.read(candidate, Kind::Pack).is_err());
        assert_eq!(pack.read(first_slot, acknowledged).unwrap(), b"A");
        drop(file);
        pending.abort().unwrap();
        assert_eq!(f.store.status().unwrap().allocated_bytes, 4096);
        assert_eq!(f.physical(), 4096);
        drop(pack);
        f.clean();
    }

    #[test]
    fn denied_tail_write_keeps_prior_acknowledgement_and_charged_candidate() {
        use nix::libc;
        let f = Fixture::new(64 << 10);
        let pack = TinyPack::new(f.store.clone()).unwrap();
        let first = pack.prepare(1, 1, 1, 2, b"A").unwrap();
        let first_slot = first.slot();
        let (_, acknowledged) = first.locator();
        first.publish().unwrap();
        let mut filter = [
            libc::sock_filter {
                code: (libc::BPF_LD | libc::BPF_W | libc::BPF_ABS) as u16,
                jt: 0,
                jf: 0,
                k: 0,
            },
            libc::sock_filter {
                code: (libc::BPF_JMP | libc::BPF_JEQ | libc::BPF_K) as u16,
                jt: 0,
                jf: 1,
                k: libc::SYS_pwrite64 as u32,
            },
            libc::sock_filter {
                code: (libc::BPF_RET | libc::BPF_K) as u16,
                jt: 0,
                jf: 0,
                k: libc::SECCOMP_RET_ERRNO | libc::EIO as u32,
            },
            libc::sock_filter {
                code: (libc::BPF_RET | libc::BPF_K) as u16,
                jt: 0,
                jf: 0,
                k: libc::SECCOMP_RET_ALLOW,
            },
        ];
        let program = libc::sock_fprog {
            len: filter.len() as u16,
            filter: filter.as_mut_ptr(),
        };
        // External syscall fault, installed only in this test thread. Product
        // source has no fault hook or alternate write path.
        unsafe {
            assert_eq!(libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0), 0);
            assert_eq!(
                libc::prctl(libc::PR_SET_SECCOMP, libc::SECCOMP_MODE_FILTER, &program),
                0
            );
        }
        let error = pack.prepare(2, 1, 2, 4, b"B").err().unwrap();
        assert!(
            matches!(error, layerfs_workspace::WorkspaceError::Backing(ref failure)
            if failure.phase == layerfs_workspace::BackingPhase::Write
                && failure.allocated_bytes == 4096)
        );
        assert_eq!(pack.read(first_slot, acknowledged).unwrap(), b"A");
        let status = f.store.status().unwrap();
        assert_eq!(
            (
                status.pages,
                status.incomplete_pages,
                status.allocated_bytes,
                status.admission_stopped
            ),
            (2, 1, 8192, true)
        );
        drop(pack);
        f.clean();
    }

    #[test]
    fn overlapping_tiny_edits_append_and_hole_have_one_final_view() {
        let f = Fixture::new(8 << 20);
        let index = Index::new(f.store.clone()).unwrap();
        let pack = TinyPack::new(f.store.clone()).unwrap();
        let mut expected = vec![b'A'; 16];
        let mut length = 16;
        for (revision, (offset, byte)) in [(2, b'X'), (4, b'Y'), (2, b'Z'), (16, b'Q'), (20, b'W')]
            .into_iter()
            .enumerate()
        {
            let prepared = pack
                .prepare(1, 1, revision as u64 + 1, offset, &[byte])
                .unwrap();
            let plan = ExtentPlan::tiny(&index, 1, length, prepared.slot()).unwrap();
            length = plan.length;
            let mut updates = plan.updates;
            let (logical, physical) = prepared.locator();
            let locator_key = [vec![b'P'], logical.to_be_bytes().to_vec()].concat();
            updates.push((locator_key, Some(locator(physical))));
            updates.sort_by(|a, b| a.0.cmp(&b.0));
            index.prepare(&updates).unwrap().publish().unwrap();
            prepared.publish().unwrap();
            expected.resize(length as usize, 0);
            expected[offset as usize] = byte;
        }
        assert_eq!(render(&index, &pack, length as usize), expected);
        drop(pack);
        drop(index);
        f.clean();
    }

    #[test]
    fn separated_4096_storage_stays_within_three_mib() {
        let f = Fixture::new(3 << 20);
        let index = Index::new(f.store.clone()).unwrap();
        let pack = TinyPack::new(f.store.clone()).unwrap();
        let mut expected = vec![b'A'; 8194];
        for write in 0..4096u64 {
            let offset = write * 2;
            let prepared = pack.prepare(1, 1, write + 1, offset, b"X").unwrap();
            let plan = ExtentPlan::tiny(&index, 1, 8194, prepared.slot()).unwrap();
            let mut updates = plan.updates;
            let (logical, physical) = prepared.locator();
            let locator_key = [vec![b'P'], logical.to_be_bytes().to_vec()].concat();
            updates.push((locator_key, Some(locator(physical))));
            updates.sort_by(|a, b| a.0.cmp(&b.0));
            index.prepare(&updates).unwrap().publish().unwrap();
            let retired = prepared.retired_physical();
            prepared.publish().unwrap();
            if let Some(retired) = retired {
                f.store.release(retired).unwrap();
            }
            expected[offset as usize] = b'X';
        }
        index.maintain().unwrap();
        assert_eq!(render(&index, &pack, 8194), expected);
        let status = f.store.status().unwrap();
        println!(
            "ACTIVE_SEPARATED writes=4096 allocated_bytes={} physical_bytes={} pages={}",
            status.allocated_bytes,
            f.physical(),
            status.pages
        );
        assert_eq!(status.allocated_bytes, f.physical());
        assert!(f.physical() <= 3 << 20);
        drop(pack);
        drop(index);
        f.clean();
    }

    #[test]
    fn captured_tail_keeps_g1_and_g2_bytes() {
        let f = Fixture::new(3 << 20);
        let active = ActiveBacking::new(
            f.directory.clone(),
            MetadataHost::new(f.payloads.clone()).unwrap(),
        )
        .unwrap();
        let g1 = active.write_tiny(1, 16, 2, b"X", &[]).unwrap();
        assert!(g1.cleanup_error.is_none());
        let old = active.capture().unwrap();
        let g2 = active.write_tiny(1, 16, 2, b"Y", &[]).unwrap();
        assert!(g2.cleanup_error.is_none());
        let middle = active.capture().unwrap();
        let g3 = active.write_tiny(1, 16, 2, b"Z", &[]).unwrap();
        assert!(g3.cleanup_error.is_none());
        assert_eq!(active.read(g1.slot, Some(&old)).unwrap(), b"X");
        assert_eq!(active.read(g2.slot, Some(&middle)).unwrap(), b"Y");
        assert_eq!(active.read(g3.slot, None).unwrap(), b"Z");
        assert!(active.status().unwrap().retired_pack_pages >= 2);
        old.release().unwrap();
        middle.release().unwrap();
        assert_eq!(active.status().unwrap().retired_pack_pages, 0);
        assert_eq!(active.status().unwrap().store.allocated_bytes, f.physical());
        active.close_clean().unwrap();
        drop(active);
        f.clean();
    }

    #[test]
    fn file_write_publishes_inode_extent_and_dirty_together() {
        let f = Fixture::new(3 << 20);
        let active = ActiveBacking::new(
            f.directory.clone(),
            MetadataHost::new(f.payloads.clone()).unwrap(),
        )
        .unwrap();
        let initial = HotInode {
            revision: 1,
            generation: 1,
            length: 0,
            kind: NodeKind::File,
            fresh: true,
            storage: 0,
            mode: 0o644,
            seconds: 0,
            nanos: 0,
            links: 1,
            base: [0; 32],
            metadata: [0; 32],
            inline: [None; 4],
        };
        let first = active.write_tiny_file(1, initial, 0, b"A").unwrap();
        assert!(first.cleanup_error.is_none());
        let inode = HotInode::parse(&active.get(&inode_key(1)).unwrap().unwrap()).unwrap();
        assert_eq!((inode.length, inode.revision, inode.storage), (1, 1, 2));
        assert_eq!(active.get(&dirty_key(1, 1)).unwrap(), Some(vec![1]));
        assert!(active.get(&Extent::key(1, 0)).unwrap().is_some());
        let second = active.write_tiny_file(1, inode, 3, b"Z").unwrap();
        assert!(second.cleanup_error.is_none());
        let frozen = active.capture().unwrap();
        let old = HotInode::parse(&frozen.get(&inode_key(1)).unwrap().unwrap()).unwrap();
        assert_eq!((old.length, old.generation, old.revision), (4, 1, 2));
        let third = active.write_tiny_file(1, old, 4, b"Q").unwrap();
        assert!(third.cleanup_error.is_none());
        assert_eq!(
            HotInode::parse(&frozen.get(&inode_key(1)).unwrap().unwrap())
                .unwrap()
                .length,
            4
        );
        assert_eq!(
            HotInode::parse(&active.get(&inode_key(1)).unwrap().unwrap())
                .unwrap()
                .length,
            5
        );
        assert_eq!(active.read(second.slot, Some(&frozen)).unwrap(), b"Z");
        assert_eq!(active.read(third.slot, None).unwrap(), b"Q");
        frozen.release().unwrap();
        active.close_clean().unwrap();
        drop(active);
        f.clean();
    }

    #[test]
    fn namespace_records_share_one_revision_and_freeze_with_dirty_state() {
        let f = Fixture::new(3 << 20);
        let active = ActiveBacking::new(
            f.directory.clone(),
            MetadataHost::new(f.payloads.clone()).unwrap(),
        )
        .unwrap();
        let directory = HotInode {
            revision: 1,
            generation: 1,
            length: 0,
            kind: NodeKind::Directory,
            fresh: true,
            storage: 0,
            mode: 0o755,
            seconds: 0,
            nanos: 0,
            links: 1,
            base: [0; 32],
            metadata: [0; 32],
            inline: [None; 4],
        };
        let old = namespace_key(1, b"old").unwrap();
        let new = namespace_key(1, b"new").unwrap();
        let binding = NamespaceRecord {
            serial: 2,
            kind: NodeKind::Directory,
            tombstone: false,
        };
        assert_eq!(
            active
                .publish_records(&[
                    (dirty_key(1, 2).to_vec(), Some(vec![1])),
                    (
                        inode_key(2).to_vec(),
                        Some(directory.value().unwrap().to_vec())
                    ),
                    (old.clone(), Some(binding.value().unwrap().to_vec())),
                ])
                .unwrap(),
            1
        );
        let frozen = active.capture().unwrap();
        let tombstone = NamespaceRecord {
            tombstone: true,
            ..binding
        };
        assert_eq!(
            active
                .publish_records(&[
                    (dirty_key(2, 2).to_vec(), Some(vec![1])),
                    (new.clone(), Some(binding.value().unwrap().to_vec())),
                    (old.clone(), Some(tombstone.value().unwrap().to_vec())),
                ])
                .unwrap(),
            3 // capture advances the active revision with the Workspace revision
        );
        assert_eq!(
            frozen.get(&old).unwrap(),
            Some(binding.value().unwrap().to_vec())
        );
        assert_eq!(frozen.get(&new).unwrap(), None);
        assert_eq!(frozen.get(&dirty_key(1, 2)).unwrap(), Some(vec![1]));
        assert_eq!(frozen.get(&dirty_key(2, 2)).unwrap(), None);
        assert_eq!(
            active.get(&old).unwrap(),
            Some(tombstone.value().unwrap().to_vec())
        );
        assert_eq!(
            active.get(&new).unwrap(),
            Some(binding.value().unwrap().to_vec())
        );
        assert_eq!(active.scan(&[b'D'], &[b'E'], 8).unwrap().entries().len(), 2);
        frozen.release().unwrap();
        active.close_clean().unwrap();
        drop(active);
        f.clean();
    }

    #[test]
    fn indexed_read_keeps_base_holes_and_frozen_packed_bytes() {
        let f = Fixture::new(3 << 20);
        let active = ActiveBacking::new(
            f.directory.clone(),
            MetadataHost::new(f.payloads.clone()).unwrap(),
        )
        .unwrap();
        let inherited = HotInode {
            revision: 1,
            generation: 1,
            length: 16,
            kind: NodeKind::File,
            fresh: false,
            storage: 2,
            mode: 0o644,
            seconds: 0,
            nanos: 0,
            links: 1,
            base: [7; 32],
            metadata: [8; 32],
            inline: [None; 4],
        };
        active.write_tiny_file(1, inherited, 2, b"X").unwrap();
        let frozen = active.capture().unwrap();
        let current = HotInode::parse(&active.get(&inode_key(1)).unwrap().unwrap()).unwrap();
        active.write_tiny_file(1, current, 20, b"Z").unwrap();
        let base = |root, source: u64, output: &mut [u8]| {
            assert_eq!(root, [7; 32]);
            assert!(source + output.len() as u64 <= 16);
            output.fill(b'A');
            Ok(())
        };
        let mut old = [0; 21];
        assert_eq!(
            active
                .read_file(
                    1,
                    0,
                    &mut old,
                    Some(&frozen),
                    base,
                    |_, _, _| unreachable!()
                )
                .unwrap(),
            16
        );
        assert_eq!(&old[..16], b"AAXAAAAAAAAAAAAA");
        let mut live = [0; 21];
        assert_eq!(
            active
                .read_file(1, 0, &mut live, None, base, |_, _, _| unreachable!())
                .unwrap(),
            21
        );
        assert_eq!(&live, b"AAXAAAAAAAAAAAAA\0\0\0\0Z");
        frozen.release().unwrap();
        active.close_clean().unwrap();
        drop(active);
        f.clean();
    }

    #[test]
    fn truncate_then_extend_zeroes_old_bytes_without_changing_frozen_view() {
        let f = Fixture::new(3 << 20);
        let active = ActiveBacking::new(
            f.directory.clone(),
            MetadataHost::new(f.payloads.clone()).unwrap(),
        )
        .unwrap();
        let initial = HotInode {
            revision: 1,
            generation: 1,
            length: 16,
            kind: NodeKind::File,
            fresh: false,
            storage: 2,
            mode: 0o644,
            seconds: 0,
            nanos: 0,
            links: 1,
            base: [7; 32],
            metadata: [8; 32],
            inline: [None; 4],
        };
        active.write_tiny_file(1, initial, 2, b"X").unwrap();
        let frozen = active.capture().unwrap();
        let selected = HotInode::parse(&active.get(&inode_key(1)).unwrap().unwrap()).unwrap();
        let mtime = (selected.seconds, selected.nanos);
        assert_eq!(active.resize_file(1, selected, 0).unwrap().length, 0);
        let selected = HotInode::parse(&active.get(&inode_key(1)).unwrap().unwrap()).unwrap();
        assert_eq!((selected.seconds, selected.nanos), mtime);
        assert_eq!(active.resize_file(1, selected, 5).unwrap().length, 5);
        let selected = HotInode::parse(&active.get(&inode_key(1)).unwrap().unwrap()).unwrap();
        assert_eq!((selected.seconds, selected.nanos), mtime);
        let mut live = [1; 5];
        assert_eq!(
            active
                .read_file(
                    1,
                    0,
                    &mut live,
                    None,
                    |_, _, _| unreachable!(),
                    |_, _, _| unreachable!()
                )
                .unwrap(),
            5
        );
        assert_eq!(live, [0; 5]);
        let mut old = [0; 16];
        let base = |root, source: u64, output: &mut [u8]| {
            assert_eq!(root, [7; 32]);
            assert!(source + output.len() as u64 <= 16);
            output.fill(b'A');
            Ok(())
        };
        assert_eq!(
            active
                .read_file(
                    1,
                    0,
                    &mut old,
                    Some(&frozen),
                    base,
                    |_, _, _| unreachable!()
                )
                .unwrap(),
            16
        );
        assert_eq!(&old, b"AAXAAAAAAAAAAAAA");
        let selected = HotInode::parse(&active.get(&inode_key(1)).unwrap().unwrap()).unwrap();
        active.write_tiny_file(1, selected, 2, b"Z").unwrap();
        let mut live = [0; 5];
        active
            .read_file(
                1,
                0,
                &mut live,
                None,
                |_, _, _| unreachable!(),
                |_, _, _| unreachable!(),
            )
            .unwrap();
        assert_eq!(live, [0, 0, b'Z', 0, 0]);
        frozen.release().unwrap();
        active.close_clean().unwrap();
        drop(active);
        f.clean();
    }

    #[test]
    fn metadata_first_touch_keeps_inherited_bytes_and_selected_mtime() {
        let f = Fixture::new(3 << 20);
        let active = ActiveBacking::new(
            f.directory.clone(),
            MetadataHost::new(f.payloads.clone()).unwrap(),
        )
        .unwrap();
        let original = HotInode {
            revision: 1,
            generation: 1,
            length: 8,
            kind: NodeKind::File,
            fresh: false,
            storage: 2,
            mode: 0o644,
            seconds: 7,
            nanos: 8,
            links: 1,
            base: [7; 32],
            metadata: [8; 32],
            inline: [None; 4],
        };
        active
            .set_attributes_file(
                1,
                original,
                PortableAttributes {
                    mode: Some(0o600),
                    mtime: Some((123, 456)),
                    ..PortableAttributes::default()
                },
            )
            .unwrap();
        let selected = HotInode::parse(&active.get(&inode_key(1)).unwrap().unwrap()).unwrap();
        assert_eq!(
            (
                selected.storage,
                selected.mode,
                selected.seconds,
                selected.nanos
            ),
            (1, 0o600, 123, 456)
        );
        let mut bytes = [0; 8];
        active
            .read_file(
                1,
                0,
                &mut bytes,
                None,
                |root, _, output| {
                    assert_eq!(root, [7; 32]);
                    output.fill(b'A');
                    Ok(())
                },
                |_, _, _| unreachable!(),
            )
            .unwrap();
        assert_eq!(&bytes, b"AAAAAAAA");
        active.resize_file(1, selected, 3).unwrap();
        let selected = HotInode::parse(&active.get(&inode_key(1)).unwrap().unwrap()).unwrap();
        assert_eq!(
            (
                selected.length,
                selected.mode,
                selected.seconds,
                selected.nanos
            ),
            (3, 0o600, 123, 456)
        );
        active.close_clean().unwrap();
        drop(active);
        f.clean();
    }

    #[test]
    fn large_payload_and_tiny_overlap_share_one_active_view() {
        let f = Fixture::new(8 << 20);
        let active = ActiveBacking::new(
            f.directory.clone(),
            MetadataHost::new(f.payloads.clone()).unwrap(),
        )
        .unwrap();
        let original = HotInode {
            revision: 1,
            generation: 1,
            length: 16,
            kind: NodeKind::File,
            fresh: false,
            storage: 2,
            mode: 0o644,
            seconds: 0,
            nanos: 0,
            links: 1,
            base: [7; 32],
            metadata: [8; 32],
            inline: [None; 4],
        };
        let data: Vec<u8> = (0..512).map(|index| b'B' + (index % 24) as u8).collect();
        let mut input = data.as_slice();
        let cancel = AtomicBool::new(false);
        let payload = f
            .payloads
            .acquire(
                f.directory.clone(),
                data.len() as u64,
                &mut input,
                Instant::now() + Duration::from_secs(10),
                &cancel,
            )
            .unwrap();
        let published = active.write_payload_file(1, original, 2, &payload).unwrap();
        assert_eq!((published.length, published.revision), (514, 1));
        assert!(published.cleanup_error.is_none());
        drop(payload);
        let frozen = active.capture().unwrap();
        let selected = HotInode::parse(&active.get(&inode_key(1)).unwrap().unwrap()).unwrap();
        let tiny = active.write_tiny_file(1, selected, 3, b"Z").unwrap();
        assert!(tiny.cleanup_error.is_none());
        let mut old = vec![0; 514];
        let base = |root, source: u64, output: &mut [u8]| {
            assert_eq!(root, [7; 32]);
            assert!(source + output.len() as u64 <= 16);
            output.fill(b'A');
            Ok(())
        };
        assert_eq!(
            active
                .read_file(1, 0, &mut old, Some(&frozen), base, read_owned)
                .unwrap(),
            514
        );
        assert_eq!(&old[..2], b"AA");
        assert_eq!(&old[2..], data);
        let mut live = vec![0; 514];
        assert_eq!(
            active
                .read_file(1, 0, &mut live, None, base, read_owned)
                .unwrap(),
            514
        );
        assert_eq!(live[3], b'Z');
        live[3] = old[3];
        assert_eq!(live, old);
        frozen.release().unwrap();
        active.close_clean().unwrap();
        drop(active);
        f.payloads
            .reclaim(
                f.directory.incarnation,
                Instant::now() + Duration::from_secs(10),
            )
            .unwrap();
        f.clean();
    }

    #[test]
    fn large_overwrite_crosses_index_pages_and_keeps_pack_tail_valid() {
        let f = Fixture::new(8 << 20);
        let active = ActiveBacking::new(
            f.directory.clone(),
            MetadataHost::new(f.payloads.clone()).unwrap(),
        )
        .unwrap();
        for offset in 0..300 {
            active.write_tiny(1, offset, offset, b"x", &[]).unwrap();
        }
        let data = vec![b'Q'; 300];
        let mut input = data.as_slice();
        let cancel = AtomicBool::new(false);
        let payload = f
            .payloads
            .acquire(
                f.directory.clone(),
                300,
                &mut input,
                Instant::now() + Duration::from_secs(10),
                &cancel,
            )
            .unwrap();
        let initial = HotInode {
            revision: 1,
            generation: 1,
            length: 300,
            kind: NodeKind::File,
            fresh: false,
            storage: 2,
            mode: 0o644,
            seconds: 0,
            nanos: 0,
            links: 1,
            base: [7; 32],
            metadata: [8; 32],
            inline: [None; 4],
        };
        active.write_payload_file(1, initial, 0, &payload).unwrap();
        drop(payload);
        let selected = HotInode::parse(&active.get(&inode_key(1)).unwrap().unwrap()).unwrap();
        active.write_tiny_file(1, selected, 300, b"Z").unwrap();
        let mut actual = [0; 301];
        assert_eq!(
            active
                .read_file(
                    1,
                    0,
                    &mut actual,
                    None,
                    |_, _, _| unreachable!(),
                    read_owned
                )
                .unwrap(),
            301
        );
        assert_eq!(&actual[..300], data);
        assert_eq!(actual[300], b'Z');
        active.close_clean().unwrap();
        drop(active);
        f.payloads
            .reclaim(
                f.directory.incarnation,
                Instant::now() + Duration::from_secs(10),
            )
            .unwrap();
        f.clean();
    }

    #[test]
    fn read_view_pins_same_generation_revision_without_capture() {
        let f = Fixture::new(3 << 20);
        let active = ActiveBacking::new(
            f.directory.clone(),
            MetadataHost::new(f.payloads.clone()).unwrap(),
        )
        .unwrap();
        let first = active.write_tiny(1, 0, 0, b"A", &[]).unwrap();
        let old_name = namespace_key(1, b"old").unwrap();
        let binding = NamespaceRecord {
            serial: 1,
            kind: NodeKind::File,
            tombstone: false,
        };
        active
            .publish_records(&[(old_name.clone(), Some(binding.value().unwrap().to_vec()))])
            .unwrap();
        let view = active.pin_view().unwrap();
        let second = active.write_tiny(1, 1, 0, b"B", &[]).unwrap();
        active
            .publish_records(&[(
                old_name.clone(),
                Some(
                    NamespaceRecord {
                        tombstone: true,
                        ..binding
                    }
                    .value()
                    .unwrap()
                    .to_vec(),
                ),
            )])
            .unwrap();
        assert_eq!(view.generation, 1);
        assert_eq!(active.read(first.slot, Some(&view)).unwrap(), b"A");
        assert_eq!(active.read(second.slot, None).unwrap(), b"B");
        assert_eq!(
            view.get(&old_name).unwrap(),
            Some(binding.value().unwrap().to_vec())
        );
        assert_eq!(
            NamespaceRecord::parse(&active.get(&old_name).unwrap().unwrap())
                .unwrap()
                .tombstone,
            true
        );
        drop(view);
        assert_eq!(active.status().unwrap().retired_pack_pages, 0);
        active.close_clean().unwrap();
        drop(active);
        f.clean();
    }

    #[test]
    fn one_tiny_write_may_replace_128_one_byte_extents() {
        let f = Fixture::new(64 << 20);
        let active = ActiveBacking::new(
            f.directory.clone(),
            MetadataHost::new(f.payloads.clone()).unwrap(),
        )
        .unwrap();
        for offset in 0..128 {
            let result = active.write_tiny(1, offset, offset, b"x", &[]).unwrap();
            assert!(result.cleanup_error.is_none());
        }
        let result = active.write_tiny(1, 128, 0, &[b'Z'; 128], &[]).unwrap();
        assert!(result.cleanup_error.is_none());
        assert_eq!(active.read(result.slot, None).unwrap(), vec![b'Z'; 128]);
        active.close_clean().unwrap();
        drop(active);
        f.clean();
    }

    #[test]
    fn repeated_4097_reclaims_dead_sealed_pages() {
        let f = Fixture::new(3 << 20);
        let active = ActiveBacking::new(
            f.directory.clone(),
            MetadataHost::new(f.payloads.clone()).unwrap(),
        )
        .unwrap();
        let mut last = None;
        for write in 0..4097u64 {
            let byte = b'B' + (write % 24) as u8;
            let result = active.write_tiny(1, 8194, 4097, &[byte], &[]).unwrap();
            assert!(result.cleanup_error.is_none());
            last = Some((result.slot, byte));
        }
        let (slot, byte) = last.unwrap();
        assert_eq!(active.read(slot, None).unwrap(), vec![byte]);
        let status = active.status().unwrap();
        println!(
            "ACTIVE_REPEAT allocated_bytes={} physical_bytes={} pages={} retired={}",
            status.store.allocated_bytes,
            f.physical(),
            status.store.pages,
            status.retired_pack_pages
        );
        assert_eq!(status.store.allocated_bytes, f.physical());
        assert_eq!(status.retired_pack_pages, 0);
        assert!(status.store.allocated_bytes <= 512 << 10);
        active.close_clean().unwrap();
        drop(active);
        f.clean();
    }

    #[test]
    fn one_pack_is_shared_by_128_short_files() {
        let f = Fixture::new(1 << 20);
        let active = ActiveBacking::new(
            f.directory.clone(),
            MetadataHost::new(f.payloads.clone()).unwrap(),
        )
        .unwrap();
        let mut slots = Vec::new();
        for inode in 1..=128 {
            let result = active.write_tiny(inode, 0, 0, &[inode as u8], &[]).unwrap();
            assert!(result.cleanup_error.is_none());
            slots.push(result.slot);
        }
        for (inode, slot) in slots.into_iter().enumerate() {
            assert_eq!(active.read(slot, None).unwrap(), vec![(inode + 1) as u8]);
        }
        let pack_files = fs::read_dir(&*f.directory.path)
            .unwrap()
            .filter(|entry| {
                entry
                    .as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with("a-pack-v1-")
            })
            .count();
        let status = active.status().unwrap();
        println!(
            "ACTIVE_MANY files=128 pack_pages={pack_files} allocated_bytes={} physical_bytes={}",
            status.store.allocated_bytes,
            f.physical()
        );
        assert_eq!(pack_files, 2);
        assert_eq!(status.store.allocated_bytes, f.physical());
        assert!(f.physical() <= 512 << 10);
        active.close_clean().unwrap();
        drop(active);
        f.clean();
    }

    #[test]
    fn thirty_two_retained_generations_release_exact_blocks() {
        let f = Fixture::new(3 << 20);
        let active = ActiveBacking::new(
            f.directory.clone(),
            MetadataHost::new(f.payloads.clone()).unwrap(),
        )
        .unwrap();
        let mut captured = Vec::new();
        for generation in 0..32u8 {
            let byte = b'A' + generation;
            let result = active.write_tiny(1, 16, 2, &[byte], &[]).unwrap();
            assert!(result.cleanup_error.is_none());
            captured.push((active.capture().unwrap(), result.slot, byte));
        }
        for (snapshot, slot, byte) in &captured {
            assert_eq!(active.read(*slot, Some(snapshot)).unwrap(), vec![*byte]);
        }
        let status = active.status().unwrap();
        println!(
            "ACTIVE_RETAINED generations=32 allocated_bytes={} physical_bytes={} retired={}",
            status.store.allocated_bytes,
            f.physical(),
            status.retired_pack_pages
        );
        assert_eq!(status.store.allocated_bytes, f.physical());
        assert!(f.physical() <= 3 << 20);
        for (snapshot, _, _) in captured {
            snapshot.release().unwrap();
        }
        assert_eq!(active.status().unwrap().retired_pack_pages, 0);
        active.close_clean().unwrap();
        drop(active);
        f.clean();
    }

    #[test]
    fn active_quota_refusal_does_not_publish_a_partial_write() {
        // The first index update needs one unselected page while replacing
        // another. Twelve KiB admits it; the second write needs a fourth page
        // while its new pack candidate and old index are still owned.
        let f = Fixture::new(12 << 10);
        let active = ActiveBacking::new(
            f.directory.clone(),
            MetadataHost::new(f.payloads.clone()).unwrap(),
        )
        .unwrap();
        let first = active.write_tiny(1, 16, 2, b"X", &[]).unwrap();
        assert!(active.write_tiny(1, 16, 2, b"Y", &[]).is_err());
        assert_eq!(active.read(first.slot, None).unwrap(), b"X");
        let status = active.status().unwrap();
        assert_eq!(status.store.allocated_bytes, f.physical());
        assert_eq!(status.store.reserved_bytes, 0);
        active.close_clean().unwrap();
        drop(active);
        f.clean();
    }

    #[test]
    fn quota_pressure_pools_partially_dead_sealed_pages() {
        let f = Fixture::new(2 << 20);
        let active = ActiveBacking::new(
            f.directory.clone(),
            MetadataHost::new(f.payloads.clone()).unwrap(),
        )
        .unwrap();
        for inode in 1..=128u64 {
            active.write_tiny(inode, 0, 0, b"A", &[]).unwrap();
        }
        active.capture().unwrap().release().unwrap();
        for inode in (1..=30u64).chain(83..=112) {
            active.write_tiny(inode, 1, 0, b"B", &[]).unwrap();
        }
        let old_slots: Vec<_> = (1..=128u64)
            .map(|inode| {
                let key = Extent::key(inode, 0);
                let extent =
                    Extent::parse(&key, &active.get(&key).unwrap().unwrap(), inode).unwrap();
                (extent.logical_page, extent.ordinal)
            })
            .collect();
        let pack_blocks = || {
            fs::read_dir(&*f.directory.path)
                .unwrap()
                .map(|entry| entry.unwrap())
                .filter(|entry| {
                    entry
                        .file_name()
                        .to_string_lossy()
                        .starts_with("a-pack-v1-")
                })
                .map(|entry| entry.metadata().unwrap().blocks() * 512)
                .sum::<u64>()
        };
        let old_pack_blocks = pack_blocks();
        let before = f.payloads.status().unwrap();
        let available = before.quota_bytes - before.allocated_bytes - before.reserved_bytes;
        let spend = (available - 120 * 1024) / 4096;
        let mut blocks = spend;
        while blocks + blocks.div_ceil(256) > spend {
            blocks -= 1;
        }
        let body = vec![0xcc; blocks as usize * 4096];
        let spare = f
            .payloads
            .acquire(
                f.directory.clone(),
                body.len() as u64,
                &mut &body[..],
                Instant::now() + Duration::from_secs(10),
                &AtomicBool::new(false),
            )
            .unwrap();
        let full = f.payloads.status().unwrap();
        let remaining = full.quota_bytes - full.allocated_bytes - full.reserved_bytes;
        assert!(remaining < 33 * 4096);
        let old = active.status().unwrap().store;
        let published = active
            .publish_reconcile(&[(vec![b'Z'], Some(vec![1]))])
            .unwrap();
        assert!(published.cleanup_error.is_none());
        let next = active.status().unwrap().store;
        println!("ACTIVE_PRESSURE remaining_before={remaining} pack_before={} pack_after={} moved_page_writes={}", old.pack_pages, next.pack_pages, next.pack_page_writes - old.pack_page_writes);
        assert_eq!(next.pack_page_writes - old.pack_page_writes, 1);
        assert!(next.pack_pages < old.pack_pages);
        assert_eq!(f.payloads.status().unwrap().allocated_bytes, f.physical());
        let mut changed_refs = 0;
        let mut moved_slots = BTreeSet::new();
        for inode in 1..=128u64 {
            let key = Extent::key(inode, 0);
            let extent = Extent::parse(&key, &active.get(&key).unwrap().unwrap(), inode).unwrap();
            let old_slot = old_slots[(inode - 1) as usize];
            if old_slot != (extent.logical_page, extent.ordinal) {
                changed_refs += 1;
                moved_slots.insert(old_slot);
            }
            let slot = PackedSlot {
                logical_page: extent.logical_page,
                ordinal: extent.ordinal,
                inode,
                generation: extent.generation,
                revision: extent.revision,
                offset: extent.start - extent.source_offset,
                length: extent.slot_length,
            };
            let expected = if inode <= 30 || (83..=112).contains(&inode) {
                b"B"
            } else {
                b"A"
            };
            assert_eq!(active.read(slot, None).unwrap(), expected);
        }
        assert_eq!((moved_slots.len(), changed_refs), (68, 68));
        assert_eq!(old_pack_blocks - pack_blocks(), 4096);
        println!(
            "ACTIVE_PRESSURE slots_moved={} references_changed={} pack_blocks_refunded={}",
            moved_slots.len(),
            changed_refs,
            old_pack_blocks - pack_blocks()
        );
        active.write_tiny(31, 1, 0, b"C", &[]).unwrap();
        let single = active.status().unwrap().store;
        active
            .publish_reconcile(&[(vec![b'Y'], Some(vec![1]))])
            .unwrap();
        let skipped = active.status().unwrap().store;
        assert_eq!(skipped.pack_page_writes, single.pack_page_writes);
        assert_eq!(skipped.pack_pages, single.pack_pages);
        let key = Extent::key(31, 0);
        let extent = Extent::parse(&key, &active.get(&key).unwrap().unwrap(), 31).unwrap();
        assert_eq!(
            active
                .read(
                    PackedSlot {
                        logical_page: extent.logical_page,
                        ordinal: extent.ordinal,
                        inode: 31,
                        generation: extent.generation,
                        revision: extent.revision,
                        offset: extent.start - extent.source_offset,
                        length: extent.slot_length,
                    },
                    None,
                )
                .unwrap(),
            b"C"
        );
        drop(spare);
        f.payloads
            .reclaim(
                f.directory.incarnation,
                Instant::now() + Duration::from_secs(10),
            )
            .unwrap();
        active.close_clean().unwrap();
        drop(active);
        f.clean();
    }

    #[test]
    fn pooled_inode_namespace_and_dirty_records_share_index_pages() {
        let f = Fixture::new(1 << 20);
        let index = Index::new(f.store.clone()).unwrap();
        for serial in 1..=128u64 {
            let inode = HotInode {
                revision: serial,
                generation: 1,
                length: 1,
                kind: NodeKind::File,
                fresh: false,
                storage: 1,
                mode: 0o644,
                seconds: 0,
                nanos: 0,
                links: 1,
                base: [1; 32],
                metadata: [2; 32],
                inline: [Some(Extent::base(0, 1)), None, None, None],
            };
            let binding = NamespaceRecord {
                serial,
                kind: NodeKind::File,
                tombstone: false,
            };
            let name = format!("f{serial:03}");
            let mut updates = vec![
                (
                    inode_key(serial).to_vec(),
                    Some(inode.value().unwrap().to_vec()),
                ),
                (
                    namespace_key(7, name.as_bytes()).unwrap(),
                    Some(binding.value().unwrap().to_vec()),
                ),
                (dirty_key(1, serial).to_vec(), Some(vec![1])),
            ];
            updates.sort_by(|a, b| a.0.cmp(&b.0));
            index.prepare(&updates).unwrap().publish().unwrap();
        }
        for serial in [1, 64, 128] {
            let inode = HotInode::parse(&index.get(&inode_key(serial)).unwrap().unwrap()).unwrap();
            assert_eq!(inode.revision, serial);
            let name = format!("f{serial:03}");
            let binding = NamespaceRecord::parse(
                &index
                    .get(&namespace_key(7, name.as_bytes()).unwrap())
                    .unwrap()
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(binding.serial, serial);
            assert_eq!(index.get(&dirty_key(1, serial)).unwrap(), Some(vec![1]));
        }
        index.maintain().unwrap();
        assert_eq!(f.store.status().unwrap().allocated_bytes, f.physical());
        assert!(f.physical() <= 1 << 20);
        drop(index);
        f.clean();
    }
}
