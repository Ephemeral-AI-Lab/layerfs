//! Exact text edits scan first, then shift the tail through bounded byte windows.
use super::mutation::{Mutation, Result};
use crate::history::lookup as args;
use layerfs_protocol::provision::install::SdkValue;
use layerfs_vfs::Operation;
impl Mutation<'_> {
    /// Replaces `old` with `new` in the file at `path`: its one occurrence,
    /// or with `all` every one; answers how many were replaced.
    pub fn edit(&mut self, path: &str, old: &str, new: &str, all: bool) -> Result<u64> {
        if old.is_empty() {
            return Err(args::invalid("oldText is empty"));
        }
        let file = self.resolve(path)?;
        if file.kind != layerfs_content::object::inode_leaf::InodeKind::RegularFile {
            let message = format!("{path} is not a regular file");
            return Err(args::refuse("NotFile", message, SdkValue::Null));
        }
        let pattern = old.as_bytes();
        let mut failure = vec![0; pattern.len()];
        for i in 1..pattern.len() {
            let mut j = failure[i - 1];
            while j > 0 && pattern[i] != pattern[j] {
                j = failure[j - 1];
            }
            if pattern[i] == pattern[j] {
                j += 1;
            }
            failure[i] = j;
        }
        let (mut offset, mut state, mut found, mut utf8) = (0u64, 0usize, Vec::new(), Vec::new());
        while offset < file.logical_len {
            let bytes = self.read(file.serial, offset, 32768)?;
            if bytes.is_empty() {
                return Err(args::failed("edit shortened"));
            }
            utf8.extend_from_slice(&bytes);
            let valid = match std::str::from_utf8(&utf8) {
                Ok(_) => utf8.len(),
                Err(e)
                    if e.error_len().is_none()
                        && offset + (bytes.len() as u64) < file.logical_len =>
                {
                    e.valid_up_to()
                }
                Err(_) => return Err(binary(path)),
            };
            if utf8.contains(&0) {
                return Err(binary(path));
            }
            utf8.drain(..valid);
            for (i, byte) in bytes.iter().enumerate() {
                while state > 0 && *byte != pattern[state] {
                    state = failure[state - 1];
                }
                if *byte == pattern[state] {
                    state += 1;
                }
                if state == pattern.len() {
                    found.push(offset + i as u64 + 1 - pattern.len() as u64);
                    state = 0;
                }
            }
            offset += bytes.len() as u64;
        }
        if found.is_empty() || (!all && found.len() != 1) {
            let (code, message) = match found.len() {
                0 => ("NoMatch", format!("the old text is not in {path}")),
                n => (
                    "AmbiguousMatch",
                    format!("the old text occurs {n} times in {path}; replace all of them or give more context"),
                ),
            };
            let matches = SdkValue::map([("matches", SdkValue::Int(found.len() as i64))]);
            return Err(args::refuse(code, message, matches));
        }
        if old == new {
            return Ok(found.len() as u64);
        }
        let length = file.logical_len as i128
            + (new.len() as i128 - old.len() as i128) * found.len() as i128;
        let length = u64::try_from(length).map_err(args::failed)?;
        self.rewrite(file.serial, file.logical_len, &found, old.len() as u64, new)?;
        self.apply(Operation::SetAttributes {
            serial: file.serial,
            mode: None,
            mtime: None,
            size: Some(length),
        })?;
        Ok(found.len() as u64)
    }

    /// Replaces the `old`-long text at each of `found` (in order, apart)
    /// with `new` in one pass over the file: shifted left from the front
    /// when the file shrinks, shifted right from the back when it grows,
    /// so no byte is overwritten before it is read.
    fn rewrite(&mut self, serial: u64, len: u64, found: &[u64], old: u64, new: &str) -> Result<()> {
        let delta = new.len() as i128 - old as i128;
        // segment k runs from the end of match k-1 to match k (the last one
        // to the end of the file); it moves by `delta * k`
        let segment = |k: usize| {
            let start = if k == 0 { 0 } else { found[k - 1] + old };
            (start, found.get(k).copied().unwrap_or(len))
        };
        let moved = |at: u64, k: usize| (at as i128 + delta * k as i128) as u64;
        match delta.cmp(&0) {
            std::cmp::Ordering::Less => {
                for k in 0..=found.len() {
                    let (mut start, end) = segment(k);
                    while k > 0 && start < end {
                        let count = 32768.min(end - start);
                        let bytes = self.read(serial, start, count as u32)?;
                        self.write_at(serial, moved(start, k), &bytes)?;
                        start += count;
                    }
                    if k < found.len() {
                        self.write_at(serial, moved(found[k], k), new.as_bytes())?;
                    }
                }
            }
            std::cmp::Ordering::Greater => {
                for k in (0..=found.len()).rev() {
                    if k < found.len() {
                        self.write_at(serial, moved(found[k], k), new.as_bytes())?;
                    }
                    let (start, mut end) = segment(k);
                    while k > 0 && end > start {
                        let from = start.max(end.saturating_sub(32768));
                        let bytes = self.read(serial, from, (end - from) as u32)?;
                        self.write_at(serial, moved(from, k), &bytes)?;
                        end = from;
                    }
                }
            }
            std::cmp::Ordering::Equal => {
                for at in found {
                    self.write_at(serial, *at, new.as_bytes())?;
                }
            }
        }
        Ok(())
    }
}
fn binary(path: &str) -> layerfs_protocol::provision::install::SdkRefusal {
    let message = format!("{path} is not UTF-8 text");
    args::refuse("Binary", message, SdkValue::Null)
}
