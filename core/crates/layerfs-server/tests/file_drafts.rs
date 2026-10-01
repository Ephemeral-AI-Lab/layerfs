//! Real Server supplied profile6 and native cleanup; no hardcap or speed claim.
#![cfg(any(target_os = "macos", target_os = "linux"))]
#[path = "support/file_save.rs"]
mod file_save;
#[path = "support/prepared_binding.rs"]
mod fixture;
#[path = "support/draft_observation.rs"]
mod observe;
use fixture::Fixture;
use layerfs_bridge::contract::*;
use std::{
    io::{self, Cursor, Read},
    path::{Path, PathBuf},
    sync::Mutex,
};
static SERIAL: Mutex<()> = Mutex::new(());

struct Body<'a> {
    fixture: &'a Fixture,
    library: &'a Path,
    bytes: Cursor<Vec<u8>>,
    scratch: Option<PathBuf>,
    budget: u64,
    saves_before: u64,
    lock_on_read: bool,
    lock: Option<observe::ReaderLock>,
}
impl Read for Body<'_> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if self.scratch.is_none() {
            let paths = observe::scratch_files(self.fixture.path());
            assert_eq!(
                paths.len(),
                1,
                "real profile6 exists before first body read"
            );
            let path = paths[0].clone();
            assert_eq!(
                observe::number(self.library, &path, "PRAGMA user_version"),
                6
            );
            let sql = format!("SELECT length(header)=216 AND hex(substr(header,1,8))='4C4643534F574E36' AND hex(substr(header,209,8))='{:016X}' FROM session_owner WHERE id=1",self.budget);
            assert_eq!(observe::number(self.library, &path, &sql), 1);
            let sql = format!("SELECT stage=0 AND records=0 AND bytes=0 AND length(scope)=105 AND hex(substr(scope,98,8))='{:016X}' FROM draft_owner WHERE id=1",self.budget);
            assert_eq!(observe::number(self.library, &path, &sql), 1);
            let store = self.fixture.path().join("store.sqlite");
            assert_eq!(
                observe::number(self.library, &store, "SELECT count(*) FROM saves"),
                self.saves_before,
                "scratch admission precedes content Save"
            );
            assert_eq!(
                observe::number(
                    self.library,
                    &store,
                    "SELECT count(*) FROM saves WHERE active_slot IS NOT NULL"
                ),
                0
            );
            if self.lock_on_read {
                self.lock = Some(observe::ReaderLock::acquire(&path, self.library));
            }
            self.scratch = Some(path);
        }
        self.bytes.read(output)
    }
}
fn read(fixture: &Fixture, root: Root, length: u64) -> Vec<u8> {
    let request = Request {
        id: 9000,
        generation: 1,
        store: 1,
        profile: 1,
        deadline_ms: 10000,
        response_bytes: MAX_FILE,
        operation: Operation::ReadFile {
            root,
            start: 0,
            end: length,
        },
    };
    let mut output = Vec::new();
    fixture
        .service
        .handle(&fixture.peer, &request, &mut io::empty(), &mut output)
        .0
        .unwrap();
    output
}
fn saved(response: Response) -> Root {
    let Response::Saved { root, .. } = response else {
        panic!("file save")
    };
    root
}
fn base(f: &Fixture) -> (Vec<u8>, Root) {
    let mut value = 0x4b29a18bu32;
    let bytes = (0..262144)
        .map(|_| {
            value ^= value << 13;
            value ^= value >> 17;
            value ^= value << 5;
            value as u8
        })
        .collect::<Vec<_>>();
    let root = saved(
        f.call(
            file_save::fresh(bytes.len() as u64),
            &mut Cursor::new(file_save::fresh_body(&bytes)),
        )
        .unwrap(),
    );
    assert!(observe::scratch_files(f.path()).is_empty());
    (bytes, root)
}
fn body<'a>(fixture: &'a Fixture, library: &'a Path, bytes: Vec<u8>, budget: u64) -> Body<'a> {
    let saves_before = observe::number(
        library,
        &fixture.path().join("store.sqlite"),
        "SELECT count(*) FROM saves",
    );
    Body {
        fixture,
        library,
        bytes: Cursor::new(bytes),
        scratch: None,
        budget,
        saves_before,
        lock_on_read: false,
        lock: None,
    }
}
#[test]
fn configured_16_and_48mib_real_edits_select_profile6_before_body_and_return_after_cleanup() {
    let _serial = SERIAL.lock().unwrap();
    let library = observe::selected_library();
    for budget in [16 * 1024 * 1024, 48 * 1024 * 1024] {
        let f = Fixture::with_scratch("file-drafts", budget);
        let (old, base) = base(&f);
        let replacement = [0x8au8; 32];
        let mut expected = old.clone();
        expected[..32].copy_from_slice(&replacement);
        let wire = file_save::body(&[(1, 0, 32), (0, 32, old.len() as u64 - 32)], &replacement);
        let mut input = body(&f, &library, wire, budget);
        let root = saved(
            f.call(
                file_save::existing(base, old.len() as u64, old.len() as u64, 2, 32),
                &mut input,
            )
            .unwrap(),
        );
        assert_ne!(root, base);
        assert!(
            input.scratch.as_ref().unwrap().exists(),
            "known exact scratch stays charged Idle after Saved root response"
        );
        assert_eq!(f.service.drain_construction_idle().unwrap(), 1);
        assert_eq!(f.service.drain_construction_idle().unwrap(), 0);
        assert!(!input.scratch.as_ref().unwrap().exists());
        assert!(observe::scratch_files(f.path()).is_empty());
        assert_eq!(read(&f, root, old.len() as u64), expected);
        assert_eq!(read(&f, base, old.len() as u64), old);
        assert_eq!(
            observe::number(
                &library,
                &f.path().join("store.sqlite"),
                "SELECT count(*) FROM saves WHERE active_slot IS NOT NULL"
            ),
            0
        );
    }
}
#[test]
fn no_op_and_malformed_actual_extent_body_do_not_bypass_draft_terminal_custody() {
    let _serial = SERIAL.lock().unwrap();
    let library = observe::selected_library();
    let f = Fixture::new("file-drafts-refusal");
    let (old, base) = base(&f);
    let mut input = body(
        &f,
        &library,
        file_save::body(&[(0, 0, old.len() as u64)], &[]),
        16 * 1024 * 1024,
    );
    let root = saved(
        f.call(
            file_save::existing(base, old.len() as u64, old.len() as u64, 1, 0),
            &mut input,
        )
        .unwrap(),
    );
    assert_eq!(root, base);
    let idle_path = input.scratch.unwrap();
    assert!(idle_path.exists());
    let before = observe::number(
        &library,
        &f.path().join("store.sqlite"),
        "SELECT count(*) FROM saves",
    );
    let mut input = body(
        &f,
        &library,
        file_save::body(&[(7, 0, old.len() as u64)], &[]),
        16 * 1024 * 1024,
    );
    let error = f
        .call(
            file_save::existing(base, old.len() as u64, old.len() as u64, 1, 0),
            &mut input,
        )
        .unwrap_err();
    assert_eq!(error.code, Code::InvalidInput);
    assert!(!error.unknown && error.cleanup.is_none());
    assert_eq!(
        input.scratch.as_ref().unwrap(),
        &idle_path,
        "same eligible file was selected before the malformed body"
    );
    assert!(!input.scratch.unwrap().exists());
    assert_eq!(f.service.drain_construction_idle().unwrap(), 0);
    assert_eq!(
        observe::number(
            &library,
            &f.path().join("store.sqlite"),
            "SELECT count(*) FROM saves"
        ),
        before
    );
    assert!(observe::scratch_files(f.path()).is_empty());
    assert_eq!(read(&f, base, old.len() as u64), old);
}

