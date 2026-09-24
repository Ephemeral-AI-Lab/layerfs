//! #241 functional positions. Live sweep is opt-in and never a performance sample.
use layerfs_content::{
    file::mapping::{build_streaming, decode_chunk_payload, emit_file_state},
    object::{decode_bytes_object, FinalizedConsumer, FinalizedObject, ObjectRole},
    policy::ConstructionPolicy,
    ContentResult,
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::File,
    io::{Cursor, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

#[path = "support/range_exec_gate.rs"]
mod range_exec_gate;

const MANIFEST: &str = include_str!("../../../../docs/issues/241/position-manifest-v1.tsv");
const MANIFEST_SHA256: &str = "e12509411e225e6eea497dc08e3cc0730f1c2ab7ce3f104cfc232eb16c7f2b6a";
const PAYLOAD_SHA256: &str = "568c5408a3f292d4a593d5ffa43736b790b6a5dac749427b0ad53c765e672616";

const FIXTURES: [(&str, u64, &str, &str, u64); 4] = [
    (
        "1mib",
        1_048_576,
        "d7dfe3d2828aceb85177e6efbeb600f23672a326c902e525e401c1545bb05bdc",
        "8fafdf06fac9dbdffb7ccb6b1bde3b2460c387ef1abc55717dee8be401ff6078",
        54,
    ),
    (
        "10mib",
        10_485_760,
        "29c89128c748e4404f31b0147d447bd524d7b75afc98d56ac4debac762ee4b79",
        "dd79a6666e83927d787c8a7679b06f4c98ca5f80b6abd48d94b5e8f84aad1c85",
        544,
    ),
    (
        "100mib",
        104_857_600,
        "1bb2d79d54f72ae15eb0bb76ad715b9aafeba8ff8f9aa4f47bad3e3f101885bd",
        "bbee7155df021324495d88954be4db125eca49442b50aadc16439f61f6c32efe",
        5394,
    ),
    (
        "500mib-capped",
        524_283_904,
        "f1b6c61d9c126beba89dd2a310f727fd63cbbf131b793a78fe21247238c98c1f",
        "6c74b4ba6ad67f352a0bd85879a2f16a77511286bf9a73883d5c8858d2eded8f",
        26_994,
    ),
];

struct BoundarySink {
    cursor: u64,
    middle: u64,
    best: Option<u64>,
    size: u64,
}

impl FinalizedConsumer for BoundarySink {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        if object.role() == ObjectRole::Chunk {
            self.cursor +=
                decode_chunk_payload(decode_bytes_object(object.canonical())?)?.len() as u64;
            if self.cursor < self.size
                && self.best.is_none_or(|old| {
                    (self.cursor.abs_diff(self.middle), self.cursor)
                        < (old.abs_diff(self.middle), old)
                })
            {
                self.best = Some(self.cursor);
            }
        }
        Ok(())
    }
}

fn sha256(path: &Path) -> String {
    let mut source = File::open(path).unwrap();
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 1 << 20];
    loop {
        let read = source.read(&mut buffer).unwrap();
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    format!("{:x}", digest.finalize())
}

struct HashingReader<R> {
    inner: R,
    hash: Sha256,
}

impl<R: Read> Read for HashingReader<R> {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        let read = self.inner.read(bytes)?;
        self.hash.update(&bytes[..read]);
        Ok(read)
    }
}

#[derive(Clone)]
struct Case {
    id: String,
    label: String,
    size: u64,
    op: String,
    offset: u64,
    delete_len: u64,
    insert_len: u64,
    payload: String,
}

fn cases() -> Vec<Case> {
    assert_eq!(
        format!("{:x}", Sha256::digest(MANIFEST.as_bytes())),
        MANIFEST_SHA256
    );
    let mut lines = MANIFEST.lines();
    assert_eq!(lines.next(), Some("case_id\tsize_label\tpristine_bytes\top\tkind\toffset\tdelete_len\tinsert_len\tpayload"));
    let rows: Vec<_> = lines
        .map(|line| {
            let fields: Vec<_> = line.split('\t').collect();
            assert_eq!(fields.len(), 9);
            let row = Case {
                id: fields[0].into(),
                label: fields[1].into(),
                size: fields[2].parse().unwrap(),
                op: fields[3].into(),
                offset: fields[5].parse().unwrap(),
                delete_len: fields[6].parse().unwrap(),
                insert_len: fields[7].parse().unwrap(),
                payload: fields[8].into(),
            };
            assert!(row.offset + row.delete_len <= row.size);
            assert!(row.insert_len <= 4096);
            row
        })
        .collect();
    assert_eq!(rows.len(), 264);
    rows
}

