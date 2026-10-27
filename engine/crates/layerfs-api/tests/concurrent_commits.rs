//! Concurrent Commits to one Branch (#352 F-008). A stale Commit merges with
//! the head current at its publication: different paths are Committed with a
//! merge, the same path is the planned Conflict. Nothing here sleeps; barriers
//! start the Commits together and every wait is bounded by the test ceiling.
//! Workspace Commits go through the daemon; the SDK calls that set up and read
//! back run in-process over the same Store (#352 F-020).
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_bridge::control::{CommitControl, Reply, Request, SdkValue, UnmountControl};
use layerfs_content::filesystem::PathName;
use layerfs_daemon::control::Service;
use layerfs_history::{BranchId, HistoryName, LayerStackId, WorkspaceId};
use layerfs_workspace::{Operation, Outcome, Position, Time};
use std::{fs, sync::Barrier};

const LANE: &str = "lane";

/// SDK calls in-process and a daemon `Service` over the same opened Store.
struct Fixture {
    sdk: support::Fixture,
    service: Service,
}
impl Fixture {
    fn call(&self, method: &str, fields: Vec<(&str, SdkValue)>) -> SdkValue {
        self.sdk.call(method, fields)
    }
    fn cleanup(self) {
        drop(self.service);
        self.sdk.cleanup();
    }
}

fn fixture(label: &str) -> Fixture {
    let sdk = support::Fixture::new(label, |source| {
        fs::write(source.join("seed.txt"), b"seed\n").unwrap();
        1
    });
    sdk.fork(LANE);
    let service = Service::new(sdk.service.store.clone(), &sdk.owner);
    Fixture { sdk, service }
}

/// Mounts a Workspace on the lane and creates `/name` holding `bytes`,
/// without committing.
fn mounted_write(
    fixture: &Fixture,
    id: u8,
    name: &str,
    bytes: &[u8],
) -> layerfs_bridge::control::WorkspaceToken {
    let bound = fixture
        .service
        .execute_control(&Request::Mount {
            workspace: WorkspaceId::from_authority([id; 32]).unwrap(),
            branch: BranchId::named(
                LayerStackId::from_authority([23; 16]),
                &HistoryName::new(LANE).unwrap(),
            ),
        })
        .unwrap();
    let Reply::Bound { token, .. } = bound.reply else {
        panic!("binding: {:?}", bound.reply);
    };
    let operation = fixture.service.operation(token).unwrap();
    let route = operation.workspace().route();
    let source = operation
        .overlay()
        .acquire_base_source(route, 9000 + u64::from(id))
        .unwrap();
    let view = operation.workspace().view_for_source(source).unwrap();
    let now = Time {
        seconds: 123,
        nanoseconds: 0,
    };
    let apply = |op| {
        let outcome = operation
            .workspace()
            .mutate(operation.overlay(), operation.ports(), &view, op, now)
            .unwrap();
        let Outcome::Applied { publication, stat } = outcome else {
            panic!("mutation: {outcome:?}");
        };
        operation.overlay().reply_attempted(publication).unwrap();
        stat
    };
    let created = apply(Operation::Create {
        parent: view.root_serial(),
        name: PathName::new(name).unwrap(),
        mode: 0o644,
    })
    .expect("created inode");
    apply(Operation::Write {
        serial: created.serial,
        position: Position::At(0),
        data: bytes.to_vec().into(),
    });
    drop(view);
    operation
        .overlay()
        .release_base_source(route, source)
        .unwrap();
    token
}

