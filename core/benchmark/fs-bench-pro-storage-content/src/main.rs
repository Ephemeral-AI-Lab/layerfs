//! The harness entry point: argv to exactly one operation, then one trace file.
//!
//! This binary never selects a case by itself and never loops over a family. It
//! runs the one case it was handed, writes `trace.jsonl` beside the caller's
//! output path, and prints the facts the runner needs. Everything reusable lives
//! in the library, because a binary crate cannot be imported by `tests/*.rs`.

use std::path::PathBuf;
use std::process::ExitCode;

use fs_bench_storage_content::ops::{self, Hold, HoldPoint, OpContext, Phase};
use fs_bench_storage_content::registry::{self, Admission};
use fs_bench_storage_content::support::phases;
use fs_bench_storage_content::workload::expected::Expected;
use fs_bench_storage_content::workload::history::{self, Corpus, Row as HistoryRow};
use fs_bench_storage_content::support::trace::{Kind, TraceWriter};

/// Parsed command line. Unknown arguments are refused, never ignored: a typo that
/// silently selected the wrong lane is exactly the class of defect this harness
/// exists to prevent.
struct Args {
    list: bool,
    lane: &'static str,
    format: &'static str,
    self_check: bool,
    emit_registry_tsv: Option<PathBuf>,
    case: Option<String>,
    out: Option<PathBuf>,
    store: Option<PathBuf>,
    objects: Option<PathBuf>,
    prepared_input: Option<PathBuf>,
    load_input: bool,
    phase: Phase,
    hold: Option<Hold>,
    verify_sample: Option<usize>,
    /// The retained-history corpus root. Never defaulted in the binary: the reader
    /// is handed a path or it is handed nothing, and `preparation.md` §8 test 8
    /// makes an absent corpus a refusal rather than a fallback.
    corpus: Option<PathBuf>,
    /// A `history.*` row to authenticate the corpus against, without running it.
    history_corpus: Option<String>,
    /// Whether the corpus probe also walks every transition.
    history_walk: bool,
}

