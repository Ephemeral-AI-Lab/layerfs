//! Native Family 4 functional controls; no SDK/FUSE timing claim.
use super::*;
use layerfs_sdk::Project;
use std::collections::BTreeMap;

fn unhex<const N: usize>(text: &str) -> [u8; N] {
    assert_eq!(text.len(), N * 2);
    std::array::from_fn(|index| u8::from_str_radix(&text[index * 2..index * 2 + 2], 16).unwrap())
}
fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut text, "{byte:02x}").unwrap();
    }
    text
}
pub(super) fn open_prepared(extra: usize, deep: bool) -> Option<(Arc<Server>, Project, [u8; 17])> {
    let directory = std::env::var_os("LAYERFS_PHASE_B_CLONES")?;
    static CLONE: AtomicU64 = AtomicU64::new(0);
    let path =
        PathBuf::from(directory).join(format!("clone-{}", CLONE.fetch_add(1, Ordering::Relaxed)));
    let text = fs::read_to_string(path.join("fixture.before")).unwrap();
    let fields: BTreeMap<_, _> = text
        .lines()
        .map(|line| line.split_once('=').unwrap())
        .collect();
    assert_eq!(
        fields
            .get("extra")
            .copied()
            .unwrap_or("0")
            .parse::<usize>()
            .unwrap(),
        extra
    );
    assert_eq!(fields.get("deep").copied().unwrap_or("0") == "1", deep);
    let project = Project {
        id: unhex(fields["project_id"]),
        genesis_layer: unhex(fields["genesis_layer"]),
        root: unhex(fields["root"]),
        root_serial: fields["root_serial"].parse().unwrap(),
    };
    let server = Arc::new(
        Server::open(ServerConfig {
            store_path: path.join("store.sqlite"),
            history_path: path.join("history.sqlite"),
            binding_key: b"inherited-workspace".to_vec(),
            incarnation: 1,
            cursor_key: [53; 32],
            history: HistoryMode::OpenWritable,
            service_host: "127.0.0.1".into(),
            runtime: Runtime::disabled(),
            telemetry_run: None,
        })
        .unwrap(),
    );
    println!(
        "PHASE_B_FIXTURE clone={} root={} initialized=false",
        path.display(),
        fields["root"]
    );
    Some((server, project, unhex(fields["branch_id"])))
}

#[test]
#[ignore = "preparation only; requires an explicit fresh destination"]
fn prepare_master() {
    let path = PathBuf::from(std::env::var_os("LAYERFS_PHASE_B_PREPARE").unwrap());
    fs::create_dir(&path).unwrap();
    let large: usize = std::env::var("LAYERFS_PHASE_B_SIZE")
        .unwrap()
        .parse()
        .unwrap();
    let extra = std::env::var("LAYERFS_PHASE_B_EXTRA")
        .unwrap_or_else(|_| "0".into())
        .parse()
        .unwrap();
    let deep = std::env::var("LAYERFS_PHASE_B_DEEP").is_ok_and(|value| value == "1");
    let unrelated = std::env::var("LAYERFS_PHASE_B_UNRELATED")
        .unwrap_or_else(|_| "0".into())
        .parse()
        .unwrap();
    let (server, project, branch) = fresh_layout(&path, extra, deep, large, unrelated);
    drop(server);
    fs::write(
        path.join("fixture.before"),
        format!(
            "project_id={}\ngenesis_layer={}\nroot={}\nroot_serial={}\nbranch_id={}\nsize={}\nextra={extra}\ndeep={}\nunrelated={unrelated}\n",
            hex(&project.id),
            hex(&project.genesis_layer),
            hex(&project.root),
            project.root_serial,
            hex(&branch),
            large,
            u8::from(deep)
        ),
    )
    .unwrap();
    println!(
        "PHASE_B_PREPARED path={} bytes={large} closed=true",
        path.display()
    );
}

pub(super) fn fresh_project(
    path: &std::path::Path,
    extra: usize,
    deep: bool,
    large: usize,
) -> (Arc<Server>, Project, [u8; 17]) {
    fresh_layout(path, extra, deep, large, 0)
}

