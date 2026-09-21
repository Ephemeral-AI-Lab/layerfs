//! Existing inode resize and logical zero ranges through the production Workspace API.
#[cfg(target_os = "linux")]
#[path = "support/native_workspace.rs"]
mod support;
#[cfg(target_os = "linux")]
mod linux {
    use super::support::*;
    use layerfs_bridge::contract::*;
    use layerfs_workspace::*;
    use std::{
        io::Write,
        time::{Duration, Instant},
    };
    fn check(id: &str) {
        println!("RESIZE_CHECK {id} PASS");
    }
    fn root(report: &CommitReport) -> Root {
        match &report.outcome {
            CommitOutcomeWire::Committed(c) => c.root,
            CommitOutcomeWire::UpToDate { root, .. } => *root,
        }
    }
    fn resize(f: &Fixture, serial: u64, length: u64) -> MutationReceipt {
        let r = f.workspace.set_len(serial, length, deadline()).unwrap();
        assert_eq!(r.inode, serial);
        assert_eq!(r.accepted_bytes, 0);
        assert_eq!(f.workspace.getattr(serial).unwrap().size, length);
        r
    }
    fn open(f: &Fixture, name: &[u8]) -> (NodeAttributes, HandleId) {
        let a = f.lookup(name);
        let h = f.workspace.open(a.serial, ReferenceScope::Local).unwrap();
        (a, h)
    }
    fn commit(f: &Fixture) -> CommitReport {
        let r = f.workspace.commit(deadline()).unwrap();
        assert!(f.workspace.status().unwrap().submission.is_none());
        assert_eq!(r.stage_token, None);
        r
    }
    fn saved(f: &Fixture, r: &CommitReport, name: &[u8]) -> (u64, Root, u64, i64, u32) {
        attr(f.native.attributes(root(r), name))
    }
    fn observe(f: &Fixture) {
        println!(
            "COMMIT_RESOURCE {:?} {:?} {:?}",
            f.workspace.status().unwrap(),
            f.workspace.backing_status().unwrap(),
            f.workspace.metadata_status().unwrap()
        );
    }
    fn edits_since(f: &Fixture, from: usize) -> Vec<(Root, u64, Vec<Edit>)> {
        f.native.observations.lock().unwrap().operations[from..]
            .iter()
            .filter_map(|op| match op {
                Operation::EditFile {
                    root,
                    base_length,
                    edits,
                } => Some((*root, *base_length, edits.clone())),
                _ => None,
            })
            .collect()
    }
    fn position(f: &Fixture) -> usize {
        f.native.observations.lock().unwrap().operations.len()
    }

    #[test]
    #[ignore = "requires resize_route.py live native service"]
    fn resize_semantics() {
        let f = Fixture::new(Gate::None);
        let (data, handle) = open(&f, b"data.bin");
        assert_eq!(f.lookup(b"alias").serial, data.serial);
        let old = f.workspace.read(handle, 96, 64, deadline()).unwrap();
        let before = position(&f);
        resize(&f, data.serial, 128);
        resize(&f, data.serial, 160);
        assert!(f.native.observations.lock().unwrap().operations[before..]
            .iter()
            .all(|op| !matches!(
                op,
                Operation::EditFile { .. } | Operation::HistoryCommand(_)
            )));
        assert_eq!(
            f.read(handle, 120, 40),
            [vec![120, 121, 122, 123, 124, 125, 126, 127], vec![0; 32]].concat()
        );
        assert_eq!(f.read(handle, 160, 1), Vec::<u8>::new());
        assert_eq!(old.as_ref(), (96u8..160).collect::<Vec<_>>());
        let live = f.workspace.getattr(data.serial).unwrap();
        let first = commit(&f);
        let a = saved(&f, &first, b"data.bin");
        assert_eq!(a.2, 160);
        assert_eq!((a.3, a.4), (live.mtime_seconds, live.mtime_nanoseconds));
        assert_eq!(saved(&f, &first, b"alias"), a);
        assert_eq!(f.native.bytes(a.1, 120, 40), f.read(handle, 120, 40));
        resize(&f, data.serial, 0);
        assert!(f.read(handle, 0, 1).is_empty());
        resize(&f, data.serial, 40);
        assert_eq!(f.read(handle, 0, 40), vec![0; 40]);
        let second = commit(&f);
        let b = saved(&f, &second, b"data.bin");
        assert_eq!(b.2, 40);
        assert_eq!(f.native.bytes(b.1, 0, 40), vec![0; 40]);
        assert_eq!(old.as_ref(), (96u8..160).collect::<Vec<_>>());
        assert_eq!(f.workspace.backing_status().unwrap().payloads, 0);
        drop(old);
        observe(&f);
        f.workspace.release(handle).unwrap();
        f.workspace
            .forget(data.serial, u64::MAX, ReferenceScope::Local);
        f.workspace.close_clean().unwrap();
        check("shrink-reextend-keeps-zero-tail-alias-handles-and-old-reply");
    }

