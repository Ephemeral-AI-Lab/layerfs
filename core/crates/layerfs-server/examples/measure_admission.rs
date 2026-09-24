//! Writer-budget admission: the same logical write set at a configured budget.
//!
//! One arm per invocation, through the real service entry point
//! (`Service::handle`, the same body the native transport calls). The question is
//! whether the Store's configured `max_concurrent_writes` changes what the
//! product can do, and whether it stays correct while doing it.
//!
//! `--mode queued` submits `--writes` distinct logical writes with at most
//! `--writers` in flight: that is the throughput arm. `--mode oversubscribed`
//! releases `--writes` callers at once against the same budget and counts who was
//! admitted and who was refused: that is a deterministic admission observation,
//! not a rate. Admitted callers hold their permit inside their input read until
//! every caller has had its decision, so the count is not a race.
//!
//! Phases are separate and named (`benchmark_rules.md` §6): preparation is
//! untimed and deterministic, the work phase is the timed operation set,
//! verification is a second pass that reads every saved root back through the
//! real read path and compares it byte for byte, and cleanup closes the Store.
//! Cache state is declared and identical in every arm - the Store is created
//! fresh inside this arm's own output directory, the payloads are built in this
//! process's memory immediately before the timed phase, and no arm is given an
//! invalidation the others do not get. One sample per arm: nothing is retried,
//! nothing is dropped, no best-of is selected, and no arm reads another arm's
//! numbers.
//!
//! This is a **diagnostic, not a gate**: it decides nothing about release
//! admission, and a larger budget producing a larger number is not a throughput
//! claim by itself.
//!
//! Usage:
//!   measure_admission --writers 4 --writes 16 --bytes 4194304 \
//!     --mode queued --commit <sha> --output FRESH_DIR

use std::error::Error;
use std::io::{self, Cursor, Read};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Barrier, Mutex};
use std::time::{Duration, Instant};

use layerfs_bridge::{adapters::native::connection::VerifiedPeer, contract::*};
use layerfs_server::{Grant, Service, StoreAccess};
use layerfs_storage::{StoragePolicy, Store};
use layerfs_telemetry::operation::OperationRecorder;
use layerfs_telemetry::timer::Timing;

type Failure = Box<dyn Error>;

const MIB: f64 = 1024.0 * 1024.0;
const DEFAULT_WRITES: usize = 16;
const DEFAULT_BYTES: usize = 4 << 20;
/// Grants for every opcode this example uses (read, inspect, construct, edit, update).
const GRANTED_OPERATIONS: u8 = 31;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Mode {
    /// `--writers` callers pull `--writes` logical writes from one queue.
    Queued,
    /// Every caller is released at once; admitted callers hold until all decide.
    Oversubscribed,
}

struct Options {
    writers: usize,
    writes: usize,
    bytes: usize,
    mode: Mode,
    commit: String,
    output: PathBuf,
}

fn parse_options() -> Result<Options, Failure> {
    let mut writers = None;
    let mut writes = DEFAULT_WRITES;
    let mut bytes = DEFAULT_BYTES;
    let mut mode = Mode::Queued;
    let mut commit = "unrecorded".to_string();
    let mut output = None;
    let mut arguments = std::env::args().skip(1);
    while let Some(flag) = arguments.next() {
        let value = arguments
            .next()
            .ok_or_else(|| format!("{flag} needs a value"))?;
        match flag.as_str() {
            "--writers" => writers = Some(value.parse::<usize>()?),
            "--writes" => writes = value.parse()?,
            "--bytes" => bytes = value.parse()?,
            "--commit" => commit = value,
            "--mode" => {
                mode = match value.as_str() {
                    "queued" => Mode::Queued,
                    "oversubscribed" => Mode::Oversubscribed,
                    other => return Err(format!("unknown mode {other}").into()),
                }
            }
            "--output" => output = Some(PathBuf::from(value)),
            other => return Err(format!("unknown flag {other}").into()),
        }
    }
    let writers = writers.ok_or("--writers is required")?;
    if writers == 0 || writers > writes {
        return Err("--writers must be between 1 and --writes".into());
    }
    if writes == 0 || bytes == 0 || writers > u8::MAX as usize {
        return Err("--writes and --bytes must be positive, --writers at most 255".into());
    }
    Ok(Options {
        writers,
        writes,
        bytes,
        mode,
        commit,
        output: output.ok_or("--output is required")?,
    })
}

