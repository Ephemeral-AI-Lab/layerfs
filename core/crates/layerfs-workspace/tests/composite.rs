//! Ordinary Commit through the real composite service command, never local UpToDate.
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
        println!("COMPOSITE_CHECK {id} PASS");
    }
    fn snapshot(f: &Fixture) -> BranchSnapshotWire {
        let Response::History(result) = f.branch() else {
            panic!("history missing")
        };
        let HistoryResult::BranchSnapshot(snapshot) = *result else {
            panic!("snapshot missing")
        };
        snapshot
    }
    fn root(report: &CommitReport) -> Root {
        match &report.outcome {
            CommitOutcomeWire::Committed(c) => c.root,
            CommitOutcomeWire::UpToDate { root, .. } => *root,
        }
    }
    fn completed(f: &Fixture, report: &CommitReport) {
        assert_eq!(report.stage_token, None);
        let actual = snapshot(f);
        assert_eq!(actual.effective_root, root(report));
        match &report.outcome {
            CommitOutcomeWire::Committed(c) => {
                assert_eq!(actual.branch.head_commit, Some(c.commit));
                assert_eq!(actual.branch.stack, c.stack);
                assert_eq!(actual.branch.base_layer, c.base_layer);
            }
            CommitOutcomeWire::UpToDate { head, .. } => {
                assert_eq!(actual.branch.head_commit, *head)
            }
        }
        let status = f.workspace.status().unwrap();
        assert!(status.submission.is_none());
        assert_eq!(status.revision, report.revision);
        assert_eq!(f.workspace.metadata_status().unwrap().reserved_slots, 0);
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
    }
    fn commit(f: &Fixture) -> CommitReport {
        let report = f.workspace.commit(deadline()).unwrap();
        completed(f, &report);
        report
    }
    fn prepared(f: &Fixture) -> Vec<PreparedChanges> {
        let observed = f.native.observations.lock().unwrap();
        assert!(observed.operations.iter().all(|op| !matches!(
            op,
            Operation::HistoryCommand(
                HistoryCommand::StageChanges(_) | HistoryCommand::CommitStaged { .. }
            )
        )));
        observed
            .operations
            .iter()
            .filter_map(|op| match op {
                Operation::HistoryCommand(HistoryCommand::Commit(changes))
                    if changes.workspace == [31; 32] =>
                {
                    Some(changes.clone())
                }
                _ => None,
            })
            .collect()
    }
    fn observe(f: &Fixture) {
        println!(
            "COMMIT_RESOURCE {:?} {:?} {:?}",
            f.workspace.status().unwrap(),
            f.workspace.backing_status().unwrap(),
            f.workspace.metadata_status().unwrap()
        );
    }
    fn retained(f: &Fixture) {
        let count = f.native.observations.lock().unwrap().operations.len();
        assert_eq!(f.workspace.commit(deadline()), Err(WorkspaceError::Busy));
        assert!(matches!(
            f.workspace.stage(deadline()),
            Err(WorkspaceError::Busy)
        ));
        assert_eq!(
            f.native.observations.lock().unwrap().operations.len(),
            count
        );
        assert_eq!(f.workspace.close_clean(), Err(WorkspaceError::Busy));
    }
    fn open(f: &Fixture, name: &[u8]) -> (NodeAttributes, HandleId) {
        let attrs = f.lookup(name);
        let handle = f
            .workspace
            .open(attrs.serial, ReferenceScope::Local)
            .unwrap();
        (attrs, handle)
    }

    #[test]
    #[ignore = "requires composite_route.py live native service"]
    fn composite_clean() {
        let f = Fixture::new(Gate::None);
        let initial = snapshot(&f);
        let first = commit(&f);
        assert_eq!(
            first.outcome,
            CommitOutcomeWire::UpToDate {
                head: initial.branch.head_commit,
                root: initial.effective_root
            }
        );
        assert_eq!(first.generation, 1);
        assert!(prepared(&f)[0].inodes.is_empty());
        assert!(prepared(&f)[0].directories.is_empty());
        assert!(f.native.observations.lock().unwrap().saved_files.is_empty());
        let (data, handle) = open(&f, b"data.bin");
        f.edit(b"data.bin", 10, 14, b"EDIT");
        let changed = commit(&f);
        assert!(matches!(changed.outcome, CommitOutcomeWire::Committed(_)));
        let later = commit(&f);
        let head = snapshot(&f).branch.head_commit;
        assert_eq!(
            later.outcome,
            CommitOutcomeWire::UpToDate {
                head,
                root: root(&changed)
            }
        );
        assert_eq!(f.read(handle, 10, 4), b"EDIT");
        let requests = prepared(&f);
        assert_eq!(requests.len(), 3);
        assert_eq!(
            requests.iter().map(|p| p.inodes.len()).collect::<Vec<_>>(),
            [0, 1, 0]
        );
        assert_eq!(requests[2].base, root(&changed));
        assert_eq!(f.native.observations.lock().unwrap().saved_files.len(), 1);
        assert!(matches!(
            f.workspace.stage(deadline()),
            Err(WorkspaceError::InvalidInput)
        ));
        observe(&f);
        f.workspace.release(handle).unwrap();
        f.workspace
            .forget(data.serial, u64::MAX, ReferenceScope::Local);
        f.workspace.close_clean().unwrap();
        check("actual-clean-UpToDate-before-and-after-dirty-Commit");
    }

    #[test]
    #[ignore = "requires actual clean C5 acknowledgement gate"]
    fn composite_clean_successor() {
        let f = Fixture::new(Gate::CommitReply);
        let (_, handle) = open(&f, b"data.bin");
        let before = snapshot(&f);
        let ws = f.workspace.clone();
        let saving = std::thread::spawn(move || ws.commit(deadline()));
        f.native.wait_commit();
        assert_eq!(f.workspace.status().unwrap().generation, 2);
        assert_eq!(f.workspace.metadata_status().unwrap().reserved_slots, 26);
        f.edit(b"data.bin", 10, 14, b"LIVE");
        assert_eq!(f.read(handle, 10, 4), b"LIVE");
        f.native.release_commit();
        let report = saving.join().unwrap().unwrap();
        completed(&f, &report);
        assert_eq!(
            report.outcome,
            CommitOutcomeWire::UpToDate {
                head: before.branch.head_commit,
                root: before.effective_root
            }
        );
        assert_eq!(f.workspace.status().unwrap().dirty_inodes, 1);
        assert_eq!(f.read(handle, 10, 4), b"LIVE");
        let next = commit(&f);
        assert!(matches!(next.outcome, CommitOutcomeWire::Committed(_)));
        let requests = prepared(&f);
        assert_eq!(requests.len(), 2);
        assert!(requests[0].inodes.is_empty());
        assert_eq!(requests[1].inodes.len(), 1);
        assert_eq!(requests[1].base, before.effective_root);
        let saved = attr(f.native.attributes(root(&next), b"data.bin"));
        assert_eq!(f.native.bytes(saved.1, 10, 4), b"LIVE");
        observe(&f);
        check("clean-capture-preserves-late-D1-and-next-composite-input");
    }

    #[test]
    #[ignore = "requires composite_route.py live native service"]
    fn composite_repeated() {
        let f = Fixture::new(Gate::None);
        let (data, handle) = open(&f, b"data.bin");
        assert_eq!(f.lookup(b"alias").serial, data.serial);
        f.lookup(b"other.bin");
        let mut roots = Vec::new();
        f.edit(b"data.bin", 10, 14, b"AAAA");
        let old = f.workspace.read(handle, 10, 4, deadline()).unwrap();
        roots.push(root(&commit(&f)));
        let a = attr(f.native.attributes(roots[0], b"data.bin")).1;
        f.edit(b"other.bin", 10, 12, b"BB");
        roots.push(root(&commit(&f)));
        assert_eq!(attr(f.native.attributes(roots[1], b"data.bin")).1, a);
        let before = f.native.observations.lock().unwrap().operations.len();
        f.edit(b"alias", 11, 13, b"x");
        roots.push(root(&commit(&f)));
        assert_eq!(f.read(handle, 10, 3), b"AxA");
        assert_eq!(old.as_ref(), b"AAAA");
        let observed = f.native.observations.lock().unwrap();
        let inputs: Vec<_> = observed.operations[before..]
            .iter()
            .filter_map(|op| match op {
                Operation::EditFile { root, edits, .. } => Some((*root, edits)),
                _ => None,
            })
            .collect();
        assert_eq!(inputs.len(), 1);
        assert_eq!(inputs[0].0, a);
        assert_eq!(inputs[0].1.iter().map(|e| e.replacement).sum::<u64>(), 1);
        assert_eq!(observed.saved_files.len(), 3);
        drop(observed);
        let requests = prepared(&f);
        assert_eq!(requests.len(), 3);
        assert!(requests
            .iter()
            .all(|p| p.inodes.len() == 1 && p.directories.is_empty()));
        assert_eq!(requests[1].base, roots[0]);
        assert_eq!(requests[2].base, roots[1]);
        drop(old);
        observe(&f);
        check("one-composite-command-per-incremental-alias-Commit");
    }

    #[test]
    #[ignore = "requires first file delivery hold and real composite command"]
    fn composite_successor() {
        let f = Fixture::new(Gate::Delivery);
        let (data, handle) = open(&f, b"data.bin");
        f.edit(b"data.bin", 10, 14, b"GGGG");
        let frozen = f.workspace.getattr(data.serial).unwrap();
        let ws = f.workspace.clone();
        let saving = std::thread::spawn(move || ws.commit(deadline()));
        f.native.wait_entered();
        assert_eq!(f.workspace.metadata_status().unwrap().reserved_slots, 26);
        f.edit(b"data.bin", 11, 13, b"LIVEIN");
        assert_eq!(f.read(handle, 10, 8), b"GLIVEING");
        let old = f.workspace.read(handle, 10, 8, deadline()).unwrap();
        f.native.release();
        let first = saving.join().unwrap().unwrap();
        completed(&f, &first);
        let saved = attr(f.native.attributes(root(&first), b"data.bin"));
        assert_eq!(f.native.bytes(saved.1, 10, 4), b"GGGG");
        assert_eq!(
            (saved.3, saved.4),
            (frozen.mtime_seconds, frozen.mtime_nanoseconds)
        );
        f.edit(b"data.bin", 10, 18, b"Z");
        let before = f.native.observations.lock().unwrap().operations.len();
        let second = commit(&f);
        let observed = f.native.observations.lock().unwrap();
        let edits: Vec<_> = observed.operations[before..]
            .iter()
            .filter_map(|op| match op {
                Operation::EditFile {
                    root,
                    base_length,
                    edits,
                } => Some((*root, *base_length, edits)),
                _ => None,
            })
            .collect();
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].0, saved.1);
        assert_eq!(edits[0].1, data.size);
        assert_eq!(
            edits[0].2,
            &vec![Edit {
                start: 10,
                end: 14,
                replacement: 1
            }]
        );
        drop(observed);
        assert_eq!(f.read(handle, 8, 6), [8, 9, b'Z', 14, 15, 16]);
        assert_eq!(old.as_ref(), b"GLIVEING");
        let final_file = attr(f.native.attributes(root(&second), b"data.bin"));
        assert_eq!(f.native.bytes(final_file.1, 8, 6), [8, 9, b'Z', 14, 15, 16]);
        assert_eq!(prepared(&f).len(), 2);
        observe(&f);
        check("composite-capture-reconciles-exact-G-with-live-successor");
    }

    fn native_save(kill: bool) {
        let f = Fixture::new(Gate::NativeSave);
        let (_, handle) = open(&f, b"data.bin");
        let mut bytes = vec![0; 4 * 1024 * 1024];
        let mut random = 0x6a09e667f3bcc909u64;
        for byte in &mut bytes {
            random ^= random << 13;
            random ^= random >> 7;
            random ^= random << 17;
            *byte = random as u8;
        }
        f.edit(b"data.bin", 0, bytes.len() as u64, &bytes);
        let ws = f.workspace.clone();
        let saving = std::thread::spawn(move || ws.commit(deadline()));
        let mut line = String::new();
        std::io::stdin().read_line(&mut line).unwrap();
        assert_eq!(line.trim(), "SAVE_PAUSED");
        f.edit(b"data.bin", 10, 14, b"LIVE");
        assert_eq!(f.read(handle, 10, 4), b"LIVE");
        println!("STAGE_LIVE_READY");
        std::io::stdout().flush().unwrap();
        let result = saving.join().unwrap();
        if kill {
            let WorkspaceError::Commit(failure) = result.unwrap_err() else {
                panic!("missing composite preparation failure")
            };
            assert_eq!(failure.phase, CommitPhase::Preparing);
            assert_eq!(failure.disposition, CommitFailureDisposition::Unknown);
            let WorkspaceError::Stage(stage) = &failure.cause else {
                panic!("source/progress cause missing")
            };
            assert_eq!(stage.phase, StagePhase::FileSave);
            assert!(matches!(&stage.cause,WorkspaceError::Service(c) if c.unknown));
            assert!(failure.known_outcome.is_none());
            assert!(failure.stage.is_none());
            assert!(prepared(&f).is_empty());
            f.edit(b"data.bin", 10, 14, b"NEXT");
            assert_eq!(f.read(handle, 10, 4), b"NEXT");
            retained(&f);
            println!("COMMIT_FAILURE {failure:?}");
            check("composite-prerequisite-loss-retains-G-D1-and-source-outcome");
        } else {
            let report = result.unwrap();
            completed(&f, &report);
            let saved = attr(f.native.attributes(root(&report), b"data.bin"));
            assert_eq!(
                f.native.bytes(saved.1, 0, MAX_READ_BYTES),
                bytes[..MAX_READ_BYTES]
            );
            assert_eq!(f.read(handle, 10, 4), b"LIVE");
            let next = commit(&f);
            let saved = attr(f.native.attributes(root(&next), b"data.bin"));
            assert_eq!(f.native.bytes(saved.1, 10, 4), b"LIVE");
            assert_eq!(prepared(&f).len(), 2);
            observe(&f);
            check("composite-progress-during-actual-service-save");
        }
    }
    #[test]
    #[ignore = "requires native SQLite RESERVED lock observation"]
    fn composite_native_save() {
        native_save(false);
    }
    #[test]
    #[ignore = "requires loss during actual native C2 save"]
    fn composite_unknown_save() {
        native_save(true);
    }

    #[test]
    #[ignore = "requires opaque native composite-terminal loss proxy"]
    fn composite_lost_result() {
        let f = Fixture::new(Gate::CommitUnknown);
        let (_, handle) = open(&f, b"data.bin");
        let before = snapshot(&f);
        f.edit(b"data.bin", 10, 14, b"GGGG");
        let WorkspaceError::Commit(failure) = f.workspace.commit(deadline()).unwrap_err() else {
            panic!("missing retained failure")
        };
        assert_eq!(failure.phase, CommitPhase::CompositeCommit);
        assert_eq!(failure.disposition, CommitFailureDisposition::Unknown);
        assert!(failure.stage.is_none());
        assert!(failure.observed_stage.is_none());
        assert!(failure.known_outcome.is_none());
        assert!(failure.observed_outcome.is_none());
        let actual = snapshot(&f);
        assert_ne!(actual.branch.head_commit, before.branch.head_commit);
        let saved = attr(f.native.attributes(actual.effective_root, b"data.bin"));
        assert_eq!(f.native.bytes(saved.1, 10, 4), b"GGGG");
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
        f.edit(b"data.bin", 10, 14, b"LIVE");
        assert_eq!(f.read(handle, 10, 4), b"LIVE");
        retained(&f);
        assert_eq!(prepared(&f).len(), 1);
        println!("COMMIT_FAILURE {failure:?}");
        println!(
            "COMMIT_LATER_OBSERVATION root={:?} head={:?} original_outcome=Unknown",
            actual.effective_root, actual.branch.head_commit
        );
        check("composite-terminal-loss-does-not-adopt-later-Branch-observation");
    }

    struct RestoreLimit;
    impl Drop for RestoreLimit {
        fn drop(&mut self) {
            file_limit("unlimited");
        }
    }
    #[test]
    #[ignore = "requires actual C5 success then native private-file failure"]
    fn composite_reconcile_failure() {
        let f = Fixture::new(Gate::CompositeCompletionFailure);
        f.lookup(b"data.bin");
        f.edit(b"data.bin", 10, 14, b"GGGG");
        let ws = f.workspace.clone();
        let restore = RestoreLimit;
        let saving = std::thread::spawn(move || ws.commit(deadline()));
        f.native.wait_entered();
        f.edit(b"data.bin", 10, 14, b"LIVE");
        f.native.release();
        let result = saving.join().unwrap();
        drop(restore);
        let WorkspaceError::Commit(failure) = result.unwrap_err() else {
            panic!("known outcome missing")
        };
        assert_eq!(failure.phase, CommitPhase::Reconcile);
        assert_eq!(
            failure.disposition,
            CommitFailureDisposition::KnownCommitLocalFailure
        );
        assert!(failure.stage.is_none());
        assert!(failure.observed_stage.is_none());
        assert!(failure.installed_revision.is_none());
        let known = f.native.observations.lock().unwrap().commits[0].clone();
        assert_eq!(failure.known_outcome, Some(known.clone()));
        let CommitOutcomeWire::Committed(commit) = known else {
            panic!("expected changed commit")
        };
        assert_eq!(snapshot(&f).effective_root, commit.root);
        retained(&f);
        println!("COMMIT_FAILURE {failure:?}");
        observe(&f);
        check("composite-known-success-survives-local-reconcile-failure");
    }

    #[test]
    #[ignore = "requires native authenticated composite denial"]
    fn composite_denied() {
        let f = Fixture::new(Gate::CommitDenied);
        let (_, handle) = open(&f, b"data.bin");
        let before = f.branch();
        f.edit(b"data.bin", 10, 14, b"GGGG");
        let WorkspaceError::Commit(failure) = f.workspace.commit(deadline()).unwrap_err() else {
            panic!("missing denial")
        };
        assert_eq!(failure.phase, CommitPhase::CompositeCommit);
        assert_eq!(
            failure.disposition,
            CommitFailureDisposition::KnownBeforeCommit
        );
        assert!(
            matches!(&failure.cause,WorkspaceError::Service(c) if c.code==Code::Denied&&!c.unknown)
        );
        assert!(failure.stage.is_none());
        assert!(failure.known_outcome.is_none());
        assert_eq!(f.branch(), before);
        assert_eq!(f.native.observations.lock().unwrap().saved_files.len(), 1);
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
        f.edit(b"data.bin", 10, 14, b"LIVE");
        assert_eq!(f.read(handle, 10, 4), b"LIVE");
        retained(&f);
        println!("COMMIT_FAILURE {failure:?}");
        observe(&f);
        check("composite-denial-preserves-file-acknowledgements-and-live-state");
    }

    #[test]
    #[ignore = "requires independent Branch advance before empty composite command"]
    fn composite_head_moved() {
        let f = Fixture::new(Gate::CompositeBefore);
        let data = f.lookup(b"data.bin");
        let before = snapshot(&f);
        let ws = f.workspace.clone();
        let saving = std::thread::spawn(move || ws.commit(deadline()));
        f.native.wait_commit();
        f.native
            .request(
                Operation::HistoryCommand(HistoryCommand::Commit(PreparedChanges {
                    directory_metadata: Vec::new(),
                    new_directories: Vec::new(),
                    new_file_serials: Vec::new(),
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
        f.native.release_commit();
        let WorkspaceError::Commit(failure) = saving.join().unwrap().unwrap_err() else {
            panic!("expected stale head")
        };
        assert_eq!(failure.phase, CommitPhase::CompositeCommit);
        assert_eq!(
            failure.disposition,
            CommitFailureDisposition::KnownBeforeCommit
        );
        assert!(
            matches!(&failure.cause,WorkspaceError::Service(c) if c.code==Code::HeadMoved&&!c.unknown)
        );
        assert!(failure.known_outcome.is_none());
        assert!(failure.stage.is_none());
        assert_eq!(prepared(&f).len(), 1);
        assert!(prepared(&f)[0].inodes.is_empty());
        retained(&f);
        println!("COMMIT_FAILURE {failure:?}");
        check("clean-composite-stale-head-is-not-local-UpToDate");
    }

    #[test]
    #[ignore = "requires native backing and exact clean admission"]
    fn composite_refusals() {
        {
            let f = Fixture::with_quota(Gate::None, 512 * 1024);
            let before = f.workspace.status().unwrap();
            assert!(f.workspace.commit(deadline()).is_err());
            let after = f.workspace.status().unwrap();
            assert_eq!(
                (after.generation, after.revision),
                (before.generation, before.revision)
            );
            assert!(after.submission.is_none());
            assert!(prepared(&f).is_empty());
            assert_eq!(f.workspace.metadata_status().unwrap().reserved_slots, 0);
            let ro = f
                .host
                .attach(
                    Fixture::options("readonly", 32, WorkspaceAccess::ReadOnly),
                    deadline(),
                )
                .unwrap();
            assert_eq!(ro.commit(deadline()), Err(WorkspaceError::ReadOnly));
            ro.close_clean().unwrap();
            f.workspace.close_clean().unwrap();
        }
        let f = Fixture::new(Gate::None);
        assert!(f
            .workspace
            .commit(Instant::now() - Duration::from_secs(1))
            .is_err());
        assert!(prepared(&f).is_empty());
        f.lookup(b"data.bin");
        f.edit(b"data.bin", 10, 14, b"GGGG");
        let selected = f.workspace.stage(deadline()).unwrap();
        let count = f.native.observations.lock().unwrap().operations.len();
        assert_eq!(f.workspace.commit(deadline()), Err(WorkspaceError::Busy));
        assert_eq!(
            f.native.observations.lock().unwrap().operations.len(),
            count
        );
        let result = f.workspace.commit_staged(&selected, deadline()).unwrap();
        assert_eq!(result.stage_token, Some(selected.stage().token));
        observe(&f);
        check("clean-quota-readonly-deadline-and-staged-refusals");
    }

    #[test]
    #[ignore = "requires existing portable metadata save route"]
    fn composite_metadata_only() {
        let f = Fixture::new(Gate::None);
        let data = f.lookup(b"data.bin");
        let before = snapshot(&f);
        let content = attr(f.native.attributes(before.effective_root, b"data.bin")).1;
        f.edit(b"data.bin", 0, 0, b"");
        let changed = f.workspace.getattr(data.serial).unwrap();
        let report = commit(&f);
        let saved = attr(f.native.attributes(root(&report), b"data.bin"));
        assert_eq!(saved.1, content);
        assert_eq!(
            (saved.3, saved.4),
            (changed.mtime_seconds, changed.mtime_nanoseconds)
        );
        assert!(f.native.observations.lock().unwrap().saved_files.is_empty());
        assert_eq!(prepared(&f)[0].inodes.len(), 1);
        check("composite-metadata-only-save-preserves-content-root");
    }

    #[test]
    #[ignore = "requires full 104-inode fixture and 25-second composite deadline"]
    fn composite_frontier() {
        let f = Fixture::new(Gate::None);
        let mut names = vec![b"data.bin".to_vec(), b"other.bin".to_vec()];
        names.extend((0..102).map(|i| format!("f{i:03}").into_bytes()));
        for (index, name) in names.iter().enumerate() {
            f.lookup(name);
            f.edit(name, 10, 12, &[index as u8, 0xfe]);
            f.workspace.reclaim_metadata(deadline()).unwrap();
        }
        let first = f
            .workspace
            .commit(Instant::now() + Duration::from_secs(25))
            .unwrap();
        completed(&f, &first);
        for (index, name) in names.iter().enumerate() {
            let saved = attr(f.native.attributes(root(&first), name));
            assert_eq!(f.native.bytes(saved.1, 10, 2), [index as u8, 0xfe]);
        }
        f.edit(b"data.bin", 10, 12, b"N2");
        let second = commit(&f);
        let requests = prepared(&f);
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].inodes.len(), 104);
        assert_eq!(requests[1].inodes.len(), 1);
        assert_eq!(requests[1].base, root(&first));
        assert_eq!(f.native.observations.lock().unwrap().saved_files.len(), 105);
        for (index, name) in names.iter().enumerate() {
            let saved = attr(f.native.attributes(root(&second), name));
            let expected = if index == 0 {
                *b"N2"
            } else {
                [index as u8, 0xfe]
            };
            assert_eq!(f.native.bytes(saved.1, 10, 2), expected);
        }
        observe(&f);
        check("composite-104-inode-frontier-and-next-generation");
    }
    #[test]
    #[ignore = "requires an actual native read held across pre-admission"]
    fn composite_remote_admission() {
        let f = Fixture::new(Gate::ReadHold);
        let (_, handle) = open(&f, b"data.bin");
        let before = f.workspace.status().unwrap();
        let ws = f.workspace.clone();
        let reader = std::thread::spawn(move || ws.read(handle, 0, 4, deadline()));
        f.native.wait_read();
        assert_eq!(f.workspace.commit(deadline()), Err(WorkspaceError::Busy));
        let after = f.workspace.status().unwrap();
        assert_eq!(
            (before.generation, before.revision),
            (after.generation, after.revision)
        );
        assert!(after.submission.is_none());
        assert_eq!(f.workspace.metadata_status().unwrap().reserved_slots, 0);
        assert!(prepared(&f).is_empty());
        f.native.release_read();
        let reply = reader.join().unwrap().unwrap();
        assert_eq!(reply.as_ref(), [0, 1, 2, 3]);
        drop(reply);
        assert!(matches!(
            commit(&f).outcome,
            CommitOutcomeWire::UpToDate { .. }
        ));
        assert_eq!(prepared(&f).len(), 1);
        check("remote-pre-admission-refuses-before-clean-capture");
    }
}
