use crate::cow_tree::{Data, FileData, Node, NodeId, Workspace};
use layerfs_content::file::rope::read_range;
use layerfs_storage::{CoreReader, Result, StorageError};
use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::os::unix::fs::FileExt;
use std::path::Path;

pub struct ReadPlan {
    reader: layerfs_branch_store::SnapshotReader,
    offset: u64,
    end: u64,
    source: ReadSource,
}

enum ReadSource {
    Base(layerfs_content::file::rope::FileStateRoot),
    Overlay {
        base: Option<(layerfs_content::file::rope::FileStateRoot, u64)>,
        spool: std::path::PathBuf,
        dirty: Vec<(u64, u64)>,
        charged: Vec<(u64, u64)>,
    },
}

impl ReadPlan {
    pub fn read(self) -> Result<Vec<u8>> {
        if self.offset >= self.end {
            return Ok(Vec::new());
        }
        match self.source {
            ReadSource::Base(root) => read_base(
                &self.reader,
                root,
                self.end,
                self.offset,
                (self.end - self.offset) as usize,
            ),
            ReadSource::Overlay {
                base,
                spool,
                dirty,
                charged,
            } => {
                let mut output = vec![0; (self.end - self.offset) as usize];
                if let Some((root, base_len)) = base {
                    let base_end = self.end.min(base_len);
                    if self.offset < base_end {
                        let bytes = read_base(
                            &self.reader,
                            root,
                            base_len,
                            self.offset,
                            (base_end - self.offset) as usize,
                        )?;
                        output[..bytes.len()].copy_from_slice(&bytes);
                    }
                }
                for (start, end) in dirty {
                    output[(start - self.offset) as usize..(end - self.offset) as usize].fill(0);
                }
                let file = File::open(spool)?;
                for (start, end) in charged {
                    read_exact_at(
                        &file,
                        &mut output[(start - self.offset) as usize..(end - self.offset) as usize],
                        start,
                    )?;
                }
                Ok(output)
            }
        }
    }
}

impl Workspace {
    pub fn read(&self, node: NodeId, offset: u64, size: usize) -> Result<Vec<u8>> {
        self.read_plan(node, offset, size)?.read()
    }

    pub fn read_plan(&self, node: NodeId, offset: u64, size: usize) -> Result<ReadPlan> {
        let source = match &self
            .nodes
            .get(&node)
            .ok_or(StorageError::NotFound("node"))?
            .data
        {
            Data::File(FileData::Base { root, .. }) => ReadSource::Base(*root),
            Data::File(FileData::Overlay {
                base,
                spool,
                len,
                dirty,
                charged,
                ..
            }) => {
                let end = (*len).min(offset.saturating_add(size as u64));
                let dirty = dirty
                    .range(..end)
                    .filter_map(|(&start, &range_end)| {
                        let start = start.max(offset);
                        let range_end = range_end.min(end);
                        (start < range_end).then_some((start, range_end))
                    })
                    .collect();
                let charged = charged
                    .range(..end)
                    .filter_map(|(&start, &range_end)| {
                        let start = start.max(offset);
                        let range_end = range_end.min(end);
                        (start < range_end).then_some((start, range_end))
                    })
                    .collect();
                return Ok(ReadPlan {
                    reader: self.reader.clone(),
                    offset,
                    end,
                    source: ReadSource::Overlay {
                        base: *base,
                        spool: spool.clone(),
                        dirty,
                        charged,
                    },
                });
            }
            _ => return Err(StorageError::InvalidInput("read")),
        };
        Ok(ReadPlan {
            reader: self.reader.clone(),
            offset,
            end: self
                .attr(node)?
                .size
                .min(offset.saturating_add(size as u64)),
            source,
        })
    }

    pub fn write(&mut self, node: NodeId, offset: u64, bytes: &[u8]) -> Result<usize> {
        self.write_inner(node, offset, bytes.len(), Some(bytes))
    }

    pub(crate) fn write_zero(&mut self, node: NodeId, offset: u64, len: usize) -> Result<usize> {
        self.write_inner(node, offset, len, None)
    }

