//! Shared bounded fixture recipes and independent complete-tree verification.
use super::{hex, sdk_edit_common, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Component, Path};
use std::sync::Arc;

pub(crate) type Result<T> = super::Result<T>;
pub(crate) type Receipt = BTreeMap<String, String>;
pub(crate) const TIERS: [usize; 4] = [1, 10, 100, 500];
pub(crate) const MIB: u64 = 1_048_576;
pub(crate) const MAX_FILE_BYTES: u64 = 500 * MIB;
pub(crate) const MAX_TOTAL_BYTES: u64 = 1024 * MIB;
pub(crate) const MTIME: i64 = 1_700_000_000;
pub(crate) const SCRATCH_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug)]
pub(crate) struct Case {
    pub id: String,
    pub family: &'static str,
    pub tier: usize,
    pub kind: &'static str,
}

#[derive(Clone, Debug)]
pub(crate) struct SdkEdit {
    pub path: String,
    pub start: u64,
    pub delete_len: u64,
    pub replacement: Vec<u8>,
}

#[derive(Clone, Debug)]
pub(crate) enum Content {
    Seed {
        seed: u64,
        len: u64,
    },
    Zero {
        len: u64,
    },
    /// Verification-only persistence custody; never a source byte generator.
    Digest {
        len: u64,
        sha256: String,
    },
    Literal(Vec<u8>),
    Slice {
        source: Arc<Content>,
        offset: u64,
        len: u64,
    },
    Concat(Vec<Content>),
    Xor {
        source: Arc<Content>,
        offset: u64,
        len: u64,
        mask: u8,
    },
}

impl Content {
    pub(crate) fn len(&self) -> u64 {
        match self {
            Self::Seed { len, .. }
            | Self::Zero { len }
            | Self::Digest { len, .. }
            | Self::Slice { len, .. } => *len,
            Self::Literal(bytes) => bytes.len() as u64,
            Self::Concat(parts) => parts
                .iter()
                .fold(0_u64, |sum, part| sum.saturating_add(part.len())),
            Self::Xor { source, .. } => source.len(),
        }
    }

    pub(crate) fn validate(&self) -> Result<()> {
        let len = match self {
            Self::Slice {
                source,
                offset,
                len,
            }
            | Self::Xor {
                source,
                offset,
                len,
                ..
            } => {
                source.validate()?;
                if matches!(source.as_ref(), Self::Digest { .. }) {
                    return Err("digest custody cannot be transformed into source bytes".into());
                }
                if offset
                    .checked_add(*len)
                    .is_none_or(|end| end > source.len())
                {
                    return Err("content slice/XOR bounds".into());
                }
                self.len()
            }
            Self::Concat(parts) => {
                let mut sum = 0_u64;
                for part in parts {
                    part.validate()?;
                    if matches!(part, Self::Digest { .. }) {
                        return Err(
                            "digest custody cannot be concatenated into source bytes".into()
                        );
                    }
                    sum = sum
                        .checked_add(part.len())
                        .ok_or("content length overflow")?;
                }
                sum
            }
            Self::Digest { len, sha256 } => {
                if sha256.len() != 64
                    || !sha256
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                {
                    return Err(
                        "digest custody SHA-256 must be 64 lowercase hexadecimal digits".into(),
                    );
                }
                *len
            }
            _ => self.len(),
        };
        if len > MAX_FILE_BYTES {
            return Err("content exceeds 500 MiB".into());
        }
        Ok(())
    }

    pub(crate) fn read_at(&self, offset: u64, output: &mut [u8]) -> Result<usize> {
        if matches!(self, Self::Digest { .. }) {
            return Err("digest custody has no source bytes".into());
        }
        if offset >= self.len() || output.is_empty() {
            return Ok(0);
        }
        let count = output.len().min((self.len() - offset) as usize);
        let output = &mut output[..count];
        match self {
            Self::Seed { seed, .. } => {
                let word = offset / 8;
                let skip = (offset % 8) as usize;
                let mut state = seed.wrapping_add(word.wrapping_mul(0x9e37_79b9_7f4a_7c15));
                let mut written = 0;
                if skip != 0 {
                    let mut first = [0; 8];
                    sdk_edit_common::fixture_block(&mut state, &mut first);
                    written = count.min(8 - skip);
                    output[..written].copy_from_slice(&first[skip..skip + written]);
                }
                sdk_edit_common::fixture_block(&mut state, &mut output[written..]);
            }
            Self::Zero { .. } => output.fill(0),
            Self::Digest { .. } => return Err("digest custody has no source bytes".into()),
            Self::Literal(bytes) => {
                output.copy_from_slice(&bytes[offset as usize..offset as usize + count])
            }
            Self::Slice {
                source,
                offset: base,
                ..
            } => {
                if source.read_at(base.checked_add(offset).ok_or("slice overflow")?, output)?
                    != count
                {
                    return Err("short content slice".into());
                }
            }
            Self::Concat(parts) => {
                let mut start = 0_u64;
                let mut written = 0;
                for part in parts {
                    let end = start.checked_add(part.len()).ok_or("concat overflow")?;
                    let position = offset + written as u64;
                    if position < end && written < count {
                        written +=
                            part.read_at(position.saturating_sub(start), &mut output[written..])?;
                    }
                    start = end;
                    if written == count {
                        break;
                    }
                }
                if written != count {
                    return Err("short content concatenation".into());
                }
            }
            Self::Xor {
                source,
                offset: start,
                len,
                mask,
            } => {
                if source.read_at(offset, output)? != count {
                    return Err("short XOR source".into());
                }
                let end = start.checked_add(*len).ok_or("XOR overflow")?;
                let from = offset.max(*start);
                let to = (offset + count as u64).min(end);
                if from < to {
                    for byte in &mut output[(from - offset) as usize..(to - offset) as usize] {
                        *byte ^= mask;
                    }
                }
            }
        }
        Ok(count)
    }

    pub(crate) fn write_to(&self, output: &mut impl Write) -> Result<()> {
        self.validate()?;
        if matches!(self, Self::Digest { .. }) {
            return Err("digest custody cannot create a fixture".into());
        }
        let mut buffer = vec![0; SCRATCH_BYTES.min(self.len() as usize)];
        let mut offset = 0;
        while offset < self.len() {
            let count = self.read_at(offset, &mut buffer)?;
            if count == 0 {
                return Err("short fixture recipe".into());
            }
            output.write_all(&buffer[..count])?;
            offset += count as u64;
        }
        Ok(())
    }

    pub(crate) fn digest(&self) -> Result<String> {
        if let Self::Digest { sha256, .. } = self {
            self.validate()?;
            return Ok(sha256.clone());
        }
        let mut sink = HashSink(Sha256::new());
        self.write_to(&mut sink)?;
        Ok(hex(&sink.0.finish()))
    }

    pub(crate) fn slice(&self, offset: u64, len: u64) -> Result<Self> {
        let result = Self::Slice {
            source: Arc::new(self.clone()),
            offset,
            len,
        };
        result.validate()?;
        Ok(result)
    }

    pub(crate) fn splice(&self, start: u64, delete_len: u64, replacement: Self) -> Result<Self> {
        let end = start.checked_add(delete_len).ok_or("splice overflow")?;
        if end > self.len() {
            return Err("splice bounds".into());
        }
        let result = Self::Concat(vec![
            self.slice(0, start)?,
            replacement,
            self.slice(end, self.len() - end)?,
        ]);
        result.validate()?;
        Ok(result)
    }