fn parse(args: &[String]) -> Result<Args, String> {
    let mut parsed = Args {
        list: false,
        lane: "full",
        format: "jsonl",
        self_check: false,
        emit_registry_tsv: None,
        case: None,
        out: None,
        store: None,
        objects: None,
        prepared_input: None,
        load_input: false,
        phase: Phase::Perf,
        hold: None,
        verify_sample: None,
        corpus: None,
        history_corpus: None,
        history_walk: false,
    };
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--list" => parsed.list = true,
            "--self-check" => parsed.self_check = true,
            "--corpus" => {
                index += 1;
                parsed.corpus =
                    Some(PathBuf::from(args.get(index).ok_or("--corpus expects a path")?));
            }
            "--history-corpus" => {
                index += 1;
                parsed.history_corpus = Some(
                    args.get(index)
                        .ok_or("--history-corpus expects a row id")?
                        .clone(),
                );
            }
            "--history-walk" => parsed.history_walk = true,
            "--lane" => {
                index += 1;
                parsed.lane = match args.get(index).map(String::as_str) {
                    Some("smoke") => "smoke",
                    Some("full") => "full",
                    // The three `history.*` lanes are named explicitly. They are not
                    // reachable through `smoke` or `full`, and `history-stride1` is
                    // never a default: `benchmark_rules.md` §15 forbids a default
                    // invocation launching an endurance run.
                    // `Row::id()` is `'static`, so the lane keeps `Args`' lifetime
                    // without leaking the argv string.
                    Some(lane) if HistoryRow::from_id(lane).is_some() => {
                        HistoryRow::from_id(lane).expect("just matched").id()
                    }
                    other => {
                        return Err(format!(
                            "--lane expects smoke|full|history-stride10|history-stride3|\
                             history-stride1, got {other:?}"
                        ))
                    }
                };
            }
            "--format" => {
                index += 1;
                parsed.format = match args.get(index).map(String::as_str) {
                    Some("tsv") => "tsv",
                    Some("jsonl") => "jsonl",
                    other => return Err(format!("--format expects tsv|jsonl, got {other:?}")),
                };
            }
            "--emit-registry-tsv" => {
                index += 1;
                parsed.emit_registry_tsv = Some(PathBuf::from(
                    args.get(index).ok_or("--emit-registry-tsv expects a path")?,
                ));
            }
            "--case" => {
                index += 1;
                parsed.case = Some(
                    args.get(index)
                        .ok_or("--case expects an identifier")?
                        .to_string(),
                );
            }
            "--out" => {
                index += 1;
                parsed.out = Some(PathBuf::from(args.get(index).ok_or("--out expects a path")?));
            }
            "--store" => {
                index += 1;
                parsed.store = Some(PathBuf::from(args.get(index).ok_or("--store expects a path")?));
            }
            "--objects" => {
                index += 1;
                parsed.objects =
                    Some(PathBuf::from(args.get(index).ok_or("--objects expects a path")?));
            }
            "--emit-input" => {
                index += 1;
                parsed.prepared_input =
                    Some(PathBuf::from(args.get(index).ok_or("--emit-input expects a path")?));
                parsed.load_input = false;
            }
            "--load-input" => {
                index += 1;
                parsed.prepared_input =
                    Some(PathBuf::from(args.get(index).ok_or("--load-input expects a path")?));
                parsed.load_input = true;
            }
            "--hold-save" => {
                index += 1;
                let marker = PathBuf::from(args.get(index).ok_or("--hold-save expects a path")?);
                parsed.hold = Some(Hold {
                    marker,
                    nanos: parsed.hold.as_ref().map(|hold| hold.nanos).unwrap_or(0),
                    point: parsed
                        .hold
                        .as_ref()
                        .map(|hold| hold.point)
                        .unwrap_or(HoldPoint::Begin),
                });
            }
            "--hold-ns" => {
                index += 1;
                let nanos: u64 = args
                    .get(index)
                    .ok_or("--hold-ns expects a number")?
                    .parse()
                    .map_err(|_| "--hold-ns expects a number".to_string())?;
                let marker = parsed
                    .hold
                    .as_ref()
                    .map(|hold| hold.marker.clone())
                    .ok_or("--hold-ns requires --hold-save")?;
                let point = parsed
                    .hold
                    .as_ref()
                    .map(|hold| hold.point)
                    .unwrap_or(HoldPoint::Begin);
                parsed.hold = Some(Hold { marker, nanos, point });
            }
            "--hold-at" => {
                index += 1;
                let point = match args.get(index).map(String::as_str) {
                    Some("begin") => HoldPoint::Begin,
                    Some("accept") => HoldPoint::Accept,
                    other => return Err(format!("--hold-at expects begin|accept, got {other:?}")),
                };
                let marker = parsed
                    .hold
                    .as_ref()
                    .map(|hold| hold.marker.clone())
                    .ok_or("--hold-at requires --hold-save")?;
                let nanos = parsed.hold.as_ref().map(|hold| hold.nanos).unwrap_or(0);
                parsed.hold = Some(Hold { marker, nanos, point });
            }
            "--verify-sample" => {
                index += 1;
                let units: usize = args
                    .get(index)
                    .ok_or("--verify-sample expects a number")?
                    .parse()
                    .map_err(|_| "--verify-sample expects a number".to_string())?;
                parsed.verify_sample = Some(units);
            }
            "--phase" => {
                index += 1;
                parsed.phase = match args.get(index).map(String::as_str) {
                    Some("prepare") => Phase::Prepare,
                    Some("perf") => Phase::Perf,
                    Some("verify") => Phase::Verify,
                    other => {
                        return Err(format!("--phase expects prepare|perf|verify, got {other:?}"))
                    }
                };
            }
            other => return Err(format!("unknown argument {other:?}")),
        }
        index += 1;
    }
    Ok(parsed)
}

