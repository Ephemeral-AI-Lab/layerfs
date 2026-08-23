#[allow(dead_code)]
mod retained_g4 {
    include!(concat!(env!("OUT_DIR"), "/retained_control.rs"));
    include!("h11.rs");
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    retained_g4::h11_main()
}

