//! Exact text edits scan first, then shift the tail through bounded byte windows.
use super::mutation::{Mutation, Result};
use crate::history::lookup as args;
use layerfs_bridge::control::SdkValue;
use layerfs_workspace::Operation;
impl Mutation<'_> {
    pub fn edit(&mut self, path: &str, old: &str, new: &str) -> Result<()> {
        if old.is_empty() {
            return Err(args::invalid("oldText is empty"));
        }
        let file = self.resolve(path)?;
        if file.kind != layerfs_content::object::inode_leaf::InodeKind::RegularFile {
            return Err(args::refuse("NotFile", path, SdkValue::Null));
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
        let (mut offset, mut state, mut matches, mut found, mut utf8) =
            (0u64, 0usize, 0usize, 0u64, Vec::new());
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
                Err(_) => return Err(args::refuse("Binary", path, SdkValue::Null)),
            };
            if utf8.contains(&0) {
                return Err(args::refuse("Binary", path, SdkValue::Null));
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
                    matches += 1;
                    found = offset + i as u64 + 1 - pattern.len() as u64;
                    state = 0;
                }
            }
            offset += bytes.len() as u64;
        }
        if matches != 1 {
            return Err(args::refuse(
                if matches == 0 {
                    "NoMatch"
                } else {
                    "AmbiguousMatch"
                },
                path,
                SdkValue::map([("matches", SdkValue::Int(matches as i64))]),
            ));
        }
        if old == new {
            return Ok(());
        }
        let tail = found + old.len() as u64;
        let length = file.logical_len as i128 + new.len() as i128 - old.len() as i128;
        let length = u64::try_from(length).map_err(args::failed)?;
        let delta = new.len() as i128 - old.len() as i128;
        match delta.cmp(&0) {
            std::cmp::Ordering::Greater => {
                let mut end = file.logical_len;
                while end > tail {
                    let start = tail.max(end.saturating_sub(32768));
                    let bytes = self.read(file.serial, start, (end - start) as u32)?;
                    self.write_at(file.serial, (start as i128 + delta) as u64, &bytes)?;
                    end = start;
                }
            }
            std::cmp::Ordering::Less => {
                let mut start = tail;
                while start < file.logical_len {
                    let count = 32768.min(file.logical_len - start);
                    let bytes = self.read(file.serial, start, count as u32)?;
                    self.write_at(file.serial, (start as i128 + delta) as u64, &bytes)?;
                    start += count;
                }
            }
            std::cmp::Ordering::Equal => {}
        }
        self.write_at(file.serial, found, new.as_bytes())?;
        self.apply(Operation::SetAttributes {
            serial: file.serial,
            mode: None,
            mtime: None,
            size: Some(length),
        })?;
        Ok(())
    }
}
