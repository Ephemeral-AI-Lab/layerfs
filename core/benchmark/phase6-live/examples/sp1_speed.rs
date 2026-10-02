//! Prospective SP1 component screen; orchestration lives in fs-bench-pro.
#[path = "sp1_speed/mod.rs"]
mod speed;
fn main() {
    if let Err(error) = speed::main() {
        eprintln!("SP1 speed driver: {error}");
        std::process::exit(1);
    }
}
