use super::*;
use std::collections::BTreeMap;
use workload_source::workspace_common::{self as common, Case, Entry, EntryKind};
use workload_source::workspace_registry as registry;

const ORCHESTRATION_SCOPE:&str="post-Store-open orchestration through timed calls and in-loop observations, including argument preparation and output; excludes post-loop observations, owner close and post-End verification; verify mode includes precommit custody";

// Output and allocation stay outside pure-call clocks. A single PhaseDeadline
// watchdog shares exact timestamp transitions; the runner owns stop/cleanup.
// Lock acquisition precedes each start timestamp; finish reads time and disarms
// under the same lock, preventing a completed phase from timing out later.
const PRODUCT_TIME_LIMIT_NS: u64 = 15_000_000_000;

pub(crate) struct ProductBudget {
    enabled: bool,
    cumulative_ns: u64,
    state: Arc<std::sync::Mutex<ProductBudgetState>>,
    _deadline: Option<PhaseDeadline>,
}
#[derive(Default)]
struct ProductBudgetState {
    completed_ns: u64,
    active: Option<(&'static str, ProductPhaseClock)>,
}
enum ProductPhaseClock { Instant(Instant), Raw(u64) }
impl ProductBudgetState {
    fn total_at(&self, active_elapsed_ns: u64) -> u64 {
        self.completed_ns.saturating_add(active_elapsed_ns)
    }
}
impl ProductBudget {
    fn new(enabled: bool) -> Self {
        let state = Arc::new(std::sync::Mutex::new(ProductBudgetState::default()));
        let deadline = enabled.then(|| PhaseDeadline::product_budget(state.clone()));
        Self { enabled, cumulative_ns: 0, state, _deadline: deadline }
    }
    pub(crate) fn start_clock(&self, phase: &'static str) -> AnyResult<Instant> {
        let mut state = self.state.lock().map_err(|_| "product budget lock")?;
        let start = Instant::now();
        state.active = Some((phase, ProductPhaseClock::Instant(start)));
        Ok(start)
    }
    pub(crate) fn finish_clock(&self, start: Instant) -> AnyResult<u64> {
        let mut state = self.state.lock().map_err(|_| "product budget lock")?;
        let elapsed = elapsed_ns(start);
        state.completed_ns = state.completed_ns.checked_add(elapsed).ok_or("phase sum overflow")?;
        state.active = None;
        Ok(elapsed)
    }
    pub(crate) fn start_raw_clock(&self, phase: &'static str) -> layerfs_sdk::Result<u64> {
        let mut state = self.state.lock().map_err(|_| layerfs_sdk::SdkError::InvalidRequest("product budget lock"))?;
        let start = super::sdk_edit_clock_ns()?;
        state.active = Some((phase, ProductPhaseClock::Raw(start)));
        Ok(start)
    }
    pub(crate) fn transition_raw_clock(&self, start: u64, phase: &'static str) -> layerfs_sdk::Result<u64> {
        let mut state = self.state.lock().map_err(|_| layerfs_sdk::SdkError::InvalidRequest("product budget lock"))?;
        let end = super::sdk_edit_clock_ns()?;
        state.completed_ns = state.completed_ns.checked_add(end.saturating_sub(start))
            .ok_or(layerfs_sdk::SdkError::InvalidRequest("phase sum overflow"))?;
        state.active = Some((phase, ProductPhaseClock::Raw(end)));
        Ok(end)
    }
    fn completed_group_ns(&self) -> AnyResult<u64> {
        self.state.lock().map_err(|_| "product budget lock")?.completed_ns
            .checked_sub(self.cumulative_ns).ok_or_else(|| "product budget group balance".into())
    }
    pub(crate) fn finish_raw_clock(&self, start: u64) -> layerfs_sdk::Result<u64> {
        let mut state = self.state.lock().map_err(|_| layerfs_sdk::SdkError::InvalidRequest("product budget lock"))?;
        let end = super::sdk_edit_clock_ns()?;
        state.completed_ns = state.completed_ns.checked_add(end.saturating_sub(start))
            .ok_or(layerfs_sdk::SdkError::InvalidRequest("phase sum overflow"))?;
        state.active = None;
        Ok(end)
    }
    fn check(&self) -> AnyResult<()> {
        if self.enabled && self.cumulative_ns > PRODUCT_TIME_LIMIT_NS {
            return Err("suppressed_phase1_time_budget".into());
        }
        Ok(())
    }
    fn begin(&self, phase: &str) -> AnyResult<()> {
        self.check()?;
        if self.enabled {
            self.event("begin", phase, 0, None);
        }
        Ok(())
    }
    fn end(&mut self, phase: &str, elapsed_ns: u64, error: Option<String>) -> AnyResult<()> {
        self.cumulative_ns = self.cumulative_ns.checked_add(elapsed_ns)
            .ok_or("product budget phase sum overflow")?;
        if self.enabled {
            self.event("end", phase, elapsed_ns, error);
            if self.cumulative_ns > PRODUCT_TIME_LIMIT_NS {
                emit("product-time-budget-exceeded", &[
                    ("limit_ns", PRODUCT_TIME_LIMIT_NS.to_string()),
                    ("cumulative_ns", self.cumulative_ns.to_string()),
                    ("phase", quote(phase)),
                    ("measurement", quote("completed-pure-call-sum")),
                ]);
            }
        }
        Ok(())
    }
    fn event(&self, state: &str, phase: &str, elapsed_ns: u64, error: Option<String>) {
        emit("product-budget-phase", &[
            ("state", quote(state)),
            ("phase", quote(phase)),
            ("cumulative_ns", self.cumulative_ns.to_string()),
            ("limit_ns", PRODUCT_TIME_LIMIT_NS.to_string()),
            ("elapsed_ns", elapsed_ns.to_string()),
            ("phase_error", error.as_deref().map(quote).unwrap_or_else(|| "null".into())),
        ]);
    }
}

struct HostSampler {
    stop: Arc<std::sync::atomic::AtomicBool>,
    thread: Option<std::thread::JoinHandle<Result<(u64, u64, u64, u64, u64), String>>>,
}

pub(crate) struct PhaseDeadline {
    stop: Option<mpsc::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl PhaseDeadline {
    fn product_budget(state: Arc<std::sync::Mutex<ProductBudgetState>>) -> Self {
        let (tx, rx) = mpsc::channel();
        let thread = std::thread::spawn(move || loop {
            match rx.recv_timeout(std::time::Duration::from_millis(1)) {
                Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
            let state = state.lock().expect("product budget lock");
            let Some((phase, ref clock)) = state.active else { continue };
            let elapsed = match clock {
                ProductPhaseClock::Instant(start) => elapsed_ns(*start),
                ProductPhaseClock::Raw(start) => match super::sdk_edit_clock_ns() {
                    Ok(now) => now.saturating_sub(*start),
                    Err(error) => {
                        emit("product-budget-observation-error", &[
                            ("phase", quote(phase)),
                            ("error", quote(&error.to_string())),
                            ("status", quote("harness-error")),
                        ]);
                        std::process::exit(125);
                    }
                },
            };
            let total = state.total_at(elapsed);
            if total <= PRODUCT_TIME_LIMIT_NS { continue }
            let completed = state.completed_ns;
            drop(state);
            emit("product-time-budget-exceeded", &[
                ("limit_ns", PRODUCT_TIME_LIMIT_NS.to_string()),
                ("cumulative_ns", total.to_string()),
                ("completed_product_ns", completed.to_string()),
                ("active_phase_ns", elapsed.to_string()),
                ("phase", quote(phase)),
                ("measurement", quote("active-pure-call-sum")),
            ]);
            std::process::exit(124);
        });
        Self { stop: Some(tx), thread: Some(thread) }
    }
    pub(crate) fn start(scope: &'static str, seconds: u64) -> Self {
        let (tx, rx) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            if matches!(
                rx.recv_timeout(std::time::Duration::from_secs(seconds)),
                Err(mpsc::RecvTimeoutError::Timeout)
            ) {
                if let Ok(snapshot) = process_resource_snapshot() {
                    resource_receipt("deadline", snapshot);
                }
                emit(
                    "deadline-failure",
                    &[
                        ("scope", quote(scope)),
                        ("hard_deadline_ns", (seconds * 1_000_000_000).to_string()),
                    ],
                );
                std::process::exit(124);
            }
        });
        Self {
            stop: Some(tx),
            thread: Some(thread),
        }
    }
}
impl Drop for PhaseDeadline {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl HostSampler {
    fn start() -> AnyResult<Self> {
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let running = stop.clone();
        let (tx, rx) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            let mut count = 0;
            let mut peak = 0;
            let mut baseline = 0;
            let mut last = Instant::now();
            let mut gap = 0;
            loop {
                let rss = process_current_rss(std::process::id()).map_err(|e| e.to_string())?;
                let now = Instant::now();
                if count == 0 {
                    baseline = rss;
                    let _ = tx.send(());
                } else {
                    gap = gap.max(nanos(last, now));
                }
                peak = peak.max(rss);
                count += 1;
                last = now;
                if rss > 2 * 1024 * 1024 * 1024 {
                    emit("resource-failure", &[("host_rss_bytes", rss.to_string())]);
                    std::process::exit(125);
                }
                if running.load(std::sync::atomic::Ordering::Relaxed) {
                    return Ok((count, baseline, peak, rss, gap));
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        });
        rx.recv_timeout(std::time::Duration::from_secs(5))?;
        Ok(Self {
            stop,
            thread: Some(thread),
        })
    }
}

impl Drop for HostSampler {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            match thread.join() {
                Ok(Ok((count, baseline, peak, final_rss, gap))) => emit(
                    "host-rss-samples",
                    &[
                        ("sample_count", count.to_string()),
                        ("baseline_bytes", baseline.to_string()),
                        ("sampled_peak_bytes", peak.to_string()),
                        ("final_bytes", final_rss.to_string()),
                        ("maximum_gap_ns", gap.to_string()),
                        ("nominal_interval_ns", "10000000".into()),
                    ],
                ),
                error => emit(
                    "host-rss-failure",
                    &[("error", quote(&format!("{error:?}")))],
                ),
            }
        }
        match process_resource_snapshot() {
            Ok(snapshot) => resource_receipt("final", snapshot),
            Err(error) => emit(
                "host-resource-failure",
                &[("error", quote(&error.to_string()))],
            ),
        }
    }
}

