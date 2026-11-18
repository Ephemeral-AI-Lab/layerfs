//! The `workspace` calls' answers as the call surface 2.0 rows name them,
//! made from what the Workspace's side answered: each function returns the
//! row's full long map, which the call boundary shapes into the short or
//! long form. They read nothing.
use super::calls::Mounted;
use crate::workspace::serve::SHOWN;
use layerfs_bridge::control::SdkValue;
use layerfs_home::state::{Reason, State};

fn take(value: &SdkValue, name: &str) -> SdkValue {
    value.get(name).cloned().unwrap_or_default()
}

/// The paths of a list of `{ path, .. }` maps, the first [`SHOWN`].
fn paths(list: &SdkValue) -> (SdkValue, usize) {
    let SdkValue::List(items) = list else {
        return (SdkValue::List(Vec::new()), 0);
    };
    let shown = (items.iter().take(SHOWN))
        .map(|item| item.get("path").cloned().unwrap_or_default())
        .collect();
    (SdkValue::List(shown), items.len())
}

/// A count shown only past the [`SHOWN`] entries a list holds.
fn beyond(count: usize, cut: bool) -> SdkValue {
    match count > SHOWN || cut {
        true => SdkValue::Int(count as i64),
        false => SdkValue::Null,
    }
}

/// `workspace.mount`: `{ workspace, path, outcome, branch, commit,
/// base_layer, root, id, namespace }`.
pub fn mounted(mounted: &Mounted) -> SdkValue {
    let answer = &mounted.answer;
    SdkValue::map([
        ("workspace", SdkValue::text(&mounted.ws)),
        ("path", take(answer, "path")),
        ("outcome", SdkValue::text("mounted")),
        ("branch", take(answer, "branch")),
        // the Branch's head as `branch get` names it: its head Commit, else
        // the Layer it was forked at (a fresh fork has no Commit yet)
        (
            "commit",
            or_else(take(answer, "base"), || take(answer, "baseLayer")),
        ),
        ("base_layer", take(answer, "baseLayer")),
        ("root", take(answer, "root")),
        ("id", take(answer, "workspace")),
        ("namespace", take(answer, "namespace")),
    ])
}

/// `value`, or what `other` answers when it is null.
fn or_else(value: SdkValue, other: impl FnOnce() -> SdkValue) -> SdkValue {
    match value {
        SdkValue::Null => other(),
        value => value,
    }
}

/// `workspace.commit`: `{ commit, unchanged, merged, overwritten,
/// overwritten_count, outcome, branch, parent, root, message, created_at,
/// merged_from, displaced }`.
pub fn committed(answer: &SdkValue) -> SdkValue {
    let unchanged = answer.get_text("kind") == Some("UpToDate");
    let made = take(answer, "commit");
    let merged = take(answer, "merged");
    let (overwritten, count) = paths(&take(answer, "lww"));
    let cut = answer.get_bool("lwwTruncated");
    let created = made.get_int("time").map_or(SdkValue::Null, |ms| {
        SdkValue::Text(layerfs_home::time::iso(ms))
    });
    let root = match unchanged {
        true => take(answer, "root"),
        false => take(&made, "root"),
    };
    SdkValue::map([
        ("commit", take(answer, "head")),
        ("unchanged", SdkValue::Bool(unchanged)),
        ("merged", SdkValue::Bool(merged != SdkValue::Null)),
        ("overwritten", overwritten),
        ("overwritten_count", beyond(count, cut)),
        (
            "outcome",
            SdkValue::text(if unchanged { "unchanged" } else { "committed" }),
        ),
        ("branch", take(answer, "branch")),
        // the node this Commit follows: the Commit before it, else the
        // Layer the Branch was forked at (its first Commit has no parent
        // Commit); null when no Commit was made
        (
            "parent",
            or_else(take(&made, "parent"), || take(&made, "base")),
        ),
        ("root", root),
        ("message", take(&made, "message")),
        ("created_at", created),
        ("merged_from", take(&merged, "head")),
        ("displaced", take(answer, "displacedHead")),
    ])
}

