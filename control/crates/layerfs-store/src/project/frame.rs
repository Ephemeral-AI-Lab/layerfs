//! The `project` worker frames (layerfs-project#109): `project.init`,
//! `project.list` (T4.5c), `project.stage`,
//! `project.export`, `project.delete` (T4.5b) and its admission ops, each
//! encoded by the caller and
//! decoded once in the worker, in the order the worker always refused them.
//! The worker sets the home to its own root; a frame names none.
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_bridge::sdk_values::{hex, unhex};
use layerfs_history::LayerStackId;
use layerfs_home::helpers::{map_of, required};
use layerfs_home::names::StoreName;
use layerfs_home::outcome::refused;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// `project.init` in a worker: add the folder copy `staged` as project
/// `name` of Store `store`. `name` and `timeoutMs` are checked by the add,
/// after the copy is found and the lease taken.
#[derive(Clone, Debug)]
pub struct InitFrame {
    /// The Store.
    pub store: StoreName,
    /// The new project's name.
    pub name: Option<String>,
    /// The folder copy's id.
    pub staged: String,
    /// Init's time limit in milliseconds.
    pub timeout_ms: Option<i64>,
    /// The Store's record.
    pub store_record: Option<SdkValue>,
}

/// `project.export` in a worker: stream Branch `branch` of project
/// `project` of Store `store` back to the caller.
#[derive(Clone, Debug)]
pub struct ExportFrame {
    /// The Store.
    pub store: StoreName,
    /// The project's name in it.
    pub project: String,
    /// The Branch; `main` when absent.
    pub branch: Option<String>,
    /// The Commit of the Branch's history exported instead of its head: a
    /// full id or a prefix of 8 or more characters.
    pub at: Option<String>,
    /// The Store's record.
    pub store_record: Option<SdkValue>,
}

/// One step of a folder copy.
#[derive(Clone, Debug, PartialEq)]
pub enum StageStep {
    /// Applies one part of the copy.
    Write(Vec<u8>),
    /// Removes the staging folder.
    Discard,
}

/// `project.stage`: one step on copy `id` of project `name` of Store
/// `store` (the worker reads only the id and the step).
#[derive(Clone, Debug)]
pub struct StageFrame {
    /// The Store, as sent.
    pub store: String,
    /// The project, as sent.
    pub name: String,
    /// The copy's id: 16 lowercase hex digits.
    pub id: String,
    /// The step.
    pub step: StageStep,
}

/// The copy id a frame's `staged` holds.
fn staged_id(frame: &SdkValue) -> Result<String> {
    let id = frame.get_text("staged").unwrap_or("");
    let hex = |c: u8| c.is_ascii_digit() || (b'a'..=b'f').contains(&c);
    if id.len() != 16 || !id.bytes().all(hex) {
        return Err(refused(
            "InvalidInput",
            "`staged` must be a folder copy's id: 16 lowercase hex digits",
        ));
    }
    Ok(id.to_owned())
}

/// The LayerStack identity a `stack_id` field holds: its 17 bytes in hex.
fn stack_id(text: &str) -> Result<LayerStackId> {
    let bytes =
        unhex(text).ok_or_else(|| refused("InvalidInput", "`stack_id` must be hex digits"))?;
    LayerStackId::from_slice(&bytes).map_err(|_| {
        refused(
            "InvalidInput",
            "`stack_id` must be a LayerStack identity (17 bytes)",
        )
    })
}

/// The operation identity an `op_id` field holds: its 16 bytes in hex.
fn op_id(text: &str) -> Result<[u8; 16]> {
    let bytes = unhex(text).ok_or_else(|| refused("InvalidInput", "`op_id` must be hex digits"))?;
    bytes.as_slice().try_into().map_err(|_| {
        refused(
            "InvalidInput",
            "`op_id` must be an operation identity (16 bytes)",
        )
    })
}

impl InitFrame {
    /// The `project.init` frame: `{ store, name, staged, timeoutMs,
    /// store_record }`, each sent when set.
    pub fn encode(&self) -> SdkValue {
        let mut fields = vec![("store", SdkValue::text(self.store.as_str()))];
        fields.extend(self.name.as_deref().map(|n| ("name", SdkValue::text(n))));
        fields.push(("staged", SdkValue::text(&self.staged)));
        fields.extend(self.timeout_ms.map(|ms| ("timeoutMs", SdkValue::Int(ms))));
        fields.extend(self.store_record.clone().map(|r| ("store_record", r)));
        map_of(fields)
    }

