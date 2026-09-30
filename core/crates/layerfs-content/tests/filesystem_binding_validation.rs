//! Scalar validation/topology contracts; fixture and graph populations are separate.

mod support;

use std::cell::Cell;
use std::collections::BTreeMap;

use layerfs_content::filesystem::rows::{
    BindingLookup, BindingRowSource, BindingRows, DirectoryCompletion, DirectoryHeader,
    DirectoryHeaderSource, DirectoryRowSource, InodeRowSource, PreparedRows, RowSource,
    SerialRowSource, SliceBindingRows,
};
use layerfs_content::filesystem::validate::{check_bindings, ValidationWork};
use layerfs_content::filesystem::{
    scope_for_seed, DirectoryUpdate, FilesystemInput, FilesystemResources, FilesystemRootId,
    InodeScope, InodeUpdate, PathName,
};
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::{ContentError, ContentResult};
use support::filesystem::{resources, synthetic, value, CountingProvider, Session, TreeStore};

#[derive(Default)]
struct Work {
    whole_rows: Cell<u64>,
    opened: Cell<u64>,
    finish_attempts: Cell<u64>,
    live: Cell<u64>,
    peak: Cell<u64>,
    present: Cell<u64>,
    absent: Cell<u64>,
    unmentioned: Cell<u64>,
}

/// This legitimate bounded source refuses its explicitly unsupported legacy API.
/// It delegates actual names/completion to the immutable borrowed slice source.
struct ScalarOnly<'a> {
    input: SliceBindingRows<'a>,
    work: Work,
    refuse_completion: Option<u64>,
}

impl<'a> ScalarOnly<'a> {
    fn new(input: &'a FilesystemInput<'a>) -> Self {
        Self {
            input: SliceBindingRows::new(input).unwrap(),
            work: Work::default(),
            refuse_completion: None,
        }
    }

    fn no_whole_rows(&self) {
        assert_eq!(self.work.whole_rows.get(), 0);
        assert_eq!(self.work.live.get(), 0);
        assert!(
            self.work.peak.get() <= 2,
            "one selected pass plus one effective-directory pass"
        );
    }
}

impl RowSource for ScalarOnly<'_> {
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
        self.work.whole_rows.set(self.work.whole_rows.get() + 1);
        Err(ContentError::UnsupportedPolicy {
            field: "whole directory rows",
        })
    }
    fn directory_for(&self, _parent: u64) -> ContentResult<Option<DirectoryUpdate>> {
        self.work.whole_rows.set(self.work.whole_rows.get() + 1);
        Err(ContentError::UnsupportedPolicy {
            field: "whole directory lookup",
        })
    }
    fn inodes(&self) -> ContentResult<Box<dyn InodeRowSource + '_>> {
        self.input.inodes()
    }
    fn new_inodes(&self) -> ContentResult<Box<dyn SerialRowSource + '_>> {
        self.input.new_inodes()
    }
    fn value_for(&self, serial: u64) -> ContentResult<Option<InodeValue>> {
        self.input.value_for(serial)
    }
    fn new_position(&self, serial: u64) -> ContentResult<Option<usize>> {
        self.input.new_position(serial)
    }
}

impl PreparedRows for ScalarOnly<'_> {
    fn base(&self) -> Option<FilesystemRootId> {
        self.input.base()
    }
    fn scope(&self) -> InodeScope {
        self.input.scope()
    }
    fn root_serial(&self) -> u64 {
        self.input.root_serial()
    }
    fn resources(&self) -> FilesystemResources {
        self.input.resources()
    }
}

impl BindingRows for ScalarOnly<'_> {
    fn directory_headers(&self) -> ContentResult<Box<dyn DirectoryHeaderSource + '_>> {
        self.input.directory_headers()
    }
    fn directory_header(&self, parent: u64) -> ContentResult<Option<DirectoryHeader>> {
        self.input.directory_header(parent)
    }
    fn bindings(&self, header: &DirectoryHeader) -> ContentResult<Box<dyn BindingRowSource + '_>> {
        let input = self.input.bindings(header)?;
        let opened = self.work.opened.get() + 1;
        self.work.opened.set(opened);
        self.work.live.set(self.work.live.get() + 1);
        self.work
            .peak
            .set(self.work.peak.get().max(self.work.live.get()));
        Ok(Box::new(CountedCursor {
            input,
            work: &self.work,
            refuse_completion: self.refuse_completion == Some(opened),
        }))
    }
    fn binding_for(&self, parent: u64, name: &[u8]) -> ContentResult<BindingLookup> {
        let result = self.input.binding_for(parent, name)?;
        let count = match result {
            BindingLookup::Present(_) => &self.work.present,
            BindingLookup::Absent => &self.work.absent,
            BindingLookup::Unmentioned => &self.work.unmentioned,
        };
        count.set(count.get() + 1);
        Ok(result)
    }
}

