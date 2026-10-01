//! Real direct Service binding claims and known cleanup, using the selected
//! SQLite provider. This logical-library scope makes no native hardcap, heap,
//! physical progress, release qualification, or performance claim.
//!
//! The phase test explicitly alters the owned external test scratch schema:
//! fixed root columns require site_owner.stage4 and graph_owner.stage7. It keeps production triggerdepth0
//! and foreign_keys1. The assertion proves first-root ordering; the owning C2
//! provider tests separately prove exact empty Sites/Graph retirement.

#![cfg(any(target_os = "macos", target_os = "linux"))]

#[path = "support/prepared_binding.rs"]
mod fixture;

use fixture::{Directory, Fixture, ORIGINAL};
use layerfs_bridge::contract::*;
use std::{
    collections::BTreeSet,
    io::{self, Cursor, Read},
    path::{Path, PathBuf},
    process::Command,
    sync::Mutex,
};

static SERIAL: Mutex<()> = Mutex::new(());

fn selected_library() -> PathBuf {
    #[cfg(target_os = "macos")]
    let libraries = {
        let output = Command::new("/usr/bin/otool")
            .arg("-L")
            .arg(std::env::current_exe().unwrap())
            .output()
            .unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .filter(|line| line.contains("libsqlite3."))
            .map(|line| PathBuf::from(line.split_whitespace().next().unwrap()))
            .collect::<BTreeSet<_>>()
    };
    #[cfg(target_os = "linux")]
    let libraries = {
        let mut maps = String::new();
        std::fs::File::open("/proc/self/maps")
            .unwrap()
            .take(1_048_577)
            .read_to_string(&mut maps)
            .unwrap();
        assert!(maps.len() <= 1_048_576);
        maps.lines()
            .filter_map(|line| line.split_whitespace().last())
            .filter(|path| path.starts_with('/') && path.contains("libsqlite3.so"))
            .map(PathBuf::from)
            .collect::<BTreeSet<_>>()
    };
    assert_eq!(
        libraries.len(),
        1,
        "one exact dynamically selected library, with no provider substitute"
    );
    libraries.into_iter().next().unwrap()
}

fn helper(library: &Path, mode: &str, database: &Path, sql: Option<&str>) -> String {
    let metadata = std::fs::symlink_metadata(database).unwrap();
    assert!(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "the owned fixture database itself must be a regular selected file"
    );
    let database = std::fs::canonicalize(database).unwrap();
    let mut command = Command::new("python3");
    command
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/claim_phase_assertion.py"))
        .arg(library)
        .arg(mode)
        .arg(database);
    if let Some(sql) = sql {
        command.arg(sql);
    }
    let output = command.output().unwrap();
    assert!(output.stdout.len() <= 8192 && output.stderr.len() <= 8192);
    assert!(
        output.status.success(),
        "selected-library observation failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    eprint!("{}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap()
}

fn number(library: &Path, database: &Path, sql: &str) -> u64 {
    helper(library, "number", database, Some(sql))
        .trim()
        .parse()
        .unwrap()
}

fn scratch_files(parent: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(parent).unwrap() {
        let entry = entry.unwrap();
        if !entry.file_name().to_string_lossy().starts_with(".lfcs-") {
            continue;
        }
        let metadata = std::fs::symlink_metadata(entry.path()).unwrap();
        assert!(metadata.is_dir() && !metadata.file_type().is_symlink());
        for file in std::fs::read_dir(entry.path()).unwrap() {
            let file = file.unwrap().path();
            let metadata = std::fs::symlink_metadata(&file).unwrap();
            assert!(metadata.is_file() && !metadata.file_type().is_symlink());
            assert_eq!(file.extension().unwrap(), "sqlite");
            files.push(file);
        }
    }
    files.sort();
    files
}

fn spools() -> BTreeSet<PathBuf> {
    let prefix = format!("layerfs-prepared-{}-", std::process::id());
    std::fs::read_dir(std::env::temp_dir())
        .unwrap()
        .filter_map(|entry| {
            let entry = entry.unwrap();
            entry
                .file_name()
                .to_str()
                .filter(|name| name.starts_with(&prefix))
                .map(|_| entry.path())
        })
        .collect()
}

struct ObservedBody<'a> {
    input: Cursor<&'a [u8]>,
    parent: &'a Path,
    library: &'a Path,
    enforce_phase: bool,
    scratch: Option<PathBuf>,
    before_spools: BTreeSet<PathBuf>,
    eof: bool,
    bytes: u64,
    scratch_bytes: u64,
}