    #[test]
    #[ignore = "requires exact zero-source envelope and real Commit"]
    fn resize_envelope() {
        let f = Fixture::new(Gate::None);
        let (data, handle) = open(&f, b"data.bin");
        resize(&f, data.serial, 0);
        resize(&f, data.serial, 8 * 1024 * 1024);
        let attrs = f.workspace.getattr(data.serial).unwrap();
        let status = f.workspace.status().unwrap();
        assert_eq!(
            f.workspace
                .set_len(data.serial, 8 * 1024 * 1024 + 1, deadline())
                .unwrap_err(),
            WorkspaceError::Capacity
        );
        assert_eq!(f.workspace.getattr(data.serial).unwrap(), attrs);
        let after = f.workspace.status().unwrap();
        assert_eq!(
            (after.generation, after.revision, after.dirty_inodes),
            (status.generation, status.revision, status.dirty_inodes)
        );
        assert_eq!(f.read(handle, 8 * 1024 * 1024 - 64, 64), vec![0; 64]);
        let backing = f.workspace.backing_status().unwrap();
        assert_eq!(backing.payloads, 0);
        assert!(backing.allocated_bytes < 128 * 1024);
        let before = position(&f);
        let report = commit(&f);
        let content = saved(&f, &report, b"data.bin");
        assert_eq!(content.2, 8 * 1024 * 1024);
        let edits = edits_since(&f, before);
        assert_eq!(edits.len(), 1);
        assert_eq!(
            edits[0].2.iter().map(|e| e.replacement).sum::<u64>(),
            8 * 1024 * 1024
        );
        for start in (0..content.2).step_by(MAX_READ_BYTES) {
            assert!(f
                .native
                .bytes(content.1, start, MAX_READ_BYTES)
                .iter()
                .all(|b| *b == 0));
        }
        f.edit(b"data.bin", 0, 0, b"x");
        let attrs = f.workspace.getattr(data.serial).unwrap();
        assert_eq!(
            f.workspace
                .set_len(data.serial, 16 * 1024 * 1024 + 1, deadline())
                .unwrap_err(),
            WorkspaceError::Capacity
        );
        assert_eq!(f.workspace.getattr(data.serial).unwrap(), attrs);
        observe(&f);
        check("zero-input-exact-eight-MiB-bound-without-payload-allocation");
    }

    #[test]
    #[ignore = "requires G lowering hold and exact D1 truncation/zero recipe"]
    fn resize_successor() {
        let f = Fixture::new(Gate::Delivery);
        let (data, handle) = open(&f, b"data.bin");
        resize(&f, data.serial, 64);
        resize(&f, data.serial, 128);
        let frozen = f.workspace.getattr(data.serial).unwrap();
        let ws = f.workspace.clone();
        let staging = std::thread::spawn(move || ws.stage(deadline()));
        f.native.wait_entered();
        resize(&f, data.serial, 48);
        resize(&f, data.serial, 100);
        assert_eq!(f.read(handle, 48, 52), vec![0; 52]);
        f.native.release();
        let stage = staging.join().unwrap().unwrap();
        let g = attr(
            f.native
                .attributes(stage.stage().candidate_root, b"data.bin"),
        );
        assert_eq!(g.2, 128);
        assert_eq!((g.3, g.4), (frozen.mtime_seconds, frozen.mtime_nanoseconds));
        assert_eq!(
            f.native.bytes(g.1, 60, 68),
            [vec![60, 61, 62, 63], vec![0; 64]].concat()
        );
        let first = f.workspace.commit_staged(&stage, deadline()).unwrap();
        assert_eq!(first.stage_token, Some(stage.stage().token));
        assert_eq!(f.workspace.getattr(data.serial).unwrap().size, 100);
        assert_eq!(
            f.read(handle, 44, 56),
            [vec![44, 45, 46, 47], vec![0; 52]].concat()
        );
        let before = position(&f);
        let next = commit(&f);
        let edits = edits_since(&f, before);
        assert_eq!(
            edits,
            vec![(
                g.1,
                128,
                vec![Edit {
                    start: 48,
                    end: 128,
                    replacement: 52
                }]
            )]
        );
        let content = saved(&f, &next, b"data.bin");
        assert_eq!(content.2, 100);
        assert_eq!(
            f.native.bytes(content.1, 44, 56),
            [vec![44, 45, 46, 47], vec![0; 52]].concat()
        );
        observe(&f);
        check("frozen-zero-range-and-live-shrink-reextend-use-exact-next-base");
    }