    fn write_inner(
        &mut self,
        node: NodeId,
        offset: u64,
        byte_len: usize,
        bytes: Option<&[u8]>,
    ) -> Result<usize> {
        self.ensure_active()?;
        let old_len = self.attr(node)?.size;
        let end = offset
            .checked_add(byte_len as u64)
            .ok_or(StorageError::InvalidInput("write length"))?;
        if byte_len == 0 {
            return Ok(0);
        }
        self.ensure_overlay(node)?;
        let old_charge = self.file_charge(node)?;
        let (path, mut next_dirty, mut next_charged) = match &self.nodes[&node].data {
            Data::File(FileData::Overlay {
                spool,
                dirty,
                charged,
                ..
            }) => (spool.clone(), dirty.clone(), charged.clone()),
            _ => unreachable!(),
        };
        if offset > old_len {
            insert_range(&mut next_dirty, old_len, offset);
        }
        insert_range(&mut next_dirty, offset, end);
        let materialize = bytes.is_some() || overlaps(&next_charged, offset, end);
        if materialize {
            insert_range(&mut next_charged, offset, end);
        }
        let new_charge = range_bytes(&next_charged);
        self.policy.check(
            self.spool_bytes
                .saturating_sub(old_charge)
                .saturating_add(new_charge),
        )?;
        match bytes {
            Some(bytes) => OpenOptions::new()
                .write(true)
                .open(&path)?
                .write_all_at(bytes, offset)?,
            None if materialize => OpenOptions::new()
                .write(true)
                .open(&path)?
                .write_all_at(&vec![0; byte_len], offset)?,
            None => {}
        }
        let Data::File(FileData::Overlay {
            len,
            dirty,
            charged,
            ..
        }) = &mut self.nodes.get_mut(&node).unwrap().data
        else {
            unreachable!()
        };
        *dirty = next_dirty;
        *charged = next_charged;
        *len = old_len.max(end);
        self.spool_bytes = self
            .spool_bytes
            .saturating_sub(old_charge)
            .saturating_add(new_charge);
        self.dirty.insert(node);
        let paths = self.nodes[&node].paths.iter().cloned().collect::<Vec<_>>();
        self.note_mutation(paths)?;
        Ok(byte_len)
    }

    pub fn truncate(&mut self, node: NodeId, size: u64) -> Result<()> {
        self.ensure_active()?;
        let old_len = self.attr(node)?.size;
        if size == old_len {
            return Ok(());
        }
        self.ensure_overlay(node)?;
        let old_charge = self.file_charge(node)?;
        let (path, mut next_dirty, mut next_charged) = match &self.nodes[&node].data {
            Data::File(FileData::Overlay {
                spool,
                dirty,
                charged,
                ..
            }) => (spool.clone(), dirty.clone(), charged.clone()),
            _ => unreachable!(),
        };
        if size > old_len {
            insert_range(&mut next_dirty, old_len, size);
        } else {
            truncate_ranges(&mut next_dirty, size);
            truncate_ranges(&mut next_charged, size);
        }
        let new_charge = range_bytes(&next_charged);
        self.policy.check(
            self.spool_bytes
                .saturating_sub(old_charge)
                .saturating_add(new_charge),
        )?;
        OpenOptions::new().write(true).open(path)?.set_len(size)?;
        let Data::File(FileData::Overlay {
            len,
            dirty,
            charged,
            ..
        }) = &mut self.nodes.get_mut(&node).unwrap().data
        else {
            unreachable!()
        };
        *dirty = next_dirty;
        *charged = next_charged;
        *len = size;
        self.spool_bytes = self
            .spool_bytes
            .saturating_sub(old_charge)
            .saturating_add(new_charge);
        self.dirty.insert(node);
        let paths = self.nodes[&node].paths.iter().cloned().collect::<Vec<_>>();
        self.note_mutation(paths)?;
        Ok(())
    }

    pub fn fsync(&self, node: Option<NodeId>) -> Result<()> {
        if let Some(node) = node {
            if let Data::File(FileData::Overlay { spool, .. }) = &self.nodes[&node].data {
                OpenOptions::new().write(true).open(spool)?.sync_all()?;
            }
            return Ok(());
        }
        for value in self.nodes.values() {
            if let Data::File(FileData::Overlay { spool, .. }) = &value.data {
                OpenOptions::new().write(true).open(spool)?.sync_all()?;
            }
        }
        Ok(())
    }

    pub(crate) fn clear_spool(&mut self) -> Result<()> {
        for value in self.nodes.values() {
            if let Data::File(FileData::Overlay { spool, .. }) = &value.data {
                if spool.exists() {
                    std::fs::remove_file(spool)?;
                }
            }
        }
        self.spool_bytes = 0;
        Ok(())
    }