impl<'a> ObservedBody<'a> {
    fn new(parent: &'a Path, library: &'a Path, bytes: &'a [u8], enforce_phase: bool) -> Self {
        assert!(scratch_files(parent).is_empty());
        Self {
            input: Cursor::new(bytes),
            parent,
            library,
            enforce_phase,
            scratch: None,
            before_spools: spools(),
            eof: false,
            bytes: 0,
            scratch_bytes: 16 * 1024 * 1024,
        }
    }

    fn known_cleanup(&self) {
        assert!(self.eof, "the production decoder acknowledged exact EOF");
        assert!(self.scratch.as_ref().is_some_and(|path| !path.exists()));
        assert!(scratch_files(self.parent).is_empty());
        assert_eq!(spools(), self.before_spools);
    }
}

impl Read for ObservedBody<'_> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if self.scratch.is_none() {
            assert_eq!(
                number(
                    self.library,
                    &self.parent.join("store.sqlite"),
                    "SELECT count(*) FROM saves WHERE active_slot IS NOT NULL"
                ),
                1,
                "the real owned Save is live before the first prepared-body read"
            );
            let files = scratch_files(self.parent);
            assert_eq!(
                files.len(),
                1,
                "one actual admitted native owner before body"
            );
            let path = files.into_iter().next().unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                let metadata = std::fs::metadata(&path).unwrap();
                assert_eq!(metadata.mode() & 0o777, 0o600);
                assert_eq!(metadata.nlink(), 1);
                assert_eq!(metadata.blocks() * 512, self.scratch_bytes);
            }
            assert_eq!(number(self.library, &path, "PRAGMA user_version"), 4);
            // max_page_count belongs to the production connection's Pager.
            // This separate reader observes persisted identity/budget only;
            // owning C2 tests require exact live connection readback.
            assert_eq!(
                number(
                    self.library,
                    &path,
                    "SELECT selected_bytes FROM session_owner"
                ),
                self.scratch_bytes
            );
            assert_eq!(
                number(
                    self.library,
                    &path,
                    "SELECT graph_records FROM session_owner"
                ),
                self.scratch_bytes / 256
            );
            assert_eq!(
                number(
                    self.library,
                    &path,
                    "SELECT graph_record_bytes FROM session_owner"
                ),
                (self.scratch_bytes / 256) * 63
            );
            self.scratch = Some(path);
        }
        let bytes = self.input.read(output)?;
        self.bytes += bytes as u64;
        if bytes == 0 && !self.eof {
            self.eof = true;
            if self.enforce_phase {
                let installed = helper(
                    self.library,
                    "install",
                    self.scratch.as_ref().unwrap(),
                    None,
                );
                assert_eq!(installed.trim(), "installed site4 graph7 assertions; owner_columns12 site_columns10 graph_columns16 solver_columns11 root_columns5 trigger_depth0 foreign_keys1 pre_phase0 constraint787");
            }
        }
        Ok(bytes)
    }
}

fn directories(f: &Fixture, count: usize, duplicate: bool) -> (Vec<Directory>, Vec<(u64, u32)>) {
    let serials = (0..count).map(|_| f.reserve()).collect::<Vec<_>>();
    assert!(serials.windows(2).all(|pair| pair[0] < pair[1]));
    let mut names = serials
        .iter()
        .enumerate()
        .map(|(index, serial)| (format!("d-{index:04}").into_bytes(), Some(*serial)))
        .collect::<Vec<_>>();
    if duplicate {
        names.push((format!("d-{count:04}").into_bytes(), Some(serials[0])));
    }
    let rows = std::iter::once((f.root_serial, names))
        .chain(serials.iter().map(|serial| (*serial, Vec::new())))
        .collect();
    let declarations = serials.iter().map(|serial| (*serial, 0o755)).collect();
    (rows, declarations)
}