    #[test]
    #[ignore = "requires overwriting a zero span before its captured root arrives"]
    fn resize_overwritten_zero() {
        let f = Fixture::new(Gate::Delivery);
        let (data, handle) = open(&f, b"data.bin");
        resize(&f, data.serial, 64);
        resize(&f, data.serial, 128);
        let ws = f.workspace.clone();
        let saving = std::thread::spawn(move || ws.commit(deadline()));
        f.native.wait_entered();
        f.edit(b"data.bin", 80, 84, b"LIVE");
        assert_eq!(f.read(handle, 78, 8), [0, 0, b'L', b'I', b'V', b'E', 0, 0]);
        f.native.release();
        let first = saving.join().unwrap().unwrap();
        let g = saved(&f, &first, b"data.bin");
        assert_eq!(f.native.bytes(g.1, 78, 8), vec![0; 8]);
        let before = position(&f);
        let second = commit(&f);
        let edits = edits_since(&f, before);
        assert_eq!(
            edits,
            vec![(
                g.1,
                128,
                vec![Edit {
                    start: 80,
                    end: 84,
                    replacement: 4
                }]
            )]
        );
        let content = saved(&f, &second, b"data.bin");
        assert_eq!(
            f.native.bytes(content.1, 78, 8),
            [0, 0, b'L', b'I', b'V', b'E', 0, 0]
        );
        observe(&f);
        check("live-overwrite-of-frozen-zero-saves-only-new-bytes");
    }

    #[test]
    #[ignore = "requires same-length portable metadata save"]
    fn resize_metadata_only() {
        let f = Fixture::new(Gate::None);
        let data = f.lookup(b"data.bin");
        let Response::History(branch) = f.branch() else {
            panic!("branch missing")
        };
        let HistoryResult::BranchSnapshot(branch) = *branch else {
            panic!("snapshot missing")
        };
        let original = attr(f.native.attributes(branch.effective_root, b"data.bin"));
        let before = f.workspace.status().unwrap();
        let mutation = resize(&f, data.serial, data.size);
        assert!(mutation.revision > before.revision);
        let current = f.workspace.getattr(data.serial).unwrap();
        let report = commit(&f);
        let result = saved(&f, &report, b"data.bin");
        assert_eq!(result.1, original.1);
        assert_eq!(
            (result.3, result.4),
            (current.mtime_seconds, current.mtime_nanoseconds)
        );
        assert!(f.native.observations.lock().unwrap().saved_files.is_empty());
        check("same-length-resize-saves-metadata-without-file-input");
    }

    #[test]
    #[ignore = "requires native quota and exact identity refusals"]
    fn resize_refusals() {
        let f = Fixture::with_quota(Gate::None, 1024 * 1024);
        let (data, handle) = open(&f, b"data.bin");
        let before = f.workspace.status().unwrap();
        for (serial, length, end) in [
            (0, 0, deadline()),
            (u64::MAX, 0, deadline()),
            (f.workspace.root().serial, 0, deadline()),
            (data.serial, MAX_FILE + 1, deadline()),
            (data.serial, 0, Instant::now() - Duration::from_secs(1)),
            (data.serial, 0, deadline()),
        ] {
            assert!(f.workspace.set_len(serial, length, end).is_err());
            assert_eq!(f.workspace.getattr(data.serial).unwrap(), data);
            let status = f.workspace.status().unwrap();
            assert_eq!(
                (status.generation, status.revision, status.dirty_inodes),
                (before.generation, before.revision, before.dirty_inodes)
            );
        }
        let ro = f
            .host
            .attach(
                Fixture::options("readonly", 32, WorkspaceAccess::ReadOnly),
                deadline(),
            )
            .unwrap();
        assert_eq!(
            ro.set_len(data.serial, 0, deadline()).unwrap_err(),
            WorkspaceError::ReadOnly
        );
        ro.close_clean().unwrap();
        assert_eq!(f.read(handle, 0, 4), [0, 1, 2, 3]);
        assert_eq!(f.workspace.backing_status().unwrap().payloads, 0);
        f.workspace.release(handle).unwrap();
        f.workspace
            .forget(data.serial, u64::MAX, ReferenceScope::Local);
        f.workspace.close_clean().unwrap();
        check("resize-kind-identity-deadline-length-and-quota-refusal-atomicity");
    }

