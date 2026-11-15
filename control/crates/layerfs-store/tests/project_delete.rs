//! `project.delete`'s home path (T4.5b): the durable intent before the marker,
//! the retry classifications (`Marked`, `Retired`, `Absent`, a hidden Store
//! project), the `home_entry_removed` echo, and the worker's own entry. A fake
//! `Effects` stands in for the Store so the home state machine is tested
//! without opening one.
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_bridge::sdk_values::hex;
use layerfs_history::LayerStackId;
use layerfs_home::home::{paths, write_record};
use layerfs_home::outcome::{failed, refused};
use layerfs_store::project::delete::{
    delete_with, remove_worker_entry, Admission, Effects, Intent, IntentState,
};
use layerfs_store_service::history::deletion::Deletion;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};

fn t(text: &str) -> SdkValue {
    SdkValue::text(text)
}

fn stack(body: u8) -> LayerStackId {
    LayerStackId::from_authority([body; 16])
}

fn home(tag: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!(
        "layerfs-project-delete-{tag}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&directory);
    layerfs_home::home::layout::create(&directory).unwrap();
    directory
}

fn project_record(home: &Path, store: &str, project: &str, stack: LayerStackId) {
    write_record(
        &paths::project_record(home, store, project),
        &SdkValue::map([
            ("v", SdkValue::Int(1)),
            ("name", t(project)),
            ("stack", t(&hex(&stack.to_bytes()))),
            ("created", SdkValue::Int(1)),
        ]),
    )
    .unwrap();
}

fn intent(_home: &Path, store: &str, project: &str, stack: LayerStackId, id: i64) -> Intent {
    Intent {
        store: store.to_owned(),
        project: project.to_owned(),
        stack,
        deleted_ms: id,
        attempt_ms: 1,
        state: IntentState::Pending,
        op: None,
    }
}

/// A fake home/worker: the Store side and entry remover, with the project
/// scope admission a live mount or operation would refuse. `mark` reads the
/// intent as the marker would see it, so a test can see the state that
/// preceded it.
struct Fake {
    home: PathBuf,
    store: String,
    project: String,
    outcomes: VecDeque<Deletion>,
    seen: Vec<(LayerStackId, i64)>,
    states: Vec<IntentState>,
    prepared: usize,
    fail_removal: bool,
    /// What `admit` refuses with, before registering anything.
    refuse: Option<SdkRefusal>,
    /// What `mark` refuses with, before writing anything.
    mark_refusal: Option<SdkRefusal>,
    /// Admissions taken, in order.
    admitted: Vec<Admission>,
    /// Admissions released, in order.
    released: Vec<Admission>,
}

impl Fake {
    fn new(home: &Path, store: &str, project: &str, outcomes: Vec<Deletion>) -> Self {
        Self {
            home: home.to_path_buf(),
            store: store.to_owned(),
            project: project.to_owned(),
            outcomes: outcomes.into(),
            seen: Vec::new(),
            states: Vec::new(),
            prepared: 0,
            fail_removal: false,
            refuse: None,
            mark_refusal: None,
            admitted: Vec::new(),
            released: Vec::new(),
        }
    }

    fn failing(mut self) -> Self {
        self.fail_removal = true;
        self
    }

    /// A Store with a live mount (or another live operation) at the project
    /// scope: `admit` refuses with the same code the Store's admission does.
    fn refusing(mut self, code: &str, detail: SdkValue) -> Self {
        self.refuse = Some(SdkRefusal {
            code: code.to_owned(),
            message: format!("test admission refusal: {code}"),
            detail,
        });
        self
    }

    /// A fake whose admission refuses.
    fn refused(&self) -> bool {
        self.refuse.is_some()
    }
}

impl Effects for Fake {
    fn prepare(&mut self) -> Result<(), SdkRefusal> {
        self.prepared += 1;
        Ok(())
    }

    fn admit(
        &mut self,
        stack: LayerStackId,
        reuse: Option<[u8; 16]>,
    ) -> Result<Admission, SdkRefusal> {
        if self.refused() {
            return Err(self.refuse.clone().expect("refused above"));
        }
        let admission = Admission::new(stack, reuse.unwrap_or([0x5a; 16]));
        self.admitted.push(admission);
        Ok(admission)
    }

    fn release(&mut self, admission: Admission) -> Result<(), SdkRefusal> {
        self.released.push(admission);
        Ok(())
    }

    fn mark(&mut self, admission: Admission, deleted_ms: i64) -> Result<Deletion, SdkRefusal> {
        if let Some(refusal) = &self.mark_refusal {
            return Err(refusal.clone());
        }
        self.seen.push((admission.stack, deleted_ms));
        let state = Intent::read(&self.home, &self.store, &self.project)
            .unwrap()
            .expect("the intent is durable before the marker")
            .state;
        self.states.push(state);
        Ok(self
            .outcomes
            .pop_front()
            .expect("one fake outcome per marker attempt"))
    }

    fn remove_entry(&mut self, entry: &Path) -> Result<(), SdkRefusal> {
        if self.fail_removal {
            return Err(refused("Failed", "test removal failure"));
        }
        match std::fs::remove_dir_all(entry) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(failed(error)),
        }
    }
}