fn fresh_layout(
    path: &std::path::Path,
    extra: usize,
    deep: bool,
    large: usize,
    unrelated: usize,
) -> (Arc<Server>, Project, [u8; 17]) {
    let source = path.join("source");
    fs::create_dir_all(source.join("packages/old/subtree/child")).unwrap();
    fs::create_dir(source.join("packages/new")).unwrap();
    fs::write(
        source.join("packages/old/subtree/child/grand.txt"),
        b"grand-base",
    )
    .unwrap();
    fs::write(
        source.join("packages/old/subtree/sibling.txt"),
        b"sibling-base",
    )
    .unwrap();
    for index in 0..extra {
        fs::write(
            source.join(format!("packages/old/subtree/child/extra{index:03}.txt")),
            b"x",
        )
        .unwrap();
    }
    if deep {
        let mut path = source.join("src");
        for _ in 0..15 {
            path.push("d".repeat(250));
        }
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("leaf"), b"deep-base").unwrap();
    }
    if unrelated != 0 {
        fs::create_dir(source.join("unrelated")).unwrap();
        for index in 0..unrelated {
            fs::write(source.join(format!("unrelated/f{index:03}")), b"u").unwrap();
        }
    }
    if large != 0 {
        use std::io::Write;
        let block: Vec<u8> = (0..251 * 4096).map(|index| (index % 251) as u8).collect();
        let mut file = fs::File::create(source.join("data.bin")).unwrap();
        for chunk in (0..large).step_by(block.len()) {
            file.write_all(&block[..block.len().min(large - chunk)])
                .unwrap();
        }
    }
    let server = Arc::new(
        Server::create(ServerConfig {
            store_path: path.join("store.sqlite"),
            history_path: path.join("history.sqlite"),
            binding_key: b"inherited-workspace".to_vec(),
            incarnation: 1,
            cursor_key: [53; 32],
            history: HistoryMode::Create,
            service_host: "127.0.0.1".into(),
            runtime: Runtime::disabled(),
            telemetry_run: None,
        })
        .unwrap(),
    );
    let project = ProjectApi::new(&server).init("inherited", &source).unwrap();
    let branch = ProjectApi::new(&server)
        .fork(&project, [54; 16], "main")
        .unwrap();
    (server, project, branch.id)
}

fn write(f: &Fixture, handle: HandleId, offset: u64, bytes: &[u8]) {
    let payload = f
        .workspace
        .own_payload(bytes.len() as u64, &mut &bytes[..], deadline())
        .unwrap();
    f.workspace
        .write_file(handle, offset, &payload, deadline())
        .unwrap();
}

// The ordinary read/payload/write ordering is the same as native_workspace::edit.
fn splice(
    f: &Fixture,
    file: &NodeAttributes,
    handle: HandleId,
    start: u64,
    end: u64,
    bytes: &[u8],
) {
    let size = f.workspace.getattr(file.serial).unwrap().size;
    let removed = end - start;
    let inserted = bytes.len() as u64;
    let next = size - removed + inserted;
    const CHUNK: u64 = 64 * 1024;
    match inserted.cmp(&removed) {
        std::cmp::Ordering::Greater => {
            f.workspace.set_len(file.serial, next, deadline()).unwrap();
            let mut right = size;
            while right > end {
                let left = end.max(right.saturating_sub(CHUNK));
                let chunk = f
                    .workspace
                    .read(handle, left, (right - left) as usize, deadline())
                    .unwrap();
                write(f, handle, left + inserted - removed, chunk.as_ref());
                right = left;
            }
        }
        std::cmp::Ordering::Less => {
            let mut left = end;
            while left < size {
                let right = size.min(left + CHUNK);
                let chunk = f
                    .workspace
                    .read(handle, left, (right - left) as usize, deadline())
                    .unwrap();
                write(f, handle, left - removed + inserted, chunk.as_ref());
                left = right;
            }
            f.workspace.set_len(file.serial, next, deadline()).unwrap();
        }
        std::cmp::Ordering::Equal => {}
    }
    write(f, handle, start, bytes);
}