pub(crate) fn quote(value: &str) -> String {
    let mut out = String::from("\"");
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c < ' ' => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

pub(crate) fn emit(kind: &str, fields: &[(&str, String)]) {
    let mut line = format!("{{\"kind\":{}", quote(kind));
    for (key, value) in fields {
        line.push_str(&format!(",{}:{}", quote(key), value));
    }
    line.push_str("}\n");
    // Watchdog/resource threads must not interleave partial JSON records.
    let mut output = std::io::stdout().lock();
    output
        .write_all(line.as_bytes())
        .expect("write benchmark evidence");
    output.flush().expect("flush benchmark evidence");
}

fn runtime_observation_window(
    case: &str,
    mode: &str,
    run: impl FnOnce() -> AnyResult<()>,
) -> AnyResult<()> {
    let started = Instant::now();
    let outcome = run();
    emit("runtime-observation-window", &[
        ("scenario_id", quote(case)),
        ("mode", quote(mode)),
        ("elapsed_ns", elapsed_ns(started).to_string()),
        ("scope", quote("selected-run dispatch through all reached product, verification and recovery work and local Client/Store drops; excludes process-wide daemon-owner teardown and supervisor cleanup")),
        ("start_event", quote("selected-run-dispatch")),
        ("end_event", quote("selected-run-return-before-process-owner-drain")),
        ("status", quote(if outcome.is_ok() { "success" } else { "error" })),
    ]);
    outcome
}

fn physical_spool_state(client: &Client, id: WorkspaceId, phase: &str) -> AnyResult<()> {
    // The instrumented getter copies maintained counters. It performs no
    // correctness oracle, namespace scan, payload read or failure injection.
    let state = match client.verification_workspace_state(id) {
        Ok(state) => state,
        Err(error) => {
            emit(
                "workspace-physical-spool-error",
                &[
                    ("phase", quote(phase)),
                    ("error", quote(&error.to_string())),
                ],
            );
            return Err(error.into());
        }
    };
    emit(
        "workspace-physical-spool",
        &[
            ("phase", quote(phase)),
            (
                "allocated_bytes",
                state
                    .physical_spool_allocated_bytes
                    .map_or("null".into(), |n| n.to_string()),
            ),
            (
                "peak_bytes",
                state
                    .physical_spool_peak_bytes
                    .map_or("null".into(), |n| n.to_string()),
            ),
            (
                "observation_errors",
                state.physical_spool_observation_errors.to_string(),
            ),
            (
                "observation_count",
                state.physical_spool_observation_count.to_string(),
            ),
            ("precision", quote("mutation-event-aggregate-allocation")),
            ("method", quote("verification_workspace_state")),
            (
                "scope",
                quote("passive counters before failure recovery; no independent verification"),
            ),
        ],
    );
    let current = state
        .physical_spool_allocated_bytes
        .ok_or("physical spool current allocation unavailable")?;
    let peak = state
        .physical_spool_peak_bytes
        .ok_or("physical spool peak allocation unavailable")?;
    if state.physical_spool_observation_errors != 0
        || current > peak
        || peak > 2 * 1024 * 1024 * 1024
    {
        return Err("physical spool observation/resource gate".into());
    }
    Ok(())
}

fn entry_info(entries: &[Entry]) -> AnyResult<(u64, usize, String)> {
    let mut bytes = 0_u64;
    let mut files = 0;
    let mut hash = workload_source::Sha256::new();
    for entry in entries {
        hash.update(format!("{entry:?}\n").as_bytes());
        match &entry.kind {
            EntryKind::File(content) => {
                bytes = bytes
                    .checked_add(content.len())
                    .ok_or("fixture byte overflow")?;
                files += 1;
            }
            EntryKind::Symlink(target) => bytes += target.len() as u64,
            EntryKind::Hardlink(target) => {
                let entry = entries
                    .iter()
                    .find(|e| &e.path == target)
                    .ok_or("hardlink target")?;
                if let EntryKind::File(content) = &entry.kind {
                    bytes += content.len();
                    files += 1;
                } else {
                    return Err("hardlink must target regular descriptor".into());
                }
            }
            EntryKind::Directory => (),
        }
    }
    if bytes >= 1_073_741_824 {
        return Err("fixture exceeds aggregate cap".into());
    }
    Ok((bytes, files, workload_source::hex(&hash.finish())))
}

fn fixture_info(case: &Case, seed: u8, branch: Option<BranchId>) -> AnyResult<()> {
    let entries = registry::fixture(case, seed)?;
    let (bytes, files, mut plan) = entry_info(&entries)?;
    if case.kind == "git-tool" {
        plan = workload_source::sdk_edit_common::sha256_hex(
            format!(
                "{plan}{}{}",
                include_str!("../ordinary_workloads.rs"),
                std::env::var("LAYERFS_V013_IMAGE").unwrap_or_default()
            )
            .as_bytes(),
        );
    }
    print!("{{\"fixture_profile\":\"workspace-input-v1\",\"fixture_bytes\":{bytes},\"regular_files\":{files},\"input_plan_sha256\":{},\"input_mode\":{}", quote(&plan),quote(if registry::is_import(case) { "directory" } else { "store" }));
    if let Some(branch) = branch {
        print!(",\"branch_id\":{}", quote(&branch.to_string()));
    }
    println!("}}");
    Ok(())
}

fn prepare(root: &Path, case: &Case, seed: u8) -> AnyResult<()> {
    if root.exists() {
        return Err("prepared input already exists".into());
    }
    std::fs::create_dir(root)?;
    let input = root.join("input");
    let mut entries = registry::fixture(case, seed)?;
    entry_info(&entries)?;
    common::create_fixture(&input, &entries)?;
    if case.kind == "git-tool" {
        native_preparation(
            root,
            &[
                "workspace-git-prepare".into(),
                "/preparation/store/input".into(),
                seed.to_string(),
            ],
        )?;
        entries = workload_source::ordinary_workloads::prepared_git_entries(&input, case, seed)?;
    }
    let manifest = common::manifest(&entries)?;
    if case.family == "edit_length_changing_capped" {
        let expected = workload_source::edit_length_changing_capped::fixture_sha256(case)?;
        if !manifest
            .lines()
            .any(|line| line.starts_with("payload.bin\t") && line.ends_with(expected))
        {
            return Err("capped input prefix hash differs from frozen identity".into());
        }
    }
    std::fs::write(
        root.parent()
            .ok_or("prepare parent")?
            .join("input-manifest.tsv"),
        manifest,
    )?;
    if registry::is_import(case) {
        fixture_info(case, seed, None)?;
    } else {
        let store = Arc::new(LayerStackStore::create(root.join("store.sqlite"))?);
        let client = Client::connect(store.clone())?;
        let initialized = client.initialize_layerstack(
            EntityName::new("phase1-input")?,
            LayerStackInitialization::Directory(input.clone()),
        )?;
        let branch = client.fork_branch(
            EntityName::new("main")?,
            LocalForkSource::Layer {
                layer_id: initialized.genesis_layer_id,
            },
        )?;
        drop(client);
        drop(store);
        std::fs::remove_dir_all(input)?;
        fixture_info(case, seed, Some(branch))?;
    }
    Ok(())
}

fn native_preparation(root: &Path, command: &[String]) -> AnyResult<()> {
    let image = std::env::var("LAYERFS_V013_IMAGE")?;
    let stage = std::fs::canonicalize(root.parent().ok_or("Git preparation stage")?)?;
    let mount = format!("type=bind,src={},dst=/preparation", stage.display());
    let token = std::env::var("LAYERFS_V013_PREPARATION_TOKEN")
        .unwrap_or_else(|_| format!("{}", std::process::id()));
    let name = format!("layerfs-prep-{token}-{}", command[0]);
    let label = format!("layerfs.phase1.preparation={token}");
    let output = Command::new("docker")
        .args([
            "run",
            "--rm",
            "--name",
            &name,
            "--label",
            &label,
            "--cpus",
            "2",
            "--memory",
            "2g",
            "--memory-swap",
            "2g",
            "--pids-limit",
            "256",
            "--mount",
            &mount,
            "--entrypoint",
            "/usr/local/bin/fs-benchmark-workload",
            &image,
        ])
        .args(command)
        .output()?;
    std::fs::write(
        stage.join(format!("{}.stdout.txt", command[0])),
        &output.stdout,
    )?;
    std::fs::write(
        stage.join(format!("{}.stderr.txt", command[0])),
        &output.stderr,
    )?;
    if !output.status.success() {
        return Err(format!(
            "Git native preparation failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(())
}

fn reference_info(case: &Case, seed: u8) -> AnyResult<()> {
    if case.kind != "git-tool" || !(1..=3).contains(&seed) {
        return Err("invalid Git reference selection".into());
    }
    let plan = workload_source::sdk_edit_common::sha256_hex(
        format!(
            "{}:{seed}:{}:{}",
            case.id,
            include_str!("../ordinary_workloads.rs"),
            std::env::var("LAYERFS_V013_IMAGE")?
        )
        .as_bytes(),
    );
    println!("{{\"fixture_profile\":\"git-reference-v1\",\"fixture_bytes\":0,\"regular_files\":0,\"input_mode\":\"directory\",\"input_plan_sha256\":{}}}",quote(&plan));
    Ok(())
}

fn prepare_reference(root: &Path, case: &Case, seed: u8) -> AnyResult<()> {
    if case.kind != "git-tool" || root.exists() {
        return Err("invalid Git reference output".into());
    }
    std::fs::create_dir(root)?;
    native_preparation(
        root,
        &[
            "workspace-git-reference".into(),
            "/preparation/store/input".into(),
            case.id.clone(),
            seed.to_string(),
        ],
    )?;
    reference_info(case, seed)
}

fn output_text(output: &OutputPage) -> AnyResult<String> {
    Ok(String::from_utf8(
        output
            .chunks
            .iter()
            .flat_map(|c| c.bytes.iter().copied())
            .collect(),
    )?)
}

pub(crate) fn observed(client: &Client, after: &mut u64) -> AnyResult<()> {
    let snapshot = match client.monitor_snapshot() {
        Ok(value) => value,
        Err(error) => {
            emit(
                "monitor-observation-failure",
                &[("error", quote(&error.to_string()))],
            );
            return Err(error.into());
        }
    };
    for operation in snapshot.operations.iter().filter(|o| o.id.0 > *after) {
        emit(
            "operation",
            &[
                ("receipt", operation.to_json()),
                ("details", quote(&format!("{operation:?}"))),
            ],
        );
    }
    if let Some(max) = snapshot.operations.iter().map(|o| o.id.0).max() {
        *after = max;
    }
    for diagnostic in layerfs_sdk::take_workspace_commit_diagnostics() {
        emit(
            "commit-diagnostics",
            &[
                ("after_operation_id", after.to_string()),
                ("details", quote(&format!("{diagnostic:?}"))),
            ],
        );
    }
    Ok(())
}

fn resource_receipt(label: &str, snapshot: ProcessResourceSnapshot) {
    emit(
        "host-resources",
        &[
            ("phase", quote(label)),
            ("user_cpu_ns", snapshot.user_cpu_ns.to_string()),
            ("system_cpu_ns", snapshot.system_cpu_ns.to_string()),
            ("resident_bytes", snapshot.resident_bytes.to_string()),
            (
                "peak_resident_bytes",
                snapshot.peak_resident_bytes.to_string(),
            ),
            (
                "physical_footprint_bytes",
                snapshot.physical_footprint_bytes.to_string(),
            ),
            ("disk_read_bytes", snapshot.disk_read_bytes.to_string()),
            ("disk_write_bytes", snapshot.disk_write_bytes.to_string()),
            ("swaps", snapshot.swaps.to_string()),
        ],
    );
}

pub(crate) fn spool_observation(phase: &str) -> AnyResult<()> {
    use std::os::unix::fs::MetadataExt;
    let started = Instant::now();
    let runtime = std::env::temp_dir().join("layerfs-runtime");
    let prefix = format!("{}-", std::process::id());
    let mut roots = Vec::new();
    if runtime.exists() {
        for entry in std::fs::read_dir(&runtime)? {
            let entry = entry?;
            if entry.file_name().to_string_lossy().starts_with(&prefix) {
                let root = entry.path().join("workspaces");
                if root.is_dir() {
                    roots.push(root);
                }
            }
        }
    }
    roots.sort();
    let mut pending = roots.clone();
    let mut logical = 0u64;
    let mut allocated = 0u64;
    let mut files = 0u64;
    while let Some(path) = pending.pop() {
        let metadata = match std::fs::symlink_metadata(&path) {
            Ok(value) => value,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        if metadata.is_dir() {
            for entry in std::fs::read_dir(path)? {
                pending.push(entry?.path());
            }
        } else {
            logical = logical
                .checked_add(metadata.len())
                .ok_or("spool logical overflow")?;
            allocated = allocated
                .checked_add(
                    metadata
                        .blocks()
                        .checked_mul(512)
                        .ok_or("spool allocation overflow")?,
                )
                .ok_or("spool allocation overflow")?;
            files += 1;
        }
    }
    let observer_ns = elapsed_ns(started);
    emit("workspace-spool-observation",&[("phase",quote(phase)),("roots",format!("[{}]",roots.iter().map(|path|quote(&path.to_string_lossy())).collect::<Vec<_>>().join(","))),("scope",quote("owned process Workspace spool/capture/rebase regular files; excludes output logs and directory allocation")),("precision",quote("boundary-observation")),("logical_bytes",logical.to_string()),("allocated_bytes",allocated.to_string()),("file_count",files.to_string()),("observer_ns",observer_ns.to_string())]);
    if logical > 2 * 1024 * 1024 * 1024 || allocated > 2 * 1024 * 1024 * 1024 {
        return Err("owned Workspace spool exceeds frozen 2 GiB bound".into());
    }
    if phase.ends_with("cleanup") && files != 0 {
        return Err("owned Workspace spool files remain after cleanup".into());
    }
    Ok(())
}

fn store_metrics(store: &LayerStackStore, label: &str, step: usize) -> AnyResult<()> {
    use std::os::unix::fs::MetadataExt;
    let meta = std::fs::metadata(store.path())?;
    let (page_size, page_count, freelist) =
        store.inspect_connection(|connection| -> AnyResult<(u64, u64, u64)> {
            let read = |name: &str| -> AnyResult<u64> {
                let value: i64 = connection.pragma_query_value(None, name, |r| r.get(0))?;
                Ok(value.try_into()?)
            };
            Ok((
                read("page_size")?,
                read("page_count")?,
                read("freelist_count")?,
            ))
        })??;
    emit(
        "store-observation",
        &[
            ("phase", quote(label)),
            ("step", step.to_string()),
            ("file_bytes", meta.len().to_string()),
            ("allocated_bytes", (meta.blocks() * 512).to_string()),
            ("page_size_bytes", page_size.to_string()),
            ("page_count", page_count.to_string()),
            ("freelist_page_count", freelist.to_string()),
            (
                "live_page_bytes",
                ((page_count - freelist) * page_size).to_string(),
            ),
        ],
    );
    if meta.len() > 4 * 1024 * 1024 * 1024 || meta.blocks() * 512 > 4 * 1024 * 1024 * 1024 {
        return Err("sample Store exceeds frozen 4 GiB bound".into());
    }
    Ok(())
}

fn native_verify(
    client: &Client,
    workspace: WorkspaceId,
    case: &Case,
    seed: u8,
    step: usize,
) -> AnyResult<()> {
    if case.kind == "git-tool" {
        let output = execute(
            client,
            workspace,
            vec![
                "/usr/local/bin/fs-benchmark-workload".into(),
                "workspace-git-verify".into(),
                case.id.clone().into(),
                seed.to_string().into(),
                "/qualified/git-reference".into(),
            ],
        )?;
        emit(
            "git-semantic-verification",
            &[("receipt", quote(&output_text(&output)?))],
        );
        return Ok(());
    }
    let output = execute(
        client,
        workspace,
        vec![
            OsString::from("/usr/local/bin/fs-benchmark-workload"),
            OsString::from("workspace-verify-tree"),
            OsString::from(&case.id),
            OsString::from(seed.to_string()),
            OsString::from(step.to_string()),
        ],
    )?;
    emit(
        "native-verification",
        &[
            ("step", step.to_string()),
            ("receipt", quote(&output_text(&output)?)),
        ],
    );
    Ok(())
}

fn run_case(
    root: &Path,
    input: &Path,
    case: &Case,
    seed: u8,
    mode: &str,
    container: ContainerId,
) -> AnyResult<()> {
    let verification = mode == "verify";
    if !verification && mode != "performance" {
        return Err("invalid phase1 mode".into());
    }
    emit(
        "sample-start",
        &[
            ("scenario_id", quote(&case.id)),
            ("family_id", quote(case.family)),
            ("seed", seed.to_string()),
            ("mode", quote(mode)),
        ],
    );
    resource_receipt("before", process_resource_snapshot()?);
    let _sampler = HostSampler::start()?;
    let path = root.join("store.sqlite");
    let store = Arc::new(if registry::is_import(case) {
        LayerStackStore::create(&path)?
    } else {
        LayerStackStore::connect(&path)?
    });
    let client = Client::connect(store.clone())?;
    store_metrics(&store, "before", 0)?;
    let mut last_operation = 0;
    let mut final_head = None;
    let branch;
    let mut history = Vec::new();
    let mut genesis_root = None;
    let orchestration_start = Instant::now();
    let mut pure_call_sum_ns = 0u64;
    let mut product_budget = ProductBudget::new(!verification);
    if registry::is_import(case) {
        let name = EntityName::new("phase1-import")?;
        let source = LayerStackInitialization::Directory(input.to_owned());
        product_budget.begin("initialize")?;
        let start = product_budget.start_clock("initialize")?;
        let result = client.initialize_layerstack(name, source);
        let initialize_ns = product_budget.finish_clock(start)?;
        product_budget.end("initialize", initialize_ns, result.as_ref().err().map(ToString::to_string))?;
        let initialized = match result {
            Ok(value) => value,
            Err(error) => {
                emit(
                    "phase-failure",
                    &[
                        ("phase", quote("initialize")),
                        ("elapsed_ns", initialize_ns.to_string()),
                        ("error", quote(&error.to_string())),
                    ],
                );
                let _ = observed(&client, &mut last_operation);
                let _ = store_metrics(&store, "initialization-failure", 0);
                return Err(error.into());
            }
        };
        pure_call_sum_ns = pure_call_sum_ns
            .checked_add(initialize_ns)
            .ok_or("phase sum overflow")?;
        emit(
            "phase",
            &[
                ("phase", quote("initialize")),
                ("elapsed_ns", initialize_ns.to_string()),
                (
                    "genesis_layer_id",
                    quote(&initialized.genesis_layer_id.to_string()),
                ),
            ],
        );
        for receipt in store.take_layerstack_initialization_receipts() {
            emit(
                "initialization-scan",
                &[
                    ("scanned_files", receipt.scanned_files.to_string()),
                    ("scanned_bytes", receipt.scanned_bytes.to_string()),
                ],
            );
        }
        product_budget.check()?;
        observed(&client, &mut last_operation)?;
        store_metrics(&store, "after-initialize", 0)?;
        if !verification {
            if product_budget.cumulative_ns != pure_call_sum_ns { return Err("product budget timing balance".into()); }
            resource_receipt("after", process_resource_snapshot()?);
            let host_orchestration_ns = elapsed_ns(orchestration_start);
            drop(client);
            drop(store);
            spool_observation("final-client-drop-cleanup")?;
            emit(
                "sample-complete",
                &[
                    ("status", quote("pass")),
                    ("host_orchestration_ns", host_orchestration_ns.to_string()),
                    ("orchestration_scope", quote(ORCHESTRATION_SCOPE)),
                    ("pure_call_sum_ns", pure_call_sum_ns.to_string()),
                    (
                        "orchestration_unattributed_ns",
                        host_orchestration_ns
                            .checked_sub(pure_call_sum_ns)
                            .ok_or("orchestration timing balance")?
                            .to_string(),
                    ),
                    ("benchmark_verifier_count", "0".into()),
                    ("benchmark_reopen_count", "0".into()),
                    ("benchmark_injection_count", "0".into()),
                ],
            );
            return Ok(());
        }
        branch = client.fork_branch(
            EntityName::new("verify-import")?,
            LocalForkSource::Layer {
                layer_id: initialized.genesis_layer_id,
            },
        )?;
    } else {
        branch = std::fs::read_to_string(root.join("branch-id"))?
            .trim()
            .parse()?;
        genesis_root = Some(store.pin_branch(branch)?.root);
        let request = CreateWorkspaceSession {
            branch_id: branch,
            placement: case_placement(&Some(container.clone()), root, seed as usize, &case.id),
            projection: Some(WorkspaceProjection::Fuse),
        };
        product_budget.begin("create")?;
        let create_start = product_budget.start_clock("create")?;
        let result = client.create_workspace_session(request);
        let create_ns = product_budget.finish_clock(create_start)?;
        product_budget.end("create", create_ns, result.as_ref().err().map(ToString::to_string))?;
        let session = match result {
            Ok(value) => value,
            Err(error) => {
                emit(
                    "phase-failure",
                    &[
                        ("phase", quote("create")),
                        ("elapsed_ns", create_ns.to_string()),
                        ("error", quote(&error.to_string())),
                    ],
                );
                let _ = observed(&client, &mut last_operation);
                return Err(error.into());
            }
        };
        pure_call_sum_ns = pure_call_sum_ns
            .checked_add(create_ns)
            .ok_or("phase sum overflow")?;
        emit(
            "phase",
            &[
                ("phase", quote("create")),
                ("elapsed_ns", create_ns.to_string()),
            ],
        );
        observed(&client, &mut last_operation)?;
        spool_observation("after-create")?;
        let result = (|| -> AnyResult<()> {
            product_budget.check()?;
            if case.family == "edit_length_changing_capped" {
                let scenario = workload_source::edit_length_changing_capped::sdk_scenario(case)?;
                let replacement = match scenario.replacement_kind {
                    workload_source::sdk_edit_common::ReplacementKind::Zero => {
                        WorkspaceFileReplacement::Zero(scenario.replacement_len)
                    }
                    workload_source::sdk_edit_common::ReplacementKind::Inline => {
                        WorkspaceFileReplacement::Inline(
                            workload_source::sdk_edit_common::replacement_bytes(&scenario),
                        )
                    }
                };
                let edit = WorkspaceFileRangeEdit {
                    workspace_id: session.id,
                    path: "payload.bin".into(),
                    start: scenario.start,
                    delete_len: scenario.delete_len,
                    replacement,
                };
                let guard = PhaseDeadline::start("inherited-sdk-edit-commit-end", 2);
                product_budget.begin("sdk-edit-commit-end")?;
                let result = sdk_file_edit::edit_commit_end_budgeted(&client, edit, &product_budget);
                product_budget.end("sdk-edit-commit-end", product_budget.completed_group_ns()?, result.as_ref().err().map(ToString::to_string))?;
                drop(guard);
                let timing = result?;
                product_budget.begin("visibility")?;
                let result = sdk_file_edit::validate_visibility_budgeted(&client, branch, &timing, &product_budget);
                product_budget.end("visibility", product_budget.completed_group_ns()?, result.as_ref().err().map(ToString::to_string))?;
                let finish = result?;
                for (phase, elapsed) in [
                    ("sdk-edit", timing.edit_call_ns),
                    ("commit", timing.commit_call_ns),
                    ("end", timing.workspace_end_ns),
                    ("visibility", finish.visibility_validation_ns),
                ] {
                    pure_call_sum_ns = pure_call_sum_ns
                        .checked_add(elapsed)
                        .ok_or("phase sum overflow")?;
                    emit(
                        "phase",
                        &[("phase", quote(phase)), ("elapsed_ns", elapsed.to_string())],
                    );
                }
                final_head = Some(timing.commit_id);
                history.push(timing.commit_id);
                observed(&client, &mut last_operation)?;
                store_metrics(&store, "after-capped-edit", 1)?;
                spool_observation("after-capped-end-cleanup")?;
                product_budget.check()?;
                return Ok(());
            }
            for step in 0..registry::steps(case) {
                if case.kind == "workspace-distributed-sdk-edit"
                    || workload_source::dedup_workloads::is_sdk(case)
                {
                    let edits = if case.kind == "workspace-distributed-sdk-edit" {
                        workload_source::ordinary_workloads::sdk_edits(case, seed)?
                    } else {
                        workload_source::dedup_workloads::sdk_edits(case, seed, step)?
                    };
                    for edit in edits {
                        let request = WorkspaceFileRangeEdit {
                            workspace_id: session.id,
                            path: edit.path,
                            start: edit.start,
                            delete_len: edit.delete_len,
                            replacement: WorkspaceFileReplacement::Inline(edit.replacement),
                        };
                        product_budget.begin("sdk-edit")?;
                        let start = product_budget.start_clock("sdk-edit")?;
                        let result = client.edit_workspace_file_range(request);
                        let edit_ns = product_budget.finish_clock(start)?;
                        product_budget.end("sdk-edit", edit_ns, result.as_ref().err().map(ToString::to_string))?;
                        result?;
                        pure_call_sum_ns = pure_call_sum_ns
                            .checked_add(edit_ns)
                            .ok_or("phase sum overflow")?;
                        emit(
                            "phase",
                            &[
                                ("phase", quote("sdk-edit")),
                                ("step", step.to_string()),
                                ("elapsed_ns", edit_ns.to_string()),
                            ],
                        );
                    }
                    observed(&client, &mut last_operation)?;
                } else if case.kind != "workspace-clean-commit" {
                    let argv = vec![
                        OsString::from("/usr/local/bin/fs-benchmark-workload"),
                        OsString::from("workspace-apply"),
                        OsString::from(&case.id),
                        OsString::from(seed.to_string()),
                        OsString::from(step.to_string()),
                        OsString::from(mode),
                    ];
                    product_budget.begin("exec")?;
                    let start = product_budget.start_clock("exec")?;
                    let result = execute(&client, session.id, argv);
                    let execution_ns = product_budget.finish_clock(start)?;
                    product_budget.end("exec", execution_ns, result.as_ref().err().map(ToString::to_string))?;
                    let output = result?;
                    pure_call_sum_ns = pure_call_sum_ns
                        .checked_add(execution_ns)
                        .ok_or("phase sum overflow")?;
                    emit(
                        "phase",
                        &[
                            ("phase", quote("exec")),
                            ("step", step.to_string()),
                            ("elapsed_ns", execution_ns.to_string()),
                            ("workload_receipt", quote(&output_text(&output)?)),
                            ("execution_receipt", quote(&format!("{:?}", output.receipt))),
                        ],
                    );
                }
                spool_observation("after-workload-before-commit")?;
                if verification && case.kind == "git-tool" {
                    let receipt = execute(
                        &client,
                        session.id,
                        vec![
                            "/usr/local/bin/fs-benchmark-workload".into(),
                            "workspace-git-custody-out".into(),
                            "/verification/precommit.tsv".into(),
                        ],
                    )?;
                    emit(
                        "git-precommit-custody",
                        &[("receipt", quote(&output_text(&receipt)?))],
                    );
                }
                product_budget.begin("commit")?;
                let start = product_budget.start_clock("commit")?;
                let result = client.commit_workspace_session_with_status(session.id);
                let elapsed = product_budget.finish_clock(start)?;
                product_budget.end("commit", elapsed, result.as_ref().err().map(ToString::to_string))?;
                let status = result?;
                pure_call_sum_ns = pure_call_sum_ns
                    .checked_add(elapsed)
                    .ok_or("phase sum overflow")?;
                final_head = match status.result {
                    WorkspaceCommitResult::Created { commit_id, .. } => {
                        history.push(commit_id);
                        Some(commit_id)
                    }
                    WorkspaceCommitResult::UpToDate { head } => head,
                    other => return Err(format!("unexpected Commit outcome {other:?}").into()),
                };
                emit(
                    "phase",
                    &[
                        ("phase", quote("commit")),
                        ("step", step.to_string()),
                        ("elapsed_ns", elapsed.to_string()),
                        ("outcome", quote(&format!("{status:?}"))),
                    ],
                );
                if status.presentation_failed {
                    return Err("Commit published with failed FUSE presentation".into());
                }
                {
                    let pinned = store.pin_branch(branch)?;
                    emit(
                        "published-root",
                        &[
                            ("step", step.to_string()),
                            ("root", quote(&pinned.root.to_string())),
                            (
                                "head",
                                quote(&format!("{:?}", pinned.branch.head_commit_id)),
                            ),
                        ],
                    );
                }
                store_metrics(&store, "after-commit", step + 1)?;
                spool_observation("after-commit")?;
                let expected_created = !matches!(
                    case.kind,
                    "workspace-clean-commit"
                        | "payload-random-read"
                        | "tiny-stat"
                        | "directory-metadata-scan"
                        | "directory-content-scan"
                );
                if expected_created
                    != matches!(status.result, WorkspaceCommitResult::Created { .. })
                {
                    return Err("Commit differs from independent expected outcome".into());
                }
                observed(&client, &mut last_operation)?;
            }
            product_budget.begin("visibility")?;
            let start = product_budget.start_clock("visibility")?;
            let result = visible_head(&client, branch, final_head);
            let visibility_ns = product_budget.finish_clock(start)?;
            product_budget.end("visibility", visibility_ns, result.as_ref().err().map(ToString::to_string))?;
            result?;
            pure_call_sum_ns = pure_call_sum_ns
                .checked_add(visibility_ns)
                .ok_or("phase sum overflow")?;
            emit(
                "phase",
                &[
                    ("phase", quote("visibility")),
                    ("elapsed_ns", visibility_ns.to_string()),
                ],
            );
            product_budget.begin("end")?;
            let start = product_budget.start_clock("end")?;
            let result = client.end_workspace_session(session.id, EndWorkspaceMode::Clean);
            let end_ns = product_budget.finish_clock(start)?;
            product_budget.end("end", end_ns, result.as_ref().err().map(ToString::to_string))?;
            result?;
            pure_call_sum_ns = pure_call_sum_ns
                .checked_add(end_ns)
                .ok_or("phase sum overflow")?;
            emit(
                "phase",
                &[("phase", quote("end")), ("elapsed_ns", end_ns.to_string())],
            );
            spool_observation("after-end-cleanup")?;
            product_budget.check()?;
            Ok(())
        })();
        if let Err(error) = result {
            let _ = observed(&client, &mut last_operation);
            let _ = store_metrics(&store, "failure", history.len());
            let _ = spool_observation("failure-before-discard");
            let physical = physical_spool_state(&client, session.id, "failure-before-discard");
            let recovery = client.end_workspace_session(session.id, EndWorkspaceMode::Discard);
            emit(
                "recovery",
                &[
                    ("status", quote(&format!("{recovery:?}"))),
                    ("original_error", quote(&error.to_string())),
                ],
            );
            let _ = observed(&client, &mut last_operation);
            let _ = spool_observation("failure-after-discard-cleanup");
            if let Err(observation_error) = physical {
                emit(
                    "required-observation-failure",
                    &[
                        ("observation", quote("physical-spool-before-discard")),
                        ("error", quote(&observation_error.to_string())),
                    ],
                );
            }
            return Err(error);
        }
    }
    if product_budget.cumulative_ns != pure_call_sum_ns { return Err("product budget timing balance".into()); }
    let host_orchestration_ns = elapsed_ns(orchestration_start);
    observed(&client, &mut last_operation)?;
    resource_receipt("after-product", process_resource_snapshot()?);
    if client.active_workspace_count()? != 0 || client.active_execution_count()? != 0 {
        return Err("owned runtime retained after End".into());
    }
    drop(client);
    drop(store);
    spool_observation("after-client-drop-cleanup")?;
    if verification {
        let reopened = Arc::new(LayerStackStore::connect(&path)?);
        store_metrics(
            &reopened,
            "after-reconnect-before-verifier-forks",
            history.len(),
        )?;
        let client = Client::connect(reopened.clone())?;
        let mut verifier_operation = 0;
        if case.family != "dedup_branch_history" {
            let mut expected = registry::expected(case, seed, registry::steps(case))?;
            if case.kind == "git-tool" {
                let manifest = std::fs::read_to_string(
                    Path::new(&std::env::var("LAYERFS_V013_VERIFIER_EXCHANGE_HOST")?)
                        .join("precommit.tsv"),
                )?;
                expected.extend(
                    common::decode_manifest(&manifest)?
                        .into_iter()
                        .filter(|e| e.path != "."),
                );
            }
            let receipt = super::workspace_verify::verify(&reopened, branch, &expected, root)?;
            emit(
                "canonical-verification",
                &[("receipt", quote(&format!("{:?}", receipt.receipt)))],
            );
            if case.family.starts_with("dedup_") {
                let dedup = if case.kind == "boundaries" {
                    super::dedup_verify::verify_boundaries(&receipt)?
                } else {
                    super::dedup_verify::verify_transcripts(
                        case,
                        seed,
                        registry::steps(case),
                        &receipt,
                    )?
                };
                emit(
                    "dedup-verification",
                    &[("receipt", quote(&format!("{dedup:?}")))],
                );
            }
            if case.family == "edit_length_changing_capped" {
                let capped = super::dedup_verify::verify_capped(case, &receipt)?;
                emit(
                    "capped-verification",
                    &[("receipt", quote(&format!("{capped:?}")))],
                );
            }
            let session = client.create_workspace_session(CreateWorkspaceSession {
                branch_id: branch,
                placement: case_placement(
                    &Some(container.clone()),
                    root,
                    seed as usize,
                    &format!("{}-reopen", case.id),
                ),
                projection: Some(WorkspaceProjection::Fuse),
            })?;
            if case.kind == "git-tool" {
                let receipt = execute(
                    &client,
                    session.id,
                    vec![
                        "/usr/local/bin/fs-benchmark-workload".into(),
                        "workspace-git-custody-out".into(),
                        "/verification/reopened.tsv".into(),
                    ],
                )?;
                emit(
                    "git-reopen-custody",
                    &[("receipt", quote(&output_text(&receipt)?))],
                );
                let exchange = PathBuf::from(std::env::var("LAYERFS_V013_VERIFIER_EXCHANGE_HOST")?);
                if std::fs::read(exchange.join("precommit.tsv"))?
                    != std::fs::read(exchange.join("reopened.tsv"))?
                {
                    return Err("Git complete precommit/reopen custody mismatch".into());
                }
            }
            native_verify(&client, session.id, case, seed, registry::steps(case))?;
            // Git semantic commands can refresh the verifier's index after the
            // immutable precommit/reopen custody comparison. Never publish them.
            client.end_workspace_session(
                session.id,
                if case.kind == "git-tool" {
                    EndWorkspaceMode::Discard
                } else {
                    EndWorkspaceMode::Clean
                },
            )?;
            observed(&client, &mut verifier_operation)?;
        }
        if case.family == "dedup_branch_history" {
            let mut query = Query::new(QueryKind::Commits).limit(127);
            let mut records = BTreeMap::new();
            loop {
                let page = client.query(query.clone())?;
                for item in &page.items {
                    if let QueryItem::Commit(commit) = item {
                        if records.insert(commit.id, commit.clone()).is_some() {
                            return Err("duplicate historical Commit query row".into());
                        }
                    }
                }
                let Some(next) = page.into_next_query(&query) else {
                    break;
                };
                query = next;
            }
            let expected_ids = history
                .iter()
                .copied()
                .collect::<std::collections::BTreeSet<_>>();
            if history.len() != case.tier
                || expected_ids.len() != case.tier
                || records
                    .keys()
                    .copied()
                    .collect::<std::collections::BTreeSet<_>>()
                    != expected_ids
            {
                return Err("history Commit query exact membership".into());
            }
            observed(&client, &mut verifier_operation)?;
            let mut accounting = super::dedup_verify::HistoryAccounting::default();
            let genesis_root = genesis_root.ok_or("missing history genesis root")?;
            let genesis_entries = registry::fixture(case, seed)?;
            let mut genesis = super::workspace_verify::verify_root(
                &reopened.snapshot_reader(genesis_root),
                genesis_root,
                &genesis_entries,
            )?;
            super::workspace_verify::persist_snapshot(
                &genesis_entries,
                &mut genesis,
                &root.join("history-0"),
            )?;
            emit(
                "history-canonical",
                &[
                    ("step", "0".into()),
                    ("root", quote(&genesis_root.to_string())),
                    ("receipt", quote(&format!("{:?}", genesis.receipt))),
                ],
            );
            let genesis_transcript =
                super::dedup_verify::verify_transcripts(case, seed, 0, &genesis)?;
            emit(
                "history-transcript",
                &[
                    ("step", "0".into()),
                    ("receipt", quote(&format!("{genesis_transcript:?}"))),
                ],
            );
            super::workspace_verify::write_gzip(
                &root.join("history-0/canonical-verification/history-canonical-union.tsv.gz"),
                |canonical_rows| {
                    writeln!(canonical_rows, "step\troot\tobject_id\trole\tcanonical_bytes\tregular_file\tmetadata_value")?;
                    emit(
                        "history-accounting",
                        &[
                            ("step", "0".into()),
                            (
                                "receipt",
                                quote(&format!(
                                    "{:?}",
                                    accounting.observe(case, 0, &genesis, canonical_rows)?
                                )),
                            ),
                        ],
                    );
                    let base_layer = records[&history[0]].base_layer_id;
                    let base_branch = client.fork_branch(
                        EntityName::new("verify-genesis")?,
                        LocalForkSource::Layer {
                            layer_id: base_layer,
                        },
                    )?;
                    let base_session = client.create_workspace_session(CreateWorkspaceSession {
                        branch_id: base_branch,
                        placement: case_placement(
                            &Some(container.clone()),
                            root,
                            seed as usize,
                            "history-genesis",
                        ),
                        projection: Some(WorkspaceProjection::Fuse),
                    })?;
                    native_verify(&client, base_session.id, case, seed, 0)?;
                    client.end_workspace_session(base_session.id, EndWorkspaceMode::Clean)?;
                    observed(&client, &mut verifier_operation)?;
                    for (offset, commit_id) in history.iter().enumerate() {
                        let step = offset + 1;
                        let record = &records[commit_id];
                        let expected_parent = if offset == 0 {
                            None
                        } else {
                            Some(history[offset - 1])
                        };
                        if record.parent_commit_id != expected_parent {
                            return Err("history parent topology".into());
                        }
                        let fork = client.fork_branch(
                            EntityName::new(format!("verify-history-{step}"))?,
                            LocalForkSource::Branch {
                                branch_id: branch,
                                commit_id: *commit_id,
                            },
                        )?;
                        let expected = registry::expected(case, seed, step)?;
                        let snapshot = super::workspace_verify::verify(
                            &reopened,
                            fork,
                            &expected,
                            &root.join(format!("history-{step}")),
                        )?;
                        emit(
                            "history-canonical",
                            &[
                                ("step", step.to_string()),
                                (
                                    "root",
                                    quote(
                                        snapshot
                                            .receipt
                                            .get("canonical_root")
                                            .ok_or("historical canonical root")?,
                                    ),
                                ),
                                ("receipt", quote(&format!("{:?}", snapshot.receipt))),
                            ],
                        );
                        let transcript =
                            super::dedup_verify::verify_transcripts(case, seed, step, &snapshot)?;
                        emit(
                            "history-transcript",
                            &[
                                ("step", step.to_string()),
                                ("receipt", quote(&format!("{transcript:?}"))),
                            ],
                        );
                        emit(
                            "history-accounting",
                            &[
                                ("step", step.to_string()),
                                ("commit_id", quote(&commit_id.to_string())),
                                (
                                    "receipt",
                                    quote(&format!(
                                        "{:?}",
                                        accounting.observe(
                                            case,
                                            step,
                                            &snapshot,
                                            canonical_rows
                                        )?
                                    )),
                                ),
                            ],
                        );
                        let historical =
                            client.create_workspace_session(CreateWorkspaceSession {
                                branch_id: fork,
                                placement: case_placement(
                                    &Some(container.clone()),
                                    root,
                                    seed as usize,
                                    &format!("{}-history-{step}", case.id),
                                ),
                                projection: Some(WorkspaceProjection::Fuse),
                            })?;
                        native_verify(&client, historical.id, case, seed, step)?;
                        client.end_workspace_session(historical.id, EndWorkspaceMode::Clean)?;
                        observed(&client, &mut verifier_operation)?;
                    }
                    Ok(())
                },
            )?;
        }
        observed(&client, &mut verifier_operation)?;
        if client.active_workspace_count()? != 0 || client.active_execution_count()? != 0 {
            return Err("verifier runtime retained after End".into());
        }
        drop(client);
        drop(reopened);
        emit("verification-complete", &[("status", quote("pass"))]);
    }
    spool_observation("final-client-drop-cleanup")?;
    emit(
        "sample-complete",
        &[
            ("status", quote("pass")),
            ("host_orchestration_ns", host_orchestration_ns.to_string()),
            ("orchestration_scope", quote(ORCHESTRATION_SCOPE)),
            ("pure_call_sum_ns", pure_call_sum_ns.to_string()),
            (
                "orchestration_unattributed_ns",
                host_orchestration_ns
                    .checked_sub(pure_call_sum_ns)
                    .ok_or("orchestration timing balance")?
                    .to_string(),
            ),
            ("created_commit_count", history.len().to_string()),
            (
                "benchmark_verifier_count",
                if verification { "1" } else { "0" }.into(),
            ),
            (
                "benchmark_reopen_count",
                if verification { "1" } else { "0" }.into(),
            ),
            ("benchmark_injection_count", "0".into()),
        ],
    );
    Ok(())
}

pub(crate) fn dispatch(args: &[OsString]) -> AnyResult<()> {
    let args = args
        .iter()
        .map(|s| s.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    match args.as_slice() {
        [command, root] if command == "workspace-qualify-digests" => {
            let receipt = super::workspace_verify::digest_qualification(Path::new(root))?;
            emit(
                "digest-qualification",
                &[
                    ("status", quote("pass")),
                    ("receipt", quote(&format!("{receipt:?}"))),
                ],
            );
            Ok(())
        }
        [command, root, prepared, id, container] if command == "workspace-reliability-run" => {
            runtime_observation_window(id, "verify", || {
                super::workspace_reliability::run(
                    Path::new(root),
                    Path::new(prepared),
                    id,
                    ContainerId(container.clone()),
                )
            })
        }
        [command, root] if command == "workspace-qualify-verifiers" => {
            let receipt = super::workspace_verify::qualification(Path::new(root))?;
            emit(
                "verifier-qualification",
                &[
                    ("status", quote("pass")),
                    ("receipt", quote(&format!("{receipt:?}"))),
                ],
            );
            Ok(())
        }
        [command] if command == "workspace-static-additions-check" => {
            workload_source::edit_length_changing_capped::self_check()?;
            workload_source::workspace_reliability::self_check()?;
            emit(
                "static-additions-check",
                &[
                    ("status", quote("pass")),
                    ("inherited_capped_cases", "5".into()),
                    ("inherited_performance_slots", "25".into()),
                    ("reliability_subcases", "28".into()),
                ],
            );
            Ok(())
        }
        [command] if command == "workspace-self-check" => {
            registry::self_check()?;
            emit(
                "self-check",
                &[
                    ("status", quote("pass")),
                    ("timed_case_count", "130".into()),
                    ("sample_slot_count", "390".into()),
                ],
            );
            Ok(())
        }
        [command] if command == "workspace-registry" => {
            for case in registry::cases()
                .into_iter()
                .chain(registry::proofs())
                .chain(registry::inherited())
            {
                let proof = case.family == "workspace_reliability" || case.kind == "boundaries";
                emit(
                    "case",
                    &[
                        (
                            "inherited",
                            (case.family == "edit_length_changing_capped").to_string(),
                        ),
                        ("proof_only", proof.to_string()),
                        ("scenario_id", quote(&case.id)),
                        ("family_id", quote(case.family)),
                        ("tier", case.tier.to_string()),
                        ("operation", quote(case.kind)),
                        (
                            "input_mode",
                            quote(if registry::is_import(&case) {
                                "directory"
                            } else {
                                "store"
                            }),
                        ),
                    ],
                );
            }
            Ok(())
        }
        [command, id, seed] if command == "workspace-reference-info" => {
            reference_info(&registry::resolve(id)?, seed.parse()?)
        }
        [command, root, id, seed] if command == "workspace-reference-prepare" => {
            prepare_reference(Path::new(root), &registry::resolve(id)?, seed.parse()?)
        }
        [command, id, seed] if command == "workspace-fixture-info" => {
            fixture_info(&registry::resolve(id)?, seed.parse()?, None)
        }
        [command, root, id, seed] if command == "workspace-prepare" => {
            prepare(Path::new(root), &registry::resolve(id)?, seed.parse()?)
        }
        [command, root, input, id, seed, mode, container] if command == "workspace-run" => {
            let case = registry::resolve(id)?;
            runtime_observation_window(id, mode, || {
                if case.family == "workspace_reliability" {
                    if mode != "verify" || seed != "1" {
                        return Err("reliability requires verify mode and fixed seed1".into());
                    }
                    resource_receipt("before-proof", process_resource_snapshot()?);
                    let _sampler = HostSampler::start()?;
                    let outcome = super::workspace_reliability::run(
                        Path::new(root),
                        Path::new(input).parent().ok_or("prepared directory")?,
                        id,
                        ContainerId(container.clone()),
                    );
                    resource_receipt("after-proof", process_resource_snapshot()?);
                    outcome
                } else {
                    if case.kind == "boundaries" && (mode != "verify" || seed != "1") {
                        return Err("CDC proof requires verify and aggregate seed1 selector".into());
                    }
                    run_case(
                        Path::new(root),
                        Path::new(input),
                        &case,
                        seed.parse()?,
                        mode,
                        ContainerId(container.clone()),
                    )
                }
            })
        }
        _ => Err("invalid phase1 host arguments".into()),
    }
}

#[cfg(test)]
mod product_budget_tests {
    use super::*;

    #[test]
    fn live_watchdog_stops_at_remaining_cumulative_budget() {
        const CHILD: &str = "LAYERFS_BUDGET_WATCHDOG_TEST_CHILD";
        if std::env::var_os(CHILD).is_some() {
            let mut budget = ProductBudget::new(true);
            budget.cumulative_ns = PRODUCT_TIME_LIMIT_NS - 10_000_000;
            budget.state.lock().unwrap().completed_ns = budget.cumulative_ns;
            budget.begin("watchdog-test").unwrap();
            let _start = budget.start_clock("watchdog-test").unwrap();
            std::thread::sleep(std::time::Duration::from_millis(50));
            // A broken watchdog fails promptly; no long phase or workload runs.
            std::process::exit(86);
        }
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["workspace_bench::product_budget_tests::live_watchdog_stops_at_remaining_cumulative_budget", "--exact", "--nocapture"])
            .env(CHILD, "1").output().unwrap();
        let stdout = String::from_utf8(output.stdout).unwrap();
        print!("{stdout}");
        assert_eq!(output.status.code(), Some(124), "{}", String::from_utf8_lossy(&output.stderr));
        assert!(stdout.contains("\"kind\":\"product-time-budget-exceeded\""));
        assert!(stdout.contains("\"measurement\":\"active-pure-call-sum\""));
        assert!(stdout.contains("\"completed_product_ns\":14990000000"));
    }

    #[test]
    fn cumulative_budget_keeps_prior_phases_and_exact_boundary() {
        // Fake phase durations: no product work, sleeping, preparation or verifier.
        let mut budget = ProductBudget::new(true);
        for (phase, duration) in [("create", 2_000_000_000), ("exec", 7_000_000_000),
            ("commit", 5_000_000_000), ("end", 1_000_000_000)] {
            budget.begin(phase).unwrap();
            budget.end(phase, duration, None).unwrap();
        }
        assert_eq!(budget.cumulative_ns, PRODUCT_TIME_LIMIT_NS);
        assert!(budget.check().is_ok());
        // The active phase's remaining allowance includes all previous calls.
        let state = ProductBudgetState { completed_ns: 14_000_000_000, active: None };
        assert_eq!(state.total_at(1_000_000_000), PRODUCT_TIME_LIMIT_NS);
        assert!(state.total_at(1_000_000_001) > PRODUCT_TIME_LIMIT_NS);
        budget.end("visibility", 1, None).unwrap();
        assert!(budget.begin("next-step").is_err());
        // Verification/proof mode keeps accounting but cannot arm a deadline.
        let mut disabled = ProductBudget::new(false);
        assert!(disabled._deadline.is_none());
        disabled.end("verify", PRODUCT_TIME_LIMIT_NS + 1, None).unwrap();
        assert!(disabled.begin("cleanup").is_ok());
    }
}