/// Deterministic splitmix64 bytes: a distinct payload per write, every run.
fn payload(seed: u64, bytes: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes);
    let mut state = 0x9E37_79B9_7F4A_7C15_u64 ^ seed.wrapping_mul(0xD1B5_4A32_D192_ED03);
    while out.len() < bytes {
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        out.extend_from_slice(&(z ^ (z >> 31)).to_le_bytes());
    }
    out.truncate(bytes);
    out
}

fn peer() -> VerifiedPeer {
    VerifiedPeer::from_private(&[7; 32]).expect("authorized caller key")
}

fn service(store: Store) -> Arc<Service> {
    Arc::new(
        Service::new(
            vec![StoreAccess {
                id: 1,
                store,
                history: None,
                grants: vec![Grant {
                    public_key: *peer().public_key(),
                    operations: GRANTED_OPERATIONS,
                    expires_unix: u64::MAX,
                }],
            }],
            OperationRecorder::disabled(),
        )
        .expect("service assembly"),
    )
}

fn request(id: u64, operation: Operation, bytes: u64) -> Request {
    Request {
        id,
        generation: 1,
        store: 1,
        profile: 1,
        deadline_ms: 600_000,
        response_bytes: bytes,
        operation,
    }
}

/// JSON for an optional count: a number, or `null` when the host cannot report it.
fn json_optional(value: Option<u64>) -> String {
    value.map_or_else(|| "null".to_string(), |count| count.to_string())
}

/// Minimal JSON string escaping for the one caller-supplied token.
fn json_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            other => out.push(other),
        }
    }
    out
}

/// Rejects a receipt that leaked Rust debug formatting instead of JSON.
fn check_json_receipt(json: &str) -> Result<(), Failure> {
    for leak in ["Some(", "None", "Ok(", "Err("] {
        if json.contains(leak) {
            return Err(format!("the JSON receipt leaked {leak}").into());
        }
    }
    if !json.starts_with('{') || !json.trim_end().ends_with('}') {
        return Err("the JSON receipt is not one object".into());
    }
    Ok(())
}

/// Resident set size in KiB, as the platform reports it; `None` if unavailable.
fn sample_rss() -> Option<u64> {
    let output = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .ok()?;
    String::from_utf8(output.stdout).ok()?.trim().parse().ok()
}

/// What one logical write did.
#[derive(Clone, Copy, Debug, Default)]
struct WriteOutcome {
    inserted: u64,
    refused: bool,
    failed: bool,
    /// Nanoseconds from the work phase's start to this write's start.
    started_ns: u64,
    /// Nanoseconds from the work phase's start to this write's end.
    ended_ns: u64,
}

/// The call body: one `ConstructFile` operation over prepared bytes.
fn construct(service: &Service, id: u64, bytes: &[u8]) -> (Option<[u8; 32]>, WriteOutcome) {
    let operation = Operation::ConstructFile {
        length: bytes.len() as u64,
    };
    let (result, _) = service.handle(
        &peer(),
        &request(id, operation, bytes.len() as u64),
        &mut Cursor::new(bytes),
        &mut io::sink(),
    );
    match result {
        Ok(Response::Saved { root, .. }) => (
            Some(root),
            WriteOutcome {
                inserted: 1,
                ..WriteOutcome::default()
            },
        ),
        Ok(other) => {
            eprintln!("measure_admission: unexpected response {other:?}");
            (
                None,
                WriteOutcome {
                    failed: true,
                    ..WriteOutcome::default()
                },
            )
        }
        Err(failure) => (
            None,
            WriteOutcome {
                refused: failure.code == Code::Capacity,
                failed: failure.code != Code::Capacity,
                ..WriteOutcome::default()
            },
        ),
    }
}

/// An input that reports its first read, waits for the release, then delivers.
///
/// The first read happens only after admission, so a report from here is proof
/// that this caller holds a writer permit.
struct Gate {
    opened: mpsc::Sender<()>,
    release: mpsc::Receiver<()>,
    reached_read: Arc<AtomicBool>,
    waiting: bool,
    payload: Cursor<Vec<u8>>,
}

