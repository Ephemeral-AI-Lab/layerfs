//! The round-4 #43 refusals: `project.init`'s lost-admission `Unknown` and
//! the shared `Unknown` detail fill (#43 Round 4, F43R4-1/F43R4-2).
//!
//! Product source holds no tests (the product-boundary guard), so
//! `project::add::add_failure` (already public) and
//! `unknown::named_unknown` (`#[doc(hidden)] pub`) are exercised here.
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_history::records::{GcIntentKind, GcScopeKind};
use layerfs_history::{HistoryError, OpConflict, UnknownOutcomeState};
use layerfs_project::store::InitError;
use layerfs_project::ProjectError;
use layerfs_store::project::add::add_failure;
use layerfs_store::unknown::named_unknown;

fn unknown_state() -> UnknownOutcomeState {
    UnknownOutcomeState::new(
        GcScopeKind::Project,
        "shop",
        Some("init".to_owned()),
        Some("00ff".repeat(8)),
    )
}

fn assert_unknown_names_shop(refusal: &SdkRefusal) {
    assert_eq!(refusal.code, "Unknown", "{refusal:?}");
    let entity = refusal.detail.get("entity").expect("entity");
    assert_eq!(entity.get_text("kind"), Some("project"), "{refusal:?}");
    assert_eq!(entity.get_text("name"), Some("shop"), "{refusal:?}");
    assert_eq!(
        refusal.detail.get_text("phase"),
        Some("init"),
        "{refusal:?}"
    );
    let expected = "00ff".repeat(8);
    assert_eq!(refusal.detail.get_text("op"), Some(expected.as_str()));
}

/// F43R4-2: Init's registration/release `UnknownOutcome` reaches the client
/// through the standard history refusal: `Unknown` naming the project, the
/// Init phase and the durable operation, not `Failed` with a null detail.
#[test]
fn a_lost_init_admission_is_unknown_naming_the_project_phase_and_op() {
    let lost = HistoryError::UnknownOutcome.with_unknown(unknown_state());
    for error in [
        InitError::Project(ProjectError::History(lost.clone())),
        InitError::History(lost.clone()),
    ] {
        let refusal = add_failure(error, None, "shop", "store");
        assert_unknown_names_shop(&refusal);
    }
}

/// The same arms keep a definite Init-intent refusal: a live Store-wide
/// operation (`BusyOp`) is `Busy` with the operation detail, never `Failed`.
#[test]
fn a_busy_init_intent_keeps_its_code_and_operation_detail() {
    let conflict = OpConflict::new(
        GcIntentKind::Compaction,
        GcScopeKind::Store,
        "store",
        Some("compact".to_owned()),
        "aa".repeat(8),
        false,
    );
    let error = InitError::History(HistoryError::BusyOp(Box::new(conflict)));
    let refusal = add_failure(error, None, "shop", "store");
    assert_eq!(refusal.code, "Busy", "{refusal:?}");
    let entity = refusal.detail.get("entity").expect("entity");
    assert_eq!(entity.get_text("kind"), Some("store"));
    assert_eq!(entity.get_text("name"), Some("store"));
}

fn refused(code: &str, detail: SdkValue) -> SdkRefusal {
    SdkRefusal {
        code: code.to_owned(),
        message: "the worker did not answer".to_owned(),
        detail,
    }
}

fn named_store(refusal: &SdkRefusal) {
    let entity = refusal.detail.get("entity").expect("entity");
    assert_eq!(entity.get_text("kind"), Some("store"), "{refusal:?}");
    assert_eq!(entity.get_text("name"), Some("shop"), "{refusal:?}");
    assert!(
        refusal.detail.get("phase").expect("phase") == &SdkValue::Null,
        "{refusal:?}"
    );
}

#[test]
fn a_null_detail_is_filled_with_the_entity() {
    let refusal = named_unknown(refused("Unknown", SdkValue::Null), "store", "shop", None);
    assert_eq!(refusal.code, "Unknown");
    named_store(&refusal);
    assert!(refusal.detail.get("op").expect("op") == &SdkValue::Null);
}

/// F43R4-1: a bare `UnknownOutcome` renders `SdkValue::map::<0>([])`, not
/// `Null`; the caller must fill that shape too.
#[test]
fn an_empty_map_detail_is_filled_with_the_entity_and_operation() {
    let op = [0xab; 16];
    let refusal = named_unknown(
        refused("Unknown", SdkValue::map::<0>([])),
        "store",
        "shop",
        Some(&op),
    );
    assert_eq!(refusal.code, "Unknown");
    named_store(&refusal);
    let expected = "ab".repeat(16);
    assert_eq!(refusal.detail.get_text("op"), Some(expected.as_str()));
}

#[test]
fn a_detail_that_already_names_something_is_kept() {
    let detail = SdkValue::map([(
        "entity",
        SdkValue::map([
            ("kind", SdkValue::text("store")),
            ("name", SdkValue::text("other")),
        ]),
    )]);
    let refusal = named_unknown(refused("Unknown", detail.clone()), "store", "shop", None);
    assert_eq!(refusal.detail, detail);
}

#[test]
fn a_definite_refusal_is_kept() {
    let refusal = named_unknown(
        refused("Busy", SdkValue::map::<0>([])),
        "store",
        "shop",
        None,
    );
    assert_eq!(refusal.code, "Busy");
    assert!(matches!(refusal.detail, SdkValue::Map(fields) if fields.is_empty()));
}