    /// A `project.init` frame: no Store, a bad Store name, then a bad
    /// copy id are refused.
    pub fn decode(frame: &SdkValue) -> Result<Self> {
        let store = StoreName::parse(required(frame.get_text("store"), "store")?)?;
        Ok(Self {
            store,
            staged: staged_id(frame)?,
            name: frame.get_text("name").map(str::to_owned),
            timeout_ms: frame.get_int("timeoutMs"),
            store_record: frame.get("store_record").cloned(),
        })
    }
}

impl StageFrame {
    /// The `project.stage` frame: `{ store, name, staged, step, part? }`.
    pub fn encode(self) -> SdkValue {
        let step = match &self.step {
            StageStep::Write(_) => "write",
            StageStep::Discard => "discard",
        };
        let mut fields = vec![
            ("store", SdkValue::text(self.store)),
            ("name", SdkValue::text(self.name)),
            ("staged", SdkValue::text(self.id)),
            ("step", SdkValue::text(step)),
        ];
        if let StageStep::Write(part) = self.step {
            fields.push(("part", SdkValue::Bytes(part)));
        }
        map_of(fields)
    }

    /// A `project.stage` frame: a bad copy id, then a `write` without
    /// bytes or an unknown step are refused. The frame is taken so that
    /// its part is moved, not copied.
    pub fn decode(mut frame: SdkValue) -> Result<Self> {
        let id = staged_id(&frame)?;
        let step = match frame.get_text("step").unwrap_or("") {
            "write" => {
                let part = match &mut frame {
                    SdkValue::Map(fields) => fields.iter_mut().find(|(name, _)| name == "part"),
                    _ => None,
                };
                let Some((_, SdkValue::Bytes(part))) = part else {
                    return Err(refused("InvalidInput", "`part` must be bytes"));
                };
                StageStep::Write(std::mem::take(part))
            }
            "discard" => StageStep::Discard,
            step => {
                return Err(refused(
                    "InvalidInput",
                    format!("no folder copy step `{step}`"),
                ))
            }
        };
        let text = |name: &str| frame.get_text(name).unwrap_or("").to_owned();
        Ok(Self {
            store: text("store"),
            name: text("name"),
            id,
            step,
        })
    }
}

impl ExportFrame {
    /// The `project.export` frame: `{ store, project, branch?, at?,
    /// store_record? }`.
    pub fn encode(&self) -> SdkValue {
        let mut fields = vec![
            ("store", SdkValue::text(self.store.as_str())),
            ("project", SdkValue::text(&self.project)),
        ];
        fields.extend(
            self.branch
                .as_deref()
                .map(|b| ("branch", SdkValue::text(b))),
        );
        fields.extend(self.at.as_deref().map(|at| ("at", SdkValue::text(at))));
        fields.extend(self.store_record.clone().map(|r| ("store_record", r)));
        map_of(fields)
    }

    /// A `project.export` frame: no Store, no project, then a bad Store
    /// name are refused.
    pub fn decode(frame: &SdkValue) -> Result<Self> {
        let store = required(frame.get_text("store"), "store")?;
        let project = required(frame.get_text("project"), "project")?.to_owned();
        Ok(Self {
            store: StoreName::parse(store)?,
            project,
            branch: frame.get_text("branch").map(str::to_owned),
            at: frame.get_text("at").map(str::to_owned),
            store_record: frame.get("store_record").cloned(),
        })
    }
}

/// `project.delete`: retire Stack `stack` of Store `store` (here, or in its
/// worker, spec §4). The caller writes `deleted_ms` to its durable intent
/// before the call and resends it on a retry; `project` and `store_record`
/// are the caller context for the worker own entry and its Store open.
/// `op_id` is the admission the caller already took, so the marker reuses it
/// instead of scanning past a second row. `pending_retry` says the caller
/// retries an intent written before that admission, so the worker keeps its
/// own entry for the call that finishes the delete.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeleteFrame {
    /// The Store.
    pub store: StoreName,
    /// The project, as sent; the worker names its own entry with it.
    pub project: Option<String>,
    /// The LayerStack id the caller recorded (never a name).
    pub stack: LayerStackId,
    /// The `deleted_ms` the marker should hold; `None` lets the worker take
    /// the current time.
    pub deleted_ms: Option<i64>,
    /// The admission operation identity the marker reuses, when the caller
    /// took one.
    pub op_id: Option<[u8; 16]>,
    /// True when the caller retries a sidecar written before its admission
    /// (no marker confirmed yet): the worker keeps its own entry.
    pub pending_retry: Option<bool>,
    /// The Store record.
    pub store_record: Option<SdkValue>,
}

