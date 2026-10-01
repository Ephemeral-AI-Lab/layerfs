//! Actual file dispatch zero-native final shapes and independent canonical identities.
#![cfg(any(target_os = "macos", target_os = "linux"))]
#[path = "support/file_save.rs"]
mod file_save;
#[path = "support/prepared_binding.rs"]
mod fixture;
use fixture::Fixture;
use layerfs_bridge::contract::*;
use std::io::{self, Cursor, Read};
fn native(f: &Fixture) -> usize {
    std::fs::read_dir(f.path())
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".lfcs-")
        })
        .count()
}
struct Body<'a> {
    f: &'a Fixture,
    raw: Cursor<Vec<u8>>,
    expect_native: bool,
    reads: usize,
}
impl Read for Body<'_> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        assert_eq!(
            native(self.f) > 0,
            self.expect_native,
            "actual authority selection precedes input consumption"
        );
        self.reads += 1;
        self.raw.read(out)
    }
}
fn saved(r: Response) -> Root {
    let Response::Saved { root, .. } = r else {
        panic!("saved")
    };
    root
}
fn fresh(f: &Fixture, bytes: &[u8]) -> Root {
    saved(
        f.call(
            file_save::fresh(bytes.len() as u64),
            &mut Cursor::new(file_save::fresh_body(bytes)),
        )
        .unwrap(),
    )
}
fn expected(bytes: &[u8]) -> Root {
    let mut value = b"LFS5SML\0".to_vec();
    value.extend_from_slice(&1u16.to_be_bytes());
    value.extend_from_slice(bytes);
    *layerfs_content::ObjectId::for_bytes(
        &layerfs_content::object::encode_bytes_object(&value).unwrap(),
    )
    .as_bytes()
}
fn read(f: &Fixture, root: Root, len: usize) -> Vec<u8> {
    let request = Request {
        id: 903,
        generation: 1,
        store: 1,
        profile: 1,
        deadline_ms: 10000,
        response_bytes: MAX_FILE,
        operation: Operation::ReadFile {
            root,
            start: 0,
            end: len as u64,
        },
    };
    let mut result = Vec::new();
    f.service
        .handle(&f.peer, &request, &mut io::empty(), &mut result)
        .0
        .unwrap();
    result
}
#[test]
fn declared_whole_changed_noop_and_chunked_base_shrink_use_no_native_draft_and_exact_output() {
    for budget in [16 * 1024 * 1024, 48 * 1024 * 1024] {
        let f = Fixture::with_scratch("no-draft-whole", budget);
        let old = b"ordinary whole base payload";
        let root = fresh(&f, old);
        assert_eq!(native(&f), 0);
        for bytes in [old.as_slice(), b"different whole payload".as_slice()] {
            let want = expected(bytes);
            let mut body = Body {
                f: &f,
                raw: Cursor::new(file_save::body(&[(1, 0, bytes.len() as u64)], bytes)),
                expect_native: false,
                reads: 0,
            };
            let operation = if budget == 48 * 1024 * 1024 && bytes != old {
                Operation::SaveFileV2 {
                    base: Some(root),
                    base_length: old.len() as u64,
                    length: bytes.len() as u64,
                    extents: 1,
                    replacement: bytes.len() as u64,
                }
            } else {
                file_save::existing(
                    root,
                    old.len() as u64,
                    bytes.len() as u64,
                    1,
                    bytes.len() as u64,
                )
            };
            if matches!(&operation, Operation::SaveFileV2 { .. }) {
                body.raw.get_mut().insert(0, SAVE_FILE_V2_VERSION);
            }
            let result = saved(f.call(operation, &mut body).unwrap());
            assert!(body.reads >= 2);
            assert_eq!(result, want);
            assert_eq!(read(&f, result, bytes.len()), bytes);
            assert_eq!(read(&f, root, old.len()), old);
            assert_eq!(native(&f), 0);
        }
        let old = (0..262144).map(|n| (n % 251) as u8).collect::<Vec<_>>();
        let root = fresh(&f, &old);
        let bytes = b"small result from actual chunked base";
        let want = expected(bytes);
        let mut body = Body {
            f: &f,
            raw: Cursor::new(file_save::body(&[(1, 0, bytes.len() as u64)], bytes)),
            expect_native: false,
            reads: 0,
        };
        let result = saved(
            f.call(
                file_save::existing(
                    root,
                    old.len() as u64,
                    bytes.len() as u64,
                    1,
                    bytes.len() as u64,
                ),
                &mut body,
            )
            .unwrap(),
        );
        assert_eq!(result, want);
        assert_eq!(read(&f, result, bytes.len()), bytes);
        assert_eq!(read(&f, root, old.len()), old);
        assert_eq!(native(&f), 0);
        let expected_empty = fresh(&f, &[]);
        let mut body = Body {
            f: &f,
            raw: Cursor::new(Vec::new()),
            expect_native: false,
            reads: 0,
        };
        let empty = saved(
            f.call(
                file_save::existing(root, old.len() as u64, 0, 0, 0),
                &mut body,
            )
            .unwrap(),
        );
        assert_eq!(empty, expected_empty);
        assert!(read(&f, empty, 0).is_empty());
        assert_eq!(native(&f), 0);
    }
}
#[test]
fn final_cutoff_keeps_native6_while_base_kind_length_and_eof_errors_cannot_fallback() {
    let f = Fixture::new("no-draft-refusal");
    let old = b"immutable base";
    let root = fresh(&f, old);
    let bytes = b"new bytes";
    let valid = file_save::body(&[(1, 0, bytes.len() as u64)], bytes);
    let mut trailing = valid.clone();
    trailing.push(77);
    let mut wrongkind = valid.clone();
    wrongkind[..8].copy_from_slice(&7u64.to_be_bytes());
    let short = valid[..valid.len() - 1].to_vec();
    for (base, declared, raw) in [
        (root, old.len() as u64, trailing),
        (root, old.len() as u64, wrongkind),
        (root, old.len() as u64, short),
        (root, old.len() as u64 + 1, valid.clone()),
        (f.file_metadata, old.len() as u64, valid.clone()),
        ([233; 32], old.len() as u64, valid),
    ] {
        let mut body = Body {
            f: &f,
            raw: Cursor::new(raw),
            expect_native: false,
            reads: 0,
        };
        assert!(f
            .call(
                file_save::existing(base, declared, bytes.len() as u64, 1, bytes.len() as u64),
                &mut body
            )
            .is_err());
        assert_eq!(native(&f), 0);
        assert_eq!(read(&f, root, old.len()), old);
    }
    let bytes = vec![19u8; 131072];
    let mut body = Body {
        f: &f,
        raw: Cursor::new(file_save::body(&[(1, 0, bytes.len() as u64)], &bytes)),
        expect_native: true,
        reads: 0,
    };
    let chunked = saved(
        f.call(
            file_save::existing(
                root,
                old.len() as u64,
                bytes.len() as u64,
                1,
                bytes.len() as u64,
            ),
            &mut body,
        )
        .unwrap(),
    );
    assert!(body.reads >= 2);
    assert_eq!(read(&f, chunked, bytes.len()), bytes);
    // Native authority's parent directory remains owned after exact session cleanup.
    assert!(native(&f) > 0);
}