/// Authenticates the retained-history corpus against one selection.
///
/// This is the Phase 1 probe: it proves the reader against the **real** corpus
/// without running a product row. `runner.py self-check` calls it for the three
/// rows, so a corpus that stopped authenticating is caught by the harness's own
/// self-check rather than by a measurement that then has to be discarded.
///
/// It writes one JSON object to stdout and nothing else. `--history-walk` adds a
/// full transition walk, which reads every changed blob in the selection — that is
/// 1.7 GB of I/O for `history-stride1`, so it is **not** part of `self-check`.
fn history_corpus(identifier: &str, parsed: &Args) -> ExitCode {
    let Some(row) = HistoryRow::from_id(identifier) else {
        eprintln!(
            "fs-bench-storage-content: {identifier:?} is not a history.* row; \
             expected one of history-stride10, history-stride3, history-stride1"
        );
        return ExitCode::from(2);
    };
    let Some(root) = &parsed.corpus else {
        eprintln!("fs-bench-storage-content: --history-corpus requires --corpus PATH");
        return ExitCode::from(2);
    };
    let mut corpus = match Corpus::open(root, row) {
        Ok(corpus) => corpus,
        Err(error) => {
            eprintln!("fs-bench-storage-content: {}: {error}", error.name());
            return ExitCode::FAILURE;
        }
    };
    let pins = corpus.pins();

    let states: Vec<String> = corpus
        .states()
        .iter()
        .map(|state| {
            format!(
                "{{\"ordinal\":{},\"full157_index\":{},\"sha\":\"{}\",\"paths\":{},\
                 \"logical_bytes\":{},\"oracle_sha256\":\"{}\"}}",
                state.ordinal,
                state.full157_index,
                state.sha,
                state.paths,
                state.logical_bytes,
                state.oracle_sha256
            )
        })
        .collect();

    let mut walk = String::new();
    if parsed.history_walk {
        let mut per_state = Vec::with_capacity(pins.states);
        let mut blobs = 0u64;
        let mut blob_bytes = 0u64;
        for position in 0..pins.states {
            let transition = match corpus.transition(position) {
                Ok(transition) => transition,
                Err(error) => {
                    eprintln!("fs-bench-storage-content: {}: {error}", error.name());
                    return ExitCode::FAILURE;
                }
            };
            blobs += transition.blobs.len() as u64;
            blob_bytes += transition
                .blobs
                .values()
                .map(|bytes| bytes.len() as u64)
                .sum::<u64>();
            per_state.push(format!(
                "{{\"ordinal\":{},\"added\":{},\"modified\":{},\"removed\":{},\
                 \"metadata_only\":{},\"changed_paths\":{},\"changed_bytes\":{},\
                 \"blobs\":{}}}",
                transition.state.ordinal,
                transition.count(history::Change::Added),
                transition.count(history::Change::Modified),
                transition.count(history::Change::Removed),
                transition.count(history::Change::MetadataOnly),
                transition.changed.len(),
                transition.changed_bytes(),
                transition.blobs.len()
            ));
        }
        walk = format!(
            ",\"walk\":{{\"blobs\":{blobs},\"blob_bytes\":{blob_bytes},\"per_state\":[{}]}}",
            per_state.join(",")
        );
    }

    println!(
        "{{\"row\":\"{}\",\"states\":{},\"path_states\":{},\"logical_bytes\":{},\
         \"manifest_files\":{},\"selection\":[{}],\"states_detail\":[{}]{walk}}}",
        row.id(),
        pins.states,
        pins.path_states,
        pins.logical_bytes,
        pins.manifest_files,
        row.selection()
            .iter()
            .map(u16::to_string)
            .collect::<Vec<_>>()
            .join(","),
        states.join(",")
    );
    ExitCode::SUCCESS
}

