//! Independent bounded oracle for one published #232 Workspace Exec/FUSE edit.
//!
//! The verifier never runs the product route: it reopens the case's Store and
//! history read-only through public C1/C2/C5 readers, re-derives the expected
//! bytes from the fixture recipe and the declared edit plan, and checks the
//! published head plus the retained historical root.
use layerfs_content::{
    filesystem::{read::FilesystemRead, root::FilesystemRootId},
    inode_leaf::InodeKind,
    read_all_bounded, read_range, FileContent, FileView, LogicalPath, ObjectId,
};
use layerfs_history::{sqlite::open_read_only, HistoryCatalog, LayerStackId};
use layerfs_storage::{Store, StoreProvider};
use layerfs_telemetry::timer::Timing;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fmt::Write as _,
    io::{self, Write},
    path::Path,
};

struct Case {
    fields: BTreeMap<String, String>,
}
impl Case {
    fn load(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let mut fields = BTreeMap::new();
        for line in std::fs::read_to_string(path)?.lines() {
            if line.trim().is_empty() || line.starts_with('#') {
                continue;
            }
            let (key, value) = line.split_once('=').ok_or("case row without '='")?;
            fields.insert(key.to_string(), value.to_string());
        }
        Ok(Self { fields })
    }
    fn get(&self, key: &str) -> Result<&str, Box<dyn std::error::Error>> {
        self.fields
            .get(key)
            .map(String::as_str)
            .ok_or_else(|| format!("case field {key}").into())
    }
    fn number(&self, key: &str) -> Result<u64, Box<dyn std::error::Error>> {
        Ok(self.get(key)?.parse()?)
    }
    fn hex(&self, key: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let text = self.get(key)?;
        if text.len() % 2 != 0 {
            return Err("odd hex width".into());
        }
        (0..text.len())
            .step_by(2)
            .map(|offset| u8::from_str_radix(&text[offset..offset + 2], 16).map_err(Into::into))
            .collect()
    }
}

struct DigestWriter {
    hash: Sha256,
    bytes: u64,
}
impl Write for DigestWriter {
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
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut text, "{byte:02x}").expect("string write");
    }
    text
}

