//! External count diagnostic for operation-scoped effective-tree validation.

mod support;

use std::cell::Cell;
use std::collections::BTreeMap;

use layerfs_content::filesystem::rows::{PreparedUpdate, RowSource};
use layerfs_content::filesystem::validate::ValidationWork;
use layerfs_content::filesystem::{
    check, DirectoryRowSource, DirectoryUpdate, FilesystemInput, InodeRowSource, InodeUpdate,
    PathName, SerialRowSource,
};
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::ContentResult;
use support::filesystem::{resources, synthetic, value, Session};

struct CountedRows<'a> {
    input: &'a FilesystemInput<'a>,
    directory_lookups: Cell<usize>,
    value_lookups: Cell<usize>,
}

impl RowSource for CountedRows<'_> {
    fn directory_rows(&self) -> usize {
        self.input.directory_rows()
    }
    fn inode_rows(&self) -> usize {
        self.input.inode_rows()
    }
    fn new_rows(&self) -> usize {
        self.input.new_rows()
    }
    fn directories(&self) -> ContentResult<Box<dyn DirectoryRowSource + '_>> {
        self.input.directories()
    }
    fn inodes(&self) -> ContentResult<Box<dyn InodeRowSource + '_>> {
        self.input.inodes()
    }
    fn new_inodes(&self) -> ContentResult<Box<dyn SerialRowSource + '_>> {
        self.input.new_inodes()
    }
    fn directory_for(&self, parent: u64) -> ContentResult<Option<DirectoryUpdate>> {
        self.directory_lookups.set(self.directory_lookups.get() + 1);
        self.input.directory_for(parent)
    }
    fn value_for(&self, serial: u64) -> ContentResult<Option<InodeValue>> {
        self.value_lookups.set(self.value_lookups.get() + 1);
        self.input.value_for(serial)
    }
    fn new_position(&self, serial: u64) -> ContentResult<Option<usize>> {
        self.input.new_position(serial)
    }
}

fn chain(depth: usize) -> (Vec<DirectoryUpdate>, Vec<InodeUpdate>, Vec<u64>) {
    let mut directories = Vec::new();
    let mut inodes = Vec::new();
    let mut fresh = Vec::new();
    for parent in 1..=depth as u64 + 1 {
        let child = parent + 1;
        directories.push(DirectoryUpdate {
            parent,
            changes: vec![(PathName::new("next").unwrap(), Some(child))],
        });
        inodes.push(InodeUpdate {
            serial: child,
            value: value(
                if parent <= depth as u64 {
                    InodeKind::Directory
                } else {
                    InodeKind::RegularFile
                },
                synthetic("cycle-work/content"),
                synthetic("cycle-work/metadata"),
            ),
        });
        fresh.push(child);
    }
    (directories, inodes, fresh)
}

fn count_chain(depth: usize, inherited: bool) -> (usize, usize, ValidationWork) {
    let mut session = Session::new(1).unwrap();
    let (directories, inodes, fresh) = chain(depth);
    if inherited {
        session.apply(&directories, &inodes, &fresh).unwrap();
    }
    let input = FilesystemInput {
        base: Some(layerfs_content::filesystem::FilesystemRootId(session.root)),
        scope: session.scope,
        root_serial: 1,
        directories: &directories,
        inodes: if inherited { &[] } else { &inodes },
        new_inodes: if inherited { &[] } else { &fresh },
        resources: resources(),
    };
    let rows = CountedRows {
        input: &input,
        directory_lookups: Cell::new(0),
        value_lookups: Cell::new(0),
    };
    let prepared = PreparedUpdate {
        base: input.base,
        scope: input.scope,
        root_serial: 1,
        resources: input.resources,
        rows: &rows,
    };
    let mut work = ValidationWork::default();
    check(&session.store, &prepared, &BTreeMap::new(), &mut work).unwrap();
    (rows.directory_lookups.get(), rows.value_lookups.get(), work)
}

/// One invocation, four deterministic inputs. No latency or storage claim.
#[test]
#[ignore = "explicit count diagnostic; retain each arm's output"]
fn count_diagnostic() {
    for depth in [16, 270] {
        for inherited in [false, true] {
            let (directories, values, work) = count_chain(depth, inherited);
            println!("C1_CYCLE_COUNT depth={depth} inherited={inherited} directory_lookups={directories} value_lookups={values} inode_demands={} inode_pages={} directory_pages={} entries_examined={} sites={:?}", work.inode_demands, work.inode_pages_read, work.directory_pages_read, work.entries_examined, work.inode_pages_by_site);
        }
    }
}
