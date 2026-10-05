//! Routine healthy-owner reclamation through the ordinary native Workspace APIs.
#[cfg(target_os = "linux")]
#[path = "support/native_workspace.rs"]
mod support;
#[cfg(target_os = "linux")]
mod linux {
    use super::support::*;
    use layerfs_bridge::contract::Source;
    use layerfs_workspace::*;
    use std::{
        fs, io, os::unix::fs::FileExt, path::PathBuf, sync::atomic::AtomicBool, time::Instant,
    };

    fn check(id: &str) {
        println!("MAINTENANCE_CHECK {id} PASS");
    }
    fn observe(f: &Fixture) {
        println!(
            "COMMIT_RESOURCE {:?} {:?}",
            f.workspace.backing_status().unwrap(),
            f.workspace.metadata_status().unwrap()
        );
    }
    fn backing() -> PathBuf {
        PathBuf::from(std::env::var("LAYERFS_STAGE_TEST_ROOT").unwrap())
            .join("private-backing/stage")
    }
    fn files(prefix: &str) -> Vec<PathBuf> {
        fs::read_dir(backing())
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.file_name().unwrap().to_str().unwrap().starts_with(prefix))
            .collect()
    }
    fn damage(path: &std::path::Path) -> u8 {
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .unwrap();
        let mut byte = [0];
        assert_eq!(file.read_at(&mut byte, 0).unwrap(), 1);
        assert_eq!(file.write_at(&[0], 0).unwrap(), 1);
        byte[0]
    }
    fn restore(path: &std::path::Path, byte: u8) {
        // Restore only this external oracle's corruption, without replacing inode identity.
        assert_eq!(
            fs::OpenOptions::new()
                .write(true)
                .open(path)
                .unwrap()
                .write_at(&[byte], 0)
                .unwrap(),
            1
        );
    }
    struct CountedInput {
        calls: usize,
    }
    impl Source for CountedInput {
        fn read(&mut self, out: &mut [u8], _: Instant, _: &AtomicBool) -> io::Result<usize> {
            self.calls += 1;
            if self.calls == 1 {
                out[0] = b'z';
                Ok(1)
            } else {
                Ok(0)
            }
        }
    }

    #[test]
    #[ignore = "requires maintenance_route.py native Linux ext4 fixture"]
    fn maintenance_payload_churn() {
        let f = Fixture::new(Gate::None);
        for i in 0..4097 {
            let input = f.own(&[(i % 251) as u8]);
            let status = f.workspace.backing_status().unwrap();
            assert_eq!(status.payloads, 1);
            assert_eq!(status.allocated_bytes, 8192);
            assert_eq!(status.failed_payloads, 0);
            drop(input);
        }
        observe(&f);
        f.workspace.close_clean().unwrap();
        check("4097-owned-inputs-reuse-bounded-records-without-explicit-reclaim");
    }

    #[test]
    #[ignore = "requires maintenance_route.py native Linux ext4 fixture"]
    fn maintenance_mutation_churn() {
        let f = Fixture::new(Gate::None);
        let data = f.lookup(b"data.bin");
        let handle = f
            .workspace
            .open(data.serial, ReferenceScope::Local)
            .unwrap();
        for i in 0..96u8 {
            f.edit(b"data.bin", 0, 1, &[i]);
            assert_eq!(f.read(handle, 0, 4), [i, 1, 2, 3]);
            assert!(f.workspace.metadata_status().unwrap().roots <= 3);
            assert!(f.workspace.backing_status().unwrap().payloads <= 2);
        }
        f.workspace.commit(deadline()).unwrap();
        for _ in 0..40 {
            f.workspace.commit(deadline()).unwrap();
            assert!(f.workspace.metadata_status().unwrap().roots <= 8);
        }
        assert_eq!(f.read(handle, 0, 4), [95, 1, 2, 3]);
        observe(&f);
        f.workspace.release(handle).unwrap();
        f.workspace
            .forget(data.serial, u64::MAX, ReferenceScope::Local);
        f.workspace.close_clean().unwrap();
        check("96-mutations-and-40-clean-Commits-stay-bounded-without-explicit-reclaim");
    }

    #[test]
    #[ignore = "requires maintenance_route.py native Linux ext4 fixture"]
    fn maintenance_cross_workspace() {
        let f = Fixture::new(Gate::None);
        let second = f
            .host
            .attach(
                Fixture::options("second", 32, WorkspaceAccess::LocalEdit),
                deadline(),
            )
            .unwrap();
        let token = f.own(b"kept");
        let mut reader = token.reader(0..4).unwrap();
        drop(token);
        drop(f.own(b"dead"));
        let next = second.own_payload(1, &mut &b"x"[..], deadline()).unwrap();
        assert_eq!(f.workspace.backing_status().unwrap().payloads, 2);
        let mut bytes = [0; 4];
        assert_eq!(
            reader
                .read(&mut bytes, deadline(), &AtomicBool::new(false))
                .unwrap(),
            4
        );
        assert_eq!(&bytes, b"kept");
        drop(reader);
        drop(next);
        drop(second.own_payload(1, &mut &b"y"[..], deadline()).unwrap());
        assert_eq!(f.workspace.backing_status().unwrap().payloads, 1);
        f.edit(b"data.bin", 0, 1, b"a");
        f.edit(b"data.bin", 0, 1, b"b");
        let before = f.workspace.metadata_status().unwrap();
        assert!(before.roots > before.external_roots);
        let held = second.own_payload(1, &mut &b"z"[..], deadline()).unwrap();
        let after = f.workspace.metadata_status().unwrap();
        assert!(after.roots < before.roots);
        assert_eq!(after.roots, after.external_roots);
        drop(held);
        f.workspace.commit(deadline()).unwrap();
        observe(&f);
        second.close_clean().unwrap();
        f.workspace.close_clean().unwrap();
        check("consumer-wide-maintenance-releases-dead-peers-and-preserves-reader-pins");
    }

    #[test]
    #[ignore = "requires maintenance_route.py native Linux ext4 fixture"]
    fn maintenance_frozen() {
        let f = Fixture::new(Gate::Delivery);
        let data = f.lookup(b"data.bin");
        let handle = f
            .workspace
            .open(data.serial, ReferenceScope::Local)
            .unwrap();
        f.edit(b"data.bin", 0, 1, b"G");
        let reader = f.workspace.read(handle, 0, 1, deadline()).unwrap();
        std::thread::scope(|scope| {
            let save = scope.spawn(|| f.workspace.commit(deadline()));
            f.native.wait_entered();
            for _ in 0..64 {
                f.edit(b"data.bin", 0, 1, b"D");
                assert_eq!(f.read(handle, 0, 1), b"D");
                assert_eq!(reader.as_ref(), b"G");
                assert!(f.workspace.metadata_status().unwrap().roots <= 8);
            }
            f.native.release();
            save.join().unwrap().unwrap();
        });
        assert_eq!(reader.as_ref(), b"G");
        assert_eq!(f.read(handle, 0, 4), [b'D', 1, 2, 3]);
        drop(reader);
        f.workspace.commit(deadline()).unwrap();
        assert_eq!(f.read(handle, 0, 4), [b'D', 1, 2, 3]);
        observe(&f);
        f.workspace.release(handle).unwrap();
        f.workspace
            .forget(data.serial, u64::MAX, ReferenceScope::Local);
        f.workspace.close_clean().unwrap();
        check("64-successor-mutations-preserve-frozen-G-and-reader-through-Commit");
    }

    #[test]
    #[ignore = "requires maintenance_route.py native Linux ext4 fixture"]
    fn maintenance_payload_failure() {
        let f = Fixture::new(Gate::None);
        drop(f.own(b"dead"));
        let path = files("p-").pop().unwrap();
        let original = damage(&path);
        let mut source = CountedInput { calls: 0 };
        assert!(f.workspace.own_payload(1, &mut source, deadline()).is_err());
        assert_eq!(source.calls, 0);
        assert_eq!(f.workspace.backing_status().unwrap().failed_payloads, 1);
        restore(&path, original);
        let next = f.own(b"next");
        assert!(path.exists(), "ordinary input retried prior failed cleanup");
        assert_eq!(f.workspace.backing_status().unwrap().failed_payloads, 1);
        assert_eq!(
            f.workspace
                .reclaim_payloads(deadline())
                .unwrap()
                .payloads_released,
            1
        );
        assert!(!path.exists());
        drop(next);
        assert!(f
            .workspace
            .own_payload(2, &mut &b"x"[..], deadline())
            .is_err());
        let before = f.workspace.backing_status().unwrap();
        assert_eq!(before.failed_payloads, 1);
        let good = f.own(b"good");
        let after = f.workspace.backing_status().unwrap();
        assert_eq!(after.failed_payloads, 1);
        assert!(after.allocated_bytes >= before.allocated_bytes + 8192);
        drop(good);
        observe(&f);
        f.workspace.close_clean().unwrap();
        check("failed-and-partial-payload-owners-stay-accounted-until-explicit-cleanup");
    }

    #[test]
    #[ignore = "requires maintenance_route.py native Linux ext4 fixture"]
    fn maintenance_metadata_failure() {
        let f = Fixture::new(Gate::None);
        f.edit(b"data.bin", 0, 1, b"a");
        f.edit(b"data.bin", 0, 1, b"b");
        let before = f.workspace.metadata_status().unwrap();
        assert!(before.roots > before.external_roots);
        let path = files("m-ledger-").pop().unwrap();
        let original = damage(&path);
        let mut source = CountedInput { calls: 0 };
        assert!(f.workspace.own_payload(1, &mut source, deadline()).is_err());
        assert_eq!(source.calls, 0);
        restore(&path, original);
        let next = f.own(b"next");
        let after = f.workspace.metadata_status().unwrap();
        assert_eq!(
            after.roots, before.roots,
            "ordinary admission retried failed root"
        );
        assert_eq!(after.allocated_pages, before.allocated_pages);
        // A later ordinary Commit must also leave the failed root retained.
        f.workspace.commit(deadline()).unwrap();
        let retained = f.workspace.metadata_status().unwrap();
        let report = f.workspace.reclaim_metadata(deadline()).unwrap();
        assert!(report.roots_released > 0);
        assert!(f.workspace.metadata_status().unwrap().roots < retained.roots);
        drop(next);
        observe(&f);
        f.workspace.close_clean().unwrap();
        check("cleanup-failure-before-DFS-is-retained-across-input-and-Commit");
    }
}