    pub(crate) fn new_spool_node(&mut self, mode: u32, path: String) -> Result<NodeId> {
        let node = NodeId(self.next_node);
        let spool = self.spool.join(node.0.to_string());
        File::create(&spool)?;
        Ok(self.allocate(Node {
            canonical: None,
            paths: [path].into(),
            mode,
            links: 1,
            pins: 0,
            mtime_seconds: 0,
            mtime_nanoseconds: 0,
            data: Data::File(FileData::Overlay {
                base: None,
                spool,
                len: 0,
                dirty: BTreeMap::new(),
                charged: BTreeMap::new(),
            }),
        }))
    }

    pub(crate) fn new_spool_node_reserved(
        &mut self,
        node: NodeId,
        mode: u32,
        path: String,
    ) -> Result<()> {
        let spool = self.spool.join(node.0.to_string());
        File::create(&spool)?;
        if self
            .nodes
            .insert(
                node,
                Node {
                    canonical: None,
                    paths: [path].into(),
                    mode,
                    links: 1,
                    pins: 0,
                    mtime_seconds: 0,
                    mtime_nanoseconds: 0,
                    data: Data::File(FileData::Overlay {
                        base: None,
                        spool,
                        len: 0,
                        dirty: BTreeMap::new(),
                        charged: BTreeMap::new(),
                    }),
                },
            )
            .is_some()
        {
            return Err(StorageError::Integrity("reserved node"));
        }
        Ok(())
    }

    fn ensure_overlay(&mut self, node: NodeId) -> Result<&Path> {
        if let Data::File(FileData::Base { root, len }) = self.nodes[&node].data {
            let path = self.spool.join(node.0.to_string());
            let file = File::create(&path)?;
            file.set_len(len)?;
            self.nodes.get_mut(&node).unwrap().data = Data::File(FileData::Overlay {
                base: Some((root, len)),
                spool: path,
                len,
                dirty: BTreeMap::new(),
                charged: BTreeMap::new(),
            });
        }
        match &self.nodes[&node].data {
            Data::File(FileData::Overlay { spool, .. }) => Ok(spool),
            _ => Err(StorageError::InvalidInput("file")),
        }
    }

    fn file_charge(&self, node: NodeId) -> Result<u64> {
        Ok(
            match &self
                .nodes
                .get(&node)
                .ok_or(StorageError::NotFound("node"))?
                .data
            {
                Data::File(FileData::Overlay { charged, .. }) => range_bytes(charged),
                _ => 0,
            },
        )
    }
}

fn read_base(
    reader: &layerfs_branch_store::SnapshotReader,
    root: layerfs_content::file::rope::FileStateRoot,
    len: u64,
    offset: u64,
    size: usize,
) -> Result<Vec<u8>> {
    let end = len.min(offset.saturating_add(size as u64));
    if offset >= end {
        return Ok(Vec::new());
    }
    let mut bytes = Vec::with_capacity((end - offset) as usize);
    read_range(&CoreReader(reader), root, offset..end, &mut bytes)?;
    Ok(bytes)
}

fn read_exact_at(file: &File, mut output: &mut [u8], mut offset: u64) -> Result<()> {
    while !output.is_empty() {
        let read = file.read_at(output, offset)?;
        if read == 0 {
            return Err(StorageError::Integrity("spool eof"));
        }
        offset += read as u64;
        output = &mut output[read..];
    }
    Ok(())
}

fn insert_range(ranges: &mut BTreeMap<u64, u64>, mut start: u64, mut end: u64) {
    if start >= end {
        return;
    }
    if let Some((&prior_start, &prior_end)) = ranges.range(..=start).next_back() {
        if prior_end >= start {
            start = prior_start;
            end = end.max(prior_end);
            ranges.remove(&prior_start);
        }
    }
    let following = ranges
        .range(start..=end)
        .map(|(&range_start, &range_end)| (range_start, range_end))
        .collect::<Vec<_>>();
    for (range_start, range_end) in following {
        end = end.max(range_end);
        ranges.remove(&range_start);
    }
    ranges.insert(start, end);
}

fn overlaps(ranges: &BTreeMap<u64, u64>, start: u64, end: u64) -> bool {
    ranges
        .range(..end)
        .next_back()
        .is_some_and(|(_, range_end)| *range_end > start)
}

fn truncate_ranges(ranges: &mut BTreeMap<u64, u64>, size: u64) {
    let old = std::mem::take(ranges);
    for (start, end) in old {
        insert_range(ranges, start.min(size), end.min(size));
    }
}

fn range_bytes(ranges: &BTreeMap<u64, u64>) -> u64 {
    ranges.iter().map(|(start, end)| end - start).sum()
}
