//! A Sandbox's record, `sandbox/<name>/record.json` (layout 3, #536 §5):
//! facts and at most one `op` intent (§7.1). A create claims the name with
//! its project, machine and an `op` (`sandbox.create`); its success records
//! the backend, kernel route, runtime id, platform, runtime and image and
//! drops the `op` in one write. A close records its `op` (`sandbox.close`)
//! before it stops the runtime. On `native` the side that runs the Sandbox
//! adds what reopens it (`daemon`) to the same file; its daemon keys are in
//! `control.json` beside it. The name is the folder's: one read finds it.
//! No state is stored: [`SandboxRecord::state`] derives it (§7.2).
use crate::common::clock;
use crate::common::home::entity_lock::{self, EntityLock};
use crate::common::home::{self, paths, read_record, with_field, write_entity};
use crate::common::op::{self, Op, Standing};
use crate::common::outcome::{refused, View};
use crate::sandbox::lifecycle;
use crate::sandbox::scratch::{self, Scratch};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, SdkRefusal>;

/// Fields a record keeps that its call results never show as written: what
/// only reopens it, and the intent (shown as [`Op::to_sdk`]).
const HIDDEN: [&str; 5] = ["v", "runtime_id", "daemon", "state", op::FIELD];

/// Takes Sandbox `name`'s lock, `sandbox/<name>/lock` (#536 §8): it guards
/// the Sandbox's record changes and its Workspace records, taken after the
/// home lock and never held across a worker or daemon call.
pub(crate) fn lock(home: &Path, name: &str) -> Result<EntityLock> {
    lock_in(&paths::sandbox_dir(home, name), name)
}

/// [`lock`] of the Sandbox folder `dir` (on `native` the home's, on a
/// machine the worker's own), named `name` in its refusal.
pub(crate) fn lock_in(dir: &Path, name: &str) -> Result<EntityLock> {
    let held = format!("Sandbox {name}'s record was being changed by another process");
    entity_lock::take(&dir.join("lock"), entity_lock::WAIT, &held)
}

/// One Sandbox's record as read.
#[derive(Clone, Debug)]
pub struct SandboxRecord {
    name: String,
    fields: SdkValue,
}

impl SandboxRecord {
    /// Sandbox `name`'s record `value`.
    pub fn from_record(name: &str, value: SdkValue) -> Self {
        Self {
            name: name.to_owned(),
            fields: value,
        }
    }

    /// Sandbox `name`'s record, if it has one.
    pub(crate) fn read(home: &Path, name: &str) -> Result<Option<Self>> {
        let path = paths::sandbox_record(home, name);
        Ok(read_record(&path)?.map(|value| Self::from_record(name, value)))
    }

    /// The record a create claims the name with: the project, the machine,
    /// now and the create's intent `op`.
    pub(crate) fn creating(name: &str, project: &str, machine: &str, op: &Op) -> Self {
        let fields = SdkValue::map([
            ("name", SdkValue::text(name)),
            ("project", SdkValue::text(project)),
            ("machine", SdkValue::text(machine)),
            ("created", SdkValue::Int(home::now_ms())),
            (lifecycle::GENERATION, SdkValue::Int(1)),
            (
                lifecycle::LAST_ACTIVITY,
                SdkValue::Int(clock::monotonic_ms()),
            ),
            (op::FIELD, op.to_record().clone()),
        ]);
        Self::from_record(name, fields)
    }

    /// The record of a started Sandbox: what its create answered
    /// (`started`), with its intent dropped.
    pub(crate) fn started(self, started: &SdkValue) -> Self {
        let mut record = self.without(op::FIELD);
        let copied = |name: &str| started.get(name).cloned().unwrap_or_default();
        for (field, value) in [
            ("id", copied("sandbox")),
            ("platform", copied("platform")),
            ("runtime", copied("runtime")),
            ("image", copied("image")),
            ("backend", copied("runtime")),
            ("route", copied("route")),
            ("runtime_id", copied("runtime_id")),
        ] {
            record = record.with(field, value);
        }
        // the scratch it owns (#549): only an equal generation may remove it
        if let Some(scratch) = started.get(scratch::FIELD) {
            if !matches!(scratch, SdkValue::Null) {
                record = record.with(scratch::FIELD, scratch.clone());
            }
        }
        record
    }

    /// The intent the record holds, if any.
    pub fn op(&self) -> Option<Op> {
        Op::of(&self.fields)
    }

    /// The record holding intent `op`.
    pub(crate) fn with_op(self, op: &Op) -> Self {
        self.with(op::FIELD, op.to_record().clone())
    }

