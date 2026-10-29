//! What a remote holds of Stores, without downloading (#506 E3, E9): one
//! stat of a Store's copy, and one listing of every Store copy under the
//! remote's namespace with a bounded number of stats where the listing
//! lacks the copy's metadata. Each request is sent once.
use crate::remote::heads::HEADS;
use crate::remote::settings::{store_object, STORE_FILE};
use layerfs_remote_storage::pieces::{self, parse_hex32};
use layerfs_remote_storage::{Driver, Metadata, RemoteError};

/// Most stats one listing sends; copies beyond show no sha256 or heads.
pub const MAX_LIST_STATS: usize = 64;

/// One Store copy on a remote.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Copy {
    pub store: String,
    /// The whole Store file's size (for a copy stored as pieces, theirs).
    pub bytes: u64,
    /// Milliseconds since the Unix epoch, when the remote says.
    pub modified: Option<u64>,
    /// `layerfs-sha256`, when read.
    pub sha256: Option<[u8; 32]>,
    /// `layerfs-heads`, when read and recorded by the pusher.
    pub heads: Option<[u8; 32]>,
}

/// The result of one stat of a Store's copy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Checked {
    Absent,
    Present(Copy),
}

/// The copy's size, sha256 and heads from its metadata; `listed` is the
/// size of the object itself, used when the metadata has no size.
fn described(store: &str, listed: u64, modified: Option<u64>, metadata: Option<&Metadata>) -> Copy {
    let get = |key: &str| metadata.and_then(|m| m.get(key));
    Copy {
        store: store.to_owned(),
        bytes: get(pieces::BYTES)
            .and_then(|text| text.parse().ok())
            .unwrap_or(listed),
        modified,
        sha256: get(pieces::SHA256).and_then(|text| parse_hex32(text)),
        heads: get(HEADS).and_then(|text| parse_hex32(text)),
    }
}

/// One stat of the copy of Store `store`: absent, or what it records.
pub fn check(driver: &dyn Driver, store: &str) -> Result<Checked, RemoteError> {
    let object = store_object(store).map_err(RemoteError::Invalid)?;
    Ok(match driver.stat(&object)? {
        None => Checked::Absent,
        Some(stat) => Checked::Present(described(
            store,
            stat.bytes,
            stat.modified,
            Some(&stat.metadata),
        )),
    })
}

/// Every Store copy on the remote (`NAME/store.sqlite`), in name order. A
/// copy stored as pieces whose metadata is unread counts its pieces' sizes.
pub fn list_stores(driver: &dyn Driver) -> Result<Vec<Copy>, RemoteError> {
    let listed = driver.list("")?;
    let suffix = format!("/{STORE_FILE}");
    let mut copies = Vec::new();
    let mut stats = 0;
    for entry in &listed {
        let Some(store) = entry.name.strip_suffix(&suffix) else {
            continue;
        };
        if store_object(store).is_err() {
            continue;
        }
        let mut metadata = entry.metadata.clone();
        if metadata.is_none() && stats < MAX_LIST_STATS {
            stats += 1;
            metadata = driver.stat(&entry.name)?.map(|stat| stat.metadata);
        }
        let mut copy = described(store, entry.bytes, entry.modified, metadata.as_ref());
        if metadata
            .as_ref()
            .is_none_or(|m| !m.contains_key(pieces::BYTES))
        {
            let piece = format!("{}.piece-", entry.name);
            let pieced: u64 = listed
                .iter()
                .filter(|other| other.name.starts_with(&piece))
                .map(|other| other.bytes)
                .sum();
            if pieced > 0 {
                copy.bytes = pieced;
            }
        }
        copies.push(copy);
    }
    Ok(copies)
}
