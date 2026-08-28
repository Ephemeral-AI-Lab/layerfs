#[cfg(any(target_os = "linux", test))]
mod process;

#[cfg(target_os = "linux")]
fn main() {
    process::main();
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("layerfs-mount requires Linux");
}