/// Commits every Workspace at once and returns each reply, in Workspace order.
fn commit_together(
    fixture: &Fixture,
    tokens: &[layerfs_bridge::control::WorkspaceToken],
) -> Vec<Reply> {
    let barrier = Barrier::new(tokens.len());
    let replies = std::thread::scope(|scope| {
        let workers: Vec<_> = tokens
            .iter()
            .map(|token| {
                let service = &fixture.service;
                let barrier = &barrier;
                let token = *token;
                scope.spawn(move || {
                    barrier.wait();
                    service
                        .execute_control(&Request::Commit(CommitControl {
                            token,
                            force: false,
                            message: None,
                        }))
                        .map(|done| done.reply)
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>()
    });
    for token in tokens {
        fixture
            .service
            .execute_control(&Request::Unmount(UnmountControl {
                token: *token,
                discard: true,
            }))
            .unwrap();
    }
    replies
        .into_iter()
        .map(|reply| reply.unwrap_or_else(|failure| panic!("Commit failed: {failure:?}")))
        .collect()
}

fn read(fixture: &Fixture, path: &str) -> String {
    fixture
        .call(
            "fs.read",
            vec![
                ("branch", SdkValue::text(LANE)),
                ("path", SdkValue::text(path)),
            ],
        )
        .get_text("text")
        .unwrap()
        .to_owned()
}

#[test]
fn stale_workspace_commits_of_different_files_all_publish() {
    let fixture = fixture("f008-different");
    let tokens: Vec<_> = (0..4u8)
        .map(|i| {
            mounted_write(
                &fixture,
                60 + i,
                &format!("w{i}.txt"),
                format!("{i}\n").as_bytes(),
            )
        })
        .collect();
    let replies = commit_together(&fixture, &tokens);
    let published = replies
        .iter()
        .filter(|reply| matches!(reply, Reply::Committed(_) | Reply::MergeCommitted(_)))
        .count();
    assert_eq!(
        published,
        tokens.len(),
        "every Commit publishes: {replies:?}"
    );
    for i in 0..tokens.len() {
        assert_eq!(read(&fixture, &format!("/w{i}.txt")), format!("{i}\n"));
    }
    assert_eq!(read(&fixture, "/seed.txt"), "seed\n");
    fixture.cleanup();
}

#[test]
fn stale_workspace_commits_of_one_file_give_one_conflict() {
    let fixture = fixture("f008-same");
    let tokens = [
        mounted_write(&fixture, 70, "same.txt", b"alpha\n"),
        mounted_write(&fixture, 71, "same.txt", b"bravo\n"),
    ];
    let replies = commit_together(&fixture, &tokens);
    let published = replies
        .iter()
        .filter(|reply| matches!(reply, Reply::Committed(_) | Reply::MergeCommitted(_)))
        .count();
    let conflicts = replies
        .iter()
        .filter(|reply| matches!(reply, Reply::Conflict(_)))
        .count();
    assert_eq!((published, conflicts), (1, 1), "{replies:?}");
    let winner = if matches!(replies[0], Reply::Conflict(_)) {
        "bravo\n"
    } else {
        "alpha\n"
    };
    assert_eq!(read(&fixture, "/same.txt"), winner);
    fixture.cleanup();
}

#[test]
fn concurrent_fs_writes_of_different_files_all_publish() {
    let fixture = fixture("f008-fs");
    let count = 8;
    let barrier = Barrier::new(count);
    let kinds: Vec<String> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..count)
            .map(|i| {
                let fixture = &fixture;
                let barrier = &barrier;
                scope.spawn(move || {
                    barrier.wait();
                    let answer = fixture.call(
                        "fs.write",
                        vec![
                            ("branch", SdkValue::text(LANE)),
                            ("path", SdkValue::text(format!("/fs{i}.txt"))),
                            ("content", SdkValue::text(format!("fs {i}\n"))),
                        ],
                    );
                    answer.get_text("kind").unwrap_or("refused").to_owned()
                })
            })
            .collect();
        workers.into_iter().map(|w| w.join().unwrap()).collect()
    });
    assert!(kinds.iter().all(|kind| kind == "Committed"), "{kinds:?}");
    for i in 0..count {
        assert_eq!(read(&fixture, &format!("/fs{i}.txt")), format!("fs {i}\n"));
    }
    fixture.cleanup();
}