fn replacement() -> Vec<u8> {
    let mut value = 6_313_238_748_831_594_097_u64;
    let mut bytes = Vec::with_capacity(4096);
    for _ in 0..512 {
        value = value.wrapping_add(0x9e3779b97f4a7c15);
        let mut word = value;
        word = (word ^ (word >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        word = (word ^ (word >> 27)).wrapping_mul(0x94d049bb133111eb);
        bytes.extend_from_slice(&(word ^ (word >> 31)).to_le_bytes());
    }
    assert_eq!(format!("{:x}", Sha256::digest(&bytes)), PAYLOAD_SHA256);
    bytes
}

fn expected_digest(source: &Path, case: &Case, payload: &[u8]) -> String {
    let prefix = File::open(source).unwrap().take(case.offset);
    let mut suffix = File::open(source).unwrap();
    suffix
        .seek(SeekFrom::Start(case.offset + case.delete_len))
        .unwrap();
    let mut reader = HashingReader {
        inner: prefix.chain(Cursor::new(payload)).chain(suffix),
        hash: Sha256::new(),
    };
    let bytes = std::io::copy(&mut reader, &mut std::io::sink()).unwrap();
    assert_eq!(bytes, case.size - case.delete_len + case.insert_len);
    format!("{:x}", reader.hash.finalize())
}

fn append_source(bytes: &mut Vec<u8>, source: &Path, offset: u64, take: u64) {
    let mut file = File::open(source).unwrap();
    file.seek(SeekFrom::Start(offset)).unwrap();
    let mut part = vec![0; take as usize];
    file.read_exact(&mut part).unwrap();
    bytes.extend_from_slice(&part);
}

fn expected_window(source: &Path, case: Option<(&Case, &[u8])>, start: u64, len: u64) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(len as usize);
    let Some((case, payload)) = case else {
        append_source(&mut bytes, source, start, len);
        return bytes;
    };
    let end = start + len;
    let prefix_end = end.min(case.offset);
    if start < prefix_end {
        append_source(&mut bytes, source, start, prefix_end - start);
    }
    let inserted_end = case.offset + case.insert_len;
    let middle_start = start.max(case.offset);
    let middle_end = end.min(inserted_end);
    if middle_start < middle_end {
        bytes.extend_from_slice(
            &payload[(middle_start - case.offset) as usize..(middle_end - case.offset) as usize],
        );
    }
    let suffix_start = start.max(inserted_end);
    if suffix_start < end {
        append_source(
            &mut bytes,
            source,
            suffix_start + case.delete_len - case.insert_len,
            end - suffix_start,
        );
    }
    assert_eq!(bytes.len(), len as usize);
    bytes
}

fn verify_mounted_windows(
    api: &layerfs_sdk::WorkspaceApi<'_>,
    id: &layerfs_api_core::WorkspaceId,
    source: &Path,
    case: &Case,
    payload: Option<&[u8]>,
    expect_baseline: bool,
) -> Result<(), String> {
    let size = if payload.is_some() {
        case.size - case.delete_len + case.insert_len
    } else {
        case.size
    };
    let windows = [0, case.offset.saturating_sub(16), size.saturating_sub(4096)]
        .map(|start| (start.min(size - 1), (size - start.min(size - 1)).min(4096)));
    let command = mounted_window_command(size, windows, expect_baseline);
    let result = api
        .exec(id, &command)
        .map_err(|error| format!("bounded mounted read: {error:?}"))?;
    if result.exit_status != Some(0) || result.stdout_truncated || result.stderr_truncated {
        return Err(format!("bounded mounted read status: {result:?}"));
    }
    let output = String::from_utf8(result.stdout).map_err(|error| error.to_string())?;
    let lines: Vec<_> = output.lines().collect();
    if lines.len() != 5
        || lines[0].trim() != size.to_string()
        || lines[4].trim() != (size % 4096).to_string()
    {
        return Err(format!("mounted size/EOF: {output:?}"));
    }
    for (index, (start, len)) in windows.into_iter().enumerate() {
        let data = expected_window(source, payload.map(|bytes| (case, bytes)), start, len);
        let expected = format!("{:x}", Sha256::digest(&data));
        if lines[index + 1].split_whitespace().next() != Some(expected.as_str()) {
            return Err(format!("mounted window {index} @{start}+{len}: {output:?}"));
        }
    }
    Ok(())
}

fn mounted_window_command(size: u64, windows: [(u64, u64); 3], expect_baseline: bool) -> String {
    let mut command = String::from(
        "set -o pipefail || exit 1; set -e; tmp=$(mktemp /tmp/layerfs-position.XXXXXX); trap 'rm -f \"$tmp\"' 0; stat -c %s payload.bin",
    );
    for (start, len) in windows {
        let block = start / 4096;
        let within = start % 4096;
        let blocks = (within + len).div_ceil(4096);
        assert!(blocks <= 2);
        command.push_str(&format!(
            "; dd if=payload.bin of=\"$tmp\" bs=4096 skip={block} count={blocks} 2>/dev/null; dd if=\"$tmp\" bs=1 skip={within} count={len} 2>/dev/null | sha256sum"
        ));
    }
    command.push_str(&format!(
        "; dd if=payload.bin of=\"$tmp\" bs=4096 skip={} count=1 2>/dev/null; wc -c < \"$tmp\"",
        size / 4096
    ));
    if expect_baseline {
        command.push_str("; marker=$(cat .position-baseline); test \"$marker\" = baseline; test \"$(wc -c < .position-baseline)\" -eq 8");
    } else {
        command.push_str("; test ! -e .position-baseline; test ! -L .position-baseline");
    }
    command
}

fn with_mount<T>(
    api: &layerfs_sdk::WorkspaceApi<'_>,
    sandbox: layerfs_api_core::SandboxId,
    project: &layerfs_api_core::Project,
    branch: [u8; 17],
    commit: Option<[u8; 33]>,
    on_unmount: impl FnOnce(&Result<(), String>),
    f: impl FnOnce(&layerfs_api_core::WorkspaceId) -> Result<T, String>,
) -> Result<T, String> {
    let mount = match api.mount(sandbox, project, branch, commit) {
        Ok(mount) => mount,
        Err(error) => {
            if let layerfs_api_core::WorkspaceError::UncertainMount { id, .. }
            | layerfs_api_core::WorkspaceError::Retained { id, .. } = &error
            {
                let unmount = api
                    .unmount(id)
                    .map_err(|cleanup| format!("unmount: {cleanup:?}"));
                on_unmount(&unmount);
                return Err(format!("mount: {error:?}; retained cleanup: {unmount:?}"));
            }
            return Err(format!("mount: {error:?}"));
        }
    };
    let result = f(&mount.id);
    let unmount = api
        .unmount(&mount.id)
        .map_err(|error| format!("unmount: {error:?}"));
    on_unmount(&unmount);
    match (result, unmount) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), Ok(())) | (Ok(_), Err(error)) => Err(error),
        (Err(primary), Err(cleanup)) => Err(format!("primary={primary}; cleanup={cleanup}")),
    }
}

fn docker_snapshot(output: &Path, stem: &str, args: &[&str], receipt: &mut File) {
    match Command::new("docker").args(args).output() {
        Ok(observed) => {
            File::create_new(output.join(format!("{stem}.stdout")))
                .unwrap()
                .write_all(&observed.stdout)
                .unwrap();
            File::create_new(output.join(format!("{stem}.stderr")))
                .unwrap()
                .write_all(&observed.stderr)
                .unwrap();
            writeln!(receipt, "{stem}_exit\t{:?}", observed.status).unwrap();
        }
        Err(error) => writeln!(receipt, "{stem}_error\t{error:?}").unwrap(),
    }
}