    pub(crate) fn xor(&self, offset: u64, len: u64, mask: u8) -> Result<Self> {
        let result = Self::Xor {
            source: Arc::new(self.clone()),
            offset,
            len,
            mask,
        };
        result.validate()?;
        Ok(result)
    }

    pub(crate) fn reader(&self) -> ContentReader<'_> {
        ContentReader {
            content: self,
            position: 0,
        }
    }
}

pub(crate) struct ContentReader<'a> {
    content: &'a Content,
    position: u64,
}
impl Read for ContentReader<'_> {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        let count = self
            .content
            .read_at(self.position, output)
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        self.position += count as u64;
        Ok(count)
    }
}
impl Seek for ContentReader<'_> {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        let next = match position {
            SeekFrom::Start(value) => i128::from(value),
            SeekFrom::Current(value) => i128::from(self.position) + i128::from(value),
            SeekFrom::End(value) => i128::from(self.content.len()) + i128::from(value),
        };
        self.position =
            u64::try_from(next).map_err(|_| std::io::Error::other("content seek bounds"))?;
        Ok(self.position)
    }
}

struct HashSink(Sha256);
impl Write for HashSink {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.update(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub(crate) enum EntryKind {
    File(Content),
    Directory,
    Symlink(String),
    Hardlink(String),
}
#[derive(Clone, Debug)]
pub(crate) struct Entry {
    pub path: String,
    pub kind: EntryKind,
    pub mode: u32,
    pub mtime_seconds: i64,
    pub mtime_nanoseconds: u32,
}
impl Entry {
    pub(crate) fn file(path: impl Into<String>, content: Content) -> Self {
        Self::new(path, EntryKind::File(content), 0o640)
    }
    pub(crate) fn directory(path: impl Into<String>) -> Self {
        Self::new(path, EntryKind::Directory, 0o750)
    }
    pub(crate) fn symlink(path: impl Into<String>, target: impl Into<String>) -> Self {
        Self::new(path, EntryKind::Symlink(target.into()), 0o777)
    }
    pub(crate) fn hardlink(path: impl Into<String>, target: impl Into<String>) -> Self {
        Self::new(path, EntryKind::Hardlink(target.into()), 0o640)
    }
    fn new(path: impl Into<String>, kind: EntryKind, mode: u32) -> Self {
        Self {
            path: path.into(),
            kind,
            mode,
            mtime_seconds: MTIME,
            mtime_nanoseconds: 0,
        }
    }
}

pub(crate) fn seed_label(seed: u8) -> Result<String> {
    if !(1..=3).contains(&seed) {
        return Err("seed must be 1, 2 or 3".into());
    }
    Ok(format!("layerfs-v0.1.3-seed-{seed}"))
}
fn frame(hash: &mut Sha256, bytes: &[u8]) {
    hash.update(&(bytes.len() as u64).to_le_bytes());
    hash.update(bytes);
}
pub(crate) fn frame_seed(fields: &[&str], indices: &[u64]) -> u64 {
    let mut hash = Sha256::new();
    for field in fields {
        frame(&mut hash, field.as_bytes());
    }
    for index in indices {
        hash.update(&index.to_le_bytes());
    }
    u64::from_le_bytes(hash.finish()[..8].try_into().expect("seed width"))
}
pub(crate) fn ranked_indices(seed: u8, domain: &str, count: usize) -> Result<Vec<usize>> {
    if count > 500 {
        return Err("schedule exceeds maximum 500".into());
    }
    let label = seed_label(seed)?;
    let mut ranks = (0..500)
        .map(|index| {
            let mut hash = Sha256::new();
            frame(&mut hash, label.as_bytes());
            frame(&mut hash, domain.as_bytes());
            hash.update(&(index as u64).to_le_bytes());
            (hash.finish(), index)
        })
        .collect::<Vec<_>>();
    ranks.sort();
    Ok(ranks
        .into_iter()
        .take(count)
        .map(|(_, index)| index)
        .collect())
}

pub(crate) fn shards(seed: u8, n: usize, prefix: &str) -> Result<Vec<Entry>> {
    if n > 500 {
        return Err("shards exceed maximum 500".into());
    }
    let label = seed_label(seed)?;
    let prefix = prefix.trim_end_matches('/');
    if !prefix.is_empty() && prefix != "." {
        validate_path(prefix)?;
    }
    let join = |path: &str| {
        if prefix.is_empty() || prefix == "." {
            path.to_owned()
        } else {
            format!("{prefix}/{path}")
        }
    };
    let spine = (1..=128)
        .map(|index| format!("d{index:03}"))
        .collect::<Vec<_>>()
        .join("/");
    let mut entries = vec![Entry::directory(".")];
    let mut directories = BTreeSet::new();
    for shard in 0..n {
        for ordinal in 0..200 {
            let (path, len) = match ordinal {
                0..=63 => (format!("wide/s{shard:03}-f{ordinal:03}.dat"), 1024),
                64..=127 => (format!("regular/s{shard:03}/f{ordinal:03}.dat"), 1024),
                128..=191 => (format!("regular/s{shard:03}/f{ordinal:03}.dat"), 8192),
                192..=198 => (format!("regular/s{shard:03}/f{ordinal:03}.dat"), 49152),
                _ => (format!("spine/{spine}/s{shard:03}.dat"), 49152),
            };
            let path = join(&path);
            add_parents(&path, &mut directories);
            entries.push(Entry::file(
                path,
                Content::Seed {
                    seed: frame_seed(&["workspace-shards-v1", &label], &[shard as u64, ordinal]),
                    len,
                },
            ));
        }
    }
    let dest = join("dest");
    add_parents(&dest, &mut directories);
    directories.insert(dest);
    entries.extend(directories.into_iter().map(Entry::directory));
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    validate_entries(&entries)?;
    Ok(entries)
}
fn add_parents(path: &str, directories: &mut BTreeSet<String>) {
    let mut parent = Path::new(path).parent();
    while let Some(value) = parent {
        if value.as_os_str().is_empty() {
            break;
        }
        directories.insert(value.to_string_lossy().into_owned());
        parent = value.parent();
    }
}
fn validate_path(path: &str) -> Result<()> {
    if path != "."
        && (path.is_empty()
            || path
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == "..")
            || Path::new(path)
                .components()
                .any(|part| !matches!(part, Component::Normal(_))))
    {
        return Err(format!("non-canonical fixture path: {path}").into());
    }
    if path.as_bytes().contains(&0) || path.contains('\n') || path.contains('\t') {
        return Err("unsupported fixture path encoding".into());
    }
    Ok(())
}
pub(crate) fn validate_entries(entries: &[Entry]) -> Result<u64> {
    let map = entries
        .iter()
        .map(|entry| (entry.path.as_str(), entry))
        .collect::<BTreeMap<_, _>>();
    if map.len() != entries.len()
        || !matches!(
            map.get(".").map(|entry| &entry.kind),
            Some(EntryKind::Directory)
        )
    {
        return Err("fixture needs unique paths and explicit root directory".into());
    }
    let mut total = 0_u64;
    for entry in entries {
        validate_path(&entry.path)?;
        if entry.mode & !0o7777 != 0 || entry.mtime_nanoseconds >= 1_000_000_000 {
            return Err("fixture metadata bounds".into());
        }
        if entry.path != "." {
            let parent = Path::new(&entry.path)
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .and_then(|p| p.to_str())
                .unwrap_or(".");
            if !matches!(
                map.get(parent).map(|entry| &entry.kind),
                Some(EntryKind::Directory)
            ) {
                return Err(format!("missing directory parent: {}", entry.path).into());
            }
        }
        let bytes = match &entry.kind {
            EntryKind::File(content) => {
                content.validate()?;
                content.len()
            }
            EntryKind::Hardlink(target) => {
                validate_path(target)?;
                let target = map.get(target.as_str()).ok_or("hard-link target missing")?;
                let EntryKind::File(content) = &target.kind else {
                    return Err("hard-link target must be canonical regular entry".into());
                };
                if (entry.mode, entry.mtime_seconds, entry.mtime_nanoseconds)
                    != (target.mode, target.mtime_seconds, target.mtime_nanoseconds)
                {
                    return Err("hard-link metadata differs from target".into());
                }
                content.len()
            }
            EntryKind::Symlink(target) => {
                if target.as_bytes().contains(&0) {
                    return Err("symlink target NUL".into());
                }
                target.len() as u64
            }
            EntryKind::Directory => 0,
        };
        if bytes > MAX_FILE_BYTES {
            return Err("fixture file cap".into());
        }
        total = total
            .checked_add(bytes)
            .ok_or("fixture logical byte overflow")?;
    }
    if total >= MAX_TOTAL_BYTES {
        return Err("fixture total must be strictly below 1 GiB".into());
    }
    Ok(total)
}

pub(crate) fn create_fixture(root: &Path, entries: &[Entry]) -> Result<()> {
    validate_entries(entries)?;
    if entries
        .iter()
        .any(|entry| matches!(entry.kind, EntryKind::File(Content::Digest { .. })))
    {
        return Err("digest custody cannot create a fixture".into());
    }
    if fs::symlink_metadata(root).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err("fixture root must not be a symlink".into());
    }
    if root.exists() {
        if !root.is_dir() || fs::read_dir(root)?.next().is_some() {
            return Err("fixture output must be absent or empty".into());
        }
    } else {
        fs::create_dir_all(root)?;
    }
    let mut ordered = entries.iter().collect::<Vec<_>>();
    ordered.sort_by(|left, right| left.path.cmp(&right.path));
    for entry in &ordered {
        let path = root.join(&entry.path);
        match &entry.kind {
            EntryKind::Directory if entry.path != "." => fs::create_dir(&path)?,
            EntryKind::File(content) => {
                content.write_to(&mut File::create(&path)?)?;
            }
            EntryKind::Symlink(target) => std::os::unix::fs::symlink(target, &path)?,
            _ => (),
        }
    }
    for entry in &ordered {
        if let EntryKind::Hardlink(target) = &entry.kind {
            fs::hard_link(root.join(target), root.join(&entry.path))?;
        }
    }
    for entry in ordered.into_iter().rev() {
        set_metadata(&root.join(&entry.path), entry)?;
    }
    Ok(())
}

