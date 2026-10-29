//! A Store file's history heads digest (#506 E2): the sha256 of the sorted
//! heads lines Persistence reads (`layerfs_persistence::read_heads`), one
//! line each followed by a newline. It changes exactly when history a pull
//! would bring changes, and is equal on two machines with the same history.
//! Push records it from its snapshot and as the copy's `layerfs-heads`
//! metadata; pull and clone record it from the downloaded file.
use layerfs_remote_storage::pieces;
use std::path::Path;

/// Metadata key of a pushed copy's heads digest (lowercase hex).
pub const HEADS: &str = "layerfs-heads";

/// The heads digest of the Store file at `path`, read on a connection of
/// its own that is closed before this returns.
pub fn heads(path: &Path) -> Result<[u8; 32], String> {
    let lines = layerfs_persistence::read_heads(path)
        .map_err(|error| format!("reading the heads of {}: {error:?}", path.display()))?;
    let mut text = String::new();
    for line in lines {
        text.push_str(&line);
        text.push('\n');
    }
    Ok(pieces::sha256(text.as_bytes()))
}