fn pinned(f: &Fixture, token: &[u8; 33], serial: u64, expected: &[u8]) {
    let mut offset = 0;
    while offset < expected.len() {
        let length = (128 * 1024).min(expected.len() - offset);
        let response = f
            .workspace
            .view_read(token, serial, offset as u64, length, deadline())
            .unwrap();
        assert_eq!(response.bytes, expected[offset..offset + length]);
        offset += length;
    }
    assert!(f
        .workspace
        .view_read(token, serial, offset as u64, 1, deadline())
        .unwrap()
        .bytes
        .is_empty());
}

#[test]
#[ignore = "requires Family4 sealed64 MiB prepared master and explicit runner"]
fn full_lowering_64mib() {
    let f = Fixture::new();
    let file = f.lookup(f.workspace.root().serial, b"data.bin");
    assert_eq!(file.size, 64 * 1024 * 1024);
    let handle = f.open(file.serial);
    let old: Vec<u8> = (0..file.size).map(|index| (index % 251) as u8).collect();
    assert_eq!(f.content_bytes(f.genesis, b"data.bin"), old);
    let old_token = [81; 33];
    let old_view = f.workspace.pin_view(old_token, deadline()).unwrap();
    let old_file = f
        .workspace
        .view_lookup(&old_token, old_view.root.serial, b"data.bin", deadline())
        .unwrap();
    let mut expected = old.clone();
    for (start, end, bytes) in [
        (100, 110, b"abc".as_slice()),
        (200, 200, b"12345"),
        (500, 700, b""),
    ] {
        splice(&f, &file, handle, start, end, bytes);
        expected.splice(start as usize..end as usize, bytes.iter().copied());
    }
    assert_eq!(expected.len(), 67_108_662);
    let g1_token = [82; 33];
    let g1_view = f.workspace.pin_view(g1_token, deadline()).unwrap();
    let g1_file = f
        .workspace
        .view_lookup(&g1_token, g1_view.root.serial, b"data.bin", deadline())
        .unwrap();
    let start = Instant::now();
    let stage = f.workspace.stage(deadline()).unwrap();
    let stage_ns = start.elapsed().as_nanos();
    assert!(stage_ns < 10_000_000_000);
    assert_eq!(f.saved_files.lock().unwrap().len(), 1);
    assert_eq!(f.saved_files.lock().unwrap()[0].1, 8);
    assert_eq!(
        f.content_bytes(stage.stage().candidate_root, b"data.bin"),
        expected
    );
    write(&f, handle, 32 * 1024 * 1024, b"LIVE");
    let report = f.workspace.commit_staged(&stage, deadline()).unwrap();
    let CommitOutcomeWire::Committed(first) = report.outcome else {
        panic!("known first Commit")
    };
    assert_eq!(first.root, stage.stage().candidate_root);
    assert_eq!(first.parent, stage.stage().expected_head);
    assert_eq!(f.content_bytes(first.root, b"data.bin"), expected);
    let mut g2 = expected.clone();
    g2[32 * 1024 * 1024..32 * 1024 * 1024 + 4].copy_from_slice(b"LIVE");
    assert_eq!(f.content_bytes(f.commit(), b"data.bin"), g2);
    pinned(&f, &old_token, old_file.serial, &old);
    pinned(&f, &g1_token, g1_file.serial, &expected);
    for token in [&old_token, &g1_token] {
        assert!(matches!(
            f.workspace.release_view(token).unwrap(),
            layerfs_workspace::ViewRelease::Completed
        ));
    }
    f.workspace.release(handle).unwrap();
    f.workspace
        .forget(file.serial, u64::MAX, ReferenceScope::Local);
    f.workspace.close_clean().unwrap();
    let backing = f.workspace.backing_status().unwrap();
    assert_eq!((backing.allocated_bytes, backing.reserved_bytes), (0, 0));
    println!("PHASE_B_LOWERING size={} replacement_bytes=8 stage_ns={stage_ns} old_g1_new_g2_pin_full=true cleanup=true", expected.len());
}

fn grand(f: &Fixture) -> (Vec<u64>, HandleId) {
    let mut parent = f.workspace.root().serial;
    let mut serials = Vec::new();
    for name in [
        b"packages".as_slice(),
        b"old",
        b"subtree",
        b"child",
        b"grand.txt",
    ] {
        parent = f.lookup(parent, name).serial;
        serials.push(parent);
    }
    (serials, f.open(parent))
}