    /// The record with no intent.
    pub(crate) fn without_op(self) -> Self {
        self.without(op::FIELD)
    }

    /// Refuses a new operation on this Sandbox while its intent holds it:
    /// `Busy` while its owner lives, `Unresolved` (naming `sandbox get` and
    /// `sandbox close`) once a gone owner left it uncertain.
    pub(crate) fn free(&self) -> Result<()> {
        match self.op() {
            None => Ok(()),
            // a close that died mid-drain leaves an uncertain outcome even
            // when its intent predates the uncertain bit; only a close clears it
            Some(op) if self.close_crashed() => Err(unresolved_close(&self.name, &op)),
            Some(op) => op.blocks("sandbox", &self.name, "sandbox get", "sandbox close"),
        }
    }

    /// `field` set to `value`.
    pub(crate) fn with(self, field: &str, value: SdkValue) -> Self {
        let fields = with_field(self.fields, field, value);
        Self { fields, ..self }
    }

    /// The Sandbox's folder in the home.
    pub(crate) fn dir(&self, home: &Path) -> PathBuf {
        paths::sandbox_dir(home, &self.name)
    }

    /// Writes the record into its folder under `home`, with its version.
    pub(crate) fn write(&self, home: &Path) -> Result<()> {
        write_entity(&paths::sandbox_record(home, &self.name), &self.fields)
    }

    /// The Sandbox's name (its folder's).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The scratch this Sandbox owns, if its start recorded one (#549):
    /// the absolute root, its `createdAt` and the generation a cleaner
    /// re-checks before deleting.
    pub fn scratch(&self) -> Result<Option<Scratch>> {
        Scratch::of(&self.fields)
    }

    /// The machine it runs on, as recorded.
    pub fn machine(&self) -> &str {
        self.fields.get_text("machine").unwrap_or("")
    }

    /// Its project, `<store>/<project>`, as recorded.
    pub fn project(&self) -> Option<&str> {
        self.fields.get_text("project")
    }

    /// Its id once started: the name its machine knows it by.
    pub fn id(&self) -> Option<&str> {
        self.fields.get_text("id")
    }

    /// The image it runs, as recorded.
    pub fn image(&self) -> Option<&str> {
        self.fields.get_text("image")
    }

    /// The Sandbox's state (#548), from its record, its `op` and, where the
    /// call asked, the daemon's answer (`gone`: its machine answered that no
    /// such Sandbox runs). [`lifecycle::sandbox_state`] is the only mapping
    /// from those facts. A record written before layout 3 that still carries
    /// an explicit `state` is read as the enum it names; an absent one
    /// derives from the facts exactly as before, now as
    /// [`lifecycle::SandboxState`].
    pub fn state(&self, gone: bool) -> lifecycle::SandboxState {
        let op = self.op();
        lifecycle::sandbox_state(lifecycle::SandboxFacts {
            recorded: lifecycle::recorded_state(&self.fields),
            lost: self.lost(),
            close_crashed: self.close_crashed(),
            op: op.as_ref().map(|op| (op.standing(), op.name(), op.phase())),
            gone,
            started: self.id().is_some(),
            live_leases: self.live_leases(home::now_ms()).len(),
        })
    }

    /// Its generation (#548): `1` for a record written before generations.
    pub fn generation(&self) -> u64 {
        lifecycle::generation_of(&self.fields)
    }

    /// The record at `generation`, the CAS write a close makes.
    pub(crate) fn with_generation(self, generation: u64) -> Self {
        self.with(
            lifecycle::GENERATION,
            SdkValue::Int(i64::try_from(generation).unwrap_or(i64::MAX)),
        )
    }

    /// The exec leases that have not expired at `now`.
    pub(crate) fn live_leases(&self, now: i64) -> Vec<lifecycle::LeaseRecord> {
        lifecycle::leases_of(&self.fields)
            .into_iter()
            .filter(|lease| lease.deadline > now)
            .collect()
    }

    /// The exec leases that expired at or before `now`.
    pub(crate) fn expired_leases(&self, now: i64) -> Vec<lifecycle::LeaseRecord> {
        lifecycle::leases_of(&self.fields)
            .into_iter()
            .filter(|lease| lease.deadline <= now)
            .collect()
    }

    /// Whether the record holds lease `id`.
    pub(crate) fn has_lease(&self, id: &str) -> bool {
        lifecycle::leases_of(&self.fields)
            .iter()
            .any(|lease| lease.id == id)
    }

