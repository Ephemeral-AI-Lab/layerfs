//! `project.delete`'s home path (T4.5b): the durable intent before the marker,
//! the retry classifications (`Marked`, `Retired`, `Absent`, a hidden Store
//! project), the `home_entry_removed` echo, and the worker's own entry. A fake
//! `Effects` stands in for the Store so the home state machine is tested
//! without opening one.
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_bridge::sdk_values::hex;
use layerfs_history::LayerStackId;
use layerfs_home::home::{paths, write_record};
use layerfs_home::names::StoreName;
use layerfs_home::outcome::{failed, refused};
use layerfs_store::project::delete::{
    delete_with, remove_project_entry, remove_worker_entry, Admission, Effects, Intent, IntentState,
};
use layerfs_store::project::frame::DeleteEntryFrame;
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
    fail_removal: bool,
    /// What `admit` refuses with, before registering anything.
    refuse: Option<SdkRefusal>,
    /// What `mark` refuses with, before writing anything.
    mark_refusal: Option<SdkRefusal>,
    /// Admissions taken, in order.
    admitted: Vec<Admission>,
    /// The project name each admission carried, in order.
    projects: Vec<String>,
    /// Admissions released, in order.
    released: Vec<Admission>,
    /// The `pending_retry` flag each marker attempt carried, in order.
    pending_retries: Vec<bool>,
    /// The Stacks whose worker-side entry a `marked` sidecar asked to remove.
    settled: Vec<LayerStackId>,
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
            fail_removal: false,
            refuse: None,
            mark_refusal: None,
            admitted: Vec::new(),
            projects: Vec::new(),
            released: Vec::new(),
            pending_retries: Vec::new(),
            settled: Vec::new(),
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
    fn admit(
        &mut self,
        stack: LayerStackId,
        op: [u8; 16],
        project: &str,
    ) -> Result<Admission, SdkRefusal> {
        if self.refused() {
            return Err(self.refuse.clone().expect("refused above"));
        }
        // The admission row is stored: it must name the project, so a later
        // caller meeting it reports `{kind: "project", name: <project>}`.
        self.projects.push(project.to_owned());
        // The sidecar names this admission and is durable before the row is
        // taken, so a crash between the row and its reply leaves it for the
        // retry.
        let pending = Intent::read(&self.home, &self.store, &self.project)
            .unwrap()
            .expect("the sidecar precedes the admission");
        assert_eq!(pending.op, Some(op), "the sidecar names this admission");
        let admission = Admission::new(stack, op);
        self.admitted.push(admission);
        Ok(admission)
    }

    fn release(&mut self, admission: Admission) -> Result<(), SdkRefusal> {
        self.released.push(admission);
        Ok(())
    }

    fn mark(
        &mut self,
        admission: Admission,
        deleted_ms: i64,
        pending_retry: bool,
    ) -> Result<Deletion, SdkRefusal> {
        if let Some(refusal) = &self.mark_refusal {
            return Err(refusal.clone());
        }
        self.pending_retries.push(pending_retry);
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

    fn settle(&mut self, stack: LayerStackId) -> Result<(), SdkRefusal> {
        self.settled.push(stack);
        Ok(())
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
    assert_eq!(fake.pending_retries, [false], "a fresh delete is no retry");
    // the identity save precedes the intent, and the intent precedes the mark
    assert_eq!(fake.states, [IntentState::Pending]);
    // the project scope was admitted after the sidecar and released after
    assert_eq!(fake.admitted.len(), 1);
    assert_eq!(
        fake.projects,
        ["app".to_owned()],
        "the admission names the project, not the Stack id"
    );
    assert_eq!(fake.released, fake.admitted);
    assert!(!paths::project_dir(&home, store, project).exists());
    assert_eq!(Intent::read(&home, store, project).unwrap(), None);
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn a_refused_admission_keeps_a_pending_sidecar_and_writes_no_marker() {
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
    // the identity save and the sidecar precede the admission; no marker
    assert!(fake.seen.is_empty(), "{:?}", fake.seen);
    let pending = Intent::read(&refused_home, store, project)
        .unwrap()
        .expect("the pending sidecar precedes the admission");
    assert_eq!(pending.state, IntentState::Pending);
    let op = pending.op.expect("the sidecar names its admission");
    // the refused admission registered nothing; the caller settles the
    // identity it wrote without adopting a row
    assert!(fake.admitted.is_empty());
    assert_eq!(fake.released, [Admission::new(stack, op)]);
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
    assert!(fake.seen.is_empty());
    assert_eq!(
        Intent::read(&busy_home, store, project)
            .unwrap()
            .expect("the pending sidecar stays")
            .state,
        IntentState::Pending
    );
    assert!(paths::project_dir(&busy_home, store, project).is_dir());
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
    // the admission taken after the sidecar is released on the failure
    assert_eq!(fake.admitted.len(), 1);
    assert_eq!(fake.released, fake.admitted);
    // the sidecar stays `pending`, naming the admission, and the entry stays
    let left = Intent::read(&home, store, project).unwrap().unwrap();
    assert_eq!(left.state, IntentState::Pending);
    assert!(left.op.is_some(), "the sidecar names the admission");
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
    assert_eq!(fake.pending_retries, [true], "a pending sidecar is a retry");
    assert_eq!(outcome.get_int("deleted_ms"), Some(222));
    assert!(
        outcome.get_bool("home_entry_removed"),
        "the marker was already there"
    );
    assert!(!paths::project_dir(&home, store, project).exists());
    assert_eq!(Intent::read(&home, store, project).unwrap(), None);
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn a_live_row_with_a_pending_intent_settles_the_marker_and_keeps_the_entry() {
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
    // The sidecar was written before an earlier admission, the row is still
    // live and there is no marker: the retry redoes the marker with the same
    // deleted_ms, settles the sidecar and keeps the entry.
    assert_eq!(fake.seen, [(stack, 111)]);
    assert_eq!(fake.states, [IntentState::Pending]);
    assert_eq!(fake.pending_retries, [true]);
    assert!(fake.admitted[0].op.iter().any(|byte| *byte != 0));
    assert_eq!(outcome.get_int("deleted_ms"), Some(111));
    assert!(!outcome.get_bool("home_entry_removed"));
    assert!(
        paths::project_dir(&home, store, project).is_dir(),
        "the entry stays"
    );
    let settled = Intent::read(&home, store, project)
        .unwrap()
        .expect("the sidecar is marked for the finishing call");
    assert_eq!(settled.state, IntentState::Marked);
    assert_eq!(settled.deleted_ms, 111);

    // The marker is confirmed now: the next call finds a `marked` sidecar,
    // takes no admission and writes no marker, and only finishes the entry.
    let mut finish = Fake::new(&home, store, project, Vec::new());
    let outcome = delete_with(&home, store, project, Some(stack), &mut finish).unwrap();
    assert!(finish.seen.is_empty(), "a marked sidecar marks nothing");
    assert!(
        finish.admitted.is_empty(),
        "a marked sidecar admits nothing"
    );
    assert_eq!(finish.settled, [stack]);
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
fn a_fresh_delete_of_a_retired_project_is_no_such_project() {
    let home = home("retired-fresh");
    let (store, project, stack) = ("shop", "app", stack(5));
    project_record(&home, store, project, stack);
    // Another home holds the stale record but not the sidecar of the delete
    // that retired the Stack: the record's Stack id is not a live project.
    // The local route resolves the Store's listing, so it passes no Stack.
    let mut fake = Fake::new(
        &home,
        store,
        project,
        vec![Deletion::Retired { deleted_ms: 333 }],
    );
    let refusal = delete_with(&home, store, project, None, &mut fake).unwrap_err();
    assert_eq!(refusal.code, "NoSuchProject");
    assert!(
        fake.seen.is_empty(),
        "nothing is marked for a missing project"
    );
    assert_eq!(Intent::read(&home, store, project).unwrap(), None);
    assert!(paths::project_dir(&home, store, project).is_dir());

    // A caller that hands over the record's Stack anyway (a worker route
    // cannot list) is trusted only by the marker: `Retired` without a
    // durable intent is the same `NoSuchProject`, with no entry removed.
    let mut marked = Fake::new(
        &home,
        store,
        project,
        vec![Deletion::Retired { deleted_ms: 333 }],
    );
    let refusal = delete_with(&home, store, project, Some(stack), &mut marked).unwrap_err();
    assert_eq!(refusal.code, "NoSuchProject");
    assert_eq!(marked.seen.len(), 1);
    assert_eq!(marked.seen[0].0, stack);
    assert_eq!(Intent::read(&home, store, project).unwrap(), None);
    assert!(paths::project_dir(&home, store, project).is_dir());

    // The same-home retry has the durable sidecar: it still removes the
    // stale entry of a Store project that no longer lists the name.
    Intent::write(&home, &intent(&home, store, project, stack, 333)).unwrap();
    let mut retry = Fake::new(
        &home,
        store,
        project,
        vec![Deletion::Retired { deleted_ms: 333 }],
    );
    let outcome = delete_with(&home, store, project, None, &mut retry).unwrap();
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

    // a same-home retry of the marked sidecar takes no admission and writes
    // no marker: it removes the entry and returns `home_entry_removed: true`
    let mut retry = Fake::new(&home, store, project, Vec::new());
    let outcome = delete_with(&home, store, project, Some(stack), &mut retry).unwrap();
    assert!(retry.seen.is_empty(), "a marked sidecar marks nothing");
    assert!(retry.admitted.is_empty(), "a marked sidecar admits nothing");
    assert_eq!(retry.settled, [stack], "the worker entry is settled once");
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
    assert!(fake.settled.is_empty(), "a done intent makes no Store call");
    assert_eq!(outcome.get_int("deleted_ms"), Some(44));
    assert!(outcome.get_bool("home_entry_removed"));
    assert_eq!(Intent::read(&home, store, project).unwrap(), None);
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn a_stale_intent_does_not_delete_a_project_the_listing_names_again() {
    let home = home("recreated-listed");
    let (store, project, old, new) = ("shop", "app", stack(8), stack(9));
    // The re-created project has no readable home record yet (a partial
    // creation or a stale entry), but its Store lists the new Stack: the
    // listing is enough to see that the sidecar is stale. The stale intent
    // goes, and the call is `NoSuchProject`: the re-created project is not
    // deleted by it.
    Intent::write(&home, &intent(&home, store, project, old, 111)).unwrap();
    let mut fake = Fake::new(&home, store, project, Vec::new());
    let refusal = delete_with(&home, store, project, Some(new), &mut fake).unwrap_err();
    assert_eq!(refusal.code, "NoSuchProject", "{refusal:?}");
    assert!(fake.seen.is_empty(), "a stale intent marks nothing");
    assert!(fake.admitted.is_empty(), "a stale intent admits nothing");
    assert_eq!(Intent::read(&home, store, project).unwrap(), None);
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn a_recreated_project_discards_the_stale_intent_and_keeps_its_entry() {
    let home = home("recreated");
    let (store, project, old, new) = ("shop", "app", stack(8), stack(9));
    project_record(&home, store, project, new);
    Intent::write(&home, &intent(&home, store, project, old, 111)).unwrap();
    let mut fake = Fake::new(&home, store, project, Vec::new());
    let refusal = delete_with(&home, store, project, Some(new), &mut fake).unwrap_err();
    assert_eq!(refusal.code, "NoSuchProject", "{refusal:?}");
    assert!(
        fake.seen.is_empty(),
        "the record that is there now is not marked"
    );
    assert!(fake.admitted.is_empty());
    assert!(
        paths::project_dir(&home, store, project).is_dir(),
        "the re-created project's entry stays"
    );
    assert_eq!(Intent::read(&home, store, project).unwrap(), None);
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn an_untracked_record_with_no_stack_is_no_such_project() {
    let home = home("untracked");
    let (store, project) = ("shop", "app");
    write_record(
        &paths::project_record(&home, store, project),
        &SdkValue::map([("v", SdkValue::Int(1)), ("name", t(project))]),
    )
    .unwrap();
    let mut fake = Fake::new(&home, store, project, Vec::new());
    // Nothing lists the record and there is no sidecar: no project, and no
    // legacy identity is saved for one that is gone.
    let refusal = delete_with(&home, store, project, None, &mut fake).unwrap_err();
    assert_eq!(refusal.code, "NoSuchProject");
    assert!(fake.seen.is_empty());
    assert!(paths::project_dir(&home, store, project).is_dir());

    // no record and no intent is NoSuchProject too
    let refusal = delete_with(&home, "shop", "gone", None, &mut fake).unwrap_err();
    assert_eq!(refusal.code, "NoSuchProject");
    let _ = std::fs::remove_dir_all(&home);
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
fn the_entry_frame_removes_only_the_recorded_entry() {
    let root = home("worker-entry-frame");
    let (store, first) = ("shop", stack(13));
    project_record(&root, store, "app", first);
    project_record(&root, store, "other", stack(14));
    let frame = DeleteEntryFrame {
        store: StoreName::parse(store).unwrap(),
        stack: first,
        project: Some("app".to_owned()),
    };
    let answer = remove_project_entry(&root, &frame).unwrap();
    assert_eq!(answer.get_text("kind"), Some("Removed"));
    assert_eq!(answer.get_text("project"), Some("app"));
    assert!(!paths::project_dir(&root, store, "app").exists());
    assert!(paths::project_dir(&root, store, "other").is_dir());
    // a retry finds the entry gone and answers the same, idempotently
    let answer = remove_project_entry(&root, &frame).unwrap();
    assert_eq!(answer.get_text("project"), Some("app"));
    assert!(
        paths::project_dir(&root, store, "other").is_dir(),
        "the other entry is untouched"
    );
    let _ = std::fs::remove_dir_all(&root);
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
        retired: Some(true),
    };
    let encoded = reply.encode();
    assert_eq!(encoded.get_text("kind"), Some("Removed"));
    assert!(encoded.get_bool("retired"));
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

/// F43R5-2: `project.delete` resolves a missing Stack id from the Store's
/// listing. A listing that cannot be determined is a bare `UnknownOutcome`
/// (empty-map detail) or a poisoned Store object (null detail); both must
/// name the project, never stay a bare `Unknown {}`.
#[test]
fn a_listing_unknown_names_the_project() {
    use layerfs_history::HistoryError;
    use layerfs_store::project::delete::listed_unknown;
    use layerfs_store_service::history::lookup::published;
    use layerfs_store_service::store::PortError;
    use std::sync::Arc;

    for refusal in [
        published(Arc::new(PortError::History(HistoryError::UnknownOutcome))),
        published(Arc::new(PortError::Poisoned)),
    ] {
        assert_eq!(refusal.code, "Unknown", "{refusal:?}");
        let refusal = listed_unknown(refusal, "shop");
        assert_eq!(refusal.code, "Unknown", "{refusal:?}");
        let entity = refusal.detail.get("entity").expect("entity");
        assert_eq!(entity.get_text("kind"), Some("project"), "{refusal:?}");
        assert_eq!(entity.get_text("name"), Some("shop"), "{refusal:?}");
        assert!(
            refusal.detail.get("phase").expect("phase") == &SdkValue::Null,
            "{refusal:?}"
        );
        assert!(
            refusal.detail.get("op").expect("op") == &SdkValue::Null,
            "{refusal:?}"
        );
    }
}