    struct RestoreLimit;
    impl Drop for RestoreLimit {
        fn drop(&mut self) {
            file_limit("unlimited");
        }
    }
    #[test]
    #[ignore = "requires actual native private metadata allocation failure"]
    fn resize_metadata_failure() {
        let f = Fixture::new(Gate::None);
        let (data, handle) = open(&f, b"data.bin");
        let before = f.workspace.status().unwrap();
        let restore = RestoreLimit;
        file_limit("2048");
        let result = f.workspace.set_len(data.serial, 128, deadline());
        drop(restore);
        assert!(matches!(result, Err(WorkspaceError::Backing(_))));
        assert_eq!(f.workspace.getattr(data.serial).unwrap(), data);
        let after = f.workspace.status().unwrap();
        assert_eq!(
            (after.generation, after.revision, after.dirty_inodes),
            (before.generation, before.revision, before.dirty_inodes)
        );
        assert_eq!(f.read(handle, 120, 16), (120u8..136).collect::<Vec<_>>());
        println!("COMMIT_FAILURE {result:?}");
        observe(&f);
        check("native-resize-metadata-failure-retains-original-visible-version");
    }

    #[test]
    #[ignore = "requires observed actual C2 save and local resize progress"]
    fn resize_native_save() {
        let f = Fixture::new(Gate::NativeSave);
        let (data, handle) = open(&f, b"data.bin");
        let mut bytes = vec![0; 4 * 1024 * 1024];
        let mut random = 0x6a09e667f3bcc909u64;
        for byte in &mut bytes {
            random ^= random << 13;
            random ^= random >> 7;
            random ^= random << 17;
            *byte = random as u8;
        }
        f.edit(b"data.bin", 0, bytes.len() as u64, &bytes);
        resize(&f, data.serial, bytes.len() as u64);
        resize(&f, data.serial, bytes.len() as u64 + 65536);
        let ws = f.workspace.clone();
        let saving = std::thread::spawn(move || ws.commit(deadline()));
        let mut line = String::new();
        std::io::stdin().read_line(&mut line).unwrap();
        assert_eq!(line.trim(), "SAVE_PAUSED");
        resize(&f, data.serial, bytes.len() as u64);
        resize(&f, data.serial, bytes.len() as u64 + 32);
        assert_eq!(f.read(handle, bytes.len() as u64, 32), vec![0; 32]);
        println!("STAGE_LIVE_READY");
        std::io::stdout().flush().unwrap();
        let first = saving.join().unwrap().unwrap();
        let g = saved(&f, &first, b"data.bin");
        assert_eq!(g.2, bytes.len() as u64 + 65536);
        assert_eq!(
            f.native.bytes(g.1, bytes.len() as u64, 65536),
            vec![0; 65536]
        );
        assert_eq!(
            f.native.bytes(g.1, 0, MAX_READ_BYTES),
            bytes[..MAX_READ_BYTES]
        );
        let next = commit(&f);
        let d = saved(&f, &next, b"data.bin");
        assert_eq!(d.2, bytes.len() as u64 + 32);
        assert_eq!(f.native.bytes(d.1, bytes.len() as u64, 32), vec![0; 32]);
        observe(&f);
        check("resize-progress-during-actual-save-preserves-frozen-and-successor-zero-tails");
    }

    #[test]
    #[ignore = "requires full104-inode fixture and25-second Commit deadline"]
    fn resize_frontier() {
        let f = Fixture::new(Gate::None);
        let mut names = vec![b"data.bin".to_vec(), b"other.bin".to_vec()];
        names.extend((0..102).map(|i| format!("f{i:03}").into_bytes()));
        for name in &names {
            let inode = f.lookup(name);
            resize(&f, inode.serial, 0);
            f.workspace.reclaim_metadata(deadline()).unwrap();
        }
        assert_eq!(f.workspace.status().unwrap().dirty_inodes, 104);
        let report = f
            .workspace
            .commit(Instant::now() + Duration::from_secs(25))
            .unwrap();
        for name in &names {
            let inode = saved(&f, &report, name);
            assert_eq!(inode.2, 0);
        }
        assert_eq!(
            saved(&f, &report, b"data.bin"),
            saved(&f, &report, b"alias")
        );
        let observed = f.native.observations.lock().unwrap();
        assert_eq!(observed.saved_files.len(), 104);
        assert!(observed
            .operations
            .iter()
            .filter_map(|op| match op {
                Operation::EditFile { edits, .. } => Some(edits),
                _ => None,
            })
            .all(|edits| edits.iter().all(|edit| edit.replacement == 0)));
        drop(observed);
        observe(&f);
        check("all-104-inode-shrinks-save-once-with-hardlink-sharing");
    }
}