    /// The record holding exec lease `id` until `deadline`, with `activity`
    /// as its last activity.
    pub(crate) fn with_exec_lease(self, id: &str, deadline: i64, activity: i64) -> Self {
        Self {
            fields: lifecycle::with_activity(
                lifecycle::with_lease(self.fields, id, deadline),
                activity,
            ),
            ..self
        }
    }

    /// The record without exec lease `id`.
    pub(crate) fn without_exec_lease(self, id: &str) -> Self {
        Self {
            fields: lifecycle::without_lease(self.fields, id),
            ..self
        }
    }

    /// The record with its last activity from the monotonic clock.
    pub(crate) fn with_activity(self, monotonic_ms: i64) -> Self {
        Self {
            fields: lifecycle::with_activity(self.fields, monotonic_ms),
            ..self
        }
    }

    /// Whether this Sandbox is `Lost`.
    pub(crate) fn lost(&self) -> bool {
        lifecycle::is_lost(&self.fields)
    }

    /// The record marked `Lost`, with no intent or lease.
    pub(crate) fn with_lost(self, reason: &str, monotonic_ms: i64) -> Self {
        Self {
            fields: lifecycle::with_lost(self.fields, reason, monotonic_ms),
            ..self
        }
    }

    /// The record with the reason its close records.
    pub(crate) fn with_close_reason(self, reason: &str) -> Self {
        self.with(lifecycle::CLOSE_REASON, SdkValue::text(reason))
    }

    /// The record without a close reason (a close that failed certainly
    /// leaves the Sandbox as it was).
    pub(crate) fn without_close_reason(self) -> Self {
        self.without(lifecycle::CLOSE_REASON)
    }

    /// Whether a close entered `Draining` and then its process ended before
    /// it settled the runtime. Such a record is `Unresolved`.
    fn close_crashed(&self) -> bool {
        self.op().is_some_and(|op| {
            op.name() == "sandbox.close"
                && op.phase() == "draining"
                && op.standing() != Standing::Running
        })
    }

    /// Whether a close has won its generation CAS and is draining.
    pub(crate) fn draining(&self) -> bool {
        matches!(
            self.op().as_ref().map(|op| (op.name(), op.phase())),
            Some(("sandbox.close", "draining"))
        )
    }

    /// The record as written.
    pub fn to_record(&self) -> &SdkValue {
        &self.fields
    }

    /// The Sandbox as each `sandbox.list` item and the start of
    /// `sandbox.get` show it: its record without what only reopens it
    /// (`v`, `runtime_id`, `daemon`), its derived `state` and its `op`
    /// (H6) when it has one. Both views show the same fields.
    pub fn to_sdk(&self, view: View) -> SdkValue {
        let _ = view;
        let SdkValue::Map(fields) = &self.fields else {
            return self.fields.clone();
        };
        let mut shown: Vec<_> = (fields.iter())
            .filter(|(n, _)| !HIDDEN.contains(&n.as_str()) && n != lifecycle::GENERATION)
            .cloned()
            .collect();
        // a record written before generations still reads at 1; the read
        // model shows it in one place, next to `state`
        shown.push((
            lifecycle::GENERATION.to_owned(),
            SdkValue::Int(i64::try_from(self.generation()).unwrap_or(i64::MAX)),
        ));
        shown.push((
            "state".to_owned(),
            SdkValue::text(self.state(false).as_str()),
        ));
        if let Some(op) = self.op() {
            shown.push((op::FIELD.to_owned(), op.to_sdk()));
        }
        SdkValue::Map(shown)
    }

    /// The record without `field`.
    fn without(self, field: &str) -> Self {
        let fields = match self.fields {
            SdkValue::Map(mut fields) => {
                fields.retain(|(name, _)| name != field);
                SdkValue::Map(fields)
            }
            other => other,
        };
        Self { fields, ..self }
    }
}

/// The refusal of a Sandbox whose close died after it entered `Draining`.
fn unresolved_close(name: &str, op: &Op) -> SdkRefusal {
    let mut refusal = refused(
        "Unresolved",
        format!(
            "Sandbox {name} has an unresolved sandbox.close ({}, phase draining): its outcome \
             is not known; see `layerfs sandbox get {name}`, then clear it with `layerfs \
             sandbox close {name}`",
            op.id()
        ),
    );
    refusal.detail = SdkValue::map([
        (
            "entity",
            SdkValue::map([
                ("kind", SdkValue::text("sandbox")),
                ("name", SdkValue::text(name)),
            ]),
        ),
        ("phase", SdkValue::text("draining")),
        ("op", SdkValue::text(op.id())),
    ]);
    refusal
}