fn occupy(f: &Fixture) -> layerfs_workspace::OwnedPayload {
    f.workspace.reclaim_payloads(deadline()).unwrap();
    let before = f.workspace.backing_status().unwrap();
    let available = before.quota_bytes - before.allocated_bytes - before.reserved_bytes;
    let mut blocks = available / 4096;
    while blocks + blocks.div_ceil(256) > available / 4096 {
        blocks -= 1;
    }
    let bytes = vec![0xcc; blocks as usize * 4096];
    let spare = f
        .workspace
        .own_payload(bytes.len() as u64, &mut bytes.as_slice(), deadline())
        .unwrap();
    let full = f.workspace.backing_status().unwrap();
    assert!(
        full.quota_bytes - full.allocated_bytes - full.reserved_bytes <= 4096,
        "before={before:?} full={full:?} blocks={blocks}"
    );
    spare
}

#[test]
fn stage_headroom_2mib() {
    let f = Fixture::with_layout_quota(0, false, 2 * 1024 * 1024);
    let (_serials, handle) = grand(&f);
    write(&f, handle, 1, b"AB");
    let spare = occupy(&f);
    let full = f.workspace.backing_status().unwrap();
    let start = Instant::now();
    let stage = f.workspace.stage(deadline()).unwrap();
    let stage_ns = start.elapsed().as_nanos();
    let after = f.workspace.backing_status().unwrap();
    assert!(after.allocated_bytes > full.allocated_bytes);
    assert!(after.reserved_bytes < full.reserved_bytes);
    assert_eq!(
        after.allocated_bytes + after.reserved_bytes,
        full.allocated_bytes + full.reserved_bytes
    );
    assert_eq!(
        f.content_bytes(
            stage.stage().candidate_root,
            b"packages/old/subtree/child/grand.txt"
        ),
        b"gABnd-base"
    );
    assert_eq!(
        f.content_bytes(f.genesis, b"packages/old/subtree/child/grand.txt"),
        b"grand-base"
    );
    assert_eq!(f.canonical_calls.load(Ordering::Acquire), 0);
    drop(stage);
    assert!(matches!(
        f.workspace.stage(deadline()),
        Err(WorkspaceError::Busy)
    ));
    assert!(matches!(
        f.workspace.close_clean(),
        Err(WorkspaceError::Busy)
    ));
    println!(
        "PHASE_B_HEADROOM stage_ns={stage_ns} full={full:?} after={after:?} spare={} retained=true",
        spare.len()
    );
    std::mem::forget(spare);
    std::mem::forget(f);
}

#[test]
fn commit_headroom_4mib() {
    let f = Fixture::with_layout_quota(0, false, 4 * 1024 * 1024);
    let (serials, handle) = grand(&f);
    write(&f, handle, 1, b"AB");
    let stage = f.workspace.stage(deadline()).unwrap();
    write(&f, handle, 7, b"XY");
    let spare = occupy(&f);
    let full = f.workspace.backing_status().unwrap();
    let start = Instant::now();
    let report = f.workspace.commit_staged(&stage, deadline()).unwrap();
    let commit_ns = start.elapsed().as_nanos();
    let CommitOutcomeWire::Committed(commit) = report.outcome else {
        panic!("known Commit")
    };
    assert_eq!(commit.root, stage.stage().candidate_root);
    assert_eq!(commit.parent, stage.stage().expected_head);
    assert_eq!(
        f.content_bytes(commit.root, b"packages/old/subtree/child/grand.txt"),
        b"gABnd-base"
    );
    assert_eq!(
        f.workspace
            .read(handle, 0, 10, deadline())
            .unwrap()
            .as_ref(),
        b"gABnd-bXYe"
    );
    let after = f.workspace.backing_status().unwrap();
    assert!(
        after.allocated_bytes + after.reserved_bytes <= full.allocated_bytes + full.reserved_bytes
    );
    drop(spare);
    f.workspace.reclaim_payloads(deadline()).unwrap();
    assert_eq!(
        f.content_bytes(f.commit(), b"packages/old/subtree/child/grand.txt"),
        b"gABnd-bXYe"
    );
    f.workspace.release(handle).unwrap();
    for serial in serials {
        f.workspace.forget(serial, u64::MAX, ReferenceScope::Local);
    }
    f.workspace.close_clean().unwrap();
    assert_eq!(f.workspace.backing_status().unwrap().allocated_bytes, 0);
    println!("PHASE_B_HEADROOM commit_ns={commit_ns} full={full:?} after={after:?} cleanup=true");
}