#[test]
fn a_fresh_delete_marks_then_removes_the_entry_and_unlinks_the_intent() {
    let home = home("fresh");
    let (store, project, stack) = ("shop", "app", stack(1));
    project_record(&home, store, project, stack);
    let mut fake = Fake::new(
        &home,
        store,
        project,
        vec![Deletion::Marked { deleted_ms: 777 }],
    );
    let outcome = delete_with(&home, store, project, Some(stack), &mut fake).unwrap();
    assert_eq!(outcome.get_text("kind"), Some("Removed"));
    assert_eq!(outcome.get_text("project"), Some("shop/app"));
    assert_eq!(outcome.get_int("deleted_ms"), Some(777));
    assert!(outcome.get_bool("home_entry_removed"));
    assert_eq!(fake.seen.len(), 1);
    assert_eq!(fake.seen[0].0, stack);
    // the identity save precedes the intent, and the intent precedes the mark
    assert_eq!(fake.prepared, 1);
    assert_eq!(fake.states, [IntentState::Pending]);
    // the project scope was admitted before the marker and released after
    assert_eq!(fake.admitted.len(), 1);
    assert_eq!(fake.released, fake.admitted);
    assert!(!paths::project_dir(&home, store, project).exists());
    assert_eq!(Intent::read(&home, store, project).unwrap(), None);
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn a_refused_admission_writes_no_sidecar_and_leaves_no_intent() {
    let refused_home = home("refused");
    let (store, project, stack) = ("shop", "app", stack(20));
    project_record(&refused_home, store, project, stack);
    let holders = SdkValue::map([(
        "holders",
        SdkValue::List(vec![SdkValue::map([
            ("kind", t("mount")),
            ("entity", t("main")),
        ])]),
    )]);
    let mut fake =
        Fake::new(&refused_home, store, project, Vec::new()).refusing("MountsActive", holders);
    let refusal = delete_with(&refused_home, store, project, Some(stack), &mut fake).unwrap_err();
    // the live mount's refusal is the call's, with its holders
    assert_eq!(refusal.code, "MountsActive");
    assert!(refusal.detail.get("holders").is_some());
    // no identity save, no sidecar, no marker
    assert_eq!(fake.prepared, 0);
    assert!(fake.seen.is_empty(), "{:?}", fake.seen);
    assert_eq!(Intent::read(&refused_home, store, project).unwrap(), None);
    // the admission refused, so there is no admission to release
    assert!(fake.admitted.is_empty());
    assert!(fake.released.is_empty());
    // the home entry and the project's record stay
    assert!(paths::project_dir(&refused_home, store, project).is_dir());
    let _ = std::fs::remove_dir_all(&refused_home);

    // a live operation at the project scope is `Busy`, with the same rule
    let busy_home = home("busy");
    project_record(&busy_home, store, project, stack);
    let conflict = SdkValue::map([
        ("kind", t("removal")),
        ("entity", t("app")),
        ("phase", SdkValue::Null),
        ("op", t("11")),
        ("uncertain", SdkValue::Bool(false)),
    ]);
    let mut fake = Fake::new(&busy_home, store, project, Vec::new()).refusing("Busy", conflict);
    let refusal = delete_with(&busy_home, store, project, Some(stack), &mut fake).unwrap_err();
    assert_eq!(refusal.code, "Busy");
    assert_eq!(fake.prepared, 0);
    assert!(fake.seen.is_empty());
    assert_eq!(Intent::read(&busy_home, store, project).unwrap(), None);
    let _ = std::fs::remove_dir_all(&busy_home);
}

#[test]
fn a_retry_reuses_the_sidecar_admission_identity() {
    let home = home("reuse");
    let (store, project, stack) = ("shop", "app", stack(22));
    project_record(&home, store, project, stack);
    let mut pending = intent(&home, store, project, stack, 111);
    pending.op = Some([3; 16]);
    Intent::write(&home, &pending).unwrap();
    let mut fake = Fake::new(
        &home,
        store,
        project,
        vec![Deletion::Retired { deleted_ms: 111 }],
    );
    let outcome = delete_with(&home, store, project, Some(stack), &mut fake).unwrap();
    // the retry re-registers the recorded identity, then releases it
    assert_eq!(fake.admitted, [Admission::new(stack, [3; 16])]);
    assert_eq!(fake.released, fake.admitted);
    assert_eq!(outcome.get_int("deleted_ms"), Some(111));
    assert_eq!(Intent::read(&home, store, project).unwrap(), None);
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn a_failed_marker_releases_the_admission_it_took() {
    let home = home("marker-refused");
    let (store, project, stack) = ("shop", "app", stack(21));
    project_record(&home, store, project, stack);
    let mut fake = Fake::new(&home, store, project, Vec::new());
    fake.mark_refusal = Some(refused("Busy", "the project is held"));
    let refusal = delete_with(&home, store, project, Some(stack), &mut fake).unwrap_err();
    assert_eq!(refusal.code, "Busy");
    // the admission taken before the sidecar is released on the failure
    assert_eq!(fake.admitted.len(), 1);
    assert_eq!(fake.released, fake.admitted);
    // the sidecar stays `pending` for the retry, and the entry stays
    let left = Intent::read(&home, store, project).unwrap().unwrap();
    assert_eq!(left.state, IntentState::Pending);
    assert!(paths::project_dir(&home, store, project).is_dir());
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn a_retired_retry_reuses_the_intent_deleted_ms_and_finishes_the_entry() {
    let home = home("retired");
    let (store, project, stack) = ("shop", "app", stack(2));
    project_record(&home, store, project, stack);
    Intent::write(&home, &intent(&home, store, project, stack, 111)).unwrap();
    let mut fake = Fake::new(
        &home,
        store,
        project,
        vec![Deletion::Retired { deleted_ms: 222 }],
    );
    let outcome = delete_with(&home, store, project, Some(stack), &mut fake).unwrap();
    assert_eq!(fake.seen, [(stack, 111)]);
    assert_eq!(outcome.get_int("deleted_ms"), Some(222));
    assert!(outcome.get_bool("home_entry_removed"));
    assert!(!paths::project_dir(&home, store, project).exists());
    assert_eq!(Intent::read(&home, store, project).unwrap(), None);
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn a_live_row_with_a_live_intent_redoes_the_marker_then_finishes() {
    let home = home("marked");
    let (store, project, stack) = ("shop", "app", stack(3));
    project_record(&home, store, project, stack);
    Intent::write(&home, &intent(&home, store, project, stack, 111)).unwrap();
    let mut fake = Fake::new(
        &home,
        store,
        project,
        vec![Deletion::Marked { deleted_ms: 111 }],
    );
    let outcome = delete_with(&home, store, project, Some(stack), &mut fake).unwrap();
    // the live row has no marker, so the marker is redone with the same ms
    assert_eq!(fake.seen, [(stack, 111)]);
    assert_eq!(fake.states, [IntentState::Pending]);
    assert_eq!(outcome.get_int("deleted_ms"), Some(111));
    assert!(outcome.get_bool("home_entry_removed"));
    assert!(!paths::project_dir(&home, store, project).exists());
    assert_eq!(Intent::read(&home, store, project).unwrap(), None);
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn an_absent_stack_finishes_a_stale_entry() {
    let home = home("absent");
    let (store, project, stack) = ("shop", "app", stack(4));
    project_record(&home, store, project, stack);
    Intent::write(&home, &intent(&home, store, project, stack, 111)).unwrap();
    let mut fake = Fake::new(&home, store, project, vec![Deletion::Absent]);
    let outcome = delete_with(&home, store, project, Some(stack), &mut fake).unwrap();
    assert_eq!(fake.seen, [(stack, 111)]);
    assert_eq!(outcome.get_int("deleted_ms"), Some(111));
    assert!(outcome.get_bool("home_entry_removed"));
    assert!(!paths::project_dir(&home, store, project).exists());
    assert_eq!(Intent::read(&home, store, project).unwrap(), None);
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn a_hidden_store_project_still_has_its_stale_entry_removed() {
    let home = home("hidden");
    let (store, project, stack) = ("shop", "app", stack(5));
    project_record(&home, store, project, stack);
    // The Store listing filters the retired project out; the Store side
    // classifies the retry by the recorded stack id as Retired.
    let mut fake = Fake::new(
        &home,
        store,
        project,
        vec![Deletion::Retired { deleted_ms: 333 }],
    );
    let outcome = delete_with(&home, store, project, Some(stack), &mut fake).unwrap();
    assert_eq!(outcome.get_int("deleted_ms"), Some(333));
    assert!(outcome.get_bool("home_entry_removed"));
    assert!(!paths::project_dir(&home, store, project).exists());
    assert_eq!(Intent::read(&home, store, project).unwrap(), None);
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn a_home_removal_failure_is_success_false_and_leaves_a_marked_intent() {
    let home = home("residue");
    let (store, project, stack) = ("shop", "app", stack(6));
    project_record(&home, store, project, stack);
    let mut fake = Fake::new(
        &home,
        store,
        project,
        vec![Deletion::Marked { deleted_ms: 44 }],
    )
    .failing();
    let outcome = delete_with(&home, store, project, Some(stack), &mut fake).unwrap();
    assert_eq!(outcome.get_text("kind"), Some("Removed"));
    assert_eq!(outcome.get_int("deleted_ms"), Some(44));
    assert!(!outcome.get_bool("home_entry_removed"));
    // the marker is the commit point; the entry and intent stay for the retry
    assert!(paths::project_dir(&home, store, project).is_dir());
    let left = Intent::read(&home, store, project).unwrap().unwrap();
    assert_eq!(left.state, IntentState::Marked);
    assert_eq!(left.deleted_ms, 44);

    // a same-home retry reuses the stored ms, finishes the entry and returns
    // `home_entry_removed: true`
    let mut retry = Fake::new(
        &home,
        store,
        project,
        vec![Deletion::Retired { deleted_ms: 44 }],
    );
    let outcome = delete_with(&home, store, project, Some(stack), &mut retry).unwrap();
    assert_eq!(retry.seen, [(stack, 44)]);
    assert_eq!(outcome.get_int("deleted_ms"), Some(44));
    assert!(outcome.get_bool("home_entry_removed"));
    assert!(!paths::project_dir(&home, store, project).exists());
    assert_eq!(Intent::read(&home, store, project).unwrap(), None);
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn a_done_intent_settles_the_entry_without_a_second_marker() {
    let home = home("done");
    let (store, project, stack) = ("shop", "app", stack(7));
    project_record(&home, store, project, stack);
    let mut done = intent(&home, store, project, stack, 44);
    done.state = IntentState::Done;
    Intent::write(&home, &done).unwrap();
    let mut fake = Fake::new(&home, store, project, Vec::new());
    let outcome = delete_with(&home, store, project, Some(stack), &mut fake).unwrap();
    assert!(fake.seen.is_empty(), "a done intent marks nothing");
    assert_eq!(outcome.get_int("deleted_ms"), Some(44));
    assert!(outcome.get_bool("home_entry_removed"));
    assert_eq!(Intent::read(&home, store, project).unwrap(), None);
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn a_recreated_project_discards_the_stale_intent_and_starts_over() {
    let home = home("recreated");
    let (store, project, old, new) = ("shop", "app", stack(8), stack(9));
    project_record(&home, store, project, new);
    Intent::write(&home, &intent(&home, store, project, old, 111)).unwrap();
    let mut fake = Fake::new(
        &home,
        store,
        project,
        vec![Deletion::Marked { deleted_ms: 555 }],
    );
    let outcome = delete_with(&home, store, project, Some(new), &mut fake).unwrap();
    assert_eq!(fake.seen.len(), 1);
    assert_eq!(fake.seen[0].0, new, "the record that is there now wins");
    assert_ne!(fake.seen[0].1, 111, "the stale deleted_ms is not reused");
    assert_eq!(outcome.get_int("deleted_ms"), Some(555));
    assert!(outcome.get_bool("home_entry_removed"));
    assert_eq!(Intent::read(&home, store, project).unwrap(), None);
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn an_untracked_record_with_no_stack_is_removed_without_a_marker() {
    let home = home("untracked");
    let (store, project) = ("shop", "app");
    write_record(
        &paths::project_record(&home, store, project),
        &SdkValue::map([("v", SdkValue::Int(1)), ("name", t(project))]),
    )
    .unwrap();
    let mut fake = Fake::new(&home, store, project, Vec::new());
    let outcome = delete_with(&home, store, project, None, &mut fake).unwrap();
    assert!(fake.seen.is_empty());
    assert_eq!(fake.prepared, 1, "a legacy identity is saved first");
    assert!(outcome.get_bool("home_entry_removed"));
    assert!(!paths::project_dir(&home, store, project).exists());
    let _ = std::fs::remove_dir_all(&home);

    // no record and no intent is NoSuchProject
    let refusal = delete_with(&home, "shop", "gone", None, &mut fake).unwrap_err();
    assert_eq!(refusal.code, "NoSuchProject");
}

#[test]
fn the_intent_moves_pending_marked_done_and_unlinks() {
    let home = home("intent");
    let (store, project, stack) = ("shop", "app", stack(10));
    let mut value = intent(&home, store, project, stack, 5);
    Intent::write(&home, &value).unwrap();
    let path = Intent::path(&home, store, project);
    assert!(path.is_file());
    assert_eq!(
        Intent::read(&home, store, project).unwrap().unwrap().state,
        IntentState::Pending
    );
    // no temporary file is left beside it
    let mut names: Vec<String> = std::fs::read_dir(path.parent().unwrap())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    assert_eq!(names, [format!("{project}.json")]);

    value.state = IntentState::Marked;
    value.deleted_ms = 6;
    value.op = Some([4; 16]);
    Intent::write(&home, &value).unwrap();
    let read = Intent::read(&home, store, project).unwrap().unwrap();
    assert_eq!((read.state, read.deleted_ms), (IntentState::Marked, 6));
    assert_eq!(read.op, Some([4; 16]), "the admission identity persists");

    value.state = IntentState::Done;
    Intent::write(&home, &value).unwrap();
    assert_eq!(
        Intent::read(&home, store, project).unwrap().unwrap().state,
        IntentState::Done
    );
    Intent::unlink(&home, store, project).unwrap();
    assert_eq!(Intent::read(&home, store, project).unwrap(), None);
    // unlinking an absent intent is not an error
    Intent::unlink(&home, store, project).unwrap();
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn the_worker_removes_only_its_entry_for_the_recorded_stack() {
    let root = home("worker-entry");
    let (store, first) = ("shop", stack(11));
    project_record(&root, store, "app", first);
    project_record(&root, store, "other", stack(12));
    // the hint names the caller's project; the other entry stays
    let removed = remove_worker_entry(&root, store, first, Some("app")).unwrap();
    assert_eq!(removed.as_deref(), Some("app"));
    assert!(!paths::project_dir(&root, store, "app").exists());
    assert!(paths::project_dir(&root, store, "other").is_dir());
    // without a hint the recorded stack id finds the entry
    let removed = remove_worker_entry(&root, store, stack(12), None).unwrap();
    assert_eq!(removed.as_deref(), Some("other"));
    assert!(!paths::project_dir(&root, store, "other").exists());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_worker_reply_names_the_stack_and_deleted_ms() {
    use layerfs_store::project::frame::DeleteReply;
    let first = stack(13);
    let reply = DeleteReply {
        project: "app".to_owned(),
        stack: first,
        deleted_ms: 42,
    };
    let encoded = reply.encode();
    assert_eq!(encoded.get_text("kind"), Some("Removed"));
    assert_eq!(
        encoded.get_text("stack_id"),
        Some(hex(&first.to_bytes()).as_str())
    );
    assert_eq!(
        DeleteReply::decode(&encoded).unwrap(),
        reply,
        "the worker reply round-trips"
    );
    // a reply for a different Stack is detectable
    let other = DeleteReply {
        stack: stack(14),
        ..reply.clone()
    };
    assert_ne!(DeleteReply::decode(&other.encode()).unwrap().stack, first);
}
