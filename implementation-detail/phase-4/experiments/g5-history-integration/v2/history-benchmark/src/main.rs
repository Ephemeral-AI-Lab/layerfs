// Custody: these lints originate only in the unchanged live-included shared sources.
#[allow(
    dead_code,
    clippy::manual_pattern_char_comparison,
    clippy::needless_borrow,
    clippy::reversed_empty_ranges,
    clippy::single_range_in_vec_init,
    clippy::too_many_arguments,
    clippy::type_complexity
)]
mod retained_g4 {
    include!(concat!(env!("OUT_DIR"), "/retained_control.rs"));
    include!("history.rs");
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    retained_g4::h11_main()
}