fn list(lane: &str, format: &str) {
    // The `history.*` lanes are their own lanes and are **not** members of
    // `--lane full` or `--smoke`: the group is outside the 217, and a lane that
    // cannot fit a budget is recorded `NOT_RUN` rather than reached by a default
    // invocation (`benchmark_rules.md` §15).
    if HistoryRow::from_id(lane).is_some() {
        for case in registry::history_lane(lane) {
            println!(
                "{}\t{}\t{}",
                case.id,
                case.family,
                match case.admission {
                    registry::Admission::Admission => "admission",
                    registry::Admission::Diagnostic => "diagnostic",
                }
            );
        }
        return;
    }
    let selected = if lane == "smoke" {
        registry::smoke_cases()
    } else {
        registry::cases().to_vec()
    };
    if format == "tsv" {
        for case in selected {
            println!(
                "{}\t{}\t{}",
                case.id,
                case.family,
                match case.admission {
                    registry::Admission::Admission => "admission",
                    registry::Admission::Diagnostic => "diagnostic",
                }
            );
        }
        return;
    }
    for case in selected {
        println!(
            "{{\"id\":\"{}\",\"family\":\"{}\",\"tier\":{},\"smoke\":{}}}",
            case.id,
            case.family,
            case.tier,
            case.smoke
        );
    }
}

/// Runs the registry self-check and returns its exit status.
fn self_check() -> ExitCode {
    let problems = registry::self_check();
    let counts = registry::cardinality();
    println!("frozen cardinality : {counts:?}");
    println!("cardinality sum    : {}", counts.iter().sum::<usize>());
    println!("registered rows    : {}", registry::cases().len());
    println!("admission rows     : {}", registry::admission_cases().len());
    println!("diagnostic rows    : {}", registry::cases().len() - registry::admission_cases().len());
    println!("smoke lane         : {}", registry::smoke_cases().len());
    // Outside the 217 and printed as its own line, so a reader cannot fold it in.
    println!(
        "history group      : {} (outside the 217)",
        registry::history_cases().len()
    );
    if let Ok(expected) = Expected::load() {
        let with_digest = registry::admission_cases()
            .iter()
            .filter(|case| expected.digest(case.id, "file_root").is_some()
                || expected.digest(case.id, "filesystem_root").is_some()
                || expected.digest(case.id, "members").is_some()
                || expected.digest(case.id, "additions").is_some()
                || expected.digest(case.id, "leaves").is_some()
                || expected.digest(case.id, "supplied").is_some())
            .count();
        println!(
            "pinned constants   : {} row(s); O1 identity pinned for {with_digest} of {} admission case(s)",
            expected.len(),
            registry::admission_cases().len()
        );
    }
    if problems.is_empty() {
        println!("registry self-check: PASS");
        return ExitCode::SUCCESS;
    }
    for problem in problems {
        println!(
            "MISMATCH {}: expected {}, actual {}",
            problem.what, problem.expected, problem.actual
        );
    }
    println!("registry self-check: FAIL");
    ExitCode::FAILURE
}

