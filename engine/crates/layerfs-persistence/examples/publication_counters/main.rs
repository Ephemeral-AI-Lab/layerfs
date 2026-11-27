//! Frozen matched component screens for publication accounting (#327), one
//! module per format version, selected by `SCREEN_SCENARIO`. No SDK/FUSE
//! timing claim.
mod v2;
mod v3;
mod v4;
mod v5;

fn main() {
    match std::env::var("SCREEN_SCENARIO").as_deref() {
        Ok("issue327-publication-totals-v2") => v2::main(),
        Ok("issue327-publication-totals-v3") => v3::main(),
        Ok("issue327-publication-totals-v4") => v4::main(),
        Ok("issue327-publication-totals-v5") => v5::main(),
        other => panic!("unknown SCREEN_SCENARIO {other:?}"),
    }
}
