//! The mount intents a forced `sandbox.remove` ends once its runtime is
//! confirmed stopped (RC-B F1, RV5 FAIL-2). The daemon it stopped owned
//! them, so each intent its Workspaces held is ended once, as that
//! daemon's own unmount would have ended it: released, or marked uncertain
//! when the Store refuses the delete. One it cannot end is a sentence of
//! `left`. A close that ran without a PostgreSQL Store URL (its machine
//! cannot dial it) hands the intents with known ids back to its caller
//! ([`handed`]), which ends them with its own URL ([`end_handed`]).
//! Nothing else ends a mount intent: `store compact` and the other GC
//! calls settle only what the Store's admission proves unowned.
use crate::record::SandboxRecord;
use layerfs_protocol::provision::install::SdkValue;
use layerfs_protocol::sdk::{hex, unhex};
use layerfs_history::BranchId;
use layerfs_home::outcome::shell_word;
use layerfs_store::open_args::OpenArgs;
use layerfs_protocol::StoreKind;
use std::path::Path;

/// The Branch a mounted Workspace is bound to: its id, as the process that
/// mounted it holds it, or its name in the Sandbox's project, as its
/// Workspace record keeps it.
pub enum Branch {
    /// The Branch's id.
    Id(BranchId),
    /// The Branch's name in the Sandbox's project.
    Named(String),
}

/// One Workspace a forced close ends: its kernel path, and its Workspace
/// id and Branch when they are known.
pub struct Bound {
    /// The Workspace's kernel path.
    pub path: String,
    /// Its Workspace id and Branch; `None` when nothing records them.
    pub bound: Option<([u8; 32], Branch)>,
}

/// Ends, once each, the mount intents `bound` left in the Store `open`
/// names (or the reason `open` could not be made), now that their owner is
/// stopped. Answers a `left` sentence for each one it could not end.
pub fn end(open: Result<&OpenArgs, &str>, bound: &[Bound]) -> Vec<SdkValue> {
    (bound.iter())
        .filter_map(|workspace| Some(not_ended(&workspace.path, &end_one(open, workspace).err()?)))
        .collect()
}

/// The `left` sentence of Workspace `path`'s mount intent, not ended for
/// reason `why`.
fn not_ended(path: &str, why: &str) -> SdkValue {
    SdkValue::text(format!(
        "Workspace {path}'s mount intent was not ended ({why}): `layerfs branch remove` of its \
         Branch and `layerfs store compact` are refused while it remains"
    ))
}

/// [`end`]'s one Workspace: why its intent was not ended.
fn end_one(open: Result<&OpenArgs, &str>, workspace: &Bound) -> Result<(), String> {
    let Some((id, branch)) = &workspace.bound else {
        return Err("nothing records its Workspace id".to_owned());
    };
    let open = open.map_err(str::to_owned)?;
    let branch = match branch {
        Branch::Id(branch) => *branch,
        Branch::Named(name) => {
            let project = open.project.as_deref().unwrap_or_default();
            layerfs_store::service::branch_named(open, project, name)
                .map_err(|refusal| refusal.message)?
        }
    };
    layerfs_store::service::end_mount(open, branch, *id)
        .map(drop)
        .map_err(|refusal| refusal.message)
}

/// The field of a close's answer that hands its caller the mount intents
/// its side could not end for want of a PostgreSQL Store URL it can dial.
pub const HANDED: &str = "ending";

/// `bound`'s Workspaces bound by id, as a close's answer hands them to its
/// caller ([`HANDED`]): `{ path, workspace, branch }`, each id in hex.
/// Only a Workspace bound to a Branch id is handed: what records less is
/// a `left` sentence where the close runs.
pub fn handed(bound: &[Bound]) -> SdkValue {
    SdkValue::List(
        (bound.iter())
            .filter_map(|workspace| match &workspace.bound {
                Some((id, Branch::Id(branch))) => Some(SdkValue::map([
                    ("path", SdkValue::text(&workspace.path)),
                    ("workspace", SdkValue::text(hex(id))),
                    ("branch", SdkValue::text(hex(branch.as_slice()))),
                ])),
                _ => None,
            })
            .collect(),
    )
}