/// Runs one registered case and writes its raw trace.
///
/// The child never publishes a status of its own: it writes the window, counter,
/// oracle and gate records it observed, and `runner.py` re-derives the row status
/// from them. A child that published its own verdict would be the collector
/// marking its own homework.
fn run_case(identifier: &str, parsed: &Args) -> ExitCode {
    // The `history.*` group is resolved first and separately: it is not a member of
    // `registry::cases()`, so a single lookup over the 220 would report a registered
    // history row as unregistered.
    let history = registry::history_cases()
        .iter()
        .find(|case| case.id == identifier);
    let Some(case) = history.or_else(|| registry::cases().iter().find(|case| case.id == identifier))
    else {
        eprintln!("fs-bench-storage-content: no registered case {identifier:?}");
        return ExitCode::from(2);
    };
    let Some(output) = parsed.out.clone() else {
        eprintln!("fs-bench-storage-content: --case requires --out");
        return ExitCode::from(2);
    };
    // The performance invocation owns the fresh output directory. The
    // verification invocation continues in it: it appends to the same trace and
    // reads the Store the measured phase wrote, so refusing an existing path there
    // would refuse the phase its own input.
    if output.exists() && parsed.phase != Phase::Verify {
        eprintln!(
            "fs-bench-storage-content: {} already exists; receipts are append-only",
            output.display()
        );
        return ExitCode::from(2);
    }
    if let Err(error) = std::fs::create_dir_all(&output) {
        eprintln!("fs-bench-storage-content: {}: {error}", output.display());
        return ExitCode::from(2);
    }
    // The declared phase clock starts here, before anything the child does, so the
    // four phases it publishes are spans of this invocation and the reconciliation
    // against the runner's process wall is a real inequality rather than a
    // tautology.
    phases::begin(&output);
    if parsed.phase == Phase::Prepare {
        // The acquisition invocation measures nothing: its whole wall is
        // preparation, and saying so is what keeps `preparation_wall_ns` a
        // measurement rather than a remainder.
        phases::preparation_is_the_invocation();
    }
    let trace_path = output.join("trace.jsonl");
    // A verification invocation continues the performance invocation's trace. A
    // preparation invocation writes its own: it is not part of the row's measured
    // record, and mixing an acquisition into the row's gates would let setup work
    // decide a measured row.
    let opened = if parsed.phase == Phase::Verify {
        TraceWriter::append(&trace_path)
    } else {
        TraceWriter::create(&trace_path)
    };
    let mut trace = match opened {
        Ok(trace) => trace,
        Err(error) => {
            eprintln!("fs-bench-storage-content: {error}");
            return ExitCode::from(2);
        }
    };
    let header = [
        ("case_id", case.id.to_string()),
        ("family", case.family.to_string()),
        (
            "admission",
            match case.admission {
                Admission::Admission => "admission".to_string(),
                Admission::Diagnostic => "diagnostic".to_string(),
            },
        ),
        ("tier", case.tier.to_string()),
        ("tier_label", case.tier_label.to_string()),
        ("profile", case.profile.to_string()),
        ("declared_bytes", case.bytes.to_string()),
        ("declared_entries", case.entries.to_string()),
        ("cache_state", format!("{:?}", case.cache)),
        ("store_state", format!("{:?}", case.store)),
        ("shape", format!("{:?}", case.shape)),
        ("phase", format!("{:?}", parsed.phase)),
    ];
    for (key, value) in header {
        if let Err(error) = trace.write(Kind::Run, key, &value, "", "registry row") {
            eprintln!("fs-bench-storage-content: {error}");
            return ExitCode::from(2);
        }
    }
    let mut context = OpContext {
        output: &output,
        store: parsed.store.clone(),
        objects: parsed.objects.clone(),
        prepared_input: parsed.prepared_input.clone(),
        load_input: parsed.load_input,
        phase: parsed.phase,
        hold: parsed.hold.clone(),
        verify_sample: parsed.verify_sample,
        corpus: parsed.corpus.clone(),
        trace: &mut trace,
    };
    let outcome = ops::run(case, &mut context);
    // The phase snapshot is taken after the driver has returned and before the
    // trace is flushed, so `cleanup_ns` is the teardown the child performed and not
    // the cost of writing its own evidence. It is published even when the row is
    // `NOT_RUN`: a phase that was paid for is a phase that is reported.
    let snapshot = phases::snapshot();
    let invocation = match parsed.phase {
        Phase::Prepare => "prepare",
        Phase::Perf => "perf",
        Phase::Verify => "verify",
    };
    let phases_path = output.join(format!("phases-{invocation}.json"));
    if let Err(error) = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&phases_path)
        .and_then(|mut file| {
            use std::io::Write;
            file.write_all(phases::render(invocation, &snapshot).as_bytes())
        })
    {
        eprintln!("fs-bench-storage-content: {}: {error}", phases_path.display());
        return ExitCode::from(2);
    }
    let outcome = match outcome {
        Ok(outcome) => outcome,
        Err(error) => {
            let _ = trace.write(Kind::Gate, "row.not-run", &error.to_string(), "", "driver");
            let _ = trace.flush();
            println!("{} NOT_RUN {}", case.id, error);
            return ExitCode::SUCCESS;
        }
    };
    // The pinned-constant gates: O3 against `tests/golden/expected.tsv` and O1
    // against its pinned identity digests. They are applied here rather than in a
    // driver so that no family can omit them, and they read the trace back off disk
    // so what they gate is what the evidence carries.
    let pinned = if parsed.phase == Phase::Perf {
        match pinned_gates(case.id, &mut trace) {
            Ok(gates) => gates,
            Err(error) => {
                eprintln!("fs-bench-storage-content: {error}");
                return ExitCode::from(2);
            }
        }
    } else {
        Vec::new()
    };
    for gate in &outcome.gates {
        if let Err(error) = trace.write(
            Kind::Gate,
            gate.id,
            &format!("{}|{}|{}", gate.status.token(), gate.measured, gate.limit),
            "",
            gate.class.token(),
        ) {
            eprintln!("fs-bench-storage-content: {error}");
            return ExitCode::from(2);
        }
    }
    for gate in &pinned {
        if let Err(error) = trace.write(
            Kind::Gate,
            gate.id,
            &format!("{}|{}|{}", gate.status.token(), gate.measured, gate.limit),
            "",
            gate.class.token(),
        ) {
            eprintln!("fs-bench-storage-content: {error}");
            return ExitCode::from(2);
        }
    }
    for note in &outcome.notes {
        if let Err(error) = trace.write(Kind::Receipt, "note", note, "", "driver declaration") {
            eprintln!("fs-bench-storage-content: {error}");
            return ExitCode::from(2);
        }
    }
    if let Err(error) = trace.flush() {
        eprintln!("fs-bench-storage-content: {error}");
        return ExitCode::from(2);
    }
    println!(
        "{} {} gates={} trace_bytes={}",
        case.id,
        outcome.status().token(),
        outcome.gates.len(),
        trace.bytes()
    );
    ExitCode::SUCCESS
}

