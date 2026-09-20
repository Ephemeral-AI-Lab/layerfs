#![forbid(unsafe_code)]
mod config;
mod headless;
mod run;
fn main() -> Result<(), layerfs_bridge::contract::Failure> {
    run::run()
}
