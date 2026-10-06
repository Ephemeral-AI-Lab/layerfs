//! One diagnostic E01 startup attempt; no performance admission or new runner.
#[path = "../../../benchmark/fs-bench-pro-storage-content/src/workload/digest.rs"]
#[allow(dead_code, clippy::needless_range_loop)]
mod digest;
#[path = "e2_startup/driver.rs"]
mod driver;
#[path = "e2_startup/json.rs"]
mod json;
#[path = "e2_startup/records.rs"]
mod records;
#[path = "e2_startup/streams.rs"]
mod streams;

fn main() {
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() != 4 {
        eprintln!("usage: e2_startup DATABASE_ABSOLUTE OUTPUT_UNUSED IDENTITY_JSON");
        std::process::exit(2);
    }
    // External functional watchdog, independent of product startup/Stop waits.
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_secs(90));
        eprintln!("E2_STARTUP_FAILED external functional watchdog 90s");
        std::process::exit(3);
    });
    match driver::collect(
        std::path::Path::new(&args[1]),
        std::path::Path::new(&args[2]),
        std::path::Path::new(&args[3]),
    ) {
        Ok(()) => println!("E2_STARTUP_DIAGNOSTIC_RECORDED qualification=NOT_EVALUATED"),
        Err(failure) => {
            eprintln!("E2_STARTUP_FAILED {failure:?}");
            // Preserve original pending/completion/owner custody until exit.
            // No guessed Close, retry, output truncation or artifact deletion.
            std::process::exit(1);
        }
    }
}
