//! Real Service/C5 all-zero route, independent v1 wire and actual native effects.
#![cfg(any(target_os = "macos", target_os = "linux"))]
#[path = "support/prepared_binding.rs"]
mod fixture;
use fixture::{Fixture, ORIGINAL};
use layerfs_bridge::contract::*;
use std::{
    io::{self, Cursor, Read},
    path::{Path, PathBuf},
};
fn native(parent: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(parent)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(".lfcs-")
        })
        .collect()
}
struct Observed<'a> {
    parent: &'a Path,
    input: Cursor<Vec<u8>>,
    reads: usize,
    expect_native: bool,
}
impl Read for Observed<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let present = !native(self.parent).is_empty();
        assert_eq!(
            present, self.expect_native,
            "real native directory presence while actual body is consumed"
        );
        self.reads += 1;
        self.input.read(bytes)
    }
}
#[test]
fn all_zero_stage_uses_exact_wire_eof_and_normal_c5_root_without_native_scratch() {
    let fixture = Fixture::with_scratch("verified-empty", 48 * 1024 * 1024);
    assert!(native(fixture.path()).is_empty());
    let (header, wire) = fixture.prepared([161; 32], 1, &[], &[]);
    assert_eq!(header.totals, PreparedTotals::default());
    assert_eq!(wire, [1]);
    let mut body = Observed {
        parent: fixture.path(),
        input: Cursor::new(wire),
        reads: 0,
        expect_native: false,
    };
    let stage = fixture.stage(header.clone(), &mut body).unwrap();
    assert!(body.reads >= 2);
    assert_eq!(
        (
            stage.expected_root,
            stage.construction_base_root,
            stage.candidate_root,
            stage.scope
        ),
        (fixture.root, fixture.root, fixture.root, fixture.scope)
    );
    assert_eq!(fixture.current_stage(header.workspace), stage);
    assert!(native(fixture.path()).is_empty());
    assert_eq!(fixture.original_bytes(), ORIGINAL);
    assert_eq!(
        fixture.list(stage.candidate_root, b""),
        vec![(b"a".to_vec(), fixture.file_serial)]
    );
    assert!(fixture
        .call(
            Operation::HistoryCommand(HistoryCommand::CommitStaged {
                workspace: header.workspace,
                token: stage.token + 1,
            }),
            &mut io::empty(),
        )
        .is_err());
    assert_eq!(fixture.current_stage(header.workspace), stage);
    let Response::History(committed) = fixture
        .call(
            Operation::HistoryCommand(HistoryCommand::CommitStaged {
                workspace: header.workspace,
                token: stage.token,
            }),
            &mut io::empty(),
        )
        .unwrap()
    else {
        panic!("history commit")
    };
    assert_eq!(
        *committed,
        HistoryResult::Committed(CommitOutcomeWire::UpToDate {
            head: None,
            root: fixture.root,
        })
    );
    let Response::History(snapshot) = fixture
        .call(
            Operation::HistoryQuery(HistoryQuery::GetBranch {
                branch: fixture.branch,
            }),
            &mut io::empty(),
        )
        .unwrap()
    else {
        panic!("history branch")
    };
    let HistoryResult::BranchSnapshot(snapshot) = *snapshot else {
        panic!("branch snapshot")
    };
    assert_eq!(snapshot.branch.head_commit, None);
    assert_eq!(snapshot.branch.base_layer, fixture.base_layer);
    assert_eq!(snapshot.effective_root, fixture.root);
    assert!(native(fixture.path()).is_empty());
}
#[test]
fn lying_tag_trailing_data_and_scope_preserve_prior_stage_and_create_no_native_owner() {
    let fixture = Fixture::new("verified-empty-refusal");
    let (header, wire) = fixture.prepared([162; 32], 1, &[], &[]);
    let prior = fixture
        .stage(header.clone(), &mut Cursor::new(wire))
        .unwrap();
    for raw in [vec![2], vec![1, 7], Vec::new()] {
        let mut body = Observed {
            parent: fixture.path(),
            input: Cursor::new(raw),
            reads: 0,
            expect_native: false,
        };
        assert!(fixture.stage(header.clone(), &mut body).is_err());
        assert_eq!(fixture.current_stage(header.workspace), prior);
        assert!(native(fixture.path()).is_empty());
    }
    let mut wrong = header.clone();
    wrong.scope = [88; 32];
    assert!(fixture.stage(wrong, &mut Cursor::new([1])).is_err());
    assert_eq!(fixture.current_stage(header.workspace), prior);
    assert!(native(fixture.path()).is_empty());
}
#[test]
fn positive_identity_declaration_requires_its_own_verified_body_and_cannot_use_zero_eof() {
    let fixture = Fixture::new("verified-empty-positive-declaration");
    let (mut header, _) = fixture.prepared([163; 32], 1, &[], &[]);
    header.totals.identities = 1;
    // The separately selected Small authority requires an actual existing-file
    // row, table/kind proof and EOF. The all-zero wire cannot stand in for it.
    let mut body = Observed {
        parent: fixture.path(),
        input: Cursor::new(vec![1]),
        reads: 0,
        expect_native: false,
    };
    assert!(fixture.stage(header, &mut body).is_err());
    assert!(native(fixture.path()).is_empty());
    assert_eq!(fixture.original_bytes(), ORIGINAL);
}
