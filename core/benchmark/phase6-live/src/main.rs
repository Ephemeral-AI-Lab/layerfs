#[cfg(target_os = "linux")]
use phase6_live_probe::daemon;
use phase6_live_probe::driver;
fn main() {
    let args: Vec<_> = std::env::args().collect();
    #[cfg(target_os = "linux")]
    if args.get(1).is_some_and(|s| s == "--idle-sandbox") {
        if let Err(e) = daemon::run() {
            eprintln!("daemon failed: {e}");
            std::process::exit(1)
        }
        return;
    }
    if args.len() != 5 || args[1] != "smoke" {
        eprintln!("usage: phase6-live-probe smoke OUTPUT PRIVATE_CONFIG IMAGE_SHA256");
        std::process::exit(2)
    }
    if let Err(e) = driver::run(
        std::path::Path::new(&args[2]),
        std::path::Path::new(&args[3]),
        &args[4],
    ) {
        eprintln!("integration diagnostic failed: {e}");
        std::process::exit(1)
    }
}