/// A close's `answer` for Sandbox `record` of home `home`, with each mount
/// intent its side handed back ([`HANDED`]) ended here, once, in the Store
/// opened as a Store call here opens it ([`store_of`]): each one not ended
/// is a sentence of its `left`, and the handed field goes. Any other answer
/// is returned as it is.
pub fn end_handed(home: &Path, record: &SandboxRecord, answer: SdkValue) -> SdkValue {
    let SdkValue::Map(fields) = answer else {
        return answer;
    };
    let (handed, mut fields): (Vec<_>, Vec<_>) =
        fields.into_iter().partition(|(name, _)| name == HANDED);
    let (bound, undecoded): (Vec<_>, Vec<_>) = match handed.into_iter().next() {
        Some((_, SdkValue::List(items))) => {
            (items.iter().map(taken)).partition(|workspace: &Bound| workspace.bound.is_some())
        }
        Some(_) | None => return SdkValue::Map(fields),
    };
    if bound.is_empty() && undecoded.is_empty() {
        return SdkValue::Map(fields);
    }
    let why = "the ids it was handed back do not decode";
    let mut more: Vec<_> = (undecoded.iter())
        .map(|workspace| not_ended(&workspace.path, why))
        .collect();
    if !bound.is_empty() {
        let open = store_of(home, record);
        more.extend(end(open.as_ref().map_err(String::as_str), &bound));
    }
    match fields.iter_mut().find(|(name, _)| name == "left") {
        Some((_, SdkValue::List(left))) => left.extend(more),
        _ => fields.push(("left".to_owned(), SdkValue::List(more))),
    }
    SdkValue::Map(fields)
}

/// One Workspace [`handed`] names; one whose ids do not decode is listed
/// with neither, and [`end_handed`] says they did not decode.
fn taken(item: &SdkValue) -> Bound {
    let id = |field: &str| item.get_text(field).and_then(unhex);
    let workspace = id("workspace").and_then(|bytes| <[u8; 32]>::try_from(bytes).ok());
    let branch = id("branch").and_then(|bytes| BranchId::from_slice(&bytes).ok());
    Bound {
        path: item.get_text("path").unwrap_or_default().to_owned(),
        bound: workspace.zip(branch.map(Branch::Id)),
    }
}

/// The Store of Sandbox `record`'s project in home `home`, opened as a
/// Store call there opens it.
pub fn store_of(home: &Path, record: &SandboxRecord) -> Result<OpenArgs, String> {
    let project = record.project().unwrap_or_default();
    let target = layerfs_store::resolve::target(home, project).map_err(|r| r.message)?;
    let mut open = OpenArgs::of_record(home, &target.store, &target.record)
        .map_err(|refusal| refusal.message)?;
    open.project = Some(target.project);
    Ok(open)
}