pub(crate) fn set_metadata(path: &Path, entry: &Entry) -> Result<()> {
    let symlink = matches!(entry.kind, EntryKind::Symlink(_));
    if symlink {
        #[cfg(target_os = "macos")]
        {
            use std::os::unix::ffi::OsStrExt;
            unsafe extern "C" {
                fn lchmod(path: *const std::ffi::c_char, mode: u16) -> i32;
            }
            let path = std::ffi::CString::new(path.as_os_str().as_bytes())?;
            if unsafe { lchmod(path.as_ptr(), entry.mode as u16) } != 0 {
                return Err(std::io::Error::last_os_error().into());
            }
        }
        #[cfg(not(target_os = "macos"))]
        if entry.mode != 0o777 {
            return Err("platform cannot set symlink mode".into());
        }
    } else {
        fs::set_permissions(path, fs::Permissions::from_mode(entry.mode))?;
    }
    set_mtime_nofollow(path, entry.mtime_seconds, entry.mtime_nanoseconds)
}

#[allow(clippy::useless_conversion, reason = "Checked conversion to platform-dependent c_long")]
pub(crate) fn set_mtime_nofollow(path: &Path, seconds: i64, nanoseconds: u32) -> Result<()> {
    use std::os::unix::ffi::OsStrExt;
    #[repr(C)]
    struct Timespec {
        seconds: std::ffi::c_long,
        nanoseconds: std::ffi::c_long,
    }
    unsafe extern "C" {
        fn utimensat(
            fd: i32,
            path: *const std::ffi::c_char,
            times: *const Timespec,
            flags: i32,
        ) -> i32;
    }
    if nanoseconds >= 1_000_000_000 {
        return Err("mtime nanoseconds".into());
    }
    let path = std::ffi::CString::new(path.as_os_str().as_bytes())?;
    let times = [
        Timespec {
            seconds: seconds.try_into()?,
            nanoseconds: nanoseconds.into(),
        },
        Timespec {
            seconds: seconds.try_into()?,
            nanoseconds: nanoseconds.into(),
        },
    ];
    #[cfg(target_os = "macos")]
    let (fd, flags) = (-2, 0x20);
    #[cfg(not(target_os = "macos"))]
    let (fd, flags) = (-100, 0x100);
    if unsafe { utimensat(fd, path.as_ptr(), times.as_ptr(), flags) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

pub(crate) fn manifest(entries: &[Entry]) -> Result<String> {
    validate_entries(entries)?;
    let mut ordered = entries.iter().collect::<Vec<_>>();
    ordered.sort_by(|left, right| left.path.cmp(&right.path));
    let mut output = String::from("workspace-independent-manifest-v1\n");
    for entry in ordered {
        let (kind, length, identity) = match &entry.kind {
            EntryKind::File(content) => ("file", content.len(), content.digest()?),
            EntryKind::Directory => ("directory", 0, "-".to_owned()),
            EntryKind::Symlink(target) => ("symlink", target.len() as u64, hex(target.as_bytes())),
            EntryKind::Hardlink(target) => ("hardlink", 0, hex(target.as_bytes())),
        };
        output.push_str(&format!(
            "{}\t{kind}\t{length}\t{:o}\t{}\t{}\t{identity}\n",
            entry.path, entry.mode, entry.mtime_seconds, entry.mtime_nanoseconds
        ));
    }
    Ok(output)
}

/// Parse an already-sealed native persistence manifest, never a source fixture.
pub(crate) fn decode_manifest(input: &str) -> Result<Vec<Entry>> {
    let mut lines = input.lines();
    if lines.next() != Some("workspace-independent-manifest-v1") {
        return Err("unsupported persistence manifest version".into());
    }
    let decode_target = |value: &str| -> Result<String> {
        if value.len() % 2 != 0
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err("manifest target must be lowercase hex".into());
        }
        let bytes = value
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| {
                let digit = |byte| {
                    if byte <= b'9' {
                        byte - b'0'
                    } else {
                        byte - b'a' + 10
                    }
                };
                (digit(pair[0]) << 4) | digit(pair[1])
            })
            .collect::<Vec<_>>();
        Ok(String::from_utf8(bytes)?)
    };
    let mut entries = Vec::new();
    for line in lines {
        let fields = line.split('\t').collect::<Vec<_>>();
        if fields.len() != 7 {
            return Err("persistence manifest requires seven columns".into());
        }
        let len = fields[2].parse::<u64>()?;
        let kind = match fields[1] {
            "file" => EntryKind::File(Content::Digest {
                len,
                sha256: fields[6].into(),
            }),
            "directory" if len == 0 && fields[6] == "-" => EntryKind::Directory,
            "symlink" => {
                let target = decode_target(fields[6])?;
                if target.len() as u64 != len {
                    return Err("manifest symlink length mismatch".into());
                }
                EntryKind::Symlink(target)
            }
            "hardlink" if len == 0 => EntryKind::Hardlink(decode_target(fields[6])?),
            _ => return Err("invalid persistence manifest entry kind/length".into()),
        };
        entries.push(Entry {
            path: fields[0].into(),
            kind,
            mode: u32::from_str_radix(fields[3], 8)?,
            mtime_seconds: fields[4].parse()?,
            mtime_nanoseconds: fields[5].parse()?,
        });
    }
    validate_entries(&entries)?;
    if manifest(&entries)? != input {
        return Err("persistence manifest is not canonical sorted encoding".into());
    }
    Ok(entries)
}

