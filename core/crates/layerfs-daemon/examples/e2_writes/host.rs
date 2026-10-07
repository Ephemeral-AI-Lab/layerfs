//! Real host initialization, authenticated serve and explicit original custody.
use super::{
    digest::{hex, sha256},
    fixture,
    json::Json,
};
use layerfs_bridge::native;
use layerfs_persistence::SqlitePersistenceProfile;
use layerfs_sdk::runtime::{
    service::ServiceOutcome,
    supervisor::{Supervisor, SupervisorConfig, SupervisorEvent},
};
#[path = "host_fence.rs"]
mod fences;
use std::{
    fs::{File, OpenOptions},
    io::{self, Write},
    net::TcpListener,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

struct Log {
    file: File,
    bytes: u64,
    records: u64,
    terminal: bool,
}
impl Log {
    fn new(path: &Path) -> io::Result<Self> {
        Ok(Self {
            file: OpenOptions::new().write(true).create_new(true).open(path)?,
            bytes: 0,
            records: 0,
            terminal: false,
        })
    }
    fn append(&mut self, body: Vec<u8>) -> io::Result<()> {
        if self.terminal {
            return Err(io::Error::other("original host recorder terminal"));
        }
        let mut offset = 0;
        while offset < body.len() {
            match self.file.write(&body[offset..]) {
                Ok(0) => {
                    self.terminal = true;
                    return Err(io::Error::new(
                        io::ErrorKind::WriteZero,
                        "original host record",
                    ));
                }
                Ok(n) => {
                    offset += n;
                    self.bytes += n as u64;
                }
                Err(error) => {
                    self.terminal = true;
                    return Err(error);
                }
            }
        }
        self.records += 1;
        Ok(())
    }
    fn event(&self, kind: &str, start: Instant) -> io::Result<Json> {
        let mut out = Json::new();
        out.raw("{\"schema\":\"cluster-two-e04-host-v1\",\"case\":\"E04-write-16m\",\"mode\":\"diagnostic\",\"sample_count\":0,\"admission_eligible\":false,\"qualification_status\":\"NOT_EVALUATED\",")?;
        out.text("kind", kind)?;
        out.raw(",")?;
        out.field("record_index", self.records)?;
        out.raw(",")?;
        out.field("at_ns", start.elapsed().as_nanos().min(u64::MAX as u128))?;
        Ok(out)
    }
}
fn fail(original: impl std::fmt::Debug) -> ! {
    // Exit while every actual host/Sessions/Supervisor/event is still in scope.
    // No unknown Save/root disposition is converted into guessed cleanup.
    eprintln!("E04_HOST_FAILED {original:?}");
    std::process::exit(1)
}
fn path(value: &std::ffi::OsStr) -> PathBuf {
    PathBuf::from(value)
}
fn write_once(path: &Path, body: &[u8]) -> io::Result<()> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?
        .write_all(body)
}