impl Read for Gate {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.waiting {
            self.waiting = false;
            self.reached_read.store(true, Ordering::Release);
            let _ = self.opened.send(());
            let _ = self.release.recv();
        }
        self.payload.read(buffer)
    }
}

/// One caller's whole result.
struct Call {
    root: Option<[u8; 32]>,
    outcome: WriteOutcome,
    admitted: bool,
}

/// Releases every caller at once and returns what each of them got.
///
/// Every caller reports its decision before any admitted caller is released:
/// a refusal is reported by the caller, an admission by the gate's first read.
/// Only then are the admitted callers allowed to finish, so the admitted count
/// is the budget's decision and not a race with completion.
fn oversubscribed(service: &Arc<Service>, payloads: &Arc<Vec<Vec<u8>>>) -> (Vec<Call>, u64) {
    let callers = payloads.len();
    let barrier = Arc::new(Barrier::new(callers + 1));
    let (reports, decisions) = mpsc::channel::<()>();
    let mut releases = Vec::with_capacity(callers);
    let mut workers = Vec::with_capacity(callers);
    for (index, bytes) in payloads.iter().enumerate() {
        let service = Arc::clone(service);
        let barrier = Arc::clone(&barrier);
        let reports = reports.clone();
        let reported_after = reports.clone();
        let reached_read = Arc::new(AtomicBool::new(false));
        let gate_reached = Arc::clone(&reached_read);
        let (release, held) = mpsc::channel::<()>();
        releases.push(release);
        let bytes = bytes.clone();
        workers.push(std::thread::spawn(move || {
            let operation = Operation::ConstructFile {
                length: bytes.len() as u64,
            };
            let request = request(index as u64 + 1, operation, bytes.len() as u64);
            barrier.wait();
            let (result, _) = service.handle(
                &peer(),
                &request,
                &mut Gate {
                    opened: reports,
                    release: held,
                    reached_read: gate_reached,
                    waiting: true,
                    payload: Cursor::new(bytes),
                },
                &mut io::sink(),
            );
            // An admitted caller reported from its first read; a caller that
            // never reached one - a refusal, or a failure before the operation
            // body - reports its decision here.
            if !reached_read.load(Ordering::Acquire) {
                let _ = reported_after.send(());
            }
            match result {
                Ok(Response::Saved { root, .. }) => Call {
                    root: Some(root),
                    outcome: WriteOutcome {
                        inserted: 1,
                        ..WriteOutcome::default()
                    },
                    admitted: true,
                },
                Ok(other) => {
                    eprintln!("measure_admission: unexpected response {other:?}");
                    Call {
                        root: None,
                        outcome: WriteOutcome {
                            failed: true,
                            ..WriteOutcome::default()
                        },
                        admitted: true,
                    }
                }
                Err(failure) => Call {
                    root: None,
                    outcome: WriteOutcome {
                        refused: failure.code == Code::Capacity,
                        failed: failure.code != Code::Capacity,
                        ..WriteOutcome::default()
                    },
                    admitted: false,
                },
            }
        }));
    }
    drop(reports);
    // Release every caller at the same moment: this is what makes the arm
    // "oversubscribed" rather than "queued behind the first arrival".
    barrier.wait();
    let decision_started = Instant::now();
    for _ in 0..callers {
        if decisions.recv_timeout(Duration::from_secs(60)).is_err() {
            break;
        }
    }
    let decision_ns = decision_started.elapsed().as_nanos() as u64;
    for release in releases {
        let _ = release.send(());
    }
    let calls = workers
        .into_iter()
        .map(|worker| {
            worker.join().unwrap_or(Call {
                root: None,
                outcome: WriteOutcome {
                    failed: true,
                    ..WriteOutcome::default()
                },
                admitted: false,
            })
        })
        .collect();
    (calls, decision_ns)
}

