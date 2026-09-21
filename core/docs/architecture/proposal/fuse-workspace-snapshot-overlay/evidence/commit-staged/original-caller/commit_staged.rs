//! Exact C5 commit and live successor reconciliation through production APIs.
#[cfg(target_os = "linux")]
#[path = "support/native_workspace.rs"]
mod support;
#[cfg(target_os = "linux")]
mod linux {
    use super::support::*;
    use layerfs_bridge::contract::*;
    use layerfs_workspace::*;
    use std::time::{Duration, Instant};

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
        assert_eq!(report.stage_token, selector.stage().token);
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
                if let Operation::EditFile { root, edits, .. } = op {
                    Some((*root, edits))
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].0, first_root);
        assert_eq!(changes[0].1.iter().map(|e| e.replacement).sum::<u64>(), 1);
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
        assert_eq!(f.read(handle, 8, 6), [8, 9, b'Z', 14, 15, 16]);
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
                } = op
                {
                    Some((*root, *base_length, edits))
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].0, saved_root);
        assert_eq!(changes[0].1, data.size);
        assert_eq!(
            changes[0].2,
            &vec![Edit {
                start: 10,
                end: 14,
                replacement: 1
            }]
        );
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
        retained_failure(&f, &stage);
        println!("COMMIT_FAILURE {failure:?}");
        observe(&f);
        check("known-C5-success-survives-native-reconciliation-failure");
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