/// Size, canonical root and extent count of one logical file, without a full read.
fn shape(
    provider: &StoreProvider<'_>,
    root: ObjectId,
    path: &LogicalPath,
) -> Result<(u64, String, u64, u64), String> {
    let mut filesystem =
        FilesystemRead::new(provider, FilesystemRootId(root)).map_err(|error| error.to_string())?;
    let stat = filesystem.stat(path).map_err(|error| error.to_string())?;
    if stat.kind != InodeKind::RegularFile {
        return Err("payload is not a regular file".into());
    }
    let view = Timing::disabled("view", |scope| {
        FileView::open(provider, stat.content_root, scope.child("file"))
    })
    .0
    .map_err(|error| error.to_string())?;
    match view.content() {
        FileContent::WholeFile { logical_len } => {
            Ok((logical_len, hex(stat.content_root.as_bytes()), 0, 0))
        }
        FileContent::Chunked(state) => Ok((
            state.logical_len,
            hex(stat.content_root.as_bytes()),
            state.extent_count,
            1,
        )),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 4 {
        return Err("case store history required".into());
    }
    let case = Case::load(Path::new(&args[1]))?;
    let cursor: [u8; 32] = {
        let text = std::env::var("LAYERFS_HISTORY_CURSOR_KEY")?;
        let raw: Vec<u8> = (0..text.len())
            .step_by(2)
            .map(|offset| u8::from_str_radix(&text[offset..offset + 2], 16))
            .collect::<Result<_, _>>()?;
        raw.try_into().map_err(|_| "cursor key width")?
    };
    let history = open_read_only(Path::new(&args[3]), b"layerfs-bench-pro", cursor)?;
    let stack: [u8; 16] = case
        .hex("stack_body")?
        .try_into()
        .map_err(|_| "stack body width")?;
    let record = history
        .layer_stack(LayerStackId::from_authority(stack))?
        .ok_or("missing stack")?;
    let branch: [u8; 17] = case
        .hex("branch_id")?
        .try_into()
        .map_err(|_| "branch id width")?;
    let branch_record = history
        .branch(layerfs_history::BranchId::from_bytes(branch).map_err(|_| "branch identity")?)?
        .ok_or("missing branch")?;
    let head = branch_record
        .head_commit
        .ok_or("branch has no head commit")?;
    let commit = history.commit(head)?.ok_or("missing head commit")?;
    if record.head_layer != commit.base_layer {
        return Err("head commit base layer is not the stack head layer".into());
    }
    let expected_head = case.get("expected_head_commit")?;
    if hex(&head.to_bytes()) != expected_head {
        return Err("branch head differs from the performance receipt".into());
    }
    let store = Timing::disabled("open", |scope| Store::open(&args[2], scope.child("store"))).0?;
    let provider = StoreProvider::new(&store);
    let path = LogicalPath::new("payload.bin")?;
    let (bytes, canonical_root, extent_count, chunked) = shape(&provider, commit.root, &path)?;
    let final_bytes = case.number("final_bytes")?;
    if bytes != final_bytes {
        return Err(format!("final size {bytes} != declared {final_bytes}").into());
    }
    let fixture_root = case.get("fixture_canonical_root")?;
    if canonical_root == fixture_root {
        return Err("published root equals the pristine fixture root; nothing was edited".into());
    }
    // Bounded coverage is declared; a full-file digest is only taken when the
    // declared result fits inside the frozen full-digest bound. Otherwise every
    // declared oracle window is read and checked, and the coverage string names
    // exactly how many windows were covered.
    let content_root = ObjectId::from_bytes(&hex_to_bytes(&canonical_root)?)?;
    let full_digest = case.get("full_file_digest")? == "1";
    let window_count = case.number("window_count")?;
    let mut window_report = String::from("[]");
    let (coverage, observed, expected, full_covered, content_ok) = if full_digest {
        let mut sink = DigestWriter {
            hash: Sha256::new(),
            bytes: 0,
        };
        Timing::disabled("read", |timer| {
            read_all_bounded(
                &provider,
                content_root,
                u64::MAX,
                &mut sink,
                timer.child("file"),
            )
        })
        .0?;
        let observed = hex(&sink.hash.finalize());
        let expected = case.get("final_sha256")?.to_string();
        let matched = observed == expected;
        (String::from("full-file"), observed, expected, true, matched)
    } else {
        let count = window_count;
        let mut entries: Vec<String> = Vec::new();
        let mut first = (String::new(), String::new());
        let mut matched = true;
        for index in 0..count {
            let offset = case.number(&format!("window{index}_offset"))?;
            let length = case.number(&format!("window{index}_bytes"))?;
            let expected = case.get(&format!("window{index}_sha256"))?.to_string();
            let mut sink = DigestWriter {
                hash: Sha256::new(),
                bytes: 0,
            };
            Timing::disabled("read", |timer| {
                read_range(
                    &provider,
                    content_root,
                    offset..offset + length,
                    &mut sink,
                    timer.child("file"),
                )
            })
            .0?;
            let observed = hex(&sink.hash.finalize());
            let window_ok = observed == expected;
            matched &= window_ok;
            if entries.is_empty() {
                first = (observed.clone(), expected.clone());
            }
            entries.push(format!(
                "{{\"index\":{index},\"offset\":{offset},\"bytes\":{length},\
\"observed_digest\":\"{observed}\",\"expected_digest\":\"{expected}\",\
\"match\":{window_ok}}}"
            ));
        }
        window_report = format!("[{}]", entries.join(","));
        (format!("bounded-windows-{count}"), first.0, first.1, false, matched)
    };
    let canonical_ok = match case.get("canonical_root_expected")? {
        "-" => true,
        value => value == canonical_root,
    };
    let count_ok = match case.get("canonical_count_expected")? {
        "-" => true,
        value => value.parse::<u64>()? == extent_count,
    };
    // The retained historical root: the pristine genesis layer still resolves
    // the untouched fixture with its recorded canonical root and extent count.
    let genesis: [u8; 33] = case
        .hex("genesis_layer")?
        .try_into()
        .map_err(|_| "genesis layer width")?;
    let layer = history
        .layer(
            layerfs_history::LayerId::from_bytes(genesis).map_err(|_| "genesis layer identity")?,
        )?
        .ok_or("missing genesis layer")?;
    let (fixture_bytes, fixture_observed, fixture_extents, _) =
        shape(&provider, layer.root, &path)?;
    let fixture_ok = fixture_bytes == case.number("fixture_bytes")?
        && fixture_observed == fixture_root
        && fixture_extents == case.number("fixture_extent_count")?;
    let status = if content_ok && canonical_ok && count_ok && fixture_ok {
        "PASS"
    } else {
        "FAIL"
    };
    let receipt = format!(
        "{{\"schema\":\"core-fs-bench-pro-exec-fuse-edit-verification-v2\",\"status\":\"{status}\",\
\"scenario_id\":\"{}\",\"observed_bytes\":{bytes},\"declared_bytes\":{final_bytes},\
\"coverage\":\"{coverage}\",\"full_file_bytes_verified\":{full_covered},\
\"window_count\":{window_count},\"windows\":{window_report},\
\"observed_digest\":\"{observed}\",\"expected_digest\":\"{expected}\",\
\"content_match\":{content_ok},\"canonical_root\":\"{canonical_root}\",\
\"canonical_root_expected\":\"{}\",\"canonical_root_match\":{canonical_ok},\
\"extent_count\":{extent_count},\"chunked\":{chunked},\"canonical_count_match\":{count_ok},\
\"head_commit\":\"{}\",\"branch_id\":\"{}\",\"genesis_root\":\"{}\",\
\"fixture_bytes\":{fixture_bytes},\"fixture_canonical_root_observed\":\"{fixture_observed}\",\
\"fixture_extent_count_observed\":{fixture_extents},\"historical_root_match\":{fixture_ok},\
\"store\":\"{}\",\"history\":\"{}\"}}",
        case.get("scenario_id")?,
        case.get("canonical_root_expected")?,
        hex(&head.to_bytes()),
        hex(&branch),
        hex(layer.root.as_bytes()),
        args[2],
        args[3],
    );
    println!("{receipt}");
    if status != "PASS" {
        return Err("verification failed".into());
    }
    Ok(())
}

fn hex_to_bytes(text: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    (0..text.len())
        .step_by(2)
        .map(|offset| u8::from_str_radix(&text[offset..offset + 2], 16).map_err(Into::into))
        .collect()
}
