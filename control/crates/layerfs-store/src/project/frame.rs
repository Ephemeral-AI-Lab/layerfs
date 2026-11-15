//! The `project` worker frames (layerfs-project#109): `project.init`,
//! `project.remove`, `project.list` (T4.5c), `project.stage` and
//! `project.export`, each encoded by the caller and
//! decoded once in the worker, in the order the worker always refused them.
//! The worker sets the home to its own root; a frame names none.
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_bridge::sdk_values::{hex, unhex};
use layerfs_history::LayerStackId;
use layerfs_home::helpers::required;
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

/// `project.remove`: drop project `project` from Store `store` (here, or in
/// its worker).
#[derive(Clone, Debug)]
pub struct RemoveFrame {
    /// The Store.
    pub store: StoreName,
    /// The project's name in it.
    pub project: String,
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

/// A frame's fields, in order.
fn map(fields: Vec<(&str, SdkValue)>) -> SdkValue {
    SdkValue::Map(fields.into_iter().map(|(n, v)| (n.to_owned(), v)).collect())
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

impl InitFrame {
    /// The `project.init` frame: `{ store, name, staged, timeoutMs,
    /// store_record }`, each sent when set.
    pub fn encode(&self) -> SdkValue {
        let mut fields = vec![("store", SdkValue::text(self.store.as_str()))];
        fields.extend(self.name.as_deref().map(|n| ("name", SdkValue::text(n))));
        fields.push(("staged", SdkValue::text(&self.staged)));
        fields.extend(self.timeout_ms.map(|ms| ("timeoutMs", SdkValue::Int(ms))));
        fields.extend(self.store_record.clone().map(|r| ("store_record", r)));
        map(fields)
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

impl RemoveFrame {
    /// The `project.remove` frame: `{ store, project, store_record }`.
    pub fn encode(&self) -> SdkValue {
        let mut fields = vec![
            ("store", SdkValue::text(self.store.as_str())),
            ("project", SdkValue::text(&self.project)),
        ];
        fields.extend(self.store_record.clone().map(|r| ("store_record", r)));
        map(fields)
    }

    /// A `project.remove` frame: no Store, no project, then a bad Store
    /// name are refused.
    pub fn decode(frame: &SdkValue) -> Result<Self> {
        let store = required(frame.get_text("store"), "store")?;
        let project = required(frame.get_text("project"), "project")?.to_owned();
        Ok(Self {
            store: StoreName::parse(store)?,
            project,
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
        map(fields)
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
    /// The `project.export` frame: `{ store, project, branch?,
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
        fields.extend(self.store_record.clone().map(|r| ("store_record", r)));
        map(fields)
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
            store_record: frame.get("store_record").cloned(),
        })
    }
}

/// `project.delete`: retire Stack `stack` of Store `store` (here, or in its
/// worker, spec §4). The caller writes `deleted_ms` to its durable intent
/// before the call and resends it on a retry; `project` and `store_record`
/// are the caller context for the worker own entry and its Store open.
#[derive(Clone, Debug)]
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
    /// The Store record.
    pub store_record: Option<SdkValue>,
}

/// `project.delete` worker reply: `{kind: "Removed", project, stack_id,
/// deleted_ms}`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeleteReply {
    /// The project the worker own entry named (or the Store name).
    pub project: String,
    /// The LayerStack the marker names.
    pub stack: LayerStackId,
    /// The stored `deleted_ms` (a retry reuses it).
    pub deleted_ms: i64,
}

impl DeleteFrame {
    /// The `project.delete` frame: `{ store, stack_id, deleted_ms?, project?,
    /// store_record? }`, each optional field sent when set.
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
        fields.extend(self.store_record.clone().map(|r| ("store_record", r)));
        map(fields)
    }

    /// A `project.delete` frame: no Store, a bad Store name, no Stack id, or
    /// a Stack id that is not a LayerStack identity are refused.
    pub fn decode(frame: &SdkValue) -> Result<Self> {
        let store = StoreName::parse(required(frame.get_text("store"), "store")?)?;
        let stack = stack_id(required(frame.get_text("stack_id"), "stack_id")?)?;
        Ok(Self {
            store,
            project: frame.get_text("project").map(str::to_owned),
            stack,
            deleted_ms: frame.get_int("deleted_ms"),
            store_record: frame.get("store_record").cloned(),
        })
    }
}

impl DeleteReply {
    /// The reply a worker sends.
    pub fn encode(&self) -> SdkValue {
        map(vec![
            ("kind", SdkValue::text("Removed")),
            ("project", SdkValue::text(&self.project)),
            ("stack_id", SdkValue::text(hex(&self.stack.to_bytes()))),
            ("deleted_ms", SdkValue::Int(self.deleted_ms)),
        ])
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
        })
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
}

/// One project a worker's Store listing names.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListedProject {
    /// Its name in the Store.
    pub name: String,
    /// Its LayerStack.
    pub stack: LayerStackId,
}

/// `project.list` worker reply: `{ kind: "Listed", projects: [{ name,
/// stack_id }] }`, in the Store's name order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListingReply {
    /// The Store's live projects.
    pub projects: Vec<ListedProject>,
}

impl ListingFrame {
    /// The `project.list` frame: `{ store, store_record? }`.
    pub fn encode(&self) -> SdkValue {
        let mut fields = vec![("store", SdkValue::text(self.store.as_str()))];
        fields.extend(self.store_record.clone().map(|r| ("store_record", r)));
        map(fields)
    }

    /// A `project.list` frame: no Store, then a bad Store name are refused.
    pub fn decode(frame: &SdkValue) -> Result<Self> {
        Ok(Self {
            store: StoreName::parse(required(frame.get_text("store"), "store")?)?,
            store_record: frame.get("store_record").cloned(),
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
                map(vec![
                    ("name", SdkValue::text(&project.name)),
                    ("stack_id", SdkValue::text(hex(&project.stack.to_bytes()))),
                ])
            })
            .collect();
        map(vec![
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
            projects.push(ListedProject { name, stack });
        }
        Ok(Self { projects })
    }
}