fn main() -> Result<(), Failure> {
    let options = parse_options()?;
    if options.output.exists() {
        return Err(format!(
            "{} exists; a run never overwrites evidence",
            options.output.display()
        )
        .into());
    }
    std::fs::create_dir_all(&options.output)?;
    let store_path = options.output.join("store.sqlite");

    // Preparation: untimed, deterministic, declared. Distinct bytes per write, so
    // every logical write inserts its own objects instead of reusing another
    // write's rows - the work is the same in every arm.
    let started = Instant::now();
    let payloads: Arc<Vec<Vec<u8>>> = Arc::new(
        (0..options.writes)
            .map(|index| payload(index as u64, options.bytes))
            .collect(),
    );
    let preparation_ns = started.elapsed().as_nanos() as u64;
    let rss_before = sample_rss();

    // Setup: the Store and the service assembly, outside the work phase.
    let setup_started = Instant::now();
    let policy = StoragePolicy::frozen_default();
    let store = Timing::disabled("measure.setup", |scope| -> Result<Store, Failure> {
        let store = Store::create(&store_path, policy, scope.child("store.create"))?;
        let applied = store
            .set_max_concurrent_writes(options.writers as u8, scope.child("store.configure"))?;
        assert_eq!(usize::from(applied), options.writers);
        Ok(store)
    })
    .0?;
    let service = service(store);
    let setup_ns = setup_started.elapsed().as_nanos() as u64;

    let roots: Arc<Mutex<Vec<Option<[u8; 32]>>>> = Arc::new(Mutex::new(vec![None; options.writes]));
    let outcomes: Arc<Mutex<Vec<WriteOutcome>>> =
        Arc::new(Mutex::new(vec![WriteOutcome::default(); options.writes]));
    let peak_in_flight = Arc::new(AtomicUsize::new(0));
    let admitted_at_once;
    let decision_ns;

    // Work phase: the timed operation set. The phase clock is what makes a
    // within-run shape measurable at any budget: summing concurrent per-write
    // durations would count the same wall time once per writer, so the receipt
    // reports bytes completed in the first and second half of the phase instead.
    let work_started = Instant::now();
    let phase = work_started;
    match options.mode {
        Mode::Queued => {
            let next = Arc::new(AtomicUsize::new(0));
            let live = Arc::new(AtomicUsize::new(0));
            let workers: Vec<_> = (0..options.writers)
                .map(|_| {
                    let service = Arc::clone(&service);
                    let payloads = Arc::clone(&payloads);
                    let next = Arc::clone(&next);
                    let roots = Arc::clone(&roots);
                    let outcomes = Arc::clone(&outcomes);
                    let peak = Arc::clone(&peak_in_flight);
                    let live = Arc::clone(&live);
                    std::thread::spawn(move || loop {
                        let index = next.fetch_add(1, Ordering::AcqRel);
                        if index >= payloads.len() {
                            return;
                        }
                        let in_flight = live.fetch_add(1, Ordering::AcqRel) + 1;
                        peak.fetch_max(in_flight, Ordering::AcqRel);
                        let started = Instant::now();
                        let (root, mut outcome) =
                            construct(&service, index as u64 + 1, &payloads[index]);
                        outcome.started_ns = started.duration_since(phase).as_nanos() as u64;
                        outcome.ended_ns = started.elapsed().as_nanos() as u64 + outcome.started_ns;
                        live.fetch_sub(1, Ordering::AcqRel);
                        roots.lock().expect("roots")[index] = root;
                        outcomes.lock().expect("outcomes")[index] = outcome;
                    })
                })
                .collect();
            for worker in workers {
                let _ = worker.join();
            }
            admitted_at_once = options.writers.min(options.writes);
            decision_ns = 0;
        }
        Mode::Oversubscribed => {
            let (calls, waited) = oversubscribed(&service, &payloads);
            let mut held_roots = roots.lock().expect("roots");
            let mut held_outcomes = outcomes.lock().expect("outcomes");
            let mut admitted = 0;
            for (index, call) in calls.into_iter().enumerate() {
                held_roots[index] = call.root;
                held_outcomes[index] = call.outcome;
                if call.admitted {
                    admitted += 1;
                }
            }
            admitted_at_once = admitted;
            decision_ns = waited;
        }
    }
    let work_ns = work_started.elapsed().as_nanos() as u64;

    // Verification: a second, separate phase over the real read path.
    let verify_started = Instant::now();
    let mut verified = 0usize;
    let mut mismatched = 0usize;
    {
        let held_roots = roots.lock().expect("roots");
        for (index, root) in held_roots.iter().enumerate() {
            let Some(root) = root else { continue };
            let bytes = &payloads[index];
            let operation = Operation::ReadFile {
                root: *root,
                start: 0,
                end: bytes.len() as u64,
            };
            let mut output = Vec::new();
            let (result, _) = service.handle(
                &peer(),
                &request(1_000_000 + index as u64, operation, bytes.len() as u64),
                &mut io::empty(),
                &mut output,
            );
            match result {
                Ok(Response::Read { length })
                    if length == bytes.len() as u64 && output == *bytes =>
                {
                    verified += 1;
                }
                _ => mismatched += 1,
            }
        }
    }
    let verification_ns = verify_started.elapsed().as_nanos() as u64;

    // Cleanup: close the Store and read the space it occupies.
    let cleanup_started = Instant::now();
    drop(service);
    let store_bytes = std::fs::metadata(&store_path)
        .map(|meta| meta.len())
        .unwrap_or(0);
    let cleanup_ns = cleanup_started.elapsed().as_nanos() as u64;
    let rss_after = sample_rss();
    let complete_command_ns = preparation_ns + setup_ns + work_ns + verification_ns + cleanup_ns;

    let outcomes = outcomes.lock().expect("outcomes").clone();
    // The shape inside one arm: bytes completed in the first and second half of
    // the phase's wall time, so a rate that decays as the Store grows is visible
    // instead of averaged away. Valid at any budget, because it uses each write's
    // own completion offset rather than a sum of overlapping durations.
    let mut completed: Vec<u64> = outcomes
        .iter()
        .filter(|outcome| outcome.inserted == 1)
        .map(|outcome| outcome.ended_ns)
        .collect();
    completed.sort_unstable();
    let midpoint = work_ns / 2;
    let first_half_bytes = completed.iter().filter(|ended| **ended <= midpoint).count() as u64;
    let second_half_bytes = completed.len() as u64 - first_half_bytes;
    let half_rate = |writes: u64| -> f64 {
        let bytes = writes * options.bytes as u64;
        let seconds = (work_ns as f64 / 2.0) / 1e9;
        if seconds == 0.0 {
            0.0
        } else {
            (bytes as f64 / MIB) / seconds
        }
    };
    let durations: Vec<u64> = outcomes
        .iter()
        .filter(|outcome| outcome.inserted == 1)
        .map(|outcome| outcome.ended_ns.saturating_sub(outcome.started_ns))
        .collect();
    let slowest = durations
        .iter()
        .enumerate()
        .max_by_key(|(_, nanos)| **nanos)
        .map(|(index, nanos)| (index, *nanos));
    let saved = roots
        .lock()
        .expect("roots")
        .iter()
        .filter(|root| root.is_some())
        .count();
    let total_bytes = options.bytes * options.writes;
    let inserted: u64 = outcomes.iter().map(|outcome| outcome.inserted).sum();
    let refused = outcomes.iter().filter(|outcome| outcome.refused).count();
    let failed = outcomes.iter().filter(|outcome| outcome.failed).count();
    let rate = |nanos: u64| -> f64 {
        if nanos == 0 {
            0.0
        } else {
            (total_bytes as f64 / MIB) / (nanos as f64 / 1e9)
        }
    };
    let cores = std::thread::available_parallelism().map_or(0, |value| value.get());

    let receipt = format!(
        "arm                writers={} writes={} bytes_per_write={} mode={:?}\n\
         commit             {}\n\
         host               {} {} ({} logical cores)\n\
         cache declaration  payloads built in-process before the timed phase; fresh\n\
         \x20                  Store inside this arm's output; no invalidation for one\n\
         \x20                  arm only; no cold-cache claim; one sample per arm\n\
         preparation        {:.3} s (untimed)\n\
         setup              {:.3} s (store create + budget set + service assembly)\n\
         work               {:.3} s   {:.2} MiB/s   {:.2} writes/s\n\
         verification       {:.3} s   {}/{} saved roots read back byte-identical, {} mismatched\n\
         cleanup            {:.3} s\n\
         complete command   {:.3} s\n\
         peak in flight     {} (budget {}); admitted at once {}\n\
         write shape        first half of the phase {:.2} MiB/s ({} writes), second half {:.2} MiB/s ({} writes); slowest write #{} at {:.3} s\n\
         decisions          {:.3} s to decide every oversubscribed caller\n\
         outcomes           inserted={} refused={} failed={}\n\
         store file         {} bytes ({:.1} MiB, {:.2}x the payload)\n\
         rss                before={:?} KiB after={:?} KiB\n",
        options.writers,
        options.writes,
        options.bytes,
        options.mode,
        options.commit,
        std::env::consts::OS,
        std::env::consts::ARCH,
        cores,
        preparation_ns as f64 / 1e9,
        setup_ns as f64 / 1e9,
        work_ns as f64 / 1e9,
        rate(work_ns),
        options.writes as f64 / (work_ns as f64 / 1e9),
        verification_ns as f64 / 1e9,
        verified,
        saved,
        mismatched,
        cleanup_ns as f64 / 1e9,
        complete_command_ns as f64 / 1e9,
        peak_in_flight.load(Ordering::Acquire).max(admitted_at_once),
        options.writers,
        admitted_at_once,
        half_rate(first_half_bytes),
        first_half_bytes,
        half_rate(second_half_bytes),
        second_half_bytes,
        slowest.map_or(0, |(index, _)| index),
        slowest.map_or(0.0, |(_, nanos)| nanos as f64 / 1e9),
        decision_ns as f64 / 1e9,
        inserted,
        refused,
        failed,
        store_bytes,
        store_bytes as f64 / MIB,
        store_bytes as f64 / total_bytes as f64,
        rss_before,
        rss_after,
    );
    println!("{receipt}");
    std::fs::write(options.output.join("measure-admission.txt"), &receipt)?;
    let json = format!(
        "{{\"kind\":\"diagnostic\",\"claim\":\"none\",\"arm\":{{\"writers\":{},\"writes\":{},\
         \"bytes_per_write\":{},\"mode\":\"{:?}\"}},\"commit\":\"{}\",\"host\":\"{} {}\",\
         \"cores\":{},\"preparation_ns\":{},\"setup_ns\":{},\"work_ns\":{},\"verification_ns\":{},\
         \"cleanup_ns\":{},\"complete_command_ns\":{},\"peak_in_flight\":{},\"inserted\":{},\
         \"refused\":{},\"admitted_at_once\":{},\"failed\":{},\"verified_roots\":{},\
         \"saved_roots\":{},\"mismatched_roots\":{},\"store_bytes\":{},\"first_half_mib_per_s\":{:.2},\
         \"second_half_mib_per_s\":{:.2},\"first_half_writes\":{},\"second_half_writes\":{},\
         \"write_spans_ns\":[{}],\"rss_before_kib\":{},\
         \"rss_after_kib\":{}}}\n",
        options.writers,
        options.writes,
        options.bytes,
        options.mode,
        json_string(&options.commit),
        std::env::consts::OS,
        std::env::consts::ARCH,
        cores,
        preparation_ns,
        setup_ns,
        work_ns,
        verification_ns,
        cleanup_ns,
        complete_command_ns,
        peak_in_flight.load(Ordering::Acquire).max(admitted_at_once),
        inserted,
        refused,
        admitted_at_once,
        failed,
        verified,
        saved,
        mismatched,
        store_bytes,
        half_rate(first_half_bytes),
        half_rate(second_half_bytes),
        first_half_bytes,
        second_half_bytes,
        outcomes
            .iter()
            .map(|outcome| format!("[{},{}]", outcome.started_ns, outcome.ended_ns))
            .collect::<Vec<_>>()
            .join(","),
        json_optional(rss_before),
        json_optional(rss_after),
    );
    check_json_receipt(&json)?;
    std::fs::write(options.output.join("measure-admission.json"), json)?;
    if mismatched != 0 || failed != 0 {
        return Err(format!("{mismatched} mismatched and {failed} failed writes").into());
    }
    Ok(())
}
