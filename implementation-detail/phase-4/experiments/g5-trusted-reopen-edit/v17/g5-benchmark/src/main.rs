#[cfg(not(target_os = "macos"))]
compile_error!("the G5 trusted transport requires Darwin borrowed argv");

#[allow(dead_code)]
mod product {
    include!(concat!(env!("OUT_DIR"), "/retained_control.rs"));
    include!("session.rs");
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    product::g5_transport_main()
}
