//! Fixed direct-engine diagnostic selection; independent observers own phase samples.
#[path = "engine_finite/driver.rs"]
mod driver;
#[allow(dead_code)]
#[path = "e2_startup/json.rs"]
mod json;
#[path = "engine_finite/receipt.rs"]
mod receipt;
#[allow(dead_code)]
#[path = "e2_startup/records.rs"]
mod records;
use std::io::{BufRead, Write};
fn main() {
    let args = std::env::args_os().collect::<Vec<_>>();
    assert_eq!(
        args.len(),
        3,
        "engine_finite FRESH_DATABASE_DIRECTORY FRESH_RECEIPTS_JSONL"
    );
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_secs(90));
        std::process::exit(124);
    });
    driver::run(
        std::path::Path::new(&args[1]),
        std::path::Path::new(&args[2]),
        |phase| {
            println!("ENGINE_PHASE {phase} pid={}", std::process::id());
            std::io::stdout().flush().unwrap();
            let mut line = String::new();
            assert!(std::io::stdin().lock().read_line(&mut line).unwrap() > 0);
            assert_eq!(line, "continue\n");
        },
    );
}