fn observe_failed_sandbox(
    output: &Path,
    case: &Case,
    stage: &str,
    sandbox: layerfs_api_core::SandboxId,
    receipt: &mut File,
) {
    let container = format!("layerfs-{sandbox}");
    let prefix = format!("{}-{stage}", case.id);
    docker_snapshot(
        output,
        &format!("{prefix}-docker-inspect"),
        // Full inspect includes ephemeral control keys; State has the OOM facts.
        &["inspect", "--format", "{{json .State}}", &container],
        receipt,
    );
    docker_snapshot(
        output,
        &format!("{prefix}-docker-top"),
        &["top", &container],
        receipt,
    );
    // Keep the original daemon log filenames as the first snapshot.
    docker_snapshot(
        output,
        &format!("{prefix}-daemon"),
        &["logs", &container],
        receipt,
    );
    let observation = Instant::now();
    std::thread::sleep(Duration::from_secs(6));
    writeln!(
        receipt,
        "{prefix}_observation_wait_ns\t{}",
        observation.elapsed().as_nanos()
    )
    .unwrap();
    docker_snapshot(
        output,
        &format!("{prefix}-docker-top-late"),
        &["top", &container],
        receipt,
    );
    docker_snapshot(
        output,
        &format!("{prefix}-daemon-late"),
        &["logs", &container],
        receipt,
    );
}

fn with_fresh_sandbox_mount<T>(
    context: &SweepContext<'_>,
    stage: &str,
    case: &Case,
    branch: [u8; 17],
    commit: Option<[u8; 33]>,
    receipt: &mut File,
    f: impl FnOnce(&layerfs_api_core::WorkspaceId) -> Result<T, String>,
) -> Result<T, String> {
    let sandbox = match context
        .sandboxes
        .create(context.image, &format!("{}-{stage}", case.id))
    {
        Ok(id) => id,
        Err(error) => {
            if let Some(id) = error.sandbox {
                observe_failed_sandbox(context.output, case, stage, id, receipt);
                writeln!(
                    receipt,
                    "{stage}_retained_sandbox_delete\t{:?}",
                    context.sandboxes.delete(id)
                )
                .unwrap();
            }
            return Err(format!("{stage} sandbox create: {error:?}"));
        }
    };
    writeln!(receipt, "{stage}_sandbox_id\t{sandbox}").unwrap();
    let result = with_mount(
        context.api,
        sandbox,
        context.project,
        branch,
        commit,
        |unmount| {
            writeln!(receipt, "{stage}_unmount\t{unmount:?}").unwrap();
        },
        f,
    );
    if result.is_err() {
        observe_failed_sandbox(context.output, case, stage, sandbox, receipt);
    }
    let deleted = context.sandboxes.delete(sandbox);
    let listed = context.sandboxes.list();
    let absent = listed
        .as_ref()
        .is_ok_and(|entries| entries.iter().all(|entry| entry.id != sandbox));
    writeln!(receipt, "{stage}_sandbox_delete\t{deleted:?}").unwrap();
    writeln!(receipt, "{stage}_sandbox_absent\t{absent}").unwrap();
    match (result, deleted.is_ok() && absent) {
        (Ok(value), true) => Ok(value),
        (Err(primary), true) => Err(format!("{stage}: {primary}")),
        (Ok(_), false) => Err(format!(
            "{stage} sandbox cleanup: delete={deleted:?} list={listed:?}"
        )),
        (Err(primary), false) => Err(format!(
            "{stage}: {primary}; sandbox cleanup: delete={deleted:?} list={listed:?}"
        )),
    }
}

fn committed(
    report: layerfs_bridge::contract::WorkspaceCommitReportWire,
) -> Result<layerfs_bridge::contract::CommitWire, String> {
    match report.outcome {
        layerfs_bridge::contract::CommitOutcomeWire::Committed(record) => Ok(record),
        other => Err(format!("expected new Commit: {other:?}")),
    }
}

fn read_master_spec(path: &Path) -> BTreeMap<String, String> {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| {
            let (key, value) = line.split_once('\t').expect("master spec field");
            (key.to_owned(), value.to_owned())
        })
        .collect()
}

fn hex_array<const N: usize>(text: &str) -> [u8; N] {
    assert_eq!(text.len(), N * 2);
    std::array::from_fn(|index| u8::from_str_radix(&text[index * 2..index * 2 + 2], 16).unwrap())
}

type BranchState = (Option<[u8; 33]>, [u8; 33], [u8; 32]);

fn public_branch_state(
    history_path: &Path,
    cursor_key: [u8; 32],
    branch: [u8; 17],
) -> Result<BranchState, String> {
    use layerfs_history::HistoryCatalog;
    let catalog =
        layerfs_history::sqlite::open_read_only(history_path, b"layerfs-bench-pro", cursor_key)
            .map_err(|error| format!("history open: {error:?}"))?;
    let id = layerfs_history::BranchId::from_bytes(branch)
        .map_err(|error| format!("branch id: {error:?}"))?;
    catalog
        .branch_snapshot(id)
        .map_err(|error| format!("branch read: {error:?}"))?
        .map(|snapshot| {
            (
                snapshot.branch.head_commit.map(|head| head.to_bytes()),
                snapshot.branch.base_layer.to_bytes(),
                snapshot.effective_root.to_bytes(),
            )
        })
        .ok_or_else(|| "missing branch".into())
}

struct DigestWriter {
    hash: Sha256,
    bytes: u64,
}

impl Write for DigestWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.hash.update(bytes);
        self.bytes += bytes.len() as u64;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn stored_oracle(
    store_path: &Path,
    filesystem_root: [u8; 32],
    full_digest: bool,
) -> Result<(String, u64, u64, Option<String>), String> {
    use layerfs_content::{
        file::{FileContent, FileView},
        filesystem::{read::FilesystemRead, root::FilesystemRootId},
        read_all_bounded, LogicalPath, ObjectId,
    };
    use layerfs_storage::{Store, StoreProvider};
    use layerfs_telemetry::timer::Timing;
    let store = Timing::disabled("store.open", |scope| {
        Store::open(store_path, scope.child("store"))
    })
    .0
    .map_err(|error| format!("store open: {error:?}"))?;
    let provider = StoreProvider::new(&store);
    let root =
        ObjectId::from_bytes(&filesystem_root).map_err(|error| format!("root: {error:?}"))?;
    let mut filesystem = FilesystemRead::new(&provider, FilesystemRootId(root))
        .map_err(|error| format!("filesystem: {error:?}"))?;
    let stat = filesystem
        .stat(&LogicalPath::new("payload.bin").unwrap())
        .map_err(|error| format!("stat: {error:?}"))?;
    let view = Timing::disabled("file.open", |scope| {
        FileView::open(&provider, stat.content_root, scope.child("file"))
    })
    .0
    .map_err(|error| format!("file view: {error:?}"))?;
    let FileContent::Chunked(state) = view.content() else {
        return Err("stored file is not chunked".into());
    };
    let digest = if full_digest {
        let mut sink = DigestWriter {
            hash: Sha256::new(),
            bytes: 0,
        };
        Timing::disabled("file.digest", |scope| {
            read_all_bounded(
                &provider,
                stat.content_root,
                u64::MAX,
                &mut sink,
                scope.child("file"),
            )
        })
        .0
        .map_err(|error| format!("stored bytes: {error:?}"))?;
        if sink.bytes != state.logical_len {
            return Err(format!(
                "stored bytes {} != logical length {}",
                sink.bytes, state.logical_len
            ));
        }
        Some(format!("{:x}", sink.hash.finalize()))
    } else {
        None
    };
    Ok((
        stat.content_root.to_string(),
        state.extent_count,
        state.logical_len,
        digest,
    ))
}