pub(crate) fn native_paths(root: &Path) -> Result<Vec<String>> {
    let mut paths = vec![".".to_owned()];
    let mut directories = vec![root.to_owned()];
    while let Some(directory) = directories.pop() {
        for item in fs::read_dir(directory)? {
            let item = item?;
            let path = item.path();
            let relative = path
                .strip_prefix(root)?
                .to_str()
                .ok_or("non-UTF8 fixture path")?
                .to_owned();
            validate_path(&relative)?;
            if fs::symlink_metadata(&path)?.is_dir() {
                directories.push(path);
            }
            paths.push(relative);
        }
    }
    paths.sort();
    Ok(paths)
}

// Verification-only observations: preserve one lstat result for every path.
fn native_metadata(root: &Path) -> Result<BTreeMap<String, fs::Metadata>> {
    let mut found = BTreeMap::from([(".".to_owned(), fs::symlink_metadata(root)?)]);
    let mut pending = vec![root.to_owned()];
    while let Some(directory) = pending.pop() {
        for item in fs::read_dir(directory)? {
            let item = item?;
            let path = item.path();
            let relative = path.strip_prefix(root)?.to_str().ok_or("non-UTF8 fixture path")?.to_owned();
            validate_path(&relative)?;
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.is_dir() { pending.push(path); }
            if found.insert(relative, metadata).is_some() { return Err("duplicate native path".into()); }
        }
    }
    Ok(found)
}

fn verify_native_observed(root: &Path, expected: &BTreeMap<String, &Entry>, observations: &BTreeMap<String, fs::Metadata>) -> Result<(usize, usize)> {
    let mut inode_classes = BTreeMap::new();
    let mut class_inodes = BTreeMap::new();
    let mut reference_counts = BTreeMap::<&str, u64>::new();
    for entry in expected.values() {
        let class = match &entry.kind {
            EntryKind::File(_) => entry.path.as_str(),
            EntryKind::Hardlink(target) => target.as_str(),
            _ => continue,
        };
        *reference_counts.entry(class).or_default() += 1;
    }
    let mut bytes = vec![0; SCRATCH_BYTES];
    let mut wanted = vec![0; SCRATCH_BYTES];
    let mut files = 0;
    let mut custody_paths = 0;
    for (relative, metadata) in observations {
        let entry = expected.get(relative).ok_or("unexpected native observation")?;
        let path = root.join(relative);
        if metadata.mode() & 0o7777 != entry.mode
            || metadata.mtime() != entry.mtime_seconds
            || metadata.mtime_nsec() != i64::from(entry.mtime_nanoseconds)
        {
            return Err(format!(
                "native metadata mismatch: {relative}: mode={:o}, mtime={}.{} expected={:o},{}.{}",
                metadata.mode() & 0o7777,
                metadata.mtime(),
                metadata.mtime_nsec(),
                entry.mode,
                entry.mtime_seconds,
                entry.mtime_nanoseconds
            )
            .into());
        }
        let (content, class) = match &entry.kind {
            EntryKind::Directory => {
                if !metadata.is_dir() {
                    return Err(format!("native directory type: {relative}").into());
                }
                continue;
            }
            EntryKind::Symlink(target) => {
                if !metadata.file_type().is_symlink()
                    || fs::read_link(&path)? != Path::new(target)
                    || metadata.len() != target.len() as u64
                {
                    return Err(format!("native symlink mismatch: {relative}").into());
                }
                continue;
            }
            EntryKind::File(content) => (content, relative.as_str()),
            EntryKind::Hardlink(target) => {
                let EntryKind::File(content) = &expected[target].kind else {
                    return Err("hard-link content missing".into());
                };
                (content, target.as_str())
            }
        };
        if !metadata.is_file() || metadata.len() != content.len() {
            return Err(format!("native file type/length: {relative}").into());
        }
        let inode = (metadata.dev(), metadata.ino());
        if metadata.nlink() != reference_counts[class] {
            return Err(format!("native hard-link count mismatch: {relative}").into());
        }
        if inode_classes
            .insert(inode, class)
            .is_some_and(|previous| previous != class)
            || class_inodes
                .insert(class, inode)
                .is_some_and(|previous| previous != inode)
        {
            return Err(format!("native hard-link class mismatch: {relative}").into());
        }
        let mut file = File::open(&path)?;
        let mut offset = 0;
        let mut custody_hash = matches!(content, Content::Digest { .. }).then(Sha256::new);
        custody_paths += usize::from(custody_hash.is_some());
        while offset < content.len() {
            let count = if custody_hash.is_some() {
                bytes.len().min((content.len() - offset) as usize)
            } else {
                content.read_at(offset, &mut wanted)?
            };
            file.read_exact(&mut bytes[..count])?;
            if let Some(hash) = &mut custody_hash {
                hash.update(&bytes[..count]);
            } else if bytes[..count] != wanted[..count] {
                return Err(format!("native content mismatch: {relative} at {offset}").into());
            }
            offset += count as u64;
        }
        if file.read(&mut bytes[..1])? != 0 {
            return Err("native file grew during verification".into());
        }
        if let (Some(hash), Content::Digest { sha256, .. }) = (custody_hash, content) {
            if hex(&hash.finish()) != *sha256 {
                return Err(format!("native persistence digest mismatch: {relative}").into());
            }
        }
        files += 1;
    }
    Ok((files, custody_paths))
}

#[derive(Clone, Debug, Default)]
pub(crate) struct FastDelta {
    pub changed_paths: BTreeSet<String>,
    pub absent_paths: BTreeSet<String>,
    pub witness_paths: BTreeSet<String>,
}

/// Structural equality is deliberately conservative: equal recipes imply equal bytes.
/// A different representation is treated as changed; persistence digests are never a source oracle.
pub(crate) fn content_recipe_equal(left: &Content, right: &Content) -> bool {
    match (left, right) {
        (Content::Seed { seed: a, len: x }, Content::Seed { seed: b, len: y }) => a == b && x == y,
        (Content::Zero { len: a }, Content::Zero { len: b }) => a == b,
        (Content::Literal(a), Content::Literal(b)) => a == b,
        (Content::Slice { source: a, offset: x, len: n }, Content::Slice { source: b, offset: y, len: m }) => x == y && n == m && content_recipe_equal(a, b),
        (Content::Concat(a), Content::Concat(b)) => a.len() == b.len() && a.iter().zip(b).all(|(x,y)| content_recipe_equal(x,y)),
        (Content::Xor { source: a, offset: x, len: n, mask: q }, Content::Xor { source: b, offset: y, len: m, mask: r }) => x == y && n == m && q == r && content_recipe_equal(a,b),
        _ => false,
    }
}

