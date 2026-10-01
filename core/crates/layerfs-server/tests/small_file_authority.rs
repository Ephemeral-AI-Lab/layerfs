//! Actual Service/C5 existing-file route with independent v1 bytes and native0.
#![cfg(any(target_os = "macos", target_os = "linux"))]
// Shared helper also serves existing-file callers in other external tests.
#[allow(dead_code)]
#[path = "support/file_save.rs"]
mod file_save;
#[path = "support/prepared_binding.rs"]
mod fixture;
use fixture::{Fixture, ORIGINAL};
use layerfs_bridge::contract::*;
use std::io::{self, Cursor, Read};
fn native(f: &Fixture) -> usize {
    std::fs::read_dir(f.path())
        .unwrap()
        .filter(|entry| {
            entry
                .as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".lfcs-")
        })
        .count()
}
struct Body<'a> {
    fixture: &'a Fixture,
    input: Cursor<Vec<u8>>,
    reads: usize,
}
impl Read for Body<'_> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        assert_eq!(
            native(self.fixture),
            0,
            "typed Small admission before real body read"
        );
        self.reads += 1;
        self.input.read(out)
    }
}
fn prepared(f: &Fixture, workspace: [u8; 32], content: Root) -> (PreparedChanges, Vec<u8>) {
    let (mut header, _) = f.prepared(workspace, 1, &[], &[]);
    header.totals.identities = 1;
    let mut raw = vec![1, 1];
    raw.extend_from_slice(&f.file_serial.to_be_bytes());
    raw.extend_from_slice(&content);
    raw.extend_from_slice(&f.file_metadata);
    assert_eq!(header.stream_bytes().unwrap(), raw.len() as u64);
    (header, raw)
}
fn read(f: &Fixture, root: Root, n: usize) -> Vec<u8> {
    let request = Request {
        id: 901,
        generation: 1,
        store: 1,
        profile: 1,
        deadline_ms: 10000,
        response_bytes: MAX_FILE,
        operation: Operation::ReadFile {
            root,
            start: 0,
            end: n as u64,
        },
    };
    let mut bytes = Vec::new();
    f.service
        .handle(&f.peer, &request, &mut io::empty(), &mut bytes)
        .0
        .unwrap();
    bytes
}
#[test]
fn genuine_changed_and_unchanged_existing_file_updates_complete_normal_c5_without_native_files() {
    for (changed, budget) in [(false, 16 * 1024 * 1024), (true, 48 * 1024 * 1024)] {
        let f = Fixture::with_scratch("small-file-c5", budget);
        let bytes = b"separately saved replacement content";
        let content = if changed {
            let Response::Saved { root, .. } = f
                .call(
                    file_save::fresh(bytes.len() as u64),
                    &mut Cursor::new(file_save::fresh_body(bytes)),
                )
                .unwrap()
            else {
                panic!("saved")
            };
            root
        } else {
            f.file_content
        };
        assert_eq!(native(&f), 0);
        let (header, raw) = prepared(&f, [181 + u8::from(changed); 32], content);
        let mut body = Body {
            fixture: &f,
            input: Cursor::new(raw),
            reads: 0,
        };
        let stage = f.stage(header.clone(), &mut body).unwrap();
        assert!(body.reads >= 2);
        assert_eq!(stage.expected_root, f.root);
        assert_eq!(stage.construction_base_root, f.root);
        assert_eq!(stage.scope, f.scope);
        assert_eq!(stage.candidate_root == f.root, !changed);
        assert_eq!(native(&f), 0);
        assert_eq!(f.original_bytes(), ORIGINAL);
        let Response::Stat {
            serial,
            content: actual,
            metadata,
            ..
        } = f.stat(stage.candidate_root, b"a")
        else {
            panic!("stat")
        };
        assert_eq!(
            (serial, actual, metadata),
            (f.file_serial, content, f.file_metadata)
        );
        assert_eq!(
            f.list(stage.candidate_root, b""),
            vec![(b"a".to_vec(), f.file_serial)]
        );
        if changed {
            assert_eq!(read(&f, actual, bytes.len()), bytes);
        }
        assert!(f
            .call(
                Operation::HistoryCommand(HistoryCommand::CommitStaged {
                    workspace: header.workspace,
                    token: stage.token + 1
                }),
                &mut io::empty()
            )
            .is_err());
        assert_eq!(f.current_stage(header.workspace), stage);
        let Response::History(result) = f
            .call(
                Operation::HistoryCommand(HistoryCommand::CommitStaged {
                    workspace: header.workspace,
                    token: stage.token,
                }),
                &mut io::empty(),
            )
            .unwrap()
        else {
            panic!("history")
        };
        let HistoryResult::Committed(result) = *result else {
            panic!("committed")
        };
        match result {
            CommitOutcomeWire::Committed(commit) if changed => {
                assert_eq!(commit.root, stage.candidate_root);
                assert_eq!(commit.parent, None);
                assert_eq!(commit.base_layer, f.base_layer);
            }
            CommitOutcomeWire::UpToDate { head, root } if !changed => {
                assert_eq!((head, root), (None, f.root));
            }
            other => panic!("unexpected C5 {other:?}"),
        }
        assert_eq!(native(&f), 0);
    }
}
#[test]
fn wrong_body_kind_serial_count_and_eof_preserve_prior_stage_without_native_fallback() {
    let f = Fixture::new("small-file-refusal");
    let (header, raw) = prepared(&f, [183; 32], f.file_content);
    let prior = f
        .stage(header.clone(), &mut Cursor::new(raw.clone()))
        .unwrap();
    let mut wrongkind = raw.clone();
    wrongkind[1] = 3;
    let mut rootserial = raw.clone();
    rootserial[2..10].copy_from_slice(&f.root_serial.to_be_bytes());
    let mut absent = raw.clone();
    absent[2..10].copy_from_slice(&i64::MAX.to_be_bytes());
    let mut trailing = raw.clone();
    trailing.push(99);
    let short = raw[..raw.len() - 1].to_vec();
    for wire in [wrongkind, rootserial, absent, trailing, short] {
        let mut body = Body {
            fixture: &f,
            input: Cursor::new(wire),
            reads: 0,
        };
        assert!(f.stage(header.clone(), &mut body).is_err());
        assert_eq!(f.current_stage(header.workspace), prior);
        assert_eq!(native(&f), 0);
        assert_eq!(f.original_bytes(), ORIGINAL);
    }
}