struct SweepContext<'a> {
    source: &'a Path,
    payload: &'a [u8],
    image: &'a str,
    output: &'a Path,
    sandboxes: &'a layerfs_sdk::SandboxApi<'a>,
    project: &'a layerfs_api_core::Project,
    projects: &'a layerfs_sdk::ProjectApi<'a>,
    api: &'a layerfs_sdk::WorkspaceApi<'a>,
    store_path: &'a Path,
    history_path: &'a Path,
    cursor_key: [u8; 32],
    source_branch: [u8; 17],
}

fn run_case(
    case: &Case,
    index: usize,
    context: &SweepContext<'_>,
    sandbox: layerfs_api_core::SandboxId,
    receipt: &mut File,
) -> Result<String, String> {
    let mut body = [0_u8; 16];
    body[..8].copy_from_slice(&((index + 2) as u64).to_be_bytes());
    let branch = context
        .projects
        .fork(context.project, body, &case.id)
        .map_err(|error| format!("fork: {error:?}"))?;
    let inserted = if case.insert_len == 0 {
        &[][..]
    } else {
        context.payload
    };
    let expected_sha = expected_digest(context.source, case, inserted);
    let expected_size = case.size - case.delete_len + case.insert_len;
    let mut baseline_observation = None;
    let mounted = with_mount(
        context.api,
        sandbox,
        context.project,
        branch.id,
        None,
        |unmount| {
            writeln!(receipt, "edit_unmount\t{unmount:?}").unwrap();
        },
        |id| {
            let started = Instant::now();
            let initial = context.api.exec(id, "printf baseline > .position-baseline");
            baseline_observation = Some((started.elapsed().as_nanos(), format!("{initial:?}")));
            let initial = initial.map_err(|error| format!("baseline exec: {error:?}"))?;
            if initial.exit_status != Some(0) {
                return Err(format!("baseline status: {initial:?}"));
            }
            let old = committed(
                context
                    .api
                    .commit(id)
                    .map_err(|error| format!("baseline commit: {error:?}"))?,
            )?;
            let before = context
                .api
                .status(id)
                .map_err(|error| format!("pre-splice status: {error:?}"))?;
            let mut command = format!(
            "/layerfs-bench/bin/layerfs-edit-tool splice --file payload.bin --expect-size {} --offset {} --delete-length {} --length {}",
            case.size, case.offset, case.delete_len, case.insert_len
        );
            if case.insert_len != 0 {
                command.push_str(" --payload ");
                command.push_str(&case.payload);
            }
            let (exec, report) = range_exec_gate::exec_and_commit_on_success(
                context.api,
                id,
                &command,
                expected_size,
            )
            .map_err(|error| format!("splice Exec/Commit: {error:?}"))?;
            let new = committed(
                report.ok_or_else(|| format!("splice unconfirmed; Commit omitted: {exec:?}"))?,
            )?;
            let status = context
                .api
                .status(id)
                .map_err(|error| format!("post-splice status: {error:?}"))?;
            if status.projection_count("range_state")
                != before
                    .projection_count("range_state")
                    .map(|count| count + 2)
                || status.projection_count("range_edit")
                    != before.projection_count("range_edit").map(|count| count + 1)
                || status.projection_count("write") != before.projection_count("write")
                || status.range_accepted_payload_bytes
                    != before.range_accepted_payload_bytes + case.insert_len
                || status.range_shifted_suffix_bytes != before.range_shifted_suffix_bytes
            {
                return Err(format!("range counters before={before:?} after={status:?}"));
            }
            let read_before = status
                .projection_count("read")
                .ok_or("missing read counter")?;
            let mounted =
                verify_mounted_windows(context.api, id, context.source, case, Some(inserted), true);
            let after = context.api.status(id);
            let read_after = after
                .as_ref()
                .ok()
                .and_then(|status| status.projection_count("read"));
            let read_delta = read_after.and_then(|count| count.checked_sub(read_before));
            if let Err(primary) = mounted {
                return Err(format!("{primary}; verifier_read_callbacks={read_delta:?}; post_verifier_status={after:?}"));
            }
            let read_delta = read_delta
                .ok_or_else(|| format!("verifier read counter unavailable: {after:?}"))?;
            Ok((old, new, read_delta))
        },
    );
    if let Some((elapsed, result)) = baseline_observation {
        writeln!(receipt, "baseline_exec_elapsed_ns\t{elapsed}").unwrap();
        writeln!(receipt, "baseline_exec_result\t{result}").unwrap();
    }
    let (old, new, verifier_read_callbacks) = mounted?;
    writeln!(
        receipt,
        "verifier_read_callbacks\t{verifier_read_callbacks}"
    )
    .map_err(|error| format!("callback receipt: {error}"))?;
    if old.commit == new.commit || old.root == new.root {
        return Err("old/new Commit identical".into());
    }
    if public_branch_state(context.history_path, context.cursor_key, branch.id)?.0
        != Some(new.commit)
    {
        return Err("published Branch head differs from Commit".into());
    }
    let pristine = FIXTURES
        .iter()
        .find(|fixture| fixture.0 == case.label)
        .unwrap();
    let (root, count, size, digest) = stored_oracle(context.store_path, new.root, true)?;
    if size != expected_size
        || digest.as_deref() != Some(expected_sha.as_str())
        || root == pristine.3
        || count == 0
    {
        return Err(format!("stored byte/canonical oracle {root}/{count}/{size}/{digest:?} != length {expected_size}, digest {expected_sha}, non-pristine nonempty chunked root"));
    }
    let (old_root, old_count, old_size, _) = stored_oracle(context.store_path, old.root, false)?;
    if (old_root.as_str(), old_count, old_size) != (pristine.3, pristine.4, case.size) {
        return Err(format!(
            "retained old Commit oracle {old_root}/{old_count}/{old_size}"
        ));
    }
    for (label, selected_branch, commit, edited, expect_baseline) in [
        ("fresh", branch.id, None, true, true),
        ("old", branch.id, Some(old.commit), false, true),
        ("source", context.source_branch, None, false, false),
    ] {
        with_fresh_sandbox_mount(
            context,
            label,
            case,
            selected_branch,
            commit,
            receipt,
            |id| {
                verify_mounted_windows(
                    context.api,
                    id,
                    context.source,
                    case,
                    if edited { Some(inserted) } else { None },
                    expect_baseline,
                )
            },
        )?;
    }
    let source_state = public_branch_state(
        context.history_path,
        context.cursor_key,
        context.source_branch,
    )?;
    if source_state != (None, context.project.genesis_layer, context.project.root) {
        return Err(format!("pristine source Branch moved: {source_state:?}"));
    }
    Ok(format!("new_commit={:?}\told_commit={:?}\tsha256={expected_sha}\tcanonical_root={root}\tcanonical_count={count}\tfinal_bytes={expected_size}\tverifier_read_callbacks={verifier_read_callbacks}", new.commit, old.commit))
}

