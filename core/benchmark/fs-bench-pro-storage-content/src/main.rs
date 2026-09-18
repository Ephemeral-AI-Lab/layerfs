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
    };
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--list" => parsed.list = true,
            "--self-check" => parsed.self_check = true,
            "--lane" => {
                index += 1;
                parsed.lane = match args.get(index).map(String::as_str) {
                    Some("smoke") => "smoke",
                    Some("full") => "full",
                    other => return Err(format!("--lane expects smoke|full, got {other:?}")),
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

fn list(lane: &str, format: &str) {
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
    let Some(case) = registry::cases().iter().find(|case| case.id == identifier) else {
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
        trace: &mut trace,
    };
    let outcome = match ops::run(case, &mut context) {
        Ok(outcome) => outcome,
        Err(error) => {
            let _ = trace.write(Kind::Gate, "row.not-run", &error.to_string(), "", "driver");
            let _ = trace.flush();
            println!("{} NOT_RUN {}", case.id, error);
            return ExitCode::SUCCESS;
        }
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