#[test]
fn stage_with_257_exclusive_directories_retires_claims_before_first_root_insert() {
    let _serial = SERIAL.lock().unwrap();
    let f = Fixture::new("claim-phase");
    let library = selected_library();
    let (rows, declarations) = directories(&f, 257, false);
    let (header, raw) = f.prepared([121; 32], 1, &rows, &declarations);
    assert_eq!((header.totals.directories, header.totals.names), (258, 257));
    let mut body = ObservedBody::new(f.path(), &library, &raw, true);
    let stage = f.stage(header.clone(), &mut body).unwrap();
    body.known_cleanup();
    assert_eq!(body.bytes, raw.len() as u64);
    assert_ne!(stage.candidate_root, f.root);
    assert_eq!(f.current_stage(header.workspace), stage);
    assert_eq!(
        (
            stage.expected_root,
            stage.construction_base_root,
            stage.scope
        ),
        (f.root, f.root, f.scope)
    );
    let expected = std::iter::once((b"a".to_vec(), f.file_serial))
        .chain(
            rows[0]
                .1
                .iter()
                .map(|(name, child)| (name.clone(), child.unwrap())),
        )
        .collect::<Vec<_>>();
    assert_eq!(f.list(stage.candidate_root, b""), expected);
    for (name, child) in &rows[0].1 {
        let Response::Stat {
            serial,
            kind,
            references,
            mode,
            ..
        } = f.stat(stage.candidate_root, name)
        else {
            panic!("directory stat")
        };
        assert_eq!(
            (serial, kind, references, mode),
            (child.unwrap(), 2, 1, 0o755)
        );
        assert!(f.list(stage.candidate_root, name).is_empty());
    }
    assert_eq!(f.list(f.root, b""), vec![(b"a".to_vec(), f.file_serial)]);
    assert_eq!(f.original_bytes(), ORIGINAL);
    assert_eq!(
        number(
            &library,
            &f.path().join("store.sqlite"),
            "SELECT count(*) FROM saves WHERE active_slot IS NOT NULL"
        ),
        0
    );
    eprintln!("DIAGNOSTIC direct-legacy selected-provider Stage: sites257 directories258, one native16MiB owner/source-bound profile4; external required-site4/graph7 FKs held at every actual rootinsert, triggerdepth0 unchanged; known scratch/spool/Save cleanup. Provider proof separately owns exactemptystage4; no heap/physical/progress/speed admission.");
}

#[test]
fn duplicate_after_256_exclusive_claims_preserves_prior_stage_and_known_save_cleanup() {
    let _serial = SERIAL.lock().unwrap();
    let f = Fixture::new("claim-duplicate");
    let library = selected_library();
    let workspace = [122; 32];
    let (header, raw) = f.prepared(
        workspace,
        1,
        &[(f.root_serial, vec![(b"b".to_vec(), Some(f.file_serial))])],
        &[],
    );
    let mut body = ObservedBody::new(f.path(), &library, &raw, false);
    let prior = f.stage(header, &mut body).unwrap();
    body.known_cleanup();
    let store = f.path().join("store.sqlite");
    let saves = number(&library, &store, "SELECT count(*) FROM saves");
    let (rows, declarations) = directories(&f, 256, true);
    let (header, raw) = f.prepared(workspace, 2, &rows, &declarations);
    assert_eq!(header.totals.names, 257);
    assert_eq!(rows[0].1[0].1, rows[0].1[256].1);
    let mut body = ObservedBody::new(f.path(), &library, &raw, true);
    let failure = f.stage(header, &mut body).unwrap_err();
    body.known_cleanup();
    assert_eq!(body.bytes, raw.len() as u64);
    assert_eq!(failure.code, Code::InvalidInput);
    assert!(!failure.unknown);
    assert_eq!(failure.cleanup, None);
    assert_eq!(f.current_stage(workspace), prior);
    assert_eq!(
        number(
            &library,
            &store,
            "SELECT count(*) FROM saves WHERE active_slot IS NOT NULL"
        ),
        0
    );
    assert_eq!(
        number(&library, &store, "SELECT count(*) FROM saves"),
        saves
    );
    assert_eq!(f.list(f.root, b""), vec![(b"a".to_vec(), f.file_serial)]);
    assert_eq!(
        f.list(prior.candidate_root, b""),
        vec![
            (b"a".to_vec(), f.file_serial),
            (b"b".to_vec(), f.file_serial)
        ]
    );
    assert_eq!(f.original_bytes(), ORIGINAL);
    eprintln!("DIAGNOSTIC direct-legacy selected-provider Stage: duplicate257 repeatsite1 across two128-key windows; InvalidInput known, priorStage/base preserved and unfinishedSave/scratch/spool removed; no fallback/retry/performance admission.");
}

