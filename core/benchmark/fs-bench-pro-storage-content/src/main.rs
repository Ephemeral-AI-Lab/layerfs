//! The harness entry point: argv to exactly one operation, then one trace file.
//!
//! This binary never selects a case by itself and never loops over a family. It
//! runs the one case it was handed, writes `trace.jsonl` beside the caller's
//! output path, and prints the facts the runner needs. Everything reusable lives
//! in the library, because a binary crate cannot be imported by `tests/*.rs`.

use std::path::PathBuf;
use std::process::ExitCode;

use fs_bench_storage_content::registry;

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
}

fn parse(args: &[String]) -> Result<Args, String> {
    let mut parsed = Args {
        list: false,
        lane: "full",
        format: "jsonl",
        self_check: false,
        emit_registry_tsv: None,
        case: None,
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
    if let Some(case) = &parsed.case {
        eprintln!("fs-bench-storage-content: case {case} has no driver yet");
        return ExitCode::from(3);
    }
    eprintln!("fs-bench-storage-content: nothing to do");
    ExitCode::from(2)
}