#[test]
fn actual_metadata_commit_unknown_retains_its_owner_and_aborts_separate_known_content_save() {
    let _serial = SERIAL.lock().unwrap();
    let library = observe::selected_library();
    let f = Fixture::new("file-drafts-unknown");
    let (old, base) = base(&f);
    let wire = file_save::body(&[(1, 0, 32), (0, 32, old.len() as u64 - 32)], &[0x8a; 32]);
    let mut input = body(&f, &library, wire, 16 * 1024 * 1024);
    input.lock_on_read = true;
    let error = f
        .call(
            file_save::existing(base, old.len() as u64, old.len() as u64, 2, 32),
            &mut input,
        )
        .unwrap_err();
    assert!(
        error.unknown,
        "actual selected scratch COMMIT loses acknowledgement"
    );
    let retained = input.scratch.as_ref().unwrap();
    assert!(
        retained.exists(),
        "exact owner is retained without unlink/refund/retry"
    );
    input.lock.as_mut().unwrap().release();
    assert!(
        retained.exists(),
        "barrier removal does not adopt or replay Unknown"
    );
    assert_eq!(
        observe::number(
            &library,
            &f.path().join("store.sqlite"),
            "SELECT count(*) FROM saves WHERE active_slot IS NOT NULL"
        ),
        0,
        "metadata Unknown cannot hide known content Save abort"
    );
    assert_eq!(read(&f, base, old.len() as u64), old);
    eprintln!("DIAGNOSTIC real profile6 metadata COMMIT Unknown retained and separate known content Save abort; no Saved root, automatic retry, native/global/physical or speed qualification");
}
