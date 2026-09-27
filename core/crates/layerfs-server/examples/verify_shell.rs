//! Read-only full-tree and history oracle for the #243 shell package scenario.
use layerfs_content::{
    filesystem::{read::FilesystemRead, root::FilesystemRootId},
    inode_leaf::InodeKind,
    read_all, LogicalPath,
};
use layerfs_history::{sqlite::open_read_only, BranchId, HistoryCatalog, LayerStackId};
use layerfs_storage::{Store, StoreProvider};
use layerfs_telemetry::timer::Timing;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fmt::Write as _,
    fs,
    io::{self, Write},
    path::Path,
};

#[derive(Clone)]
struct Expected {
    kind: char,
    mode: u32,
    size: u64,
    sha256: String,
}
struct HashWriter {
    hash: Sha256,
    bytes: u64,
}
impl Write for HashWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.hash.update(bytes);
        self.bytes += bytes.len() as u64;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut out, "{byte:02x}").unwrap();
    }
    out
}
fn unhex(value: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    if value.len() % 2 != 0 {
        return Err("odd hex width".into());
    }
    (0..value.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&value[i..i + 2], 16).map_err(Into::into))
        .collect()
}
fn fields(path: &str) -> Result<BTreeMap<String, String>, Box<dyn std::error::Error>> {
    let mut out = BTreeMap::new();
    for line in fs::read_to_string(path)?.lines() {
        let (key, value) = line.split_once('=').ok_or("invalid case row")?;
        if out.insert(key.into(), value.into()).is_some() {
            return Err("duplicate case key".into());
        }
    }
    Ok(out)
}
fn manifest(path: &str) -> Result<BTreeMap<String, Expected>, Box<dyn std::error::Error>> {
    let mut out = BTreeMap::new();
    for line in fs::read_to_string(path)?.lines() {
        let parts: Vec<_> = line.split('\t').collect();
        if parts.len() != 5 {
            return Err("manifest row width".into());
        }
        let name = if parts[0] == "." { "" } else { parts[0] };
        let wanted = Expected {
            kind: parts[1].parse()?,
            mode: parts[2].parse()?,
            size: parts[3].parse()?,
            sha256: parts[4].into(),
        };
        if out.insert(name.into(), wanted).is_some() {
            return Err("duplicate path".into());
        }
    }
    if out.get("").is_none_or(|root| root.kind != 'd') {
        return Err("missing root".into());
    }
    Ok(out)
}
fn verify_tree(
    provider: &StoreProvider<'_>,
    root: layerfs_content::ObjectId,
    expected: &BTreeMap<String, Expected>,
) -> Result<(usize, usize, u64), Box<dyn std::error::Error>> {
    let mut tree = FilesystemRead::new(provider, FilesystemRootId(root))?;
    let mut queue = VecDeque::from([String::new()]);
    let mut seen = BTreeSet::new();
    let mut files = 0;
    let mut bytes = 0;
    while let Some(path) = queue.pop_front() {
        if !seen.insert(path.clone()) {
            return Err("duplicate actual path".into());
        }
        let wanted = expected
            .get(&path)
            .ok_or_else(|| format!("unexpected path {path}"))?;
        let logical = LogicalPath::new(&path)?;
        let value = tree.resolve(&logical)?.value;
        let kind = match value.kind {
            InodeKind::Directory => 'd',
            InodeKind::RegularFile => 'f',
            InodeKind::Symlink => return Err("unexpected symlink".into()),
        };
        let portable = tree.read_portable(&logical)?;
        if kind != wanted.kind || portable.mode != wanted.mode {
            return Err(format!("type/mode mismatch {path}").into());
        }
        if kind == 'f' {
            let mut sink = HashWriter {
                hash: Sha256::new(),
                bytes: 0,
            };
            Timing::disabled("read", |scope| {
                read_all(provider, value.content_root, &mut sink, scope.child("file"))
            })
            .0?;
            if sink.bytes != wanted.size || hex(&sink.hash.finalize()) != wanted.sha256 {
                return Err(format!("content mismatch {path}").into());
            }
            files += 1;
            bytes += sink.bytes;
        } else {
            let mut after = None;
            loop {
                let page = tree.list(&logical, after.as_ref(), 128, 16_384)?;
                for (name, _) in &page.entries {
                    queue.push_back(if path.is_empty() {
                        name.as_str().to_owned()
                    } else {
                        format!("{path}/{}", name.as_str())
                    });
                }
                after = page.continuation;
                if after.is_none() {
                    break;
                }
            }
        }
    }
    if seen.len() != expected.len() {
        return Err("missing final paths".into());
    }
    Ok((seen.len(), files, bytes))
}
fn verify_separated(
    provider: &StoreProvider<'_>,
    old_root: layerfs_content::ObjectId,
    new_root: layerfs_content::ObjectId,
    count: usize,
) -> Result<usize, Box<dyn std::error::Error>> {
    let mut contents = Vec::new();
    for root in [old_root, new_root] {
        let mut tree = FilesystemRead::new(provider, FilesystemRootId(root))?;
        let file = tree.resolve(&LogicalPath::new("data.bin")?)?.value;
        let mut bytes = Vec::new();
        Timing::disabled("read", |scope| {
            read_all(provider, file.content_root, &mut bytes, scope.child("file"))
        })
        .0?;
        contents.push(bytes);
    }
    let (old, new) = (&contents[0], &contents[1]);
    if old.len() != 8194 || new.len() != old.len() || count > 4097 {
        return Err("separated file length/count mismatch".into());
    }
    let (mut runs, mut previous_changed) = (0, false);
    for (index, (&before, &after)) in old.iter().zip(new).enumerate() {
        let expected = if index % 2 == 0 && index / 2 < count {
            b'X'
        } else {
            b'A'
        };
        if before != b'A' || after != expected {
            return Err(format!("separated byte mismatch at {index}").into());
        }
        let changed = before != after;
        runs += usize::from(changed && !previous_changed);
        previous_changed = changed;
    }
    if runs != count {
        return Err(format!("changed runs {runs} != {count}").into());
    }
    Ok(runs)
}
fn verify_pattern(
    provider: &StoreProvider<'_>,
    old_root: layerfs_content::ObjectId,
    new_root: layerfs_content::ObjectId,
    pattern: &str,
) -> Result<usize, Box<dyn std::error::Error>> {
    const SIZE: usize = 10 << 20;
    let mut expected = vec![b'A'; SIZE];
    for i in 0..100usize {
        let byte = b'B' + (i % 24) as u8;
        match pattern {
            "append" => expected.push(byte),
            "dispersed" => expected[(104729 + i * 2654435761) % SIZE] = byte,
            "repeated" => expected[5 << 20] = byte,
            _ => return Err("unknown write pattern".into()),
        }
    }
    let mut contents = Vec::new();
    for root in [old_root, new_root] {
        let mut tree = FilesystemRead::new(provider, FilesystemRootId(root))?;
        let file = tree.resolve(&LogicalPath::new("data.bin")?)?.value;
        let mut bytes = Vec::new();
        Timing::disabled("read", |scope| {
            read_all(provider, file.content_root, &mut bytes, scope.child("file"))
        })
        .0?;
        contents.push(bytes);
    }
    let (old, new) = (&contents[0], &contents[1]);
    if old.len() != SIZE || old.iter().any(|byte| *byte != b'A') || new != &expected {
        return Err("write pattern old/new bytes mismatch".into());
    }
    let mut previous_changed = false;
    let mut runs = 0;
    for (index, byte) in new.iter().enumerate() {
        let changed = old.get(index) != Some(byte);
        runs += usize::from(changed && !previous_changed);
        previous_changed = changed;
    }
    if runs != (if pattern == "dispersed" { 100 } else { 1 }) {
        return Err(format!("write pattern changed runs {runs}").into());
    }
    Ok(runs)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 6 {
        return Err("case store history old-manifest new-manifest required".into());
    }
    let case = fields(&args[1])?;
    let get = |key: &str| {
        case.get(key)
            .map(String::as_str)
            .ok_or_else(|| format!("missing {key}"))
    };
    let cursor: [u8; 32] = unhex(&std::env::var("LAYERFS_HISTORY_CURSOR_KEY")?)?
        .try_into()
        .map_err(|_| "cursor width")?;
    let history = open_read_only(Path::new(&args[3]), b"layerfs-bench-pro", cursor)?;
    let project = unhex(get("project_id")?)?;
    let stack: [u8; 16] = project[1..].try_into()?;
    let stack = history
        .layer_stack(LayerStackId::from_authority(stack))?
        .ok_or("missing stack")?;
    let genesis_id = layerfs_history::LayerId::from_bytes(
        unhex(get("genesis_layer")?)?
            .try_into()
            .map_err(|_| "layer width")?,
    )?;
    let genesis = history.layer(genesis_id)?.ok_or("missing genesis")?;
    if hex(genesis.root.as_bytes()) != get("genesis_root")? || stack.head_layer != genesis.id {
        return Err("genesis identity changed".into());
    }
    let branch: [u8; 17] = unhex(get("branch_id")?)?
        .try_into()
        .map_err(|_| "branch width")?;
    let branch = history
        .branch(BranchId::from_bytes(branch).map_err(|_| "branch identity")?)?
        .ok_or("missing branch")?;
    let head = branch.head_commit.ok_or("missing branch head")?;
    let old = layerfs_history::CommitId::from_bytes(
        unhex(get("old_commit")?)?
            .try_into()
            .map_err(|_| "old commit width")?,
    )
    .map_err(|_| "old commit identity")?;
    let old_record = history.commit(old)?.ok_or("missing old Commit")?;
    let expected_failure = get("expected_failure")? == "1";
    if expected_failure && head != old {
        return Err("failure advanced Branch head".into());
    }
    if !expected_failure && (hex(&head.to_bytes()) != get("expected_head_commit")? || head == old) {
        return Err("new Branch head mismatch".into());
    }
    let store = Timing::disabled("open", |scope| Store::open(&args[2], scope.child("store"))).0?;
    let provider = StoreProvider::new(&store);
    let old_manifest = manifest(&args[4])?;
    let old_tree = verify_tree(&provider, old_record.root, &old_manifest)?;
    let (new_tree, new_root) = if expected_failure {
        (None, None)
    } else {
        let committed = history.commit(head)?.ok_or("missing new Commit")?;
        if committed.parent != Some(old) {
            return Err("new Commit parent mismatch".into());
        }
        (
            Some(verify_tree(
                &provider,
                committed.root,
                &manifest(&args[5])?,
            )?),
            Some(committed.root),
        )
    };
    let separated_runs = match (case.get("separated_count"), new_root) {
        (Some(count), Some(root)) => Some(verify_separated(
            &provider,
            old_record.root,
            root,
            count.parse()?,
        )?),
        _ => None,
    };
    let pattern_runs = match (case.get("write_pattern"), new_root) {
        (Some(pattern), Some(root)) => {
            Some(verify_pattern(&provider, old_record.root, root, pattern)?)
        }
        _ => None,
    };
    let new_tree_json = new_tree.map_or("null".to_string(), |(paths, files, bytes)| {
        format!("{{\"paths\":{paths},\"files\":{files},\"bytes\":{bytes}}}")
    });
    let separated_runs_json = separated_runs.map_or("null".to_owned(), |runs| runs.to_string());
    let pattern_runs_json = pattern_runs.map_or("null".to_owned(), |runs| runs.to_string());
    println!("{{\"status\":\"PASS\",\"old_commit\":{:?},\"head_commit\":{:?},\"old_paths\":{},\"old_files\":{},\"old_bytes\":{},\"new_tree\":{new_tree_json},\"separated_runs\":{separated_runs_json},\"pattern_runs\":{pattern_runs_json}}}",
        hex(&old.to_bytes()), hex(&head.to_bytes()), old_tree.0, old_tree.1, old_tree.2,
    );
    Ok(())
}
