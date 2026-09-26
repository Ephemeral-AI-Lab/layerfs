//! Exact C5 commit and live successor reconciliation through production APIs.
#[cfg(target_os = "linux")]
#[path = "support/native_workspace.rs"]
mod support;
#[cfg(target_os = "linux")]
mod linux {
    use super::support::*;
    use layerfs_bridge::contract::*;
    use layerfs_workspace::*;
    use std::{
        fs::File,
        os::unix::fs::FileExt,
        process::Command,
        time::{Duration, Instant},
    };

    fn page_read_listener() -> std::os::fd::OwnedFd {
        use nix::libc;
        use std::os::fd::FromRawFd;
        let arch = match std::env::consts::ARCH {
            "aarch64" => 0xc00000b7,
            "x86_64" => 0xc000003e,
            other => panic!("unsupported syscall architecture {other}"),
        };
        let stmt = |code, k| libc::sock_filter {
            code,
            jt: 0,
            jf: 0,
            k,
        };
        let code = [
            stmt(0x20, 4),
            libc::sock_filter {
                code: 0x15,
                jt: 0,
                jf: 3,
                k: arch,
            },
            stmt(0x20, 0),
            libc::sock_filter {
                code: 0x15,
                jt: 0,
                jf: 1,
                k: libc::SYS_pread64 as u32,
            },
            stmt(0x06, libc::SECCOMP_RET_USER_NOTIF),
            stmt(0x06, libc::SECCOMP_RET_ALLOW),
        ];
        let program = libc::sock_fprog {
            len: code.len() as u16,
            filter: code.as_ptr() as *mut _,
        };
        unsafe {
            assert_eq!(libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0), 0);
            let fd = libc::syscall(
                libc::SYS_seccomp,
                libc::SECCOMP_SET_MODE_FILTER,
                libc::SECCOMP_FILTER_FLAG_NEW_LISTENER,
                &program,
            );
            assert!(fd >= 0, "{}", std::io::Error::last_os_error());
            std::os::fd::OwnedFd::from_raw_fd(fd as i32)
        }
    }

    fn next_page_read(
        listener: &std::os::fd::OwnedFd,
        wait: Duration,
    ) -> Option<nix::libc::seccomp_notif> {
        use nix::libc;
        use std::os::fd::AsRawFd;
        let mut poll = libc::pollfd {
            fd: listener.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let ready = unsafe { libc::poll(&mut poll, 1, wait.as_millis() as i32) };
        assert!(ready >= 0, "{}", std::io::Error::last_os_error());
        if ready == 0 {
            return None;
        }
        if poll.revents & libc::POLLIN == 0 && poll.revents & libc::POLLHUP != 0 {
            return None;
        }
        assert_ne!(poll.revents & libc::POLLIN, 0);
        let mut event = unsafe { std::mem::zeroed() };
        assert_eq!(
            unsafe {
                libc::ioctl(
                    listener.as_raw_fd(),
                    libc::SECCOMP_IOCTL_NOTIF_RECV,
                    &mut event,
                )
            },
            0
        );
        Some(event)
    }

    fn release_page_read(listener: &std::os::fd::OwnedFd, id: u64) {
        use nix::libc;
        use std::os::fd::AsRawFd;
        let mut response = libc::seccomp_notif_resp {
            id,
            val: 0,
            error: 0,
            flags: libc::SECCOMP_USER_NOTIF_FLAG_CONTINUE as u32,
        };
        assert_eq!(
            unsafe {
                libc::ioctl(
                    listener.as_raw_fd(),
                    libc::SECCOMP_IOCTL_NOTIF_SEND,
                    &mut response,
                )
            },
            0
        );
    }

    fn check(id: &str) {
        println!("COMMIT_CHECK {id} PASS");
    }
    fn snapshot(f: &Fixture) -> BranchSnapshotWire {
        let Response::History(result) = f.branch() else {
            panic!("missing history result")
        };
        let HistoryResult::BranchSnapshot(snapshot) = *result else {
            panic!("missing Branch snapshot")
        };
        snapshot
    }
    fn committed(f: &Fixture, selector: &StageSelector, report: &CommitReport) -> CommitWire {
        let CommitOutcomeWire::Committed(commit) = &report.outcome else {
            panic!("expected a new Commit")
        };
        assert_eq!(report.generation, selector.stage().generation);
        assert_eq!(report.stage_token, Some(selector.stage().token));
        assert_eq!(commit.root, selector.stage().candidate_root);
        assert_eq!(commit.stack, selector.stage().stack);
        assert_eq!(commit.parent, selector.stage().expected_head);
        assert_eq!(commit.base_layer, selector.stage().intended_commit_base);
        let branch = snapshot(f);
        assert_eq!(branch.branch.head_commit, Some(commit.commit));
        assert_eq!(branch.effective_root, commit.root);
        let status = f.workspace.status().unwrap();
        assert!(status.submission.is_none());
        assert_eq!(status.revision, report.revision);
        let failure = f
            .native
            .request(
                Operation::HistoryQuery(HistoryQuery::GetStage {
                    workspace: [31; 32],
                }),
                16384,
                &mut std::io::sink(),
            )
            .unwrap_err();
        assert_eq!(failure.code, Code::NotFound);
        assert!(!failure.unknown);
        commit.clone()
    }
    fn count_commits(f: &Fixture) -> usize {
        f.native
            .observations
            .lock()
            .unwrap()
            .operations
            .iter()
            .filter(|op| {
                matches!(
                    op,
                    Operation::HistoryCommand(HistoryCommand::CommitStaged { .. })
                )
            })
            .count()
    }
    fn commit(f: &Fixture) -> (StageSelector, CommitReport) {
        let stage = f.workspace.stage(deadline()).unwrap();
        let result = f.workspace.commit_staged(&stage, deadline()).unwrap();
        committed(f, &stage, &result);
        (stage, result)
    }
    fn observe(f: &Fixture) {
        println!(
            "COMMIT_RESOURCE {:?} {:?} {:?}",
            f.workspace.status().unwrap(),
            f.workspace.backing_status().unwrap(),
            f.workspace.metadata_status().unwrap()
        );
    }
    fn close(f: &Fixture, handles: &[HandleId], serials: &[u64]) {
        for handle in handles {
            f.workspace.release(*handle).unwrap();
        }
        for serial in serials {
            f.workspace.forget(*serial, u64::MAX, ReferenceScope::Local);
        }
        f.workspace.close_clean().unwrap();
    }
    fn retained_failure(f: &Fixture, stage: &StageSelector) {
        let calls = count_commits(f);
        assert!(matches!(
            f.workspace.commit_staged(stage, deadline()),
            Err(WorkspaceError::Busy)
        ));
        assert!(matches!(
            f.workspace.stage(deadline()),
            Err(WorkspaceError::Busy)
        ));
        assert_eq!(count_commits(f), calls);
        assert_eq!(f.workspace.close_clean(), Err(WorkspaceError::Busy));
    }

    #[test]
    #[ignore = "requires commit_staged_route.py live native service"]
    fn commit_repeated() {
        let f = Fixture::new(Gate::None);
        let data = f.lookup(b"data.bin");
        let alias = f.lookup(b"alias");
        let other = f.lookup(b"other.bin");
        assert_eq!(data.serial, alias.serial);
        let handle = f
            .workspace
            .open(data.serial, ReferenceScope::Local)
            .unwrap();
        f.edit(b"data.bin", 10, 14, b"AAAA");
        let old_reply = f.workspace.read(handle, 10, 4, deadline()).unwrap();
        let (first, first_report) = commit(&f);
        let first_root = attr(
            f.native
                .attributes(first.stage().candidate_root, b"data.bin"),
        )
        .1;
        assert_eq!(f.read(handle, 10, 4), b"AAAA");
        assert_eq!(f.workspace.status().unwrap().dirty_inodes, 0);
        let prior_calls = count_commits(&f);
        assert!(f.workspace.commit_staged(&first, deadline()).is_err());
        assert_eq!(count_commits(&f), prior_calls);
        let file_count = f.native.observations.lock().unwrap().saved_files.len();
        f.edit(b"other.bin", 0, 2, b"BB");
        let (second, second_report) = commit(&f);
        assert_eq!(
            f.native.observations.lock().unwrap().saved_files.len(),
            file_count + 1
        );
        assert_eq!(
            attr(
                f.native
                    .attributes(second.stage().candidate_root, b"data.bin")
            )
            .1,
            first_root
        );
        f.edit(b"alias", 11, 13, b"x");
        assert_eq!(f.read(handle, 10, 3), b"AxA");
        let before = f.native.observations.lock().unwrap().operations.len();
        let (third, third_report) = commit(&f);
        let observed = f.native.observations.lock().unwrap();
        let changes: Vec<_> = observed.operations[before..]
            .iter()
            .filter_map(|op| {
                if let Operation::EditFile {
                    root, replacement, ..
                } = op
                {
                    Some((*root, *replacement))
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].0, first_root);
        assert_eq!(changes[0].1, 1);
        drop(observed);
        assert_eq!(f.read(handle, 10, 3), b"AxA");
        assert_eq!(old_reply.as_ref(), b"AAAA");
        assert_eq!(first_report.generation + 1, second_report.generation);
        assert_eq!(second_report.generation + 1, third_report.generation);
        assert_eq!(snapshot(&f).effective_root, third.stage().candidate_root);
        assert_eq!(count_commits(&f), 3);
        check("same-Workspace-A-B-A-commits-preserve-handles-and-incremental-input");
        drop(old_reply);
        observe(&f);
        close(&f, &[handle], &[data.serial, other.serial]);
        check("known-commit-clean-close");
    }

    #[test]
    #[ignore = "requires actual C5 commit reply gate; not an S-11 save proof"]
    fn commit_successor() {
        let f = Fixture::new(Gate::CommitReply);
        let data = f.lookup(b"data.bin");
        let handle = f
            .workspace
            .open(data.serial, ReferenceScope::Local)
            .unwrap();
        f.edit(b"data.bin", 10, 14, b"GGGG");
        let stage = f.workspace.stage(deadline()).unwrap();
        let saved_root = attr(
            f.native
                .attributes(stage.stage().candidate_root, b"data.bin"),
        )
        .1;
        f.edit(b"data.bin", 11, 13, b"LIVEIN");
        let old = f.workspace.read(handle, 10, 8, deadline()).unwrap();
        assert_eq!(old.as_ref(), b"GLIVEING");
        let ws = f.workspace.clone();
        let selected = stage.clone();
        let pending = std::thread::spawn(move || ws.commit_staged(&selected, deadline()));
        f.native.wait_commit();
        assert_eq!(f.workspace.metadata_status().unwrap().reserved_slots, 26);
        assert!(matches!(
            f.workspace.commit_staged(&stage, deadline()),
            Err(WorkspaceError::Busy)
        ));
        f.edit(b"data.bin", 10, 18, b"Z");
        assert!(matches!(
            f.workspace.read(handle, 8, 6, deadline()),
            Err(WorkspaceError::Busy)
        ));
        assert_eq!(f.read(handle, 10, 1), b"Z");
        f.native.release_commit();
        let report = pending.join().unwrap().unwrap();
        committed(&f, &stage, &report);
        assert_eq!(f.read(handle, 8, 6), [8, 9, b'Z', 14, 15, 16]);
        assert_eq!(old.as_ref(), b"GLIVEING");
        let before = f.native.observations.lock().unwrap().operations.len();
        let (next, _) = commit(&f);
        let observed = f.native.observations.lock().unwrap();
        let changes: Vec<_> = observed.operations[before..]
            .iter()
            .filter_map(|op| {
                if let Operation::EditFile {
                    root,
                    base_length,
                    edits,
                    replacement,
                } = op
                {
                    Some((*root, *base_length, *edits, *replacement))
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].0, saved_root);
        assert_eq!(changes[0].1, data.size);
        assert_eq!((changes[0].2, changes[0].3), (1, 1));
        drop(observed);
        let saved = attr(
            f.native
                .attributes(next.stage().candidate_root, b"data.bin"),
        );
        assert_eq!(saved.2, data.size - 3);
        assert_eq!(f.native.bytes(saved.1, 8, 6), [8, 9, b'Z', 14, 15, 16]);
        assert_eq!(f.workspace.metadata_status().unwrap().reserved_slots, 0);
        drop(old);
        observe(&f);
        check("late-D1-reconciliation-keeps-exact-G-coordinates-and-old-reply");
    }

    #[test]
    #[ignore = "requires privileged mounted FUSE and commit_staged_route.py"]
    fn commit_mounted_successor() {
        let f = Fixture::new(Gate::CommitReply);
        let before = snapshot(&f);
        let data = attr(f.native.attributes(before.effective_root, b"data.bin"));
        let old_tail = f.native.bytes(data.1, data.2 - 1, 1);
        let mut mount = layerfs_fuse::mount_writable(&f.workspace, deadline()).unwrap();
        let path = f.workspace.mount_path().to_path_buf();
        let append = |script: &str| {
            let output = Command::new("/bin/sh")
                .arg("-c")
                .arg(script)
                .current_dir(&path)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        };
        append("set -eu; printf A >> data.bin");
        let stage_a = f.workspace.stage(deadline()).unwrap();
        let ws = f.workspace.clone();
        let selected = stage_a.clone();
        let committing = std::thread::spawn(move || ws.commit_staged(&selected, deadline()));
        f.native.wait_commit();
        append("set -eu; printf B >> data.bin");
        let file = File::open(path.join("data.bin")).unwrap();
        let mut bytes = [0; 2];
        assert_eq!(file.read_at(&mut bytes, data.2).unwrap(), 2);
        assert_eq!(&bytes, b"AB");
        drop(file);
        f.native.release_commit();
        let first = committing.join().unwrap().unwrap();
        let first_root = committed(&f, &stage_a, &first).root;
        let first_file = attr(f.native.attributes(first_root, b"data.bin"));
        assert_eq!(first_file.2, data.2 + 1);
        assert_eq!(f.native.bytes(first_file.1, data.2, 1), b"A");

        let stage_b = f.workspace.stage(deadline()).unwrap();
        {
            let mut observed = f.native.observations.lock().unwrap();
            observed.commit_entered = false;
            observed.commit_released = false;
        }
        let ws = f.workspace.clone();
        let selected = stage_b.clone();
        let committing = std::thread::spawn(move || ws.commit_staged(&selected, deadline()));
        f.native.wait_commit();
        let writing = std::thread::spawn({
            let path = path.clone();
            move || {
                Command::new("/bin/sh")
                    .arg("-c")
                    .arg("set -eu; for c in 0 1 2 3 4 5 6 7; do printf %s \"$c\" >> data.bin; done")
                    .current_dir(path)
                    .output()
                    .unwrap()
            }
        });
        f.native.release_commit();
        let second = committing.join().unwrap().unwrap();
        let output = writing.join().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let CommitOutcomeWire::Committed(second_commit) = &second.outcome else {
            panic!("second Commit must publish B2")
        };
        assert_eq!(second.generation, stage_b.stage().generation);
        assert_eq!(second.stage_token, Some(stage_b.stage().token));
        assert_eq!(second_commit.root, stage_b.stage().candidate_root);
        assert_eq!(snapshot(&f).branch.head_commit, Some(second_commit.commit));
        assert!(f.workspace.status().unwrap().revision >= second.revision);
        assert!(f.workspace.status().unwrap().submission.is_none());
        let second_root = second_commit.root;
        let second_file = attr(f.native.attributes(second_root, b"data.bin"));
        assert_eq!(second_file.2, data.2 + 2);
        assert_eq!(f.native.bytes(second_file.1, data.2, 2), b"AB");
        let file = File::open(path.join("data.bin")).unwrap();
        let mut live = [0; 10];
        assert_eq!(file.read_at(&mut live, data.2).unwrap(), 10);
        assert_eq!(&live, b"AB01234567");
        drop(file);
        let old_head = attr(f.native.attributes(before.effective_root, b"data.bin"));
        assert_eq!(old_head.2, data.2);
        assert_eq!(f.native.bytes(old_head.1, data.2 - 1, 1), old_tail);
        assert_eq!(f.native.bytes(first_file.1, data.2, 1), b"A");
        mount.unmount(deadline()).unwrap();
        check("mounted-three-generation-convergence-keeps-old-heads-and-ordered-writes");
    }

    #[test]
    #[ignore = "requires mounted FUSE and a Linux syscall barrier on the successor builder"]
    fn commit_mounted_build_overlap() {
        use std::sync::mpsc;
        std::env::set_var("LAYERFS_FUSE_ERROR_DIAGNOSTIC", "1");
        let f = Fixture::new(Gate::CommitReply);
        let before = snapshot(&f);
        let data = attr(f.native.attributes(before.effective_root, b"data.bin"));
        let old_tail = f.native.bytes(data.1, data.2 - 1, 1);
        let mut mount = layerfs_fuse::mount_writable(&f.workspace, deadline()).unwrap();
        let path = f.workspace.mount_path().to_path_buf();
        let append = |letter: &str| {
            Command::new("/bin/sh")
                .arg("-c")
                .arg(format!("set -eu; printf {letter} >> data.bin"))
                .current_dir(&path)
                .output()
                .unwrap()
        };
        assert!(append("A").status.success());
        let stage_a = f.workspace.stage(deadline()).unwrap();
        let (send, receive) = mpsc::sync_channel(1);
        let ws = f.workspace.clone();
        let selected = stage_a.clone();
        let committing = std::thread::spawn(move || {
            let listener = page_read_listener();
            let task = unsafe { nix::libc::syscall(nix::libc::SYS_gettid) as u32 };
            send.send((listener, task)).unwrap();
            ws.commit_staged(&selected, deadline())
        });
        let (listener, task) = receive.recv_timeout(Duration::from_secs(5)).unwrap();
        let c5_until = Instant::now() + Duration::from_secs(8);
        while !f.native.observations.lock().unwrap().commit_entered {
            if let Some(event) = next_page_read(&listener, Duration::from_millis(20)) {
                assert_eq!(event.pid, task);
                release_page_read(&listener, event.id);
            }
            assert!(Instant::now() < c5_until, "C5 reply gate was not reached");
        }
        assert!(append("X").status.success());
        let pre_b = f.workspace.status().unwrap();
        f.native.release_commit();
        let io_until = Instant::now() + Duration::from_secs(8);
        let private = std::path::Path::new(&std::env::var("LAYERFS_STAGE_TEST_ROOT").unwrap())
            .join("private-backing/stage");
        let held = loop {
            let event = next_page_read(&listener, Duration::from_millis(20));
            if let Some(event) = event {
                assert_eq!(event.pid, task);
                let fd = event.data.args[0] as i32;
                let target = std::fs::read_link(format!("/proc/self/task/{task}/fd/{fd}")).unwrap();
                let phase = f
                    .workspace
                    .status()
                    .unwrap()
                    .submission
                    .unwrap()
                    .commit
                    .unwrap()
                    .phase;
                if target.starts_with(&private)
                    && target
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .starts_with("m-page-")
                    && phase == CommitPhase::Reconcile
                {
                    println!(
                        "COMMIT_BUILDER_IO tid={task} fd={fd} path={} syscall=pread64",
                        target.display()
                    );
                    break event;
                }
                release_page_read(&listener, event.id);
            }
            assert!(
                Instant::now() < io_until,
                "successor page read was not observed"
            );
        };
        assert_eq!(held.data.nr as i64, nix::libc::SYS_pread64);
        let output = append("B");
        let during = f.workspace.status().unwrap();
        println!(
            "COMMIT_BUILDER_B before={} after={} accepted={} stderr={}",
            pre_b.revision,
            during.revision,
            output.status.success(),
            String::from_utf8_lossy(&output.stderr)
        );
        release_page_read(&listener, held.id);
        while !committing.is_finished() {
            if let Some(event) = next_page_read(&listener, Duration::from_millis(20)) {
                assert_eq!(event.pid, task);
                release_page_read(&listener, event.id);
            }
        }
        assert!(output.status.success());
        assert_eq!(during.revision, pre_b.revision + 1);
        assert_eq!(
            during.submission.unwrap().commit.unwrap().phase,
            CommitPhase::Reconcile
        );
        let first = committing.join().unwrap().unwrap();
        assert_eq!(first.revision, during.revision + 1);
        let first_root = committed(&f, &stage_a, &first).root;
        let first_file = attr(f.native.attributes(first_root, b"data.bin"));
        assert_eq!(first_file.2, data.2 + 1);
        assert_eq!(f.native.bytes(first_file.1, data.2, 1), b"A");
        let live = File::open(path.join("data.bin")).unwrap();
        let mut bytes = [0; 3];
        assert_eq!(live.read_at(&mut bytes, data.2).unwrap(), 3);
        assert_eq!(&bytes, b"AXB");
        drop(live);
        let stage_b = f.workspace.stage(deadline()).unwrap();
        let second = f.workspace.commit_staged(&stage_b, deadline()).unwrap();
        let second_root = committed(&f, &stage_b, &second).root;
        let second_file = attr(f.native.attributes(second_root, b"data.bin"));
        assert_eq!(second_file.2, data.2 + 3);
        assert_eq!(f.native.bytes(second_file.1, data.2, 3), b"AXB");
        let live = File::open(path.join("data.bin")).unwrap();
        assert_eq!(live.read_at(&mut bytes, data.2).unwrap(), 3);
        assert_eq!(&bytes, b"AXB");
        drop(live);
        assert_eq!(
            attr(f.native.attributes(first_root, b"data.bin")).2,
            data.2 + 1
        );
        let old_head = attr(f.native.attributes(before.effective_root, b"data.bin"));
        assert_eq!(old_head.2, data.2);
        assert_eq!(f.native.bytes(old_head.1, data.2 - 1, 1), old_tail);
        mount.unmount(deadline()).unwrap();
        check("mounted-B-accepted-during-real-successor-page-read-and-old-heads-stay-exact");
    }

    #[test]
    #[ignore = "requires commit_staged_route.py live native service"]
    fn commit_selectors() {
        let f = Fixture::new(Gate::None);
        f.lookup(b"data.bin");
        f.edit(b"data.bin", 10, 14, b"GGGG");
        let stage = f.workspace.stage(deadline()).unwrap();
        let sibling = f
            .host
            .attach(
                Fixture::options("sibling", 33, WorkspaceAccess::LocalEdit),
                deadline(),
            )
            .unwrap();
        assert!(sibling.commit_staged(&stage, deadline()).is_err());
        assert_eq!(count_commits(&f), 0);
        assert!(f
            .workspace
            .commit_staged(&stage, Instant::now() - Duration::from_secs(1))
            .is_err());
        assert_eq!(count_commits(&f), 0);
        let clone = stage.clone();
        drop(stage);
        let report = f.workspace.commit_staged(&clone, deadline()).unwrap();
        committed(&f, &clone, &report);
        assert_eq!(count_commits(&f), 1);
        assert!(f.workspace.commit_staged(&clone, deadline()).is_err());
        assert_eq!(count_commits(&f), 1);
        sibling.close_clean().unwrap();
        check("foreign-expired-dropped-and-consumed-selectors-never-replay");
    }

    #[test]
    #[ignore = "requires second authenticated peer with no history-command grant"]
    fn commit_denied() {
        let f = Fixture::new(Gate::CommitDenied);
        f.lookup(b"data.bin");
        f.edit(b"data.bin", 10, 14, b"GGGG");
        let stage = f.workspace.stage(deadline()).unwrap();
        let before = f.branch();
        let WorkspaceError::Commit(failure) =
            f.workspace.commit_staged(&stage, deadline()).unwrap_err()
        else {
            panic!("missing exact Commit failure")
        };
        assert_eq!(failure.phase, CommitPhase::CommitStaged);
        assert_eq!(
            failure.disposition,
            CommitFailureDisposition::KnownBeforeCommit
        );
        assert!(
            matches!(&failure.cause,WorkspaceError::Service(cause) if cause.code==Code::Denied&&!cause.unknown)
        );
        assert!(failure.known_outcome.is_none());
        assert_eq!(f.branch(), before);
        assert_eq!(
            f.native.query(HistoryQuery::GetStage {
                workspace: [31; 32]
            }),
            Response::History(Box::new(HistoryResult::Stage(stage.stage().clone())))
        );
        f.edit(b"data.bin", 10, 14, b"LIVE");
        retained_failure(&f, &stage);
        println!("COMMIT_FAILURE {failure:?}");
        check("actual-commit-denial-retains-stage-and-local-state");
    }

    #[test]
    #[ignore = "requires opaque native result-loss proxy on CommitStaged only"]
    fn commit_lost_result() {
        let f = Fixture::new(Gate::CommitUnknown);
        let data = f.lookup(b"data.bin");
        let handle = f
            .workspace
            .open(data.serial, ReferenceScope::Local)
            .unwrap();
        f.edit(b"data.bin", 10, 14, b"GGGG");
        let stage = f.workspace.stage(deadline()).unwrap();
        f.edit(b"data.bin", 10, 14, b"LIVE");
        let WorkspaceError::Commit(failure) =
            f.workspace.commit_staged(&stage, deadline()).unwrap_err()
        else {
            panic!("missing unknown Commit failure")
        };
        assert_eq!(failure.disposition, CommitFailureDisposition::Unknown);
        assert!(matches!(&failure.cause,WorkspaceError::Service(cause) if cause.unknown));
        assert!(failure.known_outcome.is_none());
        assert!(failure.observed_outcome.is_none());
        let actual = snapshot(&f);
        assert_eq!(actual.effective_root, stage.stage().candidate_root);
        assert!(actual.branch.head_commit.is_some());
        let absent = f
            .native
            .request(
                Operation::HistoryQuery(HistoryQuery::GetStage {
                    workspace: [31; 32],
                }),
                16384,
                &mut std::io::sink(),
            )
            .unwrap_err();
        assert_eq!(absent.code, Code::NotFound);
        assert_eq!(f.read(handle, 10, 4), b"LIVE");
        retained_failure(&f, &stage);
        println!("COMMIT_FAILURE {failure:?}");
        println!(
            "COMMIT_LATER_OBSERVATION root={:?} head={:?} original_outcome=Unknown",
            actual.effective_root, actual.branch.head_commit
        );
        check("native-lost-commit-result-stays-unknown-despite-later-observation");
    }

    struct RestoreLimit;
    impl Drop for RestoreLimit {
        fn drop(&mut self) {
            file_limit("unlimited");
        }
    }
    #[test]
    #[ignore = "requires native private-file failure after actual C5 acknowledgement"]
    fn commit_reconcile_failure() {
        let f = Fixture::new(Gate::CommitCompletionFailure);
        let data = f.lookup(b"data.bin");
        f.edit(b"data.bin", 10, 14, b"GGGG");
        let handle = f
            .workspace
            .open(data.serial, ReferenceScope::Local)
            .unwrap();
        let old_reply = f.workspace.read(handle, 10, 4, deadline()).unwrap();
        let stage = f.workspace.stage(deadline()).unwrap();
        f.edit(b"data.bin", 10, 14, b"LIVE");
        let restore = RestoreLimit;
        let result = f.workspace.commit_staged(&stage, deadline());
        drop(restore);
        let WorkspaceError::Commit(failure) = result.unwrap_err() else {
            panic!("missing retained known success")
        };
        assert_eq!(
            failure.disposition,
            CommitFailureDisposition::KnownCommitLocalFailure
        );
        assert_eq!(failure.phase, CommitPhase::Reconcile);
        assert!(matches!(failure.cause, WorkspaceError::Backing(_)));
        assert_eq!(
            failure.known_outcome,
            Some(f.native.observations.lock().unwrap().commits[0].clone())
        );
        assert!(failure.installed_revision.is_none());
        let actual = snapshot(&f);
        assert_eq!(actual.effective_root, stage.stage().candidate_root);
        assert_eq!(f.workspace.getattr(data.serial).unwrap().size, data.size);
        println!("COMMIT_FAILURE {failure:?}");
        let calls = count_commits(&f);
        let report = f.workspace.commit_staged(&stage, deadline()).unwrap();
        committed(&f, &stage, &report);
        assert_eq!(count_commits(&f), calls);
        assert_eq!(
            f.native.bytes(
                attr(
                    f.native
                        .attributes(stage.stage().candidate_root, b"data.bin")
                )
                .1,
                10,
                4
            ),
            b"GGGG"
        );
        assert_eq!(f.read(handle, 10, 4), b"LIVE");
        assert_eq!(old_reply.as_ref(), b"GGGG");
        drop(old_reply);
        let (next, _) = commit(&f);
        assert_eq!(
            f.native.bytes(
                attr(
                    f.native
                        .attributes(next.stage().candidate_root, b"data.bin")
                )
                .1,
                10,
                4
            ),
            b"LIVE"
        );
        observe(&f);
        close(&f, &[handle], &[data.serial]);
        check("known-C5-success-retries-local-reconciliation-without-replay");
    }

    #[test]
    #[ignore = "requires explicit external stage consumption through C5"]
    fn commit_consumed_stage() {
        let f = Fixture::new(Gate::None);
        f.lookup(b"data.bin");
        f.edit(b"data.bin", 10, 14, b"GGGG");
        let stage = f.workspace.stage(deadline()).unwrap();
        f.native
            .request(
                Operation::HistoryCommand(HistoryCommand::CommitStaged {
                    workspace: [31; 32],
                    token: stage.stage().token,
                }),
                16384,
                &mut std::io::sink(),
            )
            .unwrap();
        let WorkspaceError::Commit(failure) =
            f.workspace.commit_staged(&stage, deadline()).unwrap_err()
        else {
            panic!("consumption must not become own success")
        };
        assert!(
            matches!(&failure.cause,WorkspaceError::Service(cause) if cause.code==Code::StageChanged&&!cause.unknown)
        );
        assert!(failure.known_outcome.is_none());
        assert_eq!(snapshot(&f).effective_root, stage.stage().candidate_root);
        retained_failure(&f, &stage);
        println!("COMMIT_FAILURE {failure:?}");
        check("consumed-token-and-identical-root-do-not-prove-own-success");
    }

    #[test]
    #[ignore = "requires independent Branch writer after Stage acknowledgement"]
    fn commit_head_moved() {
        let f = Fixture::new(Gate::None);
        let data = f.lookup(b"data.bin");
        let before = snapshot(&f);
        f.edit(b"data.bin", 10, 14, b"GGGG");
        let stage = f.workspace.stage(deadline()).unwrap();
        f.native
            .request(
                Operation::HistoryCommand(HistoryCommand::Commit(PreparedChanges {
                    directory_metadata: Vec::new(),
                    new_directories: Vec::new(),
                    new_file_serials: Vec::new(),
                    new_symlink_serials: Vec::new(),
                    workspace: [44; 32],
                    branch: before.branch.branch,
                    expected_head: before.branch.head_commit,
                    expected_base: before.branch.base_layer,
                    generation: 1,
                    base: before.effective_root,
                    scope: before.scope,
                    root_serial: before.root_serial.unwrap(),
                    directories: vec![DirectoryChange {
                        parent: before.root_serial.unwrap(),
                        changes: vec![
                            (b"alias".to_vec(), None),
                            (b"other-alias".to_vec(), Some(data.serial)),
                        ],
                    }],
                    inodes: vec![],
                })),
                16384,
                &mut std::io::sink(),
            )
            .unwrap();
        let WorkspaceError::Commit(failure) =
            f.workspace.commit_staged(&stage, deadline()).unwrap_err()
        else {
            panic!("missing conflict")
        };
        let WorkspaceError::Service(cause) = &failure.cause else {
            panic!("missing service failure")
        };
        assert_eq!(cause.code, Code::HeadMoved);
        assert!(!cause.unknown);
        assert!(
            matches!(&cause.history.as_ref().unwrap().stage,StageObservation::Retained(found) if found.as_ref()==stage.stage())
        );
        assert!(failure.known_outcome.is_none());
        f.edit(b"data.bin", 10, 14, b"LIVE");
        retained_failure(&f, &stage);
        println!("COMMIT_FAILURE {failure:?}");
        check("Branch-move-preserves-exact-losing-stage");
    }

    #[test]
    #[ignore = "requires live fixture; repeated roots remain accounted and bounded"]
    fn commit_cycles() {
        let f = Fixture::new(Gate::None);
        let data = f.lookup(b"data.bin");
        let handle = f
            .workspace
            .open(data.serial, ReferenceScope::Local)
            .unwrap();
        let mut maximum = 0;
        for index in 0..12 {
            f.edit(b"data.bin", 10, 11, &[index]);
            commit(&f);
            assert_eq!(f.read(handle, 10, 1), [index]);
            let status = f.workspace.metadata_status().unwrap();
            maximum = maximum.max(status.roots);
            assert_eq!(status.reserved_slots, 0);
            assert!(status.roots < 12);
        }
        assert_eq!(count_commits(&f), 12);
        println!("COMMIT_CYCLES completed=12 maximum_boundary_roots={maximum}");
        observe(&f);
        close(&f, &[handle], &[data.serial]);
        check("repeated-commit-frontiers-and-eligible-root-cleanup-stay-bounded");
    }

    #[test]
    #[ignore = "requires ordinary disk quota occupied before CommitStaged"]
    fn commit_headroom() {
        let f = Fixture::with_quota(Gate::None, 4 * 1024 * 1024);
        let data = f.lookup(b"data.bin");
        let handle = f
            .workspace
            .open(data.serial, ReferenceScope::Local)
            .unwrap();
        f.edit(b"data.bin", 10, 14, b"GGGG");
        let stage = f.workspace.stage(deadline()).unwrap();
        f.edit(b"data.bin", 10, 14, b"LIVE");
        let before = f.workspace.backing_status().unwrap();
        let available = before.quota_bytes - before.allocated_bytes - before.reserved_bytes;
        let mut blocks = available / 4096;
        while blocks + blocks.div_ceil(256) > available / 4096 {
            blocks -= 1;
        }
        let spare = f.own(&vec![0xcc; blocks as usize * 4096]);
        let full = f.workspace.backing_status().unwrap();
        assert!(full.quota_bytes - full.allocated_bytes - full.reserved_bytes <= 4096);
        let report = f.workspace.commit_staged(&stage, deadline()).unwrap();
        committed(&f, &stage, &report);
        assert_eq!(f.read(handle, 10, 4), b"LIVE");
        assert_eq!(f.workspace.metadata_status().unwrap().reserved_slots, 0);
        println!(
            "COMMIT_HEADROOM full={full:?} after={:?} unrelated_owned_bytes={}",
            f.workspace.backing_status().unwrap(),
            spare.len()
        );
        observe(&f);
        check("pre-reserved-reconciliation-progress-with-ordinary-quota-occupied");
    }

    #[test]
    #[ignore = "requires wide fixture; 104 exact successor associations"]
    fn commit_frontier() {
        let f = Fixture::new(Gate::None);
        let mut names = vec![b"data.bin".to_vec(), b"other.bin".to_vec()];
        names.extend((0..102).map(|i| format!("f{i:03}").into_bytes()));
        for (index, name) in names.iter().enumerate() {
            f.lookup(name);
            f.edit(name, 10, 12, &[index as u8, 0xfe]);
            f.workspace.reclaim_metadata(deadline()).unwrap();
        }
        let stage = f
            .workspace
            .stage(Instant::now() + Duration::from_secs(25))
            .unwrap();
        for (index, name) in names.iter().enumerate() {
            f.edit(name, 10, 12, &[index as u8, 0xfd]);
            f.workspace.reclaim_metadata(deadline()).unwrap();
        }
        let first = f.workspace.commit_staged(&stage, deadline()).unwrap();
        committed(&f, &stage, &first);
        assert_eq!(f.workspace.status().unwrap().dirty_inodes, 104);
        let next = f
            .workspace
            .stage(Instant::now() + Duration::from_secs(25))
            .unwrap();
        let second = f.workspace.commit_staged(&next, deadline()).unwrap();
        committed(&f, &next, &second);
        for (index, name) in names.iter().enumerate() {
            let saved = attr(f.native.attributes(next.stage().candidate_root, name));
            assert_eq!(f.native.bytes(saved.1, 10, 2), [index as u8, 0xfd]);
        }
        let observed = f.native.observations.lock().unwrap();
        assert_eq!(observed.saved_files.len(), 208);
        drop(observed);
        assert_eq!(f.workspace.status().unwrap().dirty_inodes, 0);
        observe(&f);
        check("streamed-104-inode-reconciliation-and-next-generation-save");
    }
    #[test]
    #[ignore = "requires real native canonical-read admission held by another call"]
    fn commit_remote_admission() {
        let f = Fixture::new(Gate::ReadHold);
        f.lookup(b"data.bin");
        let other = f.lookup(b"other.bin");
        let handle = f
            .workspace
            .open(other.serial, ReferenceScope::Local)
            .unwrap();
        f.edit(b"data.bin", 10, 14, b"GGGG");
        let stage = f.workspace.stage(deadline()).unwrap();
        let ws = f.workspace.clone();
        let reader = std::thread::spawn(move || ws.read(handle, 0, 4, deadline()));
        f.native.wait_read();
        assert_eq!(
            f.workspace.commit_staged(&stage, deadline()),
            Err(WorkspaceError::Busy)
        );
        assert!(f
            .workspace
            .status()
            .unwrap()
            .submission
            .unwrap()
            .commit
            .is_none());
        assert_eq!(f.workspace.metadata_status().unwrap().reserved_slots, 0);
        assert_eq!(count_commits(&f), 0);
        f.native.release_read();
        let reply = reader.join().unwrap().unwrap();
        assert_eq!(reply.as_ref(), [0, 1, 2, 3]);
        drop(reply);
        let report = f.workspace.commit_staged(&stage, deadline()).unwrap();
        committed(&f, &stage, &report);
        assert_eq!(count_commits(&f), 1);
        check("remote-admission-refusal-leaves-stage-usable-without-replay");
    }
}