#[test]
fn stored_site_permutation_and_retained_parent_refusal_preserve_real_stage_and_cleanup() {
    let _serial = SERIAL.lock().unwrap();
    let f = Fixture::with_directories("site-existing-permutation", 129);
    let library = selected_library();
    let workspace = [123; 32];
    let before = f.list(f.root, b"");
    assert_eq!(before.len(), 130);
    let directories: Vec<_> = before
        .iter()
        .filter(|(name, _)| name.starts_with(b"d"))
        .cloned()
        .collect();
    assert_eq!(directories.len(), 129);
    for (name, serial) in &directories {
        assert!(matches!(
            f.stat(f.root, name),
            Response::Stat { kind: 2, serial: found, .. } if found == *serial
        ));
        assert!(f.list(f.root, name).is_empty());
    }
    let changes: Vec<_> = directories
        .iter()
        .enumerate()
        .map(|(index, (name, _))| {
            (
                name.clone(),
                Some(directories[(index + 1) % directories.len()].1),
            )
        })
        .collect();
    let (header, raw) = f.prepared(workspace, 1, &[(f.root_serial, changes.clone())], &[]);
    assert_eq!((header.totals.names, header.totals.fresh), (129, 0));
    let mut body = ObservedBody::new(f.path(), &library, &raw, true);
    let prior = f.stage(header, &mut body).unwrap();
    body.known_cleanup();
    assert_eq!(body.bytes, raw.len() as u64);
    let mut expected = vec![(b"a".to_vec(), f.file_serial)];
    expected.extend(
        changes
            .iter()
            .map(|(name, serial)| (name.clone(), serial.unwrap())),
    );
    assert_eq!(f.list(prior.candidate_root, b""), expected);
    assert_eq!(f.list(f.root, b""), before);
    for (name, serial) in &expected[1..] {
        assert!(matches!(
            f.stat(prior.candidate_root, name),
            Response::Stat { kind: 2, serial: found, .. } if found == *serial
        ));
        assert!(f.list(prior.candidate_root, name).is_empty());
    }
    let store = f.path().join("store.sqlite");
    let saves = number(&library, &store, "SELECT count(*) FROM saves");
    // The old d0000 binding is unmentioned and survives. Binding that existing
    // non-file at x cannot gain a second parent, even after a prior valid Stage.
    let (header, raw) = f.prepared(
        workspace,
        2,
        &[(f.root_serial, vec![(b"x".to_vec(), Some(directories[0].1))])],
        &[],
    );
    let mut body = ObservedBody::new(f.path(), &library, &raw, true);
    let failure = f.stage(header, &mut body).unwrap_err();
    body.known_cleanup();
    assert_eq!(failure.code, Code::InvalidInput);
    assert!(!failure.unknown);
    assert_eq!(failure.cleanup, None);
    assert_eq!(f.current_stage(workspace), prior);
    assert_eq!(
        number(&library, &store, "SELECT count(*) FROM saves"),
        saves
    );
    assert_eq!(
        number(
            &library,
            &store,
            "SELECT count(*) FROM saves WHERE active_slot IS NOT NULL"
        ),
        0
    );
    assert_eq!(f.list(f.root, b""), before);
    assert_eq!(f.original_bytes(), ORIGINAL);
    eprintln!("DIAGNOSTIC direct-library profile4:129 existing sites cross128, actual source-bound birth/parent/final/retire->Stage with site4/graph7 FKs; old/new serial restatement permutation preserved; surviving old parent refuses and priorStage/base/known Save-scratch cleanup remain exact. No speed/global/physical qualification.");
}

