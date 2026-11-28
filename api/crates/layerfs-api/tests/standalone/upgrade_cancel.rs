//! Cancelling `store.upgrade` (#804 §6, family H) through the call surface:
//! the process-wide cancel request (`layerfs_api::cancel`, which the
//! `layerfs` command's signal thread sets) asked as a phase starts, from the
//! progress event of that phase. In `check`, `convert` or `verify` the call
//! answers `Failed` with `detail.code: "Cancelled"` and the Store file is
//! unchanged byte for byte (and still upgrades afterwards); asked at
//! `commit` it is no longer read and the call answers `upgraded`. The CLI's
//! own signals are `layerfs-cli/tests/store_upgrade_signals.rs`.
//!
//! One test in its own binary: the request is one flag per process.
//!
//! INTEGRATION: needs the engine and control halves (#804 W1, W2); on a
//! build without them it says NOT_RUN and passes.
#[path = "../support/root.rs"]
mod root;
#[path = "../support/upgrade.rs"]
mod upgrade_support;
use std::path::Path;
use std::sync::atomic::Ordering;
use upgrade_support::*;

fn file(home: &Path) -> Vec<u8> {
    std::fs::read(home.join("store/fx/store.sqlite")).unwrap()
}

/// `store.upgrade fx` with the cancel request asked as `phase` starts.
fn upgrade_cancelled_at(
    home: &Path,
    phase: &str,
) -> (
    Result<layerfs_protocol::sdk::SdkValue, layerfs_protocol::sdk::SdkRefusal>,
    Vec<String>,
) {
    layerfs_api::cancel::flag().store(false, Ordering::SeqCst);
    let args = layerfs_protocol::sdk::SdkValue::map([("store", t("fx")), DETAIL]);
    let mut seen = Vec::new();
    let outcome = layerfs_api::call(
        Some(crate::root::root_of(home)),
        "store.upgrade",
        args,
        &mut |event| {
            let started = event
                .get("progress")
                .and_then(|progress| progress.get_text("phase"))
                .map(str::to_owned);
            if let Some(started) = started {
                if started == phase {
                    layerfs_api::cancel::request();
                }
                seen.push(started);
            }
        },
    );
    layerfs_api::cancel::flag().store(false, Ordering::SeqCst);
    (outcome, seen)
}

#[test]
fn a_cancel_before_commit_changes_nothing_and_at_commit_the_upgrade_completes() {
    for phase in ["check", "convert", "verify"] {
        let Some(home) = sqlite_fixture_home(&format!("cancel-{phase}")) else {
            return;
        };
        if !built(&home, "fx") {
            return;
        }
        let before = file(&home);
        let (outcome, seen) = upgrade_cancelled_at(&home, phase);
        let refused = outcome.expect_err(phase);
        assert_eq!(refused.code, "Failed", "{phase}: {refused:?}");
        assert_eq!(
            refused.detail.get_text("code"),
            Some("Cancelled"),
            "{phase}"
        );
        assert_eq!(refused.message, "cancelled before COMMIT; nothing changed");
        assert!(!seen.iter().any(|p| p == "commit"), "{phase}: {seen:?}");
        layerfs_api::close_idle(std::time::Duration::ZERO);
        assert!(file(&home) == before, "{phase}: the file changed");
        for sidecar in ["store.sqlite-wal", "store.sqlite-journal"] {
            let path = home.join("store/fx").join(sidecar);
            let len = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            assert_eq!(len, 0, "{phase}: {sidecar} left with {len} bytes");
        }
        // the op ended: the Store is outdated, not unresolved, and upgrades
        let got = ok(&home, "store.get", &[("store", t("fx")), with(&["format"])]);
        assert_eq!(got.get_text("state"), Some("ready"), "{phase}: {got:?}");
        assert_eq!(
            got.get("format").and_then(|f| f.get_text("status")),
            Some("outdated")
        );
        let (outcome, _) = upgrade_cancelled_at(&home, "no such phase");
        assert_eq!(outcome.unwrap().get_text("outcome"), Some("upgraded"));
        layerfs_api::close_idle(std::time::Duration::ZERO);
        let _ = std::fs::remove_dir_all(home.parent().unwrap());
    }
    // at commit the request is no longer read
    let Some(home) = sqlite_fixture_home("cancel-commit") else {
        return;
    };
    let (outcome, seen) = upgrade_cancelled_at(&home, "commit");
    let answer = outcome.expect("a cancel at commit is not read");
    assert_eq!(answer.get_text("outcome"), Some("upgraded"), "{answer:?}");
    assert_eq!(seen, ["check", "convert", "verify", "commit"]);
    let got = ok(&home, "store.get", &[("store", t("fx")), with(&["format"])]);
    assert_eq!(
        got.get("format").and_then(|f| f.get_text("status")),
        Some("current")
    );
    matches_manifest(&home, &manifest("sqlite/v2"), false);
    layerfs_api::close_idle(std::time::Duration::ZERO);
    let _ = std::fs::remove_dir_all(home.parent().unwrap());
}
