//! Versioned content scope; all-state structural checks live in verify.rs.
use super::{
    producer::OpError,
    workload::{
        digest,
        history::{Corpus, State},
    },
};
use std::{
    collections::BTreeMap,
    io::{Read, Seek, SeekFrom},
    path::PathBuf,
};
pub const POLICY: &str = "all-state-structure-five-anchor-bounded-content-v1";
pub const LOGICAL_LIMIT: u64 = 8 * 1024 * 1024;
pub const ACQUIRED_LIMIT: u64 = 32 * 1024 * 1024;
#[derive(Clone, Copy)]
pub struct StateScope<'a> {
    pub corpus: &'a Corpus,
    pub state: &'a State,
    pub anchor: bool,
    pub sources: &'a Sources,
}
pub type RangeBytes = Vec<(std::ops::Range<u64>, Vec<u8>)>;
pub fn anchor(position: usize, count: usize) -> bool {
    [
        0,
        (count - 1) / 4,
        (count - 1) / 2,
        3 * (count - 1) / 4,
        count - 1,
    ]
    .contains(&position)
}
/// Closed prepared corpus blob paths; bytes are read only inside content proof.
pub struct Sources(BTreeMap<String, PathBuf>);
impl Sources {
    pub fn open(corpus: &Corpus) -> Result<Self, OpError> {
        let mut paths = BTreeMap::new();
        for entry in std::fs::read_dir(corpus.root().join("inputs")).map_err(io_error)? {
            let path = entry.map_err(io_error)?.path().join("blobs");
            if !path.is_dir() {
                continue;
            }
            for blob in std::fs::read_dir(path).map_err(io_error)? {
                let blob = blob.map_err(io_error)?;
                if !blob.file_type().map_err(io_error)?.is_file() {
                    return Err(OpError::Io("source blob must be a regular file".into()));
                }
                let name = blob
                    .file_name()
                    .into_string()
                    .map_err(|_| OpError::Io("non-UTF8 source blob".into()))?;
                if name.len() != 40 || !name.bytes().all(|b| b.is_ascii_hexdigit()) {
                    return Err(OpError::Io("source blob name shape".into()));
                }
                paths.entry(name).or_insert(blob.path());
            }
        }
        Ok(Self(paths))
    }
    pub fn ranges(
        &self,
        corpus: &Corpus,
        state: &State,
        path: &[u8],
        size: u64,
    ) -> Result<RangeBytes, OpError> {
        let manifest = std::fs::read(
            corpus
                .root()
                .join("inputs")
                .join(&state.sha)
                .join("manifest.tsv"),
        )
        .map_err(io_error)?;
        if digest::hex(&digest::sha256(&manifest)) != state.manifest_sha256 {
            return Err(OpError::Io("anchor manifest identity mismatch".into()));
        }
        let text = std::str::from_utf8(&manifest)
            .map_err(|_| OpError::Io("anchor manifest text".into()))?;
        let mut oid = None;
        for line in text.lines().filter(|line| !line.is_empty()) {
            let fields: Vec<_> = line.split('\t').collect();
            if fields.len() != 4 {
                return Err(OpError::Io("anchor manifest columns".into()));
            }
            if hex_bytes(fields[3]).as_deref() == Some(path) {
                if oid.is_some() || fields[2].parse::<u64>().ok() != Some(size) {
                    return Err(OpError::Io("anchor source binding".into()));
                }
                oid = Some(fields[1].to_string());
            }
        }
        let source = self
            .0
            .get(&oid.ok_or_else(|| OpError::Io("anchor source absent".into()))?)
            .ok_or_else(|| OpError::Io("anchor source blob absent".into()))?;
        let mut file = std::fs::File::open(source).map_err(io_error)?;
        if file.metadata().map_err(io_error)?.len() != size {
            return Err(OpError::Io("anchor source size".into()));
        }
        let width = size.min(16 * 1024);
        let mut ranges = Vec::new();
        for start in [0, (size - width) / 2, size - width] {
            file.seek(SeekFrom::Start(start)).map_err(io_error)?;
            let mut bytes = vec![0; width as usize];
            file.read_exact(&mut bytes).map_err(io_error)?;
            ranges.push((start..start + width, bytes));
        }
        Ok(ranges)
    }
}

fn io_error(error: std::io::Error) -> OpError {
    OpError::Io(error.to_string())
}
fn hex_bytes(text: &str) -> Option<Vec<u8>> {
    if text.len() % 2 != 0 {
        return None;
    }
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let text = std::str::from_utf8(pair).ok()?;
            u8::from_str_radix(text, 16).ok()
        })
        .collect()
}