fn hash_content_recipe(hash: &mut Sha256, content: &Content) -> Result<()> {
    match content {
        Content::Seed { seed, len } => { frame(hash,b"seed"); hash.update(&seed.to_le_bytes()); hash.update(&len.to_le_bytes()); },
        Content::Zero { len } => { frame(hash,b"zero"); hash.update(&len.to_le_bytes()); },
        Content::Literal(bytes) => { frame(hash,b"literal"); hash.update(&(bytes.len() as u64).to_le_bytes()); let mut value=Sha256::new(); value.update(bytes); hash.update(&value.finish()); },
        Content::Slice { source, offset, len } => { frame(hash,b"slice"); hash.update(&offset.to_le_bytes()); hash.update(&len.to_le_bytes()); hash_content_recipe(hash,source)?; },
        Content::Concat(parts) => { frame(hash,b"concat"); hash.update(&(parts.len() as u64).to_le_bytes()); for part in parts { hash_content_recipe(hash,part)?; } },
        Content::Xor { source, offset, len, mask } => { frame(hash,b"xor"); hash.update(&offset.to_le_bytes()); hash.update(&len.to_le_bytes()); hash.update(&[*mask]); hash_content_recipe(hash,source)?; },
        Content::Digest { .. } => return Err("persistence digest is not an independent content recipe".into()),
    }
    Ok(())
}

pub(crate) fn content_recipe_identity(content: &Content) -> Result<String> {
    content.validate()?;
    let mut hash=Sha256::new(); frame(&mut hash,b"workspace-content-recipe-v1"); hash_content_recipe(&mut hash,content)?;
    Ok(hex(&hash.finish()))
}

pub(crate) fn recipe_identity(entries: &[Entry]) -> Result<String> {
    validate_entries(entries)?;
    let mut sorted=entries.iter().collect::<Vec<_>>(); sorted.sort_by(|a,b| a.path.cmp(&b.path));
    let mut hash=Sha256::new(); frame(&mut hash,b"workspace-entry-recipes-v1"); hash.update(&(sorted.len() as u64).to_le_bytes());
    for entry in sorted {
        frame(&mut hash,entry.path.as_bytes()); hash.update(&entry.mode.to_le_bytes()); hash.update(&entry.mtime_seconds.to_le_bytes()); hash.update(&entry.mtime_nanoseconds.to_le_bytes());
        match &entry.kind {
            EntryKind::Directory => frame(&mut hash,b"directory"),
            EntryKind::Symlink(target) => { frame(&mut hash,b"symlink"); frame(&mut hash,target.as_bytes()); },
            EntryKind::Hardlink(target) => { frame(&mut hash,b"hardlink"); frame(&mut hash,target.as_bytes()); },
            EntryKind::File(content) => { frame(&mut hash,b"file"); hash_content_recipe(&mut hash,content)?; },
        }
    }
    Ok(hex(&hash.finish()))
}

fn add_fast_ancestors(paths: &mut BTreeSet<String>, path: &str) {
    paths.insert(".".into());
    for (index, _) in path.match_indices('/') { paths.insert(path[..index].to_owned()); }
}

pub(crate) fn fast_delta_from_entries(before: &[Entry], after: &[Entry], seed: u8, domain: &str) -> Result<FastDelta> {
    seed_label(seed)?; validate_entries(before)?; validate_entries(after)?;
    let old=before.iter().map(|entry|(entry.path.as_str(),entry)).collect::<BTreeMap<_,_>>();
    let new=after.iter().map(|entry|(entry.path.as_str(),entry)).collect::<BTreeMap<_,_>>();
    let mut delta=FastDelta::default();
    for entry in after {
        let equal=old.get(entry.path.as_str()).is_some_and(|previous| {
            let metadata=(entry.mode,entry.mtime_seconds,entry.mtime_nanoseconds)==(previous.mode,previous.mtime_seconds,previous.mtime_nanoseconds);
            metadata && match (&previous.kind,&entry.kind) {
                (EntryKind::Directory,EntryKind::Directory)=>true,
                (EntryKind::Symlink(a),EntryKind::Symlink(b))|(EntryKind::Hardlink(a),EntryKind::Hardlink(b))=>a==b,
                (EntryKind::File(a),EntryKind::File(b))=>content_recipe_equal(a,b),
                _=>false,
            }
        });
        if !equal { delta.changed_paths.insert(entry.path.clone()); add_fast_ancestors(&mut delta.changed_paths,&entry.path); }
    }
    for entry in before {
        if !new.contains_key(entry.path.as_str()) {
            delta.absent_paths.insert(entry.path.clone());
            let mut ancestors=BTreeSet::new(); add_fast_ancestors(&mut ancestors,&entry.path);
            delta.changed_paths.extend(ancestors.into_iter().filter(|path|new.contains_key(path.as_str())));
        }
    }
    // Alias deletion changes the surviving inode's link count even when its bytes are unchanged.
    for entry in before.iter().chain(after) {
        if let EntryKind::Hardlink(target)=&entry.kind {
            if delta.changed_paths.contains(&entry.path)||delta.absent_paths.contains(&entry.path)||delta.changed_paths.contains(target)||delta.absent_paths.contains(target) {
                if new.contains_key(target.as_str()) { delta.changed_paths.insert(target.clone()); }
                if new.contains_key(entry.path.as_str()) { delta.changed_paths.insert(entry.path.clone()); }
            }
        }
    }
    let alias_targets=delta.changed_paths.clone();
    for entry in after { if let EntryKind::Hardlink(target)=&entry.kind { if alias_targets.contains(target) { delta.changed_paths.insert(entry.path.clone()); } } }
    for path in delta.changed_paths.clone() { add_fast_ancestors(&mut delta.changed_paths,&path); }
    let mut classes=BTreeMap::<(String,u64,usize),Vec<&Entry>>::new();
    for entry in after {
        let EntryKind::File(content)=&entry.kind else {continue};
        if delta.changed_paths.contains(&entry.path) {continue;}
        classes.entry((entry.path.split('/').next().unwrap_or(".").to_owned(),content.len(),entry.path.bytes().filter(|b|*b==b'/').count())).or_default().push(entry);
    }
    for ((namespace,len,depth),mut candidates) in classes {
        candidates.sort_by(|a,b|a.path.cmp(&b.path));
        let selected=frame_seed(&["native-fast-witness-v2",domain,&namespace],&[seed as u64,len,depth as u64]) as usize%candidates.len();
        for index in [0,candidates.len()/2,candidates.len()-1,selected] { delta.witness_paths.insert(candidates[index].path.clone()); }
    }
    for path in delta.witness_paths.clone() {add_fast_ancestors(&mut delta.witness_paths,&path);}
    Ok(delta)
}

