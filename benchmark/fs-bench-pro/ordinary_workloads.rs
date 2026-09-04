use super::workspace_common::{self as common, Case, Content, Entry, EntryKind, Receipt, SdkEdit};
use super::{Result, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::os::unix::fs::{FileExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Instant;

const MIB: u64 = 1_048_576;
const FLAT_SEED: u64 = 0x4c41_5945_5246_5331;
const TINY_LENGTHS: [u64; 10] = [0, 1, 7, 31, 127, 511, 1024, 2500, 4096, 8192];
const DEPTHS: [usize; 10] = [1, 4, 2, 8, 3, 10, 5, 7, 6, 9];
const GIT_KINDS: [&str; 10] = [
    "modify", "add", "delete", "modify", "add", "delete", "add", "modify", "delete", "modify",
];

fn seed_label(seed: u8) -> Result<String> {
    if !(1..=3).contains(&seed) {
        return Err("ordinary seed outside 1..3".into());
    }
    Ok(format!("layerfs-v0.1.3-seed-{seed}"))
}

pub(crate) fn content(seed: u8, domain: &str, path: &str, len: u64) -> Result<Content> {
    Ok(Content::Seed {
        seed: common::frame_seed(&[&seed_label(seed)?, domain, path], &[]),
        len,
    })
}

fn rank(seed: u8, domain: &str) -> Result<Vec<usize>> {
    common::ranked_indices(seed, domain, 500)
}

pub(crate) fn shard_path(s: usize, j: usize) -> String {
    if j < 64 {
        format!("wide/s{s:03}-f{j:03}.dat")
    } else if j < 199 {
        format!("regular/s{s:03}/f{j:03}.dat")
    } else {
        format!(
            "spine/{}/s{s:03}.dat",
            (1..=128)
                .map(|d| format!("d{d:03}"))
                .collect::<Vec<_>>()
                .join("/")
        )
    }
}

fn shard_content(seed: u8, s: usize, j: usize) -> Result<Content> {
    let len = if j < 128 {
        1024
    } else if j < 192 {
        8192
    } else {
        49152
    };
    Ok(Content::Seed {
        seed: common::frame_seed(
            &["workspace-shards-v1", &seed_label(seed)?],
            &[s as u64, j as u64],
        ),
        len,
    })
}

fn add(entries: &mut BTreeMap<String, Entry>, entry: Entry) {
    entries.insert(entry.path.clone(), entry);
}

fn dir(entries: &mut BTreeMap<String, Entry>, path: &str) {
    add(entries, Entry::directory(path.to_owned()));
}

fn parents(entries: &mut BTreeMap<String, Entry>, path: &str) {
    for (at, _) in path.match_indices('/') {
        dir(entries, &path[..at]);
    }
}

fn file(entries: &mut BTreeMap<String, Entry>, path: String, data: Content) {
    parents(entries, &path);
    add(entries, Entry::file(path, data));
}

fn merge(entries: &mut BTreeMap<String, Entry>, rows: Vec<Entry>) {
    for entry in rows {
        add(entries, entry);
    }
}

fn tiny_targets(seed: u8) -> Result<Vec<(String, Content)>> {
    rank(seed, "tiny-file-churn")?
        .into_iter()
        .enumerate()
        .map(|(k, i)| {
            let path = format!("tiny/p{}/f{i:03}.dat", k % 10);
            let data = content(seed, "tiny-file-churn", &path, TINY_LENGTHS[k % 10])?;
            Ok((path, data))
        })
        .collect()
}

pub(crate) fn git_targets(seed: u8) -> Result<Vec<(&'static str, String, Content)>> {
    rank(seed, "git-tool-workflow")?
        .into_iter()
        .enumerate()
        .map(|(k, i)| {
            let kind = GIT_KINDS[k % 10];
            let path = if kind == "add" {
                format!("added/add-{i:03}.dat")
            } else {
                format!("tracked/{kind}-{i:03}.dat")
            };
            let data = content(seed, "git-tool-workflow", &path, 2500)?;
            Ok((kind, path, data))
        })
        .collect()
}

pub(crate) fn fixture(case: &Case, seed: u8) -> Result<Vec<Entry>> {
    seed_label(seed)?;
    let mut entries = BTreeMap::new();
    dir(&mut entries, ".");
    match case.kind {
        "payload-create" => (),
        "payload-random-read" => file(
            &mut entries,
            "payload.bin".into(),
            Content::Seed {
                seed: FLAT_SEED,
                len: 500 * MIB,
            },
        ),
        "tiny-create" | "tiny-stat" | "tiny-unlink" => {
            merge(&mut entries, common::shards(seed, 500, "")?);
            dir(&mut entries, "tiny");
            for p in 0..10 {
                dir(&mut entries, &format!("tiny/p{p}"));
            }
            if case.kind != "tiny-create" {
                for (p, c) in tiny_targets(seed)? {
                    file(&mut entries, p, c);
                }
            }
        }
        "tiny-bulk-create" | "tiny-bulk-delete" => {
            merge(&mut entries, common::shards(seed, 1, "witness")?);
            if case.kind == "tiny-bulk-delete" {
                merge(&mut entries, common::shards(seed, case.tier, "bulk")?);
            }
        }
        "directory-construct" => {
            merge(&mut entries, common::shards(seed, 500, "")?);
            dir(&mut entries, "new-directories");
        }
        "directory-metadata-scan"
        | "directory-content-scan"
        | "workspace-clean-commit"
        | "workspace-fixed-move" => merge(&mut entries, common::shards(seed, case.tier, "")?),
        "workspace-distributed-sdk-edit" | "workspace-dense-rewrite" => {
            merge(&mut entries, common::shards(seed, 500, "")?)
        }
        "git-tool" => {
            merge(&mut entries, common::shards(seed, 32, "background")?);
            dir(&mut entries, "tracked");
            dir(&mut entries, "added");
            for (kind, p, c) in git_targets(seed)? {
                if kind != "add" {
                    file(&mut entries, p, c);
                }
            }
        }
        "namespace-subtree-relocate-delete" => {
            for i in 0..100_000 {
                let path = format!("background/d{:03}/f{i:06}.dat", i / 1000);
                let data = content(seed, "namespace-mutation", &path, 2500)?;
                file(&mut entries, path, data);
            }
            dir(&mut entries, "destination");
            for tree in ["a", "b"] {
                for s in 0..case.tier {
                    for j in 0..200 {
                        let path = format!("source/tree-{tree}/s{s:03}/f{j:03}.dat");
                        let data = content(seed, "namespace-mutation", &path, 1024)?;
                        file(&mut entries, path, data);
                    }
                }
            }
        }
        "agent-episodes" => {
            merge(&mut entries, common::shards(seed, 64, "background")?);
            dir(&mut entries, "cells");
            dir(&mut entries, "finished");
            for i in 0..500 {
                for name in ["source.bin", "edit.bin", "replacement.bin"] {
                    let path = format!("cells/e{i:03}/{name}");
                    let data = content(seed, "agent-episodes", &path, 8192)?;
                    file(&mut entries, path, data);
                }
                add(
                    &mut entries,
                    Entry::hardlink(
                        format!("cells/e{i:03}/alias.bin"),
                        format!("cells/e{i:03}/edit.bin"),
                    ),
                );
            }
        }
        _ => return Err(format!("unknown ordinary case kind {}", case.kind).into()),
    }
    Ok(entries.into_values().collect())
}

fn remove_tree(entries: &mut BTreeMap<String, Entry>, root: &str) {
    let prefix = format!("{root}/");
    entries.retain(|p, _| p != root && !p.starts_with(&prefix));
}

fn move_tree(entries: &mut BTreeMap<String, Entry>, old: &str, new: &str) {
    let prefix = format!("{old}/");
    let paths: Vec<_> = entries
        .keys()
        .filter(|p| p.as_str() == old || p.starts_with(&prefix))
        .cloned()
        .collect();
    for p in paths {
        let mut entry = entries.remove(&p).expect("selected entry");
        entry.path = format!("{new}{}", &p[old.len()..]);
        if let EntryKind::Hardlink(target) = &mut entry.kind {
            if target == old || target.starts_with(&prefix) {
                *target = format!("{new}{}", &target[old.len()..]);
            }
        }
        add(entries, entry);
    }
}

fn read_content(c: &Content) -> Result<Vec<u8>> {
    let mut bytes = vec![0; usize::try_from(c.len())?];
    let mut at = 0;
    while at < bytes.len() {
        let n = c.read_at(at as u64, &mut bytes[at..])?;
        if n == 0 {
            return Err("content descriptor early EOF".into());
        }
        at += n;
    }
    Ok(bytes)
}

// Oracle only: derive all retained episode bytes from the immutable input recipe.
fn episode_expected(seed: u8, i: usize) -> Result<(Content, Content, Content)> {
    let source = read_content(&content(
        seed,
        "agent-episodes",
        &format!("cells/e{i:03}/source.bin"),
        8192,
    )?)?;
    let mut edited = read_content(&content(
        seed,
        "agent-episodes",
        &format!("cells/e{i:03}/edit.bin"),
        8192,
    )?)?;
    for t in 0..4096 {
        edited[2048 + t] = source[t] ^ 0x5a;
    }
    let replacement: Vec<_> = source
        .iter()
        .zip(&edited)
        .map(|(s, c)| s ^ c ^ 0x3c)
        .collect();
    let output: Vec<_> = (0..8192)
        .map(|t| {
            source[t]
                ^ edited[t]
                ^ (source[t % 16] ^ 0xa5)
                ^ edited[t]
                ^ replacement[t]
                ^ replacement[t]
                ^ source[t % 4096]
        })
        .collect();
    Ok((
        Content::Literal(edited),
        Content::Literal(replacement),
        Content::Literal(output),
    ))
}

pub(crate) fn sdk_edits(case: &Case, seed: u8) -> Result<Vec<SdkEdit>> {
    if case.kind != "workspace-distributed-sdk-edit" {
        return Err("case does not use SDK edits".into());
    }
    rank(seed, "workspace-distributed-sdk")?
        .into_iter()
        .take(case.tier)
        .map(|s| {
            let j = 128 + s % 64;
            let replacement = read_content(
                &shard_content(seed, s, j)?
                    .xor(0, 4096, 0x5a)?
                    .slice(0, 4096)?,
            )?;
            Ok(SdkEdit {
                path: shard_path(s, j),
                start: 0,
                delete_len: 4096,
                replacement,
            })
        })
        .collect()
}

pub(crate) fn expected(case: &Case, seed: u8, step: usize) -> Result<Vec<Entry>> {
    if step > 1 {
        return Err("ordinary expected step must be 0 (input) or 1 (final)".into());
    }
    let mut entries: BTreeMap<_, _> = fixture(case, seed)?
        .into_iter()
        .map(|e| (e.path.clone(), e))
        .collect();
    if step == 0 {
        return Ok(entries.into_values().collect());
    }
    match case.kind {
        "payload-create" => file(
            &mut entries,
            "payload.bin".into(),
            Content::Seed {
                seed: FLAT_SEED,
                len: case.tier as u64 * MIB,
            },
        ),
        "tiny-create" => {
            for (p, c) in tiny_targets(seed)?.into_iter().take(case.tier) {
                file(&mut entries, p, c);
            }
        }
        "tiny-unlink" => {
            for (p, _) in tiny_targets(seed)?.into_iter().take(case.tier) {
                entries.remove(&p);
            }
        }
        "tiny-bulk-create" => merge(&mut entries, common::shards(seed, case.tier, "bulk")?),
        "tiny-bulk-delete" => remove_tree(&mut entries, "bulk"),
        "directory-construct" => {
            for (k, i) in rank(seed, "directory-construction")?
                .into_iter()
                .take(case.tier)
                .enumerate()
            {
                let mut p = format!("new-directories/c{i:03}");
                dir(&mut entries, &p);
                for d in 1..DEPTHS[k % 10] {
                    p.push_str(&format!("/d{d:03}"));
                    dir(&mut entries, &p);
                }
            }
        }
        "git-tool" => {
            for (kind, p, c) in git_targets(seed)?.into_iter().take(case.tier) {
                match kind {
                    "add" => file(&mut entries, p, c),
                    "delete" => {
                        entries.remove(&p);
                    }
                    "modify" => {
                        let replaced = c.xor(1024, 10, 0x5a)?;
                        file(&mut entries, p, replaced);
                    }
                    _ => unreachable!(),
                }
            }
        }
        "namespace-subtree-relocate-delete" => {
            move_tree(&mut entries, "source/tree-a", "destination/moved-a");
            remove_tree(&mut entries, "source/tree-b");
        }
        "workspace-fixed-move" => {
            move_tree(&mut entries, "regular/s000/f064.dat", "dest/moved.dat")
        }
        "workspace-distributed-sdk-edit" => {
            for edit in sdk_edits(case, seed)? {
                let entry = entries
                    .get_mut(&edit.path)
                    .ok_or("SDK oracle target absent")?;
                let EntryKind::File(data) = &entry.kind else {
                    return Err("SDK oracle target is not file".into());
                };
                entry.kind = EntryKind::File(data.splice(
                    edit.start,
                    edit.delete_len,
                    Content::Literal(edit.replacement),
                )?);
            }
        }
        "workspace-dense-rewrite" => {
            for s in rank(seed, "workspace-dense-rewrite")?
                .into_iter()
                .take(case.tier)
            {
                for j in 0..200 {
                    let p = shard_path(s, j);
                    let len = shard_content(seed, s, j)?.len();
                    let data = content(seed, "workspace-dense-rewrite", &p, len)?;
                    file(&mut entries, p, data);
                }
            }
        }
        "agent-episodes" => {
            for i in rank(seed, "agent-episodes")?.into_iter().take(case.tier) {
                let old = format!("cells/e{i:03}");
                let new = format!("finished/e{i:03}");
                move_tree(&mut entries, &old, &new);
                let (edited, replacement, output) = episode_expected(seed, i)?;
                file(&mut entries, format!("{new}/edit.bin"), edited);
                file(&mut entries, format!("{new}/replacement.bin"), replacement);
                file(&mut entries, format!("{new}/output.bin"), output);
                add(
                    &mut entries,
                    Entry::symlink(
                        format!("{new}/replacement-link"),
                        "replacement.bin".to_owned(),
                    ),
                );
            }
        }
        "payload-random-read"
        | "tiny-stat"
        | "directory-metadata-scan"
        | "directory-content-scan"
        | "workspace-clean-commit" => (),
        _ => return Err("unknown ordinary expected kind".into()),
    }
    Ok(entries.into_values().collect())
}

pub(crate) fn random_offsets(seed: u8, count: usize) -> Result<Vec<u64>> {
    let seed = seed_label(seed)?;
    Ok((0..count)
        .map(|i| {
            let mut hash = Sha256::new();
            hash.update(seed.as_bytes());
            hash.update(&[0]);
            hash.update(b"payload-random-read");
            hash.update(&[0]);
            hash.update(&(i as u64).to_le_bytes());
            u64::from_le_bytes(hash.finish()[..8].try_into().expect("digest word"))
                % (500 * MIB - 4096 + 1)
        })
        .collect())
}

pub(crate) fn check_cases(rows: &[Case], expected: usize) -> Result<()> {
    if rows.len() != expected
        || rows.iter().map(|r| &r.id).collect::<BTreeSet<_>>().len() != expected
    {
        return Err("ordinary registry cardinality".into());
    }
    for row in rows {
        if ![1, 10, 100, 500].contains(&row.tier)
            || !row.id.ends_with(&row.tier.to_string())
                && row.id != format!("payload-create-{}m", row.tier)
        {
            return Err("ordinary ID/tier mismatch".into());
        }
    }
    for seed in 1..=3 {
        for domain in [
            "tiny-file-churn",
            "directory-construction",
            "git-tool-workflow",
            "workspace-distributed-sdk",
            "workspace-dense-rewrite",
            "agent-episodes",
        ] {
            let order = rank(seed, domain)?;
            if order.len() != 500
                || order.iter().copied().collect::<BTreeSet<_>>() != (0..500).collect()
            {
                return Err("ordinary schedule permutation".into());
            }
        }
        if random_offsets(seed, 500)?
            .iter()
            .any(|o| o + 4096 > 500 * MIB)
        {
            return Err("random read bound".into());
        }
        if let Some(case) = rows
            .iter()
            .find(|r| r.kind == "workspace-distributed-sdk-edit" && r.tier == 1)
        {
            let edits = sdk_edits(case, seed)?;
            if edits.len() != 1 || edits[0].replacement.len() != 4096 || edits[0].delete_len != 4096
            {
                return Err("SDK replacement must remain exactly 4 KiB".into());
            }
        }
    }
    if DEPTHS.iter().sum::<usize>() != 55
        || 250_000_000_u64 + 2 * 500 * 200 * 1024 != 454_800_000
        || 64 * MIB + 500 * 32768 + 500 * (8192 + 256) + 8192 + 4096 + 32 != 87_729_184
    {
        return Err("ordinary transient algebra".into());
    }
    Ok(())
}

#[derive(Default)]
struct Ops {
    counts: BTreeMap<String, u64>,
    changed: BTreeMap<String, Entry>,
    phases: BTreeMap<String, u128>,
    buffer: Vec<u8>,
    visited: BTreeSet<String>,
    traversal: Vec<String>,
}

impl Ops {
    fn count(&mut self, key: &str, value: u64) {
        *self.counts.entry(key.into()).or_default() += value;
    }
    fn call<T>(
        &mut self,
        name: &str,
        path: &str,
        mut operation: impl FnMut() -> std::io::Result<T>,
    ) -> Result<T> {
        loop {
            self.count("attempted_syscall_count", 1);
            self.count(&format!("workload_{name}_call_count"), 1);
            match operation() {
                Ok(value) => {
                    self.count("completed_syscall_count", 1);
                    return Ok(value);
                }
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {
                    self.count("interrupted_syscall_count", 1)
                }
                Err(error) => return Err(format!("{name} {path}: {error}").into()),
            }
        }
    }
    fn parent(&mut self, path: &str) {
        let parent = Path::new(path)
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        self.changed.insert(
            parent.to_string_lossy().into_owned(),
            Entry::directory(parent.to_string_lossy().into_owned()),
        );
    }
    fn touch_file(&mut self, path: &str) {
        self.changed
            .insert(path.into(), Entry::file(path, Content::Zero { len: 0 }));
    }
    fn mkdir(&mut self, path: &str) -> Result<()> {
        self.call("mkdir", path, || fs::create_dir(path))?;
        self.call("chmod", path, || {
            fs::set_permissions(path, fs::Permissions::from_mode(0o750))
        })?;
        self.changed.insert(path.into(), Entry::directory(path));
        self.parent(path);
        Ok(())
    }
    fn open(&mut self, path: &str, create: bool, write: bool) -> Result<File> {
        self.call("open", path, || {
            OpenOptions::new()
                .read(!write)
                .write(write)
                .create_new(create)
                .mode(0o640)
                .open(path)
        })
    }
    fn close(&mut self, file: File) -> Result<()> {
        use std::os::fd::IntoRawFd;
        unsafe extern "C" {
            fn close(fd: i32) -> i32;
        }
        self.count("workload_close_call_count", 1);
        self.count("attempted_syscall_count", 1);
        // close must not retry EINTR: Linux may already have released the fd.
        if unsafe { close(file.into_raw_fd()) } != 0 {
            return Err(format!("close: {}", std::io::Error::last_os_error()).into());
        }
        self.count("completed_syscall_count", 1);
        Ok(())
    }
    fn write_bytes(&mut self, file: &File, path: &str, offset: u64, bytes: &[u8]) -> Result<()> {
        let mut at = 0;
        while at < bytes.len() {
            let n = self.call("pwrite", path, || {
                file.write_at(&bytes[at..], offset + at as u64)
            })?;
            if n == 0 {
                return Err(format!("zero write {path}").into());
            }
            self.count("completed_write_bytes", n as u64);
            at += n;
        }
        Ok(())
    }
    fn write_content(
        &mut self,
        path: &str,
        data: &Content,
        create: bool,
        sync: bool,
    ) -> Result<()> {
        let file = self.open(path, create, true)?;
        if create {
            self.parent(path);
        }
        let mut buffer = std::mem::take(&mut self.buffer);
        if buffer.is_empty() {
            buffer.resize(MIB as usize, 0);
        }
        let mut at = 0;
        while at < data.len() {
            let n = data.read_at(at, &mut buffer)?;
            if n == 0 {
                return Err("generator early EOF".into());
            }
            self.write_bytes(&file, path, at, &buffer[..n])?;
            at += n as u64;
        }
        self.buffer = buffer;
        self.touch_file(path);
        if sync {
            self.normalize_path(path)?;
            let start = Instant::now();
            self.call("fsync", path, || file.sync_all())?;
            *self.phases.entry("file_sync_ns".into()).or_default() += start.elapsed().as_nanos();
        }
        self.close(file)?;
        self.count("completed_file_write_count", 1);
        Ok(())
    }
    fn read_range(&mut self, file: &File, path: &str, offset: u64, bytes: &mut [u8]) -> Result<()> {
        let mut at = 0;
        while at < bytes.len() {
            let n = self.call("pread", path, || {
                file.read_at(&mut bytes[at..], offset + at as u64)
            })?;
            if n == 0 {
                return Err(format!("unexpected EOF {path} at {}", offset + at as u64).into());
            }
            self.count("completed_read_bytes", n as u64);
            at += n;
        }
        Ok(())
    }
    fn read_small(&mut self, path: &str, len: usize) -> Result<Vec<u8>> {
        let file = self.open(path, false, false)?;
        let mut bytes = vec![0; len];
        self.read_range(&file, path, 0, &mut bytes)?;
        let mut eof = [0];
        if self.call("pread", path, || file.read_at(&mut eof, len as u64))? != 0 {
            return Err(format!("unexpected length {path}").into());
        }
        self.close(file)?;
        Ok(bytes)
    }
    fn rename(&mut self, old: &str, new: &str) -> Result<()> {
        self.call("rename", old, || fs::rename(old, new))?;
        move_tree(&mut self.changed, old, new);
        self.parent(old);
        self.parent(new);
        Ok(())
    }
    fn unlink(&mut self, path: &str, directory: bool) -> Result<()> {
        if directory {
            self.call("rmdir", path, || fs::remove_dir(path))?;
        } else {
            self.call("unlink", path, || fs::remove_file(path))?;
        }
        self.changed.remove(path);
        self.parent(path);
        Ok(())
    }
    fn children(&mut self, path: &str) -> Result<Vec<String>> {
        let read = self.call("opendir", path, || fs::read_dir(path))?;
        let mut children = Vec::new();
        for entry in read {
            let entry = entry.map_err(|e| format!("readdir {path}: {e}"))?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| "non-UTF8 workload name")?;
            children.push(if path == "." {
                name
            } else {
                format!("{path}/{name}")
            });
            self.count("directory_entry_count", 1);
        }
        self.count("workload_closedir_call_count", 1);
        children.sort();
        Ok(children)
    }
    fn delete_tree(&mut self, path: &str) -> Result<()> {
        let attr = self.call("lstat", path, || fs::symlink_metadata(path))?;
        if attr.is_dir() {
            for child in self.children(path)? {
                self.delete_tree(&child)?;
            }
            self.unlink(path, true)
        } else {
            self.unlink(path, false)
        }
    }
    fn normalize_path(&mut self, path: &str) -> Result<()> {
        if let Some(entry) = self.changed.remove(path) {
            common::set_metadata(Path::new(path), &entry)
                .map_err(|e| format!("normalize {path}: {e}"))?;
            self.count("metadata_normalization_count", 1);
        }
        Ok(())
    }
    fn finish(&mut self) -> Result<()> {
        let start = Instant::now();
        let mut paths: Vec<_> = self.changed.keys().cloned().collect();
        paths.sort_by(|a, b| {
            b.split('/')
                .count()
                .cmp(&a.split('/').count())
                .then(b.cmp(a))
        });
        for path in paths {
            self.normalize_path(&path)?;
        }
        self.phases.insert(
            "metadata_normalization_ns".into(),
            start.elapsed().as_nanos(),
        );
        let start = Instant::now();
        let file = self.call("open_directory", ".", || File::open("."))?;
        self.call("fsyncdir", ".", || file.sync_all())?;
        self.close(file)?;
        self.phases
            .insert("root_sync_ns".into(), start.elapsed().as_nanos());
        Ok(())
    }
    fn receipt(&self) -> Receipt {
        self.counts
            .iter()
            .map(|(k, v)| (k.clone(), v.to_string()))
            .chain(self.phases.iter().map(|(k, v)| (k.clone(), v.to_string())))
            .collect()
    }
}

fn create_entries(ops: &mut Ops, entries: Vec<Entry>, prefix: &str) -> Result<()> {
    for entry in entries {
        if entry.path == "." || !entry.path.starts_with(prefix) {
            continue;
        }
        match entry.kind {
            EntryKind::Directory => ops.mkdir(&entry.path)?,
            EntryKind::File(content) => ops.write_content(&entry.path, &content, true, false)?,
            _ => return Err("bulk profile unexpected link".into()),
        }
    }
    Ok(())
}

fn scan(
    ops: &mut Ops,
    path: &str,
    payload: bool,
    verify: bool,
    oracle: Option<&BTreeMap<String, Entry>>,
) -> Result<()> {
    let metadata = ops.call("lstat", path, || fs::symlink_metadata(path))?;
    if verify {
        if !ops.visited.insert(path.into()) {
            return Err(format!("duplicate traversal entry {path}").into());
        }
        ops.traversal.push(path.into());
        let entry = oracle
            .ok_or("scan oracle")?
            .get(path)
            .ok_or("scan unexpected path")?;
        let kind_ok = match &entry.kind {
            EntryKind::Directory => metadata.is_dir(),
            EntryKind::File(data) => metadata.is_file() && metadata.len() == data.len(),
            _ => false,
        };
        if !kind_ok
            || metadata.mode() & 0o7777 != entry.mode
            || metadata.mtime() != entry.mtime_seconds
            || metadata.mtime_nsec() != i64::from(entry.mtime_nanoseconds)
        {
            return Err(format!("scan metadata oracle {path}").into());
        }
    }
    ops.count("visited_path_count", 1);
    if metadata.is_dir() {
        for child in ops.children(path)? {
            scan(ops, &child, payload, verify, oracle)?;
        }
    } else if metadata.is_file() {
        ops.count("visited_file_count", 1);
        if payload {
            let file = ops.open(path, false, false)?;
            let mut buffer = std::mem::take(&mut ops.buffer);
            if buffer.is_empty() {
                buffer.resize(MIB as usize, 0);
            }
            let mut at = 0;
            loop {
                let n = ops.call("pread", path, || file.read_at(&mut buffer, at))?;
                if n == 0 {
                    break;
                }
                if verify {
                    let EntryKind::File(expected) = &oracle
                        .ok_or("scan oracle")?
                        .get(path)
                        .ok_or("scan extra path")?
                        .kind
                    else {
                        return Err("scan expected file".into());
                    };
                    let mut bytes = vec![0; n];
                    if expected.read_at(at, &mut bytes)? != n || bytes != buffer[..n] {
                        return Err(format!("scan byte oracle {path} at {at}").into());
                    }
                }
                ops.count("completed_read_bytes", n as u64);
                at += n as u64;
            }
            if at != metadata.len() {
                return Err(format!("scan length/EOF {path}").into());
            }
            ops.buffer = buffer;
            ops.close(file)?;
        }
    } else {
        return Err(format!("unexpected scan entry {path}").into());
    }
    Ok(())
}

fn require_bytes(verify: bool, observed: &[u8], expected: &[u8], label: &str) -> Result<()> {
    if verify && observed != expected {
        return Err(format!("intermediate oracle mismatch: {label}").into());
    }
    Ok(())
}

fn episode(ops: &mut Ops, seed: u8, i: usize, verify: bool) -> Result<()> {
    let old = format!("cells/e{i:03}");
    let new = format!("finished/e{i:03}");
    let source = ops.read_small(&format!("{old}/source.bin"), 8192)?;
    if verify {
        require_bytes(
            true,
            &source,
            &read_content(&content(
                seed,
                "agent-episodes",
                &format!("{old}/source.bin"),
                8192,
            )?)?,
            "episode source",
        )?;
    }
    let target = format!("{old}/edit.bin");
    let file = ops.open(&target, false, true)?;
    let replacement: Vec<_> = source[..4096].iter().map(|b| b ^ 0x5a).collect();
    ops.write_bytes(&file, &target, 2048, &replacement)?;
    ops.close(file)?;
    ops.touch_file(&target);
    let a = ops.read_small(&format!("{old}/alias.bin"), 8192)?;
    let expected = if verify {
        Some(episode_expected(seed, i)?)
    } else {
        None
    };
    if let Some((edited, _, _)) = &expected {
        require_bytes(true, &a, &read_content(edited)?, "alias edit")?;
    }
    let append: Vec<_> = source[..16].iter().map(|b| b ^ 0xa5).collect();
    let file = ops.open(&target, false, true)?;
    ops.write_bytes(&file, &target, 8192, &append)?;
    ops.close(file)?;
    let alias = format!("{old}/alias.bin");
    let file = ops.open(&alias, false, false)?;
    let mut b = vec![0; 16];
    ops.read_range(&file, &alias, 8192, &mut b)?;
    ops.close(file)?;
    require_bytes(verify, &b, &append, "alias append")?;
    if verify {
        for name in ["edit.bin", "alias.bin"] {
            if fs::metadata(format!("{old}/{name}"))?.len() != 8208 {
                return Err("alias append length".into());
            }
        }
    }
    let file = ops.open(&target, false, true)?;
    ops.call("ftruncate", &target, || file.set_len(8192))?;
    ops.close(file)?;
    ops.rename(&old, &new)?;
    let c = ops.read_small(&format!("{new}/edit.bin"), 8192)?;
    if let Some((edited, _, _)) = &expected {
        require_bytes(true, &c, &read_content(edited)?, "moved edit")?;
    }
    let temp = format!("{new}/.replacement.tmp");
    let replace: Vec<_> = (0..8192).map(|t| source[t] ^ c[t] ^ 0x3c).collect();
    ops.write_content(&temp, &Content::Literal(replace.clone()), true, true)?;
    if verify {
        require_bytes(
            true,
            &fs::read(&temp)?,
            &replace,
            "synced temporary replacement",
        )?;
    }
    ops.rename(&temp, &format!("{new}/replacement.bin"))?;
    ops.touch_file(&format!("{new}/replacement.bin"));
    let d = ops.read_small(&format!("{new}/replacement.bin"), 8192)?;
    if let Some((_, replaced, _)) = &expected {
        require_bytes(true, &d, &read_content(replaced)?, "permanent replacement")?;
    }
    let link = format!("{new}/replacement-link");
    ops.call("symlink", &link, || {
        std::os::unix::fs::symlink("replacement.bin", &link)
    })?;
    ops.changed
        .insert(link.clone(), Entry::symlink(&link, "replacement.bin"));
    ops.parent(&link);
    let e = ops.read_small(&link, 8192)?;
    require_bytes(verify, &e, &replace, "relative symlink read")?;
    let scratch = format!("{new}/scratch.bin");
    ops.write_content(
        &scratch,
        &Content::Literal(source[..4096].to_vec()),
        true,
        false,
    )?;
    let f = ops.read_small(&scratch, 4096)?;
    require_bytes(verify, &f, &source[..4096], "scratch before unlink")?;
    ops.unlink(&scratch, false)?;
    let output: Vec<_> = (0..8192)
        .map(|t| source[t] ^ a[t] ^ b[t % 16] ^ c[t] ^ d[t] ^ e[t] ^ f[t % 4096])
        .collect();
    if let Some((_, _, bytes)) = &expected {
        require_bytes(
            true,
            &output,
            &read_content(bytes)?,
            "retained dependent output",
        )?;
    }
    ops.write_content(
        &format!("{new}/output.bin"),
        &Content::Literal(output),
        true,
        false,
    )?;
    if verify {
        let edit = fs::metadata(format!("{new}/edit.bin"))?;
        let alias = fs::metadata(format!("{new}/alias.bin"))?;
        if (edit.dev(), edit.ino(), edit.nlink()) != (alias.dev(), alias.ino(), 2)
            || Path::new(&old).exists()
            || Path::new(&scratch).exists()
            || Path::new(&temp).exists()
        {
            return Err("episode intermediate topology/lifetime".into());
        }
    }
    ops.count("completed_episode_count", 1);
    Ok(())
}

const GIT_CONFIG: &[(&str, &str)] = &[
    ("core.autocrlf", "false"),
    ("core.filemode", "true"),
    ("core.symlinks", "true"),
    ("core.logAllRefUpdates", "false"),
    ("core.hooksPath", "/dev/null"),
    ("commit.gpgSign", "false"),
    ("tag.gpgSign", "false"),
    ("gc.auto", "0"),
    ("maintenance.auto", "false"),
    ("credential.helper", ""),
    ("status.showUntrackedFiles", "all"),
    ("diff.renames", "false"),
    ("index.version", "2"),
    ("core.untrackedCache", "false"),
    ("core.fsmonitor", "false"),
    ("protocol.allow", "never"),
];

fn git_command(root: &Path, genesis: bool) -> Result<Command> {
    use std::os::unix::process::CommandExt;
    let home = std::env::temp_dir().join(format!("layerfs-v013-git-home-{}", std::process::id()));
    if !home.exists() {
        fs::create_dir(&home)?;
    }
    let mut command = Command::new("git");
    command.current_dir(root);
    command
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("HOME", &home)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("LC_ALL", "C")
        .env("TZ", "UTC")
        .env("GIT_AUTHOR_NAME", "LayerFS Benchmark")
        .env("GIT_COMMITTER_NAME", "LayerFS Benchmark")
        .env("GIT_AUTHOR_EMAIL", "benchmark@layerfs.invalid")
        .env("GIT_COMMITTER_EMAIL", "benchmark@layerfs.invalid")
        .env(
            "GIT_AUTHOR_DATE",
            if genesis {
                "1700000000 +0000"
            } else {
                "1700000001 +0000"
            },
        )
        .env(
            "GIT_COMMITTER_DATE",
            if genesis {
                "1700000000 +0000"
            } else {
                "1700000001 +0000"
            },
        );
    for (key, value) in GIT_CONFIG {
        command.arg("-c").arg(format!("{key}={value}"));
    }
    unsafe {
        command.pre_exec(|| {
            unsafe extern "C" {
                fn umask(mask: u32) -> u32;
            }
            umask(0o027);
            Ok(())
        });
    }
    Ok(command)
}

fn git(root: &Path, genesis: bool, args: &[&str]) -> Result<Output> {
    let output = git_command(root, genesis)?.args(args).output()?;
    if !output.status.success() {
        return Err(format!("git {args:?}: {}", String::from_utf8_lossy(&output.stderr)).into());
    }
    if output.stdout.len() + output.stderr.len() > 64 * 1024 * 1024 {
        return Err("Git output exceeds evidence budget".into());
    }
    Ok(output)
}

fn git_text(root: &Path, args: &[&str]) -> Result<String> {
    Ok(String::from_utf8(git(root, false, args)?.stdout)?
        .trim()
        .to_owned())
}

fn git_repository_entries(root: &Path) -> Result<Vec<Entry>> {
    let mut entries = Vec::new();
    let mut bytes = 0_u64;
    for path in common::native_paths(&root.join(".git"))? {
        let relative = if path == "." {
            ".git".to_owned()
        } else {
            format!(".git/{path}")
        };
        let native = root.join(&relative);
        let attr = fs::symlink_metadata(&native)?;
        if attr.is_file() {
            bytes = bytes.checked_add(attr.len()).ok_or("Git byte overflow")?;
            if bytes > 256 * MIB {
                return Err("Git object/index bytes exceed repository cap".into());
            }
        }
        let mut entry = if attr.is_dir() {
            Entry::directory(&relative)
        } else if attr.is_file() {
            Entry::file(&relative, Content::Literal(fs::read(&native)?))
        } else {
            return Err("unexpected non-file Git repository object".into());
        };
        entry.mode = attr.mode() & 0o7777;
        entry.mtime_seconds = attr.mtime();
        entry.mtime_nanoseconds = attr.mtime_nsec().try_into()?;
        entries.push(entry);
    }
    Ok(entries)
}

pub(crate) fn prepared_git_entries(root: &Path, case: &Case, seed: u8) -> Result<Vec<Entry>> {
    if case.kind != "git-tool" {
        return Err("prepared Git entries require Git case".into());
    }
    let mut entries = fixture(case, seed)?;
    entries.extend(git_repository_entries(root)?);
    if common::validate_entries(&entries)? > 256 * MIB {
        return Err("prepared complete Git repository exceeds 256 MiB".into());
    }
    Ok(entries)
}

pub(crate) fn prepare_git(root: &Path, seed: u8) -> Result<Receipt> {
    seed_label(seed)?;
    if root.join(".git").exists() {
        return Err("Git preparation requires no existing .git".into());
    }
    let started = Instant::now();
    git(
        root,
        true,
        &[
            "init",
            "--initial-branch=main",
            "--object-format=sha1",
            "--template=",
        ],
    )?;
    git(root, true, &["add", "-A", "--"])?;
    git(
        root,
        true,
        &[
            "commit",
            "--no-gpg-sign",
            "--no-verify",
            "-m",
            "layerfs v0.1.3 tool genesis",
        ],
    )?;
    // Preparation only: all repository metadata is deterministic before import.
    for mut entry in git_repository_entries(root)?.into_iter().rev() {
        entry.mtime_seconds = 1700000000;
        entry.mtime_nanoseconds = 0;
        common::set_metadata(&root.join(&entry.path), &entry)?;
    }
    common::set_metadata(root, &Entry::directory("."))?;
    let mut receipt = Receipt::new();
    receipt.insert("git_version".into(), git_text(root, &["--version"])?);
    receipt.insert(
        "git_genesis_head".into(),
        git_text(root, &["rev-parse", "HEAD"])?,
    );
    receipt.insert(
        "git_effective_config".into(),
        GIT_CONFIG
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(";"),
    );
    receipt.insert(
        "git_prepare_ns".into(),
        started.elapsed().as_nanos().to_string(),
    );
    Ok(receipt)
}

fn parse_status(bytes: &[u8]) -> Result<BTreeMap<String, String>> {
    let mut entries = BTreeMap::new();
    for record in bytes.split(|b| *b == 0).filter(|r| !r.is_empty()) {
        if record.len() < 4 || record[2] != b' ' {
            return Err("unexpected porcelain record".into());
        }
        let status = String::from_utf8(record[..2].to_vec())?;
        let path = String::from_utf8(record[3..].to_vec())?;
        if entries.insert(path, status).is_some() {
            return Err("duplicate porcelain path".into());
        }
    }
    Ok(entries)
}

fn git_apply(
    ops: &mut Ops,
    case: &Case,
    targets: &[(&'static str, String, Content)],
    normalization: &[Entry],
    reference: &Path,
    verify: bool,
) -> Result<()> {
    let apply_start = Instant::now();
    let mut m = 0;
    let mut expected_status = BTreeMap::new();
    for (kind, path, original) in targets.iter().take(case.tier) {
        let phase = Instant::now();
        match *kind {
            "add" => {
                ops.write_content(path, original, true, false)?;
                expected_status.insert(path.clone(), "??".into());
            }
            "delete" => {
                ops.unlink(path, false)?;
                expected_status.insert(path.clone(), " D".into());
            }
            "modify" => {
                if m % 2 == 0 {
                    let mut bytes = ops.read_small(path, 2500)?;
                    for b in &mut bytes[1024..1034] {
                        *b ^= 0x5a;
                    }
                    let native = Path::new(path);
                    let temp = native
                        .with_file_name(format!(
                            ".{}.save-{m:03}.tmp",
                            native.file_name().ok_or("Git basename")?.to_string_lossy()
                        ))
                        .to_string_lossy()
                        .into_owned();
                    ops.write_content(&temp, &Content::Literal(bytes.clone()), true, true)?;
                    if verify {
                        let expected = original.xor(1024, 10, 0x5a)?;
                        require_bytes(
                            true,
                            &fs::read(&temp)?,
                            &read_content(&expected)?,
                            "Git temporary save",
                        )?;
                    }
                    ops.rename(&temp, path)?;
                    ops.touch_file(path);
                    if verify {
                        require_bytes(true, &fs::read(path)?, &bytes, "Git renamed save")?;
                        if Path::new(&temp).exists() {
                            return Err("Git temporary survived rename".into());
                        }
                    }
                    ops.count("editor_save_count", 1);
                    *ops.phases.entry("editor_save_ns".into()).or_default() +=
                        phase.elapsed().as_nanos();
                } else {
                    let bytes = read_content(&original.xor(1024, 10, 0x5a)?.slice(1024, 10)?)?;
                    let file = ops.open(path, false, true)?;
                    ops.write_bytes(&file, path, 1024, &bytes)?;
                    ops.close(file)?;
                    ops.touch_file(path);
                    ops.count("inplace_edit_count", 1);
                    *ops.phases.entry("inplace_edit_ns".into()).or_default() +=
                        phase.elapsed().as_nanos();
                }
                m += 1;
                expected_status.insert(path.clone(), " M".into());
            }
            _ => return Err("Git target kind".into()),
        }
        ops.count("completed_target_count", 1);
    }
    // Normalize source files before Git, preserving parent normalization for finish.
    let files: Vec<_> = ops
        .changed
        .values()
        .filter(|e| matches!(e.kind, EntryKind::File(_)))
        .map(|e| e.path.clone())
        .collect();
    for path in files {
        ops.normalize_path(&path)?;
    }
    ops.phases
        .insert("apply_ns".into(), apply_start.elapsed().as_nanos());
    let commands: [(&str, &[&str]); 6] = [
        ("git_first_status", &["status", "--porcelain=v1", "-z"]),
        ("git_diff", &["diff", "--no-ext-diff", "--binary", "--"]),
        ("git_add", &["add", "-A", "--"]),
        ("git_cached_check", &["diff", "--cached", "--check"]),
        (
            "git_commit",
            &[
                "commit",
                "--no-gpg-sign",
                "--no-verify",
                "-m",
                "layerfs v0.1.3 tool workflow",
            ],
        ),
        ("git_final_status", &["status", "--porcelain=v1", "-z"]),
    ];
    for (name, args) in commands {
        let start = Instant::now();
        ops.count("git_process_count", 1);
        let output = git(Path::new("."), false, args)?;
        ops.phases
            .insert(format!("{name}_ns"), start.elapsed().as_nanos());
        ops.count("git_stdout_bytes", output.stdout.len() as u64);
        ops.count("git_stderr_bytes", output.stderr.len() as u64);
        if name == "git_final_status" && !output.stdout.is_empty() {
            return Err("Git final status not clean".into());
        }
        if verify && name == "git_first_status" && parse_status(&output.stdout)? != expected_status
        {
            return Err("Git first status independent path/status oracle".into());
        }
        if verify && name == "git_diff" {
            // Every tracked change has a diff header; untracked additions must not.
            let text = String::from_utf8(output.stdout.clone())?;
            for (kind, path, _) in targets.iter().take(case.tier) {
                if text.contains(&format!("diff --git a/{path} b/{path}")) != (*kind != "add") {
                    return Err("Git unstaged diff membership".into());
                }
            }
            let got = text
                .lines()
                .filter(|l| l.starts_with("diff --git "))
                .count();
            if got
                != targets
                    .iter()
                    .take(case.tier)
                    .filter(|(k, _, _)| *k != "add")
                    .count()
            {
                return Err("Git unexpected diff path".into());
            }
            if output.stdout != fs::read(reference.join("expected-diff"))? {
                return Err("Git independent exact tracked diff bytes".into());
            }
        }
    }
    // New loose-object paths are determined by supplied file bytes and Git format,
    // not by a timed repository census. Git creates one tree per changed directory.
    // The native reference path list is prequalified and passed as workload input.
    for entry in normalization {
        ops.changed.insert(entry.path.clone(), entry.clone());
    }
    Ok(())
}

pub(crate) fn capture_git_custody(root: &Path) -> Result<Receipt> {
    let mut entries = git_repository_entries(root)?;
    // Complete source manifest is independently verified separately; this seal is
    // repository persistence custody, including index stat-cache bytes.
    entries.push(Entry::directory("."));
    let manifest = common::manifest(&entries)?;
    let mut hash = Sha256::new();
    hash.update(manifest.as_bytes());
    let mut receipt = Receipt::new();
    receipt.insert(
        "repository_manifest_sha256".into(),
        super::hex(&hash.finish()),
    );
    receipt.insert(
        "repository_path_count".into(),
        (entries.len() - 1).to_string(),
    );
    receipt.insert(
        "repository_manifest_hex".into(),
        super::hex(manifest.as_bytes()),
    );
    Ok(receipt)
}

pub(crate) fn prepare_git_reference(root: &Path, case: &Case, seed: u8) -> Result<Receipt> {
    if case.kind != "git-tool" || root.exists() {
        return Err("Git reference needs selected Git case and absent output".into());
    }
    fs::create_dir(root)?;
    let repo = root.join("repository");
    common::create_fixture(&repo, &fixture(case, seed)?)?;
    let mut receipt = prepare_git(&repo, seed)?;
    let mut allocation_bound = 34 * MIB + 2500 + MIB;
    for entries in [fixture(case, seed)?, expected(case, seed, 1)?] {
        let mut tree_bytes: BTreeMap<String, u64> = BTreeMap::new();
        let mut index_bytes = 32;
        for entry in entries {
            let path = Path::new(&entry.path);
            if entry.path != "." {
                let parent = path
                    .parent()
                    .filter(|p| !p.as_os_str().is_empty())
                    .unwrap_or(Path::new("."));
                *tree_bytes
                    .entry(parent.to_string_lossy().into_owned())
                    .or_default() += 32 + path.file_name().ok_or("Git tree name")?.len() as u64;
            }
            if let EntryKind::File(data) = entry.kind {
                let n = data.len() + 64;
                allocation_bound += n + (n >> 12) + (n >> 14) + (n >> 25) + 13;
                index_bytes += ((62 + entry.path.len() as u64 + 1 + 7) / 8) * 8;
            }
        }
        for n in tree_bytes.into_values().map(|n| n + 64) {
            allocation_bound += n + (n >> 12) + (n >> 14) + (n >> 25) + 13;
        }
        allocation_bound += index_bytes;
    }
    if allocation_bound > 256 * MIB {
        return Err("Git conservative transient repository allocation exceeds 256 MiB".into());
    }
    receipt.insert(
        "git_repository_transient_bound_bytes".into(),
        allocation_bound.to_string(),
    );
    let before: BTreeMap<_, _> = git_repository_entries(&repo)?
        .into_iter()
        .map(|e| (e.path.clone(), e))
        .collect();
    // Native reference changes are built from the independent final source
    // descriptors, not by replaying the measured editor/episode implementation.
    for (kind, path, original) in git_targets(seed)?.into_iter().take(case.tier) {
        let native = repo.join(&path);
        match kind {
            "delete" => fs::remove_file(&native)?,
            "add" | "modify" => {
                let expected = if kind == "modify" {
                    original.xor(1024, 10, 0x5a)?
                } else {
                    original
                };
                expected.write_to(&mut File::create(&native)?)?;
                common::set_metadata(&native, &Entry::file(path, expected))?;
            }
            _ => unreachable!(),
        }
    }
    let status = git(&repo, false, &["status", "--porcelain=v1", "-z"])?;
    fs::write(root.join("expected-first-status"), status.stdout)?;
    fs::write(
        root.join("expected-diff"),
        git(&repo, false, &["diff", "--no-ext-diff", "--binary", "--"])?.stdout,
    )?;
    git(&repo, false, &["add", "-A", "--"])?;
    git(&repo, false, &["diff", "--cached", "--check"])?;
    git(
        &repo,
        false,
        &[
            "commit",
            "--no-gpg-sign",
            "--no-verify",
            "-m",
            "layerfs v0.1.3 tool workflow",
        ],
    )?;
    if !git(&repo, false, &["status", "--porcelain=v1", "-z"])?
        .stdout
        .is_empty()
    {
        return Err("native reference final status".into());
    }
    let after: BTreeMap<_, _> = git_repository_entries(&repo)?
        .into_iter()
        .map(|e| (e.path.clone(), e))
        .collect();
    let mut changed = BTreeSet::new();
    for (path, entry) in &after {
        let differs = match (before.get(path).map(|e| &e.kind), &entry.kind) {
            (Some(EntryKind::Directory), EntryKind::Directory) => false,
            (Some(EntryKind::File(old)), EntryKind::File(new)) => old.digest()? != new.digest()?,
            _ => true,
        };
        if differs {
            changed.insert(path.clone());
            if let Some(parent) = Path::new(path).parent() {
                changed.insert(parent.to_string_lossy().into_owned());
            }
        }
    }
    changed.insert(".git".into()); // index.lock creation and rename change this parent.
    let mut plan = String::new();
    for path in changed {
        let entry = after.get(&path).ok_or("native Git changed parent absent")?;
        plan.push_str(&format!(
            "{:o}\t{}\t{}\n",
            entry.mode,
            if matches!(entry.kind, EntryKind::Directory) {
                "d"
            } else {
                "f"
            },
            path
        ));
        let mut normalized = entry.clone();
        normalized.mtime_seconds = 1700000000;
        normalized.mtime_nanoseconds = 0;
        common::set_metadata(&repo.join(path), &normalized)?;
    }
    fs::write(root.join("normalization.tsv"), plan)?;
    // Store exact semantic expected identities; queries never define themselves
    // from the measured repository.
    for (file, args) in [
        ("expected-head", vec!["rev-parse", "HEAD"]),
        ("expected-tree", vec!["rev-parse", "HEAD^{tree}"]),
        ("expected-parent", vec!["rev-parse", "HEAD^"]),
    ] {
        let value = git_text(&repo, &args)?;
        fs::write(root.join(file), format!("{value}\n"))?;
        receipt.insert(file.replace('-', "_"), value);
    }
    receipt.insert("reference_case".into(), case.id.clone());
    receipt.insert("reference_seed".into(), seed.to_string());
    fs::write(
        root.join("identity"),
        format!(
            "{}\n{seed}\n{}\n",
            case.id,
            git_text(&repo, &["--version"])?
        ),
    )?;
    Ok(receipt)
}

fn git_normalization(reference: &Path, case: &Case, seed: u8) -> Result<Vec<Entry>> {
    let identity = fs::read_to_string(reference.join("identity"))?;
    if !identity.starts_with(&format!("{}\n{seed}\n", case.id)) {
        return Err("Git reference case/seed identity mismatch".into());
    }
    let mut entries = Vec::new();
    for line in fs::read_to_string(reference.join("normalization.tsv"))?.lines() {
        let parts: Vec<_> = line.split('\t').collect();
        if parts.len() != 3 {
            return Err("Git normalization entry".into());
        }
        let path = parts[2];
        if (path != ".git" && !path.starts_with(".git/"))
            || Path::new(path)
                .components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
        {
            return Err("Git normalization path outside repository".into());
        }
        let mut entry = match parts[1] {
            "d" => Entry::directory(path),
            "f" => Entry::file(path, Content::Zero { len: 0 }),
            _ => return Err("Git normalization entry kind".into()),
        };
        entry.mode = u32::from_str_radix(parts[0], 8)?;
        entries.push(entry);
    }
    Ok(entries)
}

pub(crate) fn verify_git(root: &Path, case: &Case, seed: u8, reference: &Path) -> Result<Receipt> {
    if case.kind != "git-tool" {
        return Err("not a Git verifier case".into());
    }
    git_normalization(reference, case, seed)?;
    // Capture before any Git command. Root compares this seal with the
    // pre-publication seal; the following observed .git entries prove custody,
    // never independent semantics.
    let mut receipt = capture_git_custody(root)?;
    let mut entries = expected(case, seed, 1)?;
    entries.extend(git_repository_entries(root)?);
    let bytes = common::validate_entries(&entries)?;
    if bytes > 256 * MIB {
        return Err("actual complete Git repository exceeds 256 MiB".into());
    }
    receipt.insert("git_repository_logical_bytes".into(), bytes.to_string());
    receipt.extend(common::verify_native(root, &entries)?);
    for (name, args) in [
        ("head", vec!["rev-parse", "HEAD"]),
        ("tree", vec!["rev-parse", "HEAD^{tree}"]),
        ("parent", vec!["rev-parse", "HEAD^"]),
    ] {
        let expected = fs::read_to_string(reference.join(format!("expected-{name}")))?;
        let observed = git_text(root, &args)?;
        if observed != expected.trim() {
            return Err(format!("Git independent {name} mismatch").into());
        }
        receipt.insert(format!("git_{name}"), observed);
    }
    git(root, false, &["fsck", "--strict"])?;
    if !git(root, false, &["status", "--porcelain=v1", "-z"])?
        .stdout
        .is_empty()
    {
        return Err("Git reopened status not clean".into());
    }
    if git_text(root, &["rev-list", "--count", "HEAD"])? != "2" {
        return Err("Git workflow must add exactly one commit".into());
    }
    receipt.insert("git_semantic_verification_status".into(), "pass".into());
    receipt.insert(
        "git_custody_status".into(),
        "requires_precommit_reopen_comparison".into(),
    );
    Ok(receipt)
}

pub(crate) fn apply(case: &Case, seed: u8, step: usize, verify: bool) -> Result<Receipt> {
    seed_label(seed)?;
    if step != 0 {
        return Err("ordinary workload has exactly one step, index zero".into());
    }
    if matches!(
        case.kind,
        "workspace-clean-commit" | "workspace-distributed-sdk-edit"
    ) {
        return Err("owner-only case must not invoke Exec".into());
    }
    // Only selected dependencies; schedules/fixture descriptors are not measured
    // filesystem work. Dense payload generation remains inside its declared wall.
    let plan_started = Instant::now();
    let order = match case.kind {
        "directory-construct" => rank(seed, "directory-construction")?,
        "workspace-dense-rewrite" => rank(seed, "workspace-dense-rewrite")?,
        "agent-episodes" => rank(seed, "agent-episodes")?,
        _ => Vec::new(),
    };
    let tiny = if matches!(case.kind, "tiny-create" | "tiny-stat" | "tiny-unlink") {
        tiny_targets(seed)?
    } else {
        Vec::new()
    };
    let bulk = if case.kind == "tiny-bulk-create" {
        common::shards(seed, case.tier, "bulk")?
    } else {
        Vec::new()
    };
    let offsets = if case.kind == "payload-random-read" {
        random_offsets(seed, case.tier)?
    } else {
        Vec::new()
    };
    let git_targets = if case.kind == "git-tool" {
        git_targets(seed)?
    } else {
        Vec::new()
    };
    let git_reference = if case.kind == "git-tool" {
        PathBuf::from(
            std::env::var_os("LAYERFS_V013_GIT_REFERENCE")
                .ok_or("qualified Git reference required")?,
        )
    } else {
        PathBuf::new()
    };
    let git_plan = if case.kind == "git-tool" {
        git_normalization(&git_reference, case, seed)?
    } else {
        Vec::new()
    };
    let plan_ns = plan_started.elapsed().as_nanos();
    let mut ops = Ops::default();
    for key in [
        "attempted_syscall_count",
        "completed_syscall_count",
        "interrupted_syscall_count",
        "completed_read_bytes",
        "completed_write_bytes",
        "completed_file_write_count",
        "completed_read_request_count",
        "completed_target_count",
        "completed_chain_count",
        "completed_episode_count",
        "visited_path_count",
        "visited_file_count",
        "directory_entry_count",
        "metadata_normalization_count",
        "git_process_count",
        "editor_save_count",
        "inplace_edit_count",
    ] {
        ops.count(key, 0);
    }
    for name in [
        "open",
        "open_directory",
        "close",
        "pread",
        "pwrite",
        "lstat",
        "mkdir",
        "chmod",
        "opendir",
        "closedir",
        "rename",
        "unlink",
        "rmdir",
        "ftruncate",
        "symlink",
        "fsync",
        "fsyncdir",
    ] {
        ops.count(&format!("workload_{name}_call_count"), 0);
    }
    let start = Instant::now();
    let result = (|| -> Result<()> {
        match case.kind {
            "payload-create" => ops.write_content(
                "payload.bin",
                &Content::Seed {
                    seed: FLAT_SEED,
                    len: case.tier as u64 * MIB,
                },
                true,
                true,
            )?,
            "payload-random-read" => {
                let file = ops.open("payload.bin", false, false)?;
                let mut bytes = vec![0; 4096];
                for offset in offsets {
                    ops.read_range(&file, "payload.bin", offset, &mut bytes)?;
                    if verify {
                        let mut expected = vec![0; 4096];
                        Content::Seed {
                            seed: FLAT_SEED,
                            len: 500 * MIB,
                        }
                        .read_at(offset, &mut expected)?;
                        require_bytes(true, &bytes, &expected, "random read")?;
                    }
                    ops.count("completed_read_request_count", 1);
                }
                ops.close(file)?;
            }
            "tiny-create" | "tiny-stat" | "tiny-unlink" => {
                for (path, data) in tiny.into_iter().take(case.tier) {
                    match case.kind {
                        "tiny-create" => ops.write_content(&path, &data, true, false)?,
                        "tiny-unlink" => ops.unlink(&path, false)?,
                        _ => {
                            let attr = ops.call("lstat", &path, || fs::symlink_metadata(&path))?;
                            if !attr.is_file()
                                || attr.len() != data.len()
                                || attr.mode() & 0o7777 != 0o640
                                || attr.mtime() != 1700000000
                                || attr.mtime_nsec() != 0
                            {
                                return Err(format!("tiny lstat metadata {path}").into());
                            }
                        }
                    }
                    ops.count("completed_target_count", 1);
                }
            }
            "tiny-bulk-create" => create_entries(&mut ops, bulk, "bulk")?,
            "tiny-bulk-delete" => ops.delete_tree("bulk")?,
            "directory-construct" => {
                for (k, i) in order.iter().copied().take(case.tier).enumerate() {
                    let mut path = format!("new-directories/c{i:03}");
                    ops.mkdir(&path)?;
                    for d in 1..DEPTHS[k % 10] {
                        path.push_str(&format!("/d{d:03}"));
                        ops.mkdir(&path)?;
                    }
                    ops.count("completed_chain_count", 1);
                }
            }
            "directory-metadata-scan" | "directory-content-scan" => {
                let oracle: Option<BTreeMap<_, _>> = if verify {
                    Some(
                        fixture(case, seed)?
                            .into_iter()
                            .map(|e| (e.path.clone(), e))
                            .collect(),
                    )
                } else {
                    None
                };
                scan(
                    &mut ops,
                    ".",
                    case.kind == "directory-content-scan",
                    verify,
                    oracle.as_ref(),
                )?;
                if let Some(oracle) = oracle {
                    if ops.counts.get("visited_path_count").copied().unwrap_or(0)
                        != oracle.len() as u64
                    {
                        return Err("scan exact membership cardinality".into());
                    }
                }
            }
            "namespace-subtree-relocate-delete" => {
                let phase = Instant::now();
                ops.rename("source/tree-a", "destination/moved-a")?;
                ops.phases
                    .insert("move_ns".into(), phase.elapsed().as_nanos());
                if verify {
                    let mut moved = vec![Entry::directory(".")];
                    for s in 0..case.tier {
                        moved.push(Entry::directory(format!("s{s:03}")));
                        for j in 0..200 {
                            moved.push(Entry::file(
                                format!("s{s:03}/f{j:03}.dat"),
                                content(
                                    seed,
                                    "namespace-mutation",
                                    &format!("source/tree-a/s{s:03}/f{j:03}.dat"),
                                    1024,
                                )?,
                            ));
                        }
                    }
                    common::verify_native(Path::new("destination/moved-a"), &moved)?;
                }
                let phase = Instant::now();
                ops.delete_tree("source/tree-b")?;
                ops.phases
                    .insert("delete_ns".into(), phase.elapsed().as_nanos());
            }
            "workspace-fixed-move" => ops.rename("regular/s000/f064.dat", "dest/moved.dat")?,
            "workspace-dense-rewrite" => {
                for s in order.iter().copied().take(case.tier) {
                    for j in 0..200 {
                        let path = shard_path(s, j);
                        let data = content(
                            seed,
                            "workspace-dense-rewrite",
                            &path,
                            shard_content(seed, s, j)?.len(),
                        )?;
                        ops.write_content(&path, &data, false, false)?;
                    }
                }
            }
            "agent-episodes" => {
                for i in order.iter().copied().take(case.tier) {
                    episode(&mut ops, seed, i, verify)?;
                }
            }
            "git-tool" => git_apply(
                &mut ops,
                case,
                &git_targets,
                &git_plan,
                &git_reference,
                verify,
            )?,
            _ => return Err("unknown ordinary apply case".into()),
        }
        if !matches!(
            case.kind,
            "payload-random-read"
                | "tiny-stat"
                | "directory-metadata-scan"
                | "directory-content-scan"
        ) {
            ops.finish()?;
        }
        Ok(())
    })();
    ops.phases
        .insert("workload_ns".into(), start.elapsed().as_nanos());
    let mut receipt = ops.receipt();
    receipt.insert("workload_plan_ns".into(), plan_ns.to_string());
    receipt.insert("scenario_id".into(), case.id.clone());
    receipt.insert("seed".into(), seed.to_string());
    receipt.insert(
        "benchmark_verifier_count".into(),
        u8::from(verify).to_string(),
    );
    receipt.insert("benchmark_reopen_count".into(), "0".into());
    receipt.insert("benchmark_injection_count".into(), "0".into());
    if verify && !ops.traversal.is_empty() {
        let transcript = ops.traversal.join("\n");
        let mut digest = Sha256::new();
        digest.update(transcript.as_bytes());
        receipt.insert(
            "traversal_transcript_sha256".into(),
            super::hex(&digest.finish()),
        );
        receipt.insert(
            "traversal_transcript_hex".into(),
            super::hex(transcript.as_bytes()),
        );
    }
    if let Err(error) = result {
        for (key, value) in &receipt {
            eprintln!("partial_{key}={value}");
        }
        return Err(error);
    }
    receipt.insert("workload_status".into(), "pass".into());
    Ok(receipt)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_payload_and_episode_match_independent_oracles() -> Result<()> {
        // Run in a dedicated subprocess because apply intentionally uses cwd.
        if std::env::var_os("LAYERFS_ORDINARY_NATIVE_CHILD").is_none() {
            let root = std::env::temp_dir()
                .join(format!("layerfs-ordinary-native-{}", std::process::id()));
            fs::create_dir(&root)?;
            let outcome=Command::new(std::env::current_exe()?).args(["--exact","ordinary_workloads::tests::native_payload_and_episode_match_independent_oracles","--test-threads=1","--nocapture"])
                .env("LAYERFS_ORDINARY_NATIVE_CHILD","1").current_dir(&root).status();
            fs::remove_dir_all(&root)?;
            if !outcome?.success() {
                return Err("ordinary native child failed".into());
            }
            return Ok(());
        }
        let root = std::env::current_dir()?;
        let payload = Case {
            id: "payload-create-1m".into(),
            family: "payload_create_read",
            tier: 1,
            kind: "payload-create",
        };
        common::create_fixture(&root, &fixture(&payload, 1)?)?;
        apply(&payload, 1, 0, false)?;
        common::verify_native(&root, &expected(&payload, 1, 1)?)?;
        fs::remove_file("payload.bin")?;
        let episode_case = Case {
            id: "agent-episodes-1".into(),
            family: "mixed_load_bearing",
            tier: 1,
            kind: "agent-episodes",
        };
        let index = rank(1, "agent-episodes")?[0];
        let old = format!("cells/e{index:03}");
        let new = format!("finished/e{index:03}");
        let select = |e: &Entry| {
            e.path == "."
                || e.path == "cells"
                || e.path == "finished"
                || e.path == old
                || e.path.starts_with(&format!("{old}/"))
                || e.path == new
                || e.path.starts_with(&format!("{new}/"))
        };
        let input: Vec<_> = fixture(&episode_case, 1)?
            .into_iter()
            .filter(&select)
            .collect();
        common::create_fixture(&root, &input)?;
        apply(&episode_case, 1, 0, true)?;
        let final_entries: Vec<_> = expected(&episode_case, 1, 1)?
            .into_iter()
            .filter(select)
            .collect();
        common::verify_native(&root, &final_entries)?;
        Ok(())
    }

    #[test]
    fn native_git_reference_checks_workflow() -> Result<()> {
        if std::env::var_os("LAYERFS_ORDINARY_GIT_CHILD").is_none() {
            let root =
                std::env::temp_dir().join(format!("layerfs-ordinary-git-{}", std::process::id()));
            fs::create_dir(&root)?;
            let outcome = Command::new(std::env::current_exe()?)
                .args([
                    "--exact",
                    "ordinary_workloads::tests::native_git_reference_checks_workflow",
                    "--test-threads=1",
                    "--nocapture",
                ])
                .env("LAYERFS_ORDINARY_GIT_CHILD", "1")
                .current_dir(&root)
                .status();
            fs::remove_dir_all(&root)?;
            if !outcome?.success() {
                return Err("native Git child failed".into());
            }
            return Ok(());
        }
        let root = std::env::current_dir()?;
        let repo = root.join("worktree");
        let reference = root.join("reference");
        let case = Case {
            id: "git-tool-10".into(),
            family: "git_tool_workflow",
            tier: 10,
            kind: "git-tool",
        };
        prepare_git_reference(&reference, &case, 1)?;
        common::create_fixture(&repo, &fixture(&case, 1)?)?;
        prepare_git(&repo, 1)?;
        std::env::set_current_dir(&repo)?;
        std::env::set_var("LAYERFS_V013_GIT_REFERENCE", &reference);
        let receipt = apply(&case, 1, 0, true)?;
        assert_eq!(
            receipt.get("git_process_count").map(String::as_str),
            Some("6")
        );
        assert_eq!(
            receipt.get("editor_save_count").map(String::as_str),
            Some("2")
        );
        assert_eq!(
            receipt.get("inplace_edit_count").map(String::as_str),
            Some("2")
        );
        verify_git(&repo, &case, 1, &reference)?;
        Ok(())
    }
}

// BEGIN NATIVE FAST VERIFICATION V1
/// Compatibility entry point; routine families share the independent generic delta.
pub(crate) fn fast_delta(case: &Case, seed: u8, step: usize) -> Result<common::FastDelta> {
    fast_delta_for_entries(case, seed, step, &super::workspace_registry::expected(case, seed, step)?)
}

pub(crate) fn fast_changed_paths(case: &Case, seed: u8, step: usize) -> Result<BTreeSet<String>> {
    Ok(fast_delta(case, seed, step)?.changed_paths)
}

pub(crate) fn fast_delta_for_entries(case: &Case, seed: u8, step: usize, entries: &[Entry]) -> Result<common::FastDelta> {
    if super::workspace_registry::proofs().iter().any(|proof|proof.id==case.id) || step>super::workspace_registry::steps(case) {
        return Err("targeted proof or invalid step cannot use routine fast verification".into());
    }
    let before=if super::workspace_registry::is_import(case) {vec![Entry::directory(".")]} else {super::workspace_registry::fixture(case,seed)?};
    let mut delta=common::fast_delta_from_entries(&before,entries,seed,&case.id)?;
    // Read targets and canceled metadata mutations remain exercised witnesses/affected paths.
    if case.kind=="tiny-stat" && step>0 {
        for (path,_) in tiny_targets(seed)?.into_iter().take(case.tier) {
            delta.witness_paths.insert(path.clone()); delta.witness_paths.insert(".".into());
            for(index,_)in path.match_indices('/') {delta.witness_paths.insert(path[..index].to_owned());}
        }
    }
    if case.family=="dedup_branch_history" && case.kind=="metadata" && step>0 {
        let path=super::dedup_workloads::shard_path(0);
        delta.changed_paths.insert(path.clone()); delta.changed_paths.insert(".".into());
        for(index,_)in path.match_indices('/') {delta.changed_paths.insert(path[..index].to_owned());}
    }
    Ok(delta)
}
