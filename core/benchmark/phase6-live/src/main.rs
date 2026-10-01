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
    if args.get(1).is_some_and(|s| s == "verify") && (args.len() == 4 || args.len() == 5) {
        let scenario = if args.len() == 5 {
            match phase6_live_probe::scenario::Scenario::load(std::path::Path::new(&args[4])) {
                Ok(s) => Some(s),
                Err(e) => {
                    eprintln!("proof input: {e}");
                    std::process::exit(2)
                }
            }
        } else {
            None
        };
        if let Err(e) = phase6_live_probe::proof::verify(
            std::path::Path::new(&args[2]),
            std::path::Path::new(&args[3]),
            scenario.as_ref(),
        ) {
            eprintln!("proof failed: {e}");
            std::process::exit(1)
        }
        return;
    }
    if !((args.len() == 5 && args[1] == "smoke") || (args.len() == 6 && args[1] == "scenario")) {
        eprintln!("usage: phase6-live-probe smoke OUTPUT PRIVATE_CONFIG IMAGE_SHA256 | scenario OUTPUT PRIVATE_CONFIG IMAGE_SHA256 CASE_FILE");
        std::process::exit(2)
    }
    let scenario = if args[1] == "scenario" {
        match phase6_live_probe::scenario::Scenario::load(std::path::Path::new(&args[5])) {
            Ok(s) => Some(s),
            Err(e) => {
                eprintln!("scenario input failed: {e}");
                std::process::exit(2)
            }
        }
    } else {
        None
    };
    if let Err(e) = driver::run(
        std::path::Path::new(&args[2]),
        std::path::Path::new(&args[3]),
        &args[4],
        scenario.as_ref(),
    ) {
        eprintln!("integration diagnostic failed: {e}");
        std::process::exit(1)
    }
}