pub(crate) fn fast_selected_paths(entries: &[Entry], delta: &FastDelta) -> Result<BTreeSet<String>> {
    let expected = entries.iter().map(|entry| (entry.path.as_str(), entry)).collect::<BTreeMap<_, _>>();
    let mut selected = delta.changed_paths.union(&delta.witness_paths).cloned().collect::<BTreeSet<_>>();
    selected.insert(".".into());
    for path in selected.clone() {
        if let Some(Entry { kind: EntryKind::Hardlink(target), .. }) = expected.get(path.as_str()).copied() {
            selected.insert(target.clone());
        }
    }
    for entry in entries {
        if let EntryKind::Hardlink(target) = &entry.kind {
            if selected.contains(target) { selected.insert(entry.path.clone()); }
        }
    }
    for path in selected.clone() {
        validate_path(&path)?;
        for (index, _) in path.match_indices('/') { selected.insert(path[..index].to_owned()); }
    }
    if selected.iter().any(|path| !expected.contains_key(path.as_str())) { return Err("fast verifier selected an absent path".into()); }
    if delta.absent_paths.iter().any(|path| expected.contains_key(path.as_str())) { return Err("fast absence declaration exists in oracle".into()); }
    Ok(selected)
}

/// Full namespace/type census, then independently selected body/metadata checks.
/// An authenticated base certificate is required from the host; this is not full verification.
pub(crate) fn verify_native_fast(root: &Path, entries: &[Entry], delta: &FastDelta, certificate_binding: &str) -> Result<Receipt> {
    verify_native_fast_impl(root,entries,delta,certificate_binding,None)
}

pub(crate) fn verify_native_fast_with_coverage(root: &Path, entries: &[Entry], delta: &FastDelta, certificate_binding: &str, certified_paths: &BTreeSet<String>) -> Result<Receipt> {
    verify_native_fast_impl(root,entries,delta,certificate_binding,Some(certified_paths))
}

fn verify_native_fast_impl(root: &Path, entries: &[Entry], delta: &FastDelta, certificate_binding: &str, certified_paths: Option<&BTreeSet<String>>) -> Result<Receipt> {
    if certificate_binding.len() != 64 || !certificate_binding.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)) {
        return Err("fast verification requires the host's qualified certificate binding".into());
    }
    let logical = validate_entries(entries)?;
    let expected = entries.iter().map(|entry| (entry.path.clone(), entry)).collect::<BTreeMap<_, _>>();
    let mut required=delta.clone();
    if let Some(certified)=certified_paths {
        if certified.iter().any(|path| !expected.get(path).is_some_and(|entry|matches!(&entry.kind,EntryKind::File(_)|EntryKind::Hardlink(_)))) {return Err("certificate covers an unknown/nonregular current path".into());}
        for entry in entries {if matches!(&entry.kind,EntryKind::File(_)|EntryKind::Hardlink(_))&&!certified.contains(&entry.path) {required.changed_paths.insert(entry.path.clone());}}
    }
    let selected = fast_selected_paths(entries, &required)?;
    let mut found = BTreeMap::from([(".".to_owned(), fs::symlink_metadata(root)?.file_type())]);
    let mut pending = vec![root.to_owned()];
    while let Some(directory) = pending.pop() {
        for item in fs::read_dir(directory)? {
            let item = item?;
            let path = item.path();
            let relative = path.strip_prefix(root)?.to_str().ok_or("non-UTF8 fixture path")?.to_owned();
            validate_path(&relative)?;
            let kind = item.file_type()?;
            if kind.is_dir() { pending.push(path); }
            if found.insert(relative, kind).is_some() { return Err("duplicate fast native path".into()); }
        }
    }
    if found.keys().ne(expected.keys()) { return Err("complete fast native path-set mismatch".into()); }
    for (path, kind) in &found {
        let matches = match &expected[path].kind {
            EntryKind::Directory => kind.is_dir(),
            EntryKind::File(_) | EntryKind::Hardlink(_) => kind.is_file(),
            EntryKind::Symlink(_) => kind.is_symlink(),
        };
        if !matches { return Err(format!("fast native namespace type mismatch: {path}").into()); }
    }
    for path in &delta.absent_paths {
        validate_path(path)?;
        match fs::symlink_metadata(root.join(path)) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
            Ok(_) => return Err(format!("fast native required absence: {path}").into()),
        }
    }
    let mut observations = BTreeMap::new();
    let mut selected_entries = Vec::new();
    for path in &selected {
        observations.insert(path.clone(), fs::symlink_metadata(root.join(path))?);
        selected_entries.push((*expected[path]).clone());
        if matches!(&expected[path].kind, EntryKind::File(Content::Digest { .. })) { return Err("fast expected bytes must be independently generated".into()); }
    }
    let (files, custody) = verify_native_observed(root, &expected, &observations)?;
    if custody != 0 { return Err("fast verification cannot downgrade to persistence custody".into()); }
    let regular_paths = entries.iter().filter(|entry| matches!(&entry.kind, EntryKind::File(_) | EntryKind::Hardlink(_))).count();
    let checked_bytes = selected_entries.iter().map(|entry| match &entry.kind {
        EntryKind::File(content) => content.len(),
        EntryKind::Hardlink(target) => match &expected[target].kind { EntryKind::File(content) => content.len(), _ => 0 },
        _ => 0,
    }).sum::<u64>();
    Ok(BTreeMap::from([
        ("verification_status".into(), "fast_iteration_verified".into()),
        ("fully_verified".into(), "false".into()),
        ("certificate_binding".into(), certificate_binding.into()),
        ("fast_witness_profile".into(), "native-fast-witness-v2:first-middle-last-seeded-per-namespace-length-depth-class".into()),
        ("native_namespace_paths_verified".into(), entries.len().to_string()),
        ("native_namespace_types_verified".into(), entries.len().to_string()),
        ("native_namespace_type_source".into(), "DirEntry::file_type;platform-metadata-fallback-permitted;skipped-counts-describe-unperformed-checks-not-syscalls".into()),
        ("changed_paths_declared".into(), delta.changed_paths.len().to_string()),
        ("absent_paths_verified".into(), delta.absent_paths.len().to_string()),
        ("witness_paths_declared".into(), delta.witness_paths.len().to_string()),
        ("selected_metadata_paths_verified".into(), selected.len().to_string()),
        ("selected_regular_paths_verified".into(), files.to_string()),
        ("selected_regular_bytes_verified".into(), checked_bytes.to_string()),
        ("skipped_untouched_regular_bodies".into(), (regular_paths - files).to_string()),
        ("skipped_untouched_metadata_paths".into(), (entries.len() - selected.len()).to_string()),
        ("logical_bytes".into(), logical.to_string()),
        ("selected_independent_oracle_identity".into(), if certified_paths.is_some() { recipe_identity(&selected_entries)? } else { sdk_edit_common::sha256_hex(manifest(&selected_entries)?.as_bytes()) }),
        ("oracle_identity_encoding".into(), if certified_paths.is_some() {"structural-entry-recipes-v1"} else {"independent-manifest-v1"}.into()),
        ("uncertified_regular_paths_checked".into(), certified_paths.map_or(0,|paths|entries.iter().filter(|entry|matches!(&entry.kind,EntryKind::File(_)|EntryKind::Hardlink(_))&&!paths.contains(&entry.path)).count()).to_string()),
        ("oracle_scope".into(), "independent-delta-and-witnesses;complete-native-namespace;certified-unchanged-content-roots-checked-by-host".into()),
    ]))
}

