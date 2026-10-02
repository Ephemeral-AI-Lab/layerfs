mod json;
mod model;
mod native;
mod perf;
mod prepare;
mod verify;
use json::{object, Json};
use model::{Args, Result};
use std::{io::Write, time::Instant};
pub fn main() -> Result<()> {
    let wall = Instant::now();
    let args = Args::parse()?;
    if args.out.exists() {
        return Err("fresh append-only receipt path required".into());
    }
    let result = match args.phase.as_str() {
        "prepare" => prepare::prepare(&args),
        "setup-provider" => prepare::setup_provider(&args),
        "perf" => perf::run(&args),
        "verify" => verify::run(&args),
        _ => Err("unsupported driver phase".into()),
    };
    let mut receipt = object([
        ("schema_version", 1u64.into()),
        ("operation_contract_id", "sp1-component-speed-v1".into()),
        (
            "operation_surface",
            "experimental strict component API".into(),
        ),
        ("phase", args.phase.clone().into()),
        ("case", args.case.clone().into()),
        ("admission_eligible", false.into()),
        ("operation_ns", Json::Null),
        ("cpu_user_ns", Json::Null),
        ("cpu_system_ns", Json::Null),
        (
            "provider_bucket",
            std::env::var("SP1_MINIO_BUCKET").unwrap_or_default().into(),
        ),
        (
            "provider_endpoint",
            std::env::var("SP1_MINIO_AUTHORITY")
                .unwrap_or_default()
                .into(),
        ),
        (
            "missing_metrics",
            object([
                ("rss", "not instrumented; quota is not allocation".into()),
                (
                    "cgroup_phase_peak",
                    "host operation; provider quota only, no reset phase peak".into(),
                ),
                ("disk_device_reads", "not instrumented".into()),
                ("provider_allocation", "not instrumented".into()),
            ]),
        ),
    ]);
    let failure = match result {
        Ok(Json::Map(fields)) => {
            let failure = match fields.get("error") {
                Some(Json::Str(e)) => Some(e.clone()),
                _ => None,
            };
            if let Json::Map(ref mut map) = receipt {
                map.extend(fields);
            }
            if failure.is_none() {
                receipt.put("status", "PASS");
            }
            failure
        }
        Ok(_) => Some("receipt object required".to_owned()),
        Err(e) => Some(e),
    };
    if let Some(error) = &failure {
        receipt.put("status", "FAIL");
        receipt.put("error", error.clone());
    }
    receipt.put("wall_ns", wall.elapsed().as_nanos() as u64);
    receipt.put("wall_ns_scope","driver entry through completed operation/proof and observations, before JSON file write; external family owns complete subprocess/lifecycle wall");
    receipt.put("numeric_speed_eligibility","INELIGIBLE unless external evidence attests every required cache domain; operation status alone is not numeric latency PASS");
    if let Some(parent) = args.out.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut f = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&args.out)
        .map_err(|e| e.to_string())?;
    writeln!(f, "{}", receipt.encode()).map_err(|e| e.to_string())?;
    if let Some(error) = failure {
        return Err(error);
    }
    Ok(())
}
