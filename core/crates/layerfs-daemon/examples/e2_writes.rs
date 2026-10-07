//! One E04 uncontrolled diagnostic through actual host and Workspace owners.
//! Functional watchdogs belong to this external collector, not the product.
#[path = "e2_startup/streams.rs"]
#[allow(dead_code)]
mod common_streams;
#[path = "e2_writes/control.rs"]
#[allow(dead_code)] // Shared host and consumer lifecycle sides.
mod control;
#[path = "../../../benchmark/fs-bench-pro-storage-content/src/workload/digest.rs"]
#[allow(dead_code, clippy::needless_range_loop)]
mod digest;
#[path = "e2_writes/driver.rs"]
mod driver;
#[path = "e2_writes/fixture.rs"]
#[allow(dead_code)] // Shared host helpers and retained failure owners are intentional.
mod fixture;
#[path = "e2_startup/json.rs"]
mod json;
#[path = "e2_startup/records.rs"]
#[allow(dead_code)]
mod observed;
#[path = "e2_writes/oracle.rs"]
mod oracle;
#[path = "e2_writes/outcomes.rs"]
mod outcomes;
#[path = "e2_writes/ports.rs"]
mod ports;
#[path = "e2_writes/records.rs"]
mod records;
#[path = "e2_writes/streams.rs"]
mod streams;

fn main() {
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() != 9 {
        eprintln!("usage: e2_writes ENDPOINT ASSIGNMENT BASE_INPUT REPLACEMENTS DATABASE_ABSOLUTE OUTPUT_UNUSED IDENTITY_JSON CONTROL_CONFIG");
        std::process::exit(2);
    }
    if !cfg!(target_os = "linux") {
        eprintln!("E04_DIAGNOSTIC_FAILED actual Linux consumer required");
        std::process::exit(2);
    }
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_secs(90));
        eprintln!("E04_DIAGNOSTIC_FAILED external functional watchdog90s");
        std::process::exit(3);
    });
    match driver::collect(&args) {
        Ok(()) => println!("E04_DIAGNOSTIC_RECORDED qualification=NOT_EVALUATED samples=0"),
        Err(original) => {
            eprintln!("E04_DIAGNOSTIC_FAILED {original:?}");
            // process::exit retains actual owners through the deciding failure.
            // No Drop/reconnect/reply release/Close is guessed on a failed path.
            std::process::exit(1);
        }
    }
}