pub fn run() {
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() != 9 {
        fail("usage: e2_writes_host STORE_UNUSED SOURCE_UNUSED ASSIGNMENT_UNUSED BASE_INPUT PROFILE OUTPUT_UNUSED ADVERTISED_HOST DEADLINE_SECONDS");
    }
    let profile = match args[5].to_str() {
        Some("durable") => SqlitePersistenceProfile::Durable,
        Some("disposable") => SqlitePersistenceProfile::Disposable,
        _ => fail("actual host persistence profile"),
    };
    let seconds = args[8]
        .to_str()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0 && *v <= 90)
        .unwrap_or_else(|| fail("caller functional deadline must be1..90seconds"));
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(seconds));
        eprintln!("E04_HOST_FAILED complete outer functional watchdog");
        std::process::exit(3);
    });
    let start = Instant::now();
    let deadline = start + Duration::from_secs(seconds);
    let store = path(&args[1]);
    let source = path(&args[2]);
    let assignment = path(&args[3]);
    let base = path(&args[4]);
    let output = path(&args[6]);
    if [
        store.as_path(),
        source.as_path(),
        assignment.as_path(),
        base.as_path(),
        output.as_path(),
    ]
    .iter()
    .any(|v| !v.is_absolute())
        || store.starts_with(&output)
        || source.starts_with(&output)
        || base.starts_with(&output)
    {
        fail("fresh separate absolute host inputs/outputs required");
    }
    std::fs::create_dir(&output).unwrap_or_else(|error| fail(error));
    let mut log = Log::new(&output.join("host.jsonl")).unwrap_or_else(|error| fail(error));
    let mut before = log.event("setup-start", start).unwrap_or_else(|e| fail(e));
    before.raw(",\"scope\":\"actual-Project-acquisition-and-host-initialization-separate-from-Linux-writes\",\"namespace_init_workers\":4,\"ordinary_constructor_environment\":").unwrap_or_else(|e|fail(e));
    before
        .string(
            &std::env::var("LAYERFS_CONSTRUCTION_WORKERS").unwrap_or_else(|_| "UNAVAILABLE".into()),
        )
        .unwrap_or_else(|e| fail(e));
    before.raw(",").unwrap_or_else(|e| fail(e));
    before
        .text(
            "profile",
            if profile == SqlitePersistenceProfile::Durable {
                "durable"
            } else {
                "disposable"
            },
        )
        .unwrap_or_else(|e| fail(e));
    before.raw("}").unwrap_or_else(|e| fail(e));
    log.append(before.finish().unwrap_or_else(|e| fail(e)))
        .unwrap_or_else(|e| fail(e));
    let mut fixture = fixture::prepare_host(
        &store,
        &source,
        &assignment,
        &base,
        profile,
        [0xe4; 32],
        deadline,
    )
    .unwrap_or_else(|original| fail(original));
    let mut setup = log.event("setup-ready", start).unwrap_or_else(|e| fail(e));
    setup.raw(",").unwrap_or_else(|e| fail(e));
    setup
        .text(
            "effective_root",
            &hex(&fixture.expected.snapshot.effective_root.to_bytes()),
        )
        .unwrap_or_else(|e| fail(e));
    setup.raw(",").unwrap_or_else(|e| fail(e));
    setup
        .text("scope", &hex(&fixture.expected.snapshot.scope.to_bytes()))
        .unwrap_or_else(|e| fail(e));
    setup.raw(",").unwrap_or_else(|e| fail(e));
    setup
        .field("root_serial", fixture.expected.root_serial)
        .unwrap_or_else(|e| fail(e));
    setup.raw(",").unwrap_or_else(|e| fail(e));
    setup
        .text("base_sha256", &fixture.acquisition.base_sha256)
        .unwrap_or_else(|e| fail(e));
    setup.raw(",").unwrap_or_else(|e| fail(e));
    setup
        .field(
            "source_copied_bytes",
            fixture.acquisition.source_copied_bytes,
        )
        .unwrap_or_else(|e| fail(e));
    setup.raw(",").unwrap_or_else(|e| fail(e));
    setup
        .field(
            "source_allocated_bytes",
            fixture.acquisition.source_allocated_bytes,
        )
        .unwrap_or_else(|e| fail(e));
    setup.raw(",").unwrap_or_else(|e| fail(e));
    setup
        .field("source_removed", fixture.acquisition.source_removed)
        .unwrap_or_else(|e| fail(e));
    setup.raw(",\"source_binding\":\"actual-closed-native-copy-and-public-Project-Init-not-a-Project-read-counter\"}").unwrap_or_else(|e|fail(e));
    log.append(setup.finish().unwrap_or_else(|e| fail(e)))
        .unwrap_or_else(|e| fail(e));
    let listener = TcpListener::bind("0.0.0.0:0").unwrap_or_else(|e| fail(e));
    listener.set_nonblocking(true).unwrap_or_else(|e| fail(e));
    let advertised = args[7]
        .to_str()
        .filter(|v| !v.is_empty() && !v.contains('\n') && !v.contains('\r'))
        .unwrap_or_else(|| fail("caller advertised host"));
    let endpoint = format!(
        "{advertised}:{}",
        listener.local_addr().unwrap_or_else(|e| fail(e)).port()
    );
    write_once(
        &output.join("endpoint.txt"),
        format!("{endpoint}\n").as_bytes(),
    )
    .unwrap_or_else(|e| fail(e));
    println!(
        "E04_HOST_READY endpoint={endpoint} assignment={}",
        assignment.display()
    );
    let (stream, _) = loop {
        match listener.accept() {
            Ok(v) => break v,
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    fail("original listener readiness deadline");
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            Err(original) => fail(original),
        }
    };
    stream.set_nonblocking(false).unwrap_or_else(|e| fail(e));
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap_or_else(|e| fail(e));
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .unwrap_or_else(|e| fail(e));
    let receive_options = stream.try_clone().unwrap_or_else(|e| fail(e));
    let connection = native::accept(stream, &fixture::HOST_PRIVATE, fixture.expected.local_peer)
        .unwrap_or_else(|original| fail(original));
    // The independent input worker waits even when no call is admitted. Its
    // idle lifetime ends at the explicit socket fence, not the handshake bound.
    receive_options
        .set_read_timeout(None)
        .unwrap_or_else(|e| fail(e));
    drop(receive_options);
    let mut handshake = log
        .event("native-handshake", start)
        .unwrap_or_else(|e| fail(e));
    handshake.raw(",").unwrap_or_else(|e| fail(e));
    handshake
        .text(
            "native_work_debug",
            &format!("{:?}", connection.handshake_work),
        )
        .unwrap_or_else(|e| fail(e));
    handshake
        .raw(",\"scope\":\"original-authenticated-handshake-only\",\"handshake_read_timeout_ms\":5000,\"host_input_idle_timeout_ms\":null,\"host_input_stop\":\"explicit-fence-or-outer-functional-watchdog\"}")
        .unwrap_or_else(|e| fail(e));
    log.append(handshake.finish().unwrap_or_else(|e| fail(e)))
        .unwrap_or_else(|e| fail(e));
    let mut sessions = fixture.runtime.sessions();
    let binding = sessions
        .bind(
            &connection.peer,
            fixture.expected.workspace,
            fixture.expected.snapshot.branch.id,
        )
        .unwrap_or_else(|original| fail(original));
    if binding.snapshot() != &fixture.expected.snapshot {
        let _original = connection;
        fail("actual initialized Binding differs from provisioned assignment");
    }
    let retained_binding = binding.clone();
    let mut supervisor = Supervisor::new(&mut sessions, SupervisorConfig::default())
        .unwrap_or_else(|original| fail(original));
    let attachment = supervisor
        .attach_bound(connection, binding)
        .unwrap_or_else(|original| {
            eprintln!("E04_HOST_FAILED {}", fences::attach_cause(&original));
            let _retained = original;
            std::process::exit(1)
        });
    let mut delivered = 0u64;
    let mut incomplete = None;
    let mut explicit_fence = false;
    let fence = loop {
        if Instant::now() >= deadline {
            fail("original host serving deadline; owners retained");
        }
        let turn = supervisor.step_observed();
        if let Some(original) = &turn.wake_error {
            fail(original);
        }
        if let Some(event) = turn.event {
            match event {
                SupervisorEvent::Delivered(delivery) => {
                    if delivery.attachment != attachment {
                        fail("original delivery attachment mismatch");
                    }
                    let mut row = log.event("delivered", start).unwrap_or_else(|e| fail(e));
                    row.raw(",").unwrap_or_else(|e| fail(e));
                    row.field("delivery_index", delivered)
                        .unwrap_or_else(|e| fail(e));
                    row.raw(",").unwrap_or_else(|e| fail(e));
                    row.text("operation", &format!("{:?}", delivery.header.operation))
                        .unwrap_or_else(|e| fail(e));
                    row.raw(",").unwrap_or_else(|e| fail(e));
                    row.field("complete", delivery.output.complete)
                        .unwrap_or_else(|e| fail(e));
                    row.raw(",").unwrap_or_else(|e| fail(e));
                    row.field("completed_bytes", delivery.output.completed_bytes)
                        .unwrap_or_else(|e| fail(e));
                    row.raw(",\"provider_outcome\":")
                        .unwrap_or_else(|e| fail(e));
                    match &delivery.completion {
                        Some(value) => {
                            let status = match value.outcome() {
                                ServiceOutcome::Dispatched(Ok(_)) => "OK",
                                ServiceOutcome::Dispatched(Err(_)) => "FAILED",
                                ServiceOutcome::Unattempted(_) => "UNATTEMPTED",
                            };
                            row.string(status).unwrap_or_else(|e| fail(e));
                            row.raw(",").unwrap_or_else(|e| fail(e));
                            row.field("provider_queue_wait_ns", value.queue_wait_ns())
                                .unwrap_or_else(|e| fail(e));
                            row.raw(",").unwrap_or_else(|e| fail(e));
                            row.field("provider_service_ns", value.service_ns())
                                .unwrap_or_else(|e| fail(e));
                        }
                        None => row.raw("null").unwrap_or_else(|e| fail(e)),
                    };
                    row.raw(
                        ",\"scope\":\"original-local-socket-send-not-remote-acknowledgement\"}",
                    )
                    .unwrap_or_else(|e| fail(e));
                    log.append(row.finish().unwrap_or_else(|e| fail(e)))
                        .unwrap_or_else(|e| fail(e));
                    let provider_failed = delivery.refused.is_some()
                        || delivery.completion.as_ref().is_none_or(|value| {
                            !matches!(value.outcome(), ServiceOutcome::Dispatched(Ok(_)))
                        });
                    if !delivery.output.complete || provider_failed {
                        incomplete = Some(delivery);
                    } else {
                        delivered += 1;
                        drop(delivery);
                    }
                }
                SupervisorEvent::Fenced(original) => break original,
            }
        } else if let Some(park) = turn.park {
            park.wait_until(deadline.min(Instant::now() + Duration::from_millis(10)))
                .unwrap_or_else(|original| fail(original));
        }
        if incomplete.is_some() && !explicit_fence {
            supervisor
                .fence(attachment)
                .unwrap_or_else(|original| fail(original));
            explicit_fence = true;
        }
    };
    let mut joined = log
        .event("attachment-fence", start)
        .unwrap_or_else(|e| fail(e));
    joined.raw(",").unwrap_or_else(|e| fail(e));
    joined
        .field("deliveries", delivered)
        .unwrap_or_else(|e| fail(e));
    let clean = fences::record(&mut joined, &fence, attachment, &retained_binding)
        .unwrap_or_else(|e| fail(e));
    log.append(joined.finish().unwrap_or_else(|e| fail(e)))
        .unwrap_or_else(|e| fail(e));
    // Original workers, partials, receipts and the incomplete delivery remain
    // held on refusal. No timeout/truncated EOF is printed as a successful fence.
    if !clean || incomplete.is_some() {
        fail("original joined EOF/credit/output refusal; all original custody retained");
    }
    drop(fence);
    let mut final_row = log
        .event("supervisor-final", start)
        .unwrap_or_else(|e| fail(e));
    final_row.raw(",").unwrap_or_else(|e| fail(e));
    final_row
        .text("supervisor_work_debug", &format!("{:?}", supervisor.work()))
        .unwrap_or_else(|e| fail(e));
    final_row.raw(",").unwrap_or_else(|e| fail(e));
    final_row
        .text(
            "service_work_debug",
            &format!("{:?}", supervisor.service_work()),
        )
        .unwrap_or_else(|e| fail(e));
    final_row.raw(",").unwrap_or_else(|e| fail(e));
    final_row
        .text(
            "output_work_debug",
            &format!("{:?}", supervisor.output_work()),
        )
        .unwrap_or_else(|e| fail(e));
    final_row.raw(",").unwrap_or_else(|e| fail(e));
    final_row
        .field("input_owners", supervisor.input_owners())
        .unwrap_or_else(|e| fail(e));
    final_row.raw("}").unwrap_or_else(|e| fail(e));
    log.append(final_row.finish().unwrap_or_else(|e| fail(e)))
        .unwrap_or_else(|e| fail(e));
    let service = supervisor.service_work();
    let output_work = supervisor
        .output_work()
        .unwrap_or_else(|original| fail(original));
    if supervisor.work().attachments != 0
        || supervisor.input_owners() != 0
        || service.outstanding != 0
        || service.credited_bytes != 0
        || output_work.owners != 0
        || output_work.messages != 0
        || output_work.credited_bytes != 0
        || output_work.reserved_bytes != 0
        || output_work.packet_capacity_bytes != 0
    {
        fail("original supervisor/provider/native credits remain held");
    }
    drop(supervisor);
    let custody = sessions.fence().unwrap_or_else(|original| fail(original));
    if custody.slots().iter().any(Option::is_some) {
        fail(&custody);
    }
    let mut done = log
        .event("serving-scope-fenced", start)
        .unwrap_or_else(|e| fail(e));
    done.raw(",\"save_slots_held\":0,")
        .unwrap_or_else(|e| fail(e));
    done.field("acknowledged_host_records", log.records)
        .unwrap_or_else(|e| fail(e));
    done.raw(",").unwrap_or_else(|e| fail(e));
    done.field("acknowledged_host_bytes", log.bytes)
        .unwrap_or_else(|e| fail(e));
    done.raw(",").unwrap_or_else(|e| fail(e));
    done.text(
        "assignment_sha256",
        &hex(&sha256(
            &std::fs::read(&assignment).unwrap_or_else(|e| fail(e)),
        )),
    )
    .unwrap_or_else(|e| fail(e));
    done.raw("}").unwrap_or_else(|e| fail(e));
    log.append(done.finish().unwrap_or_else(|e| fail(e)))
        .unwrap_or_else(|e| fail(e));
    println!("E04_HOST_FENCED deliveries={delivered} qualification=NOT_EVALUATED");
}