/// `workspace.unmount`: `{ ok, outcome, abort, detach, cleanup }`, a forced
/// unmount's three steps lowercase.
pub fn unmounted(answer: &SdkValue) -> SdkValue {
    let forced = answer.get_text("kind") == Some("ForceUnmounted");
    let step = |name: &str| match answer.get_text(name) {
        Some(word) if forced => SdkValue::Text(snake(word)),
        _ => SdkValue::Null,
    };
    SdkValue::map([
        ("ok", SdkValue::Bool(true)),
        (
            "outcome",
            SdkValue::text(if forced {
                "force_unmounted"
            } else {
                "unmounted"
            }),
        ),
        ("abort", step("abort")),
        ("detach", step("detach")),
        ("cleanup", step("cleanup")),
    ])
}

/// `Idle` as `idle`, `LocalFailure` as `local_failure`.
fn snake(word: &str) -> String {
    let mut out = String::new();
    for (at, c) in word.chars().enumerate() {
        if c.is_ascii_uppercase() && at > 0 {
            out.push('_');
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}

/// A Workspace's state from its daemon's `activity` (as `sandbox.get`
/// reads it): `idle` is ready; attaching, committing and closing are busy
/// with that operation, in the `phase` the daemon names and since
/// `since_ms` when it says (a daemon of another build may not: its
/// activity's own phase word then, and no time); `uncertain` is broken
/// `unresolved` and `local_failure` broken `failed`, each fixed by a
/// forced unmount of handle `ws`.
pub fn state(activity: &str, phase: Option<&str>, since_ms: Option<i64>, ws: &str) -> State {
    let busy = |name: &str, first: &str| State::Busy {
        name: name.to_owned(),
        phase: phase.unwrap_or(first).to_owned(),
        started_ms: since_ms,
    };
    let fix = format!("layerfs workspace unmount {ws} --force");
    match activity {
        "attaching" => busy("mount", "attach"),
        "committing" => busy("commit", "publish"),
        "closing" => busy("unmount", "close"),
        "uncertain" => State::broken(Reason::Unresolved, Some(&fix)),
        "local_failure" => State::broken(Reason::Failed, Some(&fix)),
        _ => State::Ready,
    }
}

/// `workspace.status` of handle `ws`: `{ changes, changes_count, partial,
/// branch_moved, state, op, reason, fix, branch, commit, base_layer,
/// branch_head, activity, overlay, pending_merge }`. `changes` is the
/// first [`SHOWN`] entries of the daemon's one bounded read, each `{ path,
/// change, type }`; `changes_count` is the number read, given only past
/// those shown; `partial` says the read hit its bound.
pub fn status(answer: &SdkValue, ws: &str) -> SdkValue {
    let entries = match answer.get("changes") {
        Some(SdkValue::List(entries)) => entries.clone(),
        _ => Vec::new(),
    };
    let count = entries.len();
    let partial = answer.get_bool("partial");
    let activity = snake(answer.get_text("activity").unwrap_or("Idle"));
    let overlay = take(answer, "overlay");
    let pending = overlay.get_bool("pendingMerge");
    let mut fields: Vec<(String, SdkValue)> = vec![
        (
            "changes".to_owned(),
            SdkValue::List(entries.into_iter().take(SHOWN).collect()),
        ),
        ("changes_count".to_owned(), beyond(count, partial)),
        ("partial".to_owned(), SdkValue::Bool(partial)),
        (
            "branch_moved".to_owned(),
            SdkValue::Bool(answer.get_bool("headMovedSinceMount")),
        ),
    ];
    let since = answer.get_int("activitySinceMs");
    let state = state(&activity, answer.get_text("activityPhase"), since, ws);
    let companions = state.fields();
    for key in ["state", "op", "reason", "fix"] {
        let value = (companions.iter())
            .find(|(name, _)| *name == key)
            .map_or(SdkValue::Null, |(_, value)| value.clone());
        fields.push((key.to_owned(), value));
    }
    fields.extend([
        ("branch".to_owned(), take(answer, "branch")),
        ("commit".to_owned(), take(answer, "base")),
        ("base_layer".to_owned(), take(answer, "baseLayer")),
        ("branch_head".to_owned(), take(answer, "branchHead")),
        ("activity".to_owned(), SdkValue::Text(activity)),
        (
            "overlay".to_owned(),
            match overlay {
                SdkValue::Null => SdkValue::Null,
                overlay => SdkValue::map([
                    ("inode_count", take(&overlay, "inodes")),
                    ("entry_count", take(&overlay, "entries")),
                ]),
            },
        ),
        ("pending_merge".to_owned(), SdkValue::Bool(pending)),
    ]);
    SdkValue::Map(fields)
}
