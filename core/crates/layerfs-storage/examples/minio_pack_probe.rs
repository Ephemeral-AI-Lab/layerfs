//! External pack construction fixture for the standalone MinIO experiment.
use std::error::Error;
use std::fs::{self, File};
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use layerfs_content::file::mapping::encode_chunk_object;
use layerfs_content::{ObjectId, ObjectRole};
use layerfs_storage::cas::SaveProfile;
use layerfs_storage::encoding::codec::CompressionWorkspace;
use layerfs_storage::encoding::encode_full;
use layerfs_storage::pack::layout::record_range;
use layerfs_storage::pack::{
    append_fits, assemble_consuming, assembled_length, build_group, framed_group_length,
    group_view, parse_header, EncodedGroup, PackLane,
};
use layerfs_storage::policy::{GROUP_TARGET, PACK_LIMIT};
use layerfs_storage::{StorageCapacities, StoragePolicy};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

struct Builder {
    root: PathBuf,
    groups: Vec<EncodedGroup>,
    members: Vec<Vec<usize>>,
    pack_number: usize,
    used: usize,
    locators: Vec<String>,
}

impl Builder {
    fn group(&mut self, records: &mut Vec<Vec<u8>>, indices: &mut Vec<usize>) -> Result<()> {
        if records.is_empty() {
            return Ok(());
        }
        let group = build_group(PackLane::Native, records, None)?;
        if !self.groups.is_empty()
            && !append_fits(PackLane::Native, self.used, self.groups.len(), &group)?
        {
            self.seal()?;
        }
        self.groups.push(group);
        self.members.push(std::mem::take(indices));
        self.used = assembled_length(PackLane::Native, &self.groups)?;
        records.clear();
        Ok(())
    }

    fn seal(&mut self) -> Result<()> {
        if self.groups.is_empty() {
            return Ok(());
        }
        let bytes = assemble_consuming(PackLane::Native, std::mem::take(&mut self.groups))?;
        assert!(bytes.len() <= PACK_LIMIT);
        let header = parse_header(&bytes)?;
        let filename = format!("pack-{:05}.bin", self.pack_number);
        fs::write(self.root.join(&filename), &bytes)?;
        for (group, indices) in self.members.drain(..).enumerate() {
            let view = group_view(&bytes, header, group)?;
            let body = &bytes[view.start..view.end];
            let count = u32::from_le_bytes(body[..4].try_into()?) as usize;
            assert_eq!(count, indices.len());
            for (record, index) in indices.into_iter().enumerate() {
                let (start, end) =
                    record_range(count, &body[4..4 + 4 * count], body.len(), record)?;
                self.locators.push(format!(
                    "{{\"index\":{index},\"pack\":\"{filename}\",\"group\":{group},\"record\":{record},\"start\":{},\"end\":{}}}",
                    view.start + start,
                    view.start + end
                ));
            }
        }
        self.pack_number += 1;
        self.used = 0;
        Ok(())
    }
}

fn build(input: &Path, root: &Path, width: usize) -> Result<()> {
    fs::create_dir(root)?;
    let mut source = BufReader::new(File::open(input)?);
    let size = usize::try_from(source.get_ref().metadata()?.len())?;
    assert!(matches!(width, 8192 | 16384 | 32768) && size % width == 0);
    let capacities = StorageCapacities::from_policy(StoragePolicy::default())?;
    let mut workspace = CompressionWorkspace::new()?;
    let mut profile = SaveProfile::default();
    let mut builder = Builder {
        root: root.to_path_buf(),
        groups: Vec::new(),
        members: Vec::new(),
        pack_number: 0,
        used: 0,
        locators: Vec::new(),
    };
    let mut records = Vec::new();
    let mut indices = Vec::new();
    let mut identities = Vec::new();
    let mut payload_total = 0;
    let mut stored = 0;
    let mut raw = vec![0; width];
    let started = Instant::now();
    for index in 0..size / width {
        source.read_exact(&mut raw)?;
        let canonical = encode_chunk_object(&raw)?;
        identities.push(ObjectId::for_bytes(&canonical).to_string());
        let encoded = encode_full(
            &canonical,
            ObjectRole::Chunk,
            &capacities,
            &mut workspace,
            &mut profile,
        )?;
        stored += usize::from(encoded.is_stored());
        fs::write(root.join(format!("record-{index:05}.bin")), &encoded.record)?;
        let prospective = framed_group_length(records.len() + 1, payload_total + encoded.width())?;
        if !records.is_empty() && prospective > GROUP_TARGET {
            builder.group(&mut records, &mut indices)?;
            payload_total = 0;
        }
        payload_total += encoded.width();
        records.push(encoded.record);
        indices.push(index);
    }
    builder.group(&mut records, &mut indices)?;
    builder.seal()?;
    let construction_ns = started.elapsed().as_nanos();
    let mut manifest = File::create(root.join("manifest.json"))?;
    writeln!(
        manifest,
        "{{\"chunk_bytes\":{width},\"logical_bytes\":{size},\"records\":{},\"packs\":{},\"stored_records\":{stored},\"construction_ns\":{construction_ns},\"locators\":[{}],\"cas_ids\":[{}]}}",
        size / width,
        builder.pack_number,
        builder.locators.join(","),
        identities
            .iter()
            .map(|id| format!("\"{id}\""))
            .collect::<Vec<_>>()
            .join(",")
    )?;
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 4 {
        return Err("usage: minio_pack_probe INPUT OUTPUT CHUNK_BYTES".into());
    }
    build(Path::new(&args[1]), Path::new(&args[2]), args[3].parse()?)
}