struct CountedCursor<'a> {
    input: Box<dyn BindingRowSource + 'a>,
    work: &'a Work,
    refuse_completion: bool,
}

impl BindingRowSource for CountedCursor<'_> {
    fn next_binding(&mut self) -> ContentResult<Option<(PathName, Option<u64>)>> {
        self.input.next_binding()
    }
    fn finish(&mut self) -> ContentResult<DirectoryCompletion> {
        self.work
            .finish_attempts
            .set(self.work.finish_attempts.get() + 1);
        if self.refuse_completion {
            return Err(ContentError::ProviderFailure {
                what: "selected binding completion",
            });
        }
        self.input.finish()
    }
}

impl Drop for CountedCursor<'_> {
    fn drop(&mut self) {
        self.work.live.set(self.work.live.get() - 1);
    }
}

fn name(value: &str) -> PathName {
    PathName::new(value).unwrap()
}

fn directory() -> InodeValue {
    value(
        InodeKind::Directory,
        synthetic("binding-validation/empty"),
        synthetic("binding-validation/meta"),
    )
}

fn regular() -> InodeValue {
    value(
        InodeKind::RegularFile,
        synthetic("binding-validation/file"),
        synthetic("binding-validation/meta"),
    )
}

fn nested() -> (Session, u64, u64) {
    let mut session = Session::new(1).unwrap();
    let d = session.allocate();
    let e = session.allocate();
    session
        .apply(
            &[
                DirectoryUpdate {
                    parent: 1,
                    changes: vec![(name("d"), Some(d))],
                },
                DirectoryUpdate {
                    parent: d,
                    changes: vec![(name("e"), Some(e))],
                },
                DirectoryUpdate {
                    parent: e,
                    changes: Vec::new(),
                },
            ],
            &[
                InodeUpdate {
                    serial: d,
                    value: directory(),
                },
                InodeUpdate {
                    serial: e,
                    value: directory(),
                },
            ],
            &[d, e],
        )
        .unwrap();
    (session, d, e)
}

#[test]
fn a_full_name_star_validates_without_whole_directory_rows() {
    let width = 129u64;
    let directories = [DirectoryUpdate {
        parent: 1,
        changes: (0..width)
            .map(|index| {
                (
                    name(&format!("n{index:04}{}", "x".repeat(250))),
                    Some(index + 2),
                )
            })
            .collect(),
    }];
    let mut inodes = vec![InodeUpdate {
        serial: 1,
        value: directory(),
    }];
    inodes.extend((2..width + 2).map(|serial| InodeUpdate {
        serial,
        value: regular(),
    }));
    let fresh: Vec<_> = (1..width + 2).collect();
    let input = FilesystemInput {
        base: None,
        scope: scope_for_seed([0x35; 32]),
        root_serial: 1,
        directories: &directories,
        inodes: &inodes,
        new_inodes: &fresh,
        resources: resources(),
    };
    let source = ScalarOnly::new(&input);
    let fixture = TreeStore::new();
    let reader = CountingProvider::new(&fixture);
    let mut work = ValidationWork::default();
    let checked = check_bindings(&reader, &source, &BTreeMap::new(), &mut work).unwrap();
    assert_eq!(checked.additions.len() as u64, width);
    assert_eq!(
        checked.additions.keys().copied().collect::<Vec<_>>(),
        (2..width + 2).collect::<Vec<_>>()
    );
    assert!(checked.additions.values().all(|count| *count == 0));
    assert_eq!(work.entries_examined, width);
    assert_eq!(reader.demands(), 0);
    assert_eq!(source.work.opened.get(), source.work.finish_attempts.get());
    source.no_whole_rows();
}

#[test]
fn an_unmentioned_base_binding_keeps_its_parent_and_is_not_a_tombstone() {
    let (session, d, e) = nested();
    let directories = [DirectoryUpdate {
        parent: d,
        changes: vec![(name("fresh"), Some(e))],
    }];
    let input = FilesystemInput {
        base: Some(FilesystemRootId(session.root)),
        scope: session.scope,
        root_serial: 1,
        directories: &directories,
        inodes: &[],
        new_inodes: &[],
        resources: resources(),
    };
    let source = ScalarOnly::new(&input);
    let result = check_bindings(
        &session.store,
        &source,
        &BTreeMap::new(),
        &mut ValidationWork::default(),
    );
    assert!(matches!(
        result,
        Err(ContentError::InvalidRecord("multiple parents"))
    ));
    assert!(source.work.unmentioned.get() > 0);
    source.no_whole_rows();
}