fn generic_fast_delta_qualification() -> Result<Receipt> {
    let before=vec![Entry::directory("."),Entry::directory("a"),Entry::file("a/data",Content::Seed{seed:1,len:8}),Entry::hardlink("a/alias","a/data"),Entry::hardlink("a/alias2","a/data"),Entry::file("untouched",Content::Zero{len:4})];
    let mut after=before.clone(); after.retain(|entry|entry.path!="a/alias");
    let data=after.iter_mut().find(|entry|entry.path=="a/data").ok_or("generic qualification data")?;
    data.kind=EntryKind::File(Content::Seed{seed:2,len:8});
    after.iter_mut().find(|entry|entry.path=="a").ok_or("generic qualification directory")?.mode=0o700;
    after.push(Entry::file("created",Content::Literal(vec![1,2])));
    let delta=fast_delta_from_entries(&before,&after,1,"generic-qualification")?;
    for path in [".","a","a/data","a/alias2","created"] {if !delta.changed_paths.contains(path) {return Err(format!("generic fast delta omitted affected path:{path}").into());}}
    if delta.absent_paths!=BTreeSet::from(["a/alias".to_owned()])||!delta.witness_paths.contains("untouched") {return Err("generic fast delta absence/witness mismatch".into());}
    if !content_recipe_equal(&Content::Seed{seed:1,len:8},&Content::Seed{seed:1,len:8})||content_recipe_equal(&Content::Seed{seed:1,len:8},&Content::Seed{seed:2,len:8})||content_recipe_equal(&Content::Zero{len:2},&Content::Literal(vec![0,0])) {return Err("generic conservative recipe equality mismatch".into());}
    if content_recipe_identity(&Content::Digest{len:2,sha256:"0".repeat(64)}).is_ok() {return Err("generic fast recipe accepted persistence digest".into());}
    let mut reversed=before.clone(); reversed.reverse();
    if recipe_identity(&before)?!=recipe_identity(&reversed)?||recipe_identity(&before)?==recipe_identity(&after)? {return Err("generic recipe identity ordering/change mismatch".into());}
    let stable=fast_delta_from_entries(&before,&before,1,"generic-qualification")?;
    if !stable.changed_paths.is_empty()||!stable.absent_paths.is_empty()||stable.witness_paths.is_empty() {return Err("generic unchanged recipe delta mismatch".into());}
    Ok(Receipt::from([("fast_generic_delta_qualification".into(),"pass".into()),("fast_generic_qualified_checks".into(),"created-content,changed-content,metadata,ancestors,alias-closure,deletion,witnesses,conservative-recipe-equality,digest-rejection,order-stable-identity".into())]))
}

/// One small aggregate qualification, called only by the explicit host command.
pub(crate) fn fast_qualification(root: &Path) -> Result<Receipt> {
    let entries = vec![Entry::directory("."), Entry::file("changed.dat", Content::Literal(vec![1,2,3,4])),
        Entry::hardlink("alias.dat", "changed.dat"), Entry::file("witness.dat", Content::Literal(vec![5,6,7])),
        Entry::file("untouched.dat", Content::Literal(vec![8,9]))];
    let delta = FastDelta { changed_paths: BTreeSet::from(["changed.dat".into()]),
        absent_paths: BTreeSet::from(["gone.dat".into()]), witness_paths: BTreeSet::from(["witness.dat".into()]) };
    let binding = "a".repeat(64);
    let mut receipt = generic_fast_delta_qualification()?;
    for mutation in ["baseline", "wrong-bytes", "extra-path", "missing-path", "required-absence", "alias-mode", "alias-split", "witness-bytes", "missing-certificate"] {
        let directory = root.join(mutation);
        create_fixture(&directory, &entries)?;
        match mutation {
            "wrong-bytes" => { fs::write(directory.join("changed.dat"), [9,2,3,4])?; set_metadata(&directory.join("changed.dat"), &entries[1])?; },
            "extra-path" => fs::write(directory.join("extra.dat"), [0])?,
            "missing-path" => fs::remove_file(directory.join("witness.dat"))?,
            "required-absence" => fs::write(directory.join("gone.dat"), [0])?,
            "alias-mode" => fs::set_permissions(directory.join("alias.dat"), fs::Permissions::from_mode(0o600))?,
            "alias-split" => { fs::remove_file(directory.join("alias.dat"))?; fs::write(directory.join("alias.dat"), [1,2,3,4])?; set_metadata(&directory.join("alias.dat"), &entries[2])?; set_metadata(&directory, &entries[0])?; },
            "witness-bytes" => { fs::write(directory.join("witness.dat"), [9,6,7])?; set_metadata(&directory.join("witness.dat"), &entries[3])?; },
            _ => (),
        }
        let result = verify_native_fast(&directory, &entries, &delta, if mutation == "missing-certificate" { "" } else { &binding });
        if mutation == "baseline" {
            let observed = result?;
            if observed.get("verification_status").map(String::as_str) != Some("fast_iteration_verified")
                || observed.get("fully_verified").map(String::as_str) != Some("false")
                || observed.get("skipped_untouched_regular_bodies").map(String::as_str) != Some("1") {
                return Err("fast native scope receipt mislabeled".into());
            }
            verify_native(&directory, &entries)?;
            receipt.insert("fast_native_positive_scope".into(), "pass".into());
        } else {
            let error = result.err().ok_or("fast native accepted negative fixture")?.to_string();
            let expected = match mutation {
                "wrong-bytes" | "witness-bytes" => "native content mismatch",
                "alias-mode" => "native metadata mismatch",
                "alias-split" => "native hard-link count mismatch",
                "missing-certificate" => "qualified certificate binding",
                _ => "complete fast native path-set mismatch",
            };
            if !error.contains(expected) { return Err(format!("fast native rejected {mutation} for unexpected reason: {error}").into()); }
            receipt.insert(format!("fast_native_rejection_{mutation}"), error);
        }
    }
    let directory=root.join("uncertified-content"); create_fixture(&directory,&entries)?;
    fs::write(directory.join("untouched.dat"),[0,9])?; set_metadata(&directory.join("untouched.dat"),&entries[4])?;
    let covered=BTreeSet::from(["changed.dat".into(),"alias.dat".into(),"witness.dat".into()]);
    let error=verify_native_fast_with_coverage(&directory,&entries,&delta,&binding,&covered).err().ok_or("fast native skipped uncertified content")?.to_string();
    if !error.contains("native content mismatch: untouched.dat") {return Err(format!("uncertified content rejected for unexpected reason:{error}").into());}
    receipt.insert("fast_native_rejection_uncertified-content".into(),error);
    let error=verify_native_fast_with_coverage(&directory,&entries,&delta,&binding,&BTreeSet::from(["missing".into()])).err().ok_or("fast native accepted unknown certified path")?.to_string();
    if !error.contains("certificate covers an unknown/nonregular") {return Err(format!("certified membership rejected for unexpected reason:{error}").into());}
    receipt.insert("fast_native_rejection_unknown-certified-path".into(),error);
    receipt.insert("fast_native_qualification_status".into(), "pass".into());
    receipt.insert("fast_native_negative_cases".into(), "10".into());
    Ok(receipt)
}