#[test]
fn mounted_public_sdk_position_sweep() {
    use layerfs_sdk::{
        HistoryMode, Project, ProjectApi, SandboxApi, Server, ServerConfig, WorkspaceApi,
    };
    use layerfs_telemetry::{
        output::{Identity, OutputConfig},
        runtime::{Configuration, MonitorConfig, Runtime},
    };
    const ENV: [&str; 7] = [
        "LAYERFS_TEST_IMAGE",
        "LAYERFS_POSITION_SIZE",
        "LAYERFS_POSITION_SOURCE_DIR",
        "LAYERFS_POSITION_OUTPUT",
        "LAYERFS_POSITION_MASTER_RECEIPT",
        "LAYERFS_POSITION_MASTER_SHA256",
        "LAYERFS_HISTORY_CURSOR_KEY",
    ];
    if ENV.iter().all(|key| std::env::var(key).is_err()) {
        return;
    }
    let [image, label, source_dir, output_dir, master_receipt, master_sha, cursor_text] =
        ENV.map(|key| std::env::var(key).unwrap_or_else(|_| panic!("missing {key}")));
    let cursor_key = hex_array::<32>(&cursor_text);
    let fixture = FIXTURES
        .iter()
        .find(|fixture| fixture.0 == label)
        .expect("declared size");
    let source = Path::new(&source_dir).join(format!("{label}.bin"));
    assert_eq!(std::fs::metadata(&source).unwrap().len(), fixture.1);
    assert_eq!(sha256(&source), fixture.2);
    let selected = std::env::var("LAYERFS_POSITION_CASE_ID").ok();
    let rows: Vec<_> = cases()
        .into_iter()
        .filter(|case| case.label == label && selected.as_ref().is_none_or(|id| &case.id == id))
        .collect();
    assert_eq!(rows.len(), if selected.is_some() { 1 } else { 66 });
    let output = PathBuf::from(output_dir);
    std::fs::create_dir(&output).expect("fresh append-only output directory");
    let mut run_receipt = File::create_new(output.join("run.tsv")).unwrap();
    writeln!(run_receipt, "schema\tissue241-position-functional-run-v1").unwrap();
    writeln!(run_receipt, "manifest_sha256\t{MANIFEST_SHA256}").unwrap();
    writeln!(run_receipt, "master_sha256\t{master_sha}").unwrap();
    writeln!(run_receipt, "size\t{label}").unwrap();
    writeln!(run_receipt, "status\tSTARTED").unwrap();
    let diagnostic_run = std::env::var("LAYERFS_POSITION_DIAGNOSTIC_RUN")
        .ok()
        .map(|value| {
            value
                .parse::<u128>()
                .expect("numeric diagnostic run identity")
        });
    let telemetry = if let Some(run) = diagnostic_run {
        writeln!(run_receipt, "diagnostic_telemetry_run\t{run}").unwrap();
        Runtime::start(Configuration {
            enabled: true,
            timing: true,
            monitor: MonitorConfig {
                cpu: true,
                memory: true,
                interval_ms: 10,
                history: 600,
                windows: 32,
            },
            output: OutputConfig::forward(),
            identity: Identity {
                run,
                pid: std::process::id(),
                role: 1,
                namespace: 1,
            },
        })
        .unwrap()
    } else {
        Runtime::disabled()
    };
    let state = output.join("private-state");
    let prepared = Command::new("python3")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../layerfs-fuse/tests/position_master.py"
        ))
        .args([
            "--master",
            &master_receipt,
            "--master-sha256",
            &master_sha,
            "--size",
            &label,
            "--out",
        ])
        .arg(&state)
        .output()
        .unwrap();
    File::create_new(output.join("master-copy.stdout"))
        .unwrap()
        .write_all(&prepared.stdout)
        .unwrap();
    File::create_new(output.join("master-copy.stderr"))
        .unwrap()
        .write_all(&prepared.stderr)
        .unwrap();
    assert!(
        prepared.status.success(),
        "closed master rejected: {:?}",
        prepared.status
    );
    let master = read_master_spec(&state.join("master-spec.tsv"));
    assert_eq!(master["master_sha256"], master_sha);
    for field in [
        "producer_source_commit",
        "producer_source_tree",
        "producer_binary_sha256",
        "compatibility_key_sha256",
        "store_sha256",
        "history_sha256",
        "clone_method",
    ] {
        writeln!(run_receipt, "{field}\t{}", master[field]).unwrap();
    }
    let store_path = state.join("store.sqlite");
    let history_path = state.join("history.sqlite");
    let server = Server::open(ServerConfig {
        store_path: store_path.clone(),
        history_path: history_path.clone(),
        binding_key: b"layerfs-bench-pro".to_vec(),
        incarnation: 1,
        cursor_key,
        history: HistoryMode::OpenWritable,
        service_host: "host.docker.internal".into(),
        runtime: telemetry,
        telemetry_run: diagnostic_run,
    })
    .unwrap();
    server.listen().unwrap();
    let owner = server.owner().unwrap();
    let projects = ProjectApi::new(&server);
    let sandboxes = SandboxApi::new(&owner);
    let api = WorkspaceApi::new(&owner);
    let project = Project {
        id: hex_array(&master["project_id"]),
        genesis_layer: hex_array(&master["genesis_layer"]),
        root: hex_array(&master["genesis_root"]),
        root_serial: master["genesis_root_serial"].parse().unwrap(),
    };
    let (pristine_root, pristine_count, pristine_size, pristine_digest) =
        stored_oracle(&store_path, project.root, true).expect("copied master C1/C5 oracle");
    assert_eq!(
        (
            pristine_root.as_str(),
            pristine_count,
            pristine_size,
            pristine_digest.as_deref()
        ),
        (fixture.3, fixture.4, fixture.1, Some(fixture.2)),
        "copied closed master fixture identity"
    );
    let source_branch = projects.fork(&project, [1; 16], "pristine-source").unwrap();
    let payload = replacement();
    let context = SweepContext {
        source: &source,
        payload: &payload,
        image: &image,
        output: &output,
        sandboxes: &sandboxes,
        project: &project,
        projects: &projects,
        api: &api,
        store_path: &store_path,
        history_path: &history_path,
        cursor_key,
        source_branch: source_branch.id,
    };
    let mut failed = Vec::new();
    let mut attempted = 0;
    let all_cases = cases();
    for case in &rows {
        attempted += 1;
        let mut receipt = File::create_new(output.join(format!("{}.tsv", case.id))).unwrap();
        writeln!(receipt, "schema\tissue241-position-functional-v1").unwrap();
        writeln!(receipt, "manifest_sha256\t{MANIFEST_SHA256}").unwrap();
        writeln!(receipt, "case_id\t{}", case.id).unwrap();
        writeln!(receipt, "operation\t{}", case.op).unwrap();
        writeln!(receipt, "offset\t{}", case.offset).unwrap();
        writeln!(receipt, "status\tSTARTED").unwrap();
        let index = all_cases.iter().position(|row| row.id == case.id).unwrap();
        let sandbox = match sandboxes.create(&image, &format!("{}-edit", case.id)) {
            Ok(id) => id,
            Err(error) => {
                let message = format!("edit sandbox create: {error:?}");
                writeln!(receipt, "status\tFAIL\t{message}").unwrap();
                if let Some(id) = error.sandbox {
                    observe_failed_sandbox(&output, case, "edit", id, &mut receipt);
                    writeln!(
                        receipt,
                        "retained_sandbox_delete\t{:?}",
                        sandboxes.delete(id)
                    )
                    .unwrap();
                }
                failed.push(format!("{} @{}: {message}", case.id, case.offset));
                break;
            }
        };
        writeln!(receipt, "edit_sandbox_id\t{sandbox}").unwrap();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_case(case, index, &context, sandbox, &mut receipt)
        }))
        .unwrap_or_else(|panic| {
            let message = panic
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| panic.downcast_ref::<&str>().copied())
                .unwrap_or("unknown panic");
            Err(format!("test panic: {message}"))
        });
        if result.is_err() {
            observe_failed_sandbox(&output, case, "edit", sandbox, &mut receipt);
        }
        let deleted = sandboxes.delete(sandbox);
        let listed = sandboxes.list();
        let absent = listed
            .as_ref()
            .is_ok_and(|entries| entries.iter().all(|entry| entry.id != sandbox));
        writeln!(receipt, "edit_sandbox_delete\t{deleted:?}").unwrap();
        writeln!(receipt, "edit_sandbox_absent\t{absent}").unwrap();
        let outcome = match (result, deleted.is_ok() && absent) {
            (Ok(detail), true) => Ok(detail),
            (Err(primary), true) => Err(primary),
            (Ok(_), false) => Err(format!(
                "edit sandbox cleanup: delete={deleted:?} list={listed:?}"
            )),
            (Err(primary), false) => Err(format!(
                "{primary}; edit sandbox cleanup: delete={deleted:?} list={listed:?}"
            )),
        };
        match outcome {
            Ok(detail) => writeln!(receipt, "status\tPASS\t{detail}").unwrap(),
            Err(error) => {
                writeln!(receipt, "status\tFAIL\t{error}").unwrap();
                failed.push(format!("{} @{}: {error}", case.id, case.offset));
                break;
            }
        }
    }
    for case in rows.iter().skip(attempted) {
        let mut receipt = File::create_new(output.join(format!("{}.tsv", case.id))).unwrap();
        writeln!(receipt, "schema\tissue241-position-functional-v1").unwrap();
        writeln!(receipt, "manifest_sha256\t{MANIFEST_SHA256}").unwrap();
        writeln!(receipt, "case_id\t{}", case.id).unwrap();
        writeln!(receipt, "offset\t{}", case.offset).unwrap();
        writeln!(receipt, "status\tNOT_RUN\tstopped after prior failure").unwrap();
    }
    let listed = sandboxes.list();
    let cleanup_ok = listed.as_ref().is_ok_and(Vec::is_empty);
    let mut summary = File::create_new(output.join("summary.tsv")).unwrap();
    writeln!(summary, "selected\t{}", rows.len()).unwrap();
    writeln!(summary, "attempted\t{attempted}").unwrap();
    writeln!(summary, "cleanup\t{cleanup_ok}").unwrap();
    writeln!(summary, "sandbox_list\t{listed:?}").unwrap();
    writeln!(summary, "failures\t{:?}", failed).unwrap();
    writeln!(
        run_receipt,
        "status\t{}",
        if cleanup_ok && failed.is_empty() && attempted == rows.len() {
            "PASS"
        } else {
            "FAIL"
        }
    )
    .unwrap();
    server.shutdown();
    assert!(cleanup_ok, "sandbox cleanup: {listed:?}");
    assert!(failed.is_empty(), "{}", failed.join("; "));
}