#[test]
fn an_exact_tombstone_allows_the_same_directory_to_move() {
    let (session, d, e) = nested();
    let directories = [DirectoryUpdate {
        parent: d,
        changes: vec![(name("e"), None), (name("fresh"), Some(e))],
    }];
    let input = FilesystemInput {
        base: Some(FilesystemRootId(session.root)),
        scope: session.scope,
        root_serial: 1,
        directories: &directories,
        inodes: &[],
        new_inodes: &[],
        resources: resources(),
    };
    let source = ScalarOnly::new(&input);
    let checked = check_bindings(
        &session.store,
        &source,
        &BTreeMap::new(),
        &mut ValidationWork::default(),
    )
    .unwrap();
    assert_eq!(checked.additions.get(&e), Some(&1));
    assert!(source.work.absent.get() > 0);
    assert_eq!(source.work.opened.get(), source.work.finish_attempts.get());
    source.no_whole_rows();
}

fn wide_nested() -> (Session, u64, u64, u64, PathName) {
    let mut session = Session::new(1).unwrap();
    let d = session.allocate();
    let e = session.allocate();
    let files: Vec<_> = (0..129).map(|_| session.allocate()).collect();
    let last = name(&format!("z{}", "x".repeat(254)));
    let mut children: Vec<_> = files
        .iter()
        .enumerate()
        .map(|(index, &serial)| {
            (
                name(&format!("n{index:04}{}", "x".repeat(250))),
                Some(serial),
            )
        })
        .collect();
    children.push((last.clone(), Some(e)));
    let mut values = vec![
        InodeUpdate {
            serial: d,
            value: directory(),
        },
        InodeUpdate {
            serial: e,
            value: directory(),
        },
    ];
    values.extend(files.iter().map(|&serial| InodeUpdate {
        serial,
        value: regular(),
    }));
    let mut fresh = vec![d, e];
    fresh.extend(files.iter().copied());
    session
        .apply(
            &[
                DirectoryUpdate {
                    parent: 1,
                    changes: vec![(name("d"), Some(d))],
                },
                DirectoryUpdate {
                    parent: d,
                    changes: children,
                },
                DirectoryUpdate {
                    parent: e,
                    changes: Vec::new(),
                },
            ],
            &values,
            &fresh,
        )
        .unwrap();
    (session, d, e, files[0], last)
}

#[test]
fn a_retained_last_page_edge_still_closes_the_effective_cycle() {
    let (session, d, e, file, _) = wide_nested();
    let directories = [
        DirectoryUpdate {
            parent: 1,
            changes: vec![(name("d"), None)],
        },
        DirectoryUpdate {
            parent: d,
            changes: vec![
                (name(&format!("n0000{}", "x".repeat(250))), None),
                (name("n1000"), Some(file)),
            ],
        },
        DirectoryUpdate {
            parent: e,
            changes: vec![(name("moved"), Some(d))],
        },
    ];
    let input = FilesystemInput {
        base: Some(FilesystemRootId(session.root)),
        scope: session.scope,
        root_serial: 1,
        directories: &directories,
        inodes: &[],
        new_inodes: &[],
        resources: resources(),
    };
    let source = ScalarOnly::new(&input);
    let mut work = ValidationWork::default();
    let result = check_bindings(&session.store, &source, &BTreeMap::new(), &mut work);
    assert!(matches!(
        result,
        Err(ContentError::InvalidRecord("effective tree cycle"))
    ));
    assert!(work.directory_pages_read >= 3);
    assert_eq!(source.work.peak.get(), 2);
    source.no_whole_rows();
}

#[test]
fn removing_the_back_edge_allows_a_wide_subtree_to_change_parent() {
    let (session, d, e, _, last) = wide_nested();
    let directories = [
        DirectoryUpdate {
            parent: 1,
            changes: vec![(name("d"), None), (name("e"), Some(e))],
        },
        DirectoryUpdate {
            parent: d,
            changes: vec![(last, None)],
        },
        DirectoryUpdate {
            parent: e,
            changes: vec![(name("moved"), Some(d))],
        },
    ];
    let input = FilesystemInput {
        base: Some(FilesystemRootId(session.root)),
        scope: session.scope,
        root_serial: 1,
        directories: &directories,
        inodes: &[],
        new_inodes: &[],
        resources: resources(),
    };
    let source = ScalarOnly::new(&input);
    let checked = check_bindings(
        &session.store,
        &source,
        &BTreeMap::new(),
        &mut ValidationWork::default(),
    )
    .unwrap();
    assert_eq!(checked.additions.get(&d), Some(&1));
    assert_eq!(checked.additions.get(&e), Some(&1));
    assert_eq!(source.work.opened.get(), source.work.finish_attempts.get());
    source.no_whole_rows();
}