/// What ends a mount intent a forced remove of Sandbox `record` could not
/// end: only the Store's own admission, once it can prove the intent's
/// owner gone (another boot of the kernel that ran it, for a sqlite Store),
/// and never for an intent the Store marked uncertain.
pub fn remedy(home: &Path, record: &SandboxRecord) -> String {
    let target = layerfs_store::resolve::target(home, record.project().unwrap_or_default());
    let (store, kind) = match &target {
        Ok(target) => (shell_word(&target.store), target.store_ref.kind()),
        Err(_) => ("<store>".to_owned(), StoreKind::Sqlite),
    };
    let adopts =
        format!("`layerfs store compact {store}` adopts it unless the Store marked it uncertain");
    let machine = record.machine();
    // a built-in machine with no record yet has the platform of its name
    let platform = layerfs_machine::record::MachineRecord::read(home, machine)
        .ok()
        .flatten()
        .and_then(|found| found.platform_name().map(str::to_owned))
        .or_else(|| layerfs_home::names::built_in_platform(machine).map(str::to_owned));
    // a platform no provider holds runs Sandboxes on this computer's own kernel
    let own_os = platform
        .as_deref()
        .is_some_and(|p| !layerfs_machine::provided(p));
    match (kind, own_os, platform.as_deref()) {
        (StoreKind::Postgresql, ..) => {
            "its PostgreSQL Store is shared, so nothing can prove its owner gone and it stays"
                .to_owned()
        }
        (_, true, _) if cfg!(target_os = "macos") => {
            "nothing on this computer can prove its owner gone, so it stays".to_owned()
        }
        (_, true, _) => format!("once this computer restarts, {adopts}"),
        (_, _, Some("docker")) => {
            format!("once the kernel Docker runs on restarts, {adopts}")
        }
        _ => {
            let machine = shell_word(machine);
            format!(
                "`layerfs machine stop {machine} && layerfs machine start {machine}` restarts its \
                 kernel, after which {adopts}"
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn branch(n: u8) -> BranchId {
        BranchId::from_authority([n; 16])
    }

    fn workspaces() -> Vec<Bound> {
        vec![
            Bound {
                path: "/w/a".to_owned(),
                bound: Some(([1; 32], Branch::Id(branch(7)))),
            },
            Bound {
                path: "/w/named".to_owned(),
                bound: Some(([3; 32], Branch::Named("main".to_owned()))),
            },
            Bound {
                path: "/w/b".to_owned(),
                bound: Some(([2; 32], Branch::Id(branch(8)))),
            },
            Bound {
                path: "/w/none".to_owned(),
                bound: None,
            },
        ]
    }

    /// What a close hands back ([`handed`]) is what its caller reads
    /// ([`taken`]): each Workspace bound by Branch id, in order, with the
    /// same path and ids; one bound by name or by nothing is not handed.
    #[test]
    fn handed_intents_decode_as_their_caller_reads_them() {
        let SdkValue::List(items) = handed(&workspaces()) else {
            panic!("handed is a list");
        };
        assert_eq!(items.len(), 2, "{items:?}");
        let expected = [("/w/a", [1; 32], branch(7)), ("/w/b", [2; 32], branch(8))];
        for (item, (path, workspace, id)) in items.iter().zip(expected) {
            let read = taken(item);
            assert_eq!(read.path, path);
            match read.bound {
                Some((read_workspace, Branch::Id(read_branch))) => {
                    assert_eq!(read_workspace, workspace, "{path}");
                    assert!(read_branch == id, "{path}");
                }
                _ => panic!("{path} reads with no ids: {item:?}"),
            }
        }
    }

    /// The caller of a close ends what it handed back and drops the field:
    /// each one not ended is a sentence of `left`, after what was there; an
    /// item whose ids do not decode says so (N2 of the 3cf2cfc246 review),
    /// not that nothing records its Workspace id.
    #[test]
    fn an_item_whose_ids_do_not_decode_says_so() {
        let home = std::env::temp_dir().join(format!("layerfs-handed-{}", std::process::id()));
        let record = SandboxRecord::from_record(
            "sb",
            SdkValue::map([("project", SdkValue::text("shop/api"))]),
        );
        let SdkValue::List(mut items) = handed(&workspaces()) else {
            panic!("handed is a list");
        };
        items.push(SdkValue::map([
            ("path", SdkValue::text("/w/bad")),
            ("workspace", SdkValue::text("not hex")),
            ("branch", SdkValue::text(hex(branch(9).as_slice()))),
        ]));
        let answer = SdkValue::map([
            ("kind", SdkValue::text("Closed")),
            ("left", SdkValue::List(vec![SdkValue::text("earlier")])),
            (HANDED, SdkValue::List(items)),
        ]);
        let answer = end_handed(&home, &record, answer);
        assert!(answer.get(HANDED).is_none(), "{answer:?}");
        let Some(SdkValue::List(left)) = answer.get("left") else {
            panic!("no left: {answer:?}");
        };
        let left: Vec<&str> = (left.iter())
            .filter_map(|line| match line {
                SdkValue::Text(line) => Some(line.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(left.len(), 4, "{left:?}");
        assert_eq!(left[0], "earlier");
        let of = |path: &str| {
            let found = left
                .iter()
                .find(|line| line.contains(&format!("{path}'s ")));
            found
                .copied()
                .unwrap_or_else(|| panic!("no {path}: {left:?}"))
        };
        // decoded: not ended for want of the Store (this home has none)
        for path in ["/w/a", "/w/b"] {
            let line = of(path);
            assert!(line.contains("no Store named shop"), "{line}");
        }
        let line = of("/w/bad");
        assert!(
            line.contains("ids it was handed back do not decode"),
            "{line}"
        );
        assert!(!line.contains("nothing records"), "{line}");
    }
}