/// `project.delete.admit`: register the caller's project-scope admission
/// before it writes its sidecar and sends the marker (T4.6). The frame is
/// `{store, stack_id, op_id, project?, store_record?}`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeleteAdmitFrame {
    /// The Store.
    pub store: StoreName,
    /// The LayerStack the caller recorded.
    pub stack: LayerStackId,
    /// The admission operation identity.
    pub op: [u8; 16],
    /// The project's name, when the caller knows it: the registered intent
    /// carries it, so a later `Busy`/`Unresolved` names the project rather
    /// than the hex LayerStack id. An older caller omits it and the worker
    /// fills it from the Stack record.
    pub project: Option<String>,
    /// The Store record.
    pub store_record: Option<SdkValue>,
}

/// `project.delete.release`: delete the caller's admission row after the
/// marker, on every exit path (T4.6). The frame is
/// `{store, op_id, store_record?}`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeleteReleaseFrame {
    /// The Store.
    pub store: StoreName,
    /// The admission operation identity.
    pub op: [u8; 16],
    /// The Store record.
    pub store_record: Option<SdkValue>,
}

/// `project.delete` worker reply: `{kind: "Removed", project, stack_id,
/// deleted_ms, retired?}`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeleteReply {
    /// The project the worker own entry named (or the Store name).
    pub project: String,
    /// The LayerStack the marker names.
    pub stack: LayerStackId,
    /// The stored `deleted_ms` (a retry reuses it).
    pub deleted_ms: i64,
    /// Whether the marker already existed (`Retired`); a caller retrying a
    /// pending sidecar then finishes its entry. `None` from an older worker
    /// is treated as a freshly written marker.
    pub retired: Option<bool>,
}

impl DeleteFrame {
    /// The `project.delete` frame: `{ store, stack_id, deleted_ms?, project?,
    /// op_id?, store_record? }`, each optional field sent when set.
    pub fn encode(&self) -> SdkValue {
        let mut fields = vec![
            ("store", SdkValue::text(self.store.as_str())),
            ("stack_id", SdkValue::text(hex(&self.stack.to_bytes()))),
        ];
        fields.extend(self.deleted_ms.map(|ms| ("deleted_ms", SdkValue::Int(ms))));
        fields.extend(
            self.project
                .as_deref()
                .map(|p| ("project", SdkValue::text(p))),
        );
        fields.extend(self.op_id.map(|op| ("op_id", SdkValue::text(hex(&op)))));
        fields.extend(
            self.pending_retry
                .map(|retry| ("pending_retry", SdkValue::Bool(retry))),
        );
        fields.extend(self.store_record.clone().map(|r| ("store_record", r)));
        map_of(fields)
    }

    /// A `project.delete` frame: no Store, a bad Store name, no Stack id, a
    /// Stack id that is not a LayerStack identity, or an `op_id` that is not
    /// an operation identity are refused.
    pub fn decode(frame: &SdkValue) -> Result<Self> {
        let store = StoreName::parse(required(frame.get_text("store"), "store")?)?;
        let stack = stack_id(required(frame.get_text("stack_id"), "stack_id")?)?;
        Ok(Self {
            store,
            project: frame.get_text("project").map(str::to_owned),
            stack,
            deleted_ms: frame.get_int("deleted_ms"),
            op_id: frame.get_text("op_id").map(op_id).transpose()?,
            pending_retry: frame.get("pending_retry").and_then(|value| match value {
                SdkValue::Bool(retry) => Some(*retry),
                _ => None,
            }),
            store_record: frame.get("store_record").cloned(),
        })
    }
}

impl DeleteAdmitFrame {
    /// The `project.delete.admit` frame: `{store, stack_id, op_id, project?,
    /// store_record?}`, `project` sent when the caller knows it.
    pub fn encode(&self) -> SdkValue {
        let mut fields = vec![
            ("store", SdkValue::text(self.store.as_str())),
            ("stack_id", SdkValue::text(hex(&self.stack.to_bytes()))),
            ("op_id", SdkValue::text(hex(&self.op))),
        ];
        fields.extend(
            self.project
                .as_deref()
                .map(|project| ("project", SdkValue::text(project))),
        );
        fields.extend(self.store_record.clone().map(|r| ("store_record", r)));
        map_of(fields)
    }