#[test]
fn a_disconnected_build_cycle_is_refused_without_collecting_its_rows() {
    let directories = [
        DirectoryUpdate {
            parent: 1,
            changes: Vec::new(),
        },
        DirectoryUpdate {
            parent: 2,
            changes: vec![(name("b"), Some(3))],
        },
        DirectoryUpdate {
            parent: 3,
            changes: vec![(name("a"), Some(2))],
        },
    ];
    let inodes: Vec<_> = (1..=3)
        .map(|serial| InodeUpdate {
            serial,
            value: directory(),
        })
        .collect();
    let input = FilesystemInput {
        base: None,
        scope: scope_for_seed([0x37; 32]),
        root_serial: 1,
        directories: &directories,
        inodes: &inodes,
        new_inodes: &[1, 2, 3],
        resources: resources(),
    };
    let source = ScalarOnly::new(&input);
    let result = check_bindings(
        &TreeStore::new(),
        &source,
        &BTreeMap::new(),
        &mut ValidationWork::default(),
    );
    assert!(matches!(
        result,
        Err(ContentError::InvalidRecord("effective tree cycle"))
    ));
    source.no_whole_rows();
}

#[test]
fn the_selected_completion_refusal_is_preserved_before_a_result() {
    let directories = [DirectoryUpdate {
        parent: 1,
        changes: vec![(name("f"), Some(2))],
    }];
    let inodes = [
        InodeUpdate {
            serial: 1,
            value: directory(),
        },
        InodeUpdate {
            serial: 2,
            value: regular(),
        },
    ];
    let input = FilesystemInput {
        base: None,
        scope: scope_for_seed([0x39; 32]),
        root_serial: 1,
        directories: &directories,
        inodes: &inodes,
        new_inodes: &[1, 2],
        resources: resources(),
    };
    let mut source = ScalarOnly::new(&input);
    source.refuse_completion = Some(2);
    let fixture = TreeStore::new();
    let reader = CountingProvider::new(&fixture);
    let result = check_bindings(
        &reader,
        &source,
        &BTreeMap::new(),
        &mut ValidationWork::default(),
    );
    assert!(matches!(
        result,
        Err(ContentError::ProviderFailure {
            what: "selected binding completion"
        })
    ));
    assert_eq!(source.work.opened.get(), 2);
    assert_eq!(reader.demands(), 0);
    source.no_whole_rows();
}

#[test]
fn each_changed_existing_directory_is_followed_once_in_a_permutation() {
    let mut session = Session::new(1).unwrap();
    let serials: Vec<_> = (0..129).map(|_| session.allocate()).collect();
    let mut base = vec![DirectoryUpdate {
        parent: 1,
        changes: serials
            .iter()
            .enumerate()
            .map(|(index, &serial)| (name(&format!("d{index:04}")), Some(serial)))
            .collect(),
    }];
    base.extend(serials.iter().map(|&parent| DirectoryUpdate {
        parent,
        changes: Vec::new(),
    }));
    let values: Vec<_> = serials
        .iter()
        .map(|&serial| InodeUpdate {
            serial,
            value: directory(),
        })
        .collect();
    session.apply(&base, &values, &serials).unwrap();
    let rotated = [DirectoryUpdate {
        parent: 1,
        changes: (0..serials.len())
            .map(|index| {
                (
                    name(&format!("d{index:04}")),
                    Some(serials[(index + 1) % serials.len()]),
                )
            })
            .collect(),
    }];
    let input = FilesystemInput {
        base: Some(FilesystemRootId(session.root)),
        scope: session.scope,
        root_serial: 1,
        directories: &rotated,
        inodes: &[],
        new_inodes: &[],
        resources: resources(),
    };
    let source = ScalarOnly::new(&input);
    let mut work = ValidationWork::default();
    let checked = check_bindings(&session.store, &source, &BTreeMap::new(), &mut work).unwrap();
    assert_eq!(checked.additions.len(), serials.len());
    assert!(checked.additions.values().all(|count| *count == 1));
    assert_eq!(work.entries_examined, 2 * serials.len() as u64);
    assert_eq!(source.work.present.get(), 2 * serials.len() as u64);
    assert_eq!(source.work.opened.get(), source.work.finish_attempts.get());
    eprintln!("bounded-source count proof only: N={}, alias visits={}, changed-name probes={}; graph/physical/speed qualification separate", serials.len(), work.entries_examined, source.work.present.get());
    source.no_whole_rows();
}
