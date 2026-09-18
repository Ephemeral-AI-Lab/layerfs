//! The fixture properties the case registry depends on.
//!
//! A fixture is a recipe, and a recipe that only *usually* produces the shape a
//! row's ID claims makes that row a coin flip. Where a row's direction rests on a
//! property of the frozen chunker rather than on a declared byte range, the
//! property is asserted here, so a profile change fails loudly instead of quietly
//! inverting a direction.

use fs_bench_storage_content::fixture;
use layerfs_content::file::cdc::{FastCdc, TARGET_CHUNK_BYTES};
use std::io::Cursor;

/// The declared chunk-dense run is cut well below the profile's target chunk.
///
/// `CHUNK_DENSE_PAIR` exists so C1-3's `decrease` row overwrites a window that is
/// genuinely denser than the zero run replacing it. If the frozen profile ever
/// stops cutting this run at its minimum, the run stops being dense, the row's
/// direction becomes the coin flip it was before the fixture was corrected, and
/// this assertion says so before a run reports it as a finding.
#[test]
fn the_declared_chunk_dense_run_is_cut_below_the_profile_target() {
    let run = fixture::chunk_dense(32 * 1024);
    let mut sizes = Vec::new();
    FastCdc::new()
        .scan(Cursor::new(&run), |chunk| {
            sizes.push(chunk.len());
            Ok(())
        })
        .expect("a run scans");
    assert_eq!(
        sizes.len(),
        4,
        "a 32 KiB run of the declared pair is cut into four chunks, not {sizes:?}"
    );
    for size in &sizes {
        assert!(
            *size <= TARGET_CHUNK_BYTES,
            "a chunk of {size} bytes reaches past the profile target {TARGET_CHUNK_BYTES}: {sizes:?}"
        );
    }
    // The control: a zero run of the same length is cut at the profile maximum, so
    // the dense run really is denser and the row's direction is not symmetric.
    let mut zero_sizes = Vec::new();
    FastCdc::new()
        .scan(Cursor::new(&fixture::zeros(32 * 1024)), |chunk| {
            zero_sizes.push(chunk.len());
            Ok(())
        })
        .expect("a zero run scans");
    assert!(
        sizes.len() > zero_sizes.len(),
        "the dense run ({sizes:?}) must hold more extents than a zero run ({zero_sizes:?})"
    );
}
