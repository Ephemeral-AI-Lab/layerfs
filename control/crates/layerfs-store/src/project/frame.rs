//! The `project` worker frames (layerfs-project#109): `project.init`,
//! `project.remove` and `project.stage`, each encoded by the caller and
//! decoded once in the worker, in the order the worker always refused them.
//! The worker sets the home to its own root; a frame names none.
use layerfs_bridge::control::{SdkRefusal, SdkValue};
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