    /// A `project.delete.admit` frame: its Store, Stack and operation
    /// identity are required and checked; an absent `project` is an unnamed
    /// caller.
    pub fn decode(frame: &SdkValue) -> Result<Self> {
        let store = StoreName::parse(required(frame.get_text("store"), "store")?)?;
        let stack = stack_id(required(frame.get_text("stack_id"), "stack_id")?)?;
        let op = op_id(required(frame.get_text("op_id"), "op_id")?)?;
        Ok(Self {
            store,
            stack,
            op,
            project: frame.get_text("project").map(str::to_owned),
            store_record: frame.get("store_record").cloned(),
        })
    }
}

impl DeleteReleaseFrame {
    /// The `project.delete.release` frame: `{store, op_id, store_record?}`.
    pub fn encode(&self) -> SdkValue {
        let mut fields = vec![
            ("store", SdkValue::text(self.store.as_str())),
            ("op_id", SdkValue::text(hex(&self.op))),
        ];
        fields.extend(self.store_record.clone().map(|r| ("store_record", r)));
        map_of(fields)
    }

    /// A `project.delete.release` frame: its Store and operation identity
    /// are required and checked.
    pub fn decode(frame: &SdkValue) -> Result<Self> {
        let store = StoreName::parse(required(frame.get_text("store"), "store")?)?;
        let op = op_id(required(frame.get_text("op_id"), "op_id")?)?;
        Ok(Self {
            store,
            op,
            store_record: frame.get("store_record").cloned(),
        })
    }
}

impl DeleteReply {
    /// The reply a worker sends.
    pub fn encode(&self) -> SdkValue {
        let mut fields = vec![
            ("kind", SdkValue::text("Removed")),
            ("project", SdkValue::text(&self.project)),
            ("stack_id", SdkValue::text(hex(&self.stack.to_bytes()))),
            ("deleted_ms", SdkValue::Int(self.deleted_ms)),
        ];
        fields.extend(
            self.retired
                .map(|retired| ("retired", SdkValue::Bool(retired))),
        );
        map_of(fields)
    }

    /// The reply fields; `kind` must be `Removed`.
    pub fn decode(frame: &SdkValue) -> Result<Self> {
        if frame.get_text("kind") != Some("Removed") {
            return Err(refused(
                "Failed",
                "the worker project.delete reply is not `Removed`",
            ));
        }
        let project = frame
            .get_text("project")
            .ok_or_else(|| refused("InvalidInput", "reply has no `project`"))?
            .to_owned();
        let stack = stack_id(required(frame.get_text("stack_id"), "stack_id")?)?;
        let deleted_ms = frame
            .get_int("deleted_ms")
            .ok_or_else(|| refused("InvalidInput", "reply has no `deleted_ms`"))?;
        Ok(Self {
            project,
            stack,
            deleted_ms,
            retired: frame.get("retired").and_then(|value| match value {
                SdkValue::Bool(retired) => Some(*retired),
                _ => None,
            }),
        })
    }
}

/// `project.delete.entry`: remove the worker's own home entry for a Stack
/// whose marker is already durable (a `marked` sidecar retry, T4.5b). The
/// frame is `{store, stack_id, project?}`; it opens no Store, so it carries
/// no Store record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeleteEntryFrame {
    /// The Store.
    pub store: StoreName,
    /// The LayerStack whose entry goes.
    pub stack: LayerStackId,
    /// The project's name, when the caller knows it.
    pub project: Option<String>,
}

/// `project.delete.entry` worker reply: `{kind: "Removed", project,
/// stack_id}`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeleteEntryReply {
    /// The project its entry named (or the Store name).
    pub project: String,
    /// The LayerStack the removal named.
    pub stack: LayerStackId,
}

impl DeleteEntryFrame {
    /// The frame: `{store, stack_id, project?}`, with `project` sent when the
    /// caller knows it.
    pub fn encode(&self) -> SdkValue {
        let mut fields = vec![
            ("store", SdkValue::text(self.store.as_str())),
            ("stack_id", SdkValue::text(hex(&self.stack.to_bytes()))),
        ];
        fields.extend(
            self.project
                .as_deref()
                .map(|project| ("project", SdkValue::text(project))),
        );
        map_of(fields)
    }

    /// A `project.delete.entry` frame: its Store and Stack are required and
    /// checked; an absent `project` is an unnamed caller.
    pub fn decode(frame: &SdkValue) -> Result<Self> {
        let store = StoreName::parse(required(frame.get_text("store"), "store")?)?;
        let stack = stack_id(required(frame.get_text("stack_id"), "stack_id")?)?;
        Ok(Self {
            store,
            stack,
            project: frame.get_text("project").map(str::to_owned),
        })
    }
}

