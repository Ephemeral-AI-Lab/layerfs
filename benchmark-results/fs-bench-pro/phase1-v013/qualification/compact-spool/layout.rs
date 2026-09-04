#![allow(dead_code,private_interfaces)]
mod before {
use std::collections::{BTreeMap,BTreeSet}; use std::path::PathBuf; use std::sync::Arc;
#[derive(Clone,Debug,Eq,PartialEq)] struct FileStateRoot([u8;32]);
#[derive(Clone,Debug,Eq,PartialEq)] struct DirectoryStateRoot([u8;32]);
#[derive(Clone,Debug,Eq,PartialEq)] struct InodeId([u8;32]);
#[derive(Clone,Debug,Eq,PartialEq)] struct NodeId(u64);
#[derive(Clone,Debug,Eq,PartialEq)] pub(crate) struct PieceTree {root:Option<Arc<()>>,serial:u64}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Data {
    File(FileData),
    Directory(DirectoryData),
    Symlink(Vec<u8>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum FileData {
    Base {
        root: FileStateRoot,
        len: u64,
    },
    Edited {
        base: Option<(FileStateRoot, u64)>,
        spool: PathBuf,
        spool_high_water: u64,
        pieces: PieceTree,
        edits: u32,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DirectoryData {
    pub base: Option<DirectoryStateRoot>,
    pub changes: BTreeMap<Vec<u8>, Option<NodeId>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Node {
    pub canonical: Option<InodeId>,
    pub paths: BTreeSet<String>,
    pub mode: u32,
    pub links: u32,
    pub pins: u32,
    pub mtime_seconds: i64,
    pub mtime_nanoseconds: u32,
    pub data: Data,
}

}
mod after {
use std::collections::{BTreeMap,BTreeSet}; use std::path::PathBuf; use std::sync::Arc;
#[derive(Clone,Debug,Eq,PartialEq)] struct FileStateRoot([u8;32]);
#[derive(Clone,Debug,Eq,PartialEq)] struct DirectoryStateRoot([u8;32]);
#[derive(Clone,Debug,Eq,PartialEq)] struct InodeId([u8;32]);
#[derive(Clone,Debug,Eq,PartialEq)] struct NodeId(u64);
#[derive(Clone,Debug,Eq,PartialEq)] pub(crate) struct PieceTree {root:Option<Arc<()>>,serial:u64,contiguous_spool_len:u64}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Data {
    File(FileData),
    Directory(DirectoryData),
    Symlink(Vec<u8>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum FileData {
    Base {
        root: FileStateRoot,
        len: u64,
    },
    Edited {
        base: Option<(FileStateRoot, u64)>,
        spool: PathBuf,
        spool_high_water: u64,
        pieces: PieceTree,
        edits: u32,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DirectoryData {
    pub base: Option<DirectoryStateRoot>,
    pub changes: BTreeMap<Vec<u8>, Option<NodeId>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Node {
    pub canonical: Option<InodeId>,
    pub paths: BTreeSet<String>,
    pub mode: u32,
    pub links: u32,
    pub pins: u32,
    pub mtime_seconds: i64,
    pub mtime_nanoseconds: u32,
    pub data: Data,
}

}
fn main(){
println!("before PieceTree size={} align={}",std::mem::size_of::<before::PieceTree>(),std::mem::align_of::<before::PieceTree>());
println!("before FileData size={} align={}",std::mem::size_of::<before::FileData>(),std::mem::align_of::<before::FileData>());
println!("before Data size={} align={}",std::mem::size_of::<before::Data>(),std::mem::align_of::<before::Data>());
println!("before Node size={} align={}",std::mem::size_of::<before::Node>(),std::mem::align_of::<before::Node>());
println!("after PieceTree size={} align={}",std::mem::size_of::<after::PieceTree>(),std::mem::align_of::<after::PieceTree>());
println!("after FileData size={} align={}",std::mem::size_of::<after::FileData>(),std::mem::align_of::<after::FileData>());
println!("after Data size={} align={}",std::mem::size_of::<after::Data>(),std::mem::align_of::<after::Data>());
println!("after Node size={} align={}",std::mem::size_of::<after::Node>(),std::mem::align_of::<after::Node>());
}