#[test]
fn configured_48mib_state_budget_is_bound_before_body_and_retires_before_roots() {
    let _serial = SERIAL.lock().unwrap();
    let budget = 48 * 1024 * 1024;
    let f = Fixture::with_scratch("graph-configured", budget);
    let library = selected_library();
    let (rows, declarations) = directories(&f, 129, false);
    let (header, raw) = f.prepared([124; 32], 1, &rows, &declarations);
    let mut body = ObservedBody::new(f.path(), &library, &raw, true);
    body.scratch_bytes = budget;
    let stage = f.stage(header.clone(), &mut body).unwrap();
    body.known_cleanup();
    assert_eq!(body.bytes, raw.len() as u64);
    assert_eq!(f.current_stage(header.workspace), stage);
    assert_eq!(f.list(stage.candidate_root, b"").len(), 130);
    assert_eq!(f.list(f.root, b""), vec![(b"a".to_vec(), f.file_serial)]);
    assert_eq!(f.original_bytes(), ORIGINAL);
    assert_eq!(
        number(
            &library,
            &f.path().join("store.sqlite"),
            "SELECT count(*) FROM saves WHERE active_slot IS NOT NULL"
        ),
        0
    );
    eprintln!("DIAGNOSTIC configured48MiB actual native reservation/header/selectedS-R-L before body; one owner, sites4/graph7 root fences and known scratch/spool/Save cleanup. RAM/work/request limits unchanged; no larger input or speed/physical admission.");
}

#[test]
fn overlapping_257_directory_chain_restatement_preserves_root_and_cycle_preserves_stage() {
    let _serial = SERIAL.lock().unwrap();
    // Root plus256 ordinary imported directories; deepest path has256 components.
    let f = Fixture::with_chain("graph-chain", 256);
    let library = selected_library();
    let mut path = Vec::new();
    let mut parent = f.root_serial;
    let mut rows = Vec::new();
    let mut first = None;
    let mut second = None;
    for index in 0..256 {
        if index != 0 {
            path.push(b'/');
        }
        path.push(b'd');
        let Response::Stat { serial, kind, .. } = f.stat(f.root, &path) else {
            panic!("directory stat");
        };
        assert_eq!(kind, 2);
        rows.push((parent, vec![(b"d".to_vec(), Some(serial))]));
        if index == 0 {
            first = Some(serial);
        }
        if index == 1 {
            second = Some(serial);
        }
        parent = serial;
    }
    rows.push((parent, Vec::new()));
    rows.sort_by_key(|row| row.0);
    assert!(rows.windows(2).all(|pair| pair[0].0 < pair[1].0));
    let workspace = [125; 32];
    let (header, raw) = f.prepared(workspace, 1, &rows, &[]);
    assert_eq!((header.totals.directories, header.totals.names), (257, 256));
    let mut body = ObservedBody::new(f.path(), &library, &raw, true);
    let prior = f.stage(header, &mut body).unwrap();
    body.known_cleanup();
    // The expected identity is the immutable pre-operation root, not candidate output.
    assert_eq!(prior.candidate_root, f.root);
    assert_eq!(f.current_stage(workspace), prior);
    assert_eq!(f.original_bytes(), ORIGINAL);
    let first = first.unwrap();
    let second = second.unwrap();
    let mut cycle = vec![
        (f.root_serial, vec![(b"d".to_vec(), None)]),
        (second, vec![(b"back".to_vec(), Some(first))]),
    ];
    cycle.sort_by_key(|row| row.0);
    let (header, raw) = f.prepared(workspace, 2, &cycle, &[]);
    let mut body = ObservedBody::new(f.path(), &library, &raw, true);
    let error = f.stage(header, &mut body).unwrap_err();
    body.known_cleanup();
    assert_eq!(error.code, Code::InvalidInput);
    assert!(!error.unknown);
    assert_eq!(error.cleanup, None);
    assert_eq!(f.current_stage(workspace), prior);
    assert_eq!(f.original_bytes(), ORIGINAL);
    assert_eq!(
        number(
            &library,
            &f.path().join("store.sqlite"),
            "SELECT count(*) FROM saves WHERE active_slot IS NOT NULL"
        ),
        0
    );
    eprintln!("DIAGNOSTIC real Service imported257-directory chain/all256 restated edges; independent unchangedBase root and phase fences. Final-batch ancestor move forms selected cycle: known InvalidInput, priorStage/base/Save-scratch-spool cleanup exact. C1 owns actual expansion/SCC count proof; no timing or speed admission.");
}
