//! Actual direct Service canonical8 reuse and independent roots/bytes; no global/native heap claim.
#![cfg(any(target_os = "macos", target_os = "linux"))]
#[path = "support/prepared_binding.rs"]
mod fixture;
#[allow(dead_code)]
#[path = "support/draft_observation.rs"]
mod observe;
#[path = "support/scratch_reuse.rs"]
mod oracle;
use fixture::{Fixture, ORIGINAL};
use layerfs_bridge::contract::*;
use std::{
    collections::BTreeSet,
    io::{self, Cursor, Read},
    path::{Path, PathBuf},
    sync::Mutex,
};
static SERIAL: Mutex<()> = Mutex::new(());
#[derive(Debug)]
struct Capture {
    path: PathBuf,
    device: u64,
    inode: u64,
    descriptors: BTreeSet<i32>,
    image: oracle::Image,
}
struct Body<'a> {
    fixture: &'a Fixture,
    library: &'a Path,
    wire: Cursor<Vec<u8>>,
    capture: Option<Capture>,
    budget: u64,
    lock_store_on_eof: bool,
    store_reader: Option<oracle::StoreReader>,
}
impl Read for Body<'_> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if self.capture.is_none() {
            use std::os::unix::fs::MetadataExt;
            let paths = observe::scratch_files(self.fixture.path());
            assert_eq!(
                paths.len(),
                1,
                "actualcanonical authority beforebody; noSmall/Emptyselection"
            );
            let path = paths[0].clone();
            let image = oracle::image(self.library, &path);
            assert_eq!(image.header.len(), 386);
            assert_eq!(&image.header[..8], b"LFCSOWN8");
            assert_eq!(&image.header[8..10], &8u16.to_be_bytes());
            assert_eq!(
                image.scalars,
                vec![1, 4, self.budget, 0, 0, 0, 0, 0, 0, 0, 0, 0, 9, 9]
            );
            assert_eq!(image.graph, image.solver);
            assert_eq!(image.site.len(), 89);
            assert_eq!(image.graph.len(), 188);
            let meta = std::fs::metadata(&path)?;
            assert!(
                meta.blocks() * 512 <= self.budget,
                "actualnativeextentswithinS"
            );
            self.capture = Some(Capture {
                device: meta.dev(),
                inode: meta.ino(),
                descriptors: oracle::descriptors(&path),
                path,
                image,
            });
        }
        let count = self.wire.read(out)?;
        if count == 0 && self.lock_store_on_eof && self.store_reader.is_none() {
            self.store_reader = Some(oracle::StoreReader::acquire(
                self.library,
                &self.fixture.path().join("store.sqlite"),
            ));
        }
        Ok(count)
    }
}
fn body<'a>(
    fixture: &'a Fixture,
    library: &'a Path,
    wire: Vec<u8>,
    budget: u64,
    lock: bool,
) -> Body<'a> {
    Body {
        fixture,
        library,
        wire: Cursor::new(wire),
        capture: None,
        budget,
        lock_store_on_eof: lock,
        store_reader: None,
    }
}
fn names() -> Vec<Vec<u8>> {
    vec![
        b"a".to_vec(),
        b"b00".to_vec(),
        b"b01".to_vec(),
        b"b02".to_vec(),
        b"b03".to_vec(),
    ]
}
fn expected(f: &Fixture) -> Root {
    let Response::Stat {
        metadata, kind: 2, ..
    } = f.stat(f.root, b"")
    else {
        panic!("immutablebaseRootmetadata")
    };
    oracle::alias_root(
        f.scope,
        f.root_serial,
        metadata,
        f.file_serial,
        f.file_content,
        f.file_metadata,
        &names(),
    )
}
fn changes(f: &Fixture, workspace: u8) -> (PreparedChanges, Vec<u8>) {
    let bindings = names()
        .into_iter()
        .skip(1)
        .map(|name| (name, Some(f.file_serial)))
        .collect();
    f.prepared([workspace; 32], 1, &[(f.root_serial, bindings)], &[])
}
fn assert_result(f: &Fixture, stage: &StageWire, wanted: Root) {
    assert_eq!(stage.candidate_root, wanted);
    assert_eq!(stage.expected_root, f.root);
    assert_eq!(stage.construction_base_root, f.root);
    assert_eq!(stage.scope, f.scope);
    assert_eq!(
        f.list(stage.candidate_root, b""),
        names()
            .into_iter()
            .map(|name| (name, f.file_serial))
            .collect::<Vec<_>>()
    );
    let Response::Stat {
        serial,
        kind,
        references,
        content,
        metadata,
        ..
    } = f.stat(stage.candidate_root, b"b03")
    else {
        panic!("alias")
    };
    assert_eq!(
        (serial, kind, references, content, metadata),
        (f.file_serial, 1, 5, f.file_content, f.file_metadata)
    );
    assert_eq!(f.list(f.root, b""), vec![(b"a".to_vec(), f.file_serial)]);
    assert_eq!(f.original_bytes(), ORIGINAL);
}
fn idle(library: &Path, path: &Path, budget: u64) -> oracle::Image {
    let image = oracle::image(library, path);
    assert_eq!(
        image.scalars,
        vec![1, 4, budget, 0, 0, 0, 0, 0, 0, 0, 0, 0, 9, 9]
    );
    image
}
#[test]
fn same_service_two_real_stage_updates_reuse_profile8_full_binding_native_fds_and16_48_idle() {
    let _serial = SERIAL.lock().unwrap();
    let library = observe::selected_library();
    for budget in [16 * 1024 * 1024, 48 * 1024 * 1024] {
        let f = Fixture::with_scratch("canonical-reuse", budget);
        f.service.drain_construction_idle().unwrap();
        assert!(observe::scratch_files(f.path()).is_empty());
        let wanted = expected(&f);
        let (header_a, wire_a) = changes(&f, 91);
        let mut input_a = body(&f, &library, wire_a, budget, false);
        let stage_a = f.stage(header_a.clone(), &mut input_a).unwrap();
        assert_result(&f, &stage_a, wanted);
        assert_eq!(f.current_stage(header_a.workspace), stage_a);
        let capture_a = input_a.capture.take().unwrap();
        let before = idle(&library, &capture_a.path, budget);
        assert_eq!(before.header, capture_a.image.header);
        assert_eq!(
            observe::scratch_files(f.path()),
            vec![capture_a.path.clone()]
        );
        let (header_b, wire_b) = changes(&f, 92);
        let mut input_b = body(&f, &library, wire_b, budget, false);
        let stage_b = f.stage(header_b.clone(), &mut input_b).unwrap();
        assert_result(&f, &stage_b, wanted);
        assert_eq!(stage_a.candidate_root, stage_b.candidate_root);
        assert_eq!(f.current_stage(header_b.workspace), stage_b);
        let capture_b = input_b.capture.take().unwrap();
        assert_eq!(capture_b.path, capture_a.path);
        assert_eq!(
            (capture_b.device, capture_b.inode),
            (capture_a.device, capture_a.inode)
        );
        assert_eq!(capture_b.descriptors, capture_a.descriptors);
        assert_eq!(
            &capture_b.image.header[88..192],
            &capture_a.image.header[88..192]
        );
        assert_ne!(
            &capture_b.image.header[16..24],
            &capture_a.image.header[16..24],
            "freshfulltoken"
        );
        assert_ne!(
            &capture_b.image.header[24..56],
            &capture_a.image.header[24..56],
            "freshrequestselector"
        );
        assert_ne!(
            &capture_b.image.header[56..88],
            &capture_a.image.header[56..88],
            "freshnativebinding"
        );
        assert_ne!(
            &capture_b.image.header[192..200],
            &capture_a.image.header[192..200],
            "freshfullsourceissuer"
        );
        assert_ne!(capture_b.image.graph, capture_a.image.graph);
        assert_eq!(capture_b.image.graph, capture_b.image.solver);
        idle(&library, &capture_b.path, budget);
        assert_eq!(
            observe::number(
                &library,
                &f.path().join("store.sqlite"),
                "SELECT count(*) FROM saves WHERE active_slot IS NOT NULL"
            ),
            0
        );
        assert!(capture_b.path.exists());
        assert_eq!(f.service.drain_construction_idle().unwrap(), 1);
        assert!(!capture_b.path.exists());
        assert_eq!(f.service.drain_construction_idle().unwrap(), 0);
        assert!(observe::scratch_files(f.path()).is_empty());
        assert_result(&f, &stage_b, wanted);
    }
}
#[test]
fn deduplicated_repeat_store_finish_unknown_preserves_idle_and_skips_c5_stage() {
    let _serial = SERIAL.lock().unwrap();
    let library = observe::selected_library();
    let budget = 16 * 1024 * 1024;
    let f = Fixture::with_scratch("canonical-c5-unknown", budget);
    f.service.drain_construction_idle().unwrap();
    let wanted = expected(&f);
    let (header_a, wire_a) = changes(&f, 93);
    let mut first = body(&f, &library, wire_a, budget, false);
    let original_workspace = header_a.workspace;
    let stage = f.stage(header_a, &mut first).unwrap();
    assert_result(&f, &stage, wanted);
    let original = first.capture.take().unwrap();
    idle(&library, &original.path, budget);
    drop(first);
    // Identical canonical bytes are already published. Hold the actual Store
    // SHARED lock at bodyEOF, so the final Save COMMIT loses acknowledgement.
    // The assertion below requires independently known scratch reset/Idle,
    // rejecting an earlier object-write failure or simulated Unknown.
    let (header_b, wire_b) = changes(&f, 94);
    let mut second = body(&f, &library, wire_b, budget, true);
    let failed_workspace = header_b.workspace;
    let error = f.stage(header_b, &mut second).unwrap_err();
    assert!(error.unknown, "real Store COMMIT mustbeUnknown: {error:?}");
    let current = second.capture.as_ref().unwrap();
    assert_eq!(current.path, original.path);
    let reset = idle(&library, &current.path, budget);
    assert_eq!(reset.header, current.image.header);
    assert!(current.path.exists());
    assert_eq!(
        f.service.drain_construction_idle().unwrap(),
        1,
        "StorefinishUnknown cannotquarantine knownsuccessfulscratch"
    );
    assert!(!current.path.exists());
    second.store_reader.as_mut().unwrap().release();
    assert_eq!(f.service.drain_construction_idle().unwrap(), 0);
    assert_eq!(
        f.current_stage(original_workspace),
        stage,
        "originalC5stage remainsunchanged"
    );
    let absent = f
        .call(
            Operation::HistoryQuery(HistoryQuery::GetStage {
                workspace: failed_workspace,
            }),
            &mut io::empty(),
        )
        .unwrap_err();
    assert_eq!(
        absent.code,
        Code::NotFound,
        "failedStore.finish mustnotexecuteC5Stage"
    );
    // No Store adoption/replay/cleanup is attempted after its actual Unknown.
    drop(second);
    std::mem::forget(f);
}