pub(crate) fn verify_native(root: &Path, entries: &[Entry]) -> Result<Receipt> {
    let logical = validate_entries(entries)?;
    let expected = entries.iter().map(|entry| (entry.path.clone(), entry)).collect::<BTreeMap<_, _>>();
    let observations = native_metadata(root)?;
    if observations.keys().ne(expected.keys()) { return Err("complete native path-set mismatch".into()); }
    let (files, custody_paths) = verify_native_observed(root, &expected, &observations)?;
    Ok(BTreeMap::from([
        ("verification_status".into(), "pass".into()),
        ("verified_paths".into(), entries.len().to_string()),
        ("verified_regular_paths".into(), files.to_string()),
        ("logical_bytes".into(), logical.to_string()),
        (
            "oracle_identity".into(),
            sdk_edit_common::sha256_hex(manifest(entries)?.as_bytes()),
        ),
        (
            "persistence_custody_paths".into(),
            custody_paths.to_string(),
        ),
        (
            "independent_content_paths".into(),
            (files - custody_paths).to_string(),
        ),
        (
            "oracle_scope".into(),
            if custody_paths == 0 {
                "independent-source"
            } else {
                "independent-source-plus-precommit-persistence-custody"
            }
            .into(),
        ),
    ]))
}

pub(crate) fn self_check() -> Result<()> {
    let content = Content::Seed {
        seed: 73,
        len: 1031,
    };
    let mut sequential = Vec::new();
    content.write_to(&mut sequential)?;
    let mut state = 73;
    let mut direct = vec![0; 1031];
    sdk_edit_common::fixture_block(&mut state, &mut direct);
    if sequential != direct {
        return Err("fixture generator reuse".into());
    }
    for offset in 0..17 {
        let mut bytes = [0; 71];
        content.read_at(offset, &mut bytes)?;
        if bytes != sequential[offset as usize..offset as usize + 71] {
            return Err("unaligned content read".into());
        }
    }
    let modified = content
        .splice(9, 31, Content::Literal(vec![42; 7]))?
        .xor(6, 18, 7)?;
    let mut wanted = sequential.clone();
    wanted.splice(9..40, [42; 7]);
    for byte in &mut wanted[6..24] {
        *byte ^= 7;
    }
    let mut actual = Vec::new();
    modified.write_to(&mut actual)?;
    if actual != wanted || content.slice(1030, 2).is_ok() {
        return Err("content recipe bounds/oracle".into());
    }
    let small = ranked_indices(1, "self-check", 10)?;
    if small != ranked_indices(1, "self-check", 500)?[..10] {
        return Err("nested schedules".into());
    }
    let entries = shards(1, 1, "")?;
    if validate_entries(&entries)? != MIB
        || entries
            .iter()
            .filter(|entry| matches!(entry.kind, EntryKind::File(_)))
            .count()
            != 200
    {
        return Err("shard algebra".into());
    }
    Ok(())
}

pub(crate) fn native_qualification_entries() -> Vec<Entry> {
    let mut entries = vec![
        Entry::directory("."),
        Entry::directory("empty"),
        Entry::file(
            "payload",
            Content::Seed {
                seed: 71,
                len: 4099,
            },
        ),
        Entry::hardlink("alias", "payload"),
        Entry::symlink("link", "payload"),
    ];
    for entry in &mut entries {
        entry.mtime_nanoseconds = 123_456_789;
    }
    entries
}

/// Small, explicitly selected verifier qualification; never part of performance.
pub(crate) fn native_qualification(root: &Path) -> Result<Receipt> {
    let entries = native_qualification_entries();
    create_fixture(root, &entries)?;
    let mut receipt = verify_native(root, &entries)?;
    let reject = |description: &str| -> Result<String> {
        match verify_native(root, &entries) {
            Err(error) => Ok(format!("{description}: {error}")),
            Ok(_) => Err(format!("verifier accepted {description}").into()),
        }
    };
    let mut rejections = Vec::new();
    fs::write(root.join("extra"), b"unexpected")?;
    set_metadata(root, &entries[0])?;
    rejections.push(reject("extra path")?);
    fs::remove_file(root.join("extra"))?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .open(root.join("payload"))?;
    file.write_all(b"corrupt")?;
    set_metadata(&root.join("payload"), &entries[2])?;
    set_metadata(root, &entries[0])?;
    rejections.push(reject("changed bytes")?);
    if let EntryKind::File(content) = &entries[2].kind {
        content.write_to(&mut File::create(root.join("payload"))?)?;
    }
    set_metadata(&root.join("payload"), &entries[2])?;
    fs::remove_file(root.join("alias"))?;
    fs::copy(root.join("payload"), root.join("alias"))?;
    set_metadata(&root.join("alias"), &entries[3])?;
    set_metadata(root, &entries[0])?;
    rejections.push(reject("broken hard-link class")?);
    fs::remove_file(root.join("alias"))?;
    fs::hard_link(root.join("payload"), root.join("alias"))?;
    fs::remove_file(root.join("link"))?;
    std::os::unix::fs::symlink("empty", root.join("link"))?;
    set_metadata(&root.join("link"), &entries[4])?;
    set_metadata(root, &entries[0])?;
    rejections.push(reject("changed symlink target")?);
    fs::remove_file(root.join("link"))?;
    std::os::unix::fs::symlink("payload", root.join("link"))?;
    set_metadata(&root.join("link"), &entries[4])?;
    set_mtime_nofollow(&root.join("empty"), MTIME, 123_456_788)?;
    set_metadata(root, &entries[0])?;
    rejections.push(reject("nanosecond timestamp mismatch")?);
    set_metadata(&root.join("empty"), &entries[1])?;
    fs::set_permissions(root.join("payload"), fs::Permissions::from_mode(0o600))?;
    rejections.push(reject("mode mismatch")?);
    set_metadata(&root.join("payload"), &entries[2])?;
    verify_native(root, &entries)?;
    receipt.insert(
        "qualification_expected_rejections".into(),
        rejections.join(" | "),
    );
    receipt.insert("qualification_status".into(), "pass".into());
    Ok(receipt)
}

/// Product-free checks for the verification-only manifest decoder.
pub(crate) fn digest_self_check() -> Result<()> {
    let source = native_qualification_entries();
    let encoded = manifest(&source)?;
    let parsed = decode_manifest(&encoded)?;
    if manifest(&parsed)? != encoded {
        return Err("digest manifest round trip".into());
    }
    let file = parsed
        .iter()
        .find(|entry| entry.path == "payload")
        .ok_or("digest check file")?;
    let EntryKind::File(digest) = &file.kind else {
        return Err("digest check descriptor".into());
    };
    if digest.read_at(0, &mut [0; 1]).is_ok()
        || digest.write_to(&mut Vec::new()).is_ok()
        || digest.slice(0, 1).is_ok()
    {
        return Err("custody digest was accepted as a source generator".into());
    }
    let empty = Content::Digest {
        len: 0,
        sha256: sdk_edit_common::sha256_hex(&[]),
    };
    if empty.read_at(0, &mut []).is_ok() || empty.write_to(&mut Vec::new()).is_ok() {
        return Err("empty custody digest was accepted as generator".into());
    }
    for broken in [
        encoded.replace("workspace-independent-manifest-v1", "unknown-format"),
        encoded.replace("payload\tfile", "../payload\tfile"),
        encoded.replace("\t640\t", "\t0640\t"),
        encoded.replace("\t123456789\t", "\t1000000000\t"),
        encoded.replace("\t4099\t", "\t524288001\t"),
        format!(
            "{encoded}{}\n",
            encoded.lines().last().ok_or("manifest line")?
        ),
    ] {
        if decode_manifest(&broken).is_ok() {
            return Err("invalid custody manifest accepted".into());
        }
    }
    if (Content::Digest {
        len: 1,
        sha256: "invalid".into(),
    })
    .validate()
    .is_ok()
    {
        return Err("invalid custody SHA accepted".into());
    }
    Ok(())
}
