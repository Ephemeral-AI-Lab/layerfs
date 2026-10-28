//! Owner clarification: one Commit per Workspace, concurrent Commit producers
//! on separate Workspaces, independent captures/roots and short Store writer
//! admission. A real external flock stages before-effect admission; every
//! process and observation has an independent finite bound. Both Workspaces
//! bind one Branch head. Since #352 F-008 each Commit reads the head, merges
//! and publishes inside the Store's publication lane, so the second merges once
//! with the first's Commit and both publish (no HeadMoved, no second merge).
#![cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "support/installed_store.rs"]
mod installed;
#[allow(dead_code)]
#[path = "support/namespace_model.rs"]
mod model;
#[allow(dead_code)]
#[path = "support/mounted.rs"]
mod mounted;
#[allow(dead_code)]
#[path = "support/mounted_commit.rs"]
mod rig;
use layerfs_bridge::control::{Activity, Reply};
use layerfs_daemon::{control::Failure, store::CommitError};
use layerfs_history::{CommitOutcome, HistoryError};
use mounted::COMMAND;
use rig::{bash_in, host, passed, root, Rig};
use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

struct AdmissionHolder(Child);
impl AdmissionHolder {
    fn acquire(path: &std::path::Path) -> Self {
        let body="import fcntl,sys,select,os\nf=os.open(sys.argv[1],os.O_RDONLY|os.O_DIRECTORY); fcntl.flock(f,fcntl.LOCK_EX); print('held',flush=True)\nselect.select([sys.stdin],[],[],3)\nfcntl.flock(f,fcntl.LOCK_UN); os.close(f)\n";
        let mut held = Self(
            Command::new("python3")
                .args(["-c", body])
                .arg(path)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()
                .unwrap(),
        );
        let mut output = BufReader::new(held.0.stdout.take().unwrap());
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let mut line = String::new();
            let _ = tx.send(output.read_line(&mut line).map(|_| line));
        });
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(1)).unwrap().unwrap(),
            "held\n"
        );
        held
    }
    fn release(mut self) {
        self.0
            .stdin
            .take()
            .unwrap()
            .write_all(b"release\n")
            .unwrap();
        assert!(self.0.wait().unwrap().success());
    }
}
impl Drop for AdmissionHolder {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(None)) {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

#[test]
fn parallel_workspace_commits_keep_distinct_captures_and_one_admission_each() {
    let rig = Rig::new("issue319-parallel-commits");
    let a = rig.mount(1);
    let b = rig.mount(2);
    passed(
        &bash_in(COMMAND, root(&a), "timeout 2 sh -c 'printf alpha > only-a'"),
        "create A",
    );
    passed(
        &bash_in(COMMAND, root(&b), "timeout 2 sh -c 'printf bravo > only-b'"),
        "create B",
    );
    let ea = host(root(&a));
    let eb = host(root(&b));
    let holder = AdmissionHolder::acquire(rig.fixture.config.path.parent().unwrap());
    let (tx, rx) = mpsc::channel();
    let (ready_tx, ready_rx) = mpsc::channel();
    let mut starters = Vec::new();
    let mut threads = Vec::new();
    for (tag, token) in [(1, a.token), (2, b.token)] {
        let service = rig.harness.service.clone();
        let tx = tx.clone();
        let ready = ready_tx.clone();
        let (start, started) = mpsc::channel();
        starters.push(start);
        threads.push(thread::spawn(move || {
            if ready.send(tag).is_err() || started.recv_timeout(Duration::from_secs(1)).is_err() {
                return;
            }
            let result = service.execute_control(&layerfs_bridge::control::Request::Commit(
                layerfs_bridge::control::CommitControl {
                    token,
                    force: false,
                    message: None,
                },
            ));
            let _ = tx.send((tag, result));
        }));
    }
    drop(tx);
    drop(ready_tx);
    for _ in 0..2 {
        ready_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("bounded worker readiness");
    }
    for start in starters {
        start.send(()).expect("ready worker still awaiting start");
    }
    let stop = Instant::now() + Duration::from_secs(1);
    let both_in_flight = loop {
        let aa = rig.harness.status(a.token).activity == Activity::Committing;
        let bb = rig.harness.status(b.token).activity == Activity::Committing;
        if aa && bb {
            break true;
        }
        if Instant::now() >= stop {
            break false;
        }
        thread::yield_now();
    };
    let duplicate = if both_in_flight {
        Some(rig.commit(a.token))
    } else {
        None
    };
    let duplicate_refused = matches!(
        &duplicate,
        Some(Err(Failure::Rejected(
            layerfs_bridge::control::ControlCode::Busy,
            _
        )))
    ) || matches!(&duplicate,Some(Err(Failure::Commit(f))) if matches!(f.error,CommitError::InFlight));
    let early = rx.recv_timeout(Duration::from_millis(50)).ok();
    let acknowledged_before_release = early.is_some();
    holder.release();
    let mut results = Vec::new();
    if let Some(row) = early {
        results.push(row);
    }
    while results.len() < 2 {
        results.push(
            rx.recv_timeout(Duration::from_secs(20))
                .expect("bounded original Commit completion"),
        );
    }
    for thread in threads {
        thread.join().unwrap();
    }
    let mut roots = Vec::new();
    let mut moved = 0;
    let mut outcomes = Vec::new();
    for (tag, result) in results {
        outcomes.push(format!("workspace={tag} result={result:?}"));
        if let Err(Failure::Commit(failure)) = &result {
            if matches!(
                failure.error,
                CommitError::History(HistoryError::HeadMoved(_))
            ) && failure.published.is_none()
            {
                moved += 1;
            }
        }
        if let Ok(done) = result {
            let outcome = match done.reply {
                Reply::Committed(outcome) => Some((outcome, false)),
                Reply::MergeCommitted(merged) => Some((merged.outcome, true)),
                _ => None,
            };
            if let Some((CommitOutcome::Committed { record, .. }, merged)) = outcome {
                let operation = rig
                    .harness
                    .service
                    .operation(if tag == 1 { a.token } else { b.token })
                    .unwrap();
                let actual = model::canonical(operation.client(), record.root);
                roots.push((tag, merged, actual));
            }
        }
    }
    let live_a = host(root(&a));
    let live_b = host(root(&b));
    rig.unmount(&a);
    rig.unmount(&b);
    rig.finish();
    println!("PARALLEL_COMMIT both_in_flight={both_in_flight} duplicate_refused={duplicate_refused} roots={} outcomes={outcomes:?}",roots.len());
    assert!(
        !acknowledged_before_release,
        "no publication before writer admission release"
    );
    assert!(
        both_in_flight,
        "both public Commits overlap while awaiting Store admission"
    );
    assert!(duplicate_refused, "one ongoing Commit per Workspace");
    // Both bound one head; the lane orders their publications, and the
    // second merges once with the first's Commit.
    assert_eq!(
        (roots.len(), moved),
        (2, 0),
        "both Commits publish; none meets a moved head: {outcomes:?}"
    );
    let file = |flat: &model::Flat, name: &[u8]| {
        flat.iter()
            .find(|(path, _)| path.ends_with(name))
            .map(|(_, seen)| (seen.kind, seen.payload.clone()))
    };
    for (tag, merged, actual) in &roots {
        if *merged {
            for (name, expected) in [(&b"only-a"[..], &ea), (&b"only-b"[..], &eb)] {
                assert_eq!(
                    file(actual, name),
                    file(expected, name),
                    "merged tree keeps both files"
                );
                assert!(file(actual, name).is_some());
            }
        } else {
            model::assert_same(
                if *tag == 1 { &ea } else { &eb },
                actual,
                "independent exact captured tree",
            );
        }
    }
    assert_eq!(roots.iter().filter(|(_, merged, _)| *merged).count(), 1);
    // The first keeps its exact tree; the merged one installs the merge.
    for (tag, merged, actual) in &roots {
        let live = if *tag == 1 { &live_a } else { &live_b };
        if *merged {
            assert_eq!(file(live, b"only-a"), file(actual, b"only-a"));
            assert_eq!(file(live, b"only-b"), file(actual, b"only-b"));
        } else {
            model::assert_same(if *tag == 1 { &ea } else { &eb }, live, "live install");
        }
    }
}