/// Applies the frozen oracle's pinned constants to what the row published.
///
/// The counters come from the trace itself, so the gate is a statement about the
/// published evidence; the identity digests come from the driver's `oracle`
/// records, which is where a row declares the roots it produced.
fn pinned_gates(
    case_id: &str,
    trace: &mut TraceWriter,
) -> Result<Vec<fs_bench_storage_content::gates::Gate>, String> {
    trace.flush().map_err(|error| error.to_string())?;
    let expected = Expected::load()?;
    let records = fs_bench_storage_content::support::trace::read_flat(trace.path())?;
    let counters: Vec<(String, i128)> = records
        .iter()
        .filter(|record| record.kind == "counter" && record.numeric)
        .filter_map(|record| record.value.parse::<i128>().ok().map(|value| (record.key.clone(), value)))
        .collect();
    let mut gates = expected.counter_gates(case_id, &counters);
    for record in records.iter().filter(|record| record.kind == "oracle") {
        let Some(name) = record.key.strip_prefix("identity.") else {
            continue;
        };
        gates.push(expected.digest_gate(
            case_id,
            name,
            "g1.o1-pinned-identity",
            &record.value,
        ));
    }
    Ok(gates)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let parsed = match parse(&args) {
        Ok(parsed) => parsed,
        Err(error) => {
            eprintln!("fs-bench-storage-content: {error}");
            return ExitCode::from(2);
        }
    };
    if let Some(path) = &parsed.emit_registry_tsv {
        if let Err(error) = std::fs::write(path, registry::render_tsv()) {
            eprintln!("fs-bench-storage-content: {}: {error}", path.display());
            return ExitCode::FAILURE;
        }
        return ExitCode::SUCCESS;
    }
    if let Some(identifier) = &parsed.history_corpus {
        return history_corpus(identifier, &parsed);
    }
    if parsed.self_check {
        return self_check();
    }
    if parsed.list {
        list(parsed.lane, parsed.format);
        return ExitCode::SUCCESS;
    }
    if let Some(identifier) = &parsed.case {
        return run_case(identifier, &parsed);
    }
    eprintln!("fs-bench-storage-content: nothing to do");
    ExitCode::from(2)
}
