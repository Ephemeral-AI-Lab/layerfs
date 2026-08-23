#[allow(dead_code)]
mod product {
    include!(concat!(env!("OUT_DIR"), "/retained_control.rs"));
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    product::main_entry()
}