#[test]
fn sealed_position_manifest_has_192_bands_and_named_edges() {
    let rows = cases();
    assert_eq!(
        rows.iter().filter(|case| case.id.contains("-band")).count(),
        192
    );
    assert_eq!(
        rows.iter()
            .filter(|case| !case.id.contains("-band"))
            .count(),
        72
    );
    let _ = replacement();
}

#[test]
fn bounded_oracle_maps_prefix_replacement_and_suffix() {
    let source = std::env::temp_dir().join(format!("issue241-window-{}", std::process::id()));
    File::create_new(&source)
        .unwrap()
        .write_all(b"abcdefghijklmnop")
        .unwrap();
    let case = Case {
        id: "mapping".into(),
        label: "tiny".into(),
        size: 16,
        op: "overwrite".into(),
        offset: 5,
        delete_len: 3,
        insert_len: 2,
        payload: "-".into(),
    };
    assert_eq!(
        expected_window(&source, Some((&case, b"XY")), 0, 15),
        b"abcdeXYijklmnop"
    );
    assert_eq!(
        expected_window(&source, Some((&case, b"XY")), 4, 5),
        b"eXYij"
    );
    assert_eq!(expected_window(&source, None, 4, 5), b"efghi");
    std::fs::remove_file(source).unwrap();
}

