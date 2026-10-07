//! One bounded uncontrolled E04 attempt; original failed owners remain in custody.
use super::{
    digest::{hex, sha256},
    fixture::{self, Config, Fixture},
    json::Json,
    observed, oracle,
    ports::Observed,
    records::{self, JobInfo},
    streams::{Input, Recorder},
};
use layerfs_bridge::codec::Message;
use layerfs_daemon::upstream::UpstreamOperation;
use layerfs_daemon::{Command, Completion, Owner, OwnerConfig, OwnerStart, Response, ServiceClass};
use layerfs_overlay::{BaseSource, CreationWork, ProfileConfig, Publication};
use layerfs_sdk::client::ConsumerFence;
use layerfs_workspace::{Operation, Outcome, Position, Time};
use std::{
    cell::RefCell,
    error::Error,
    ffi::OsString,
    fmt, io,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

const WRITES: u64 = 1000;
const FINAL_TIME: Time = Time {
    seconds: -7,
    nanoseconds: 42,
};
type Result<T> = std::result::Result<T, Box<dyn Error>>;
pub struct Failure {
    pub cause: Box<dyn Error>,
    pub report_failure: Option<Box<dyn Error>>,
    pub owner: Option<Owner>,
    pub fixture: Option<Fixture>,
    pub operation: Option<UpstreamOperation>,
    pub source: Option<BaseSource>,
    pub bootstrap: Option<(Completion, Message, Message)>,
    pub native_fence: Option<ConsumerFence>,
    pub startup: Option<Arc<CreationWork>>,
    pub original_startup_error: Option<layerfs_daemon::OwnerError>,
    pub recorder: Option<Rc<RefCell<Recorder>>>,
}
impl fmt::Debug for Failure {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut view = out.debug_struct("OriginalE04Failure");
        view.field("cause", &self.cause)
            .field("secondary_report_failure", &self.report_failure)
            .field("original_startup_error", &self.original_startup_error)
            .field("owner_retained", &self.owner.is_some())
            .field("fixture_retained", &self.fixture.is_some())
            .field("operation", &self.operation)
            .field("original_source_token", &self.source)
            .field("bootstrap_retained", &self.bootstrap.is_some())
            .field("fence_retained", &self.native_fence.is_some());
        if let Some(records) = &self.recorder {
            let r = records.borrow();
            view.field("output", &r.root)
                .field("startup_acknowledged_bytes", &r.startup.bytes)
                .field("jobs_acknowledged_bytes", &r.jobs.bytes)
                .field("trace_acknowledged_records", &r.trace.records);
        }
        view.finish()
    }
}
impl Failure {
    fn new(cause: Box<dyn Error>) -> Self {
        Self {
            cause,
            report_failure: None,
            owner: None,
            fixture: None,
            operation: None,
            source: None,
            bootstrap: None,
            native_fence: None,
            startup: None,
            original_startup_error: None,
            recorder: None,
        }
    }
}
pub fn collect(args: &[OsString]) -> std::result::Result<(), Failure> {
    let input = Input::parse(args).map_err(|error| Failure::new(Box::new(error)))?;
    let recorder = Recorder::create(&input).map_err(|error| Failure::new(Box::new(error)))?;
    let mut state = Failure::new(Box::new(io::Error::other("original E04 attempt")));
    state.recorder = Some(Rc::new(RefCell::new(recorder)));
    match run(&mut state, &input) {
        Ok(()) => Ok(()),
        Err(original) => {
            state.cause = original;
            // A healthy recorder may describe the actual failed prefix. An
            // already failed writer is never reopened or used for a replay.
            let message = format!("{}", state.cause);
            if let Some(recorder) = &state.recorder {
                let mut r = recorder.borrow_mut();
                if r.healthy() {
                    if let Err(error) = finish(&mut r, "FAILED", Some(&message)) {
                        if state.report_failure.is_none() {
                            state.report_failure = Some(Box::new(error));
                        }
                    }
                }
            }
            Err(state)
        }
    }
}
use super::outcomes::finish;
fn run(state: &mut Failure, input: &Input) -> Result<()> {
    let recorder = state.recorder.as_ref().expect("original recorder").clone();
    let opened = recorder.borrow().at();
    let started =
        Owner::start_observed(&input.db, ProfileConfig::default(), OwnerConfig::default());
    state.startup = Some(started.startup.clone());
    let recorded = (|| -> io::Result<()> {
        let mut r = recorder.borrow_mut();
        let mut out = Json::new();
        r.header(&mut out, "startup", 0)?;
        out.raw(",")?;
        out.field("opened_ns", opened)?;
        out.raw(",")?;
        out.field("closed_ns", r.at())?;
        out.raw(",\"clock\":\"process-local-Instant-elapsed\",\"configuration\":")?;
        observed::configuration(&mut out, ProfileConfig::default(), OwnerConfig::default())?;
        observed::startup(&mut out, &started)?;
        out.raw("}")?;
        r.append("startup", out.finish()?)
    })();
    let OwnerStart { result, .. } = started;
    match result {
        Ok(owner) => {
            state.owner = Some(owner);
            recorder.borrow_mut().startup_status = "READY";
        }
        Err(original) => {
            state.original_startup_error = Some(original);
            recorder.borrow_mut().startup_status = "FAILED";
        }
    }
    recorded?;
    if let Some(original) = state.original_startup_error.take() {
        return Err(Box::new(original));
    }
    let client = state.owner.as_ref().expect("ready owner").client();
    records::owner(&mut recorder.borrow_mut(), &client, "before-attach")?;
    let attach_opened = recorder.borrow().at();
    let bootstrap_index = recorder.borrow().jobs.records;
    let bootstrap_record_id = recorder.borrow().id("owner-job", bootstrap_index);
    let bootstrap_job_id = recorder.borrow().id("job", bootstrap_index);
    let config = Config {
        endpoint: input.invocation[1].clone(),
        assignment: input.assignment.clone(),
        base_input: input.base.clone(),
        replacements: input.replacements.clone(),
    };
    state.fixture = Some(fixture::attach(&config, client.clone())?);
    let fixture = state.fixture.as_mut().expect("original fixture");
    recorder.borrow_mut().global_profile = Some(match fixture.upstream.expected().persistence {
        layerfs_persistence::SqlitePersistenceProfile::Durable => "durable",
        layerfs_persistence::SqlitePersistenceProfile::Disposable => "disposable",
    });
    {
        let mut out = Json::new();
        out.raw("{")?;
        out.field("route_namespace", fixture.upstream.route().namespace())?;
        out.raw(",")?;
        out.field("serial", fixture.serial)?;
        out.raw(",")?;
        out.text(
            "base_root",
            &hex(&fixture
                .upstream
                .expected()
                .snapshot
                .effective_root
                .to_bytes()),
        )?;
        out.raw(",")?;
        out.text(
            "file_root",
            &hex(&fixture.stat.value.content_root.to_bytes()),
        )?;
        out.raw(",")?;
        out.field("mode", fixture.stat.metadata.mode)?;
        out.raw(",")?;
        out.field("mtime_seconds", fixture.stat.metadata.mtime_seconds)?;
        out.raw(",")?;
        out.field("mtime_nanoseconds", fixture.stat.metadata.mtime_nanoseconds)?;
        out.raw(",")?;
        out.field("nlink", fixture.stat.value.namespace_ref_count)?;
        out.raw(",")?;
        out.field("logical_len", fixture.stat.logical_len)?;
        out.raw(",\"acquisition\":{")?;
        out.text("base_sha256", &fixture.acquisition.base_sha256)?;
        out.raw(",")?;
        out.field(
            "source_allocated_bytes",
            fixture.acquisition.source_allocated_bytes,
        )?;
        out.raw(",")?;
        out.field(
            "source_copied_bytes",
            fixture.acquisition.source_copied_bytes,
        )?;
        out.raw(",")?;
        out.field("source_removed", fixture.acquisition.source_removed)?;
        out.raw("}}")?;
        recorder.borrow_mut().fixture_context = Some(String::from_utf8(out.finish()?)?);
    }
    state.bootstrap = fixture.take_attach_receipts();
    let (open, binding, policy) = state
        .bootstrap
        .as_ref()
        .ok_or("actual attach receipts unavailable")?;
    {
        let mut r = recorder.borrow_mut();
        let info = JobInfo {
            record_id: bootstrap_record_id,
            external_job_id: bootstrap_job_id,
            name: "Open",
            class: ServiceClass::Lifecycle,
            phase: "setup",
            operation: None,
            route: None,
            source: None,
            publication: None,
            opened: attach_opened,
            span_scope: "external-attach-inclusive-boundary-exact-Open-JobWork",
        };
        let row = records::job(&r, r.jobs.records, &info, Some(open), "completed", None)?;
        r.append("jobs", row)?;
        let mut out = Json::new();
        r.header(&mut out, "bootstrap-messages", r.probes.records)?;
        out.raw(",")?;
        out.field("binding_bytes", binding.bytes().len())?;
        out.raw(",")?;
        out.field("policy_bytes", policy.bytes().len())?;
        out.raw(",")?;
        out.text("binding_sha256", &hex(&sha256(binding.bytes())))?;
        out.raw(",")?;
        out.text("policy_sha256", &hex(&sha256(policy.bytes())))?;
        out.raw(
            ",\"scope\":\"original-credited-Binding-Policy-replies-retained-through-record\"}",
        )?;
        r.append("probes", out.finish()?)?;
    }
    drop(state.bootstrap.take());
    records::owner(&mut recorder.borrow_mut(), &client, "before-writes")?;
    for index in 0..WRITES {
        write(state, index)?;
    }
    records::owner(&mut recorder.borrow_mut(), &client, "after-writes")?;
    state.operation = Some(
        state
            .fixture
            .as_ref()
            .expect("fixture")
            .upstream
            .operation()
            .map_err(super::ports::ScopeRefusal)?,
    );
    let operation = state
        .operation
        .as_ref()
        .expect("original verification operation");
    let ports = Observed::new(
        client.clone(),
        &recorder,
        Some(operation.client()),
        "verification",
        None,
    );
    state.source = Some(ports.acquire(operation.workspace().route(), 1001)?);
    let source = state.source.expect("original source");
    let view = operation.workspace().view_for_source(source)?;
    let report = oracle::verify(
        state.fixture.as_ref().expect("fixture"),
        &view,
        &ports,
        FINAL_TIME,
    )?;
    {
        let mut summary = Json::new();
        summary.raw("{\"status\":\"PASS\",")?;
        summary.field("bytes", report.bytes)?;
        summary.raw(",")?;
        summary.text("final_sha256", &report.final_sha256)?;
        summary.raw(",")?;
        summary.text("expected_sha256", &report.expected_sha256)?;
        summary.raw(",")?;
        summary.field("file_mode", report.mode)?;
        summary.raw(",")?;
        summary.field("nlink", report.nlink)?;
        summary.raw(",")?;
        summary.field("mtime_seconds", report.mtime_seconds)?;
        summary.raw(",")?;
        summary.field("mtime_nanoseconds", report.mtime_nanoseconds)?;
        summary.raw(",")?;
        summary.field(
            "old_file_content_root_unchanged",
            report.old_file_content_root_unchanged,
        )?;
        summary.raw(",")?;
        summary.field("binding_unchanged", report.binding_unchanged)?;
        summary.raw(",")?;
        summary.field(
            "namespace_membership_checked",
            report.namespace_membership_checked,
        )?;
        summary.raw(",")?;
        summary.field("root_serial", report.root_serial)?;
        summary.raw(",")?;
        summary.field("namespace_entries", report.namespace_entries)?;
        summary.raw(",\"binding_scope\":\"captured-runtime-binding-not-current-history-query\",\"eof_checked\":true}")?;
        recorder.borrow_mut().oracle_summary = Some(String::from_utf8(summary.finish()?)?);
        let mut r = recorder.borrow_mut();
        let mut out = Json::new();
        r.header(&mut out, "independent-oracle", r.probes.records)?;
        out.raw(",\"status\":\"PASS\",\"scope\":\"separate-in-process-independent-full-state-algorithm-not-separate-process-or-performance-admission\",")?;
        out.field("bytes", report.bytes)?;
        out.raw(",")?;
        out.text("final_sha256", &report.final_sha256)?;
        out.raw(",")?;
        out.text("expected_sha256", &report.expected_sha256)?;
        out.raw(",")?;
        out.field("file_mode", report.mode)?;
        out.raw(",")?;
        out.field("nlink", report.nlink)?;
        out.raw(",")?;
        out.field("mtime_seconds", report.mtime_seconds)?;
        out.raw(",")?;
        out.field("mtime_nanoseconds", report.mtime_nanoseconds)?;
        out.raw(",")?;
        out.field(
            "old_file_content_root_unchanged",
            report.old_file_content_root_unchanged,
        )?;
        out.raw(",")?;
        out.field("binding_unchanged", report.binding_unchanged)?;
        out.raw(",")?;
        out.field(
            "namespace_membership_checked",
            report.namespace_membership_checked,
        )?;
        out.raw(",")?;
        out.field("root_serial", report.root_serial)?;
        out.raw(",")?;
        out.field("namespace_entries", report.namespace_entries)?;
        out.raw(",\"binding_scope\":\"captured-runtime-binding-not-current-history-query\"}")?;
        r.append("probes", out.finish()?)?;
    }
    drop(view);
    ports.release(source)?;
    state.source = None;
    drop(ports);
    state.operation = None;
    let cleanup = Observed::new(client.clone(), &recorder, None, "cleanup", None);
    cleanup.done(
        state.fixture.as_ref().expect("fixture").upstream.route(),
        Command::Close,
        "Close",
        None,
    )?;
    // No maintenance/status/mutation pump in this declared idle interval.
    // This is a diagnostic wait, not a numerical drain guarantee.
    std::thread::sleep(Duration::from_millis(250));
    let route = state.fixture.as_ref().expect("fixture").upstream.route();
    let done = cleanup.command(
        route,
        Command::CleanupState,
        "CleanupState",
        ServiceClass::Lifecycle,
        None,
    )?;
    if !matches!(
        done.result(),
        Ok(Response::CleanupState(layerfs_overlay::CleanupState::Gone))
    ) {
        return Err(Box::new(super::ports::OriginalFailure::Shape(Box::new(
            done,
        ))));
    }
    drop(done);
    let fixture = state.fixture.as_mut().expect("fixture");
    fixture.upstream.fence();
    let deadline = Instant::now() + Duration::from_secs(5);
    state.native_fence = Some(loop {
        if let Some(value) = fixture.upstream.try_join()? {
            break value;
        }
        if Instant::now() >= deadline {
            return Err("original attachment completion exceeded external5s fence wait".into());
        }
        std::thread::sleep(Duration::from_millis(1));
    });
    let fence = state.native_fence.as_ref().expect("original fence");
    if fence.close.is_err() || !fence.partial.is_empty() {
        return Err("original native fence returned error/partial custody".into());
    }
    let receive = fence
        .receive
        .as_ref()
        .map_err(|error| Box::new(io::Error::other(format!("{error:?}"))) as Box<dyn Error>)?;
    if receive.live_messages != 0 || receive.credited_bytes != 0 {
        return Err("original native reply credits remain held".into());
    }
    {
        let mut r = recorder.borrow_mut();
        let mut out = Json::new();
        r.header(&mut out, "native-fence", r.probes.records)?;
        out.raw(",\"scope\":\"attachment-lifetime-framing-native-reassembly-not-exclusive-physical-io\",")?;
        out.field("partial_messages", fence.partial.len())?;
        out.raw(",")?;
        out.field("live_messages", receive.live_messages)?;
        out.raw(",")?;
        out.field("credited_bytes", receive.credited_bytes)?;
        out.raw(",")?;
        out.field("id_copied_bytes", fence.id_copied_bytes)?;
        out.raw(",")?;
        out.text("send_framing_debug", &format!("{:?}", fence.send_framing))?;
        out.raw(",")?;
        out.text("send_native_debug", &format!("{:?}", fence.send_native))?;
        out.raw(",")?;
        out.text(
            "receive_native_debug",
            &format!("{:?}", fence.receive_native),
        )?;
        out.raw(",")?;
        out.text("receive_debug", &format!("{receive:?}"))?;
        out.raw("}")?;
        r.append("probes", out.finish()?)?;
    }
    drop(state.native_fence.take());
    records::owner(&mut recorder.borrow_mut(), &client, "pre-stop")?;
    state.fixture = None;
    let stopped = state.owner.take().expect("original owner").stop();
    recorder.borrow_mut().stop_status = if stopped.is_ok() { "STOPPED" } else { "FAILED" };
    stopped?;
    finish(&mut recorder.borrow_mut(), "RECORDED", None)?;
    Ok(())
}
fn write(state: &mut Failure, index: u64) -> Result<()> {
    let recorder = state.recorder.as_ref().expect("recorder").clone();
    let opened = recorder.borrow().at();
    let record_id = recorder.borrow().id("write-operation", index);
    let attempt_id = recorder.borrow().id("public-write", index);
    recorder.borrow_mut().write_attempts = index + 1;
    let input = state
        .fixture
        .as_ref()
        .expect("fixture")
        .write_input(index as usize)?;
    let input_hash = hex(&sha256(&input));
    let offset = 4096 * ((104729 * index) % 4096);
    let serial = state.fixture.as_ref().expect("fixture").serial;
    state.operation = Some(
        state
            .fixture
            .as_ref()
            .expect("fixture")
            .upstream
            .operation()
            .map_err(super::ports::ScopeRefusal)?,
    );
    let operation = state.operation.as_ref().expect("actual operation");
    let ports = Observed::new(
        operation.overlay().clone(),
        &recorder,
        Some(operation.client()),
        "writes",
        Some(index),
    );
    state.source = Some(ports.acquire(operation.workspace().route(), index + 1)?);
    let source = state.source.expect("actual source");
    let view = operation.workspace().view_for_source(source)?;
    let result = operation.workspace().mutate(
        &ports,
        operation.serials(),
        &view,
        Operation::Write {
            serial,
            position: Position::At(offset),
            data: input.as_slice().into(),
        },
        FINAL_TIME,
    );
    let publication = match result {
        Ok(Outcome::Applied { publication, .. }) => publication,
        Ok(Outcome::Unchanged { .. }) => {
            return Err("selected ordinary write did not publish".into())
        }
        Err(original) => {
            if let Err(error) = ports.finish_base_interval("SUPPLY_FAILED") {
                if state.report_failure.is_none() {
                    state.report_failure = Some(Box::new(error));
                }
            }
            if recorder.borrow().healthy() {
                if let Err(error) = trace(
                    &mut recorder.borrow_mut(),
                    &Trace {
                        record_id: &record_id,
                        attempt_id: &attempt_id,
                        index,
                        serial,
                        source,
                        offset,
                        hash: &input_hash,
                        opened,
                        publication: None,
                        outcome: "Failed",
                    },
                ) {
                    if state.report_failure.is_none() {
                        state.report_failure = Some(Box::new(error));
                    }
                }
            }
            return Err(Box::new(original));
        }
    };
    ports.reply(publication)?;
    drop(view);
    ports.release(source)?;
    state.source = None;
    trace(
        &mut recorder.borrow_mut(),
        &Trace {
            record_id: &record_id,
            attempt_id: &attempt_id,
            index,
            serial,
            source,
            offset,
            hash: &input_hash,
            opened,
            publication: Some(publication),
            outcome: "Applied",
        },
    )?;
    drop(ports);
    state.operation = None;
    Ok(())
}
struct Trace<'a> {
    record_id: &'a str,
    attempt_id: &'a str,
    index: u64,
    serial: u64,
    source: BaseSource,
    offset: u64,
    hash: &'a str,
    opened: u64,
    publication: Option<Publication>,
    outcome: &'a str,
}
fn trace(r: &mut Recorder, row: &Trace<'_>) -> io::Result<()> {
    let Trace {
        index,
        serial,
        source,
        offset,
        hash,
        opened,
        publication,
        outcome,
        ..
    } = *row;
    let mut out = Json::new();
    r.header_with_id(&mut out, "write-operation", index, row.record_id)?;
    out.raw(",")?;
    out.text("public_attempt_id", row.attempt_id)?;
    out.raw(",")?;
    out.field("operation_index", index)?;
    out.raw(",")?;
    out.field("index", index)?;
    out.raw(",")?;
    out.field("serial", serial)?;
    out.raw(",\"source\":")?;
    records::source(&mut out, source)?;
    out.raw(",")?;
    out.field("offset", offset)?;
    out.raw(",\"length\":4096,")?;
    out.text("replacement_sha256", hash)?;
    out.raw(",")?;
    out.field("opened_ns", opened)?;
    out.raw(",")?;
    out.field("closed_ns", r.at())?;
    out.raw(",")?;
    out.text("outcome", outcome)?;
    out.raw(",\"publication\":")?;
    match publication {
        Some(value) => records::publication(&mut out, value)?,
        None => out.raw("null")?,
    };
    out.raw(",\"boundary\":\"caller-input-through-write-reply-attempt-and-source-release\",\"mtime_seconds\":-7,\"mtime_nanoseconds\":42}")?;
    r.append("trace", out.finish()?)
}