impl DeleteEntryReply {
    /// The reply a worker sends.
    pub fn encode(&self) -> SdkValue {
        SdkValue::map([
            ("kind", SdkValue::text("Removed")),
            ("project", SdkValue::text(&self.project)),
            ("stack_id", SdkValue::text(hex(&self.stack.to_bytes()))),
        ])
    }

    /// The reply fields; `kind` must be `Removed`.
    pub fn decode(frame: &SdkValue) -> Result<Self> {
        if frame.get_text("kind") != Some("Removed") {
            return Err(refused(
                "Failed",
                "the worker project.delete entry reply is not `Removed`",
            ));
        }
        let project = frame
            .get_text("project")
            .ok_or_else(|| refused("InvalidInput", "reply has no `project`"))?
            .to_owned();
        let stack = stack_id(required(frame.get_text("stack_id"), "stack_id")?)?;
        Ok(Self { project, stack })
    }
}

/// `project.list` in a worker (T4.5c): Store `store`'s filtered project
/// listing, the same `layerfs_project::store::projects` a local or
/// PostgreSQL Store is read with, so a project retired in the Store is
/// hidden here too. A worker's root does not hold the caller's home
/// entries; the caller intersects this listing with them (`project.list`,
/// `project.get`).
#[derive(Clone, Debug)]
pub struct ListingFrame {
    /// The Store to list.
    pub store: StoreName,
    /// The Store's record: it opens the Store under the worker's root.
    pub store_record: Option<SdkValue>,
    /// Whether each project's `size` is read too (`project list --with
    /// stats`): one Layer record per project.
    pub stats: bool,
}

/// One project a worker's Store listing names.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListedProject {
    /// Its name in the Store.
    pub name: String,
    /// Its LayerStack.
    pub stack: LayerStackId,
    /// Main's head Layer, its full 66-character id.
    pub head: Option<String>,
    /// The length main's head Layer records, when it was asked for.
    pub size: Option<i64>,
}

/// `project.list` worker reply: `{ kind: "Listed", projects: [{ name,
/// stack_id, head, size? }] }`, in the Store's name order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListingReply {
    /// The Store's live projects.
    pub projects: Vec<ListedProject>,
}

impl ListingFrame {
    /// The `project.list` frame: `{ store, store_record?, stats? }`.
    pub fn encode(&self) -> SdkValue {
        let mut fields = vec![("store", SdkValue::text(self.store.as_str()))];
        fields.extend(self.store_record.clone().map(|r| ("store_record", r)));
        if self.stats {
            fields.push(("stats", SdkValue::Bool(true)));
        }
        map_of(fields)
    }

    /// A `project.list` frame: no Store, then a bad Store name are refused.
    pub fn decode(frame: &SdkValue) -> Result<Self> {
        Ok(Self {
            store: StoreName::parse(required(frame.get_text("store"), "store")?)?,
            store_record: frame.get("store_record").cloned(),
            stats: frame.get("stats") == Some(&SdkValue::Bool(true)),
        })
    }
}

impl ListingReply {
    /// The reply a worker sends.
    pub fn encode(&self) -> SdkValue {
        let projects = self
            .projects
            .iter()
            .map(|project| {
                map_of(vec![
                    ("name", SdkValue::text(&project.name)),
                    ("stack_id", SdkValue::text(hex(&project.stack.to_bytes()))),
                    ("head", SdkValue::opt_text(project.head.clone())),
                    ("size", project.size.map_or(SdkValue::Null, SdkValue::Int)),
                ])
            })
            .collect();
        map_of(vec![
            ("kind", SdkValue::text("Listed")),
            ("projects", SdkValue::List(projects)),
        ])
    }

    /// The reply fields; `kind` must be `Listed`, and every project names
    /// its Store name and its LayerStack identity.
    pub fn decode(frame: &SdkValue) -> Result<Self> {
        if frame.get_text("kind") != Some("Listed") {
            return Err(refused(
                "Failed",
                "the worker project.list reply is not `Listed`",
            ));
        }
        let Some(SdkValue::List(items)) = frame.get("projects") else {
            return Err(refused("InvalidInput", "reply has no `projects` list"));
        };
        let mut projects = Vec::with_capacity(items.len());
        for item in items {
            let name = item
                .get_text("name")
                .ok_or_else(|| refused("InvalidInput", "reply project has no `name`"))?
                .to_owned();
            let stack = stack_id(required(item.get_text("stack_id"), "stack_id")?)?;
            let head = item.get_text("head").map(str::to_owned);
            let size = item.get_int("size");
            projects.push(ListedProject {
                name,
                stack,
                head,
                size,
            });
        }
        Ok(Self { projects })
    }
}