fn mounted_oracle_test_dir(name: &str) -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("issue241-{name}-{}-{nonce}", std::process::id()));
    std::fs::create_dir(&dir).unwrap();
    std::fs::write(dir.join("payload.bin"), vec![b'a'; 4096]).unwrap();
    dir
}

fn mounted_shell_output(dir: &Path, command: &str) -> std::process::Output {
    // The live image uses GNU-style stat; macOS hosts need this local size shim.
    let command = format!(
        "stat() {{ [ \"$1\" = -c ] && [ \"$2\" = %s ] || return 2; wc -c < \"$3\"; }}; {command}"
    );
    Command::new("sh")
        .arg("-c")
        .arg(&command)
        .current_dir(dir)
        .output()
        .unwrap()
}

fn mounted_shell_passes(dir: &Path, command: &str) -> bool {
    mounted_shell_output(dir, command).status.success()
}

#[test]
fn mounted_oracle_rejects_eof_read_error() {
    let dir = mounted_oracle_test_dir("eof-error");
    let bytes: Vec<_> = (0..8192).map(|index| (index % 251) as u8).collect();
    std::fs::write(dir.join("payload.bin"), &bytes).unwrap();
    let command = mounted_window_command(8192, [(0, 4096), (3758, 4096), (4096, 4096)], false);
    let failed_eof = format!(
        "dd() {{ for arg in \"$@\"; do [ \"$arg\" = skip=2 ] && return 74; done; command dd \"$@\"; }}; {command}"
    );
    let output = mounted_shell_output(&dir, &command);
    assert!(output.status.success(), "{output:?}");
    let text = String::from_utf8(output.stdout).unwrap();
    let seam_sha = format!("{:x}", Sha256::digest(&bytes[3758..3758 + 4096]));
    assert_eq!(
        text.lines().nth(2).unwrap().split_whitespace().next(),
        Some(seam_sha.as_str())
    );
    assert!(!mounted_shell_passes(&dir, &failed_eof));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn mounted_oracle_distinguishes_old_commit_from_pristine_source() {
    let dir = mounted_oracle_test_dir("old-marker");
    let windows = [(0, 4096); 3];
    let source = mounted_window_command(4096, windows, false);
    let old = mounted_window_command(4096, windows, true);
    assert!(mounted_shell_passes(&dir, &source));
    assert!(!mounted_shell_passes(&dir, &old));
    std::fs::write(dir.join(".position-baseline"), b"baseline").unwrap();
    assert!(!mounted_shell_passes(&dir, &source));
    assert!(mounted_shell_passes(&dir, &old));
    std::fs::write(dir.join(".position-baseline"), b"baseline\n").unwrap();
    assert!(!mounted_shell_passes(&dir, &old));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn baseline_exec_liveness_diagnostic() {
    use layerfs_sdk::{
        HistoryMode, Project, ProjectApi, SandboxApi, Server, ServerConfig, WorkspaceApi,
    };
    use layerfs_telemetry::{
        output::{Identity, OutputConfig},
        runtime::{Configuration, MonitorConfig, Runtime},
    };
    use std::{net::TcpStream, time::Instant};

    let Ok(output) = std::env::var("LAYERFS_EXEC_PROBE_OUTPUT") else {
        return;
    };
    let image = std::env::var("LAYERFS_TEST_IMAGE").expect("diagnostic image");
    let state = PathBuf::from(std::env::var("LAYERFS_EXEC_PROBE_STATE").expect("diagnostic copy"));
    let run = std::env::var("LAYERFS_EXEC_PROBE_RUN")
        .expect("telemetry run identity")
        .parse::<u128>()
        .expect("numeric telemetry run identity");
    let cursor_key =
        hex_array::<32>(&std::env::var("LAYERFS_HISTORY_CURSOR_KEY").expect("history cursor key"));
    let output = PathBuf::from(output);
    std::fs::create_dir(&output).expect("fresh diagnostic output");
    let mut receipt = File::create_new(output.join("receipt.tsv")).unwrap();
    writeln!(
        receipt,
        "schema\tissue241-baseline-exec-liveness-diagnostic-v1"
    )
    .unwrap();
    let exec_count = std::env::var("LAYERFS_EXEC_PROBE_COUNT")
        .ok()
        .map_or(1, |value| value.parse::<usize>().unwrap());
    assert!((1..=128).contains(&exec_count));
    writeln!(
        receipt,
        "command_template\tprintf baseline > .position-baseline[-NNN]"
    )
    .unwrap();
    writeln!(receipt, "planned_exec_count\t{exec_count}").unwrap();
    writeln!(
        receipt,
        "cache\tfunctional-uncontrolled;admission-ineligible"
    )
    .unwrap();
    writeln!(receipt, "status\tSTARTED").unwrap();
    let master = read_master_spec(&state.join("master-spec.tsv"));
    let telemetry = Runtime::start(Configuration {
        enabled: true,
        timing: true,
        monitor: MonitorConfig {
            cpu: true,
            memory: true,
            interval_ms: 10,
            history: 600,
            windows: 32,
        },
        output: OutputConfig::forward(),
        identity: Identity {
            run,
            pid: std::process::id(),
            role: 1,
            namespace: 1,
        },
    })
    .unwrap();
    let server = Server::open(ServerConfig {
        store_path: state.join("store.sqlite"),
        history_path: state.join("history.sqlite"),
        binding_key: b"layerfs-bench-pro".to_vec(),
        incarnation: 1,
        cursor_key,
        history: HistoryMode::OpenWritable,
        service_host: "host.docker.internal".into(),
        runtime: telemetry,
        telemetry_run: Some(run),
    })
    .unwrap();
    server.listen().unwrap();
    let owner = server.owner().unwrap();
    let projects = ProjectApi::new(&server);
    let sandboxes = SandboxApi::new(&owner);
    let api = WorkspaceApi::new(&owner);
    let project = Project {
        id: hex_array(&master["project_id"]),
        genesis_layer: hex_array(&master["genesis_layer"]),
        root: hex_array(&master["genesis_root"]),
        root_serial: master["genesis_root_serial"].parse().unwrap(),
    };
    let branch = projects
        .fork(&project, [241; 16], "baseline-exec-liveness-diagnostic")
        .unwrap();
    let sandbox = sandboxes
        .create(&image, "baseline-exec-liveness-diagnostic")
        .unwrap();
    writeln!(receipt, "sandbox_id\t{sandbox}").unwrap();
    let mount = api.mount(sandbox, &project, branch.id, None);
    writeln!(receipt, "mount\t{mount:?}").unwrap();
    let mut failed;
    if let Ok(mount) = mount {
        let held_count = std::env::var("LAYERFS_EXEC_PROBE_HELD_SESSIONS")
            .ok()
            .map_or(0, |value| value.parse::<usize>().unwrap());
        assert!(held_count <= 3);
        let mut held = Vec::new();
        for _ in 0..held_count {
            let mut socket = TcpStream::connect(server.endpoint().unwrap()).unwrap();
            socket.write_all(&1u32.to_be_bytes()).unwrap();
            held.push(socket);
        }
        if held_count > 0 {
            std::thread::sleep(Duration::from_millis(200));
        }
        writeln!(receipt, "held_incomplete_handshakes\t{held_count}").unwrap();
        failed = false;
        for index in 0..exec_count {
            let command = if exec_count == 1 {
                "printf baseline > .position-baseline".to_owned()
            } else {
                format!("printf baseline > .position-baseline-{index:03}")
            };
            let started = Instant::now();
            let exec = api.exec(&mount.id, &command);
            writeln!(receipt, "exec_{index:03}_command\t{command}").unwrap();
            writeln!(
                receipt,
                "exec_{index:03}_elapsed_ms\t{}",
                started.elapsed().as_millis()
            )
            .unwrap();
            writeln!(receipt, "exec_{index:03}_result\t{exec:?}").unwrap();
            failed = !exec
                .as_ref()
                .is_ok_and(|result| result.exit_status == Some(0));
            if failed {
                break;
            }
        }
        drop(held);
        let container = format!("layerfs-{sandbox}");
        for (name, args) in [
            (
                "inspect",
                vec!["inspect", "--format", "{{json .State}}", container.as_str()],
            ),
            ("top", vec!["top", container.as_str()]),
            ("logs", vec!["logs", container.as_str()]),
        ] {
            let snapshot = Command::new("docker").args(&args).output().unwrap();
            File::create_new(output.join(format!("docker-{name}.stdout")))
                .unwrap()
                .write_all(&snapshot.stdout)
                .unwrap();
            File::create_new(output.join(format!("docker-{name}.stderr")))
                .unwrap()
                .write_all(&snapshot.stderr)
                .unwrap();
            writeln!(receipt, "docker_{name}_status\t{:?}", snapshot.status).unwrap();
        }
        let unmount = api.unmount(&mount.id);
        writeln!(receipt, "unmount\t{unmount:?}").unwrap();
        failed |= unmount.is_err();
    } else {
        failed = true;
    }
    let deleted = sandboxes.delete(sandbox);
    let listed = sandboxes.list();
    let absent = listed
        .as_ref()
        .is_ok_and(|entries| entries.iter().all(|entry| entry.id != sandbox));
    writeln!(receipt, "sandbox_delete\t{deleted:?}").unwrap();
    writeln!(receipt, "sandbox_absent\t{absent}").unwrap();
    writeln!(
        receipt,
        "status\t{}",
        if failed || !absent { "FAIL" } else { "PASS" }
    )
    .unwrap();
    server.shutdown();
    assert!(absent, "diagnostic Sandbox cleanup failed: {listed:?}");
    assert!(
        !failed,
        "diagnostic SDK control sequence failed; see receipt"
    );
}

#[test]
fn derive_validated_pristine_chunk_boundaries() {
    let (Ok(source_dir), Ok(output)) = (
        std::env::var("LAYERFS_POSITION_SOURCE_DIR"),
        std::env::var("LAYERFS_POSITION_BOUNDARIES_OUT"),
    ) else {
        return;
    };
    let mut receipt = String::from(
        "size_label\tpristine_bytes\tsha256\tcanonical_root\tcanonical_count\tboundary\n",
    );
    for (label, size, expected_sha, expected_root, expected_count) in FIXTURES {
        let source = Path::new(&source_dir).join(format!("{label}.bin"));
        assert_eq!(
            std::fs::metadata(&source).unwrap().len(),
            size,
            "{label} size"
        );
        assert_eq!(sha256(&source), expected_sha, "{label} SHA-256");
        let mut sink = BoundarySink {
            cursor: 0,
            middle: size / 2,
            best: None,
            size,
        };
        let capacities = ConstructionPolicy::frozen_default().capacities();
        let mapping =
            build_streaming(&capacities, File::open(&source).unwrap(), &mut sink).unwrap();
        assert_eq!(mapping.extent_count, expected_count, "{label} chunk count");
        assert_eq!(sink.cursor, size, "{label} consumed bytes");
        let root = emit_file_state(&mut sink, mapping.root.unwrap()).unwrap();
        assert_eq!(root.to_string(), expected_root, "{label} canonical root");
        let boundary = sink.best.unwrap();
        receipt.push_str(&format!(
            "{label}\t{size}\t{expected_sha}\t{root}\t{expected_count}\t{boundary}\n"
        ));
    }
    use std::io::Write;
    File::create_new(output)
        .unwrap()
        .write_all(receipt.as_bytes())
        .unwrap();
}