fn separated_count(count: usize) {
    let f = Fixture::new();
    let file = f.lookup(f.workspace.root().serial, b"data.bin");
    assert_eq!(file.size, 327_680);
    let handle = f.open(file.serial);
    let old: Vec<u8> = (0..file.size).map(|index| (index % 251) as u8).collect();
    assert_eq!(f.content_bytes(f.genesis, b"data.bin"), old);
    let mut expected = old.clone();
    let token = [83; 33];
    let view = f.workspace.pin_view(token, deadline()).unwrap();
    let selected = f
        .workspace
        .view_lookup(&token, view.root.serial, b"data.bin", deadline())
        .unwrap();
    for index in 0..count {
        write(&f, handle, (index * 2) as u64, b"X");
        expected[index * 2] = b'X';
    }
    let start = Instant::now();
    let result = f.workspace.commit(deadline());
    let commit_ns = start.elapsed().as_nanos();
    match result {
        Ok(report) => {
            let CommitOutcomeWire::Committed(commit) = report.outcome else {
                panic!("known count Commit")
            };
            assert_eq!(f.canonical_calls.load(Ordering::Acquire), 1);
            assert_eq!(f.content_bytes(commit.root, b"data.bin"), expected);
            pinned(&f, &token, selected.serial, &old);
            assert!(matches!(
                f.workspace.release_view(&token).unwrap(),
                layerfs_workspace::ViewRelease::Completed
            ));
            f.workspace.release(handle).unwrap();
            f.workspace
                .forget(file.serial, u64::MAX, ReferenceScope::Local);
            f.workspace.close_clean().unwrap();
            let backing = f.workspace.backing_status().unwrap();
            assert_eq!((backing.allocated_bytes, backing.reserved_bytes), (0, 0));
            println!("PHASE_B_COUNT writes={count} memory_budget={} commit_ns={commit_ns} known=true full_old_new_pin=true retained=false cleanup=true", DEFAULT_MEMORY_BUDGET_BYTES);
        }
        Err(WorkspaceError::Commit(failure)) if count == 10240 => {
            assert_eq!(
                failure.disposition,
                layerfs_workspace::CommitFailureDisposition::KnownCommitLocalFailure
            );
            let Some(CommitOutcomeWire::Committed(ref commit)) = failure.known_outcome else {
                panic!("known canonical root")
            };
            assert!(failure.installed_revision.is_none());
            assert_eq!(f.content_bytes(commit.root, b"data.bin"), expected);
            pinned(&f, &token, selected.serial, &old);
            assert_eq!(f.canonical_calls.load(Ordering::Acquire), 1);
            assert!(f.workspace.status().unwrap().submission.is_some());
            assert!(matches!(
                f.workspace.commit(deadline()),
                Err(WorkspaceError::Busy)
            ));
            assert!(matches!(
                f.workspace.close_clean(),
                Err(WorkspaceError::Busy)
            ));
            let backing = f.workspace.backing_status().unwrap();
            assert!(
                backing.accounting_complete
                    && backing.allocated_bytes > 0
                    && backing.reserved_bytes > 0
            );
            println!("PHASE_B_COUNT writes={count} memory_budget={} commit_ns={commit_ns} known_canonical_local_failure=true full_old_new_pin=true retained=true backing={backing:?}", DEFAULT_MEMORY_BUDGET_BYTES);
            std::mem::forget(f);
        }
        other => panic!("unexpected count outcome: {other:?}"),
    }
}

#[test]
#[ignore = "requires Family4 sealed count master and explicit runner"]
fn native_count_8192() {
    separated_count(8192);
}

#[test]
#[ignore = "owner deferred further10240 attempts and architecture work to issue276"]
fn native_count_10240() {
    separated_count(10240);
}
